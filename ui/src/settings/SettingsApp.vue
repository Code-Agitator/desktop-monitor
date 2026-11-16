<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import Stage from "../Stage.vue";
import Dropdown from "./Dropdown.vue";
import { buildSnapTargets, snapRect, type Guide, type Rect } from "../snap";
import { contentBounds, HUD_PAD_X, HUD_PAD_Y } from "../hudLayout";
import { rendererFor, renderersFor } from "../renderers";
import { check as checkUpdater } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { THEMES, themeVars } from "../themes";
import type { Bootstrap, MetricDef, PanelConfig, Sample, WindowConfig } from "../types";

type Surface = "panel" | "hud" | "devices" | "plugins";

const surface = ref<Surface>("panel");
const panel = ref<PanelConfig | null>(null);
const hud = ref<PanelConfig | null>(null);
const metrics = ref<MetricDef[]>([]);
const values = ref<Record<string, Sample>>({});
const now = ref(Date.now());
const screen = ref({ w: 2560, h: 1440 });
const scale = ref(1);
const selectedId = ref<string | null>(null);
const checkedIds = ref<string[]>([]);
const notice = ref("");
const wrap = ref<HTMLElement | null>(null);
const viewport = ref<HTMLElement | null>(null);
/** 用户手动缩放倍数（1 = 自适应），Ctrl+滚轮调整 */
const zoom = ref(1);

/** 按住空格 = 临时进入「抓手」平移模式 */
const spaceHeld = ref(false);
const panning = ref(false);
const undoStack = ref<string[]>([]);
const plugins = ref<PluginRow[]>([]);
const diag = ref<DiagInfo | null>(null);
/** 保存状态：给用户一个「改动已经生效」的反馈 */
const saveState = ref<"idle" | "saving" | "saved" | "error">("idle");

interface PluginRow {
  id: string;
  name: string;
  version: string;
  status: string;
  detail?: string | null;
  metrics: number;
}
interface DiagInfo {
  configDir: string;
  pluginsDir: string;
  metrics: number;
  version: string;
}

const STATUS_TEXT: Record<string, string> = {
  discovered: "已发现",
  starting: "启动中",
  running: "运行中",
  failed: "启动失败",
  restarting: "重启中",
  disabled: "已禁用",
  stopped: "已停止",
};

let lastSave = 0;
/** 预览用：指标事件先攒着，1Hz 合批刷新 */
let pendingValues: Record<string, Sample> = {};
let previewTimer: number | undefined;
let saveTimer: number | undefined;
let ro: ResizeObserver | undefined;
const unlisten: UnlistenFn[] = [];

const config = computed(() => (surface.value === "panel" ? panel.value : hud.value));
const base = computed(() => {
  if (surface.value === "panel") return { w: screen.value.w, h: screen.value.h };
  const c = contentSize.value;
  return {
    w: Math.max(40, c.w + HUD_PAD_X * 2),
    h: Math.max(24, c.h + HUD_PAD_Y * 2),
  };
});
const themeStyle = computed<Record<string, string>>(() => themeVars(config.value?.theme));
const contentSize = computed(() => contentBounds(config.value?.cards ?? [], defOf));
const selected = computed(() => config.value?.cards.find((c) => c.id === selectedId.value) ?? null);
const selectedDef = computed(() => defOf(selected.value?.metric ?? ""));
const hasCheckedGroup = computed(() =>
  (config.value?.cards ?? []).some((c) => checkedIds.value.includes(c.id) && c.group),
);

function defOf(id: string): MetricDef | undefined {
  return metrics.value.find((m) => m.id === id);
}

function cardTitle(c: any): string {
  return c.options?.label || defOf(c.metric)?.title || c.metric;
}

/** 卡片列表里显示渲染器的中文名，而不是 number / ring 这种 id */
function rendererName(c: any): string {
  return rendererFor(c.view, defOf(c.metric)?.kind).name;
}

/** 指标按来源分组：内置指标在前，各插件用自己的显示名 */
const metricGroups = computed(() => {
  const map = new Map<string, { key: string; label: string; items: MetricDef[] }>();
  for (const m of metrics.value) {
    const key = m.plugin ?? "";
    let g = map.get(key);
    if (!g) {
      const plugin = key ? plugins.value.find((p) => p.id === key) : undefined;
      g = {
        key: key || "builtin",
        label: key ? plugin?.name || key : "内置指标",
        items: [],
      };
      map.set(key, g);
    }
    g.items.push(m);
  }
  return [...map.values()];
});

/** 自绘下拉的选项 */
const themeOptions = computed(() => THEMES.map((t) => ({ value: t.id, label: t.name })));

/** 指标 → 设备键（插件上报的设备指标清单建立映射） */
const deviceOfMetric = computed(() => {
  const map = new Map<string, string>();
  for (const d of devices.value) {
    for (const m of d.metrics ?? []) map.set(m.id, d.key);
  }
  return map;
});

/** 指标所属设备（"" = 系统/通用） */
const selectedDevice = computed(() => deviceOfMetric.value.get(selected.value?.metric ?? "") ?? "");

const deviceOptions = computed(() => [
  { value: "", label: "系统 / 通用" },
  ...devices.value
    .filter((d) => !d.hidden)
    .map((d) => ({ value: d.key, label: `${d.name}（${d.level}%）` })),
]);

/** 只列当前设备（或系统）下的指标：指标 + 设备 = 一张卡片 */
const metricOptions = computed(() => {
  const dev = selectedDevice.value;
  const out: { value: string; label: string; group?: string }[] = [];
  for (const g of metricGroups.value) {
    for (const m of g.items) {
      if ((deviceOfMetric.value.get(m.id) ?? "") !== dev) continue;
      out.push({ value: m.id, label: m.title, group: dev ? undefined : g.label });
    }
  }
  const cur = selected.value?.metric;
  if (cur && !out.some((o) => o.value === cur)) {
    out.push({ value: cur, label: cur + "（指标不可用）" });
  }
  return out;
});

/** 切换设备：自动落到该设备的第一个指标上 */
function setDevice(key: string) {
  if (!selected.value) return;
  let metricId: string | undefined;
  if (key === "") {
    metricId = metrics.value.find((m) => !deviceOfMetric.value.has(m.id))?.id;
  } else {
    const dev = devices.value.find((d) => d.key === key);
    metricId =
      dev?.metrics?.[0]?.id ??
      metrics.value.find((m) => deviceOfMetric.value.get(m.id) === key)?.id;
  }
  if (metricId) setMetric(metricId);
}

const rendererOptions = computed(() =>
  views.value.map((r) => ({ value: r.id, label: r.name })),
);

const layoutOptions = [
  { value: "auto", label: "自动（按卡片大小）" },
  { value: "stacked", label: "上下 · 标签在上" },
  { value: "inline", label: "左右 · 标签在左" },
];

const cardThemeOptions = computed(() => [
  { value: "", label: "跟随整体（" + themeName(config.value?.theme) + "）" },
  ...THEMES.map((t) => ({ value: t.id, label: t.name })),
]);

const views = computed(() => renderersFor(selectedDef.value?.kind ?? "scalar"));

function snap(v: number): number {
  return Math.round(v / 8) * 8;
}

/** ---------- 智能参考线（对标 PPT：移动/缩放时吸附到相邻元素与画布边缘） ---------- */

const guides = ref<Guide[]>([]);
/** 屏幕上多大的距离内触发吸附 */
const SNAP_PX = 7;

/**
 * 给候选矩形算出吸附后的位置，并生成参考线。
 * mode=move 时左/中/右（上/中/下）都可吸；mode=resize 时只吸被拖动的那条边。
 */
function computeSnap(
  rect: Rect,
  excludeId: string,
  mode: "move" | "resize",
  disable: boolean,
): { x: number; y: number; guides: Guide[]; snappedX: boolean; snappedY: boolean } {
  if (disable) {
    return { x: rect.x, y: rect.y, guides: [], snappedX: false, snappedY: false };
  }
  const cards = config.value?.cards ?? [];
  const index = cards.findIndex((c) => c.id === excludeId);
  const rects = cards.map(cardRect);
  const targets = buildSnapTargets(rects, index, { w: base.value.w, h: base.value.h });
  const tol = SNAP_PX / (scale.value || 1);
  return snapRect(rect, targets, tol, mode, rects, index);
}

/** 参考线在预览里的样式（视口坐标 = 内容坐标 × scale） */
function guideStyle(g: Guide): Record<string, string> {
  const s = scale.value;
  const from = Math.min(g.from, g.to) * s;
  const len = Math.abs(g.to - g.from) * s;
  return g.vertical
    ? { left: g.at * s + "px", top: from + "px", height: len + "px", width: "0" }
    : { top: g.at * s + "px", left: from + "px", width: len + "px", height: "0" };
}

function clearGuides() {
  if (guides.value.length) guides.value = [];
}

function cardRect(c: CardConfig): Rect {
  const r = rendererFor(c.view, defOf(c.metric)?.kind);
  return { x: c.x, y: c.y, w: c.w ?? r.defaultSize.w, h: c.h ?? r.defaultSize.h };
}

function unionRects(rs: Rect[]): Rect {
  let x1 = Number.POSITIVE_INFINITY;
  let y1 = Number.POSITIVE_INFINITY;
  let x2 = Number.NEGATIVE_INFINITY;
  let y2 = Number.NEGATIVE_INFINITY;
  for (const r of rs) {
    x1 = Math.min(x1, r.x);
    y1 = Math.min(y1, r.y);
    x2 = Math.max(x2, r.x + r.w);
    y2 = Math.max(y2, r.y + r.h);
  }
  return { x: x1, y: y1, w: x2 - x1, h: y2 - y1 };
}

