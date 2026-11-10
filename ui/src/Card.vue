<script setup lang="ts">
import { computed } from "vue";
import { rendererFor } from "./renderers";
import { themeById } from "./themes";

const props = defineProps<{
  card: any;
  def?: any;
  sample?: any;
  now: number;
  selected?: boolean;
  fused?: boolean;
  compact?: boolean;
  /** 所属融合块的主题（块内成员没有单独指定主题时用它） */
  groupTheme?: string | null;
  /** 融合块里这张卡片的圆角（只在外沿圆，内部接缝直角） */
  corners?: string;
}>();

const state = computed(() => {
  const s = props.sample;
  if (!s) return "stale";
  if (s.error) return "error";
  const interval = props.def?.intervalMs ?? 1000;
  if (props.now - s.ts > interval * 3) return "stale";
  return "ok";
});

const renderer = computed(() => rendererFor(props.card.view, props.def?.kind));

const ageText = computed(() => {
  const ts = props.sample?.ts ?? 0;
  if (!ts) return "";
  const sec = Math.max(0, Math.round((props.now - ts) / 1000));
  if (sec < 60) return `${sec}s 前`;
  if (sec < 3600) return `${Math.round(sec / 60)}m 前`;
  return `${Math.round(sec / 3600)}h 前`;
});

const size = computed(() => ({
  w: props.card.w ?? renderer.value.defaultSize.w,
  h: props.card.h ?? renderer.value.defaultSize.h,
}));

/** 内边距：面板卡片按比例宽松；HUD 格子小，但也留出左右呼吸空间 */
function padding(w: number, h: number) {
  const padY = props.compact
    ? Math.max(1, Math.min(6, Math.round(h * 0.05)))
    : Math.max(5, Math.min(16, Math.round(h * 0.1)));
  const padX = props.compact
    ? Math.max(4, Math.min(12, Math.round(w * 0.075)))
    : Math.max(7, Math.min(18, Math.round(w * 0.09)));
  return { padX, padY };
}

/**
 * 单块主题：每张卡片优先用自己的主题（融合块里就是一小块纯色，拼成马赛克），
 * 没写才回落到所在块的主题。
 */
const ownTheme = computed(() => (props.card.theme ? themeById(props.card.theme) : null));
const themed = computed(() => ownTheme.value ?? (props.groupTheme ? themeById(props.groupTheme) : null));

const style = computed<Record<string, string>>(() => {
  const { w, h } = size.value;
  const { padX, padY } = padding(w, h);
  const base: Record<string, string> = {
    left: `${props.card.x ?? 0}px`,
    top: `${props.card.y ?? 0}px`,
    width: `${w}px`,
    height: `${h}px`,
    padding: `${padY}px ${padX}px`,
    // 内容盒尺寸，渲染器可以按像素算字号（不依赖容器查询单位）
    "--cw": `${Math.max(1, w - padX * 2)}px`,
    "--ch": `${Math.max(1, h - padY * 2)}px`,
  };
  // 融合块里只在外沿的角上给圆角，内部接缝保持直角，整块才拼得平
  if (props.corners) base.borderRadius = props.corners;
  // 字号缩放：数字卡片可以单独把字放大/缩小，不影响卡片尺寸
  const fontScale = Number((props.card.options as Record<string, unknown> | null)?.fontScale);
  if (Number.isFinite(fontScale) && fontScale > 0) {
    base["--font-scale"] = String(fontScale);
  }
  // 主题变量写在卡片自身，渲染器只引用变量 → 天然按块生效
  if (themed.value) Object.assign(base, themed.value.vars);
  const extra = props.card.style;
  if (extra && typeof extra === "object") {
    for (const [k, v] of Object.entries(extra)) {
      if (typeof v === "string") base[k] = v;
    }
  }
  return base;
});

/** 内容盒的像素尺寸：数字渲染器据此决定能显示几位小数 */
const boxSize = computed(() => {
  const { w, h } = size.value;
  const { padX, padY } = padding(w, h);
  return { cw: Math.max(1, w - padX * 2), ch: Math.max(1, h - padY * 2) };
});
</script>

<template>
  <div class="card" :class="[state, { selected, fused, themed: !!ownTheme }]" :data-card-id="card.id" :style="style">
    <component
      :is="renderer.component"
      :def="def"
      :sample="sample"
      :options="card.options || {}"
      :box="renderer.id === 'number' ? boxSize : undefined"
    />
    <div v-if="state !== 'ok' && ageText" class="age">{{ ageText }}</div>
  </div>
</template>
