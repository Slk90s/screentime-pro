/**
 * pet/composables/usePetDrag.ts
 * 桌宠窗口拖拽逻辑（PointerEvents，跨桌面/移动端统一）。
 *
 * v0.6.2-beta.28 重构：
 * - 旧方案：每帧 requestAnimationFrame → invoke('move_pet_window') 异步 IPC 写回原生窗口。
 *   透明置顶窗每帧 set_position 触发 DWM 重合成 + IPC 往返延迟，导致拖拽「不跟手」。
 * - 新方案：指针移动超过阈值（4px）后用 Tauri 原生 startDragging() 把拖拽交给 OS 处理
 *   （OS 直接搬运窗口位图，零 IPC、零延迟 → 真正跟手）。
 *   - 仅在越过阈值才触发原生拖拽，单击/双击仍走点击交互（不吞事件）。
 *   - 拖拽结束（startDragging Promise resolve）后读取 outerPosition / scaleFactor
 *     换算逻辑坐标写回 store 持久化。
 *   - startDragging 不可用（极少数环境/浏览器预览）时回退 rAF + move_pet_window 手动拖拽，
 *     行为不退化。
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - PointerEvents + 物理像素坐标
 *   - 2026-07-17 @v0.6.0-beta.1: UI优化 - 改用 rAF 驱动，丝滑度提升
 *   - 2026-07-24 @v0.6.2-beta.6: 修复 - 增加 setPointerCapture + onStart/onEnd 暂停持久化，解决拖动卡顿
 *   - 2026-08-06 @v0.6.2-beta.28: 重构 - 阈值触发原生 startDragging 替代每帧 IPC，根治拖拽不跟手
 *   - 2026-08-07 @v0.6.2-33: 修复 - 原生拖拽回退分支补 onEnd() (BUG-6)、移除 onMoveHandler 中重复 onStart (BUG-7)
 *   - 2026-08-13 @v0.7.3: 修复 - macOS 跳过不可靠原生 startDragging，改走与菜单一致的手动 move_pet_window 拖拽；修正回退/onUpHandler 重复 onEnd 并复位 isDragging
 */
import { onBeforeUnmount, ref } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';

/** 超过该位移（px）才判定为拖拽，否则视为点击（保留点击交互） */
const DRAG_THRESHOLD = 4;

/** 是否 macOS：无边框透明窗的 startDragging 在 macOS 上不可靠（窗口不真正跟随光标），
 *  故 macOS 一律走与右键菜单一致的手动拖拽（手动 move 窗口在 macOS 实测更跟手）。 */
const IS_MAC =
  typeof navigator !== 'undefined' &&
  /Mac|iPhone|iPod|iPad/i.test(navigator.platform || navigator.userAgent || '');

export interface UsePetDragReturn {
  isDragging: ReturnType<typeof ref<boolean>>;
  onPointerDown: (e: PointerEvent) => void;
}

