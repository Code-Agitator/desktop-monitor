<script setup lang="ts">
// 渲染器总览（仅 dev）：把每个渲染器在各种尺寸、各种状态下的样子排出来，
// 方便一眼看出布局崩坏（例如条形在超薄高度下消失）。
import { ref } from "vue";
import Card from "../Card.vue";
import { RENDERERS, rendererFor } from "../renderers";
import { applyTheme } from "../themes";
import type { MetricDef, Sample } from "../types";

applyTheme(document.documentElement, "default-dark");

const compact = ref(false);
const now = Date.now();

function def(partial: Partial<MetricDef>): MetricDef {
  return {
    id: partial.id ?? "dev.metric",
    title: partial.title ?? "指标",
    kind: partial.kind ?? "scalar",
    unit: partial.unit ?? null,
    intervalMs: 1000,
    history: false,
    min: partial.min ?? null,
    max: partial.max ?? null,
  };
}

const ratioSamples = (a = 0.42, b = 0.8, c = 0.95) => ({ ok: a, warn: b, bad: c });
const scalarSamples = (a: number, b: number, c: number) => ({ ok: a, warn: b, bad: c });

interface Item {
  key: string;
  view: string;
  def: MetricDef;
  samples: Record<"ok" | "warn" | "bad", unknown>;
  options?: Record<string, unknown>;
}

const spark = (seed: number) =>
  Array.from({ length: 48 }, (_, i) => 20 + Math.sin((i + seed) / 5) * 12 + ((i * seed) % 7));

const items: Item[] = [
  {
    key: "number-ratio",
    view: "number",
    def: def({ id: "dev.cpu", title: "CPU 使用率", kind: "ratio" }),
    samples: ratioSamples(),
  },
  {
    key: "number-bytes",
    view: "number",
    def: def({ id: "dev.net", title: "网络下行", kind: "bytes", unit: "B/s" }),
    samples: { ok: 1_520_000, warn: 78_000_000, bad: 240_000_000 },
  },
  {
    key: "number-scalar",
    view: "number",
    def: def({ id: "dev.temp", title: "CPU 温度", kind: "scalar", unit: "°C" }),
    samples: scalarSamples(42, 72, 95),
    options: { thresholds: { warn: 70, bad: 90 } },
  },
  {
    key: "ring",
    view: "ring",
    def: def({ id: "dev.mem", title: "内存", kind: "ratio" }),
    samples: ratioSamples(),
  },
  {
    key: "battery",
    view: "battery",
    def: def({ id: "dev.batt", title: "鼠标电量", kind: "ratio" }),
    samples: { ok: 0.82, warn: 0.22, bad: 0.06 },
  },
  {
    key: "bar",
    view: "bar",
    def: def({ id: "dev.disk", title: "磁盘占用", kind: "ratio" }),
    samples: ratioSamples(),
  },
  {
    key: "sparkline",
    view: "sparkline",
    def: def({ id: "dev.spark", title: "网络速度", kind: "series", unit: "KB/s" }),
    samples: { ok: spark(3), warn: spark(7), bad: spark(11) },
  },
  {
    key: "status",
    view: "status",
    def: def({ id: "dev.state", title: "服务状态", kind: "state" }),
    samples: { ok: "运行中", warn: "负载偏高", bad: "已断开" },
  },
  {
    key: "list",
    view: "list",
    def: def({ id: "dev.list", title: "磁盘", kind: "list" }),
    samples: {
      ok: [
        { label: "C:", value: "62%", ratio: 0.62 },
        { label: "D:", value: "34%", ratio: 0.34 },
      ],
      warn: [
        { label: "C:", value: "81%", ratio: 0.81 },
        { label: "D:", value: "44%", ratio: 0.44 },
      ],
      bad: [
        { label: "C:", value: "96%", ratio: 0.96 },
        { label: "D:", value: "51%", ratio: 0.51 },
      ],
    },
  },
  {
    key: "text",
    view: "text",
    def: def({ id: "dev.uptime", title: "运行时长", kind: "duration" }),
    samples: { ok: 3725, warn: 36000, bad: 90061 },
  },
];

