//! 用户偏好（目前只有「低显存模式」）。
//!
//! WebView2 的浏览器参数只能在**进程启动时**确定，而且同一个应用的所有窗口必须用
//! 同一套参数（参数不同会导致第二个窗口创建失败：0x8007139F ERROR_INVALID_STATE）。
//! 所以这里读一次偏好，所有窗口构建时都传 `browser_args()`。

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Tauri 默认会带上的参数（我们显式传入以保持所有窗口一致）
const BASE_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// 低显存模式：关掉 GPU 渲染与合成，WebView2 不再持有 GPU 表面
/// （实测 GPU 进程私有内存 127MB → 27MB，显存 104MB → 0，CPU 基本不变）
const LOW_MEM_ARGS: &str = "--disable-gpu --disable-gpu-compositing --disable-gpu-rasterization --js-flags=--max-old-space-size=64";

static LOW_MEM: AtomicBool = AtomicBool::new(false);
/// 手动代理（空 = 跟随系统代理）。形如 http://127.0.0.1:10809
static PROXY: Mutex<String> = Mutex::new(String::new());

fn path(app: &AppHandle) -> PathBuf {
    app.path()
        .config_dir()
        .map(|d| d.join("desktop-monitor").join("prefs.json"))
        .unwrap_or_else(|_| PathBuf::from("prefs.json"))
}

pub fn load(app: &AppHandle) -> bool {
    let raw = std::fs::read_to_string(path(app)).unwrap_or_default();
    let value: serde_json::Value =
        serde_json::from_str(raw.trim_start_matches('\u{feff}')).unwrap_or(serde_json::Value::Null);
    let on = value
        .get("lowGpuMemory")
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    let proxy = value
        .get("proxy")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    LOW_MEM.store(on, Ordering::Relaxed);
    *PROXY.lock().unwrap() = proxy;
    on
}

/// 手动代理地址（空 = 跟随系统）
pub fn proxy() -> String {
    PROXY.lock().unwrap().clone()
}

pub fn set_proxy(app: &AppHandle, value: &str) {
    *PROXY.lock().unwrap() = value.trim().to_string();
    save(app, low_mem());
}

pub fn low_mem() -> bool {
    LOW_MEM.load(Ordering::Relaxed)
}

pub fn save(app: &AppHandle, low: bool) {
    LOW_MEM.store(low, Ordering::Relaxed);
    let p = path(app);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::File::create(&p) {
        Ok(mut f) => {
            let proxy = PROXY.lock().unwrap().clone();
            let body = serde_json::json!({ "lowGpuMemory": low, "proxy": proxy });
            let _ = f.write_all(
                format!("{}\n", serde_json::to_string_pretty(&body).unwrap_or_default()).as_bytes(),
            );
        }
        Err(e) => eprintln!("[prefs] 写入失败：{e}"),
    }
}

/// 所有窗口构建时统一使用的浏览器参数
pub fn browser_args() -> String {
    if low_mem() {
        format!("{BASE_ARGS} {LOW_MEM_ARGS}")
    } else {
        BASE_ARGS.to_string()
    }
}
