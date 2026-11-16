//! 自动更新：启动后检查 GitHub Release（通过 tauri-plugin-updater 的 latest.json），
//! 有新版本就缓存并广播 `update-available`；真正的下载/安装由设置页触发
//! （`@tauri-apps/plugin-updater` 的 downloadAndInstall + process.relaunch）。
//!
//! 注意：只有用 `tauri.release.conf.json` 构建的包才带 pubkey/endpoints，
//! 本地开发构建 check() 会直接报错，这里吞掉错误只记日志。

use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, serde::Serialize)]
pub struct UpdateInfo {
    /// 远端最新版本（不带 v 前缀）
    pub version: String,
    /// 当前版本
    pub current: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

static INFO: Mutex<Option<UpdateInfo>> = Mutex::new(None);

/// 按偏好构建 updater：手动代理优先，否则交给插件的 system-proxy 特性（跟随系统设置）
fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app.updater_builder();
    let proxy = crate::prefs::proxy();
    if !proxy.is_empty() {
        let url: url::Url =
            proxy.parse().map_err(|e| format!("代理地址无效（{proxy}）：{e}"))?;
        builder = builder.proxy(url);
    }
    builder.build().map_err(|e| e.to_string())
}

/// 查一次；没有新版本返回 Ok(None)
fn check_once(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = build_updater(app)?;
    let current = app.package_info().version.to_string();
    let found = tauri::async_runtime::block_on(updater.check()).map_err(|e| e.to_string())?;
    Ok(found.map(|u| UpdateInfo {
        version: u.version.clone(),
        current,
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    }))
}

/// 后台循环：启动 20 秒后查一次，之后每 6 小时一次
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        loop {
            match check_once(&app) {
                Ok(Some(info)) => {
                    eprintln!("[update] 发现新版本 {}（当前 {}）", info.version, info.current);
                    *INFO.lock().unwrap() = Some(info.clone());
                    let _ = app.emit("update-available", info);
                }
                Ok(None) => {
                    eprintln!(
                        "[update] 已是最新版本 {}",
                        app.package_info().version
                    );
                    *INFO.lock().unwrap() = None;
                }
                Err(e) => eprintln!("[update] 检查失败：{e}"),
            }
            std::thread::sleep(Duration::from_secs(6 * 3600));
        }
    });
}

#[tauri::command]
pub fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let info = check_once(&app)?;
    *INFO.lock().unwrap() = info.clone();
    Ok(info)
}

#[tauri::command]
pub fn update_state() -> Option<UpdateInfo> {
    INFO.lock().unwrap().clone()
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
