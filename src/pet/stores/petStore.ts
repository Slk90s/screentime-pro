/**
 * pet/stores/petStore.ts
 * 桌宠响应式状态（v0.6.0-beta 引入）。
 *
 * 设计思路：
 * - 项目主架构不使用 Pinia（保持 v0.5.0 现状），改用 Vue 3 reactive + module-level 单例
 * - 用单一 localStorage key `screentime-pet` 存整体快照，避免多 key 原子性问题
 * - 直接导出 reactive 对象 + 操作函数；模板里 `petStore.enabled` 自动响应追踪，script 里读 `petStore.enabled` 也直接是值
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - reactive + localStorage 持久化
 *   - 2026-08-07 @v0.6.2-33: 修复 - reload() 扩展同步 override + 喂食字段
 *     （菜单独立窗口 setOverride/feed 后需经 pet-store-updated 事件触发本函数，才能同步到桌宠窗口）
 *   - 2026-08-07 @v0.6.2-33: 修复 - reload() 补 scale 同步 (BUG-1)，
 *     重构 canFeedToday 消除 computed 内副作用 (BUG-2)
 *   - 2026-08-13 @v0.7.3: 修复 - setEnabled 广播 pet-enabled-changed，桌宠开关三窗口状态同步
 */
import { reactive, computed, watch } from 'vue';
import { emit } from '@tauri-apps/api/event';
import type { PetState } from '../types';

/** 是否运行在 Tauri 环境（浏览器 mock 模式不触发跨窗口事件，避免报错） */
const isTauriEnv =
  typeof window !== 'undefined' &&
  (window as any).__TAURI_INTERNALS__ !== undefined;

const STORAGE_KEY = 'screentime-pet';

/** 默认窗口位置（屏幕右下偏内 80px，窗口 140×140） */
function defaultPosition(): { x: number; y: number } {
  const screenW = typeof window !== 'undefined' ? window.screen.width : 1440;
  const screenH = typeof window !== 'undefined' ? window.screen.height : 900;
  return { x: screenW - 200, y: screenH - 240 };
}

function todayStr(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

interface RawState {
  enabled: boolean;
  position: { x: number; y: number };
  /** v0.6.2-beta.15：滚轮缩放，区间 [0.5, 2.0]，默认 1.0 */
  scale: number;
  state: PetState;
  override: PetState | null;
  fullness: number;
  feedCount: number;
  level: number;
  todayFeedCount: number;
  todayFeedDate: string;
  /** v0.6.2-beta.15：系统过热模式（true=负载高，桌宠暴躁升温） */
  isHeating?: boolean;
  /** 最近一次系统负载值（0.0~1.0），调试/展示用 */
  lastSystemLoad?: number | null;
}

function loadInitial(): RawState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<RawState>;
      return {
        enabled: p.enabled ?? false,
        position: p.position ?? defaultPosition(),
        scale: typeof p.scale === 'number' ? Math.min(2, Math.max(0.5, p.scale)) : 1.0,
        state: p.state ?? 'idle',
        override: p.override ?? null,
        fullness: p.fullness ?? 100,
        feedCount: p.feedCount ?? 0,
        level: p.level ?? 1,
        todayFeedCount: p.todayFeedDate === todayStr() ? p.todayFeedCount ?? 0 : 0,
        todayFeedDate: p.todayFeedDate ?? todayStr(),
      };
    }
  } catch (e) {
    console.error('[pet] 读取持久化数据失败', e);
  }
  return {
    enabled: false,
    position: defaultPosition(),
    scale: 1.0,
    state: 'idle',
    override: null,
    fullness: 100,
    feedCount: 0,
    level: 1,
    todayFeedCount: 0,
    todayFeedDate: todayStr(),
  };
}

// 模块级 reactive 单例（所有 PetWindow / Settings 共享同一份状态）
const _state = reactive<RawState>(loadInitial());

