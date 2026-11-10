//! 指标类型与存储：带类型的度量 + 环形历史缓冲。
//! 定义可以动态增删（插件上下线），渲染提示由 MetricDef 携带。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum MetricKind {
    Scalar,
    Ratio,
    Bytes,
    Duration,
    State,
    Series,
    List,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricDef {
    pub id: String,
    pub title: String,
    pub kind: MetricKind,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default = "default_interval")]
    pub interval_ms: u64,
    #[serde(default)]
    pub history: bool,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// None 表示内置数据源；Some(plugin_id) 表示插件提供
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

fn default_interval() -> u64 {
    1000
}

#[derive(Clone, Serialize)]
pub struct Sample {
    pub ts: u64,
    pub v: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct MetricStore {
    defs: Mutex<Vec<MetricDef>>,
    current: Mutex<HashMap<String, Sample>>,
    history: Mutex<HashMap<String, VecDeque<f64>>>,
    history_cap: usize,
}

impl MetricStore {
    pub fn new(defs: Vec<MetricDef>) -> Self {
        Self {
            defs: Mutex::new(defs),
            current: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            history_cap: 120,
        }
    }

    pub fn defs(&self) -> Vec<MetricDef> {
        self.defs.lock().unwrap().clone()
    }

    /// 插件上线时注册指标；同 id 覆盖。
    pub fn add_defs(&self, defs: Vec<MetricDef>) {
        let mut all = self.defs.lock().unwrap();
        for def in defs {
            match all.iter().position(|d| d.id == def.id) {
                Some(pos) => all[pos] = def,
                None => all.push(def),
            }
        }
    }

    /// 插件下线时移除其指标与数据。
    pub fn remove_by_plugin(&self, plugin: &str) {
        let mut ids = Vec::new();
        {
            let mut all = self.defs.lock().unwrap();
            all.retain(|d| {
                let keep = d.plugin.as_deref() != Some(plugin);
                if !keep {
                    ids.push(d.id.clone());
                }
                keep
            });
        }
        let mut cur = self.current.lock().unwrap();
        let mut hist = self.history.lock().unwrap();
        for id in ids {
            cur.remove(&id);
            hist.remove(&id);
        }
    }

    /// 插件重报指标列表（例如外部设备热插拔）时整体替换其定义。
    pub fn replace_for_plugin(&self, plugin: &str, defs: Vec<MetricDef>) {
        let mut orphaned: std::collections::HashSet<String> = std::collections::HashSet::new();
        {
            let mut all = self.defs.lock().unwrap();
            all.retain(|d| {
                let keep = d.plugin.as_deref() != Some(plugin);
                if !keep {
                    orphaned.insert(d.id.clone());
                }
                keep
            });
            let existing: std::collections::HashSet<String> =
                all.iter().map(|d| d.id.clone()).collect();
            for def in defs {
                if existing.contains(&def.id) {
                    continue;
                }
                orphaned.remove(&def.id);
                all.push(def);
            }
        }

        let mut cur = self.current.lock().unwrap();
        let mut hist = self.history.lock().unwrap();
        for id in orphaned {
            cur.remove(&id);
            hist.remove(&id);
        }
    }

    pub fn set(&self, id: &str, sample: Sample) {
        let defs = self.defs.lock().unwrap();
        if let Some(def) = defs.iter().find(|d| d.id == id) {
            if def.history {
                if let Some(n) = sample.v.as_f64() {
                    let mut h = self.history.lock().unwrap();
                    let ring = h.entry(id.to_string()).or_default();
                    ring.push_back(n);
                    while ring.len() > self.history_cap {
                        ring.pop_front();
                    }
                }
            }
        }
        drop(defs);
        self.current.lock().unwrap().insert(id.to_string(), sample);
    }

    /// 失败时保留最后一次的值与时间戳，只挂上错误标记（用于"失效"样式）。
    pub fn set_error(&self, id: &str, msg: &str) {
        let mut cur = self.current.lock().unwrap();
        match cur.get_mut(id) {
            Some(sample) => sample.error = Some(msg.to_string()),
            None => {
                cur.insert(
                    id.to_string(),
                    Sample { ts: 0, v: Value::Null, error: Some(msg.to_string()) },
                );
            }
        }
    }

    pub fn history(&self, id: &str) -> Vec<f64> {
        self.history
            .lock()
            .unwrap()
            .get(id)
            .map(|r| r.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn snapshot(&self) -> HashMap<String, Sample> {
        self.current.lock().unwrap().clone()
    }
}