export function usePetDrag(
  getPosition: () => { x: number; y: number },
  onMove: (x: number, y: number) => void,
  onStart?: () => void,
  onEnd?: (didDrag: boolean) => void,
): UsePetDragReturn {
  const isDragging = ref(false);
  const appWindow = getCurrentWindow();

  let pointerId: number | null = null;
  let startX = 0;
  let startY = 0;
  let originX = 0;
  let originY = 0;
  let moved = false; // 是否已越过拖拽阈值
  let nativeActive = false; // 原生拖拽进行中（OS 接管指针）
  // 回退手动路径状态
  let fallbackX = 0;
  let fallbackY = 0;
  let rafId: number | null = null;
  let dirty = false;

  function clearListeners(): void {
    window.removeEventListener('pointermove', onMoveHandler);
    window.removeEventListener('pointerup', onUpHandler);
    window.removeEventListener('pointercancel', onUpHandler);
  }

  function flushFallback(): void {
    if (!dirty) return;
    dirty = false;
    invoke('move_pet_window', { x: fallbackX, y: fallbackY }).catch(() => {});
  }

  /** 原生拖拽：交给 OS 处理，零延迟跟手 */
  async function beginNativeDrag(): Promise<void> {
    // macOS：跳过原生 startDragging（不可靠），直接进入手动拖拽模式。
    // 保留 onPointerDown 挂的监听，由 onMoveHandler 的回退分支逐帧 move_pet_window 接管，
    // 与桌宠菜单拖拽机制一致 → macOS 下跟手。
    if (IS_MAC) {
      isDragging.value = true;
      onStart?.();
      nativeActive = false; // 保持手动模式
      return;
    }
    nativeActive = true;
    isDragging.value = true;
    onStart?.();
    // OS 接管指针，移除自有监听（pointerup 不再经我们）
    clearListeners();
    try {
      await appWindow.startDragging();
    } catch (err) {
      // 原生拖拽不可用 → 回退手动 rAF 拖拽（重新挂监听走 fallback 分支）
      console.warn('[pet] 原生拖拽不可用，回退手动拖拽', err);
      isDragging.value = false;
      nativeActive = false;
      // 注意：此处不调用 onEnd —— pointerup 时 onUpHandler 的回退分支会统一收尾，
      // 避免与 onUpHandler 重复触发导致 persistSuspended 状态错乱。
      window.addEventListener('pointermove', onMoveHandler);
      window.addEventListener('pointerup', onUpHandler);
      window.addEventListener('pointercancel', onUpHandler);
      return;
    }
    // 拖拽结束：读取最终位置（物理像素）→ 换算逻辑像素写回 store
    isDragging.value = false;
    try {
      const phys = await appWindow.outerPosition();
      const sf = await appWindow.scaleFactor();
      onMove(Math.round(phys.x / sf), Math.round(phys.y / sf));
    } catch {
      const p = getPosition();
      onMove(p.x, p.y);
    }
    nativeActive = false;
    onEnd?.(true);
  }

  function onMoveHandler(e: PointerEvent): void {
    if (e.pointerId !== pointerId || nativeActive) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (!moved) {
      if (Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
      // 越过阈值：交给 OS 原生拖拽（跟手），不再手动逐帧 IPC
      moved = true;
      void beginNativeDrag(); // onStart 已在 beginNativeDrag 内调用，不重复 (BUG-7)
      return;
    }
    // 回退手动路径（原生拖拽未激活时）：更新 store + 合并 IPC
    const nx = originX + dx;
    const ny = originY + dy;
    onMove(nx, ny);
    fallbackX = nx;
    fallbackY = ny;
    dirty = true;
    if (rafId === null) {
      rafId = requestAnimationFrame(() => {
        rafId = null;
        flushFallback();
      });
    }
  }

  function onUpHandler(e: PointerEvent): void {
    if (e.pointerId !== pointerId) return;
    clearListeners();
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    // 仅回退手动路径在此收尾（原生路径已在 beginNativeDrag.finally 处理）
    if (moved && !nativeActive) {
      const fx = dirty ? fallbackX : getPosition().x;
      const fy = dirty ? fallbackY : getPosition().y;
      dirty = false;
      invoke('move_pet_window', { x: fx, y: fy }).catch(() => {});
      invoke('set_pet_cursor_passthrough', { passthrough: false }).catch(() => {});
      onEnd?.(true);
      isDragging.value = false; // 手动拖拽结束复位（macOS 走此路径）
    }
    pointerId = null;
  }

  function onPointerDown(e: PointerEvent): void {
    // 仅响应鼠标左键/单指触摸
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    e.preventDefault();
    pointerId = e.pointerId;
    startX = e.clientX;
    startY = e.clientY;
    const pos = getPosition();
    originX = pos.x;
    originY = pos.y;
    moved = false;
    nativeActive = false;
    dirty = false;
    window.addEventListener('pointermove', onMoveHandler);
    window.addEventListener('pointerup', onUpHandler);
    window.addEventListener('pointercancel', onUpHandler);
  }

  onBeforeUnmount(() => {
    clearListeners();
    if (rafId !== null) cancelAnimationFrame(rafId);
  });

  return {
    isDragging,
    onPointerDown,
  };
}
