<script setup lang="ts">
import { computed } from "vue";
import { formatValue, levelOf } from "../format";

const props = defineProps<{
  def?: any;
  sample?: any;
  options: Record<string, any>;
  /** 卡片内容盒尺寸（px），由 Card.vue 传入；用来决定能显示几位小数 */
  box?: { cw: number; ch: number };
}>();
const level = computed(() => levelOf(props.def, props.sample?.v, props.options));
const label = computed(() => props.options?.label || props.def?.title || "");
// HUD 单元格排版：options.layout = "stacked"（标签在上）/ "inline"（标签在左）
// 兼容早期只写 options.stacked: true 的配置。
const layout = computed(() => {
  const l = props.options?.layout;
  if (l === "inline" || l === "stacked") return l;
  return props.options?.stacked === true ? "stacked" : "";
});
const stacked = computed(() => layout.value === "stacked");
const inline = computed(() => layout.value === "inline");

/** 估算文字宽度（em），用于在固定尺寸的格子里把字号撑到刚好放得下。 */
function advance(text: string): number {
  let w = 0;
  for (const ch of text) {
    const code = ch.charCodeAt(0);
    if (ch >= "0" && ch <= "9") w += 0.6;
    else if (ch === "." || ch === "," || ch === ":") w += 0.3;
    else if (ch === "/" || ch === " ") w += 0.32;
    // 箭头等符号（↑ ↓ → ←）在 Segoe UI 里接近全角，估窄了会被省略号裁掉
    else if (code >= 0x2190 && code <= 0x2bff) w += 0.6;
    else if (code > 0x2e80) w += 1;
    else w += 0.66;
  }
  return w;
}

function widthOf(text: string, unit?: string): number {
  return advance(text) + (unit ? advance(unit) * 0.72 + 0.1 : 0) + 0.12;
}

const auto = computed(() => layout.value === "");
// 纯符号标签（↑ ↓ 这类）：宽度估算时按符号算
const symbolLabel = computed(() => /^[\u2190-\u2bff\u2600-\u27bf]+$/.test(label.value.trim()));

const digitsEm = (n: number) => (n > 0 ? "." + "9".repeat(n) : "");

/** 数值「最坏情况」的宽度（em）：百分比 100.99、标量 999.99、字节按最长单位档 */
function valueEmOf(n: number): number {
  const kind = props.def?.kind ?? "scalar";
  if (kind === "ratio") return widthOf("100" + digitsEm(n), "%");
  if (kind === "bytes") return widthOf("999.99", props.def?.unit === "B/s" ? "MB/s" : "GB");
  if (kind === "scalar") return widthOf("999" + digitsEm(n), props.def?.unit);
  return widthOf(f.value.text, f.value.unit);
}

/**
 * HUD 单元格的整体排版：标签和数值**同字号**，只靠字重/亮度区分。
 * 字号按「最坏情况的标签 + 最坏情况的数值」一次算出来（而不是按当前值），
 * 所以数值怎么变，字号、小数位、整组宽度都不跳。
 */
const hud = computed(() => {
  const b = props.box;
  if (!b || !layout.value) return null;
  const gap = Math.max(2, b.ch * 0.06);
  const cap = Math.min(b.ch * (inline.value ? 0.68 : 0.64), 32);
  // 标签宽度（em）：最多 3 个字符 + 按需留省略号位；再长就截断
  const labelEm = Math.min(
    advance([...label.value].slice(0, 3).join("")) + (label.value.length > 3 ? 1.0 : 0),
    3.6,
  );
  const sizeFor = (n: number) => {
    const vEm = valueEmOf(n);
    return inline.value
      ? Math.min(cap, (b.cw - gap) / Math.max(1, labelEm + vEm))
      : Math.min(cap, b.cw / Math.max(labelEm, vEm));
  };
  // 小数位：宽度够就给两位，为多一位把字号压掉 12% 以上就退一位
  const explicit = props.options?.decimals;
  const kind = props.def?.kind;
  let dec: number | undefined;
  if (typeof explicit === "number") dec = explicit;
  else if (kind === "ratio" || kind === "scalar") {
    dec = 0;
    for (const n of [2, 1]) {
      if (sizeFor(n) >= cap * 0.88) {
        dec = n;
        break;
      }
    }
  }
  return { size: Math.max(9, sizeFor(dec ?? 0)), decimals: dec };
});