/** 严格相交：边贴边不算重叠 */
function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

/** 除正在编辑的卡片所属块之外的所有静态块（融合组按整块算） */
function staticBlocks(movingId: string): Rect[] {
  const cards = config.value?.cards ?? [];
  const groups = new Map<string, CardConfig[]>();
  for (const c of cards) {
    if (!c.group) continue;
    const arr = groups.get(c.group) ?? [];
    arr.push(c);
    groups.set(c.group, arr);
  }
  const out: Rect[] = [];
  for (const [, members] of groups) {
    if (members.some((m) => m.id === movingId)) continue;
    out.push(unionRects(members.map(cardRect)));
  }
  for (const c of cards) {
    if (!c.group && c.id !== movingId) out.push(cardRect(c));
  }
  return out;
}

/** 卡片在候选几何下所占的块（融合卡片取整组并集） */
function candidateBlock(card: CardConfig, override: Partial<Rect>): Rect {
  const r = { ...cardRect(card), ...override };
  if (!card.group) return r;
  const mates = (config.value?.cards ?? []).filter(
    (c) => c.group === card.group && c.id !== card.id,
  );
  return unionRects([...mates.map(cardRect), r]);
}

function overlapArea(a: Rect, b: Rect): number {
  const w = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
  const h = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
  return w > 0 && h > 0 ? w * h : 0;
}

/** 候选位置上，与每个静态块、每个同组伙伴的重叠面积（顺序稳定） */
function overlapProfile(card: CardConfig, override: Partial<Rect>, blocks: Rect[]): number[] {
  const block = candidateBlock(card, override);
  const out = blocks.map((b) => overlapArea(block, b));
  if (card.group) {
    const self = { ...cardRect(card), ...override };
    const mates = (config.value?.cards ?? []).filter(
      (c) => c.group === card.group && c.id !== card.id,
    );
    out.push(...mates.map((m) => overlapArea(self, cardRect(m))));
  }
  return out;
}

/**
 * 不允许新增重叠：没有重叠时必须保持零重叠；
 * 已经重叠时，总面积必须变小，且任何一块的重叠都不能变大（只能往外拖）。
 */
function canMove(card: CardConfig, override: Partial<Rect>, blocks: Rect[]): boolean {
  const cur = overlapProfile(card, {}, blocks);
  const nxt = overlapProfile(card, override, blocks);
  const sum = (a: number[]) => a.reduce((s, v) => s + v, 0);
  const curTotal = sum(cur);
  const nxtTotal = sum(nxt);
  if (curTotal === 0) return nxtTotal === 0;
  if (nxtTotal >= curTotal - 0.5) return false;
  return nxt.every((v, i) => v <= (cur[i] ?? 0) + 0.5);
}

function showNotice(msg: string) {
  notice.value = msg;
  window.setTimeout(() => {
    if (notice.value === msg) notice.value = "";
  }, 2600);
}

let fitScale = 1;

/** 按「自适应 × 用户缩放」算出最终比例 */
function applyScale() {
  scale.value = Math.max(0.1, Math.min(4, fitScale * zoom.value));
}

function recalc() {
  const el = wrap.value;
  if (!el) return;
  const pad = 32;
  const fit = Math.min(
    (el.clientWidth - pad * 2) / base.value.w,
    (el.clientHeight - pad * 2) / base.value.h,
  );
  const max = surface.value === "hud" ? 2.6 : 1;
  // 「自适应」本身有个上限（HUD 可以放大到 2.6，桌面最多 1:1），用户的缩放再乘上去
  fitScale = Math.max(0.15, Math.min(max, fit));
  applyScale();
}

/** Ctrl + 滚轮缩放预览；以光标位置为锚点，缩放后光标下的点不动 */
function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return;
  e.preventDefault();
  const el = wrap.value;
  const vp = viewport.value;
  if (!el || !vp) return;

  // 记录光标下的内容坐标（缩放前）
  const rect = el.getBoundingClientRect();
  const px = e.clientX - rect.left;
  const py = e.clientY - rect.top;
  const contentX = (px + el.scrollLeft - vp.offsetLeft) / (scale.value || 1);
  const contentY = (py + el.scrollTop - vp.offsetTop) / (scale.value || 1);

  const factor = Math.exp(-e.deltaY * 0.0015);
  const next = Math.max(0.1 / Math.max(fitScale, 0.0001), Math.min(20, zoom.value * factor));
  zoom.value = next;
  applyScale();

  // 缩放后把同一个内容点摆回光标下
  nextTick(() => {
    const vp2 = viewport.value;
    if (!vp2) return;
    el.scrollLeft = contentX * scale.value + vp2.offsetLeft - px;
    el.scrollTop = contentY * scale.value + vp2.offsetTop - py;
  });
}

/** 进入桌面编辑模式：面板变成可交互，设置页先最小化让开桌面 */
async function enterDesktopEdit() {
  try {
    await invoke("set_panel_edit", { enabled: true });
    await invoke("settings_window", { action: "minimize" });
  } catch (e) {
    showNotice("进入桌面编辑失败：" + String(e));
  }
}


/** 空格拖拽平移画布（只在内容溢出时有用） */
function startPan(e: PointerEvent) {
  const el = wrap.value;
  if (!el) return;
  panning.value = true;
  const sx = e.clientX;
  const sy = e.clientY;
  const l0 = el.scrollLeft;
  const t0 = el.scrollTop;
  const move = (ev: PointerEvent) => {
    el.scrollLeft = l0 - (ev.clientX - sx);
    el.scrollTop = t0 - (ev.clientY - sy);
  };
  const up = () => {
    panning.value = false;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  e.preventDefault();
}

/** 松开空格/失焦时退出抓手模式 */
function onKeyUp(e: KeyboardEvent) {
  if (e.code === "Space") spaceHeld.value = false;
}

function onBlur() {
  spaceHeld.value = false;
  panning.value = false;
}

/** 恢复自适应（点击右下角比例文字） */
function resetZoom() {
  zoom.value = 1;
  applyScale();
  nextTick(() => {
    if (wrap.value) {
      wrap.value.scrollLeft = 0;
      wrap.value.scrollTop = 0;
    }
  });
}

function pushUndo() {
  if (!config.value) return;
  undoStack.value.push(JSON.stringify(config.value));
  if (undoStack.value.length > 50) undoStack.value.shift();
}

function undo() {
  const snapshot = undoStack.value.pop();
  if (!snapshot) return;
  if (surface.value === "panel") panel.value = JSON.parse(snapshot);
  else hud.value = JSON.parse(snapshot);
  scheduleSave();
}

function commit(mutate: () => void) {
  pushUndo();
  mutate();
  scheduleSave();
}

function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = window.setTimeout(async () => {
    if (!config.value) return;
    lastSave = Date.now();
    saveState.value = "saving";
    try {
      // 落盘前把几何取整：小数坐标会让融合块的边界和卡片边界差 1px，圆角会错
      const snapshot = JSON.parse(JSON.stringify(config.value));
      for (const c of snapshot.cards) {
        if (typeof c.x === "number") c.x = Math.round(c.x);
        if (typeof c.y === "number") c.y = Math.round(c.y);
        if (typeof c.w === "number") c.w = Math.round(c.w);
        if (typeof c.h === "number") c.h = Math.round(c.h);
      }
      await invoke("save_surface_config", {
        surface: surface.value,
        config: snapshot,
      });
      saveState.value = "saved";
      window.setTimeout(() => {
        if (saveState.value === "saved") saveState.value = "idle";
      }, 1400);
    } catch (e) {
      // 冲突（外部改动过文件）时重新载入，避免覆盖
      console.warn("保存失败", e);
      saveState.value = "error";
      showNotice("配置文件被外部修改过，已重新载入");
      await reloadBootstrap();
    }
  }, 150);
}

async function reloadBootstrap() {
  try {
    const boot = await invoke<Bootstrap>("get_bootstrap");
    panel.value = boot.panel;
    hud.value = boot.hud;
    metrics.value = boot.metrics;
  } catch {}
}

function switchSurface(next: Surface) {
  if (next === surface.value) return;
  surface.value = next;
  selectedId.value = null;
  checkedIds.value = [];
  undoStack.value = [];
  nextTick(recalc);
}

/** 融合块编号：同一 group 用同一个字母，方便区分多个块 */
const groupLetters = computed(() => {
  const map = new Map<string, string>();
  let n = 0;
  for (const c of config.value?.cards ?? []) {
    if (!c.group || map.has(c.group)) continue;
    map.set(c.group, String.fromCharCode(65 + (n++ % 26)));
  }
  return map;
});

function toggleChecked(id: string) {
  checkedIds.value = checkedIds.value.includes(id)
    ? checkedIds.value.filter((x) => x !== id)
    : [...checkedIds.value, id];
}

