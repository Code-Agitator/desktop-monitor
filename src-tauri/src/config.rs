//! 面板配置：桌面面板与 HUD 各一份 JSON（%APPDATA%\desktop-monitor\），
//! 首次运行写入默认布局，之后用文件监听热重载并推给前端。

use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    Panel,
    Hud,
}

impl Surface {
    pub fn file(self) -> &'static str {
        match self {
            Surface::Panel => "panel.json",
            Surface::Hud => "hud.json",
        }
    }

    /// 文件变化时推给前端的事件名
    pub fn event(self) -> &'static str {
        match self {
            Surface::Panel => "panel-config",
            Surface::Hud => "hud-config",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PanelConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub cards: Vec<CardConfig>,
    /// 仅 HUD 使用：窗口位置/尺寸与启用状态
    #[serde(default)]
    pub window: Option<WindowConfig>,
    /// 卡片背景不透明度（0.2 ~ 1.0）：唯一的视觉旋钮
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

fn default_opacity() -> f64 {
    1.0
}

fn default_version() -> u32 {
    1
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WindowConfig {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "default_hud_w")]
    pub w: f64,
    #[serde(default = "default_hud_h")]
    pub h: f64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

pub fn default_window() -> WindowConfig {
    // 默认放右上角，避免压住桌面面板左上角的卡片
    let (sw, _) = unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};
        let w = GetSystemMetrics(SM_CXSCREEN);
        (w, 0)
    };
    let x = if sw > 0 {
        (sw as f64 - default_hud_w() - 40.0).max(0.0)
    } else {
        48.0
    };
    WindowConfig {
        x,
        y: 48.0,
        w: default_hud_w(),
        h: default_hud_h(),
        enabled: true,
    }
}

fn default_hud_w() -> f64 {
    // 内容 2×87 + 左右各 2 留白
    178.0
}

