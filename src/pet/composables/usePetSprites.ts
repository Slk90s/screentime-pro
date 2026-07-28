/**
 * pet/composables/usePetSprites.ts
 * 自定义桌宠素材 + 表情组合编辑器（v0.6.0-beta.1 新增）。
 *
 * 功能：
 * 1. 素材替换：用户可上传 PNG 替换默认 sprite（body_base / eye_* / mouth_* / brow_* / decorations）
 *    存储为 base64 data URL 在 localStorage，key 为 `pet_custom_sprites`。
 * 2. 表情组合编辑：用户可覆盖任意状态的 eye/mouth/brow/decorations 组合。
 *    存储为 JSON 在 localStorage，key 为 `pet_custom_compositions`。
 * 3. 预览模式：实时预览自定义组合效果，确认后保存。
 *
 * 设计思路：
 * - 全部数据 localStorage 持久化，不依赖后端/文件系统
 * - 默认值回退：无自定义时使用内置 import 的原始 sprite
 * - 合并策略：customCompositions[s] 优先 → 回退 stateMachine.getComposition(s)
 * - 素材大小限制：单张 ≤ 256KB（防止 localStorage 膨胀）
 *
 * 使用方式：
 *   const { customSprites, customCompositions, setCustomSprite, setCustomComposition, resetAll, exportConfig, importConfig } = usePetSprites();
 */
import { reactive, watch } from 'vue';
import type { PetState, PetDecoration } from '../types';
import {
  getComposition,
  type PetComposition,
  type EyeSprite,
  type MouthSprite,
  type BrowSprite,
} from '../engine/stateMachine';

// ---- 类型 ----
export interface CustomComposition {
  eye: string;
  mouth: string;
  brow: string;
  decorations: string[];
}

export interface CustomSprites {
  [spriteId: string]: string; // spriteId -> base64 data URL
}

interface SpritesData {
  sprites: CustomSprites;
  compositions: Record<string, CustomComposition>;
}

const STORAGE_KEY_SPRITES = 'pet_custom_sprites';
const STORAGE_KEY_COMPS = 'pet_custom_compositions';
const MAX_SPRITE_SIZE = 256 * 1024; // 256KB per sprite

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
    console.warn('[pet] localStorage 写入失败（可能超限）', e);
  }
}

// ---- 响应式状态 ----
const state = reactive<SpritesData>({
  sprites: loadJSON<CustomSprites>(STORAGE_KEY_SPRITES, {}),
  compositions: loadJSON<Record<string, CustomComposition>>(STORAGE_KEY_COMPS, {}),
});

// 自动持久化 compositions 变更
watch(
  () => ({ ...state.compositions }),
  (val) => { saveJSON(STORAGE_KEY_COMPS, val); },
  { deep: true },
);

// ---- API ----

/**
 * 设置/替换某个 sprite 的图片（base64 data URL）
 * @param spriteId 如 'body_base', 'eye_open_normal', 'glasses' 等
 * @param dataUrl 图片的 base64 data URL（data:image/png;base64,...）
 * @returns 是否成功
 */
function setCustomSprite(spriteId: string, dataUrl: string): boolean {
  // 粗略估算大小（base64 ≈ 原始 * 4/3）
  const estimatedBytes = Math.floor((dataUrl.length - 'data:image/png;base64,'.length) * 0.75);
  if (estimatedBytes > MAX_SPRITE_SIZE) {
    console.warn(`[pet] 素材 ${spriteId} 过大（~${(estimatedBytes / 1024).toFixed(0)}KB > 256KB）`);
    return false;
  }
  state.sprites[spriteId] = dataUrl;
  saveJSON(STORAGE_KEY_SPRITES, state.sprites);
  return true;
}

/**
 * 删除某个自定义 sprite（恢复为默认内置图）
 */
function removeCustomSprite(spriteId: string): void {
  delete state.sprites[spriteId];
  saveJSON(STORAGE_KEY_SPRITES, state.sprites);
}

/**
 * 获取自定义 sprite URL（如果有的话），否则返回 null（表示用默认）
 */
function getCustomSprite(spriteId: string): string | null {
  return state.sprites[spriteId] || null;
}

/**
 * 设置某个状态的自定义部件组合
 */
