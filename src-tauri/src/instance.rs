//! 单实例：命名互斥体 + 广播消息唤醒已有实例。

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW, HWND_BROADCAST};
use windows::Win32::System::Threading::CreateMutexW;

pub const ACTIVATE_MESSAGE_NAME: windows::core::PCWSTR = w!("DesktopMonitorActivate");

/// 返回 false 表示已有实例在运行（已广播唤醒消息）。
pub fn acquire() -> bool {
    unsafe {
        // 互斥体句柄本进程生命周期内不释放
        let _ = CreateMutexW(
            None,
            true,
            w!("Local\\desktop-monitor-single-instance"),
        );
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let msg = RegisterWindowMessageW(ACTIVATE_MESSAGE_NAME);
            if msg != 0 {
                let _ = PostMessageW(Some(HWND_BROADCAST), msg, WPARAM(0), LPARAM(0));
            }
            return false;
        }
        true
    }
}