fn default_hud_h() -> f64 {
    // 内容 2×22 + 上下各 2 留白
    48.0
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CardConfig {
    pub id: String,
    pub metric: String,
    pub view: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub w: Option<f64>,
    #[serde(default)]
    pub h: Option<f64>,
    #[serde(default)]
    pub options: Value,
    #[serde(default)]
    pub style: Value,
    /// 融合组：同组卡片渲染成一个视觉块；None = 独立卡片
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// 单块主题：覆盖所在面板/HUD 的主题；None = 跟随整体
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

fn card(
    id: &str,
    metric: &str,
    view: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    options: Value,
) -> CardConfig {
    CardConfig {
        id: id.into(),
        metric: metric.into(),
        view: view.into(),
        x,
        y,
        w: Some(w),
        h: Some(h),
        options,
        style: Value::Null,
        group: None,
        theme: None,
    }
}

pub fn default_config() -> PanelConfig {
    PanelConfig {
        version: 1,
        theme: Some("default-dark".into()),
        window: None,
        opacity: 1.0,
        cards: vec![
            card("cpu", "sys.cpu.usage", "ring", 64.0, 72.0, 170.0, 170.0, json!({ "label": "CPU" })),
            card("cpu-temp", "sys.cpu.temp", "number", 246.0, 72.0, 150.0, 84.0, json!({})),
            card("cpu-freq", "sys.cpu.freq", "number", 246.0, 168.0, 150.0, 84.0, json!({})),
            card("clock", "sys.clock", "text", 408.0, 72.0, 230.0, 170.0, json!({ "align": "right" })),
            card("mem", "sys.mem.usage", "bar", 64.0, 262.0, 260.0, 84.0, json!({ "label": "内存" })),
            card("mem-used", "sys.mem.used", "number", 336.0, 262.0, 230.0, 84.0, json!({})),
            card(
                "net",
                "sys.net.hist",
                "sparkline",
                64.0,
                356.0,
                260.0,
                96.0,
                json!({ "label": "网络下行", "unit": "B/s" }),
            ),
            card("status", "sys.status", "status", 64.0, 472.0, 260.0, 60.0, json!({})),
            card("disk", "sys.disk.parts", "list", 64.0, 548.0, 260.0, 150.0, json!({ "label": "磁盘" })),
            card("gpu", "sys.gpu.usage", "ring", 700.0, 72.0, 170.0, 170.0, json!({ "label": "GPU" })),
            card("gpu-temp", "sys.gpu.temp", "number", 880.0, 72.0, 150.0, 84.0, json!({})),
            card("gpu-power", "sys.gpu.power", "number", 880.0, 168.0, 150.0, 84.0, json!({})),
            card("gpu-fan", "sys.gpu.fan", "number", 880.0, 262.0, 150.0, 84.0, json!({})),
            card("gpu-mem", "sys.gpu.mem", "number", 700.0, 262.0, 170.0, 84.0, json!({})),
            card("uptime", "sys.uptime", "number", 700.0, 364.0, 170.0, 84.0, json!({})),
        ],
    }
}

/// HUD 单元格尺寸：两列 × 两行，每列是一个独立融合块
const HUD_CELL_W: f64 = 87.0;
const HUD_CELL_H: f64 = 22.0;
/// 两块之间不留缝：贴边渲染成一条连续的形状（共享边会抹平圆角和边框）
const HUD_BLOCK_GAP: f64 = 0.0;

/// HUD 单元格：左右排版（标签在左、数值在右），按列/行定位，并归属到指定的融合块与主题
fn hud_cell(
    id: &str,
    metric: &str,
    col: usize,
    row: usize,
    label: &str,
    group: &str,
    theme: &str,
) -> CardConfig {
    let mut c = card(
        id,
        metric,
        "number",
        col as f64 * (HUD_CELL_W + HUD_BLOCK_GAP),
        row as f64 * HUD_CELL_H,
        HUD_CELL_W,
        HUD_CELL_H,
        json!({ "label": label, "layout": "inline" }),
    );
    c.group = Some(group.into());
    c.theme = Some(theme.into());
    c
}

pub fn default_hud_config() -> PanelConfig {
    PanelConfig {
        version: 1,
        theme: Some("default-dark".into()),
        window: Some(default_window()),
        opacity: 1.0,
        cards: vec![
            // 左块：CPU / 内存；右上块：网络上行；右下块：网络下行（标签用箭头）
            hud_cell("hud-cpu", "sys.cpu.usage", 0, 0, "CPU", "g-hud-left", "default-dark"),
            hud_cell("hud-mem", "sys.mem.usage", 0, 1, "内存", "g-hud-left", "default-dark"),
            hud_cell("hud-net-up", "sys.net.up", 1, 0, "↑", "g-hud-up", "midnight"),
            hud_cell("hud-net-down", "sys.net.down", 1, 1, "↓", "g-hud-down", "violet"),
        ],
    }
}

/// 任务栏指标条：任务栏不支持 deskband，这里用「置顶透明小窗 + 跟随任务栏」实现，
/// 配置单独放 taskbar.json，改动后重启生效（或从托盘菜单切换开关）。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 文字条宽度（逻辑像素）
    #[serde(default = "default_taskbar_width")]
    pub width: f64,
    /// 显示哪些指标，顺序即显示顺序
    #[serde(default = "default_taskbar_items")]
    pub items: Vec<String>,
}

fn default_taskbar_width() -> f64 {
    248.0
}

fn default_taskbar_items() -> Vec<String> {
    vec![
        "sys.cpu.usage".into(),
        "sys.mem.usage".into(),
        "sys.net.up".into(),
        "sys.net.down".into(),
    ]
}

pub fn default_taskbar_config() -> TaskbarConfig {
    TaskbarConfig {
        enabled: true,
        width: default_taskbar_width(),
        items: default_taskbar_items(),
    }
}

pub fn taskbar_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(base_dir(app)?.join("taskbar.json"))
}

pub fn load_taskbar(app: &AppHandle) -> TaskbarConfig {
    let Ok(p) = taskbar_path(app) else {
        return default_taskbar_config();
    };
    // 首次运行把默认配置落盘，方便用户直接编辑要显示哪些指标
    if !p.exists() {
        let cfg = default_taskbar_config();
        let _ = save_taskbar(app, &cfg);
        return cfg;
    }
    let Ok(text) = fs::read_to_string(&p) else {
        return default_taskbar_config();
    };
    serde_json::from_str(&text).unwrap_or_else(|_| default_taskbar_config())
}

