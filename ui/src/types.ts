export type MetricKind =
  | "scalar"
  | "ratio"
  | "bytes"
  | "duration"
  | "state"
  | "series"
  | "list";

export interface MetricDef {
  id: string;
  title: string;
  kind: MetricKind;
  unit?: string | null;
  intervalMs: number;
  history?: boolean;
  min?: number | null;
  max?: number | null;
  plugin?: string | null;
}

export interface Sample {
  ts: number;
  v: unknown;
  error?: string;
}

export interface ListRow {
  label: string;
  value: string;
  ratio?: number;
}

export interface CardConfig {
  id: string;
  metric: string;
  view: string;
  x: number;
  y: number;
  w?: number | null;
  h?: number | null;
  options?: Record<string, unknown> | null;
  style?: Record<string, unknown> | null;
  group?: string | null;
  /** 单块主题：覆盖所在面板/HUD 的主题；不填 = 跟随整体 */
  theme?: string | null;
}

export interface WindowConfig {
  x: number;
  y: number;
  w: number;
  h: number;
  enabled: boolean;
}

export interface PanelConfig {
  version: number;
  theme?: string | null;
  cards: CardConfig[];
  window?: WindowConfig | null;
  /** 卡片背景不透明度（0.2 ~ 1.0） */
  opacity?: number;
}

export interface Bootstrap {
  panel: PanelConfig;
  hud: PanelConfig;
  metrics: MetricDef[];
}
