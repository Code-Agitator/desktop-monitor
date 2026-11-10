//! 宿主侧派生指标：时钟，以及由历史缓冲生成的序列。
//! 真实系统指标由 sys-collector 插件提供（plugins/builtin/sys-collector）。

use crate::metrics::{MetricDef, MetricKind, MetricStore, Sample};
use crate::scheduler::now_ms;
use serde_json::{json, Value};

fn def(id: &str, title: &str, kind: MetricKind, interval_ms: u64) -> MetricDef {
    MetricDef {
        id: id.into(),
        title: title.into(),
        kind,
        unit: None,
        interval_ms,
        history: false,
        min: None,
        max: None,
        plugin: None,
    }
}

pub fn definitions() -> Vec<MetricDef> {
    vec![
        def("sys.clock", "时间", MetricKind::State, 1000),
        def("sys.cpu.hist", "CPU 历史", MetricKind::Series, 1000),
        def("sys.net.hist", "网络历史", MetricKind::Series, 1000),
    ]
}

pub fn poll(def: &MetricDef, store: &MetricStore) -> Sample {
    let v: Value = match def.id.as_str() {
        "sys.clock" => json!(chrono::Local::now().format("%H:%M:%S").to_string()),
        "sys.cpu.hist" => json!(store.history("sys.cpu.usage")),
        "sys.net.hist" => json!(store.history("sys.net.down")),
        _ => Value::Null,
    };
    Sample { ts: now_ms(), v, error: None }
}
