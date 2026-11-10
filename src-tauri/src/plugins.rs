//! 插件管理：目录发现、NDJSON/JSON-RPC 传输、进程监管（超时/退避/重启/禁用）。

use crate::metrics::{MetricDef, MetricKind, MetricStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use notify::Watcher;
use tauri::{AppHandle, Emitter, Manager};

const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(rename = "protocolVersion", default = "one")]
    pub protocol_version: u32,
    pub entry: Entry,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(rename = "timeoutMs", default = "default_timeout")]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

fn one() -> u32 {
    1
}

fn default_timeout() -> u64 {
    5000
}

struct HandleState {
    stdin: Option<ChildStdin>,
    child: Option<Child>,
    pending: HashMap<u64, mpsc::Sender<Result<Value, String>>>,
}

pub struct PluginHandle {
    pub manifest: Manifest,
    next_id: AtomicU64,
    alive: AtomicBool,
    state: Mutex<HandleState>,
    /// 插件通过 "metrics" 通知重报指标列表时调用（设备热插拔）
    on_metrics: Mutex<Option<Arc<dyn Fn(Vec<MetricDef>) + Send + Sync>>>,
}

impl PluginHandle {
    fn new(manifest: Manifest) -> Self {
        Self {
            manifest,
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(false),
            state: Mutex::new(HandleState {
                stdin: None,
                child: None,
                pending: HashMap::new(),
            }),
            on_metrics: Mutex::new(None),
        }
    }

    fn set_on_metrics(&self, cb: Arc<dyn Fn(Vec<MetricDef>) + Send + Sync>) {
        *self.on_metrics.lock().unwrap() = Some(cb);
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let timeout = self.manifest.timeout_ms.max(500);
        self.request_with_timeout(method, params, timeout)
    }

    fn request_with_timeout(&self, method: &str, params: Value, timeout_ms: u64) -> Result<Value, String> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err("插件进程未运行".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.state.lock().unwrap().pending.insert(id, tx);

        let written = {
            let mut st = self.state.lock().unwrap();
            match st.stdin.as_mut() {
                Some(stdin) => {
                    let mut line =
                        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
                            .to_string();
                    line.push('\n');
                    stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush())
                }
                None => Err(std::io::Error::other("插件 stdin 不可用")),
            }
        };
        if written.is_err() {
            self.state.lock().unwrap().pending.remove(&id);
            return Err("写入插件失败".into());
        }

        match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
            Ok(r) => r,
            Err(_) => {
                self.state.lock().unwrap().pending.remove(&id);
                Err(format!("插件响应超时（{method}）"))
            }
        }
    }

    fn on_line(&self, line: &str) {
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                eprintln!("[plugin:{}] {}", self.manifest.id, line);
                return;
            }
        };
        if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
            let sender = self.state.lock().unwrap().pending.remove(&id);
            if let Some(tx) = sender {
                let result = if let Some(err) = value.get("error") {
                    Err(err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("插件返回错误")
                        .to_string())
                } else {
                    Ok(value.get("result").cloned().unwrap_or(Value::Null))
                };
                let _ = tx.send(result);
            }
            return;
        }
        if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
            match method {
                "log" => {
                    let params = value.get("params").cloned().unwrap_or(Value::Null);
                    eprintln!("[plugin:{}] {}", self.manifest.id, params);
                }
                "metrics" => {
                    let defs = parse_metrics(
                        &self.manifest.id,
                        value.get("params").and_then(|p| p.get("metrics")),
                    );
                    if let Some(cb) = self.on_metrics.lock().unwrap().clone() {
                        cb(defs);
                    }
                }
                _ => {}
            }
        }
    }

    fn mark_dead(&self) {
        self.alive.store(false, Ordering::SeqCst);
        let mut st = self.state.lock().unwrap();
        for (_, tx) in st.pending.drain() {
            let _ = tx.send(Err("插件进程已退出".into()));
        }
        if let Some(mut child) = st.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        st.stdin = None;
    }
}

