use std::sync::{Arc, Mutex};
use tauri::Manager;

pub struct AppState {
    pub store: Arc<crate::metrics::MetricStore>,
    pub panel: Arc<Mutex<crate::config::PanelConfig>>,
    pub hud: Arc<Mutex<crate::config::PanelConfig>>,
    pub taskbar: Arc<Mutex<crate::config::TaskbarConfig>>,
}

pub struct PluginState(pub Arc<crate::plugins::PluginManager>);

#[tauri::command]
fn save_surface_config(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    surface: String,
    config: crate::config::PanelConfig,
) -> Result<(), String> {
    let surface = if surface == "hud" {
        crate::config::Surface::Hud
    } else {
        crate::config::Surface::Panel
    };
    eprintln!(
        "[config] save {} {} 张卡片 from={} win={:?}",
        surface.file(),
        config.cards.len(),
        window.label(),
        config.window
    );
    crate::config::save(&app, surface, &config)?;
    match surface {
        crate::config::Surface::Panel => *state.panel.lock().unwrap() = config,
        crate::config::Surface::Hud => *state.hud.lock().unwrap() = config,
    }
    Ok(())
}

#[tauri::command]
fn list_plugins(plugins: tauri::State<'_, PluginState>) -> Vec<serde_json::Value> {
    plugins.0.list()
}

#[tauri::command]
fn reload_plugins(plugins: tauri::State<'_, PluginState>) {
    plugins.0.reload();
}

#[tauri::command]
fn open_plugins_dir(plugins: tauri::State<'_, PluginState>) {
    let _ = std::process::Command::new("explorer").arg(plugins.0.dir()).spawn();
}

