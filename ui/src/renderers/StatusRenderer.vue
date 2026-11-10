<script setup lang="ts">
import { computed } from "vue";
import { levelOf } from "../format";

const props = defineProps<{ def?: any; sample?: any; options: Record<string, any> }>();

const level = computed(() => levelOf(props.def, props.sample?.v, props.options));
const text = computed(() => (props.sample?.v == null ? "--" : String(props.sample.v)));
const label = computed(() => props.options?.label || "");
</script>

<template>
  <div class="status">
    <span class="dot" :class="level" />
    <div class="text">
      <div v-if="label" class="label">{{ label }}</div>
      <div class="value">{{ text }}</div>
    </div>
  </div>
</template>

<style scoped>
.status {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--ok);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 14%, transparent);
  flex: none;
}
.dot.warn {
  background: var(--warn);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--warn) 14%, transparent);
}
.dot.bad {
  background: var(--bad);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--bad) 14%, transparent);
}
.text {
  min-width: 0;
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.label {
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--text-dim);
}
.value {
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
