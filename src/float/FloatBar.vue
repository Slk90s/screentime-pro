<!--
  src/float/FloatBar.vue
  悬浮指标条窗口（v0.7.6 引入，v0.7.7 重做）：透明置顶无边框 webview，1Hz 显示系统指标。

  设计：
  - 由 App.vue 按 webview label === 'float' 分流渲染（与 pet / pet-menu 同模式）
  - onMounted 局部把 html/body/#app 背景设为 transparent（学 PetMenuWindow.vue，
    只动本 webview 文档，绝不用全局 CSS，避免污染主窗口——见 beta.17 回归根因）
  - 数据：1Hz invoke get_status_bar_config + get_system_metrics（与托盘采样独立，
    多一组轻量 IPC，代价可忽略）；show_cpu / show_mem / show_disk / show_net 与托盘共用配置
  - 拖拽：整条都是拖拽区，指针按下即 startDragging()（OS 接管零延迟）；结束后读
    outerPosition/scaleFactor 换算逻辑坐标写 localStorage（float_pos），下次挂载恢复
  - 显隐由 Rust 侧管理（托盘菜单/设置页开关 + 全屏自动隐藏），本组件只负责渲染与位置

  修改历史：
  - 2026-09-10 @v0.7.6: 初始创建
  - 2026-09-10 @v0.7.7: ① 修 bug——cpu_usage / memory_usage 后端返回的是 0~1 分数，
      旧代码 Math.round(x) 直接当百分比，导致 77% 显示成 1%（漏乘 100）；
    ② 新增内存 / 磁盘显示（后端已补齐三平台采样，不再限 macOS）；
    ③ 宽度自适应：窗口固定 340px 而内容常有 ~190px，右侧大片空白被用户反馈「浮窗过长」。
       改为测量内容宽度后调 resize_float_window，并在 show 之前完成测量（用户看不到宽态）；
    ④ 视觉重做：36px 高度、更紧凑的间距、tabular-nums 防数字跳动、
       标签弱化 / 数值加粗、网速上下行分色、指标组之间加分隔线。
  - 2026-09-10 @v0.7.7（续）: ⑤ 宽度贴合加固——用户反馈「网速内容溢出（被裁）」。三处：
      a) 测量改为 rect.width 与 scrollWidth 取较大者，余量 +1px → +2px。
         原因：窗口宽度与内容同为逻辑像素，但 set_size 落到物理像素时会被取整
         （Windows 125%/150% 缩放常见），视口可能比内容少 1px，正好裁掉末位字符的尾巴；
      b) 新增 ResizeObserver 兜底：字体异步就绪 / DPI 变化 / 语言切换 / 网速位数变多
         都会改变内容真实宽度，只靠 1Hz tick 测量会漏掉「中间态」，用户会看到一闪的裁切。
         元素是 width:max-content（固有宽度与视口无关），窗口 resize 不会反向触发它，
         不存在 RO → resize → RO 死循环，外加 appliedW 阈值判断双重保险；
      c) 百分比与网速数值定宽 + 右对齐：位数变化（7%→100%、↓75K→↓12.7M）不再每秒
         改变内容宽度——旧实现整条会随数字「呼吸」，窗口跟着 resize 视觉上很毛躁。
         定宽后浮窗宽度稳定，数字也排成整齐的右对齐列；真超出时仍由 fitWidth 兜底跟随。
-->
<template>
  <div ref="rootRef" class="float-bar" @pointerdown="onPointerDown" @contextmenu.prevent>
    <span class="f-dot"></span>
    <span v-if="cfg.show_cpu" class="f-item">
      <i>CPU</i><b>{{ pct(metrics?.cpu_usage) }}</b>
    </span>
    <span v-if="cfg.show_mem" class="f-item">
      <i>MEM</i><b>{{ pct(metrics?.memory_usage) }}</b>
    </span>
    <span v-if="cfg.show_disk" class="f-item">
      <i>DSK</i><b>{{ pct(metrics?.disk_usage) }}</b>
    </span>
    <template v-if="cfg.show_net">
      <span v-if="hasAnyBefore" class="f-sep"></span>
      <span class="f-item f-net">
        <b class="down">↓{{ fmtRate(metrics?.net_rx_bps ?? 0) }}</b>
        <b class="up">↑{{ fmtRate(metrics?.net_tx_bps ?? 0) }}</b>
      </span>
    </template>
    <span v-if="!hasAny" class="f-item"><b class="f-dim">ScreenTime Pro</b></span>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
