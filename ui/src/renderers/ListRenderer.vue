<script setup lang="ts">
import { computed } from "vue";
import type { ListRow } from "../types";

const props = defineProps<{ def?: any; sample?: any; options: Record<string, any> }>();

const rows = computed<ListRow[]>(() =>
  Array.isArray(props.sample?.v) ? (props.sample.v as ListRow[]) : [],
);
const label = computed(() => props.options?.label || props.def?.title || "");
</script>

<template>
  <div class="list">
    <div v-if="label" class="label">{{ label }}</div>
    <div v-for="row in rows" :key="row.label" class="row">
      <div class="line">
        <span class="k">{{ row.label }}</span>
        <span class="v">{{ row.value }}</span>
      </div>
      <div v-if="row.ratio != null" class="bar">
        <div class="fill" :style="{ width: Math.min(100, row.ratio * 100) + '%' }" />
      </div>
    </div>
    <div v-if="!rows.length" class="empty">--</div>
  </div>
</template>

<style scoped>
.list {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow: hidden;
}
.label {
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--text-dim);
}
.row {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.list .row + .row {
  padding-top: 9px;
  border-top: 1px solid var(--card-border);
}
.line {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}
.k {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-strong);
  flex: none;
}
.v {
  font-size: 12px;
  color: var(--text-dim);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.bar {
  height: 3px;
  border-radius: 999px;
  background: var(--track);
  overflow: hidden;
}
.fill {
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--violet), var(--accent));
  transition: width 0.4s ease;
}
.empty {
  color: var(--text-dim);
  font-size: 12px;
}
</style>
