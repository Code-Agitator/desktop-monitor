<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import Stage from "./Stage.vue";
import { contentBounds, HUD_PAD_X, HUD_PAD_Y } from "./hudLayout";
import { applyTheme } from "./themes";
import type { Bootstrap, MetricDef, PanelConfig, Sample } from "./types";

const config = ref<PanelConfig | null>(null);
const metrics = ref<MetricDef[]>([]);
const values = ref<Record<string, Sample>>({});
const now = ref(Date.now());

const unlisten: UnlistenFn[] = [];

onMounted(async () => {
  try {
    unlisten.push(
      await listen<{ ts: number; values: Record<string, Sample> }>("metrics", (e) => {
        values.value = { ...values.value, ...e.payload.values };
      }),
    );
    unlisten.push(
      await listen<PanelConfig>("hud-config", (e) => {
        config.value = e.payload;
        applyTheme(document.documentElement, e.payload.theme);
      }),
    );
    unlisten.push(
      await listen<MetricDef[]>("metric-defs", (e) => {
        metrics.value = e.payload;
      }),
    );
  } catch {
    // 非 Tauri 环境（浏览器预览）
  }

  try {
    const boot = await invoke<Bootstrap>("get_bootstrap");
    config.value = boot.hud;
    metrics.value = boot.metrics;
    applyTheme(document.documentElement, boot.hud?.theme);
  } catch (e) {
    console.error(e);
  }
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

/** 右键不弹浏览器菜单，改用和托盘一致的菜单 */
const onContextMenu = (e: MouseEvent) => {
  e.preventDefault();
  invoke("popup_hud_menu").catch(() => {});
};
window.addEventListener("contextmenu", onContextMenu);

onUnmounted(() => {
  unlisten.forEach((f) => f());
  clearInterval(nowTimer);
  window.removeEventListener("contextmenu", onContextMenu);
});

function defOf(id: string): MetricDef | undefined {
  return metrics.value.find((m) => m.id === id);
}

/** 卡片外接矩形（含渲染器默认尺寸），用于窗口自适应 */
const contentSize = computed(() => contentBounds(config.value?.cards ?? [], defOf));

let resizeTimer: number | undefined;
watch(
  [
    contentSize,
    () => config.value?.window?.w,
    () => config.value?.window?.h,
  ],
  () => {
    const cfg = config.value;
    if (!cfg) return;
    const w = Math.max(40, contentSize.value.w + HUD_PAD_X * 2);
    const h = Math.max(24, contentSize.value.h + HUD_PAD_Y * 2);
    if (Math.abs((cfg.window?.w ?? 0) - w) < 1 && Math.abs((cfg.window?.h ?? 0) - h) < 1) return;
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = window.setTimeout(() => {
      invoke("resize_hud", { width: w, height: h }).catch(() => {});
    }, 80);
  },
);

</script>

<template>
  <div class="hud" data-tauri-drag-region>
    <!-- 绝对定位的卡片是相对「定位祖先的 padding box」定位的，padding 不会把它们往里推，
         所以留白用一个 inset 的内层容器来做，窗口尺寸（内容 + 2×留白）才对得上 -->
    <!-- Tauri 的拖拽只认「鼠标事件 target 自己带 data-tauri-drag-region」，
         而内层容器盖住了整块区域，所以属性必须也挂在这一层上 -->
    <div
      class="hud-inner"
      data-tauri-drag-region
      :style="{ inset: HUD_PAD_Y + 'px ' + HUD_PAD_X + 'px' }"
    >
      <Stage
        :cards="config?.cards ?? []"
        :def-of="defOf"
        :values="values"
        :now="now"
        compact
      />
    </div>
  </div>
</template>

<style scoped>
.hud {
  position: absolute;
  inset: 0;
  border-radius: 12px;
  box-sizing: border-box;
}
.hud-inner {
  position: absolute;
}


</style>
