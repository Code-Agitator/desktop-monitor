//! 窗口管理：设置窗口按需创建，已存在则前置。

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("desktop-monitor 设置")
        .inner_size(1180.0, 800.0)
        .min_inner_size(920.0, 640.0)
        // 去掉系统标题栏，标题栏改由页面自己画（沉浸式）；shadow 让 Win11 保留圆角与投影，
        // 也让 DWM 继续负责拖边缘缩放
        .decorations(false)
        .shadow(true)
        .center()
        .build()?;
    let _ = win.set_focus();
    Ok(())
}

/// 开发用：渲染器总览页。只在 debug 构建或设置了 DM_DEV=1 时创建。
pub fn dev_enabled() -> bool {
    cfg!(debug_assertions) || std::env::var("DM_DEV").ok().as_deref() == Some("1")
}

pub fn open_dev(app: &AppHandle) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window("dev") {
        let _ = win.set_focus();
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(app, "dev", WebviewUrl::App("dev.html".into()))
        .title("desktop-monitor dev")
        .inner_size(1240.0, 900.0)
        .center()
        .build()?;
    let _ = win.set_focus();
    Ok(())
}
