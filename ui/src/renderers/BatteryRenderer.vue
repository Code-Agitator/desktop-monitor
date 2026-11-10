<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{ def?: any; sample?: any; options: Record<string, any> }>();

const ratio = computed(() => {
  const n = Number(props.sample?.v);
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(1, n));
});
const pct = computed(() => (ratio.value == null ? 0 : Math.round(ratio.value * 100)));
const display = computed(() => (ratio.value == null ? "--" : `${pct.value}%`));
const level = computed(() => {
  if (ratio.value == null) return "";
  if (ratio.value < 0.1) return "bad";
  if (ratio.value < 0.25) return "warn";
  return "";
});
const label = computed(() => props.options?.label || props.def?.title || "");
</script>

<template>
  <div class="battery">
    <div class="body">
      <div class="fill" :class="level" :style="{ width: pct + '%' }" />
      <div class="cap" />
    </div>
    <div class="meta">
      <div class="value" :class="level">{{ display }}</div>
      <div class="label">{{ label }}</div>
    </div>
  </div>
</template>

<style scoped>
.battery {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
}
.body {
  position: relative;
  flex: 0 0 46%;
  height: 20px;
  border: 2px solid var(--text-dim);
  border-radius: 5px;
  box-sizing: border-box;
  overflow: visible;
}
.fill {
  position: absolute;
  left: 2px;
  top: 2px;
  bottom: 2px;
  border-radius: 2px;
  background: linear-gradient(90deg, var(--violet), var(--accent));
  transition: width 0.45s ease, background 0.3s;
}
.fill.warn { background: linear-gradient(90deg, var(--amber), var(--warn)); }
.fill.bad { background: linear-gradient(90deg, var(--bad-start), var(--bad)); }
.cap {
  position: absolute;
  right: -6px;
  top: 50%;
  width: 4px;
  height: 8px;
  transform: translateY(-50%);
  border-radius: 0 2px 2px 0;
  background: var(--text-dim);
}
.meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.value {
  font-size: 22px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  line-height: 1.1;
}
.value.warn { color: var(--warn); }
.value.bad { color: var(--bad); }
.label {
  font-size: 12px;
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
