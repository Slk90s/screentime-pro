/**
 * pet/composables/usePetBadges.ts
 * 3D 桌宠「表情（emoji 浮层）」自定义（v0.6.2-beta.6 新增）。
 *
 * 背景：
 * - v0.6.2 起桌宠皮肤为 Pop Mart 3D（popmart3d），状态差异通过 emoji 浮层表达（见 PopMartPandaPet.vue 的 STATE_BADGES）
 * - 此前编辑器只对 2D 部件合成有意义；3D 皮肤不消费 spriteLayout/usePetSprites
 * - 本模块让用户按状态自定义 emoji 表情，编辑器（主窗口）与实时桌宠（pet 窗口）通过
 *   `pet-custom-updated` 事件 + reloadConfig 跨窗口同步（与 2D 自定义素材同一机制）
 *
 * 存储：
 * - localStorage key `pet_custom_badges`：{ [state]: string }，value 为 emoji 串（可含多个字形，按 grapheme 切分）
 * - 默认回退：无自定义时使用 PopMartPandaPet.vue 内置 STATE_BADGES
 *
 * 设计要点：
 * - reactive 单例 + deep watch 自动持久化（与 usePetSprites 同构）
 * - reloadConfig() 不持久化、不重发事件（事件循环防护，见 MEMORY 跨窗口同步范式）
 *
 * 修改历史：
 *   - 2026-07-24 @v0.6.2-beta.6: 初始创建 - 3D 桌宠按状态自定义 emoji 表情
 */
import { reactive, watch } from 'vue';
import type { PetState } from '../types';

const STORAGE_KEY = 'pet_custom_badges';

/** state -> emoji 串（可能含多个 grapheme，渲染时按 Array.from 切分） */
type BadgeMap = Partial<Record<PetState, string>>;

function loadJSON<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function saveJSON(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch (e) {
    console.warn('[pet] 表情持久化失败', e);
  }
}

// ---- 响应式状态 ----
const state = reactive<{ badges: BadgeMap }>({
  badges: loadJSON<BadgeMap>(STORAGE_KEY, {}),
});

// 自动持久化（内存变更 → localStorage）
watch(
  () => ({ ...state.badges }),
  (val) => saveJSON(STORAGE_KEY, val),
  { deep: true },
);

/** 读取某状态的自定义 emoji 串（空串视为未设置） */
function getCustomBadge(s: PetState): string | null {
  const v = state.badges[s];
  return v && v.trim().length ? v : null;
}

/** 设置/更新某状态的 emoji（空串 → 删除自定义，回退默认） */
function setCustomBadge(s: PetState, emoji: string): void {
  const trimmed = emoji.trim();
  if (!trimmed) delete state.badges[s];
  else state.badges[s] = trimmed;
  saveJSON(STORAGE_KEY, state.badges);
}

/** 删除某状态的自定义 emoji（回退默认） */
function removeCustomBadge(s: PetState): void {
  delete state.badges[s];
  saveJSON(STORAGE_KEY, state.badges);
}

/** 从 localStorage 重读（跨窗口同步：编辑器保存后 pet 窗口收到 pet-custom-updated 调它） */
function reloadConfig(): void {
  state.badges = loadJSON<BadgeMap>(STORAGE_KEY, {});
}

/** 同步落盘（保存前调用，避免 deep watch 异步 flush 早于 IPC 事件） */
function persistNow(): void {
  saveJSON(STORAGE_KEY, state.badges);
}

/** 重置所有自定义表情 */
function resetAll(): void {
  state.badges = {};
  localStorage.removeItem(STORAGE_KEY);
}

/** 是否有任一自定义表情 */
function hasCustomContent(): boolean {
  return Object.keys(state.badges).length > 0;
}

export function usePetBadges() {
  return {
    getCustomBadge,
    setCustomBadge,
    removeCustomBadge,
    reloadConfig,
    persistNow,
    resetAll,
    hasCustomContent,
    /** 只读访问（编辑器内联预览用） */
    allBadges: () => state.badges as Readonly<BadgeMap>,
  };
}
