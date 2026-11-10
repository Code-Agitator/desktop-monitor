<script setup lang="ts">
// 任务栏指标条：一行紧凑文字，叠在任务栏空白处（窗口本身点击穿透）。
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { applyTheme } from "./themes";
import type { MetricDef, Sample } from "./types";

interface TaskbarConfig {
  enabled: boolean;
  width: number;
  items: string[];
}

const cfg = ref<TaskbarConfig>({ enabled: true, width: 248, items: [] });
const metrics = ref<MetricDef[]>([]);
const values = ref<Record<string, Sample>>({});
const unlisten: UnlistenFn[] = [];

/** 任务栏空间有限，标签用最短写法 */
const SHORT_LABEL: Record<string, string> = {
  "sys.cpu.usage": "CPU",
  "sys.mem.usage": "内存",
  "sys.net.up": "↑",
  "sys.net.down": "↓",
  "sys.gpu.usage": "GPU",
  "sys.cpu.temp": "CPU°",
  "sys.gpu.temp": "GPU°",
};

function defOf(id: string): MetricDef | undefined {
  return metrics.value.find((m) => m.id === id);
}

/** 字节压缩成 1.5M 这种，省宽度 */
function bytesCompact(n: number): string {
  const units = ["B", "K", "M", "G", "T"];
  let x = Math.abs(n);
  let i = 0;
  while (x >= 1024 && i < units.length - 1) {
    x /= 1024;
    i++;
  }
  const text = i === 0 ? String(Math.round(x)) : x >= 100 ? x.toFixed(0) : x.toFixed(1);
  return `${text}${units[i]}`;
}

function format(def: MetricDef | undefined, v: unknown): string {
  if (v == null) return "--";
  if (def?.kind === "ratio" && typeof v === "number") return `${Math.round(v * 100)}%`;
  if (def?.kind === "bytes" && typeof v === "number") {
    return bytesCompact(v) + (def.unit === "B/s" ? "/s" : "");
  }
  if (typeof v === "number") return `${Math.round(v)}${def?.unit ?? ""}`;
  return String(v);
}

/** 拿不到配置时的兜底项，保证这条永远有内容 */
const DEFAULT_ITEMS = ["sys.cpu.usage", "sys.mem.usage", "sys.net.up", "sys.net.down"];
const items = computed(() => (cfg.value.items.length ? cfg.value.items : DEFAULT_ITEMS));

const cells = computed(() =>
  items.value.map((id) => {
    const def = defOf(id);
    return {
      id,
      label: SHORT_LABEL[id] ?? def?.title ?? id,
      text: format(def, values.value[id]?.v),
    };
  }),
);

onMounted(async () => {
  applyTheme(document.documentElement, "default-dark");
  // 先取配置（不依赖事件），再挂监听：顺序反了会互相拖住
  try {
    const boot = await invoke<{ config: TaskbarConfig; metrics: MetricDef[] }>(
      "get_taskbar_config",
    );
    cfg.value = boot.config;
    metrics.value = boot.metrics;
  } catch (e) {
    console.error("taskbar config", e);
  }
  try {
    unlisten.push(
      await listen<{ ts: number; values: Record<string, Sample> }>("metrics", (e) => {
        values.value = { ...values.value, ...e.payload.values };
      }),
    );
    unlisten.push(
      await listen<MetricDef[]>("metric-defs", (e) => {
        metrics.value = e.payload;
      }),
    );
  } catch {
    // 浏览器预览时没有 Tauri 运行时
  }
});

onUnmounted(() => unlisten.forEach((f) => f()));
</script>

<template>
  <div class="strip">
    <template v-for="(c, i) in cells" :key="c.id">
      <span v-if="i > 0" class="sep" />
      <span class="cell">
        <span class="label">{{ c.label }}</span>
        <span class="value">{{ c.text }}</span>
      </span>
    </template>
  </div>
</template>

<style scoped>
.strip {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: 0 2px;
  font-family: "Segoe UI Variable Text", "Microsoft YaHei UI", system-ui, sans-serif;
  /* 任务栏自带亚克力背景，文字用中性灰保持低调 */
  color: #d8d8de;
  white-space: nowrap;
  overflow: hidden;
}

.cell {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
}

.label {
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.04em;
  color: #9a9aa6;
}

.value {
  font-size: 12px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.01em;
}

.sep {
  width: 1px;
  height: 11px;
  background: rgba(255, 255, 255, 0.14);
}
</style>