function fuseSelected() {
  if (!config.value || checkedIds.value.length < 2) return;
  const ids = new Set(checkedIds.value);
  const members = config.value.cards.filter((c) => ids.has(c.id));
  const memberRects = members.map(cardRect);
  for (let i = 0; i < memberRects.length; i++) {
    for (let j = i + 1; j < memberRects.length; j++) {
      if (overlaps(memberRects[i], memberRects[j])) {
        showNotice("所选卡片之间有重叠，请先把它们分开再融合");
        return;
      }
    }
  }
  const block = unionRects(memberRects);
  const blocker = config.value.cards.find((c) => !ids.has(c.id) && overlaps(block, cardRect(c)));
  if (blocker) {
    showNotice(`融合后的块会和「${cardTitle(blocker)}」重叠，先把它挪开再融合`);
    return;
  }

  // 勾选的卡片如果分属多个融合块，这次操作会把它们并成一块
  const prevGroups = new Set(members.map((c) => c.group).filter(Boolean));
  const gid = `g-${Date.now().toString(36)}`;
  commit(() => {
    for (const c of config.value!.cards) {
      if (checkedIds.value.includes(c.id)) c.group = gid;
    }
  });
  selectedId.value = checkedIds.value[0] ?? null;
  // 关键：融合完清空勾选，否则接着勾其它卡片会把这些也一起并进新块
  checkedIds.value = [];
  showNotice(
    prevGroups.size > 1
      ? `已把 ${prevGroups.size} 个融合块合并成一块`
      : `已融合 ${members.length} 张卡片（再勾选其它卡片可以做第二块）`,
  );
}

function unfuseChecked() {
  if (!config.value) return;
  const count = checkedIds.value.length;
  commit(() => {
    for (const c of config.value!.cards) {
      if (checkedIds.value.includes(c.id)) c.group = null;
    }
  });
  checkedIds.value = [];
  showNotice(`已解除 ${count} 张卡片的融合`);
}

async function refreshPlugins() {
  try {
    plugins.value = await invoke<PluginRow[]>("list_plugins");
    diag.value = await invoke<DiagInfo>("get_diag");
  } catch {}
}

/* ---------- 低显存模式 ---------- */

const lowMem = ref(false);
const prefsDirty = ref(false);

async function loadPrefs() {
  try {
    lowMem.value = await invoke<boolean>("get_low_mem");
  } catch {
    // 非 Tauri 环境
  }
}

async function toggleLowMem(value: boolean) {
  try {
    await invoke("set_low_mem", { enabled: value });
    lowMem.value = value;
    prefsDirty.value = true;
    showNotice(value ? "低显存模式已开启，重启后生效" : "低显存模式已关闭，重启后生效");
  } catch (e) {
    showNotice("保存失败：" + String(e));
  }
}

async function restartApp() {
  try {
    await relaunch();
  } catch (e) {
    showNotice("重启失败：" + String(e));
  }
}

/* ---------- 自动更新 ---------- */

interface UpdateInfo {
  version: string;
  current: string;
  notes?: string;
  date?: string;
}

const appVersion = ref("");
/** 手动代理（空 = 跟随系统代理） */
const proxyValue = ref("");

async function loadProxy() {
  try {
    proxyValue.value = await invoke<string>("get_proxy");
  } catch {
    // 非 Tauri 环境
  }
}

async function saveProxy() {
  try {
    await invoke("set_proxy", { proxy: proxyValue.value });
    proxyValue.value = (await invoke<string>("get_proxy")) || "";
    showNotice(proxyValue.value ? `代理已设为 ${proxyValue.value}` : "已改为跟随系统代理");
  } catch (e) {
    showNotice("保存代理失败：" + String(e));
  }
}

const updateInfo = ref<UpdateInfo | null>(null);
const updateChecking = ref(false);
const updateInstalling = ref(false);
const updateProgress = ref(0);
const updateError = ref("");

/** 读当前版本 + Rust 端缓存的检查结果 */
async function loadUpdateState() {
  try {
    appVersion.value = await invoke<string>("app_version");
    updateInfo.value = await invoke<UpdateInfo | null>("update_state");
  } catch {
    // 非 Tauri 环境
  }
}

async function checkUpdates() {
  updateChecking.value = true;
  updateError.value = "";
  try {
    updateInfo.value = await invoke<UpdateInfo | null>("update_check");
    showNotice(
      updateInfo.value ? `发现新版本 v${updateInfo.value.version}` : "已是最新版本",
    );
  } catch (e) {
    updateError.value = String(e);
  } finally {
    updateChecking.value = false;
  }
}

/** 下载并暂存 Update 句柄：下载完再让用户选时机安装（Windows 上 install 会退出应用） */
let pendingUpdate: Awaited<ReturnType<typeof checkUpdater>> = null;
const updateDownloaded = ref(false);

async function downloadUpdate() {
  updateInstalling.value = true;
  updateProgress.value = 0;
  updateError.value = "";
  updateDownloaded.value = false;
  try {
    const proxy = proxyValue.value.trim();
    pendingUpdate = await checkUpdater(proxy ? { proxy } : undefined);
    if (!pendingUpdate) {
      showNotice("没有可用更新");
      return;
    }
    let total = 0;
    let got = 0;
    // download 结束时会自动校验签名，校验失败会 throw
    await pendingUpdate.download(
      (event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          got += event.data.chunkLength;
          if (total > 0) updateProgress.value = Math.round((got * 100) / total);
        }
      },
      proxy ? { proxy } : undefined,
    );
    updateDownloaded.value = true;
    updateProgress.value = 100;
    showNotice("更新已下载并校验通过，可以重启安装");
  } catch (e) {
    updateError.value = String(e);
  } finally {
    updateInstalling.value = false;
  }
}

/** 安装已下载的更新：Windows 上会启动安装器并退出应用 */
async function installDownloaded() {
  if (!pendingUpdate) return;
  try {
    await pendingUpdate.install();
    // 正常情况下已经退出，这里只是兜底
    await relaunch();
  } catch (e) {
    updateError.value = String(e);
  }
}

/* ---------- 设备发现 / 绑定 ---------- */

interface DiscoveredDevice {
  key: string;
  name: string;
  source: string;
  level: number;
  charging: boolean;
  hidden: boolean;
  metrics?: { id: string; title: string; kind: string }[];
}

const devices = ref<DiscoveredDevice[]>([]);
const devicesLoading = ref(false);
const devicesError = ref("");

/** 拉设备列表；rescan = 让插件重新扫一遍（罗技/狼蛛要等回包，会慢一点） */
async function refreshDevices(rescan = false) {
  if (devicesLoading.value) return;
  devicesLoading.value = true;
  devicesError.value = "";
  try {
    const r = await invoke<{ devices: DiscoveredDevice[] }>("get_plugin_devices", {
      plugin: "device-battery",
      refresh: rescan,
    });
    devices.value = r?.devices ?? [];
  } catch (e) {
    devicesError.value = String(e);
  } finally {
    devicesLoading.value = false;
  }
}

/** 显示/隐藏设备：隐藏后该设备不再产生指标 */
async function toggleHidden(d: DiscoveredDevice) {
  const hidden = devices.value.filter((x) => x.hidden && x.key !== d.key).map((x) => x.key);
  if (!d.hidden) hidden.push(d.key);
  try {
    await invoke("set_plugin_config", {
      plugin: "device-battery",
      config: { hidden },
    });
    d.hidden = !d.hidden;
    showNotice(`${d.name} 已${d.hidden ? "隐藏" : "显示"}`);
  } catch (e) {
    showNotice("保存失败：" + String(e));
  }
}

// 第一次切到「插件」页时拉一次设备（扫描要等设备回包，别在启动时就做）
watch(surface, (s) => {
  if ((s === "devices" || s === "plugins") && !devices.value.length && !devicesLoading.value) {
    void refreshDevices(false);
  }
});

function reloadPlugins() {
  invoke("reload_plugins").catch(() => {});
  window.setTimeout(refreshPlugins, 800);
}

function openPluginsDir() {
  invoke("open_plugins_dir").catch(() => {});
}

function quitApp() {
  if (!confirm("确定退出 desktop-monitor 吗？壁纸层会被移除。")) return;
  invoke("quit_app").catch(() => {});
}

function setTheme(id: string) {
  const cfg = config.value;
  if (!cfg) return;
  commit(() => {
    cfg.theme = id;
  });
}

const opacityOf = computed(() => config.value?.opacity ?? 1);

/** 卡片背景不透明度（唯一的视觉旋钮） */
function setOpacity(v: number) {
  const cfg = config.value;
  if (!cfg) return;
  commit(() => {
    cfg.opacity = v;
  });
}

function setMetric(id: string) {
  const card = selected.value;
  if (!card) return;
  commit(() => {
    card.metric = id;
    const def = defOf(id);
    if (def) {
      const first = renderersFor(def.kind)[0];
      if (first) card.view = first.id;
    }
  });
}

function setView(id: string) {
  const card = selected.value;
  if (!card) return;
  commit(() => {
    card.view = id;
  });
}

function setLabelLive(v: string) {
  const card = selected.value;
  if (!card) return;
  card.options = { ...(card.options || {}), label: v };
  scheduleSave();
}

/** 数字卡片的标签/数值排版：auto = 按卡片大小自动，stacked = 上下，inline = 左右 */
function layoutOf(c: any): string {
  const l = c?.options?.layout;
  if (l === "inline" || l === "stacked") return l;
  return c?.options?.stacked === true ? "stacked" : "auto";
}

function setLayout(v: string) {
  const card = selected.value;
  if (!card) return;
  commit(() => {
    const options = { ...(card.options || {}) };
    delete options.stacked; // 旧字段，统一改用 layout
    if (v === "auto") delete options.layout;
    else options.layout = v;
    card.options = options;
  });
}

/** 单块主题：空字符串 = 跟随整体 */
function themeName(id?: string | null): string {
  return THEMES.find((t) => t.id === (id ?? "default-dark"))?.name ?? "默认";
}

/** 这张卡片自己的主题（空 = 跟随所在块 / 整体） */
/** 数字卡片的字号倍率（1 = 默认） */
const fontScaleOf = computed(() => {
  const v = Number((selected.value?.options as Record<string, unknown> | null)?.fontScale);
  return Number.isFinite(v) && v > 0 ? v : 1;
});