// 持久化挂起标志：拖拽期间置 true，避免 deep watch 每帧写 localStorage 导致卡顿
let persistSuspended = false;

// 任何字段变化都自动持久化（deep watch 覆盖嵌套字段）
watch(
  _state,
  (v) => {
    if (persistSuspended) return; // 拖拽中：跳过每帧写盘
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(v));
    } catch (e) {
      console.error('[pet] 持久化失败', e);
    }
  },
  { deep: true },
);

// v0.6.2-33 (BUG-2): 跨天重置从 computed 副作用移到 watch，保证 pure computed
const todayFeedDateRef = reactive({ value: _state.todayFeedDate });
const todayFeedCountRef = reactive({ value: _state.todayFeedCount });

// 跨天自动重置每日计数器（非 computed 副作用，由 watch 驱动）
watch(
  () => _state.todayFeedDate,
  (date) => { todayFeedDateRef.value = date; },
  { immediate: true },
);

const canFeedToday = computed(() => {
  const today = todayStr();
  if (todayFeedDateRef.value !== today) {
    // 跨天重置（在 watch 里改原始 _state 会造成循环，这里只返回 false）
    return false;
  }
  // 同步实际计数（日常使用场景 computed 自动响应）
  return todayFeedCountRef.value < 5;
});

// 供 feed() 调用：跨天时重置
function ensureDailyReset(): void {
  const today = todayStr();
  if (_state.todayFeedDate !== today) {
    _state.todayFeedDate = today;
    _state.todayFeedCount = 0;
    todayFeedDateRef.value = today;
    todayFeedCountRef.value = 0;
  }
}

const isHungry = computed(() => _state.fullness < 30);

const effectiveState = computed<PetState>(() => {
  if (_state.override) return _state.override;
  if (_state.isHeating) return 'angry';
  return _state.state;
});

// ---- 操作方法 ----
function setEnabled(v: boolean): void {
  _state.enabled = v;
  // 跨窗口同步：通知所有窗口（主界面/桌宠窗/菜单窗）重读 enabled，
  // 避免三处开关状态不同步。事件由本窗口发出、也会回灌自身（reload 幂等无害）。
  if (isTauriEnv) {
    emit('pet-enabled-changed').catch(() => {});
  }
}
/**
 * 跨窗口同步：从 localStorage 重新读取持久化字段覆盖当前 reactive。
 * Tauri 多窗口各持独立 JS 上下文，module-level reactive 不共享；桌宠窗口（渲染宠物）
 * 与 pet-menu 窗口（右键菜单）是不同 webview，菜单里 setOverride/feed/setPosition 只改
 * 菜单自己的 store 并写盘，桌宠窗口必须靠本函数 + `pet-store-updated` 事件重读才能同步。
 *
 * 注意：不覆盖 `state` —— state 由桌宠窗口的 useForegroundWatcher 自动驱动，重读会闪。
 * 只同步手动覆盖（override）与喂食相关字段（fullness/feedCount/level/todayFeed*）。
 */
function reload(): void {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<RawState>;
      if (typeof p.enabled === 'boolean') _state.enabled = p.enabled;
      if (p.position) _state.position = p.position;
      // v0.6.2-33：菜单手动切状态 → 同步 override 到桌宠窗口
      if (p.override === null || typeof p.override === 'string') _state.override = p.override;
      // v0.6.2-33：菜单喂食 → 同步饱食度/等级/每日计数，保证每日上限一致
      if (typeof p.fullness === 'number') _state.fullness = p.fullness;
      if (typeof p.feedCount === 'number') _state.feedCount = p.feedCount;
      if (typeof p.level === 'number') _state.level = p.level;
      if (typeof p.todayFeedCount === 'number') _state.todayFeedCount = p.todayFeedCount;
      if (typeof p.todayFeedDate === 'string') _state.todayFeedDate = p.todayFeedDate;
      // v0.6.2-33 (BUG-1): 补 scale 同步，避免跨窗口改缩放后桌宠不同步
      if (typeof p.scale === 'number') _state.scale = Math.min(2, Math.max(0.5, p.scale));
    }
  } catch (e) {
    console.error('[pet] reload 失败', e);
  }
}
function setPosition(x: number, y: number): void {
  _state.position = { x, y };
}
/**
 * v0.6.2-beta.15：滚轮缩放持久化（夹紧到 [0.5, 2.0]）。
 * 持久化会触发 deep watch → localStorage，但用户调尺度不频繁，可接受。
 */
