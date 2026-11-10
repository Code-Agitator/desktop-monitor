<script setup lang="ts">
// 统一的舞台：融合组背景 + 卡片 + 设置页的缩放手柄。
// 面板、HUD、设置预览共用，保证三处渲染一致。
import { computed } from "vue";
import Card from "./Card.vue";
import { rendererFor } from "./renderers";
import { themeVars } from "./themes";
import type { CardConfig, MetricDef, Sample } from "./types";

const props = defineProps<{
  cards: CardConfig[];
  defOf: (id: string) => MetricDef | undefined;
  values: Record<string, Sample>;
  now: number;
  selectedId?: string | null;
  editable?: boolean;
  compact?: boolean;
}>();

function sizeOf(c: CardConfig) {
  const r = rendererFor(c.view, props.defOf(c.metric)?.kind);
  return { w: c.w ?? r.defaultSize.w, h: c.h ?? r.defaultSize.h };
}

/** 同一 group 的卡片合并成一个背景块（矩形为成员并集，间隙也一起融合）。 */
const groups = computed(() => {
  const map = new Map<string, CardConfig[]>();
  for (const c of props.cards) {
    if (!c.group) continue;
    const arr = map.get(c.group) ?? [];
    arr.push(c);
    map.set(c.group, arr);
  }
  return [...map.entries()].map(([id, cards]) => {
    let x1 = Number.POSITIVE_INFINITY;
    let y1 = Number.POSITIVE_INFINITY;
    let x2 = Number.NEGATIVE_INFINITY;
    let y2 = Number.NEGATIVE_INFINITY;
    for (const c of cards) {
      const s = sizeOf(c);
      x1 = Math.min(x1, c.x);
      y1 = Math.min(y1, c.y);
      x2 = Math.max(x2, c.x + s.w);
      y2 = Math.max(y2, c.y + s.h);
    }
    // 取整：成员坐标可能是 70.999999 这种小数，不取整会让「块边界」和「卡片边界」
    // 差 1px，圆角判断就会失配（表现为某一边明明是外沿却成了直角）
    const rx = Math.round(x1);
    const ry = Math.round(y1);
    return { id, x: rx, y: ry, w: Math.round(x2) - rx, h: Math.round(y2) - ry };
  });
});

/** 融合块的主题：取组内第一个显式指定了主题的成员；没有就跟随整体 */
const groupThemes = computed(() => {
  const map = new Map<string, string>();
  for (const c of props.cards) {
    if (c.group && c.theme && !map.has(c.group)) map.set(c.group, c.theme);
  }
  return map;
});

/** 融合组背景块的样式（主题变量 + 几何） */
function groupStyle(g: { id: string; x: number; y: number; w: number; h: number }) {
  const theme = groupThemes.value.get(g.id);
  const R = props.compact ? 10 : 18;
  // 角点落在别的块上 = 这块在那条边上和别人贴边，圆角要抹平；
  // 边中点被别人贴着 = 那条边框要去掉。这样多个块能拼成一个连续形状。
  const covered = (px: number, py: number) =>
    groups.value.some(
      (o) =>
        o.id !== g.id &&
        px >= o.x - 0.5 &&
        px <= o.x + o.w + 0.5 &&
        py >= o.y - 0.5 &&
        py <= o.y + o.h + 0.5,
    );
  const top = covered(g.x + g.w / 2, g.y);
  const right = covered(g.x + g.w, g.y + g.h / 2);
  const bottom = covered(g.x + g.w / 2, g.y + g.h);
  const left = covered(g.x, g.y + g.h / 2);
  const tl = covered(g.x, g.y) ? 0 : R;
  const tr = covered(g.x + g.w, g.y) ? 0 : R;
  const br = covered(g.x + g.w, g.y + g.h) ? 0 : R;
  const bl = covered(g.x, g.y + g.h) ? 0 : R;
  return {
    left: g.x + "px",
    top: g.y + "px",
    width: g.w + "px",
    height: g.h + "px",
    borderRadius: `${tl}px ${tr}px ${br}px ${bl}px`,
    borderWidth: `${top ? 0 : 1}px ${right ? 0 : 1}px ${bottom ? 0 : 1}px ${left ? 0 : 1}px`,
    ...(theme ? themeVars(theme) : {}),
  };
}

/**
 * 融合块里单张卡片的圆角：只有贴着整块外沿的那几个角才圆，
 * 这样每张卡片可以有自己的底色（拼成马赛克），外轮廓仍然是圆角矩形。
 */
function cardCorners(c: CardConfig): string {
  if (!c.group) return "";
  const g = groups.value.find((x) => x.id === c.group);
  if (!g) return "";
  const s = sizeOf(c);
  const R = props.compact ? 10 : 18;
  // 帖边判断留 2px 容差：成员尺寸不完全一致时（比如 170 和 171 高），
  // 外沿那张卡也应该拿到圆角，否则整块会出现「三个角圆、一个角方」
  const near = (a: number, b: number) => Math.abs(a - b) <= 2;
  const tl = near(c.x, g.x) && near(c.y, g.y) ? R : 0;
  const tr = near(c.x + s.w, g.x + g.w) && near(c.y, g.y) ? R : 0;
  const br = near(c.x + s.w, g.x + g.w) && near(c.y + s.h, g.y + g.h) ? R : 0;
  const bl = near(c.x, g.x) && near(c.y + s.h, g.y + g.h) ? R : 0;
  return tl + "px " + tr + "px " + br + "px " + bl + "px";
}

/** 选中卡片的右下角缩放手柄；单独放在卡片外，避免被卡片的 overflow 裁掉。 */
const handle = computed(() => {
  if (!props.editable || !props.selectedId) return null;
  const c = props.cards.find((x) => x.id === props.selectedId);
  if (!c) return null;
  const s = sizeOf(c);
  return { x: c.x + s.w, y: c.y + s.h, id: c.id };
});
</script>

<template>
  <div
    v-for="g in groups"
    :key="'g-' + g.id"
    class="card-group"
    :data-group="g.id"
    :style="groupStyle(g)"
  />
  <Card
    v-for="c in cards"
    :key="c.id"
    :card="c"
    :def="defOf(c.metric)"
    :sample="values[c.metric]"
    :now="now"
    :selected="c.id === selectedId"
    :fused="!!c.group"
    :compact="compact"
    :group-theme="c.group ? groupThemes.get(c.group) ?? null : null"
    :corners="cardCorners(c)"
  />
  <div
    v-if="handle"
    class="resize-handle"
    data-resize="1"
    :data-card-id="handle.id"
    :style="{ left: handle.x + 'px', top: handle.y + 'px' }"
  />
</template>
