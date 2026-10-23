/**
 * pet/skins/registry.ts
 * 皮肤注册表（v0.6.2 引入）。
 *
 * 设计：
 * - 模块级单例（与 petStore 同样的"无 Pinia"理念）
 * - 活跃皮肤 id 持久化到 localStorage 'screentime-pet-skin'，与 petStore 解耦（避免修改原 store）
 * - subscribe API 方便响应活跃皮肤切换（PetSkinRenderer.vue 用它来触发组件重建）
 * - setActive 不存在时 fallback 到第一个注册的皮肤，控制台 warn 但不 throw
 * - setActive 后通过 Tauri 全局事件 'pet-skin-changed' 跨窗口广播，PetWindow/Settings 监听
 *   并调用 reloadActive() 把本地副本对齐——这是 Tauri 多窗口 JS 上下文隔离的标准做法（与编辑→桌宠
 *   自定义同步同一根因，详见 MEMORY.md）
 *
 * 修改历史：
 *   - 2026-07-24 @v0.6.2: 初始创建 - 注册表 + 持久化 + 订阅
 *   - 2026-07-24 @v0.6.2-beta.3: 加 Tauri 全局事件广播 + reloadActive()，修跨窗口同步
 *   - 2026-07-24 @v0.6.2-beta.5: 废弃 - 移除 panda-2d，FALLBACK_ID 改 popmart-3d；reloadActive 加失效 id 回落
 *   - 2026-08-07 @v0.6.2-33: 修复 - register() 不再因皮肤注册顺序错误地自动切换 FALLBACK_ID (BUG-11)
 *   - 2026-10-23 @v1.2.6: 修复 - 皮肤选择被静默改回默认（BUG-11 的遗留形态）。
 *     原 register() 在**注册过程中**就校验 activeId 是否在 byId 里：注册第一个皮肤时
 *     注册表必然只有一个，于是任何「稍后注册」的皮肤都会被判为无效 → 回落到 FALLBACK_ID
 *     并 persist 覆盖掉用户的选择。典型症状：用户选了 spiderman，刷新/重启后变回
 *     popmart-3d。现改为 register() 只登记，校验延后到 sealRegistration()（由
 *     skins/index.ts 在**全部注册完成后**调用一次）统一执行。
 *     兼容说明：若调用方从未调用 sealRegistration()，则首次 active()/setActive() 时
 *     懒触发一次 seal（懒 seal 在「注册已稳定」的语境下语义等价，且不会漏修）。
 */
import { reactive } from 'vue';
import { emit as tauriEmit } from '@tauri-apps/api/event';
import type { PetSkinManifest, PetSkinRegistry } from './types';
import { log } from "@/lib/logger";

const STORAGE_KEY = 'screentime-pet-skin';
const FALLBACK_ID = 'popmart-3d'; // v0.6.2-beta.5 起默认皮肤（移除 panda-2d）
const SKIN_CHANGE_EVENT = 'pet-skin-changed'; // 跨窗口广播事件名

// 模块级状态：皮肤清单 + 活跃 id
const state = reactive({
  byId: new Map<string, PetSkinManifest>(),
  activeId: loadActiveId(),
});

/**
 * 注册阶段是否已结束（seal 后为 true）。
 *
 * v1.2.6：未 seal 之前**不做任何 activeId 校验**——因为此时 byId 还在填充中，
 * 任何「当前没看到」的 id 都可能是「还没注册」，而不是「真的失效」。
 * 这正是 BUG-11 的病根：早期校验把「未注册」误当成「已失效」。
 */
let sealed = false;

function loadActiveId(): string {
  try {
    if (typeof localStorage !== 'undefined') {
      const v = localStorage.getItem(STORAGE_KEY);
      if (v) return v;
    }
  } catch {
    /* ignore */
  }
  return FALLBACK_ID;
}

const listeners = new Set<(id: string) => void>();

function notify(): void {
  for (const l of listeners) l(state.activeId);
}

/**
 * 解析回落用的皮肤 id：优先 FALLBACK_ID，其次**首个已注册**的皮肤。
 * @returns 可用的皮肤 id；注册表为空时返回 null（调用方需自行决定是否跳过）
 */
function resolveFallbackId(): string | null {
  const pref = state.byId.get(FALLBACK_ID);
  if (pref) return pref.id;
  const first = state.byId.values().next().value as PetSkinManifest | undefined;
  return first ? first.id : null;
}