pub struct PluginManager {
    app: AppHandle,
    store: Arc<MetricStore>,
    dir: PathBuf,
    plugins: Mutex<HashMap<String, Arc<PluginHandle>>>,
    stops: Mutex<HashMap<String, Arc<AtomicBool>>>,
    supervised: Mutex<std::collections::HashSet<String>>,
    /// 已发现的 manifest（用于设置窗口的插件列表）
    known: Mutex<HashMap<String, Manifest>>,
    /// 最近一次状态：id -> (status, detail, ts)
    statuses: Mutex<HashMap<String, (String, Option<String>, u64)>>,
    /// 插件 stderr 落盘目录（GUI 宿主没有控制台）
    log_dir: PathBuf,
}

impl PluginManager {
    pub fn new(app: AppHandle, store: Arc<MetricStore>) -> Arc<Self> {
        let dir = app
            .path()
            .config_dir()
            .map(|d| d.join("desktop-monitor").join("plugins"))
            .unwrap_or_else(|_| PathBuf::from("plugins"));
        let _ = std::fs::create_dir_all(&dir);
        let log_dir = app
            .path()
            .local_data_dir()
            .map(|d| d.join("desktop-monitor").join("logs"))
            .unwrap_or_else(|_| dir.join("logs"));
        let _ = std::fs::create_dir_all(&log_dir);
        Arc::new(Self {
            app,
            store,
            dir,
            plugins: Mutex::new(HashMap::new()),
            stops: Mutex::new(HashMap::new()),
            supervised: Mutex::new(std::collections::HashSet::new()),
            known: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
            log_dir,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 设置窗口用：所有已发现插件的状态与指标数。
    pub fn list(&self) -> Vec<Value> {
        let known = self.known.lock().unwrap().clone();
        let statuses = self.statuses.lock().unwrap().clone();
        let defs = self.store.defs();
        let mut out: Vec<Value> = known
            .iter()
            .map(|(id, manifest)| {
                let (status, detail, ts) = statuses
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| ("discovered".into(), None, 0));
                let metrics = defs
                    .iter()
                    .filter(|d| d.plugin.as_deref() == Some(id.as_str()))
                    .count();
                json!({
                    "id": id,
                    "name": manifest.name,
                    "version": manifest.version,
                    "status": status,
                    "detail": detail,
                    "ts": ts,
                    "metrics": metrics,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            a.get("id").and_then(|v| v.as_str()).cmp(&b.get("id").and_then(|v| v.as_str()))
        });
        out
    }

    fn emit_status(&self, id: &str, status: &str, detail: Option<String>) {
        eprintln!("[plugin:{id}] {status}{}", detail.as_deref().map(|d| format!(" — {d}")).unwrap_or_default());
        self.statuses.lock().unwrap().insert(
            id.to_string(),
            (status.to_string(), detail.clone(), crate::scheduler::now_ms()),
        );
        let _ = self.app.emit(
            "plugin-status",
            json!({ "id": id, "status": status, "detail": detail }),
        );
    }

    /// 扫描插件目录，启动新增、停止移除。
    pub fn rescan(self: &Arc<Self>) {
        let found = discover(&self.dir);
        let existing: Vec<String> = self.plugins.lock().unwrap().keys().cloned().collect();

        for id in existing {
            if !found.contains_key(&id) {
                self.stop(&id);
            }
        }
        {
            let mut known = self.known.lock().unwrap();
            known.retain(|id, _| found.contains_key(id));
            for (id, (manifest, _)) in &found {
                known.insert(id.clone(), manifest.clone());
            }
        }

        for (id, (manifest, dir)) in found {
            // 已经有监管线程就跳过：rescan 可能被目录事件高频触发
            if !self.supervised.lock().unwrap().insert(id.clone()) {
                continue;
            }
            self.spawn_supervisor(manifest, dir);
        }
    }

    pub fn stop(&self, id: &str) {
        self.supervised.lock().unwrap().remove(id);
        if let Some(flag) = self.stops.lock().unwrap().get(id) {
            flag.store(true, Ordering::SeqCst);
        }
        if let Some(h) = self.plugins.lock().unwrap().remove(id) {
            let _ = h.request("shutdown", Value::Null);
            h.mark_dead();
        }
        self.stops.lock().unwrap().remove(id);
        self.store.remove_by_plugin(id);
        let _ = self.app.emit("metric-defs", self.store.defs());
        self.emit_status(id, "stopped", None);
    }

    pub fn stop_all(&self) {
        let ids: Vec<String> = self.plugins.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }

    pub fn reload(self: &Arc<Self>) {
        self.stop_all();
        self.rescan();
    }

    fn spawn_supervisor(self: &Arc<Self>, manifest: Manifest, dir: PathBuf) {
        let stop = Arc::new(AtomicBool::new(false));
        self.stops
            .lock()
            .unwrap()
            .insert(manifest.id.clone(), stop.clone());

        let manager = self.clone();
        std::thread::spawn(move || {
            let id = manifest.id.clone();
            let mut restarts: Vec<Instant> = Vec::new();

            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                manager.emit_status(&id, "starting", None);

                match manager.spawn_once(manifest.clone(), &dir, &stop) {
                    Ok(handle) => {
                        manager
                            .plugins
                            .lock()
                            .unwrap()
                            .insert(id.clone(), handle.clone());

                        // 等待进程退出或被停止
                        while !stop.load(Ordering::SeqCst) && handle.alive.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(200));
                        }
                        manager.plugins.lock().unwrap().remove(&id);
                        handle.mark_dead();
                        // 保留指标定义与最后的值，让卡片进入"失效"态而不是消失

                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                    Err(e) => {
                        manager.emit_status(&id, "failed", Some(e));
                    }
                }

                // 重启退避与熔断
                let now = Instant::now();
                restarts.push(now);
                restarts.retain(|t| now.duration_since(*t).as_secs() < 600);
                if restarts.len() > 5 {
                    // 保留指标定义与最后的值，卡片继续显示"失效"态；可在托盘手动重新加载
                    manager.emit_status(&id, "disabled", Some("10 分钟内重启超过 5 次".into()));
                    break;
                }
                let backoff = match restarts.len() {
                    1 => 1,
                    2 => 5,
                    _ => 15,
                };
                manager.emit_status(&id, "restarting", Some(format!("{backoff}s 后重试")));
                let deadline = Instant::now() + Duration::from_secs(backoff);
                while Instant::now() < deadline && !stop.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            manager.supervised.lock().unwrap().remove(&id);
        });
    }

    fn spawn_once(
        &self,
        manifest: Manifest,
        dir: &Path,
        stop: &Arc<AtomicBool>,
    ) -> Result<Arc<PluginHandle>, String> {
        let handle = Arc::new(PluginHandle::new(manifest.clone()));
        {
            let store = self.store.clone();
            let app = self.app.clone();
            let id = manifest.id.clone();
            handle.set_on_metrics(Arc::new(move |defs| {
                eprintln!("[plugin:{id}] 指标列表更新（{} 个）", defs.len());
                store.replace_for_plugin(&id, defs);
                let _ = app.emit("metric-defs", store.defs());
            }));
        }

        // 相对入口名按插件目录解析（Command 不会用 current_dir 解析程序路径）
        let mut program = PathBuf::from(&manifest.entry.command);
        if program.is_relative() {
            let candidate = dir.join(&program);
            if candidate.exists() {
                program = candidate;
            }
        }
        let mut cmd = Command::new(&program);
        cmd.args(&manifest.entry.args)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // GUI 宿主没有控制台；不隐藏的话，控制台插件会各自弹出一个控制台窗口
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn().map_err(|e| format!("启动失败：{e}"))?;
        let stdin = child.stdin.take().ok_or("无法获取插件 stdin")?;
        let stdout = child.stdout.take().ok_or("无法获取插件 stdout")?;

        // stderr 吸入管道，转存到 logs/<plugin>.log（GUI 下 eprintln 不可见）
        if let Some(stderr) = child.stderr.take() {
            let id = manifest.id.clone();
            let log_dir = self.log_dir.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    eprintln!("[plugin:{id}] {line}");
                    let _ = std::fs::create_dir_all(&log_dir);
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(log_dir.join(format!("{id}.log")))
                    {
                        use std::io::Write;
                        let _ = writeln!(f, "{line}");
                    }
                }
            });
        }

        {
            let mut st = handle.state.lock().unwrap();
            st.stdin = Some(stdin);
            st.child = Some(child);
        }
        handle.alive.store(true, Ordering::SeqCst);

        // 读取线程：按行解析 NDJSON，EOF 表示进程退出
        {
            let h = handle.clone();
            let id = manifest.id.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            if !l.trim().is_empty() {
                                h.on_line(&l);
                            }
                        }
                        Err(_) => break,
                    }
                }
                eprintln!("[plugin:{id}] 进程输出结束");
                h.alive.store(false, Ordering::SeqCst);
            });
        }

        // 初始化：首次加载传感器驱动可能较慢，给更长的超时；失败必须回收进程
        let result = match handle.request_with_timeout(
            "initialize",
            json!({ "protocolVersion": PROTOCOL_VERSION, "config": null }),
            15000,
        ) {
            Ok(v) => v,
            Err(e) => {
                handle.mark_dead();
                return Err(e);
            }
        };
        let proto = result
            .get("protocolVersion")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if proto != PROTOCOL_VERSION as u64 {
            handle.mark_dead();
            return Err(format!("协议版本不匹配：插件 {proto}，宿主 {PROTOCOL_VERSION}"));
        }
        let metrics = parse_metrics(&manifest.id, result.get("metrics"));
        self.store.add_defs(metrics.clone());
        let _ = self.app.emit("metric-defs", self.store.defs());
        let _ = stop;
        self.emit_status(
            &manifest.id,
            "running",
            Some(format!("{} 个指标", metrics.len())),
        );
        Ok(handle)
    }

    pub fn poll(&self, id: &str, ids: &[String]) -> Result<HashMap<String, Value>, String> {
        let handle = self
            .plugins
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| "插件未运行".to_string())?;
        let result = handle.request(
            "poll",
            json!({ "ids": ids, "ts": crate::scheduler::now_ms() }),
        )?;
        let mut out = HashMap::new();
        if let Some(obj) = result.get("values").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                out.insert(k.clone(), v.clone());
            }
        }
        Ok(out)
    }

    pub fn watch(self: &Arc<Self>) {
        let dir = self.dir.clone();
        let manager = self.clone();
        std::thread::spawn(move || {
            let (tx, rx) = mpsc::channel();
            let mut watcher = match notify::recommended_watcher(
                move |res: notify::Result<notify::Event>| {
                    let _ = tx.send(res);
                },
            ) {
                Ok(w) => w,
                Err(_) => return,
            };
            if watcher.watch(&dir, notify::RecursiveMode::NonRecursive).is_err() {
                return;
            }
            loop {
                match rx.recv() {
                    Ok(_) => {
                        // 去抖：等目录写入稳定
                        while rx.recv_timeout(Duration::from_millis(400)).is_ok() {}
                        manager.rescan();
                    }
                    Err(_) => break,
                }
            }
            let _ = watcher;
        });
    }
}

