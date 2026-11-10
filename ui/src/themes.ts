// 主题 = 一组 CSS 变量。卡片与渲染器只引用变量，不写死颜色。
// 应用到 documentElement（面板/HUD）或设置窗口里的预览容器。

export interface ThemeDef {
  id: string;
  name: string;
  vars: Record<string, string>;
}

export const THEMES: ThemeDef[] = [
  {
    id: "default-dark",
    name: "暗色玻璃",
    vars: {
      "--accent": "#5ac8ff",
      "--violet": "#b58cff",
      "--amber": "#ffb454",
      "--ok": "#46e08a",
      "--warn": "#ffd166",
      "--bad": "#ff6b6b",
      "--bad-start": "#ff8a5c",
      "--text": "#e8e8ea",
      "--text-dim": "#9a9aa6",
      "--text-strong": "#d6d6de",
      "--card-bg": "rgb(18, 18, 22)",
      "--card-border": "rgba(255, 255, 255, 0.08)",
      "--card-shadow":
        "0 18px 40px rgba(0, 0, 0, 0.35), inset 0 1px 0 rgba(255, 255, 255, 0.06)",
      "--track": "rgba(255, 255, 255, 0.08)",
    },
  },
  {
    id: "midnight",
    name: "午夜蓝",
    vars: {
      "--accent": "#6ea8ff",
      "--violet": "#a78bfa",
      "--amber": "#fbbf24",
      "--ok": "#34d399",
      "--warn": "#fbbf24",
      "--bad": "#f87171",
      "--bad-start": "#fb923c",
      "--text": "#e6ecff",
      "--text-dim": "#8f9bc4",
      "--text-strong": "#cfd9f7",
      "--card-bg": "rgb(12, 18, 42)",
      "--card-border": "rgba(120, 160, 255, 0.14)",
      "--card-shadow":
        "0 18px 40px rgba(2, 8, 30, 0.5), inset 0 1px 0 rgba(150, 190, 255, 0.08)",
      "--track": "rgba(140, 170, 255, 0.12)",
    },
  },
  {
    id: "violet",
    name: "暗紫",
    vars: {
      "--accent": "#c4a2ff",
      "--violet": "#8b5cf6",
      "--amber": "#ffb454",
      "--ok": "#4ade80",
      "--warn": "#ffd166",
      "--bad": "#ff6b8b",
      "--bad-start": "#ff8a6b",
      "--text": "#efeafb",
      "--text-dim": "#9a90b8",
      "--text-strong": "#d9d0f0",
      "--card-bg": "rgb(24, 18, 36)",
      "--card-border": "rgba(181, 140, 255, 0.18)",
      "--card-shadow":
        "0 18px 40px rgba(10, 4, 24, 0.5), inset 0 1px 0 rgba(200, 170, 255, 0.08)",
      "--track": "rgba(181, 140, 255, 0.14)",
    },
  },
  {
    id: "frost",
    name: "浅色磨砂",
    vars: {
      "--accent": "#0a84ff",
      "--violet": "#7c3aed",
      "--amber": "#d97706",
      "--ok": "#059669",
      "--warn": "#d97706",
      "--bad": "#dc2626",
      "--bad-start": "#ea580c",
      "--text": "#1c2028",
      "--text-dim": "#5f6773",
      "--text-strong": "#2b313b",
      "--card-bg": "rgb(248, 250, 252)",
      "--card-border": "rgba(15, 23, 42, 0.10)",
      "--card-shadow":
        "0 18px 40px rgba(15, 23, 42, 0.16), inset 0 1px 0 rgba(255, 255, 255, 0.7)",
      "--track": "rgba(15, 23, 42, 0.10)",
    },
  },
];

export function themeById(id?: string | null): ThemeDef {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

/** 变量写进元素的内联样式，子元素按就近作用域解析。 */
export function applyTheme(el: HTMLElement, id?: string | null): ThemeDef {
  const theme = themeById(id);
  for (const [k, v] of Object.entries(theme.vars)) el.style.setProperty(k, v);
  // 融合块拼成的外轮廓用整体主题的边框色，避免各块自带主题时外框颜色断成几截
  el.style.setProperty("--frame-border", theme.vars["--card-border"] ?? "transparent");
  el.dataset.theme = theme.id;
  return theme;
}

export function themeVars(id?: string | null): Record<string, string> {
  return { ...themeById(id).vars };
}