/**
 * 注册结束后的统一校验（幂等，可重复调用）。
 *
 * 三件事：
 *  1. activeId 指向的皮肤确实存在 → 保持不动（**不写 localStorage**，避免无谓写盘）
 *  2. activeId 失效（如已删除的 panda-2d）→ 回落到 FALLBACK_ID（或首个已注册皮肤）
 *  3. 注册表为空 → 什么都不做（保留 activeId，等后续注册）
 *
 * 与旧实现的差别：旧实现在每次 register() 里做、且条件写反方向（`!has` 就回落），
 * 导致「先注册 A 再注册 B、用户选的是 B」必然被改成 A。
 */
function seal(): void {
  if (sealed) return;
  if (state.byId.size === 0) return; // 还没有任何皮肤，等下一次
  sealed = true;

  if (state.byId.has(state.activeId)) {
    // 用户的选择有效：保持，不需要 persist（localStorage 里原本就是它）
    return;
  }

  const fallbackId = resolveFallbackId();
  if (fallbackId === null) return;
  log.warn(
    `[pet-skin] 持久化的皮肤 id "${state.activeId}" 不在已注册列表中，回落到 "${fallbackId}"`
  );
  state.activeId = fallbackId;
  persist(state.activeId);
  notify();
}

export const skinRegistry: PetSkinRegistry = {
  register(manifest) {
    // v1.2.6：只登记。**不在这里校验 activeId**（注册尚未完成，见 seal 注释）。
    state.byId.set(manifest.id, manifest);
    // 已 seal 之后又注册新皮肤（热插拔场景）：不影响现有 activeId，即使它曾因
    // 未注册而被回落过，也不在这里回滚——避免「补注册就把用户选择改回来」的抖动。
  },
  /**
   * 声明「内置皮肤都注册完了」，触发一次统一校验。
   * skins/index.ts 在两次 register 之后调用；不调用也不会出错（active/setActive 懒 seal）。
   */
  sealRegistration() {
    seal();
  },
  get(id) {
    return state.byId.get(id);
  },
  list() {
    return Array.from(state.byId.values());
  },
  active() {
    // v1.2.6：懒 seal —— 兼容没显式调用 sealRegistration() 的调用方
    seal();
    // 防御：万一 active 不在清单，回落到第一个
    const m = state.byId.get(state.activeId);
    if (m) return m;
    const first = state.byId.values().next().value as PetSkinManifest | undefined;
    if (first) return first;
    throw new Error('[pet-skin] 注册表为空：未注册任何皮肤');
  },
  setActive(id) {
    if (!state.byId.has(id)) {
      log.warn(`[pet-skin] setActive: 未知皮肤 id "${id}"`);
      return false;
    }
    if (state.activeId === id) return true;
    state.activeId = id;
    persist(id);
    notify();
    // v0.6.2-beta.3：跨窗口广播。本窗口的 listeners 已经通过上面的 notify 收到；
    // 其他窗口（Tauri 多窗口 JS 上下文隔离，模块级 reactive 不共享）需要靠事件
    // 通知后调用 reloadActive() 把本地副本对齐。emit 失败（非 Tauri / 权限）
    // 由 try/catch 吞掉，不影响本地 notify。
    void emitSkinChanged(id).catch((e) =>
      log.warn('[pet-skin] 广播 pet-skin-changed 失败', e)
    );
    return true;
  },
  reloadActive() {
    // 从持久化层重读对齐（跨窗口同步用途，本地 setActive 不会调用）
    // v0.6.2-beta.5：若持久化的是已删除皮肤 id（如迁移前的 panda-2d），回落到 FALLBACK_ID，
    // 避免状态机停在不存在的 activeId（active() 虽能兜但回归体验不一致）
    // v1.2.6：先 ensure seal（注册可能尚未收尾），且回落目标必须是**确实已注册**的皮肤——
    // 旧实现直接回落 FALLBACK_ID，若该 id 本身没注册就会把 activeId 置成不存在的值。
    seal();
    const next = loadActiveId();
    const safe = state.byId.has(next) ? next : resolveFallbackId();
    if (safe === null || state.activeId === safe) return;
    state.activeId = safe;
    notify();
  },
  subscribe(listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};

/**
 * Tauri 全局广播皮肤变更——通知其他窗口（PetWindow/Settings）调用 reloadActive。
 * fire-and-forget：失败仅 console.warn，不影响主流程。
 */
async function emitSkinChanged(id: string): Promise<void> {
  try {
    await tauriEmit(SKIN_CHANGE_EVENT, { id });
  } catch (e) {
    // 非 Tauri 运行时（浏览器预览 / SSR）emit 会抛，吞掉即可
    log.warn('[pet-skin] emit pet-skin-changed 失败（可能非 Tauri 环境）', e);
  }
}

function persist(id: string): void {
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, id);
    }
  } catch (e) {
    log.warn('[pet-skin] 持久化 active id 失败', e);
  }
}

/** 调试用：暴露当前活跃 id（响应式） */
export function activeSkinId(): string {
  return state.activeId;
}
