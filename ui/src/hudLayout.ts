import { rendererFor } from "./renderers";
import type { CardConfig, MetricDef } from "./types";

/** HUD 内容四周留白，避免融合块边框贴住窗口边缘（只留 2px，尽量把空间给内容） */
/**
 * 窗口留白：HUD 窗口是全透明的，这里的留白只是不可见的窗口面积（影响拖拽区域），
 * 观感上真正起作用的是卡片内边距（见 Card.vue 的 padding()），所以保持最小。
 */
export const HUD_PAD_X = 2;
export const HUD_PAD_Y = 2;

/** 所有卡片的外接矩形（无显式宽高时用渲染器默认尺寸） */
export function contentBounds(
  cards: CardConfig[],
  defOf: (id: string) => MetricDef | undefined,
): { w: number; h: number } {
  let w = 0;
  let h = 0;
  for (const c of cards) {
    const r = rendererFor(c.view, defOf(c.metric)?.kind);
    w = Math.max(w, c.x + (c.w ?? r.defaultSize.w));
    h = Math.max(h, c.y + (c.h ?? r.defaultSize.h));
  }
  return { w: Math.ceil(w), h: Math.ceil(h) };
}
