/**
 * pet/composables/usePetDrag.ts
 * 桌宠窗口拖拽逻辑（PointerEvents，跨桌面/移动端统一）。
 *
 * 设计思路：
 * - 用 Pointer Events 统一鼠标/触摸，避坑：mousedown 在 Safari 上 touch 失效
 * - **setPointerCapture**：按下即捕获指针，鼠标拖出桌宠小窗（150×330）后仍持续收到
 *   pointermove，否则一旦光标离开窗口拖拽即中断 → 这是「拖动不丝滑」的根因（v0.6.2-beta.6 修复）
 * - **requestAnimationFrame 驱动位置同步**（替代 setTimeout 节流），与显示器刷新率对齐，拖拽更丝滑
 * - 响应式 store 位置立即更新（onMove），Tauri IPC 异步写回原生窗口（rAF 合并）
 * - 拖拽时 passthrough:false 保持不变（默认已关闭穿透）
 * - onStart/onEnd 钩子：拖拽期间暂停 petStore 持久化（deep watch 每帧写 localStorage 会卡顿）
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - PointerEvents + 物理像素坐标
 *   - 2026-07-17 @v0.6.0-beta.1: UI优化 - 改用 rAF 驱动，丝滑度提升
 *   - 2026-07-24 @v0.6.2-beta.6: 修复 - 增加 setPointerCapture + onStart/onEnd 暂停持久化，解决拖动卡顿
 */
import { onBeforeUnmount, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export interface UsePetDragReturn {
  isDragging: ReturnType<typeof ref<boolean>>;
  onPointerDown: (e: PointerEvent) => void;
}

export function usePetDrag(
  getPosition: () => { x: number; y: number },
  onMove: (x: number, y: number) => void,
  onStart?: () => void,
  onEnd?: () => void,
): UsePetDragReturn {
  const isDragging = ref(false);

  let startX = 0;
  let startY = 0;
  let originX = 0;
  let originY = 0;
  let pointerId: number | null = null;
  let pendingX = 0;
  let pendingY = 0;
  let rafId: number | null = null;
  let dirty = false;

  // rAF 回调：每帧最多调用一次 Tauri IPC（与显示器刷新率对齐，通常 60/120fps）
  function flushPosition(): void {
    if (!dirty || !isDragging.value) return;
    dirty = false;
    invoke('move_pet_window', { x: pendingX, y: pendingY }).catch((err) => {
      console.error('[pet] 同步窗口位置失败', err);
    });
  }

  function onMoveHandler(e: PointerEvent): void {
    if (!isDragging.value || e.pointerId !== pointerId) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    const newX = originX + dx;
    const newY = originY + dy;

    // 立即更新响应式 store（UI 无延迟）
    onMove(newX, newY);

    // 标记脏数据，等下一帧 flush 到原生窗口
    pendingX = newX;
    pendingY = newY;
    dirty = true;
    if (rafId === null) {
      rafId = requestAnimationFrame(() => {
        rafId = null;
        flushPosition();
      });
    }
  }

  function onUpHandler(e: PointerEvent): void {
    if (e.pointerId !== pointerId) return;
    isDragging.value = false;
    pointerId = null;
    window.removeEventListener('pointermove', onMoveHandler);
    window.removeEventListener('pointerup', onUpHandler);
    window.removeEventListener('pointercancel', onUpHandler);

    // 取消未发出的 rAF
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    // 最后一帧强制 flush（确保最终位置精确落库）
    if (dirty) {
      dirty = false;
      invoke('move_pet_window', { x: pendingX, y: pendingY }).catch(() => {});
    } else {
      // 即使没有脏数据也用 store 当前值兜底写回一次
      const pos = getPosition();
      invoke('move_pet_window', { x: pos.x, y: pos.y }).catch(() => {});
    }

    // 保持可交互（passthrough:false）
    invoke('set_pet_cursor_passthrough', { passthrough: false }).catch(() => {});

    // 通知外层拖拽结束（恢复持久化等）
    onEnd?.();
  }

  function onPointerDown(e: PointerEvent): void {
    // 仅响应鼠标左键/单指触摸
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    e.preventDefault();
    // 捕获指针：光标移出桌宠小窗后仍持续收到 move 事件（修复拖拽中断导致的卡顿）
    (e.currentTarget as HTMLElement | null)?.setPointerCapture?.(e.pointerId);
    isDragging.value = true;
    pointerId = e.pointerId;
    startX = e.clientX;
    startY = e.clientY;
    const pos = getPosition();
    originX = pos.x;
    originY = pos.y;
    pendingX = pos.x;
    pendingY = pos.y;
    dirty = false;
    onStart?.();
    window.addEventListener('pointermove', onMoveHandler);
    window.addEventListener('pointerup', onUpHandler);
    window.addEventListener('pointercancel', onUpHandler);
  }

  onBeforeUnmount(() => {
    window.removeEventListener('pointermove', onMoveHandler);
    window.removeEventListener('pointerup', onUpHandler);
    window.removeEventListener('pointercancel', onUpHandler);
    if (rafId !== null) cancelAnimationFrame(rafId);
  });

  return {
    isDragging,
    onPointerDown,
  };
}
