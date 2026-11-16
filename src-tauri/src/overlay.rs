//! 面板承载：置底 + 点击穿透 + 不抢焦点的顶层透明窗口。
//! 不使用 SetParent，Explorer 重启不会销毁窗口，只需保持置底与可见。
//!
//! 另外提供「桌面编辑模式」：面板变成可交互、置顶，可以直接在桌面上拖卡片。
//! 命中测试用窗口子类化（WM_NCHITTEST 返回 HTTRANSPARENT），这样只有卡片区域接收鼠标，
//! 空桌面照常能用；而且不用反复改 WS_EX_TRANSPARENT（改窗口样式会强制重绘，屏幕会闪）。

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetSystemMetrics, GetTopWindow, GetWindow, GetWindowLongPtrW, IsIconic,
    IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GW_HWNDNEXT,
    HWND_BOTTOM, HWND_TOPMOST, HWND_TOP, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, SIZE_MINIMIZED, WM_NCHITTEST, WM_SHOWWINDOW,
    WM_SIZE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

const HTTRANSPARENT: i32 = -1;

/// 桌面编辑模式：面板不再点击穿透、暂时置顶，可以直接在桌面上拖卡片
static EDITING: AtomicBool = AtomicBool::new(false);
/// 拖动过程中即使光标离开卡片也要保持可交互
static DRAG_HOLD: AtomicBool = AtomicBool::new(false);
/// 面板是否被用户手动隐藏（托盘/HUD 菜单的「显示/隐藏面板」）
static HIDDEN: AtomicBool = AtomicBool::new(false);
/// 编辑模式下的卡片矩形（CSS 像素）与缩放，供 WM_NCHITTEST 判断
static HIT_CELLS: Mutex<Vec<(f64, f64, f64, f64)>> = Mutex::new(Vec::new());
static HIT_SCALE: Mutex<f64> = Mutex::new(1.0);

pub fn is_editing() -> bool {
    EDITING.load(Ordering::Relaxed)
}

/// 托盘 / HUD 菜单里的「显示/隐藏面板」：隐藏时**销毁 webview**，把 renderer 进程还给系统；
/// 再次显示时重建窗口并重新挂上子类/样式（约 0.3s 重载）。
pub fn toggle_hidden(app: &AppHandle) {
    let hide = !HIDDEN.load(Ordering::Relaxed);
    HIDDEN.store(hide, Ordering::Relaxed);
    if hide {
        if let Some(win) = app.get_webview_window("panel") {
            let _ = win.destroy();
            eprintln!("[overlay] 面板已销毁（释放 renderer）");
        }
    } else {
        ensure_panel(app);
        reshow(app);
    }
}

/// 面板不在就重建（隐藏时会销毁），并把子类、样式重新挂上去
pub fn ensure_panel(app: &AppHandle) {
    if app.get_webview_window("panel").is_some() {
        return;
    }
    if let Err(e) = crate::app::create_panel(app) {
        eprintln!("[overlay] 重建面板失败：{e}");
        return;
    }
    attach_panel(app);
    eprintln!("[overlay] 面板已重建");
}

/// 给面板挂上「最小化/被隐藏时自动恢复」的子类与扩展样式
fn attach_panel(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("panel") {
        let _ = win.set_ignore_cursor_events(!is_editing());
        if let Ok(raw) = win.hwnd() {
            unsafe {
                let _ = SetWindowSubclass(HWND(raw.0), Some(keep_alive_proc), 0x1002, 0);
            }
        }
    }
    apply_styles(app);
}

pub fn set_drag_hold(hold: bool) {
    DRAG_HOLD.store(hold, Ordering::Relaxed);
}

/// 子类化窗口过程：只有落在卡片上的点可点，其余穿透给桌面。
unsafe extern "system" fn hit_test_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_NCHITTEST && is_editing() && !DRAG_HOLD.load(Ordering::Relaxed) {
        let x = (lparam.0 as i32 & 0xFFFF) as i16 as f64;
        let y = ((lparam.0 as i32 >> 16) & 0xFFFF) as i16 as f64;
        let scale = *HIT_SCALE.lock().unwrap();
        let cx = x / scale;
        let cy = y / scale;
        let over = {
            let cells = HIT_CELLS.lock().unwrap();
            cells.iter().any(|&(x0, y0, w, h)| {
                cx >= x0 - 4.0 && cx <= x0 + w + 4.0 && cy >= y0 - 4.0 && cy <= y0 + h + 4.0
            })
        };
        if !over {
            return LRESULT(HTTRANSPARENT as isize);
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Win+D / 显示桌面会把面板最小化，等 800ms 轮询会明显卡一下。
/// 这里直接子类化拦 WM_SIZE(SIZE_MINIMIZED)，立刻恢复显示。
unsafe extern "system" fn keep_alive_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if !is_editing()
        && !HIDDEN.load(Ordering::Relaxed)
        && ((msg == WM_SIZE && wparam.0 == SIZE_MINIMIZED as usize)
            || (msg == WM_SHOWWINDOW && wparam.0 == 0))
    {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(insert_after(hwnd)),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    attach_panel(app);
    reshow(app);

    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(120));
        keep_bottom(&handle);
    });
    Ok(())
}