const decimals = computed<number | undefined>(() => hud.value?.decimals);

const fmtOptions = computed(() => {
  const o = { ...(props.options || {}) };
  if (decimals.value != null) o.decimals = decimals.value;
  return o;
});

const f = computed(() => formatValue(props.def, props.sample?.v, fmtOptions.value));
const long = computed(
  () =>
    auto.value &&
    (props.def?.kind === "duration" || f.value.text.length + (f.value.unit?.length ?? 0) > 5),
);
const longLabel = computed(() => auto.value && label.value.length > 4);

/** 兜底：没有拿到卡片尺寸时（理论上只在非 HUD 场景），沿用 em 估宽 */
const sizeDen = computed(() => {
  const kind = props.def?.kind ?? "scalar";
  const dec = decimals.value ?? 0;
  const decs = digitsEm(dec);
  if (kind === "ratio") return widthOf("100" + decs, "%").toFixed(2);
  if (kind === "bytes") return widthOf("999.9", props.def?.unit === "B/s" ? "MB/s" : "GB").toFixed(2);
  if (kind === "scalar") return widthOf("999" + decs, f.value.unit ?? props.def?.unit).toFixed(2);
  return widthOf(f.value.text, f.value.unit).toFixed(2);
});
const labelDen = computed(() => (advance(label.value) + 0.08).toFixed(2));
const labelLead = computed(() => {
  const head = advance([...label.value].slice(0, 3).join(""));
  return (head + (label.value.length > 3 ? 1.0 : 0)).toFixed(2);
});
const style = computed(() => {
  const base: Record<string, string> = { "--den": sizeDen.value, "--lden": labelDen.value, "--l3": labelLead.value };
  // 标签与数值共用一个字号
  if (hud.value) base["--fsize"] = `${hud.value.size.toFixed(2)}px`;
  return base;
});
</script>

