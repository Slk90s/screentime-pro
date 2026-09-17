/**
 * pet/composables/usePetHitMask.ts
 * 桌宠「按身体形状命中」的掩码上报（v0.8.0，配合 Rust 端 pet/hit_mask.rs）。
 *
 * 背景（对齐 Rust 侧设计）：
 * - 修前桌宠窗整窗接收鼠标 → 150×330 的竖窗里绝大多数是透明区，会吃掉下层应用点击（点击死区）。
 * - Rust 侧用后台线程轮询全局光标位置来切换穿透，需要前端提供「哪些格子是身体」的粗粒度网格。
 *   本 composable 就负责把渲染结果压成 COLS×ROWS 的 0/1 网格并上报。
 *
 * 实现要点：
 * - 用离屏 canvas 把桌宠窗内所有可见 <img>（各皮肤的图层/表情帧）按其布局矩形重绘一遍，
 *   再逐像素读 alpha → 压成网格。这样对任意皮肤（2D 精灵 / Pop Mart 3D 帧序列）都通用，
 *   不需要皮肤自己声明轮廓。
 * - 气泡（.pet-bubble）会被排除：它是临时提示，不该扩大可点击区域。
 * - 图片尚未 decode 完时跳过该图层；每次 img load 事件与皮肤切换都会重新上报。
 * - 动画帧会改变轮廓，故额外 2.5s 周期兜底刷新一次（一次网格构建成本极低）。
 */

import { onBeforeUnmount, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { isTauri } from '../../api/tracker';

/** 网格密度：32×32 对 150×330 的窗来说每格 ≈ 5×10px，足够贴合身体轮廓又不至于太重 */
const COLS = 32;
const ROWS = 32;
/** 判定「不透明」的 alpha 阈值（排除阴影/羽化边缘的极淡像素） */
const ALPHA_THRESHOLD = 24;
/** 动画帧轮廓兜底刷新间隔 */
const REFRESH_MS = 2500;

export interface UsePetHitMaskReturn {
  /** 立即重建并上报掩码（换肤 / 缩放 / 图片加载完成后调用） */
  refresh: () => void;
  /** 拖拽锁：拖拽期间必须锁住，否则光标移出身体会被线程切成穿透、拖拽中断 */
  setDragLock: (locked: boolean) => void;
}

export function usePetHitMask(getRoot: () => HTMLElement | null): UsePetHitMaskReturn {
  let canvas: HTMLCanvasElement | null = null;
  let rafId = 0;
  let intervalId: number | null = null;
  let unsubSkin: (() => void) | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let destroyed = false;

  async function report(): Promise<void> {
    if (!isTauri || destroyed) return;
    const root = getRoot();
    if (!root) return;
    // 用窗内实际像素尺寸（Tauri 窗 = CSS 视口）
    const w = Math.round(root.clientWidth || window.innerWidth);
    const h = Math.round(root.clientHeight || window.innerHeight);
    if (w <= 0 || h <= 0) return;

    if (!canvas) canvas = document.createElement('canvas');
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (!ctx) return;
    ctx.clearRect(0, 0, w, h);

    const rootRect = root.getBoundingClientRect();
    const imgs = Array.from(root.querySelectorAll('img')) as HTMLImageElement[];
    for (const img of imgs) {
      if (!img.complete || img.naturalWidth === 0) continue;
      // 气泡等临时提示不算身体
      if (img.closest('.pet-bubble')) continue;
      const cs = window.getComputedStyle(img);
      if (cs.display === 'none' || cs.visibility === 'hidden' || Number(cs.opacity) === 0) continue;
      const r = img.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) continue;
      ctx.drawImage(img, r.left - rootRect.left, r.top - rootRect.top, r.width, r.height);
    }

    const { data } = ctx.getImageData(0, 0, w, h);
    const cells = new Uint8Array(COLS * ROWS);
    for (let y = 0; y < h; y++) {
      const row = Math.min(ROWS - 1, Math.floor((y / h) * ROWS));
      const rowBase = row * COLS;
      for (let x = 0; x < w; x++) {
        if (data[(y * w + x) * 4 + 3] > ALPHA_THRESHOLD) {
          cells[rowBase + Math.min(COLS - 1, Math.floor((x / w) * COLS))] = 1;
        }
      }
    }

    try {
      await invoke('set_pet_hit_mask', { cols: COLS, rows: ROWS, cells: Array.from(cells) });
    } catch (err) {
      console.warn('[pet] 上报命中掩码失败', err);
    }
  }

  function refresh(): void {
    if (rafId) cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      void report();
    });
  }

  function setDragLock(locked: boolean): void {
    if (!isTauri) return;
    invoke('set_pet_drag_lock', { locked }).catch(() => {
      /* 忽略：锁失败只会让穿透线程在拖拽期间多切一次，不致命 */
    });
  }

  onMounted(async () => {
    const root = getRoot();
    // 首次上报：等首帧布局 + 图片 decode 完成（图片可能还在网络/磁盘读取）
    refresh();
    window.setTimeout(refresh, 400);
    window.setTimeout(refresh, 1200);

    if (root) {
      // 每次有图层加载完成就重算（不同皮肤的图层数量/加载时机不一）
      root.querySelectorAll('img').forEach((img) => {
        img.addEventListener('load', refresh);
      });
      resizeObserver = new ResizeObserver(() => refresh());
      resizeObserver.observe(root);
    }

    // 换肤 / 缩放（皮肤尺寸变化 → 窗口尺寸变化）后重算
    try {
      const { skinRegistry } = await import('../skins/registry');
      unsubSkin = skinRegistry.subscribe(() => {
        window.setTimeout(refresh, 250); // 等 setSize 生效再量
      });
    } catch {
      /* 注册表不可用时忽略 */
    }

    intervalId = window.setInterval(refresh, REFRESH_MS);

    // 桌宠重新显示时也重算一次（隐藏期间窗口尺寸可能变过）
    try {
      const { listen } = await import('@tauri-apps/api/event');
      await listen('pet-shown', () => window.setTimeout(refresh, 300));
      await listen('pet-skin-changed', () => window.setTimeout(refresh, 300));
    } catch {
      /* 非 Tauri 环境 */
    }
  });

  onBeforeUnmount(() => {
    destroyed = true;
    if (rafId) cancelAnimationFrame(rafId);
    if (intervalId !== null) clearInterval(intervalId);
    unsubSkin?.();
    resizeObserver?.disconnect();
    // 清空掩码 → 回到「整窗可交互」的旧行为，避免残留掩码影响下次会话
    if (isTauri) {
      invoke('clear_pet_hit_mask').catch(() => {});
    }
  });

  return { refresh, setDragLock };
}
