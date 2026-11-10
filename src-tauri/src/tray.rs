use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager};

fn tray_image() -> tauri::image::Image<'static> {
    let (w, h) = (32u32, 32u32);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - 16.0;
            let dy = y as f32 - 16.0;
            let d = (dx * dx + dy * dy).sqrt();
            let i = ((y * w + x) * 4) as usize;
            if d <= 13.0 {
                let ring = d > 11.0 || (12..20).contains(&x) && (12..20).contains(&y);
                let (r, g, b) = if ring { (90, 200, 255) } else { (32, 32, 36) };
                buf[i] = r;
                buf[i + 1] = g;
                buf[i + 2] = b;
                buf[i + 3] = 255;
            }
        }
    }
    tauri::image::Image::new_owned(buf, w, h)
}

/// 托盘菜单和 HUD 右键菜单共用同一套菜单项
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "show", "显示/隐藏面板", true, None::<&str>)?;
    let hud = MenuItem::with_id(app, "hud", "显示/隐藏 HUD", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "重新加载插件", true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "打开插件目录", true, None::<&str>)?;
    let taskbar = MenuItem::with_id(app, "taskbar", "任务栏指标", true, None::<&str>)?;
    let edit = MenuItem::with_id(app, "edit", "桌面编辑模式", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[&show, &hud, &taskbar, &edit, &settings, &reload, &folder, &quit],
    )
}

fn dispatch(app: &AppHandle, id: &str) {
    match id {
        "quit" => app.exit(0),
        "show" => crate::overlay::toggle_hidden(app),
        "hud" => crate::hud::toggle(app),
        "taskbar" => crate::taskbar::toggle(app),
        "edit" => {
            let on = !crate::overlay::is_editing();
            let _ = crate::overlay::set_edit_mode(app, on);
        }
        "settings" => {
            if let Err(e) = crate::windows::open_settings(app) {
                eprintln!("打开设置窗口失败：{e}");
            }
        }
        "reload" => {
            let state = app.state::<crate::app::PluginState>();
            state.0.reload();
        }
        "folder" => {
            let state = app.state::<crate::app::PluginState>();
            let _ = std::process::Command::new("explorer")
                .arg(state.0.dir())
                .spawn();
        }
        _ => {}
    }
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let handle = app.handle().clone();
    let menu = build_menu(&handle)?;

    TrayIconBuilder::new()
        .icon(tray_image())
        .tooltip("desktop-monitor")
        .menu(&menu)
        // 这个回调是全局的：托盘菜单和 HUD 右键菜单都会走到这里
        .on_menu_event(|app, event| dispatch(app, event.id.as_ref()))
        .build(app)?;
    Ok(())
}

/// HUD 里点右键：在鼠标位置弹出和托盘一致的菜单
pub fn popup_hud_menu(app: &AppHandle) -> tauri::Result<()> {
    let Some(win) = app.get_webview_window("hud") else {
        return Ok(());
    };
    let menu = build_menu(app)?;
    win.popup_menu(&menu)
}
