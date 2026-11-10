import type { Component } from "vue";
import type { MetricKind } from "../types";
import NumberRenderer from "./NumberRenderer.vue";
import RingRenderer from "./RingRenderer.vue";
import BarRenderer from "./BarRenderer.vue";
import SparklineRenderer from "./SparklineRenderer.vue";
import StatusRenderer from "./StatusRenderer.vue";
import ListRenderer from "./ListRenderer.vue";
import TextRenderer from "./TextRenderer.vue";
import BatteryRenderer from "./BatteryRenderer.vue";

export interface RendererDef {
  id: string;
  name: string;
  accepts: MetricKind[];
  defaultSize: { w: number; h: number };
  component: Component;
}

export const RENDERERS: RendererDef[] = [
  {
    id: "number",
    name: "数字",
    accepts: ["scalar", "ratio", "bytes", "duration"],
    defaultSize: { w: 150, h: 74 },
    component: NumberRenderer,
  },
  {
    id: "ring",
    name: "环形",
    accepts: ["ratio", "scalar"],
    defaultSize: { w: 170, h: 170 },
    component: RingRenderer,
  },
  {
    id: "battery",
    name: "电池",
    accepts: ["ratio"],
    defaultSize: { w: 140, h: 64 },
    component: BatteryRenderer,
  },
  {
    id: "bar",
    name: "条形",
    accepts: ["ratio", "scalar", "bytes"],
    defaultSize: { w: 260, h: 74 },
    component: BarRenderer,
  },
  {
    id: "sparkline",
    name: "折线",
    accepts: ["series"],
    defaultSize: { w: 260, h: 96 },
    component: SparklineRenderer,
  },
  {
    id: "status",
    name: "状态",
    accepts: ["state"],
    defaultSize: { w: 220, h: 60 },
    component: StatusRenderer,
  },
  {
    id: "list",
    name: "列表",
    accepts: ["list"],
    defaultSize: { w: 260, h: 150 },
    component: ListRenderer,
  },
  {
    id: "text",
    name: "文本",
    accepts: ["state", "scalar", "duration"],
    defaultSize: { w: 220, h: 120 },
    component: TextRenderer,
  },
];

const byId = new Map(RENDERERS.map((r) => [r.id, r]));

/** 按配置的 view 取渲染器；view 不存在或不兼容时按 kind 回退。 */
export function rendererFor(view: string | undefined, kind: MetricKind | undefined): RendererDef {
  const explicit = view ? byId.get(view) : undefined;
  if (explicit && (!kind || explicit.accepts.includes(kind))) return explicit;
  if (kind) {
    const fallback = RENDERERS.find((r) => r.accepts.includes(kind));
    if (fallback) return fallback;
  }
  return byId.get("number")!;
}

/** 该 kind 可用的渲染器（设置窗口用） */
export function renderersFor(kind: MetricKind): RendererDef[] {
  return RENDERERS.filter((r) => r.accepts.includes(kind));
}