// Tauri v2：monitor API 为模块级函数（Window 实例上没有 currentMonitor）
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { tracker } from "../api/tracker";
import type { MetricsOut, StatusBarConfig } from "../types";

/** 与 float_window.rs FLOAT_INITIAL_WIDTH 一致（逻辑像素），默认位计算用 */
const BAR_W = 340;
/** 宽度安全余量（逻辑像素）：抵消 set_size 物理像素取整造成的视口不足，见文件头 ⑤a */
const WIDTH_SLACK = 2;
const POS_KEY = "float_pos";

const rootRef = ref<HTMLElement | null>(null);
const cfg = ref<StatusBarConfig>({
  enabled: false,
  show_cpu: true,
  show_mem: true,
  show_disk: false,
  show_net: true,
  float_enabled: true,
});
const metrics = ref<MetricsOut | null>(null);
let timer: number | null = null;
/** 上次已应用的内容宽度，避免每秒无谓 IPC */
let appliedW = 0;
/** 拖拽进行中标记：拖拽期间不调整尺寸，避免与 OS 拖拽循环抢窗口状态 */
let dragging = false;
/**
 * 内容宽度观察器（v0.7.7 加固，见文件头 ⑤b）：
 * 字体异步就绪、DPI 变化、语言切换、网速位数变多都会改变内容真实宽度，
 * 只靠 1Hz tick 测量会漏掉「中间态」——用户会看到一闪的裁切。
 */
let resizeObs: ResizeObserver | null = null;

const hasAny = computed(
  () => cfg.value.show_cpu || cfg.value.show_mem || cfg.value.show_disk || cfg.value.show_net,
);
/** 网速组之前是否有别的指标（决定要不要画分隔线） */
const hasAnyBefore = computed(
  () => cfg.value.show_cpu || cfg.value.show_mem || cfg.value.show_disk,
);

/** 0~1 分数 → 百分比整数串（后端 cpu_usage / memory_usage / disk_usage 均为分数） */
function pct(v: number | undefined): string {
  const n = Math.round((v ?? 0) * 100);
  return `${Math.max(0, Math.min(100, n))}%`;
}