function setFontScale(v: number) {
  const card = selected.value;
  if (!card) return;
  commit(() => {
    const opts = { ...((card.options as Record<string, unknown> | null) ?? {}) };
    if (Math.abs(v - 1) < 0.001) delete opts.fontScale;
    else opts.fontScale = Math.round(v * 100) / 100;
    card.options = Object.keys(opts).length ? opts : null;
  });
}

/** 复制的样式：只包含尺寸和主题 */
const copiedStyle = ref<{ w: number; h: number; theme: string | null } | null>(null);

const copiedLabel = computed(() => {
  const st = copiedStyle.value;
  if (!st) return "";
  const t = st.theme ? themeName(st.theme) : "跟随整体";
  return "已复制 " + Math.round(st.w) + "×" + Math.round(st.h) + " · " + t;
});

/** 复制当前卡片的宽高 + 主题 */
function copyStyle() {
  const card = selected.value;
  if (!card) return;
  const r = rendererFor(card.view, selectedDef.value?.kind);
  copiedStyle.value = {
    w: card.w ?? r.defaultSize.w,
    h: card.h ?? r.defaultSize.h,
    theme: card.theme ?? null,
  };
  showNotice(
    "已复制样式：" +
      Math.round(copiedStyle.value.w) +
      "×" +
      Math.round(copiedStyle.value.h) +
      " · " +
      (copiedStyle.value.theme ? themeName(copiedStyle.value.theme) : "跟随整体"),
  );
}

/** 把复制的样式贴到「勾选的卡片」（没勾选就贴给当前卡片） */
function pasteStyle() {
  const copied = copiedStyle.value;
  if (!copied || !config.value) return;
  const cards = config.value.cards;
  const targets = checkedIds.value.length
    ? cards.filter((c) => checkedIds.value.includes(c.id))
    : selected.value
      ? [selected.value]
      : [];
  if (!targets.length) return;

  commit(() => {
    for (const c of targets) {
      c.w = copied.w;
      c.h = copied.h;
      if (copied.theme) c.theme = copied.theme;
      else delete c.theme;
    }
  });

  // 尺寸变大可能压到别的卡片，提示一下（不阻止，位置让用户自己调）
  const ids = new Set(targets.map((c) => c.id));
  const rects = cards.map(cardRect);
  let overlap = false;
  for (let i = 0; i < rects.length && !overlap; i++) {
    for (let j = i + 1; j < rects.length; j++) {
      if (!ids.has(cards[i].id) && !ids.has(cards[j].id)) continue;
      if (overlaps(rects[i], rects[j])) { overlap = true; break; }
    }
  }
  showNotice(
    overlap
      ? "已应用到 " + targets.length + " 张卡片，但和别的卡片有重叠"
      : "已把样式应用到 " + targets.length + " 张卡片",
  );
}

function cardTheme(c: any): string {
  return c?.theme ?? "";
}

function setCardTheme(id: string) {
  const card = selected.value;
  if (!card) return;
  commit(() => {
    // 逐卡设置：融合块里每格可以有自己的主题（拼成一整块马赛克）
    if (id) card.theme = id;
    else delete card.theme;
  });
}

function setNumber(field: "x" | "y" | "w" | "h", raw: string) {
  const card = selected.value as any;
  if (!card) return;
  // 位置坐标可以为 0；宽高给一个小下限（宽 40 / 高 24），HUD 小格子才能缩到 32 甚至更矮
  const min = field === "x" || field === "y" ? 0 : field === "h" ? 24 : 40;
  const n = Math.max(min, Math.round(Number(raw) || 0));
  const blocks = staticBlocks(card.id);
  if (!canMove(card, { [field]: n } as Partial<Rect>, blocks)) {
    showNotice("这个改动会与其他卡片重叠，已忽略");
    return;
  }
  commit(() => {
    card[field] = n;
  });
}

function ensureWindow(): WindowConfig {
  if (!hud.value) throw new Error("HUD 配置未加载");
  if (!hud.value.window) {
    hud.value.window = { x: 48, y: 48, w: 340, h: 132, enabled: true };
  }
  return hud.value.window;
}

function setWindowNumber(field: "x" | "y" | "w" | "h", raw: string) {
  if (!config.value) return;
  const n = Math.round(Number(raw) || 0);
  commit(() => {
    const wc = ensureWindow();
    if (field === "w") wc.w = Math.max(80, n);
    else if (field === "h") wc.h = Math.max(48, n);
    else wc[field] = Math.max(-20000, n);
  });
  nextTick(recalc);
}

function setEnabled(v: boolean) {
  if (!config.value) return;
  commit(() => {
    ensureWindow().enabled = v;
  });
}

const saveText = computed(() =>
  saveState.value === "saving"
    ? "保存中…"
    : saveState.value === "saved"
      ? "已保存"
      : saveState.value === "error"
        ? "保存失败"
        : "",
);

/** 把 HUD 挪回屏幕右上角 */
function resetHudPosition() {
  if (!config.value) return;
  commit(() => {
    const wc = ensureWindow();
    wc.x = Math.max(0, screen.value.w - base.value.w - 40);
    wc.y = 48;
  });
  nextTick(recalc);
}

/**
 * 一键居中：先把所有卡片的外接矩形挪到 (0,0)（去掉拖动留下的空边），
 * 再把整个 HUD 摆到屏幕正中间。
 */
function centerHud() {
  const cfg = config.value;
  if (!cfg) return;
  const cards = cfg.cards ?? [];
  const rects = cards.map(cardRect);
  const minX = rects.length ? Math.min(...rects.map((r) => r.x)) : 0;
  const minY = rects.length ? Math.min(...rects.map((r) => r.y)) : 0;
  commit(() => {
    for (const c of cards) {
      c.x = Math.max(0, Math.round(c.x - minX));
      c.y = Math.max(0, Math.round(c.y - minY));
    }
    const wc = ensureWindow();
    const w = Math.max(40, contentSize.value.w + HUD_PAD_X * 2);
    const h = Math.max(24, contentSize.value.h + HUD_PAD_Y * 2);
    wc.x = Math.round((screen.value.w - w) / 2);
    wc.y = Math.round((screen.value.h - h) / 2);
  });
  nextTick(recalc);
}

/** 自绘标题栏的窗口按钮（无边框窗口没有系统按钮了） */
function winAction(action: "minimize" | "maximize" | "close") {
  invoke("settings_window", { action }).catch(() => {});
}

function findFreeSpot(w: number, h: number): { x: number; y: number } {
  const blocks = staticBlocks("__new__");
  const maxX = surface.value === "panel" ? screen.value.w : Math.max(80, hud.value?.window?.w ?? 340);
  const maxY = surface.value === "panel" ? screen.value.h : Math.max(48, hud.value?.window?.h ?? 140);
  const start = surface.value === "hud" ? 8 : 96;
  for (let y = start; y + h <= maxY; y += 8) {
    for (let x = start; x + w <= maxX; x += 8) {
      if (blocks.every((b) => !overlaps({ x, y, w, h }, b))) return { x, y };
    }
  }
  return { x: start, y: start };
}

function addCard() {
  if (!config.value) return;
  const def = metrics.value.find((m) => !deviceOfMetric.value.has(m.id)) ?? metrics.value[0];
  const id = `card-${Date.now().toString(36)}`;
  const view = def ? renderersFor(def.kind)[0]?.id ?? "number" : "text";
  const size = rendererFor(view, def?.kind).defaultSize;
  const spot = findFreeSpot(size.w, size.h);
  commit(() => {
    config.value!.cards.push({
      id,
      metric: def?.id ?? "sys.clock",
      view,
      x: spot.x,
      y: spot.y,
      w: null,
      h: null,
      options: {},
      style: null,
    });
  });
  selectedId.value = id;
}

function removeSelected() {
  if (!config.value || !selected.value) return;
  const id = selected.value.id;
  commit(() => {
    config.value!.cards = config.value!.cards.filter((c) => c.id !== id);
  });
  selectedId.value = null;
  checkedIds.value = checkedIds.value.filter((x) => x !== id);
}

