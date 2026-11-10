<script setup lang="ts">
import { computed } from "vue";
import { formatBytes } from "../format";

const props = defineProps<{ def?: any; sample?: any; options: Record<string, any> }>();

const W = 100;
const H = 28;

const series = computed<number[]>(() =>
  Array.isArray(props.sample?.v) ? (props.sample.v as number[]) : [],
);

const points = computed(() => {
  const arr = series.value;
  if (arr.length < 2) return "";
  const min = Math.min(...arr);
  const max = Math.max(...arr);
  const span = max - min || 1;
  return arr
    .map((v, i) => `${((i / (arr.length - 1)) * W).toFixed(2)},${(H - ((v - min) / span) * H).toFixed(2)}`)
    .join(" ");
});

// 折线下方补一条闭合路径做渐隐填充，比纯线条更有层次
const area = computed(() => {
  const p = points.value;
  if (!p) return "";
  return `M0,${H} L${p.split(" ").join(" L")} L${W},${H} Z`;
});

const lastText = computed(() => {
  const arr = series.value;
  if (!arr.length) return "--";
  const v = arr[arr.length - 1];
  const unit = props.options?.unit ?? props.def?.unit ?? "";
  if (unit === "B/s") return formatBytes(v) + "/s";
  if (unit === "B") return formatBytes(v);
  return Math.round(v * 100) + "%";
});
const gradId = `spark-${Math.random().toString(36).slice(2, 8)}`;
</script>

<template>
  <div class="spark">
    <div class="head">
      <span class="label">{{ options?.label || def?.title }}</span>
      <span class="value">{{ lastText }}</span>
    </div>
    <svg :viewBox="`0 0 ${W} ${H}`" preserveAspectRatio="none">
      <defs>
        <linearGradient :id="gradId" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="var(--accent)" stop-opacity="0.22" />
          <stop offset="100%" stop-color="var(--accent)" stop-opacity="0" />
        </linearGradient>
      </defs>
      <path v-if="area" :d="area" :fill="`url(#${gradId})`" />
      <polyline v-if="points" :points="points" class="line" />
    </svg>
  </div>
</template>

<style scoped>
.spark {
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 8px;
}
.head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
}
.label {
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.value {
  font-size: 16px;
  font-weight: 600;
  line-height: 1;
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
  color: var(--accent);
  white-space: nowrap;
}
svg {
  width: 100%;
  height: 34px;
  overflow: visible;
}
.line {
  fill: none;
  stroke: var(--accent);
  stroke-width: 1.5;
  vector-effect: non-scaling-stroke;
  stroke-linejoin: round;
  stroke-linecap: round;
}
</style>
