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
 */
import { reactive } from 'vue';
import { emit as tauriEmit } from '@tauri-apps/api/event';
import type { PetSkinManifest, PetSkinRegistry } from './types';

const STORAGE_KEY = 'screentime-pet-skin';
const FALLBACK_ID = 'popmart-3d'; // v0.6.2-beta.5 起默认皮肤（移除 panda-2d）
const SKIN_CHANGE_EVENT = 'pet-skin-changed'; // 跨窗口广播事件名

// 模块级状态：皮肤清单 + 活跃 id
const state = reactive({
  byId: new Map<string, PetSkinManifest>(),
  activeId: loadActiveId(),
});

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

export const skinRegistry: PetSkinRegistry = {
  register(manifest) {
    state.byId.set(manifest.id, manifest);
    // v0.6.2-33 (BUG-11): 仅当 activeId 不在注册表中时才回落，不因 FALLBACK_ID 未注册就误切
    if (!state.byId.has(state.activeId)) {
      const fallback = state.byId.get(FALLBACK_ID) ?? manifest;
      state.activeId = fallback.id;
      persist(state.activeId);
      notify();
    }
  },
  get(id) {
    return state.byId.get(id);
  },
  list() {
    return Array.from(state.byId.values());
  },
  active() {
    // 防御：万一 active 不在清单，回落到第一个
    const m = state.byId.get(state.activeId);
    if (m) return m;
    const first = state.byId.values().next().value as PetSkinManifest | undefined;
    if (first) return first;
    throw new Error('[pet-skin] 注册表为空：未注册任何皮肤');
  },
  setActive(id) {
    if (!state.byId.has(id)) {
      console.warn(`[pet-skin] setActive: 未知皮肤 id "${id}"`);
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
      console.warn('[pet-skin] 广播 pet-skin-changed 失败', e)
    );
    return true;
  },
  reloadActive() {
    // 从持久化层重读对齐（跨窗口同步用途，本地 setActive 不会调用）
    // v0.6.2-beta.5：若持久化的是已删除皮肤 id（如迁移前的 panda-2d），回落到 FALLBACK_ID，
    // 避免状态机停在不存在的 activeId（active() 虽能兜但回归体验不一致）
    const next = loadActiveId();
    const safe = state.byId.has(next) ? next : FALLBACK_ID;
    if (state.activeId === safe) return;
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
    console.warn('[pet-skin] emit pet-skin-changed 失败（可能非 Tauri 环境）', e);
  }
}

function persist(id: string): void {
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, id);
    }
  } catch (e) {
    console.warn('[pet-skin] 持久化 active id 失败', e);
  }
}

/** 调试用：暴露当前活跃 id（响应式） */
export function activeSkinId(): string {
  return state.activeId;
}