fn discover(dir: &Path) -> HashMap<String, (Manifest, PathBuf)> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("manifest.json");
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        match serde_json::from_str::<Manifest>(&text) {
            Ok(manifest) => {
                if manifest.protocol_version != PROTOCOL_VERSION {
                    eprintln!(
                        "[plugins] {} 协议版本 {} 不受支持",
                        manifest.id, manifest.protocol_version
                    );
                    continue;
                }
                out.insert(manifest.id.clone(), (manifest, path));
            }
            Err(e) => eprintln!("[plugins] manifest 解析失败 {}: {e}", manifest_path.display()),
        }
    }
    out
}

fn parse_metrics(plugin_id: &str, value: Option<&Value>) -> Vec<MetricDef> {
    let Some(arr) = value.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            let title = m.get("title").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
            let kind: MetricKind = match serde_json::from_value(m.get("kind").cloned().unwrap_or(Value::Null)) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("[plugins] 跳过指标 {id}：kind 解析失败 {e}");
                    return None;
                }
            };
            Some(MetricDef {
                id,
                title,
                kind,
                unit: m.get("unit").and_then(|v| v.as_str()).map(Into::into),
                interval_ms: m.get("intervalMs").and_then(|v| v.as_u64()).unwrap_or(1000),
                history: m.get("history").and_then(|v| v.as_bool()).unwrap_or(false),
                min: m.get("min").and_then(|v| v.as_f64()),
                max: m.get("max").and_then(|v| v.as_f64()),
                plugin: Some(plugin_id.to_string()),
            })
        })
        .collect()
}