<template>
  <div
    class="number"
    :class="{ long, longLabel, symbolLabel, stacked, inline }"
    :style="style"
  >
    <div class="row">
      <div class="label">{{ label }}</div>
      <div class="value" :class="[level, { compact: long }]">
        {{ f.text }}<span v-if="f.unit" class="unit">{{ f.unit }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.number {
  height: 100%;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* 垂直/水平都居中；大卡片纵向排（标签在上），小卡片横向排 */
.row {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  min-width: 0;
  max-width: 100%;
  text-align: center;
}

.label {
  font-size: calc(clamp(9px, 12cqh, 11px) * var(--font-scale, 1));
  font-weight: 500;
  /* 小字 + 字距 + 大写：英文标签有小标题的感觉，中文不受影响 */
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
  flex: none;
}

.value {
  font-size: calc(clamp(14px, 36cqh, 30px) * var(--font-scale, 1));
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.01em;
  line-height: 1;
  white-space: nowrap;
  min-width: 0;
}

.value.compact {
  font-size: calc(clamp(11px, 24cqh, 20px) * var(--font-scale, 1));
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

/* 小卡片：单行紧凑布局（标签 + 数值），整组居中 */
@container (max-height: 58px) or (max-width: 112px) {
  .row {
    flex-direction: row;
    align-items: baseline;
    gap: 8px;
    text-align: left;
  }
  .label {
    font-size: calc(clamp(9px, 20cqh, 12px) * var(--font-scale, 1));
  }
  .value.compact {
    font-size: calc(clamp(10px, 22cqh, 17px) * var(--font-scale, 1));
  }
  /* 数值或标签较长时优先保证数值可读，收起标签 */
  .number.long .label,
  .number.longLabel .label {
    display: none;
  }
}

/* 极窄的卡片只保留数值 */
@container (max-width: 56px) {
  .label {
    display: none;
  }
}

/* HUD 单元格（上下排版）：铺满整格，标签在上、数值在下，整组垂直居中。
   字号只跟内容盒尺寸有关（--cw/--ch 由 Card.vue 给出），宽度那一项用的是
   「最坏情况」数值的估算宽度，所以数值怎么变，字号和标签位置都不动。
   标签与数值之间只留 1px 行距。 */
.number.stacked {
  align-items: stretch;
}
.number.stacked .row {
  width: 100%;
  height: 100%;
  box-sizing: border-box;
  /* 数字没有下伸部，行盒下方会多出一截空白；补一点上边距让整组看起来居中 */
  padding-top: 3px;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1px;
  text-align: center;
}
/* 纯符号标签（↑ ↓）：小字号下太糊，单独放大一档 */
.number.stacked .label {
  display: block;
  font-size: calc(var(--fsize, max(9px, min(calc(var(--ch, 42px) * 0.30), 12px, calc(var(--cw, 66px) / var(--lden, 4))))) * var(--font-scale, 1));
  font-weight: 600;
  line-height: 1;
  letter-spacing: 0.2px;
  color: var(--text-dim);
  text-align: center;
}
.number.stacked .value,
.number.stacked .value.compact {
  margin: 0;
  font-size: calc(var(--fsize, max(10px, min(calc(var(--ch, 42px) * 0.57), 32px, calc(var(--cw, 66px) / var(--den, 3))))) * var(--font-scale, 1));
  line-height: 0.95;
  text-align: center;
  /* 纯白太扎眼，压一档 */
  color: var(--text-strong);
}
.number.stacked .value.warn {
  color: var(--warn);
}
.number.stacked .value.bad {
  color: var(--bad);
}
.number.stacked .unit {
  font-size: 0.72em;
  font-weight: 500;
  color: var(--text-strong);
  margin-left: 0.1em;
}
.number.stacked.long .label,
.number.stacked.longLabel .label {
  display: block;
}

/* HUD 单元格（左右排版）：标签在左、数值紧跟其后，整组在格子里水平居中，
   所以左右两侧留白永远相等。数值用等宽数字，位数不变时整组宽度是恒定的，
   1 -> 2 位整数才会移半格（很少发生）。
   --lcol 只是给「数值字号」估算可用宽度用的，不再参与栅格定位。 */
.number.inline {
  /* 间隙跟着格子高度走，格子缩小时不会显得缝太大 */
  --lgap: max(2px, calc(var(--ch, 42px) * 0.06));
  --lpx: clamp(9px, calc(var(--ch, 42px) * 0.30), 12px);
  /* 标签占宽：够放下标签本身（不足 3 字时按实际字数，超过 3 字时 3 字 + 省略号余量），
     最多占 52%；短标签（如 ↑）能把宽度让给数值 */
  --lcol: min(calc(var(--cw, 66px) * 0.52), calc(var(--l3, 3) * var(--lpx) + 1px));
}
.number.inline .row {
  display: flex;
  align-items: center;
  /* 标签贴左、数值贴右：两列的边界对得齐，数值位数变化也不会挪动标签 */
  justify-content: space-between;
  gap: var(--lgap);
  width: 100%;
  height: auto;
  box-sizing: border-box;
  text-align: left;
}
.number.inline .row .label {
  display: block;
  flex: none;
  max-width: 52%;
  font-size: calc(var(--fsize, var(--lpx)) * var(--font-scale, 1));
  font-weight: 600;
  line-height: 1;
  letter-spacing: 0.2px;
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-align: left;
}
.number.inline .value,
.number.inline .value.compact {
  flex: none;
  margin: 0;
  font-size: calc(
    max(
      10px,
      min(
        calc(var(--ch, 42px) * 0.68),
        32px,
        calc((var(--cw, 66px) - var(--lcol) - var(--lgap)) / var(--den, 3))
      )
    ) * var(--font-scale, 1)
  );
  line-height: 1;
  font-variant-numeric: tabular-nums;
  text-align: left;
  white-space: nowrap;
  /* 纯白太扎眼，压一档 */
  color: var(--text-strong);
}
.number.inline .value.warn {
  color: var(--warn);
}
.number.inline .value.bad {
  color: var(--bad);
}
.number.inline .unit {
  font-size: 0.72em;
  font-weight: 500;
  color: var(--text-strong);
  margin-left: 0.1em;
}
</style>