function onPointerDown(e: PointerEvent) {
  // 按住空格时，整块画布都用来平移，不进入卡片选择
  if (spaceHeld.value) {
    startPan(e);
    return;
  }
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
  const blocks = staticBlocks(card.id);
  pushUndo();

  const startX = e.clientX;
  const startY = e.clientY;
  const ox = card.x ?? 0;
  const oy = card.y ?? 0;
  const r = rendererFor(card.view, defOf(card.metric)?.kind);
  const ow = card.w ?? r.defaultSize.w;
  const oh = card.h ?? r.defaultSize.h;

  let moved = false;
  const move = (ev: PointerEvent) => {
    const dx = (ev.clientX - startX) / scale.value;
    const dy = (ev.clientY - startY) / scale.value;
    // 按住 Alt 临时关闭吸附（和 PPT 一致）
    const alt = ev.altKey;

    if (resizing) {
      // 只有该轴真正移动时才改尺寸，避免竖向拖动把宽度也吸附掉 2px
      // 最小值放宽到 40×24（8px 吸附），这样 HUD 小格子能缩到 32 甚至更矮
      const rawW = Math.abs(dx) < 3 ? ow : ow + dx;
      const rawH = Math.abs(dy) < 3 ? oh : oh + dy;
      const cand: Rect = {
        x: ox,
        y: oy,
        w: Math.max(40, rawW),
        h: Math.max(24, rawH),
      };
      const s = computeSnap(cand, card.id, "resize", alt);
      const snapX = s.guides.some((g) => g.vertical);
      const snapY = s.guides.some((g) => !g.vertical);
      let w = snapX ? cand.w + (s.x - cand.x) : alt ? cand.w : snap(cand.w);
      let h = snapY ? cand.h + (s.y - cand.y) : alt ? cand.h : snap(cand.h);
      w = Math.max(40, w);
      h = Math.max(24, h);
      if (!canMove(card, { w, h }, blocks)) {
        clearGuides();
        return;
      }
      card.w = w;
      card.h = h;
      guides.value = s.guides;
      moved = true;
    } else {
      const cand: Rect = { x: ox + dx, y: oy + dy, w: ow, h: oh };
      const s = computeSnap(cand, card.id, "move", alt);
      const snapX = s.guides.some((g) => g.vertical);
      const snapY = s.guides.some((g) => !g.vertical);
      const nx = Math.max(0, snapX ? s.x : alt ? cand.x : snap(cand.x));
      const ny = Math.max(0, snapY ? s.y : alt ? cand.y : snap(cand.y));
      if (canMove(card, { x: nx, y: ny }, blocks)) {
        card.x = nx;
        card.y = ny;
        guides.value = s.guides;
      } else if (canMove(card, { x: nx, y: card.y }, blocks)) {
        card.x = nx;
        guides.value = s.guides.filter((g) => g.vertical);
      } else if (canMove(card, { x: card.x, y: ny }, blocks)) {
        card.y = ny;
        guides.value = s.guides.filter((g) => !g.vertical);
      } else {
        clearGuides();
        return;
      }
      moved = true;
    }
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    clearGuides();
    if (moved) scheduleSave();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  e.preventDefault();
}

let lastNudge = 0;
function onKey(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
    e.preventDefault();
    undo();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.key === "0") {
    e.preventDefault();
    resetZoom();
    return;
  }
  const t = e.target as HTMLElement | null;
  // 输入框里不抢键盘
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;

  // 空格：进入抓手平移模式（并阻止页面滚动）
  if (e.code === "Space") {
    e.preventDefault();
    spaceHeld.value = true;
    return;
  }

  const card = selected.value;
  if (!card) return;

  if (e.key === "Delete" || e.key === "Backspace") {
    e.preventDefault();
    removeSelected();
    return;
  }

  // 方向键微调：1px；按住 Shift 是 8px（和 PPT 一致）
  const step = e.shiftKey ? 8 : 1;
  const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
  const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
  if (!dx && !dy) return;
  e.preventDefault();

  const blocks = staticBlocks(card.id);
  const x = Math.max(0, (card.x ?? 0) + dx);
  const y = Math.max(0, (card.y ?? 0) + dy);
  if (!canMove(card, { x, y }, blocks)) return;
  // 连续微调合并成一步撤销
  const now = Date.now();
  if (now - lastNudge > 700) pushUndo();
  lastNudge = now;
  card.x = x;
  card.y = y;
  scheduleSave();
}

watch(
  () => [base.value.w, base.value.h, surface.value] as const,
  () => nextTick(recalc),
);

let pluginTimer: number | undefined;
watch(surface, (v) => {
  if (pluginTimer) {
    clearInterval(pluginTimer);
    pluginTimer = undefined;
  }
  if (v === "plugins") {
    refreshPlugins();
    pluginTimer = window.setInterval(refreshPlugins, 2000);
  }
});

onMounted(async () => {
  try {
    unlisten.push(
      await listen<{ ts: number; values: Record<string, Sample> }>("metrics", (e) => {
        // 预览只需要「看一眼」，攒到 1Hz 再刷，省掉 5 倍的整页重绘
        pendingValues = { ...pendingValues, ...e.payload.values };
      }),
    );
    unlisten.push(
      await listen<PanelConfig>("panel-config", (e) => {
        if (Date.now() - lastSave < 800) return;
        panel.value = e.payload;
      }),
    );
    unlisten.push(
      await listen<PanelConfig>("hud-config", (e) => {
        if (Date.now() - lastSave < 800) return;
        hud.value = e.payload;
      }),
    );
    unlisten.push(
      await listen<MetricDef[]>("metric-defs", (e) => {
        metrics.value = e.payload;
      }),
    );
    unlisten.push(
      await listen<UpdateInfo>("update-available", (e) => {
        updateInfo.value = e.payload;
        showNotice(`发现新版本 v${e.payload.version}`);
      }),
    );
    unlisten.push(
      await listen("plugin-status", () => {
        if (surface.value === "plugins") refreshPlugins();
      }),
    );
  } catch {
    // 非 Tauri 环境
  }

  try {
    const boot = await invoke<Bootstrap>("get_bootstrap");
    panel.value = boot.panel;
    hud.value = boot.hud;
    metrics.value = boot.metrics;
    selectedId.value = boot.panel.cards[0]?.id ?? null;
    const size = await invoke<{ w: number; h: number }>("get_screen_size");
    screen.value = size;
  } catch (e) {
    console.error(e);
  }

  previewTimer = window.setInterval(() => {
    if (!Object.keys(pendingValues).length) return;
    values.value = { ...values.value, ...pendingValues };
    pendingValues = {};
  }, 1000);

  if (wrap.value) {
    ro = new ResizeObserver(recalc);
    ro.observe(wrap.value);
  }
  // 指标下拉要按插件分组，先把插件列表（含显示名）取一次
  refreshPlugins();
  // 当前版本 + 后台线程已经检查到的更新
  void loadUpdateState();
  void loadPrefs();
  void loadProxy();
  // 卡片里的指标要知道属于哪台设备，所以启动就拉一次（插件端有缓存，很快）
  void refreshDevices(false);
  recalc();
  window.addEventListener("resize", recalc);
  window.addEventListener("keydown", onKey);
  window.addEventListener("keyup", onKeyUp);
  window.addEventListener("blur", onBlur);
});

onUnmounted(() => {
  if (pluginTimer) clearInterval(pluginTimer);
  if (previewTimer) clearInterval(previewTimer);
  unlisten.forEach((f) => f());
  ro?.disconnect();
  window.removeEventListener("resize", recalc);
  window.removeEventListener("keydown", onKey);
  window.removeEventListener("keyup", onKeyUp);
  window.removeEventListener("blur", onBlur);
});
</script>