#[tauri::command]
fn get_diag(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> serde_json::Value {
    let base = app
        .path()
        .config_dir()
        .map(|d| d.join("desktop-monitor"))
        .unwrap_or_default();
    serde_json::json!({
        "configDir": base.display().to_string(),
        "pluginsDir": base.join("plugins").display().to_string(),
        "metrics": state.store.defs().len(),
        "version": env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
fn popup_hud_menu(app: tauri::AppHandle) -> Result<(), String> {
    crate::tray::popup_hud_menu(&app).map_err(|e| e.to_string())
}

/// 设置窗口自绘标题栏要用的窗口操作（无边框窗口不再有系统按钮）
#[tauri::command]
fn settings_window(app: tauri::AppHandle, action: String) {
    let Some(win) = app.get_webview_window("settings") else {
        return;
    };
    match action.as_str() {
        "minimize" => {
            let _ = win.minimize();
        }
        "maximize" => {
            if win.is_maximized().unwrap_or(false) {
                let _ = win.unmaximize();
            } else {
                let _ = win.maximize();
            }
        }
        "close" => {
            let _ = win.close();
        }
        _ => {}
    }
}

#[tauri::command]
fn resize_hud(app: tauri::AppHandle, state: tauri::State<'_, AppState>, width: f64, height: f64) {
    let w = width.clamp(40.0, 4000.0);
    let h = height.clamp(24.0, 2000.0);
    if let Some(win) = app.get_webview_window("hud") {
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
    let mut cfg = state.hud.lock().unwrap().clone();
    let changed = match &cfg.window {
        Some(wc) => (wc.w - w).abs() > 0.5 || (wc.h - h).abs() > 0.5,
        None => true,
    };
    if changed {
        let wc = cfg.window.get_or_insert_with(crate::config::default_window);
        wc.w = w;
        wc.h = h;
        *state.hud.lock().unwrap() = cfg.clone();
        if let Err(e) = crate::config::save(&app, crate::config::Surface::Hud, &cfg) {
            eprintln!("[hud] 保存自动尺寸失败：{e}");
        }
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle, plugins: tauri::State<'_, PluginState>) {
    plugins.0.stop_all();
    app.exit(0);
}

/// 桌面编辑模式：开/关面板的直接编辑
#[tauri::command]
fn set_panel_edit(app: tauri::AppHandle, enabled: bool) -> serde_json::Value {
    match crate::overlay::set_edit_mode(&app, enabled) {
        Ok(_) => serde_json::json!({ "ok": true, "enabled": enabled }),
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    }
}

/// 拖动期间保持面板可交互（否则光标滑出卡片会立刻点击穿透）
#[tauri::command]
fn set_panel_edit_hold(hold: bool) {
    crate::overlay::set_drag_hold(hold);
}

#[tauri::command]
fn is_panel_edit() -> bool {
    crate::overlay::is_editing()
}

#[tauri::command]
fn get_screen_size() -> serde_json::Value {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    serde_json::json!({ "w": w, "h": h })
}

#[tauri::command]
fn get_taskbar_config(state: tauri::State<'_, AppState>) -> serde_json::Value {
    let cfg = state.taskbar.lock().unwrap().clone();
    serde_json::json!({ "config": cfg, "metrics": state.store.defs() })
}

#[tauri::command]
fn get_bootstrap(state: tauri::State<'_, AppState>) -> serde_json::Value {
    let panel = state.panel.lock().unwrap().clone();
    let hud = state.hud.lock().unwrap().clone();
    serde_json::json!({ "panel": panel, "hud": hud, "metrics": state.store.defs() })
}

pub fn run() {
    let defs = crate::source::definitions();
    let store = Arc::new(crate::metrics::MetricStore::new(defs));

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_bootstrap,
            get_taskbar_config,
            save_surface_config,
            list_plugins,
            reload_plugins,
            open_plugins_dir,
            get_diag,
            resize_hud,
            popup_hud_menu,
            settings_window,
            quit_app,
            get_screen_size,
            set_panel_edit,
            set_panel_edit_hold,
            is_panel_edit
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let plugins = crate::plugins::PluginManager::new(handle.clone(), store.clone());
            app.manage(PluginState(plugins.clone()));
            crate::tray::setup(app)?;
            if crate::windows::dev_enabled() {
                if let Err(e) = crate::windows::open_dev(&handle) {
                    eprintln!("dev 窗口创建失败：{e}");
                }
            }
            crate::overlay::setup(&handle)?;
            crate::sink::start(handle.clone());
            plugins.rescan();
            plugins.watch();

            let panel_cfg = crate::config::load_or_create(
                &handle,
                crate::config::Surface::Panel,
                crate::config::default_config,
            )
            .unwrap_or_else(|e| {
                eprintln!("面板配置加载失败：{e}");
                crate::config::default_config()
            });
            let hud_cfg = crate::config::load_or_create(
                &handle,
                crate::config::Surface::Hud,
                crate::config::default_hud_config,
            )
            .unwrap_or_else(|e| {
                eprintln!("HUD 配置加载失败：{e}");
                crate::config::default_hud_config()
            });

            let taskbar_cfg = crate::config::load_taskbar(&handle);
            let panel_shared = Arc::new(Mutex::new(panel_cfg));
            let hud_shared = Arc::new(Mutex::new(hud_cfg));
            let taskbar_shared = Arc::new(Mutex::new(taskbar_cfg));
            app.manage(AppState {
                store: store.clone(),
                panel: panel_shared.clone(),
                hud: hud_shared.clone(),
                taskbar: taskbar_shared.clone(),
            });

            crate::config::start_watch(handle.clone(), panel_shared, hud_shared.clone());
            if let Err(e) = crate::hud::setup(&handle, hud_shared) {
                eprintln!("HUD 窗口创建失败：{e}");
            }
            if let Err(e) = crate::taskbar::setup(&handle, taskbar_shared) {
                eprintln!("任务栏指标条创建失败：{e}");
            }
            crate::scheduler::start(handle.clone(), store.clone(), plugins.clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("desktop-monitor 启动失败");
}
