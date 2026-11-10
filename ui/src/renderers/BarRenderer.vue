<script setup lang="ts">
import { computed } from "vue";
import { formatValue, levelOf, normalized } from "../format";

const props = defineProps<{ def?: any; sample?: any; options: Record<string, any> }>();

const f = computed(() => formatValue(props.def, props.sample?.v, props.options));
const level = computed(() => levelOf(props.def, props.sample?.v, props.options));
const width = computed(() => (normalized(props.def, props.sample?.v) * 100).toFixed(1) + "%");
const label = computed(() => props.options?.label || props.def?.title || "");
</script>

<template>
  <div class="bar-wrap">
    <div class="row">
      <span class="label">{{ label }}</span>
      <span class="value" :class="level">
        {{ f.text }}<span v-if="f.unit" class="unit">{{ f.unit }}</span>
      </span>
    </div>
    <div class="bar">
      <div class="fill" :class="level" :style="{ width }" />
    </div>
  </div>
</template>

<style scoped>
.bar-wrap {
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 10px;
}
.row {
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
  font-size: 20px;
  font-weight: 600;
  line-height: 1;
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.value.warn {
  color: var(--warn);
}
.value.bad {
  color: var(--bad);
}
.unit {
  font-size: 0.55em;
  font-weight: 500;
  color: var(--text-dim);
  letter-spacing: 0.02em;
  margin-left: 0.28em;
}
.bar {
  height: 4px;
  border-radius: 999px;
  background: var(--track);
  overflow: hidden;
}
.fill {
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--violet), var(--accent));
  transition: width 0.45s ease;
}
.fill.warn {
  background: linear-gradient(90deg, var(--amber), var(--warn));
}
.fill.bad {
  background: linear-gradient(90deg, var(--bad-start), var(--bad));
}
</style>
