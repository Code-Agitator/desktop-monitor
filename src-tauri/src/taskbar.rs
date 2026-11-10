//! 任务栏指标条。
//!
//! Win11 的任务栏是 XAML 实现的，不支持老的 deskband，也没有公开的「往任务栏塞控件」接口。
//! 这里的做法是：开一个透明、点击穿透的小窗，再用 `SetParent` **把它挂到 `Shell_TrayWnd`
//! 下面当子窗口**——这样它真的属于任务栏：跟着任务栏移动、跟着自动隐藏、z 序也在任务栏里，
//! 不是在任务栏上面盖一层。

use crate::config::TaskbarConfig;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClientRect, GetParent, GetWindowLongPtrW, GetWindowRect,
    IsWindow, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE,
    HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
    WS_CHILD, WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

const BASE_LABEL: &str = "taskbar";
/// 当前窗口的 label：explorer 重启会把子窗口连坐销毁，重建时必须换新 label
static CURRENT_LABEL: Mutex<String> = Mutex::new(String::new());

pub fn setup(app: &AppHandle, shared: Arc<Mutex<TaskbarConfig>>) -> tauri::Result<()> {
    let cfg = shared.lock().unwrap().clone();
    let win = create_window(app, &cfg, BASE_LABEL)?;
    *CURRENT_LABEL.lock().unwrap() = BASE_LABEL.to_string();
    // 在第一帧之前就挂到任务栏下，避免先闪一下再跳进去
    attach(&win, cfg.width);

    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(500));
            let cfg = shared.lock().unwrap().clone();
            if !cfg.enabled {
                let label = CURRENT_LABEL.lock().unwrap().clone();
                if let Some(win) = app.get_webview_window(&label) {
                    hide(&win);
                }
                continue;
            }

            let Some(tray) = find_tray() else {
                continue;
            };

            // explorer 重启会把子窗口连坐销毁：换一个新 label 重建再挂上去
            let label = CURRENT_LABEL.lock().unwrap().clone();
            let alive = app
                .get_webview_window(&label)
                .and_then(|w| w.hwnd().ok())
                .map(|h| unsafe { IsWindow(Some(h)).as_bool() })
                .unwrap_or(false);
            if !alive {
                let next = format!("{}-{}", BASE_LABEL, std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0));
                match create_window(&app, &cfg, &next) {
                    Ok(win) => {
                        *CURRENT_LABEL.lock().unwrap() = next.clone();
                        attach(&win, cfg.width);
                        eprintln!("[taskbar] 任务栏重建，窗口重挂为 {next}");
                    }
                    Err(e) => eprintln!("[taskbar] 重建窗口失败：{e}"),
                }
                continue;
            }

            let Some(win) = app.get_webview_window(&label) else {
                continue;
            };
            // 父窗口被换掉（explorer 重建了任务栏）时重新挂
            let parent = win.hwnd().ok().and_then(|h| unsafe { GetParent(h).ok() });
            if parent != Some(tray) {
                attach(&win, cfg.width);
                continue;
            }
            place(&win, tray, cfg.width);
            show(&win);
        }
    });
    Ok(())
}

fn create_window(
    app: &AppHandle,
    cfg: &TaskbarConfig,
    label: &str,
) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App("taskbar.html".into()))
        .title("desktop-monitor taskbar")
        .inner_size(cfg.width, 32.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .focused(false)
        .visible(false)
        .build()?;

    if let Ok(h) = win.hwnd() {
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            let add = (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize;
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | add);
        }
    }
    // 这行字只是「画」在任务栏上：点击、右键都还给任务栏自己
    let _ = win.set_ignore_cursor_events(true);
    Ok(win)
}

fn find_tray() -> Option<HWND> {
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None).ok() }
}

/// 把窗口改成任务栏的子窗口，并摆到通知区域左侧的空白处
fn attach(win: &WebviewWindow, width: f64) {
    let Ok(h) = win.hwnd() else { return };
    let Some(tray) = find_tray() else { return };
    unsafe {
        // 子窗口样式：去掉 WS_POPUP、加上 WS_CHILD / WS_CLIPSIBLINGS
        let style = GetWindowLongPtrW(h, GWL_STYLE) as u32;
        let child = (style & !WS_POPUP.0) | WS_CHILD.0 | WS_CLIPSIBLINGS.0;
        SetWindowLongPtrW(h, GWL_STYLE, child as isize);
        let _ = SetParent(h, Some(tray));
        let _ = SetWindowPos(
            h,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        );
    }
    place(win, tray, width);
}

/// 在任务栏客户区里定位：右对齐到通知区域（时钟）左侧 10px，垂直居中
fn place(win: &WebviewWindow, tray: HWND, width: f64) {
    let Ok(h) = win.hwnd() else { return };
    let sf = win.scale_factor().unwrap_or(1.0);
    unsafe {
        let mut client = RECT::default();
        if GetClientRect(tray, &mut client).is_err() {
            return;
        }
        let mut tb = RECT::default();
        if GetWindowRect(tray, &mut tb).is_err() {
            return;
        }
        let (cw, ch) = (client.right - client.left, client.bottom - client.top);
        if cw < 300 || ch < 12 {
            hide(win);
            return;
        }

        // 通知区域左边界（换到任务栏客户区坐标）
        let mut right = cw - (16.0 * sf) as i32;
        if let Ok(notify) = FindWindowExW(Some(tray), None, w!("TrayNotifyWnd"), None) {
            let mut nr = RECT::default();
            if GetWindowRect(notify, &mut nr).is_ok() && nr.right > nr.left {
                right = nr.left - tb.left - (10.0 * sf) as i32;
            }
        }

        let w = (width * sf).round() as i32;
        let hgt = (ch as f64 - 8.0 * sf).round() as i32;
        let x = (right - w).max((8.0 * sf) as i32);
        let y = ((ch - hgt) / 2).max(0);
        let _ = SetWindowPos(
            h,
            Some(HWND_TOP),
            x,
            y,
            w,
            hgt,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

fn show(win: &WebviewWindow) {
    if let Ok(h) = win.hwnd() {
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
    }
}

fn hide(win: &WebviewWindow) {
    if let Ok(h) = win.hwnd() {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

/// 托盘菜单：切换任务栏指标条
pub fn toggle(app: &AppHandle) {
    let state = app.state::<crate::app::AppState>();
    let mut cfg = state.taskbar.lock().unwrap().clone();
    cfg.enabled = !cfg.enabled;
    let enabled = cfg.enabled;
    *state.taskbar.lock().unwrap() = cfg.clone();
    if let Err(e) = crate::config::save_taskbar(app, &cfg) {
        eprintln!("[taskbar] 保存开关失败：{e}");
    }
    eprintln!("[taskbar] enabled={enabled}");
}