pub fn save_taskbar(app: &AppHandle, cfg: &TaskbarConfig) -> Result<(), String> {
    let p = taskbar_path(app)?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| e.to_string())
}

/// 每个配置文件最近一次由本进程读入/写出的文本，用于检测外部修改
static LAST_TEXT: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();

fn last_text() -> &'static Mutex<HashMap<PathBuf, String>> {
    LAST_TEXT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn base_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .config_dir()
        .map_err(|e| e.to_string())?
        .join("desktop-monitor");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn path(app: &AppHandle, surface: Surface) -> Result<PathBuf, String> {
    Ok(base_dir(app)?.join(surface.file()))
}

pub fn load(app: &AppHandle, surface: Surface) -> Result<PanelConfig, String> {
    let path = path(app, surface)?;
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let cfg = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    last_text().lock().unwrap().insert(path, text);
    Ok(cfg)
}

pub fn save(app: &AppHandle, surface: Surface, cfg: &PanelConfig) -> Result<(), String> {
    let path = path(app, surface)?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;

    // 若文件在此期间被外部修改过，拒绝覆盖，交由前端重新载入
    if let Ok(current) = fs::read_to_string(&path) {
        let known = last_text().lock().unwrap().get(&path).cloned();
        if let Some(known) = known {
            if current != known {
                return Err("配置文件已被外部修改，请重新载入后再保存".into());
            }
        }
    }

    fs::write(&path, &text).map_err(|e| e.to_string())?;
    last_text().lock().unwrap().insert(path, text);
    Ok(())
}

pub fn load_or_create(
    app: &AppHandle,
    surface: Surface,
    default: impl FnOnce() -> PanelConfig,
) -> Result<PanelConfig, String> {
    let path = path(app, surface)?;
    if !path.exists() {
        let cfg = default();
        let text = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        fs::write(&path, &text).map_err(|e| e.to_string())?;
        last_text().lock().unwrap().insert(path, text);
        return Ok(cfg);
    }
    load(app, surface)
}

pub fn start_watch(
    app: AppHandle,
    panel: Arc<Mutex<PanelConfig>>,
    hud: Arc<Mutex<PanelConfig>>,
) {
    let panel_path = path(&app, Surface::Panel).ok();
    let hud_path = path(&app, Surface::Hud).ok();
    let Some(dir) = panel_path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf()) else {
        return;
    };

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(
            move |res: notify::Result<notify::Event>| {
                let _ = tx.send(res);
            },
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[config] 创建监听失败：{e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
            eprintln!("[config] watch {} 失败：{e}", dir.display());
            return;
        }
        eprintln!("[config] watching {}", dir.display());

        loop {
            let mut touched: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
            match rx.recv() {
                Ok(Ok(event)) => touched.extend(event.paths),
                Ok(Err(_)) => continue,
                Err(_) => break,
            }
            // 去抖：编辑器保存往往连发多次事件，把这一阵的路径合并处理
            loop {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(Ok(event)) => {
                        touched.extend(event.paths);
                    }
                    Ok(Err(_)) => {}
                    Err(_) => break,
                }
            }

            reload_if(&app, Surface::Panel, &panel_path, &touched, &panel);
            reload_if(&app, Surface::Hud, &hud_path, &touched, &hud);
        }
        let _ = watcher;
    });
}

fn reload_if(
    app: &AppHandle,
    surface: Surface,
    expected: &Option<PathBuf>,
    touched: &std::collections::HashSet<PathBuf>,
    shared: &Arc<Mutex<PanelConfig>>,
) {
    let Some(path) = expected else { return };
    let hit = touched.iter().any(|p| {
        p == path || p.to_string_lossy().eq_ignore_ascii_case(&path.to_string_lossy())
    });
    if !hit {
        return;
    }
    match load(app, surface) {
        Ok(cfg) => {
            eprintln!("[config] {} reload {} 张卡片", surface.file(), cfg.cards.len());
            {
                let mut guard = shared.lock().unwrap();
                *guard = cfg.clone();
            }
            let _ = app.emit(surface.event(), cfg);
        }
        Err(e) => eprintln!("[config] {} reload 失败：{e}", surface.file()),
    }
}
