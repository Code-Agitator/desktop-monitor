<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import Stage from "./Stage.vue";
import { applyTheme } from "./themes";
import { rendererFor } from "./renderers";
import { buildSnapTargets, snapRect, type Guide, type Rect } from "./snap";
import type { Bootstrap, MetricDef, PanelConfig, Sample } from "./types";

const config = ref<PanelConfig | null>(null);
const metrics = ref<MetricDef[]>([]);
const values = ref<Record<string, Sample>>({});
const now = ref(Date.now());
const error = ref<string | null>(null);

/** 桌面编辑模式：直接在桌面上拖卡片改位置/大小 */
const editing = ref(false);
const selectedId = ref<string | null>(null);
const guides = ref<Guide[]>([]);

const unlisten: UnlistenFn[] = [];

onMounted(async () => {
  // 先挂监听再取初始数据，避免插件在两者之间注册指标时漏掉 metric-defs
  try {
    unlisten.push(
      await listen<{ ts: number; values: Record<string, Sample> }>("metrics", (e) => {
        values.value = { ...values.value, ...e.payload.values };
      }),
    );
    unlisten.push(
      await listen<PanelConfig>("panel-config", (e) => {
        config.value = e.payload;
      }),
    );
    unlisten.push(
      await listen<MetricDef[]>("metric-defs", (e) => {
        metrics.value = e.payload;
      }),
    );
    unlisten.push(
      await listen<{ enabled: boolean }>("panel-edit", (e) => {
        editing.value = !!e.payload?.enabled;
        if (!editing.value) {
          selectedId.value = null;
          guides.value = [];
        }
      }),
    );
  } catch {
    // 浏览器预览时没有 Tauri 运行时
  }

  try {
    const boot = await invoke<Bootstrap>("get_bootstrap");
    config.value = boot.panel;
    metrics.value = boot.metrics;
    applyTheme(document.documentElement, boot.panel?.theme);
    editing.value = await invoke<boolean>("is_panel_edit").catch(() => false);
  } catch (e) {
    error.value = String(e);
  }

  window.addEventListener("pointerdown", onPointerDown);
  window.addEventListener("keydown", onKey);
});

watch(
  () => config.value?.theme,
  (theme) => applyTheme(document.documentElement, theme),
);

watch(config, (c) => {
  if (c) applyTheme(document.documentElement, c.theme);
});

/** 卡片背景不透明度：唯一的视觉旋钮 */
watch(
  () => config.value?.opacity,
  (v) => document.documentElement.style.setProperty("--card-alpha", String(v ?? 1)),
  { immediate: true },
);

const nowTimer = window.setInterval(() => {
  now.value = Date.now();
}, 1000);

// 事件偶尔收不到（面板窗口特殊），这里再兜一层轮询
const editPoll = window.setInterval(() => {
  invoke<boolean>("is_panel_edit")
    .then((on) => {
      if (on === editing.value) return;
      editing.value = on;
      if (!on) {
        selectedId.value = null;
        guides.value = [];
      }
    })
    .catch(() => {});
}, 1200);

onUnmounted(() => {
  unlisten.forEach((f) => f());
  clearInterval(nowTimer);
  clearInterval(editPoll);
  window.removeEventListener("pointerdown", onPointerDown);
  window.removeEventListener("keydown", onKey);
});

function defOf(id: string): MetricDef | undefined {
  return metrics.value.find((m) => m.id === id);
}

// ---------- 桌面编辑 ----------

function cardRect(c: any): Rect {
  const r = rendererFor(c.view, defOf(c.metric)?.kind);
  return { x: c.x ?? 0, y: c.y ?? 0, w: c.w ?? r.defaultSize.w, h: c.h ?? r.defaultSize.h };
}

function allRects(): Rect[] {
  return (config.value?.cards ?? []).map(cardRect);
}

/** 屏幕像素 → 内容像素的换算：面板铺满主屏，没缩放 */
function scaleFactor(): number {
  return window.devicePixelRatio || 1;
}