const sizesOf = (item: Item) => {
  const r = rendererFor(item.view, item.def.kind);
  return [
    { label: "超薄", w: 220, h: 32 },
    { label: "小", w: 120, h: 48 },
    { label: "默认", w: r.defaultSize.w, h: r.defaultSize.h },
    { label: "大", w: 300, h: 160 },
  ];
};

const defaultSizeOf = (item: Item) => rendererFor(item.view, item.def.kind).defaultSize;

function cardFor(item: Item, w: number, h: number) {
  return {
    id: `${item.key}-${w}x${h}`,
    metric: item.def.id,
    view: item.view,
    x: 0,
    y: 0,
    w,
    h,
    options: item.options ?? null,
  };
}

function sampleOf(item: Item, state: "ok" | "warn" | "bad" | "empty"): Sample {
  return {
    ts: now,
    v: state === "empty" ? null : item.samples[state],
  } as Sample;
}

/** 每个渲染器接受的指标类型，方便确认「为什么这个指标选不到它」 */
const acceptsOf = (view: string) => RENDERERS.find((r) => r.id === view)?.accepts.join(" / ") ?? "";
</script>

<template>
  <div class="dev">
    <header class="head">
      <h1>渲染器总览</h1>
      <label class="toggle">
        <input v-model="compact" type="checkbox" />
        紧凑模式（HUD）
      </label>
      <span class="hint">
        每种渲染器：一排是尺寸（超薄 / 小 / 默认 / 大），一排是状态（正常 / 警告 / 危险 / 无数据）
      </span>
    </header>

    <section v-for="item in items" :key="item.key" class="block">
      <div class="block-head">
        <b>{{ item.view }}</b>
        <span class="tag">{{ item.key }}</span>
        <span class="dim">kind = {{ item.def.kind }}</span>
        <span class="dim">accepts = {{ acceptsOf(item.view) }}</span>
      </div>

      <div class="row">
        <div v-for="s in sizesOf(item)" :key="s.label" class="cell">
          <div class="cap">{{ s.label }} · {{ s.w }}×{{ s.h }}</div>
          <div class="slot" :style="{ width: s.w + 'px', height: s.h + 'px' }">
            <Card
              :card="cardFor(item, s.w, s.h)"
              :def="item.def"
              :sample="sampleOf(item, 'ok')"
              :now="now"
              :compact="compact"
            />
          </div>
        </div>
      </div>

      <div class="row">
        <div v-for="st in ['ok', 'warn', 'bad', 'empty'] as const" :key="st" class="cell">
          <div class="cap">{{ st }}</div>
          <div
            class="slot"
            :style="{ width: defaultSizeOf(item).w + 'px', height: defaultSizeOf(item).h + 'px' }"
          >
            <Card
              :card="cardFor(item, defaultSizeOf(item).w, defaultSizeOf(item).h)"
              :def="item.def"
              :sample="sampleOf(item, st)"
              :now="now"
              :compact="compact"
            />
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.dev {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  padding: 20px 24px 64px;
  box-sizing: border-box;
  background: #121316;
  color: #e8e8ea;
  font-family: "Segoe UI Variable Text", "Microsoft YaHei UI", system-ui, sans-serif;
}

.head {
  display: flex;
  align-items: baseline;
  gap: 14px;
  margin-bottom: 18px;
  flex-wrap: wrap;
}

h1 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
}

.toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: #9a9aa6;
  cursor: pointer;
}

.hint {
  font-size: 12px;
  color: #7d7d8a;
}

.block {
  margin-bottom: 26px;
  padding: 14px 16px 16px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.02);
}

.block-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
  font-size: 12px;
}

.block-head b {
  font-size: 13px;
  color: #bfe6ff;
}

.tag {
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(90, 200, 255, 0.14);
  color: #9fd8ff;
  font-size: 11px;
}

.dim {
  color: #7d7d8a;
  font-size: 11px;
}

.row {
  display: flex;
  align-items: flex-start;
  gap: 18px;
  flex-wrap: wrap;
  margin-bottom: 10px;
}

.cell {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.cap {
  font-size: 10px;
  color: #6f6f7c;
  letter-spacing: 0.04em;
}

.slot {
  position: relative;
  outline: 1px dashed rgba(255, 255, 255, 0.1);
  outline-offset: 0;
}
</style>