function setCustomComposition(stateName: string, comp: CustomComposition): void {
  state.compositions[stateName] = comp;
}

/**
 * 删除某个状态的自定义组合（恢复默认）
 */
function removeCustomComposition(stateName: string): void {
  delete state.compositions[stateName];
}

/**
 * 获取某个状态的自定义组合（如果有）
 */
function getCustomComposition(stateName: string): CustomComposition | null {
  return state.compositions[stateName] || null;
}

/**
 * 合并组合：customCompositions[s] 优先 → 回退 stateMachine.getComposition(s)。
 * 供实时桌宠 PetCanvas 与编辑器预览一致地消费用户保存的自定义组合。
 */
function getMergedComposition(state: PetState): PetComposition {
  const custom = getCustomComposition(state);
  const base = getComposition(state);
  if (!custom) return base;
  return {
    eye: (custom.eye as EyeSprite) || base.eye,
    mouth: (custom.mouth as MouthSprite) || base.mouth,
    brow: (custom.brow as BrowSprite) || base.brow,
    decorations: custom.decorations?.length
      ? (custom.decorations as PetDecoration[])
      : base.decorations,
  };
}

/**
 * 从 localStorage 重读自定义素材/组合到响应式 state（跨窗口同步用）。
 * pet 窗口进程与编辑器（主窗口）是独立 webview，各自持有一份内存副本；
 * 编辑器保存后写入 localStorage，pet 窗口需主动重读才能反映到实时桌宠。
 */
function reloadConfig(): void {
  state.sprites = loadJSON<CustomSprites>(STORAGE_KEY_SPRITES, {});
  state.compositions = loadJSON<Record<string, CustomComposition>>(STORAGE_KEY_COMPS, {});
}

/**
 * 同步落盘（保存前调用，避免 watch 异步 flush 与 IPC 事件的竞态）：
 * 编辑器 setCustomComposition 仅改内存，持久化靠 deep watch，跨进程 emit 可能早于 flush。
 */
function persistNow(): void {
  saveJSON(STORAGE_KEY_SPRITES, state.sprites);
  saveJSON(STORAGE_KEY_COMPS, state.compositions);
}

/** 是否有任一自定义内容 */
function hasCustomContent(): boolean {
  return Object.keys(state.sprites).length > 0 || Object.keys(state.compositions).length > 0;
}

/** 重置所有自定义内容 */
function resetAll(): void {
  state.sprites = {};
  state.compositions = {};
  localStorage.removeItem(STORAGE_KEY_SPRITES);
  localStorage.removeItem(STORAGE_KEY_COMPS);
}

/**
 * 导出配置（用于备份/分享）
 * 返回 JSON 字符串
 */
function exportConfig(): string {
  return JSON.stringify({
    version: 1,
    sprites: state.sprites,
    compositions: state.compositions,
    exportedAt: new Date().toISOString(),
  }, null, 2);
}

/**
 * 导入配置（从 JSON 字符串）
 * @returns 是否成功
 */
function importConfig(jsonStr: string): boolean {
  try {
    const data = JSON.parse(jsonStr);
    if (!data.version) throw new Error('无效格式');
    if (data.sprites && typeof data.sprites === 'object') {
      Object.assign(state.sprites, data.sprites);
      saveJSON(STORAGE_KEY_SPRITES, state.sprites);
    }
    if (data.compositions && typeof data.compositions === 'object') {
      Object.assign(state.compositions, data.compositions);
      // watch 会自动持久化 compositions
    }
    return true;
  } catch (e) {
    console.error('[pet] 导入配置失败', e);
    return false;
  }
}

export function usePetSprites() {
  return {
    // 响应式只读访问
    customSprites: () => state.sprites as Readonly<CustomSprites>,
    customCompositions: () => state.compositions as Readonly<Record<string, CustomComposition>>,

    // Sprite 操作
    setCustomSprite,
    removeCustomSprite,
    getCustomSprite,

    // Composition 操作
    setCustomComposition,
    removeCustomComposition,
    getCustomComposition,
    getMergedComposition,
    reloadConfig,
    persistNow,

    // 工具
    hasCustomContent,
    resetAll,
    exportConfig,
    importConfig,
  };
}