<template>
  <div class="settings">
    <!-- 自绘标题栏：整条可拖动，双击最大化 -->
    <header class="titlebar" data-tauri-drag-region>
      <span class="brand">desktop-monitor <em class="sub">设置</em></span>
      <span v-if="saveState !== 'idle'" class="save-badge" :class="saveState">{{ saveText }}</span>
      <div class="win-btns">
        <button class="win-btn" title="最小化" @click="winAction('minimize')">&#x2013;</button>
        <button class="win-btn" title="最大化 / 还原" @click="winAction('maximize')">&#x25A2;</button>
        <button class="win-btn close" title="关闭" @click="winAction('close')">&#x2715;</button>
      </div>
    </header>

    <div class="toolbar">
      <div class="tabs">
        <button class="tab" :class="{ on: surface === 'panel' }" @click="switchSurface('panel')">
          桌面面板
        </button>
        <button class="tab" :class="{ on: surface === 'hud' }" @click="switchSurface('hud')">
          置顶 HUD
        </button>
        <button class="tab" :class="{ on: surface === 'devices' }" @click="switchSurface('devices')">
          设备
        </button>
        <button class="tab" :class="{ on: surface === 'plugins' }" @click="switchSurface('plugins')">
          插件与诊断
        </button>
      </div>

      <label v-if="surface === 'panel' || surface === 'hud'" class="theme">
        <span>主题</span>
        <Dropdown
          class="dd-theme"
          :model-value="config?.theme ?? 'default-dark'"
          :options="themeOptions"
          @update:model-value="setTheme"
        />
      </label>

      <label v-if="surface === 'panel' || surface === 'hud'" class="opacity" title="只有透明度，没有其它视觉效果">
        <span>不透明度</span>
        <input
          type="range"
          min="0.2"
          max="1"
          step="0.05"
          :value="opacityOf"
          @input="setOpacity(Number(($event.target as HTMLInputElement).value))"
        />
        <em>{{ Math.round(opacityOf * 100) }}%</em>
      </label>

      <div class="hint">
        <span v-if="surface === 'panel' || surface === 'hud'" class="hint-text">
          拖动卡片移动 · 拖右下角改尺寸 · 改动即时生效
        </span>
      </div>
      <button class="btn ghost" :disabled="!undoStack.length" title="撤销上一步改动" @click="undo">
        撤销 (Ctrl+Z)
      </button>
    </div>

    <div v-if="surface === 'hud' && config" class="winbar">
      <label class="check">
        <input
          type="checkbox"
          :checked="config.window?.enabled ?? true"
          @change="setEnabled(($event.target as HTMLInputElement).checked)"
        />
        启用 HUD
      </label>
      <span class="sep" />
      <label class="mini">
        X
        <input
          type="number"
          :value="config.window?.x ?? 48"
          @change="setWindowNumber('x', ($event.target as HTMLInputElement).value)"
        />
      </label>
      <label class="mini">
        Y
        <input
          type="number"
          :value="config.window?.y ?? 48"
          @change="setWindowNumber('y', ($event.target as HTMLInputElement).value)"
        />
      </label>
      <span class="auto-size">宽高随内容自动 · {{ base.w }} × {{ base.h }}</span>
      <button class="btn small" title="所有卡片归位到左上角，并把 HUD 摆到屏幕正中" @click="centerHud">
        一键居中
      </button>
      <button class="btn small ghost" title="把 HUD 挪回屏幕右上角" @click="resetHudPosition">
        回到右上角
      </button>
      <span class="hint-inline">在桌面上直接拖动 HUD 也会记住位置</span>
    </div>

    <div v-if="surface === 'panel' || surface === 'hud'" class="body">
      <aside class="list">
        <div class="list-head">
          <span>卡片</span>
          <button class="btn small" @click="addCard">+ 新增</button>
        </div>
        <div
          v-for="c in config?.cards ?? []"
          :key="c.id"
          class="item"
          :class="{ active: c.id === selectedId }"
          @click="selectedId = c.id"
        >
          <label class="check" title="选中用于融合" @click.stop>
            <input
              type="checkbox"
              :checked="checkedIds.includes(c.id)"
              @change="toggleChecked(c.id)"
            />
          </label>
          <span class="dot" />
          <span class="name">{{ cardTitle(c) }}</span>
          <span
            v-if="c.group"
            class="grp"
            :title="'和同字母的卡片共用一块背景（融合块 ' + groupLetters.get(c.group) + '）'"
          >{{ groupLetters.get(c.group) }}</span>
          <span class="view">{{ rendererName(c) }}</span>
        </div>
        <div v-if="checkedIds.length" class="fuse-bar">
          <span class="fuse-count">已选 {{ checkedIds.length }} 个</span>
          <button class="btn small" :disabled="checkedIds.length < 2" @click="fuseSelected">融合</button>
          <button class="btn small" :disabled="!hasCheckedGroup" @click="unfuseChecked">解除融合</button>
          <span class="fuse-hint">勾选 2 张以上再点融合；换一组卡片重复一次就是另一个块</span>
        </div>
        <div v-if="!config?.cards?.length" class="empty">
          还没有卡片，点上面的「+ 新增」加一张
        </div>
      </aside>

      <main
      class="canvas-wrap"
      :class="{ space: spaceHeld, panning }"
      ref="wrap"
      @wheel="onWheel"
    >
        <div
          ref="viewport"
          class="viewport"
          :style="{ width: base.w * scale + 'px', height: base.h * scale + 'px' }"
        >
          <div
            class="screen"
            :class="{ 'hud-screen': surface === 'hud' }"
            :style="[
              {
                width: base.w + 'px',
                height: base.h + 'px',
                transform: `scale(${scale})`,
              },
              themeStyle,
            ]"
            @pointerdown="onPointerDown"
          >
            <Stage
              :cards="config?.cards ?? []"
              :def-of="defOf"
              :values="values"
              :now="now"
              :selected-id="selectedId"
              editable
              :compact="surface === 'hud'"
            />
          </div>
          <button
            v-if="surface === 'panel'"
            class="desk-edit"
            title="在桌面上直接拖动卡片改位置和大小（Esc 完成）"
            @click.stop="enterDesktopEdit"
          >
            桌面编辑
          </button>
          <button
            v-if="zoom !== 1"
            class="zoom-reset"
            title="恢复自适应大小（Ctrl+0）"
            @click.stop="resetZoom"
          >
            适应
          </button>
          <!-- 智能参考线：和 PPT 一样，吸附时显示对齐位置的虚线 -->
          <div class="guides">
            <div
              v-for="(g, i) in guides"
              :key="i"
              class="guide"
              :class="g.vertical ? 'v' : 'h'"
              :style="guideStyle(g)"
            />
          </div>
          <div
            class="screen-label"
            :class="{ zoomed: zoom !== 1 }"
            title="Ctrl + 滚轮缩放；点这里恢复自适应"
            @click="resetZoom"
          >
            {{ base.w }} × {{ base.h }} · {{ Math.round(scale * 100) }}% ·
            {{ surface === "panel" ? "桌面" : "HUD" }}
            <span v-if="zoom !== 1" class="zoom-tag">缩放 {{ Math.round(zoom * 100) }}%</span>
          </div>
        </div>
      </main>

      <aside class="inspector">
        <div v-if="notice" class="notice">{{ notice }}</div>
        <template v-if="selected">
          <div class="insp-title">卡片属性</div>

          <label class="field">
            <span>设备</span>
            <Dropdown
              :model-value="selectedDevice"
              :options="deviceOptions"
              @update:model-value="setDevice"
            />
          </label>

          <label class="field">
            <span>指标</span>
            <Dropdown
              :model-value="selected.metric"
              :options="metricOptions"
              @update:model-value="setMetric"
            />
          </label>

          <label class="field">
            <span>渲染器</span>
            <Dropdown
              :model-value="selected.view"
              :options="rendererOptions"
              @update:model-value="setView"
            />
          </label>

          <label class="field">
            <span>标签</span>
            <input
              :value="selected.options?.label ?? ''"
              placeholder="留空则用指标名"
              @focus="pushUndo()"
              @input="setLabelLive(($event.target as HTMLInputElement).value)"
            />
          </label>

          <label v-if="selected.view === 'number'" class="field">
            <span>字号 · {{ Math.round(fontScaleOf * 100) }}%</span>
            <input
              type="range"
              min="0.6"
              max="2"
              step="0.05"
              :value="fontScaleOf"
              @input="setFontScale(Number(($event.target as HTMLInputElement).value))"
              @dblclick="setFontScale(1)"
            />
            <em class="field-hint">只影响数字字号，卡片大小不变；双击恢复 100%</em>
          </label>

          <label v-if="selected.view === 'number'" class="field">
            <span>排版</span>
            <Dropdown
              :model-value="layoutOf(selected)"
              :options="layoutOptions"
              @update:model-value="setLayout"
            />
          </label>

          <label class="field">
            <span>主题</span>
            <Dropdown
              :model-value="cardTheme(selected)"
              :options="cardThemeOptions"
              @update:model-value="setCardTheme"
            />
          </label>

          <div class="grid2">
            <label class="field">
              <span>X</span>
              <input
                type="number"
                min="0"
                step="1"
                :value="selected.x"
                @change="setNumber('x', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field">
              <span>Y</span>
              <input
                type="number"
                min="0"
                step="1"
                :value="selected.y"
                @change="setNumber('y', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field">
              <span>宽 <em>px</em></span>
              <input
                type="number"
                min="40"
                step="8"
                :value="selected.w ?? rendererFor(selected.view, selectedDef?.kind).defaultSize.w"
                @change="setNumber('w', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field">
              <span>高 <em>px</em></span>
              <input
                type="number"
                min="24"
                step="8"
                :value="selected.h ?? rendererFor(selected.view, selectedDef?.kind).defaultSize.h"
                @change="setNumber('h', ($event.target as HTMLInputElement).value)"
              />
            </label>
          </div>

          <div class="style-row">
            <button class="btn small" title="记住这张卡片的宽高和主题" @click="copyStyle">
              复制样式
            </button>
            <button
              class="btn small"
              :disabled="!copiedStyle"
              :title="checkedIds.length ? '贴到左侧勾选的卡片' : '贴到当前卡片；先在左侧勾选多张可以一次贴一批'"
              @click="pasteStyle"
            >
              粘贴样式{{ checkedIds.length ? " → 已选 " + checkedIds.length + " 张" : "" }}
            </button>
          </div>
          <div v-if="copiedLabel" class="style-hint">{{ copiedLabel }}</div>

          <button class="btn danger" title="删掉这张卡片（可 Ctrl+Z 撤销）" @click="removeSelected">
            删除卡片
          </button>
        </template>
        <div v-else class="insp-empty">在左侧列表或画布上点一张卡片来编辑</div>
      </aside>
    </div>

    <div v-else-if="surface === 'devices'" class="devices-page">
      <div class="plug-head">
        <div class="plug-title">已发现的设备</div>
        <div class="plug-actions">
          <button class="btn small" :disabled="devicesLoading" @click="refreshDevices(true)">
            {{ devicesLoading ? "扫描中…" : "重新扫描" }}
          </button>
        </div>
      </div>
      <p class="dev-hint">
        设备只负责「提供数据」；指标要绑定到某台设备才会出现在卡片里。
        隐藏一台设备后，它的指标会从指标列表里消失（已有卡片保留，显示 --
        ）。无线设备休眠时列表里会暂时消失，按一下设备再点「重新扫描」。
      </p>
      <div class="dev-list">
        <div v-for="d in devices" :key="d.key" class="dev-row" :class="{ off: d.hidden }">
          <span class="dev-name">{{ d.name }}</span>
          <span class="dev-src">{{ d.source }}</span>
          <span class="dev-level">{{ d.level }}%{{ d.charging ? " · 充电中" : "" }}</span>
          <span class="dev-metrics">{{ (d.metrics || []).length }} 个指标</span>
          <button class="btn small" :class="{ hidden: d.hidden }" @click="toggleHidden(d)">
            {{ d.hidden ? "已隐藏" : "显示中" }}
          </button>
        </div>
        <div v-if="!devices.length && !devicesLoading" class="empty">
          没有发现带电量的设备（键盘休眠时请按一下按键再扫描）
        </div>
        <div v-else-if="devicesLoading && !devices.length" class="empty">正在扫描设备…</div>
      </div>
      <div v-if="devicesError" class="empty">{{ devicesError }}</div>
    </div>

    <div v-else class="plugins">
      <div class="plug-head" style="margin-top: 22px">
        <div class="plug-title">插件</div>
        <div class="plug-actions">
          <button class="btn small" @click="refreshPlugins">刷新</button>
          <button class="btn small" @click="reloadPlugins">重新加载全部</button>
          <button class="btn small" @click="openPluginsDir">打开插件目录</button>
        </div>
      </div>

      <div class="plug-list">
        <div v-for="p in plugins" :key="p.id" class="plug-row">
          <div class="plug-name">
            <span class="pname" :title="p.id">{{ p.name }}</span>
            <span class="pid">v{{ p.version || "—" }}</span>
          </div>
          <div class="plug-status" :class="p.status">{{ STATUS_TEXT[p.status] ?? p.status }}</div>
          <div class="plug-metrics">{{ p.metrics }} 个指标</div>
          <div class="plug-detail" :title="p.detail ?? ''">{{ p.detail || "" }}</div>
        </div>
        <div v-if="!plugins.length" class="empty">
          插件目录里还没有插件，把插件文件夹复制进去就会自动加载
        </div>
      </div>

      <div class="plug-title" style="margin-top: 28px">性能</div>
      <label class="perf-row">
        <input
          type="checkbox"
          :checked="lowMem"
          @change="toggleLowMem(($event.target as HTMLInputElement).checked)"
        />
        <div class="perf-text">
          <div class="perf-title">低显存模式</div>
          <div class="perf-desc">
            关闭 WebView2 的 GPU 渲染与合成：GPU 进程私有内存 ~127MB → ~27MB、显存 ~104MB → 0。
            代价是界面变成软件渲染（文字/动画轻微变糙），CPU 实测基本不变。
          </div>
          <div v-if="prefsDirty" class="perf-warn">需要重启应用后生效</div>
        </div>
        <button v-if="prefsDirty" class="btn small" @click="restartApp">立即重启</button>
      </label>

      <div class="plug-title" style="margin-top: 28px">更新</div>
      <div class="upd-row">
        <div class="upd-info">
          <div class="upd-line">当前版本 <b>v{{ appVersion || "—" }}</b></div>
          <div v-if="updateInfo" class="upd-line upd-new">
            新版本 v{{ updateInfo.version }}
            <span v-if="updateInfo.date"> · {{ String(updateInfo.date).slice(0, 10) }}</span>
          </div>
          <div v-else class="upd-line upd-dim">已是最新版本</div>
          <div v-if="updateInstalling" class="upd-line">下载中 {{ updateProgress }}%</div>
          <div v-if="updateError" class="upd-line upd-err">{{ updateError }}</div>
        </div>
        <div class="upd-actions">
          <button
            class="btn small"
            :disabled="updateChecking || updateInstalling"
            @click="checkUpdates"
          >
            {{ updateChecking ? "检查中…" : "检查更新" }}
          </button>
          <button
            v-if="updateInfo && !updateDownloaded"
            class="btn small primary"
            :disabled="updateInstalling"
            @click="downloadUpdate"
          >
            {{ updateInstalling ? "下载中 " + updateProgress + "%" : "下载更新" }}
          </button>
          <button
            v-if="updateDownloaded"
            class="btn small primary"
            @click="installDownloaded"
          >
            重启并安装
          </button>
        </div>
      </div>
      <label class="proxy-row">
        <span>代理</span>
        <input
          type="text"
          placeholder="留空 = 跟随系统代理，例如 http://127.0.0.1:10809"
          :value="proxyValue"
          @input="proxyValue = ($event.target as HTMLInputElement).value"
          @blur="saveProxy"
          @keydown.enter="saveProxy"
        />
      </label>

      <div class="plug-title" style="margin-top: 28px">诊断</div>
      <div class="diag">
        <div class="diag-row"><span>配置文件目录</span><code>{{ diag?.configDir ?? "—" }}</code></div>
        <div class="diag-row"><span>插件目录</span><code>{{ diag?.pluginsDir ?? "—" }}</code></div>
        <div class="diag-row"><span>指标总数</span><code>{{ diag?.metrics ?? "—" }}</code></div>
        <div class="diag-row"><span>版本</span><code>{{ diag?.version ?? "—" }}</code></div>
      </div>
      <button class="btn danger quit" @click="quitApp">退出 desktop-monitor</button>
      <p class="plug-hint">
        插件目录支持自动加载：复制进去约 1 秒后出现在列表里，删除后对应指标会自动清理。
      </p>
    </div>
  </div>
