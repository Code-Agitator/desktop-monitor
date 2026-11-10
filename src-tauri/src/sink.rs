//! 隐藏消息窗口：接收单实例广播等系统广播消息。
//! 后续 Explorer 重启（TaskbarCreated）也在这里接。

use std::sync::OnceLock;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
static mut ACTIVATE_MSG: u32 = 0;

pub fn start(app: tauri::AppHandle) {
    let _ = APP.set(app);
    std::thread::spawn(|| unsafe {
        ACTIVATE_MSG = RegisterWindowMessageW(crate::instance::ACTIVATE_MESSAGE_NAME);
        let hinst = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(_) => return,
        };
        let class = w!("DMSinkWnd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            w!("desktop-monitor sink"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst.into()),
            None,
        );
        if hwnd.is_err() {
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if ACTIVATE_MSG != 0 && msg == ACTIVATE_MSG {
        if let Some(app) = APP.get() {
            let a = app.clone();
            let b = a.clone();
            let _ = a.run_on_main_thread(move || {
                crate::overlay::reshow(&b);
                if let Err(e) = crate::windows::open_settings(&b) {
                    eprintln!("打开设置窗口失败：{e}");
                }
            });
        }
        return LRESULT(0);
    }
    DefWindowProcW(h, msg, wp, lp)
}
