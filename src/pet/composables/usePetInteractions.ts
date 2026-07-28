/**
 * pet/composables/usePetInteractions.ts
 * 桌宠点击交互（v0.6.0-beta 引入，v0.6.2-beta.15 改造：单击轮流触发 3 动画）。
 *
 * 设计思路：
 * - 单击轮流触发三种动画之一：jump / squash / jolt。
 * - 三种动画分别映射到 happy / sad / angry + 各自 CSS 动画名，由 lastClickIndex 轮转。
 * - 单击 *头* 取 happy 基底（仍走 happy）但动画随机；单击 *身体* 取 angry 基底。
 * - 单击、双击、长按动作分离：单击轮流 / 双击固定 spin-burst / 长按固定心疼 shrink。
 * - 状态临时覆盖 1.4s 后回到 idle（不打断自动监听）。
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 4 类点击反应
 *   - 2026-07-25 @v0.6.2-beta.15: 改造 - 单击轮流 jump/squash/jolt，长按为心疼
 */
import { ref, onBeforeUnmount } from 'vue';
import { petStore } from '../stores/petStore';
import type { PetState } from '../types';

interface UsePetInteractionsReturn {
  /** 当前动画 class（用于给 PetCanvas 套动态样式） */
  animClass: ReturnType<typeof ref<string>>;
  /** 绑定到指定元素（null 安全） */
  attach: (el: HTMLElement | null) => void;
  detach: (el: HTMLElement | null) => void;
}

const DOUBLE_CLICK_MS = 300;
const LONG_PRESS_MS = 700;
const RESET_MS = 1400;

/**
 * 单击轮流动画表（v0.6.2-beta.15+）。
 * 三种动画配三种状态基调，按 lastClickIndex % 3 切换。
 */
const CLICK_VARIANTS: { anim: string; state: PetState }[] = [
  { anim: 'pet-anim-jump', state: 'happy' },     // 跳跃：跳起来弹一下
  { anim: 'pet-anim-squash', state: 'surprised' }, // 压扁：被压成饼再弹起
  { anim: 'pet-anim-jolt', state: 'angry' },     // 抖动：原地高频颤
];

export function usePetInteractions(): UsePetInteractionsReturn {
  const animClass = ref('');
  let lastClickTime = 0;
  let clickIndex = 0;
  let longPressTimer: number | null = null;
  let resetTimer: number | null = null;

  function setTemporaryState(s: PetState, anim: string, ms = RESET_MS): void {
    petStore.setOverride(s);
    animClass.value = anim;
    if (resetTimer !== null) clearTimeout(resetTimer);
    resetTimer = window.setTimeout(() => {
      petStore.setOverride(null);
      animClass.value = '';
    }, ms);
  }

  function onPointerUp(e: PointerEvent, el: HTMLElement | null): void {
    if (longPressTimer !== null) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const yPct = (e.clientY - rect.top) / rect.height;
    const isHead = yPct < 0.45;
    const now = Date.now();
    const isDouble = now - lastClickTime < DOUBLE_CLICK_MS;
    lastClickTime = now;

    if (isDouble) {
      // 双击：固定开心转圈（v0.6.0-beta 行为保留）
      setTemporaryState('happy', 'pet-anim-spin');
      return;
    }

    if (isHead) {
      // 单击头：用 jump/squash/jolt 中偏轻的 happy/surprised 系，挑下一个变体
      const variant = CLICK_VARIANTS[clickIndex % CLICK_VARIANTS.length];
      clickIndex += 1;
      setTemporaryState(variant.state, variant.anim);
    } else {
      // 单击身体：偏强烈感受——固定 jolt 抖动（按用户的身体= 反馈更直接）
      setTemporaryState('angry', 'pet-anim-jolt', 800);
    }
  }

  function onPointerDownForLongPress(): void {
    if (longPressTimer !== null) clearTimeout(longPressTimer);
    longPressTimer = window.setTimeout(() => {
      // 长按：撒娇/心疼
      setTemporaryState('sad', 'pet-anim-shrink');
      longPressTimer = null;
    }, LONG_PRESS_MS);
  }

  function attach(el: HTMLElement | null): void {
    if (!el) return;
    el.addEventListener('pointerup', (e) => onPointerUp(e, el));
    el.addEventListener('pointerdown', onPointerDownForLongPress);
  }
  function detach(el: HTMLElement | null): void {
    if (!el) return;
    el.removeEventListener('pointerup', (e) => onPointerUp(e, el));
    el.removeEventListener('pointerdown', onPointerDownForLongPress);
  }

  onBeforeUnmount(() => {
    if (resetTimer !== null) clearTimeout(resetTimer);
    if (longPressTimer !== null) clearTimeout(longPressTimer);
  });

  return {
    animClass,
    attach,
    detach,
  };
}
