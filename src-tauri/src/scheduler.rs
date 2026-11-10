//! 统一调度器：每个指标按自己的 interval 到期拉取，结果合批推给前端。

use crate::metrics::{MetricStore, Sample};
use crate::plugins::PluginManager;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn start(app: AppHandle, store: Arc<MetricStore>, plugins: Arc<PluginManager>) {
    std::thread::spawn(move || {
        let mut last_poll: HashMap<String, Instant> = HashMap::new();
        let mut dirty = true;
        let mut last_emit = Instant::now() - Duration::from_secs(1);

        loop {
            let now = Instant::now();
            let defs = store.defs().to_vec();

            let mut by_plugin: HashMap<String, Vec<String>> = HashMap::new();

            for def in &defs {
                let due = last_poll
                    .get(&def.id)
                    .map(|t| now.duration_since(*t).as_millis() as u64 >= def.interval_ms)
                    .unwrap_or(true);
                if !due {
                    continue;
                }
                last_poll.insert(def.id.clone(), now);
                match &def.plugin {
                    Some(plugin) => by_plugin
                        .entry(plugin.clone())
                        .or_default()
                        .push(def.id.clone()),
                    None => {
                        let sample = crate::source::poll(def, &store);
                        store.set(&def.id, sample);
                        dirty = true;
                    }
                }
            }

            for (plugin, ids) in by_plugin {
                match plugins.poll(&plugin, &ids) {
                    Ok(values) => {
                        for id in ids {
                            match values.get(&id) {
                                Some(Value::Object(obj)) if obj.contains_key("error") => {
                                    let msg = obj
                                        .get("error")
                                        .and_then(|e| e.as_str())
                                        .unwrap_or("插件返回错误");
                                    store.set_error(&id, msg);
                                }
                                Some(v) => store.set(
                                    &id,
                                    Sample { ts: now_ms(), v: v.clone(), error: None },
                                ),
                                None => {}
                            }
                        }
                        dirty = true;
                    }
                    Err(e) => {
                        for id in ids {
                            store.set_error(&id, &e);
                        }
                        dirty = true;
                    }
                }
            }

            if dirty && last_emit.elapsed() >= Duration::from_millis(200) {
                let payload = serde_json::json!({
                    "ts": now_ms(),
                    "values": store.snapshot(),
                });
                let _ = app.emit("metrics", payload);
                dirty = false;
                last_emit = Instant::now();
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    });
}