</template>

<style scoped>
.settings {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  color: var(--text);
  /* 让原生控件（下拉弹层、滚动条）跟着走暗色，不然展开的白底列表很跳 */
  color-scheme: dark;
  background: #141418;
  /* 无边框窗口：Win11 自己会给圆角和投影，这里只补一条内描边让边缘更清楚 */
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
}

/* 自绘标题栏 */
.titlebar {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 40px;
  padding: 0 6px 0 16px;
  flex: none;
  user-select: none;
}
/* 整条都能拖动：子元素不接收指针事件，只有窗口按钮例外 */
.titlebar > * {
  pointer-events: none;
}
.titlebar .win-btns {
  pointer-events: auto;
}

.brand {
  font-size: 14px;
  font-weight: 600;
  letter-spacing: 0.2px;
}

.brand .sub {
  color: var(--text-dim);
  font-weight: 400;
  font-style: normal;
  margin-left: 5px;
}

.win-btns {
  margin-left: auto;
  display: flex;
  gap: 2px;
}

.win-btn {
  width: 40px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  border-radius: 6px;
  color: var(--text-dim);
  font-size: 12px;
  line-height: 1;
  cursor: default;
}

.win-btn:hover {
  background: rgba(255, 255, 255, 0.09);
  color: var(--text);
}

.win-btn.close:hover {
  background: #d33a3a;
  color: #fff;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 0 16px 10px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.07);
  flex-wrap: wrap;
}

.tabs {
  display: flex;
  gap: 4px;
  background: rgba(255, 255, 255, 0.05);
  border-radius: 9px;
  padding: 3px;
}

.tab {
  background: transparent;
  border: none;
  color: var(--text-dim);
  border-radius: 7px;
  padding: 6px 12px;
  font-size: 12px;
  cursor: pointer;
}

.tab.on {
  background: rgba(90, 200, 255, 0.16);
  color: #cfefff;
}

.theme {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-dim);
}

.theme select {
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  color: var(--text);
  padding: 6px 9px;
  font-size: 12px;
  outline: none;
}

.style-row {
  display: flex;
  gap: 8px;
  margin: 4px 0 2px;
}

.style-hint {
  font-size: 10px;
  color: var(--text-dim);
  margin-bottom: 10px;
}

.field-hint {
  font-style: normal;
  font-size: 10px;
  color: var(--text-dim);
  line-height: 1.35;
}

.opacity {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-dim);
}

.opacity input[type="range"] {
  width: 110px;
  accent-color: #5ac8ff;
}

.opacity em {
  font-style: normal;
  font-variant-numeric: tabular-nums;
  color: var(--text);
  min-width: 34px;
  text-align: right;
}

.hint {
  font-size: 12px;
  color: var(--text-dim);
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 10px;
}

.hint-text {
  white-space: nowrap;
}

.save-badge {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  border: 1px solid transparent;
  animation: badge-in 0.16s ease-out;
}
.save-badge.saving {
  color: var(--text-dim);
  border-color: rgba(255, 255, 255, 0.12);
}
.save-badge.saved {
  color: #6ee7a8;
  border-color: rgba(110, 231, 168, 0.32);
  background: rgba(110, 231, 168, 0.08);
}
.save-badge.error {
  color: #ff9b9b;
  border-color: rgba(255, 107, 107, 0.35);
  background: rgba(255, 107, 107, 0.08);
}
@keyframes badge-in {
  from {
    opacity: 0;
    transform: translateY(-2px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.winbar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 20px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  font-size: 12px;
  color: var(--text-dim);
  flex-wrap: wrap;
}

.check {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text);
}

.sep {
  width: 1px;
  height: 16px;
  background: rgba(255, 255, 255, 0.12);
}

.mini {
  display: flex;
  align-items: center;
  gap: 6px;
}

.mini input {
  width: 64px;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  color: var(--text);
  padding: 4px 6px;
  font-size: 12px;
  outline: none;
}

.auto-size {
  font-size: 11px;
  color: var(--text-dim);
}

.hint-inline {
  margin-left: auto;
  font-size: 11px;
  opacity: 0.75;
}

.body {
  flex: 1;
  display: grid;
  grid-template-columns: 240px 1fr 300px;
  min-height: 0;
}

.list {
  border-right: 1px solid rgba(255, 255, 255, 0.07);
  overflow-y: auto;
  padding: 12px;
}

.list-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 12px;
  color: var(--text-dim);
  margin-bottom: 10px;
}

.item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 13px;
}

.item .check {
  display: flex;
  align-items: center;
  cursor: pointer;
}

.item .check input {
  width: 14px;
  height: 14px;
  margin: 0;
  accent-color: var(--accent);
  cursor: pointer;
}

.item .grp {
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 4px;
  color: #bfe9ff;
  background: rgba(90, 200, 255, 0.16);
  flex: none;
}

.fuse-hint {
  font-size: 11px;
  color: var(--text-dim);
  margin-left: auto;
}

.fuse-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 8px 2px 4px;
  padding: 8px 10px;
  border-radius: 8px;
  background: rgba(90, 200, 255, 0.08);
  border: 1px dashed rgba(90, 200, 255, 0.3);
  font-size: 12px;
  color: var(--text-dim);
}

.fuse-count {
  flex: 1;
}

.item:hover {
  background: rgba(255, 255, 255, 0.05);
}

.item.active {
  background: rgba(90, 200, 255, 0.14);
  color: #cfefff;
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-dim);
  flex: none;
}