function saveLater() {
  const cfg = config.value;
  if (!cfg) return;
  // 拖动结束才落盘，避免每帧写文件；落盘前把几何取整（融合块圆角依赖整数边界）
  const snapshot = JSON.parse(JSON.stringify(cfg));
  for (const c of snapshot.cards) {
    if (typeof c.x === "number") c.x = Math.round(c.x);
    if (typeof c.y === "number") c.y = Math.round(c.y);
    if (typeof c.w === "number") c.w = Math.round(c.w);
    if (typeof c.h === "number") c.h = Math.round(c.h);
  }
  invoke("save_surface_config", { surface: "panel", config: snapshot }).catch(() => {});
}

function exitEdit() {
  editing.value = false;
  selectedId.value = null;
  guides.value = [];
  invoke("set_panel_edit", { enabled: false }).catch(() => {});
}

function onKey(e: KeyboardEvent) {
  if (!editing.value) return;
  if (e.key === "Escape") {
    e.preventDefault();
    exitEdit();
  } else if (e.key === "Delete" || e.key === "Backspace") {
    const cfg = config.value;
    if (!cfg || !selectedId.value) return;
    e.preventDefault();
    cfg.cards = cfg.cards.filter((c) => c.id !== selectedId.value);
    selectedId.value = null;
    saveLater();
  }
}

