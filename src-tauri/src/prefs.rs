//! 用户偏好（目前只有「低显存模式」）。
//!
//! WebView2 的浏览器参数只能在**进程启动时**确定，而且同一个应用的所有窗口必须用
//! 同一套参数（参数不同会导致第二个窗口创建失败：0x8007139F ERROR_INVALID_STATE）。
//! 所以这里读一次偏好，所有窗口构建时都传 `browser_args()`。

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};

/// Tauri 默认会带上的参数（我们显式传入以保持所有窗口一致）
const BASE_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// 低显存模式：关掉 GPU 渲染与合成，WebView2 不再持有 GPU 表面
/// （实测 GPU 进程私有内存 127MB → 27MB，显存 104MB → 0，CPU 基本不变）
const LOW_MEM_ARGS: &str = "--disable-gpu --disable-gpu-compositing --disable-gpu-rasterization --js-flags=--max-old-space-size=64";

static LOW_MEM: AtomicBool = AtomicBool::new(false);

fn path(app: &AppHandle) -> PathBuf {
    app.path()
        .config_dir()
        .map(|d| d.join("desktop-monitor").join("prefs.json"))
        .unwrap_or_else(|_| PathBuf::from("prefs.json"))
}

pub fn load(app: &AppHandle) -> bool {
    let on = std::fs::read_to_string(path(app))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| v.get("lowGpuMemory").and_then(|b| b.as_bool()))
        .unwrap_or(false);
    LOW_MEM.store(on, Ordering::Relaxed);
    on
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
            let _ = f.write_all(
                format!("{{\n  \"lowGpuMemory\": {low}\n}}\n").as_bytes(),
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
