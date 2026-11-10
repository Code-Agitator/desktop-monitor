<script setup lang="ts">
import { computed } from "vue";
import { formatValue } from "../format";

const props = defineProps<{ def?: any; sample?: any; options: Record<string, any> }>();

const text = computed(() => {
  const f = formatValue(props.def, props.sample?.v, props.options);
  return f.unit ? `${f.text} ${f.unit}` : f.text;
});
const align = computed(() => {
  const a = props.options?.align;
  return a === "left" || a === "right" ? a : "center";
});
</script>

<template>
  <div class="text" :style="{ textAlign: align }">
    <div v-if="options?.label" class="label">{{ options.label }}</div>
    <div class="value">{{ text }}</div>
  </div>
</template>

<style scoped>
.text {
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 6px;
}
.label {
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--text-dim);
}
.value {
  font-size: 34px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.01em;
  line-height: 1.05;
}
</style>
