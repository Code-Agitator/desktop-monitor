<script setup lang="ts">
import { computed } from "vue";
import { levelOf, normalized, numberOf } from "../format";

const props = defineProps<{ def?: any; sample?: any; options: Record<string, any> }>();

const R = 42;
const C = 2 * Math.PI * R;

const ratio = computed(() => normalized(props.def, props.sample?.v));
const level = computed(() => levelOf(props.def, props.sample?.v, props.options));
const dash = computed(() => `${(C * ratio.value).toFixed(2)} ${C.toFixed(2)}`);
const label = computed(() => props.options?.label || props.def?.title || "");
const display = computed(() => {
  const n = numberOf(props.sample?.v);
  if (n == null) return "--";
  if (props.def?.kind === "ratio") return Math.round(n * 100) + "%";
  return Math.round(n) + (props.def?.unit ?? "");
});
// 每个实例用独立渐变 id，避免多个环形共用第一个渐变
const gradId = `ring-${Math.random().toString(36).slice(2, 8)}`;
</script>

<template>
  <div class="ring">
    <svg viewBox="0 0 100 100">
      <defs>
        <linearGradient :id="gradId" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stop-color="var(--violet)" />
          <stop offset="100%" stop-color="var(--accent)" />
        </linearGradient>
      </defs>
      <circle class="track" cx="50" cy="50" :r="R" />
      <circle
        class="bar"
        :class="level"
        cx="50"
        cy="50"
        :r="R"
        :stroke-dasharray="dash"
        :style="{ stroke: level === 'ok' ? `url(#${gradId})` : undefined }"
      />
    </svg>
    <div class="center">
      <div class="v">{{ display }}</div>
      <div class="l">{{ label }}</div>
    </div>
  </div>
</template>

<style scoped>
.ring {
  height: 100%;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}
svg {
  width: 100%;
  height: 100%;
  transform: rotate(-90deg);
}
circle {
  fill: none;
  stroke-width: 6.5;
}
.track {
  stroke: var(--track);
}
.bar {
  stroke-linecap: round;
  transition: stroke-dasharray 0.45s ease, stroke 0.3s;
}
.bar.warn {
  stroke: var(--warn);
}
.bar.bad {
  stroke: var(--bad);
}
.center {
  position: absolute;
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
}
.v {
  font-size: 26px;
  font-weight: 600;
  line-height: 1;
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
}
.l {
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--text-dim);
}
</style>
