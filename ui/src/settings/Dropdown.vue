<script setup lang="ts">
// 自绘下拉：原生 <select> 的弹层是 WebView2 自己画的，会跟随系统浅色主题
// （白底、直角），和设置页的暗色 UI 完全不搭，所以这里自己实现一个。
import { computed, nextTick, onUnmounted, ref, watch } from "vue";

interface Option {
  value: string;
  label: string;
  /** 分组标题：相邻同组只在第一项前显示一次 */
  group?: string;
  hint?: string;
}

const props = defineProps<{
  modelValue: string;
  options: Option[];
  placeholder?: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

const root = ref<HTMLElement | null>(null);
const popup = ref<HTMLElement | null>(null);
const open = ref(false);
const active = ref(-1);
const pos = ref({ left: 0, top: 0, width: 0 });

const current = computed(() => props.options.find((o) => o.value === props.modelValue));

/** 只算位置，不碰滚动/高亮——滚动时反复调用它不能把列表"拽回"选中项 */
function position() {
  const r = root.value?.getBoundingClientRect();
  if (!r) return;
  const width = Math.max(r.width, 150);
  let top = r.bottom + 6;
  const h = popup.value?.offsetHeight ?? 0;
  // 下方放不下就往上翻
  if (h && top + h > window.innerHeight - 8) top = Math.max(8, r.top - 6 - h);
  pos.value = { left: r.left, top, width };
}

async function show() {
  if (props.disabled) return;
  open.value = true;
  active.value = Math.max(
    0,
    props.options.findIndex((o) => o.value === props.modelValue),
  );
  await nextTick();
  position();
  scrollActiveIntoView();
}

function hide() {
  open.value = false;
}

function toggle() {
  open.value ? hide() : show();
}

function pick(o: Option) {
  emit("update:modelValue", o.value);
  hide();
}

function scrollActiveIntoView() {
  nextTick(() => {
    const el = popup.value?.querySelector<HTMLElement>(".dd-item.active");
    el?.scrollIntoView({ block: "nearest" });
  });
}

function onKey(e: KeyboardEvent) {
  if (props.disabled) return;
  if (!open.value) {
    if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      show();
    }
    return;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    hide();
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    active.value = Math.min(props.options.length - 1, active.value + 1);
    scrollActiveIntoView();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    active.value = Math.max(0, active.value - 1);
    scrollActiveIntoView();
  } else if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    const o = props.options[active.value];
    if (o) pick(o);
  }
}

function onDocPointer(e: PointerEvent) {
  const t = e.target as Node;
  if (root.value?.contains(t) || popup.value?.contains(t)) return;
  hide();
}

function onScrollOrResize(e: Event) {
  if (!open.value) return;
  // 弹层内部滚动时不要重新定位（否则每滚一下都被拉回去）
  const t = e.target;
  if (popup.value && t instanceof Node && popup.value.contains(t)) return;
  position();
}

watch(open, (v) => {
  if (v) {
    window.addEventListener("pointerdown", onDocPointer, true);
    window.addEventListener("resize", onScrollOrResize);
    window.addEventListener("scroll", onScrollOrResize, true);
  } else {
    window.removeEventListener("pointerdown", onDocPointer, true);
    window.removeEventListener("resize", onScrollOrResize);
    window.removeEventListener("scroll", onScrollOrResize, true);
  }
});

onUnmounted(() => {
  window.removeEventListener("pointerdown", onDocPointer, true);
  window.removeEventListener("resize", onScrollOrResize);
  window.removeEventListener("scroll", onScrollOrResize, true);
});
</script>

<template>
  <div
    ref="root"
    class="dd"
    :class="{ open, disabled }"
    tabindex="0"
    @click="toggle"
    @keydown="onKey"
  >
    <span v-if="current" class="dd-value">{{ current.label }}</span>
    <span v-else class="dd-placeholder">{{ placeholder ?? "请选择" }}</span>
    <span class="dd-caret" aria-hidden="true" />
  </div>

  <Teleport to="body">
    <div
      v-if="open"
      ref="popup"
      class="dd-popup"
      :style="{ left: pos.left + 'px', top: pos.top + 'px', minWidth: pos.width + 'px' }"
    >
      <template v-for="(o, i) in options" :key="o.value">
        <div
          v-if="o.group && (i === 0 || options[i - 1].group !== o.group)"
          class="dd-group"
        >
          {{ o.group }}
        </div>
        <div
          class="dd-item"
          :class="{ active: i === active, selected: o.value === modelValue }"
          @click.stop="pick(o)"
          @pointermove="active = i"
        >
          <span class="dd-item-label">{{ o.label }}</span>
          <span v-if="o.value === modelValue" class="dd-tick">✓</span>
        </div>
      </template>
      <div v-if="!options.length" class="dd-empty">没有可选项</div>
    </div>
  </Teleport>
</template>

<style scoped>
.dd {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 32px;
  box-sizing: border-box;
  padding: 6px 30px 6px 10px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: rgba(255, 255, 255, 0.06);
  color: var(--text);
  font-size: 12px;
  line-height: 1.3;
  cursor: default;
  user-select: none;
  outline: none;
  transition: border-color 0.12s ease, background 0.12s ease;
}

.dd:hover {
  background: rgba(255, 255, 255, 0.085);
}

.dd.open,
.dd:focus-visible {
  border-color: rgba(90, 200, 255, 0.55);
  box-shadow: 0 0 0 3px rgba(90, 200, 255, 0.12);
}

.dd.disabled {
  opacity: 0.5;
}

.dd-value {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dd-placeholder {
  color: var(--text-dim);
}

.dd-caret {
  position: absolute;
  right: 10px;
  width: 7px;
  height: 7px;
  margin-top: -3px;
  border-right: 1.5px solid var(--text-dim);
  border-bottom: 1.5px solid var(--text-dim);
  transform: rotate(45deg);
  transition: transform 0.14s ease;
}

.dd.open .dd-caret {
  transform: rotate(-135deg);
  margin-top: 3px;
}
</style>

<style>
/* 弹层挂在 body 上，不能用 scoped */
.dd-popup {
  position: fixed;
  z-index: 999;
  max-height: 300px;
  overflow-y: auto;
  padding: 5px;
  border-radius: 10px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: #1f1f26;
  box-shadow:
    0 18px 40px rgba(0, 0, 0, 0.55),
    inset 0 1px 0 rgba(255, 255, 255, 0.06);
  color: var(--text, #e8e8ea);
  font-size: 12px;
  animation: dd-in 0.12s ease-out;
}

@keyframes dd-in {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.dd-popup .dd-group {
  padding: 7px 8px 4px;
  font-size: 10px;
  font-weight: 500;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #7d7d8a;
}

.dd-popup .dd-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 6px 8px;
  border-radius: 6px;
  white-space: nowrap;
  cursor: default;
}

.dd-popup .dd-item.active {
  background: rgba(255, 255, 255, 0.08);
}

.dd-popup .dd-item.selected {
  color: #9fd8ff;
}

.dd-popup .dd-item-label {
  overflow: hidden;
  text-overflow: ellipsis;
}

.dd-popup .dd-tick {
  font-size: 11px;
  color: #6ec1ff;
}

.dd-popup .dd-empty {
  padding: 8px;
  color: #7d7d8a;
}

.dd-popup::-webkit-scrollbar {
  width: 8px;
}
.dd-popup::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.14);
  border-radius: 4px;
}
.dd-popup::-webkit-scrollbar-track {
  background: transparent;
}
</style>
