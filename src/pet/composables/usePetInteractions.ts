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
 * - v0.6.2-beta.31：皮肤感知——蜘蛛侠皮肤下单击轮转额外加入 pet-anim-web（跳起射蛛丝网）。
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 4 类点击反应
 *   - 2026-07-25 @v0.6.2-beta.15: 改造 - 单击轮流 jump/squash/jolt，长按为心疼
 *   - 2026-08-07 @v0.6.2-beta.31: 蜘蛛侠皮肤下单击轮转加入 web（跳起射蛛丝网），由注册表活跃皮肤判定
 *   - 2026-08-07 @v0.6.2-33: 修复 detach() 用内联箭头函数导致 removeEventListener 永远匹配不到 (BUG-4)
  - 2026-08-10 @v0.7.2: 修复 - 拖拽结束后 pointerup 误触发点击反应（抖动）；右键不再触发点击反应；
    拖拽开始时清除残留动画 class，避免 CSS transform 与 OS 窗口移动叠加造成「卡顿」观感
 */
import { ref, onBeforeUnmount } from 'vue';
import { petStore } from '../stores/petStore';
import { skinRegistry } from '../skins/registry';
import type { PetState } from '../types';

interface UsePetInteractionsReturn {
  /** 当前动画 class（用于给 PetCanvas 套动态样式） */
  animClass: ReturnType<typeof ref<string>>;
  /** 绑定到指定元素（null 安全） */
  attach: (el: HTMLElement | null) => void;
  detach: (el: HTMLElement | null) => void;
  /** 抑制接下来一小段时间内的点击反应（拖拽结束时调用，避免拖完又抖一下） */
  suppressNextClick: () => void;
  /** 立即清除当前点击动画 class（拖拽开始时调用，避免动画与窗口移动叠加产生卡顿观感） */
  clearAnim: () => void;
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

/**
 * 单击轮转的变体列表（皮肤感知）。
 * 蜘蛛侠皮肤额外加入「跳起射蛛丝网」（pet-anim-web），其余皮肤保持原 3 种。
 */
function clickVariants(): { anim: string; state: PetState }[] {
  const variants = [...CLICK_VARIANTS];
  if (skinRegistry.active().id === 'spiderman') {
    variants.push({ anim: 'pet-anim-web', state: 'happy' });
  }
  return variants;
}

export function usePetInteractions(): UsePetInteractionsReturn {
  const animClass = ref('');
  let lastClickTime = 0;
  let clickIndex = 0;
  let longPressTimer: number | null = null;
  let resetTimer: number | null = null;
  // v0.7.2：拖拽结束后的 pointerup 会误触发点击反应（抖动），用此标志位抑制接下来一小段时间内的点击反应
  let clickSuppressed = false;

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
    // v0.7.2：拖拽结束的 pointerup 已被拖拽逻辑 suppress，这里直接忽略，避免「拖完又抖一下」
    if (clickSuppressed) return;
    // v0.7.2：仅左键/单指触发点击反应；右键交给菜单（onContextMenu），不应让桌宠抖动
    if (e.button !== 0) return;
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
      // 单击头：用 jump/squash/jolt/web 中偏轻的 happy/surprised 系，挑下一个变体
      const variants = clickVariants();
      const variant = variants[clickIndex % variants.length];
      clickIndex += 1;
      setTemporaryState(variant.state, variant.anim);
    } else {
      // 单击身体：偏强烈感受——固定 jolt 抖动（按用户的身体= 反馈更直接）
      setTemporaryState('angry', 'pet-anim-jolt', 800);
    }
  }

  function onPointerDownForLongPress(e: PointerEvent): void {
    // v0.7.2：仅左键/单指才有长按（心疼）反应；右键长按不触发，避免与菜单冲突
    if (e.button !== 0) return;
    if (longPressTimer !== null) clearTimeout(longPressTimer);
    longPressTimer = window.setTimeout(() => {
      // 长按：撒娇/心疼
      setTemporaryState('sad', 'pet-anim-shrink');
      longPressTimer = null;
    }, LONG_PRESS_MS);
  }

  // v0.6.2-33 (BUG-4 fix): 保存绑定引用，否则 detach 永远匹配不到
  let boundPointerUp: ((e: PointerEvent) => void) | null = null;
  let boundEl: HTMLElement | null = null;

  function attach(el: HTMLElement | null): void {
    if (!el) return;
    boundEl = el;
    boundPointerUp = (e: PointerEvent) => onPointerUp(e, el);
    el.addEventListener('pointerup', boundPointerUp);
    el.addEventListener('pointerdown', onPointerDownForLongPress);
  }
  function detach(el: HTMLElement | null): void {
    if (!el) return;
    if (boundPointerUp) el.removeEventListener('pointerup', boundPointerUp);
    el.removeEventListener('pointerdown', onPointerDownForLongPress);
    if (el === boundEl) { boundPointerUp = null; boundEl = null; }
  }

  // v0.7.2：拖拽结束的尾随 pointerup 不应再触发点击反应（抖动）
  function suppressNextClick(): void {
    clickSuppressed = true;
    window.setTimeout(() => {
      clickSuppressed = false;
    }, 80);
  }

  // v0.7.2：拖拽开始时清掉正在运行的点击动画 class，避免 CSS transform 与 OS 窗口移动叠加出现卡顿观感
  // （只清动画，不清用户手动设置的 state override）
  function clearAnim(): void {
    if (resetTimer !== null) clearTimeout(resetTimer);
    animClass.value = '';
  }

  onBeforeUnmount(() => {
    if (resetTimer !== null) clearTimeout(resetTimer);
    if (longPressTimer !== null) clearTimeout(longPressTimer);
  });

  return {
    animClass,
    attach,
    detach,
    suppressNextClick,
    clearAnim,
  };
}
