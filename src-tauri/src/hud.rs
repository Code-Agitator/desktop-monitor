//! HUD：置顶透明小窗。记住位置；前台出现全屏应用时自动隐藏。
//!
//! 关闭 HUD 时**销毁窗口**（而不是 SW_HIDE）：隐藏的 webview 仍然占着一个 renderer 进程
//! （实测 ~85–95 MB 工作集），销毁掉才能把内存还给系统；再次开启时重建（约 0.3s）。

use crate::config::{self, PanelConfig, Surface, WindowConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindowVisible,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

const LABEL: &str = "hud";

/// 拖动后置脏：由监管线程合并落盘，避免拖动过程中每帧写文件
static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn setup(app: &AppHandle, shared: Arc<Mutex<PanelConfig>>) -> tauri::Result<()> {
    let cfg = shared.lock().unwrap().clone();
    let win_cfg = cfg.window.clone().unwrap_or_else(config::default_window);

    if win_cfg.enabled {
        build_window(app, &win_cfg, &shared)?;
    }

    // 监管线程：应用几何/启用状态、检测全屏、落盘拖动后的位置
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last_show: Option<bool> = None;
        let mut last_geometry: Option<(f64, f64, f64, f64)> = None;
        loop {
            std::thread::sleep(Duration::from_millis(350));
            let cfg = shared.lock().unwrap().clone();
            let Some(wc) = cfg.window.clone() else { continue };

            // 关掉 → 销毁窗口（隐藏的 webview 依然占着一个 renderer 进程）
            if !wc.enabled {
                if let Some(win) = app.get_webview_window(LABEL) {
                    let _ = win.destroy();
                    eprintln!("[hud] 已销毁窗口（释放 renderer）");
                    last_show = None;
                    last_geometry = None;
                }
                flush_dirty(&shared, &app);
                continue;
            }

            // 开着但没有窗口 → 回主线程重建
            if app.get_webview_window(LABEL).is_none() {
                let app2 = app.clone();
                let wc2 = wc.clone();
                let shared2 = shared.clone();
                let _ = app.run_on_main_thread(move || {
                    if app2.get_webview_window(LABEL).is_none() {
                        match build_window(&app2, &wc2, &shared2) {
                            Ok(_) => eprintln!("[hud] 已重建窗口"),
                            Err(e) => eprintln!("[hud] 重建窗口失败：{e}"),
                        }
                    }
                });
                last_show = None;
                last_geometry = None;
                continue;
            }

            let Some(win) = app.get_webview_window(LABEL) else { continue };

            let geometry = (wc.x, wc.y, wc.w, wc.h);
            if last_geometry != Some(geometry) {
                apply_geometry(&win, &wc);
                last_geometry = Some(geometry);
            }

            let fullscreen = foreground_fullscreen(&app);
            let should_show = wc.enabled && !fullscreen;
            if last_show != Some(should_show) {
                eprintln!(
                    "[hud] fullscreen={fullscreen} enabled={} show={should_show}",
                    wc.enabled
                );
                if should_show {
                    show(&win);
                } else {
                    hide(&win);
                }
                last_show = Some(should_show);
            }

            flush_dirty(&shared, &app);
        }
    });

    Ok(())
}

fn flush_dirty(shared: &Arc<Mutex<PanelConfig>>, app: &AppHandle) {
    if !DIRTY.swap(false, Ordering::SeqCst) {
        return;
    }
    let latest = shared.lock().unwrap().clone();
    if let Err(e) = config::save(app, Surface::Hud, &latest) {
        eprintln!("[hud] 保存位置失败：{e}");
    }
}

