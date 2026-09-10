<!--
  src/float/FloatBar.vue
  悬浮指标条窗口（v0.7.6，2026-09-10）：透明置顶无边框 webview，1Hz 显示系统指标。

  设计：
  - 由 App.vue 按 webview label === 'float' 分流渲染（与 pet / pet-menu 同模式）
  - onMounted 局部把 html/body/#app 背景设为 transparent（学 PetMenuWindow.vue，
    只动本 webview 文档，绝不用全局 CSS，避免污染主窗口——见 beta.17 回归根因）
  - 数据：1Hz invoke get_status_bar_config + get_system_metrics（与托盘采样独立，
    多一组轻量 IPC，代价可忽略）；show_cpu / show_net / show_mem 与托盘共用配置
  - 拖拽：整条都是拖拽区，指针按下即 startDragging()（OS 接管零延迟）；结束后读
    outerPosition/scaleFactor 换算逻辑坐标写 localStorage（float_pos），下次挂载恢复
  - 显隐由 Rust 侧管理（托盘菜单/设置页开关 + 全屏自动隐藏），本组件只负责渲染与位置
-->
<template>
  <div class="float-bar" @pointerdown="onPointerDown" @contextmenu.prevent>
    <span class="f-dot"></span>
    <span v-if="cfg.show_cpu" class="f-metric">CPU {{ Math.round(metrics?.cpu_usage ?? 0) }}%</span>
    <span v-if="cfg.show_net" class="f-metric">
      ↓{{ fmtRate(metrics?.net_rx_bps ?? 0) }} ↑{{ fmtRate(metrics?.net_tx_bps ?? 0) }}
    </span>
    <span v-if="isMac && cfg.show_mem" class="f-metric">M {{ Math.round(metrics?.memory_usage ?? 0) }}%</span>
    <span v-if="!hasAny" class="f-metric f-dim">ScreenTime Pro</span>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
// Tauri v2：monitor API 为模块级函数（Window 实例上没有 currentMonitor）
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { tracker } from "../api/tracker";
import type { MetricsOut, StatusBarConfig } from "../types";

/** 与 float_window.rs inner_size 一致（逻辑像素），默认位计算用 */
const BAR_W = 340;
const POS_KEY = "float_pos";

const isMac = /Mac|iPhone|iPod|iPad/i.test(navigator.platform || navigator.userAgent || "");
const cfg = ref<StatusBarConfig>({
  enabled: false,
  show_cpu: true,
  show_mem: true,
  show_net: true,
  float_enabled: true,
});
const metrics = ref<MetricsOut | null>(null);
let timer: number | null = null;

const hasAny = computed(
  () => cfg.value.show_cpu || cfg.value.show_net || (isMac && cfg.value.show_mem),
);

/** 字节/秒格式化（与 Rust format_bps 同口径：B / K / M / G 自适应） */
function fmtRate(bps: number): string {
  if (!Number.isFinite(bps) || bps < 0) return "0B";
  if (bps < 1024) return `${Math.round(bps)}B`;
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(bps < 10240 ? 1 : 0)}K`;
  if (bps < 1024 * 1024 * 1024) return `${(bps / 1048576).toFixed(1)}M`;
  return `${(bps / 1073741824).toFixed(1)}G`;
}

/** 恢复上次位置；无记录则放主屏右上角（顶 24px / 右 24px） */
async function applyPosition(): Promise<void> {
  const raw = localStorage.getItem(POS_KEY);
  if (raw) {
    try {
      const p = JSON.parse(raw) as { x: number; y: number };
      await invoke("move_float_window", { x: p.x, y: p.y });
      return;
    } catch {
      /* 恢复失败落默认位 */
    }
  }
  try {
    const mon = await currentMonitor();
    const sf = mon?.scaleFactor ?? 1;
    const logicalW = (mon?.size.width ?? 1920) / sf;
    await invoke("move_float_window", {
      x: Math.max(0, logicalW - BAR_W - 24),
      y: 24,
    });
  } catch {
    /* 保持创建默认位置 */
  }
}

/** 整条拖拽：按下即交给 OS（跟手），结束后读窗口位置持久化 */
async function onPointerDown(e: PointerEvent): Promise<void> {
  if (e.button !== 0) return;
  e.preventDefault();
  const win = getCurrentWindow();
  try {
    await win.startDragging();
  } catch (err) {
    console.warn("[float] 原生拖拽不可用", err);
    return;
  }
  try {
    const phys = await win.outerPosition();
    const sf = await win.scaleFactor();
    const pos = { x: Math.round(phys.x / sf), y: Math.round(phys.y / sf) };
    localStorage.setItem(POS_KEY, JSON.stringify(pos));
  } catch {
    /* 拖拽成功但记录失败：下次拖动会再写 */
  }
}

onMounted(async () => {
  // 局部透明（学 PetMenuWindow.vue：只改本 webview 的 html/body/#app，不用全局 CSS）
  const els = [document.documentElement, document.body, document.getElementById("app")].filter(
    (el): el is HTMLElement => el != null,
  );
  for (const el of els) {
    el.style.background = "transparent";
    el.style.margin = "0";
    el.style.padding = "0";
    el.style.overflow = "hidden";
  }
  await applyPosition();
  // 就绪后再显示（Rust 启动/开关路径也会 show，幂等；透明空窗 show 本就不可见）
  try {
    await invoke("show_float_window");
  } catch {
    /* 已显示或窗口未创建 */
  }
  const tick = async () => {
    try {
      cfg.value = await tracker.getStatusBarConfig();
      metrics.value = await tracker.getSystemMetrics();
    } catch {
      /* 应用退出中偶发失败，忽略 */
    }
  };
  void tick();
  timer = window.setInterval(tick, 1000);
});
onBeforeUnmount(() => {
  if (timer !== null) clearInterval(timer);
});
</script>

<style scoped>
.float-bar {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 16px;
  border-radius: 22px;
  background: rgba(20, 22, 28, 0.72);
  -webkit-backdrop-filter: blur(14px);
  backdrop-filter: blur(14px);
  border: 1px solid rgba(255, 255, 255, 0.14);
  color: #f5f5f7;
  font-size: 12.5px;
  font-weight: 600;
  cursor: grab;
  user-select: none;
  white-space: nowrap;
  overflow: hidden;
}
.float-bar:active {
  cursor: grabbing;
}
.f-dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--accent, #ff7e27);
  box-shadow: 0 0 6px rgba(255, 126, 39, 0.8);
}
.f-metric {
  line-height: 1;
}
.f-dim {
  opacity: 0.72;
  font-weight: 500;
}
</style>