function setScale(v: number): void {
  _state.scale = Math.min(2, Math.max(0.5, Number(v.toFixed(2))));
}
/**
 * v0.6.2-beta.15：系统过热模式。
 * - heating=true 时 effectiveState 视为 'angry'（除非用户手动 override）
 * - 仅在内存中保持，不持久化（重启归零）
 */
function setHeating(on: boolean, load: number | null): void {
  _state.isHeating = on;
  _state.lastSystemLoad = load;
}
/**
 * 拖拽期间暂停/恢复持久化。
 * - 置 true：watch 跳过写盘（拖拽每帧改 position 不再触发 localStorage 写入）
 * - 置 false：恢复写盘，并立即落盘一次当前值（确保最终位置被保存）
 */
function setPersistSuspended(v: boolean): void {
  persistSuspended = v;
  if (!v) {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(_state));
    } catch (e) {
      console.error('[pet] 持久化失败', e);
    }
  }
}
function setState(s: PetState): void {
  _state.state = s;
  _state.override = null;
}
function setOverride(s: PetState | null): void {
  _state.override = s;
}
function feed(foodValue: number): { ok: boolean; reason?: string } {
  ensureDailyReset(); // v0.6.2-33: 每次喂食前先重置跨天计数
  if (!canFeedToday.value) return { ok: false, reason: 'today-limit' };
  _state.fullness = Math.min(100, _state.fullness + foodValue);
  _state.feedCount += 1;
  _state.todayFeedCount += 1;
  // v0.7.0 (BUG 修复)：同步 todayFeedCountRef，否则 canFeedToday 计算属性读到的是
  // 残旧的初始计数，导致「每日 5 次上限」判定失灵（可无限喂 / 或首喂被误拦）。
  todayFeedCountRef.value = _state.todayFeedCount;
  _state.level = Math.floor(_state.feedCount / 10) + 1;
  if (_state.state === 'sleeping' && _state.fullness > 30) {
    _state.state = 'idle';
  }
  return { ok: true };
}
function tickFullness(): void {
  if (_state.fullness > 0) {
    _state.fullness = Math.max(0, _state.fullness - 1);
  }
}

/**
 * 直接导出 reactive 对象 + 计算属性 + 操作函数。
 * 模板里 `petStore.enabled` 自动响应追踪（reactive 属性访问），
 * script 里读 `petStore.enabled` 也直接是值。
 */
export const petStore = {
  // state（reactive，模板和 setup 直接访问）
  get enabled() { return _state.enabled; },
  get position() { return _state.position; },
  get scale() { return _state.scale; },
  get state() { return _state.state; },
  get override() { return _state.override; },
  get fullness() { return _state.fullness; },
  get feedCount() { return _state.feedCount; },
  get level() { return _state.level; },
  get todayFeedCount() { return _state.todayFeedCount; },
  get todayFeedDate() { return _state.todayFeedDate; },
  get isHeating() { return !!_state.isHeating; },
  get lastSystemLoad() { return _state.lastSystemLoad ?? null; },
  // computed
  canFeedToday,
  isHungry,
  effectiveState,
  // actions
  setEnabled,
  setPosition,
  setScale,
  setPersistSuspended,
  setState,
  setOverride,
  setHeating,
  feed,
  tickFullness,
  reload,
};