/// 建 HUD 窗口（含「拖动写回位置」的钩子）
fn build_window(
    app: &AppHandle,
    win_cfg: &WindowConfig,
    shared: &Arc<Mutex<PanelConfig>>,
) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("hud.html".into()))
        .title("desktop-monitor HUD")
        .additional_browser_args(&crate::prefs::browser_args())
        // 下限放宽到 40×24：HUD 单元格可以做到 32 高，窗口跟着内容自适应时不能被这里卡住
        .inner_size(win_cfg.w.max(40.0), win_cfg.h.max(24.0))
        .position(win_cfg.x, win_cfg.y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
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

    if win_cfg.enabled {
        show(&win);
    }

    {
        let shared = shared.clone();
        let app = app.clone();
        win.on_window_event(move |event| {
            if let WindowEvent::Moved(pos) = event {
                let sf = app
                    .get_webview_window(LABEL)
                    .and_then(|w| w.scale_factor().ok())
                    .unwrap_or(1.0);
                let lp = pos.to_logical::<f64>(sf);
                let mut cfg = shared.lock().unwrap();
                let wc = cfg.window.get_or_insert_with(config::default_window);
                // 只有用户真正按住左键拖动时才写回位置；程序化移动不参与，避免回环
                let dragging =
                    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 };
                if dragging && ((wc.x - lp.x).abs() > 1.0 || (wc.y - lp.y).abs() > 1.0) {
                    wc.x = lp.x;
                    wc.y = lp.y;
                    drop(cfg);
                    DIRTY.store(true, Ordering::SeqCst);
                }
            }
        });
    }

    Ok(win)
}

fn apply_geometry(win: &WebviewWindow, wc: &WindowConfig) {
    let sf = win.scale_factor().unwrap_or(1.0);
    if let Ok(size) = win.inner_size() {
        let ls = size.to_logical::<f64>(sf);
        if (ls.width - wc.w).abs() > 1.0 || (ls.height - wc.h).abs() > 1.0 {
            let _ = win.set_size(tauri::LogicalSize::new(wc.w, wc.h));
        }
    }
    if let Ok(pos) = win.outer_position() {
        let lp = pos.to_logical::<f64>(sf);
        if (lp.x - wc.x).abs() > 1.0 || (lp.y - wc.y).abs() > 1.0 {
            let _ = win.set_position(tauri::LogicalPosition::new(wc.x, wc.y));
        }
    }
}

fn show(win: &WebviewWindow) {
    if let Ok(h) = win.hwnd() {
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                h,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
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

/// 前台窗口是否铺满其所在显示器（排除桌面、任务栏与我们自己的窗口）。
pub fn foreground_fullscreen(app: &AppHandle) -> bool {
    let mut ours: Vec<isize> = Vec::new();
    for (label, w) in app.webview_windows() {
        if label == "panel" || label == "settings" || label == LABEL || label.starts_with("taskbar") {
            if let Ok(h) = w.hwnd() {
                ours.push(h.0 as isize);
            }
        }
    }

    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || ours.contains(&(fg.0 as isize)) {
            return false;
        }

        let mut buf = [0u16; 64];
        let n = GetClassNameW(fg, &mut buf);
        let class = if n > 0 {
            String::from_utf16_lossy(&buf[..n as usize])
        } else {
            String::new()
        };
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Windows.UI.Core.CoreWindow"
        ) {
            return false;
        }
        if IsIconic(fg).as_bool() || !IsWindowVisible(fg).as_bool() {
            return false;
        }

        let mut rect = RECT::default();
        if GetWindowRect(fg, &mut rect).is_err() {
            return false;
        }

        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }

        let m = mi.rcMonitor;
        let tol = 2;
        rect.left <= m.left + tol
            && rect.top <= m.top + tol
            && rect.right >= m.right - tol
            && rect.bottom >= m.bottom - tol
    }
}

/// 托盘菜单：持久切换 HUD 启用状态（与设置窗口里的开关一致）。
pub fn toggle(app: &AppHandle) {
    let state = app.state::<crate::app::AppState>();
    let mut cfg = state.hud.lock().unwrap().clone();
    let wc = cfg.window.get_or_insert_with(config::default_window);
    wc.enabled = !wc.enabled;
    let enabled = wc.enabled;
    *state.hud.lock().unwrap() = cfg.clone();
    if let Err(e) = config::save(app, Surface::Hud, &cfg) {
        eprintln!("[hud] 保存启用状态失败：{e}");
    }
    eprintln!("[hud] enabled={enabled}");
}