/// 切换编辑模式：开启时面板变成可交互的置顶窗口，退出时恢复置底 + 点击穿透
pub fn set_edit_mode(app: &AppHandle, on: bool) -> tauri::Result<()> {
    EDITING.store(on, Ordering::Relaxed);
    if on {
        // 编辑模式必须看得见面板：解除隐藏并在需要时重建
        HIDDEN.store(false, Ordering::Relaxed);
        ensure_panel(app);
    }
    DRAG_HOLD.store(false, Ordering::Relaxed);

    if let Some(win) = app.get_webview_window("panel") {
        let _ = win.set_ignore_cursor_events(!on);
        let _ = win.set_always_on_top(on);

        if let Ok(raw) = win.hwnd() {
            let h = HWND(raw.0);
            unsafe {
                if on {
                    let _ = SetWindowSubclass(h, Some(hit_test_proc), 0x1001, 0);
                    // 编辑时要能点到窗口，摘掉 NOACTIVATE
                    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
                    SetWindowLongPtrW(h, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
                } else {
                    let _ = RemoveWindowSubclass(h, Some(hit_test_proc), 0x1001);
                    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
                    let add = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
                    SetWindowLongPtrW(h, GWL_EXSTYLE, ex | add);
                }

                let _ = SetWindowPos(
                    h,
                    Some(if on { HWND_TOPMOST } else { insert_after(h) }),
                    0,
                    0,
                    0,
                    0,
                    if on {
                        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW
                    } else {
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
                    },
                );
                // 切换过程中窗口被系统藏起来过（WS_VISIBLE 变 false），统一再显示一遍
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    h,
                    Some(if on { HWND_TOPMOST } else { insert_after(h) }),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    if on {
        start_hit_refresh(app.clone());
    } else {
        HIT_CELLS.lock().unwrap().clear();
    }

    eprintln!("[overlay] edit mode -> {on}");
    let _ = app.emit("panel-edit", serde_json::json!({ "enabled": on }));
    Ok(())
}

/// 编辑模式下刷新「卡片矩形」，供 WM_NCHITTEST 判断（只读配置，不改窗口样式）
fn start_hit_refresh(app: AppHandle) {
    std::thread::spawn(move || {
        let mut logged = false;
        while is_editing() {
            std::thread::sleep(Duration::from_millis(120));
            let Some(win) = app.get_webview_window("panel") else { break };
            *HIT_SCALE.lock().unwrap() = win.scale_factor().unwrap_or(1.0);

            let state = app.state::<crate::app::AppState>();
            let cfg = state.panel.lock().unwrap().clone();
            let cells: Vec<(f64, f64, f64, f64)> = cfg
                .cards
                .iter()
                .map(|c| (c.x, c.y, c.w.unwrap_or(170.0), c.h.unwrap_or(170.0)))
                .collect();
            *HIT_CELLS.lock().unwrap() = cells;
            if !logged {
                logged = true;
                eprintln!(
                    "[overlay] hit cells={} scale={}",
                    HIT_CELLS.lock().unwrap().len(),
                    *HIT_SCALE.lock().unwrap()
                );
            }

            // 编辑期间窗口若被系统藏起来，补一次显示
            if let Ok(raw) = win.hwnd() {
                let h = HWND(raw.0);
                if !unsafe { IsWindowVisible(h) }.as_bool() {
                    unsafe {
                        let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                        let _ = SetWindowPos(
                            h,
                            Some(HWND_TOPMOST),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                        );
                    }
                }
            }
        }
    });
}

/// 目标：面板位于壁纸（Progman）之上、普通窗口之下。
/// SetWindowPos 的 hWndInsertAfter 表示「插到这个窗口下面」，
/// 所以要先沿 Z 序找到 Progman 正上方的那个窗口，把面板插到两者之间。
/// 找不到 Progman 时退回最底层。
fn insert_after(panel: HWND) -> HWND {
    unsafe {
        let Ok(progman) = FindWindowW(w!("Progman"), None) else {
            return HWND_BOTTOM;
        };
        let Ok(top) = GetTopWindow(None) else {
            return HWND_BOTTOM;
        };
        let mut w = top;
        let mut prev: Option<HWND> = None;
        while !w.0.is_null() {
            if w == progman {
                return prev.unwrap_or(HWND_TOP);
            }
            if w != panel {
                prev = Some(w);
            }
            let Ok(next) = GetWindow(w, GW_HWNDNEXT) else {
                break;
            };
            w = next;
        }
        HWND_BOTTOM
    }
}

fn hwnd(app: &AppHandle) -> Option<HWND> {
    let win = app.get_webview_window("panel")?;
    let raw = win.hwnd().ok()?.0 as isize;
    Some(HWND(raw as *mut c_void))
}

fn apply_styles(app: &AppHandle) {
    unsafe {
        let Some(h) = hwnd(app) else { return };
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let add = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex | add);
    }
}

/// 显示并铺满主屏，同时压到最底层。
pub fn reshow(app: &AppHandle) {
    unsafe {
        let Some(h) = hwnd(app) else { return };
        let w = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            h,
            Some(insert_after(h)),
            0,
            0,
            w,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

fn keep_bottom(app: &AppHandle) {
    // 编辑模式下窗口是置顶可交互的，别再往下压；用户手动隐藏时也不要复活
    if is_editing() || HIDDEN.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        let Some(h) = hwnd(app) else { return };
        // Win+D / 显示桌面会把普通顶层窗最小化，这里恢复
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
        if !IsWindowVisible(h).as_bool() {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
        let _ = SetWindowPos(
            h,
            Some(insert_after(h)),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}
