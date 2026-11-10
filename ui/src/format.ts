import type { MetricDef } from "./types";

export type Level = "ok" | "warn" | "bad";

export function numberOf(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

/**
 * 字节数拆成「数字 + 单位」两段：数字给大字号，单位给小字号。
 * 12.30 GB 这类多余的尾零会去掉（12.3 GB / 1 GB）。
 */
export function formatBytesParts(n: number, decimals?: number): Formatted {
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let x = Math.abs(n);
  let i = 0;
  while (x >= 1024 && i < units.length - 1) {
    x /= 1024;
    i++;
  }
  const d = decimals ?? (i === 0 ? 0 : x >= 100 ? 0 : x >= 10 ? 1 : 2);
  let text = x.toFixed(d);
  if (d > 0) text = text.replace(/\.?0+$/, "");
  return { text, unit: units[i] };
}

export function formatBytes(n: number, decimals?: number): string {
  const p = formatBytesParts(n, decimals);
  return `${p.text} ${p.unit}`;
}

export function formatDuration(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d}天 ${h}小时`;
  if (h > 0) return `${h}小时 ${m}分`;
  if (m > 0) return `${m}分 ${s % 60}秒`;
  return `${s}秒`;
}

export interface Formatted {
  text: string;
  unit?: string;
}

export function formatValue(
  def: MetricDef | undefined,
  v: unknown,
  options: Record<string, any> = {},
): Formatted {
  const kind = def?.kind ?? "scalar";
  const d = options.decimals as number | undefined;

  switch (kind) {
    case "ratio": {
      const n = numberOf(v);
      if (n == null) return { text: "--" };
      return { text: (n * 100).toFixed(d ?? 0), unit: "%" };
    }
    case "bytes": {
      const n = numberOf(v);
      if (n == null) return { text: "--" };
      const isRate = def?.unit === "B/s";
      const p = formatBytesParts(n, d);
      return { text: p.text, unit: p.unit + (isRate ? "/s" : "") };
    }
    case "duration": {
      const n = numberOf(v);
      if (n == null) return { text: "--" };
      return { text: formatDuration(n) };
    }
    case "scalar": {
      const n = numberOf(v);
      if (n == null) return { text: "--" };
      const dec = d ?? (Math.abs(n) >= 100 ? 0 : 1);
      return { text: n.toFixed(dec), unit: def?.unit ?? undefined };
    }
    case "state":
      return { text: v == null ? "--" : String(v) };
    default:
      return { text: v == null ? "--" : String(v) };
  }
}

export function levelOf(
  def: MetricDef | undefined,
  v: unknown,
  options: Record<string, any> = {},
): Level {
  const n = numberOf(v);
  if (n == null) return "ok";
  const t = options.thresholds as { warn?: number; bad?: number } | undefined;
  if (t) {
    if (t.bad != null && n >= t.bad) return "bad";
    if (t.warn != null && n >= t.warn) return "warn";
  }
  if (def?.kind === "ratio") {
    if (n >= 0.9) return "bad";
    if (n >= 0.75) return "warn";
  }
  return "ok";
}

/** 把任意 kind 的数值归一化到 0..1，用于环形/条形 */
export function normalized(def: MetricDef | undefined, v: unknown): number {
  const n = numberOf(v);
  if (n == null) return 0;
  if (def?.kind === "ratio") return Math.min(1, Math.max(0, n));
  const min = def?.min ?? 0;
  const max = def?.max ?? 100;
  const span = max - min || 1;
  return Math.min(1, Math.max(0, (n - min) / span));
}