/** 字节/秒格式化（与 Rust format_bps 同口径：B / K / M / G 自适应） */
function fmtRate(bps: number): string {
  if (!Number.isFinite(bps) || bps < 0) return "0B";
  if (bps < 1024) return `${Math.round(bps)}B`;
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(bps < 10240 ? 1 : 0)}K`;
  if (bps < 1024 * 1024 * 1024) return `${(bps / 1048576).toFixed(1)}M`;
  return `${(bps / 1073741824).toFixed(1)}G`;
}

/**
 * 宽度自适应：测量内容真实宽度并收缩窗口（Rust 侧做右边缘锚定）。
 *
 * 测量取 getBoundingClientRect().width 与 scrollWidth 的**较大者**：
 * 前者是元素 border-box 宽度（width:max-content 下即内容固有宽度，含 padding/border），
 * 后者在「视口比内容窄」时会给出内容实际需要宽度——两者取大最稳，不会漏算。
 * 再 +WIDTH_SLACK 抵消 set_size 的物理像素取整（见文件头 ⑤a）。
 */
async function fitWidth(): Promise<void> {
  if (dragging) return;
  await nextTick();
  if (dragging) return;
  const el = rootRef.value;
  if (!el) return;
  const need = Math.max(el.getBoundingClientRect().width, el.scrollWidth);
  const w = Math.ceil(need) + WIDTH_SLACK;
  if (Math.abs(w - appliedW) < 1) return;
  appliedW = w;
  try {
    await invoke("resize_float_window", { width: w });
  } catch {
    /* 窗口尚未创建 / 退出中，忽略 */
  }
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
  dragging = true;
  try {
    await win.startDragging();
  } catch (err) {
    console.warn("[float] 原生拖拽不可用", err);
    dragging = false;
    return;
  }
  try {
    const phys = await win.outerPosition();
    const sf = await win.scaleFactor();
    const pos = { x: Math.round(phys.x / sf), y: Math.round(phys.y / sf) };
    localStorage.setItem(POS_KEY, JSON.stringify(pos));
  } catch {
    /* 拖拽成功但记录失败：下次拖动会再写 */
  } finally {
    dragging = false;
    // 拖拽期间跳过了 resize（避免与 OS 拖拽循环抢窗口状态），结束后补测一次
    await fitWidth();
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
  // v0.7.7：先按首帧内容收缩宽度，再显示——避免用户看到初始 340px 的宽态
  await fitWidth();
  // 内容宽度一变就立刻跟随（字体就绪 / DPI 变化 / 位数变多），不等下一个 1Hz tick
  if (typeof ResizeObserver !== "undefined" && rootRef.value) {
    resizeObs = new ResizeObserver(() => {
      void fitWidth();
    });
    resizeObs.observe(rootRef.value);
  }
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
    await fitWidth();
  };
  void tick();
  timer = window.setInterval(tick, 1000);
});
onBeforeUnmount(() => {
  if (timer !== null) clearInterval(timer);
  resizeObs?.disconnect();
  resizeObs = null;
});
</script>

<style scoped>
.float-bar {
  /* max-content：元素宽度贴合内容，配合测量回调把窗口也收到同样宽 */
  width: max-content;
  height: 36px;
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 12px;
  border-radius: 18px;
  background: rgba(22, 24, 30, 0.78);
  -webkit-backdrop-filter: blur(18px) saturate(140%);
  backdrop-filter: blur(18px) saturate(140%);
  border: 1px solid rgba(255, 255, 255, 0.1);
  /* 内阴影充当「顶部高光」，透明窗口无系统投影，用内描边提质感 */
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.07);
  color: #f5f5f7;
  font-size: 12px;
  font-weight: 600;
  /* 数字等宽：CPU/MEM 百分比变化时宽度不跳 */
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.2px;
  cursor: grab;
  user-select: none;
  white-space: nowrap;
  overflow: hidden;
  transition:
    background 0.18s ease,
    border-color 0.18s ease;
}
.float-bar:hover {
  background: rgba(28, 30, 38, 0.86);
  border-color: rgba(255, 255, 255, 0.16);
}
.float-bar:active {
  cursor: grabbing;
}
.f-dot {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--accent, #ff7e27);
  box-shadow: 0 0 6px rgba(255, 126, 39, 0.75);
}
.f-item {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
  line-height: 1;
}
/* 标签弱化、数值加粗：一眼扫的是数字 */
.f-item > i {
  font-style: normal;
  font-size: 9.5px;
  font-weight: 600;
  letter-spacing: 0.5px;
  color: rgba(255, 255, 255, 0.5);
}
.f-item > b {
  font-weight: 600;
  color: #fff;
  /* 定宽 + 右对齐（v0.7.7 见文件头 ⑤c）：7% → 100% 这类位数变化若不定宽，
     整条会每秒改宽度、窗口跟着 resize，视觉上「呼吸」得厉害。
     定宽后浮窗宽度稳定，数字也排成整齐的右对齐列（2.7em 足够放下 "100%"）。 */
  display: inline-block;
  min-width: 2.7em;
  text-align: right;
}
.f-net {
  gap: 7px;
}
/* 网速值域比百分比宽（↓75K / ↑12.7M / ↓999K），单独给更宽的定位宽度 */
.f-net > b {
  min-width: 3.8em;
}
.f-net > b.down {
  color: #6fd3ff;
}
.f-net > b.up {
  color: #ffb46b;
}
.f-sep {
  flex: none;
  width: 1px;
  height: 14px;
  background: rgba(255, 255, 255, 0.14);
}
.f-dim {
  opacity: 0.72;
  font-weight: 500;
}
</style>