function onPointerDown(e: PointerEvent) {
  if (!editing.value) return;
  const target = e.target as HTMLElement;
  const el = target.closest("[data-card-id]") as HTMLElement | null;
  if (!el) {
    selectedId.value = null;
    return;
  }
  const id = el.dataset.cardId!;
  const card = config.value?.cards.find((c) => c.id === id);
  if (!card) return;
  selectedId.value = id;

  const resizing = !!target.closest("[data-resize]");
  const k = scaleFactor();
  const startX = e.clientX;
  const startY = e.clientY;
  const ox = card.x ?? 0;
  const oy = card.y ?? 0;
  const ow = card.w ?? cardRect(card).w;
  const oh = card.h ?? cardRect(card).h;
  let moved = false;
  invoke("set_panel_edit_hold", { hold: true }).catch(() => {});

  // 融合块：拖动任意一个成员，整块一起走
  const allCards = config.value?.cards ?? [];
  const groupId = card.group ?? null;
  const members = groupId ? allCards.filter((c) => c.group === groupId) : [card];
  const memberIds = new Set(members.map((c) => c.id));
  const memberIndexes = new Set(
    members.map((m) => allCards.findIndex((c) => c.id === m.id)).filter((i) => i >= 0),
  );
  const startOf = members.map((c) => ({ card: c, x: c.x ?? 0, y: c.y ?? 0 }));
  const box: Rect =
    members.length === 1
      ? { x: ox, y: oy, w: ow, h: oh }
      : {
          x: Math.min(...startOf.map((m) => m.x)),
          y: Math.min(...startOf.map((m) => m.y)),
          w:
            Math.max(...startOf.map((m) => m.x + (m.card.w ?? cardRect(m.card).w))) -
            Math.min(...startOf.map((m) => m.x)),
          h:
            Math.max(...startOf.map((m) => m.y + (m.card.h ?? cardRect(m.card).h))) -
            Math.min(...startOf.map((m) => m.y)),
        };

  const move = (ev: PointerEvent) => {
    const dx = (ev.clientX - startX) / k;
    const dy = (ev.clientY - startY) / k;
    const alt = ev.altKey;
    const rects = allRects();
    const targets = buildSnapTargets(rects, memberIndexes, {
      w: window.innerWidth,
      h: window.innerHeight,
    });
    const tol = 7 / k;

    if (resizing && members.length === 1) {
      const cand: Rect = {
        x: ox,
        y: oy,
        w: Math.max(40, ow + dx),
        h: Math.max(24, oh + dy),
      };
      const s = snapRect(cand, targets, tol, "resize", rects, memberIndexes);
      card.w = Math.max(40, s.snappedX ? cand.w + (s.x - cand.x) : cand.w);
      card.h = Math.max(24, s.snappedY ? cand.h + (s.y - cand.y) : cand.h);
      guides.value = alt ? [] : s.guides;
    } else {
      const cand: Rect = { x: box.x + dx, y: box.y + dy, w: box.w, h: box.h };
      const s = snapRect(cand, targets, tol, "move", rects, memberIndexes);
      let nx = s.snappedX ? s.x : cand.x;
      let ny = s.snappedY ? s.y : cand.y;
      // 整块一起移动时，保证最左边/最上边的成员不出界
      const shiftX = nx - box.x;
      const shiftY = ny - box.y;
      if (Math.min(...startOf.map((m) => m.x)) + shiftX < 0) nx = box.x - Math.min(...startOf.map((m) => m.x));
      if (Math.min(...startOf.map((m) => m.y)) + shiftY < 0) ny = box.y - Math.min(...startOf.map((m) => m.y));
      const fx = nx - box.x;
      const fy = ny - box.y;
      for (const m of startOf) {
        m.card.x = Math.round(m.x + fx);
        m.card.y = Math.round(m.y + fy);
      }
      guides.value = alt ? [] : s.guides;
    }
    moved = true;
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    guides.value = [];
    invoke("set_panel_edit_hold", { hold: false }).catch(() => {});
    if (moved) saveLater();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  e.preventDefault();
}

/** 参考线样式（面板没有缩放，直接用内容坐标） */
function guideStyle(g: Guide): Record<string, string> {
  const from = Math.min(g.from, g.to);
  const len = Math.abs(g.to - g.from);
  return g.vertical
    ? { left: g.at + "px", top: from + "px", height: len + "px" }
    : { top: g.at + "px", left: from + "px", width: len + "px" };
}

const editingCount = computed(() => config.value?.cards.length ?? 0);
</script>

<template>
  <div class="stage" :class="{ editing }">
    <Stage
      :cards="config?.cards ?? []"
      :def-of="defOf"
      :values="values"
      :now="now"
      :editable="editing"
      :selected-id="editing ? selectedId : null"
    />

    <template v-if="editing">
      <div class="guides">
        <div
          v-for="(g, i) in guides"
          :key="i"
          class="guide"
          :class="g.vertical ? 'v' : 'h'"
          :style="guideStyle(g)"
        />
      </div>
      <div class="edit-bar">
        <span class="dot" />
        <b>桌面编辑</b>
        <span class="sep" />
        <span>拖动卡片/融合块移动 · 拖右下角改大小 · 方向键微调 · Delete 删除 · Alt 关吸附</span>
        <span class="sep" />
        <span class="count">{{ editingCount }} 张卡片</span>
        <button class="done" @click="exitEdit">完成（Esc）</button>
      </div>
    </template>

    <div v-if="error" class="error">配置读取失败：{{ error }}</div>
  </div>
</template>

<style scoped>
.stage.editing {
  pointer-events: auto;
}

.guides {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 60;
}

.guide {
  position: absolute;
}

.guide.v {
  border-left: 1px dashed #ff5b8a;
  box-shadow: 0 0 6px rgba(255, 91, 138, 0.55);
}

.guide.h {
  border-top: 1px dashed #ff5b8a;
  box-shadow: 0 0 6px rgba(255, 91, 138, 0.55);
}

.edit-bar {
  position: absolute;
  left: 50%;
  top: 16px;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 10px;
  background: rgba(16, 17, 22, 0.92);
  border: 1px solid rgba(90, 200, 255, 0.35);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  color: var(--text, #e8e8ea);
  font-size: 12px;
  white-space: nowrap;
  z-index: 70;
}

.edit-bar .dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #5ac8ff;
  box-shadow: 0 0 8px rgba(90, 200, 255, 0.8);
}

.edit-bar .sep {
  width: 1px;
  height: 12px;
  background: rgba(255, 255, 255, 0.16);
}

.edit-bar .count {
  color: var(--text-dim, #9a9aa6);
}

.edit-bar .done {
  padding: 4px 10px;
  border-radius: 7px;
  border: 1px solid rgba(90, 200, 255, 0.5);
  background: rgba(90, 200, 255, 0.16);
  color: #bfe6ff;
  font-size: 12px;
  cursor: pointer;
}

.edit-bar .done:hover {
  background: rgba(90, 200, 255, 0.28);
}
</style>