.item.active .dot {
  background: var(--accent);
}

.name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.view {
  font-size: 11px;
  color: var(--text-dim);
}

.canvas-wrap {
  position: relative;
  /* 缩放后需要能滚动查看；用 margin:auto 居中（flex 居中 + 溢出会裁掉左侧） */
  overflow: auto;
  padding: 32px;
  box-sizing: border-box;
  display: flex;
  background:
    radial-gradient(circle at 50% 0%, rgba(90, 200, 255, 0.06), transparent 60%),
    #101014;
}


.viewport {
  position: relative;
  flex: none;
  margin: auto;
}

/* 按住空格：整块画布变成抓手 */
.canvas-wrap.space,
.canvas-wrap.space * {
  cursor: grab !important;
}

.canvas-wrap.panning,
.canvas-wrap.panning * {
  cursor: grabbing !important;
}

/* 滚动条低调一点：细、半透明，悬停才明显 */
.canvas-wrap::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}

.canvas-wrap::-webkit-scrollbar-track {
  background: transparent;
}

.canvas-wrap::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.07);
  border: 3px solid transparent;
  background-clip: padding-box;
  border-radius: 8px;
}

.canvas-wrap:hover::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
  border: 3px solid transparent;
  background-clip: padding-box;
}

.canvas-wrap::-webkit-scrollbar-corner {
  background: transparent;
}

.screen {
  position: absolute;
  top: 0;
  left: 0;
  transform-origin: top left;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 12px;
  box-sizing: border-box;
}

.screen.hud-screen {
  padding: 4px;
}

.screen-label {
  position: absolute;
  right: 12px;
  bottom: 8px;
  font-size: 11px;
  color: var(--text-dim);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
}

.screen-label:hover {
  color: var(--text);
}

.screen-label.zoomed {
  color: var(--accent);
}

.desk-edit {
  position: absolute;
  top: 8px;
  left: 96px;
  z-index: 7;
  padding: 3px 8px;
  border-radius: 6px;
  border: 1px solid rgba(90, 200, 255, 0.45);
  background: rgba(90, 200, 255, 0.14);
  color: #9fd8ff;
  font-size: 11px;
  cursor: pointer;
}

.desk-edit:hover {
  background: rgba(90, 200, 255, 0.26);
}


.zoom-reset {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 7;
  padding: 3px 8px;
  border-radius: 6px;
  border: 1px solid rgba(90, 200, 255, 0.45);
  background: rgba(18, 20, 26, 0.82);
  color: #9fd8ff;
  font-size: 11px;
  cursor: pointer;
}

.zoom-reset:hover {
  background: rgba(90, 200, 255, 0.18);
}

.screen-label .zoom-tag {
  margin-left: 6px;
  padding: 0 4px;
  border-radius: 4px;
  background: rgba(90, 200, 255, 0.16);
}

/* 智能参考线：铺在预览之上，不挡鼠标 */
.guides {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 6;
}

.guides .guide {
  position: absolute;
  opacity: 0.95;
}

.guides .guide.v {
  border-left: 1px dashed #ff5b8a;
  box-shadow: 0 0 6px rgba(255, 91, 138, 0.55);
}

.guides .guide.h {
  border-top: 1px dashed #ff5b8a;
  box-shadow: 0 0 6px rgba(255, 91, 138, 0.55);
}

.inspector {
  border-left: 1px solid rgba(255, 255, 255, 0.07);
  padding: 16px;
  overflow-y: auto;
  overflow-x: hidden;
  box-sizing: border-box;
}

.insp-title {
  font-size: 12px;
  color: var(--text-dim);
  margin-bottom: 12px;
}

.notice {
  margin-bottom: 12px;
  padding: 8px 10px;
  border-radius: 8px;
  font-size: 12px;
  line-height: 1.4;
  color: #ffe39a;
  background: rgba(255, 209, 102, 0.12);
  border: 1px solid rgba(255, 209, 102, 0.3);
}

.insp-empty,
.empty {
  font-size: 12px;
  color: var(--text-dim);
  padding: 8px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 12px;
  font-size: 12px;
  color: var(--text-dim);
}

.field em {
  font-style: normal;
  opacity: 0.6;
  font-size: 11px;
  margin-left: 2px;
}

.field input,
.field select {
  width: 100%;
  box-sizing: border-box;
  min-width: 0;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  color: var(--text);
  padding: 7px 9px;
  font-size: 13px;
  outline: none;
}

.field input:focus,
.field select:focus {
  border-color: var(--accent);
}

.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  min-width: 0;
}

.grid2 .field {
  min-width: 0;
}

.btn {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: var(--text);
  border-radius: 8px;
  padding: 7px 12px;
  font-size: 12px;
  cursor: pointer;
}

.btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.14);
}

.btn:disabled {
  opacity: 0.4;
  cursor: default;
}

.btn.small {
  padding: 4px 8px;
  font-size: 11px;
}

.btn.ghost {
  background: transparent;
}

.btn.danger {
  width: 100%;
  margin-top: 6px;
  color: #ffb4b4;
  border-color: rgba(255, 107, 107, 0.35);
  background: rgba(255, 107, 107, 0.1);
}

.plugins {
  flex: 1;
  overflow-y: auto;
  padding: 20px 24px;
  box-sizing: border-box;
}

.proxy-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 8px;
  font-size: 11px;
  color: var(--text-dim);
}

.proxy-row span {
  flex: none;
  width: 28px;
}

.proxy-row input {
  flex: 1;
  min-width: 0;
  padding: 6px 9px;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: rgba(0, 0, 0, 0.25);
  color: var(--text);
  font-size: 11px;
}

.proxy-row input::placeholder {
  color: var(--text-dim);
}

.perf-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 14px;
  border: 1px solid rgba(255, 255, 255, 0.07);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.03);
  cursor: pointer;
}

.perf-row input {
  margin-top: 3px;
  flex: none;
}

.perf-text {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  flex: 1;
}

.perf-title {
  font-size: 12px;
  color: var(--text);
}

.perf-desc {
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-dim);
}

.perf-warn {
  font-size: 11px;
  color: var(--warn);
}

.upd-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 14px;
  border: 1px solid rgba(255, 255, 255, 0.07);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.03);
}

.upd-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.upd-line {
  font-size: 12px;
  color: var(--text);
}

.upd-line b {
  color: var(--text-strong);
  font-weight: 600;
}

.upd-new {
  color: #9fd8ff;
}

.upd-dim {
  color: var(--text-dim);
}

.upd-err {
  color: var(--bad);
  word-break: break-all;
}

.upd-actions {
  display: flex;
  gap: 8px;
  flex: none;
}

.dev-hint {
  margin: 0 0 10px;
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-dim);
}

.dev-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 6px;
}

.dev-row {
  display: grid;
  grid-template-columns: minmax(110px, 1fr) 60px 92px 56px 72px;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.07);
  background: rgba(255, 255, 255, 0.03);
}

.dev-row.off {
  opacity: 0.55;
}

.dev-metrics {
  font-size: 10px;
  color: var(--text-dim);
}

.dev-row.off .dev-name {
  text-decoration: line-through;
}

.btn.small.hidden {
  color: var(--text-dim);
}

.dev-name {
  font-size: 12px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dev-src {
  font-size: 10px;
  color: var(--text-dim);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.dev-level {
  font-size: 12px;
  color: var(--text-strong);
  font-variant-numeric: tabular-nums;
}

.btn.small.bound {
  color: #9fd8ff;
  border-color: rgba(90, 200, 255, 0.5);
  background: rgba(90, 200, 255, 0.14);
}

.plug-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.plug-title {
  font-size: 12px;
  color: var(--text-dim);
}

.plug-actions {
  display: flex;
  gap: 8px;
}

.plug-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.plug-row {
  display: grid;
  grid-template-columns: minmax(220px, 1.2fr) 90px 90px minmax(200px, 2fr);
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.06);
  font-size: 13px;
}

.plug-name {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.pname {
  font-weight: 600;
}

.pid {
  font-size: 11px;
  color: var(--text-dim);
}

.plug-status {
  font-size: 12px;
  padding: 3px 8px;
  border-radius: 999px;
  text-align: center;
  background: rgba(255, 255, 255, 0.06);
  color: var(--text-dim);
  white-space: nowrap;
}

.plug-status.running {
  color: #9ff0bf;
  background: rgba(70, 224, 138, 0.14);
}

.plug-status.failed,
.plug-status.disabled {
  color: #ffb4b4;
  background: rgba(255, 107, 107, 0.14);
}

.plug-status.starting,
.plug-status.restarting {
  color: #ffe39a;
  background: rgba(255, 209, 102, 0.14);
}

.plug-metrics {
  font-size: 12px;
  color: var(--text-dim);
}

.plug-detail {
  font-size: 12px;
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.diag {
  margin-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: 900px;
}

.diag-row {
  display: flex;
  align-items: baseline;
  gap: 16px;
  font-size: 12px;
  color: var(--text-dim);
}

.diag-row span {
  width: 90px;
  flex: none;
}

.diag-row code {
  color: var(--text);
  font-family: Consolas, "Cascadia Mono", monospace;
  font-size: 12px;
  background: rgba(255, 255, 255, 0.05);
  padding: 3px 8px;
  border-radius: 6px;
  overflow-wrap: anywhere;
}

.btn.quit {
  width: 220px;
  margin-top: 18px;
}

.plug-hint {
  margin-top: 18px;
  font-size: 12px;
  color: var(--text-dim);
  max-width: 720px;
  line-height: 1.6;
}
</style>
