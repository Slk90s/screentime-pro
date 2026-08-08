/**
 * pet/composables/usePetBubble.ts
 * 桌宠随机中文气泡源（v0.6.2-beta.15 引入，v0.6.2-34 支持按皮肤配置）。
 *
 * 设计：
 * - 内置气泡短语按皮肤分桶：default / popmart-3d / spiderman 各有专属文案。
 * - 配置文件：src/pet/config/bubble-phrases.json（构建时打包，用户可编辑源码后重新构建）。
 * - 运行时自定义：localStorage key `pet_bubble_phrases` 可覆盖任意皮肤的任意状态/通用短语。
 * - pickBubble(state, skinId?) 优先取皮肤专属，无则回退 default；70% 状态桶 + 30% 通用桶。
 * - 返回随机选中的短句与随机间隔（8~22s），父组件控制气泡显示。
 *
 * 修改历史：
 *   - 2026-07-25 @v0.6.2-beta.15: 初始版本 - 50+ 中文短句池
 *   - 2026-08-08 @v0.6.2-34: 皮肤独立气泡配置 + 运行时自定义覆盖
 */
import type { PetState } from '../types';
import defaultPhrases from '../config/bubble-phrases.json';

const STORAGE_KEY = 'pet_bubble_phrases';

/** 单个皮肤的气泡配置 */
interface SkinPhraseConfig {
  common: string[];
  state: Partial<Record<PetState, string[]>>;
}

/** 全部皮肤的默认配置 */
type PhraseLibrary = Record<string, SkinPhraseConfig>;

/** 运行时自定义覆盖（与默认配置同结构） */
type CustomPhraseLibrary = Partial<PhraseLibrary>;

/** 加载运行时自定义覆盖（只读，不响应式） */
function loadCustom(): CustomPhraseLibrary {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as CustomPhraseLibrary;
      if (parsed && typeof parsed === 'object') return parsed;
    }
  } catch (e) {
    console.warn('[pet] 读取自定义气泡配置失败', e);
  }
  return {};
}

/** 保存运行时自定义覆盖 */
function saveCustom(custom: CustomPhraseLibrary): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(custom));
  } catch (e) {
    console.warn('[pet] 保存自定义气泡配置失败', e);
  }
}

/** 取得某个皮肤的合并后配置（default < 皮肤默认 < 自定义覆盖） */
function getSkinConfig(skinId?: string | null): SkinPhraseConfig {
  const lib = defaultPhrases as PhraseLibrary;
  const custom = loadCustom();
  const base = lib.default ?? lib['popmart-3d'] ?? Object.values(lib)[0] ?? { common: [], state: {} };
  const skinDefault = skinId ? lib[skinId] : undefined;
  const skinCustom = skinId ? custom[skinId] : undefined;

  const merge = (a: string[] = [], b: string[] = []): string[] => {
    const merged = [...a];
    for (const item of b) {
      if (!merged.includes(item)) merged.push(item);
    }
    return merged;
  };

  const states = new Set<PetState>([
    ...Object.keys(base.state),
    ...(skinDefault ? Object.keys(skinDefault.state) : []),
    ...(skinCustom ? Object.keys(skinCustom.state) : []),
  ] as PetState[]);

  const state: Partial<Record<PetState, string[]>> = {};
  for (const s of states) {
    state[s] = merge(
      merge(skinDefault?.state[s], base.state[s]),
      skinCustom?.state?.[s] ?? []
    );
  }

  return {
    common: merge(merge(skinDefault?.common, base.common), skinCustom?.common ?? []),
    state,
  };
}

/** 取得一个候选短语 */
export function pickBubble(state: PetState, skinId?: string | null): string {
  const cfg = getSkinConfig(skinId);
  // 70% 概率返回状态桶，30% 返回通用桶
  const useState = Math.random() < 0.7;
  if (useState) {
    const bucket = cfg.state[state] ?? [];
    if (bucket.length > 0) {
      return bucket[Math.floor(Math.random() * bucket.length)];
    }
  }
  const common = cfg.common;
  if (common.length > 0) {
    return common[Math.floor(Math.random() * common.length)];
  }
  return '…';
}

/** 取得下一个气泡的随机间隙毫秒（8~22s） */
export function nextGapMs(): number {
  return 8000 + Math.floor(Math.random() * 14000);
}

/** 读取某个皮肤的自定义配置（无自定义则 undefined） */
export function getCustomPhrases(skinId: string): SkinPhraseConfig | undefined {
  const custom = loadCustom();
  return custom[skinId];
}

/** 设置/覆盖某个皮肤的自定义配置（空数组可删除该条目） */
export function setCustomPhrases(skinId: string, config: Partial<SkinPhraseConfig>): void {
  const custom = loadCustom();
  const cleaned: SkinPhraseConfig = { common: [], state: {} };

  if (config.common && config.common.length > 0) {
    cleaned.common = config.common.filter((s) => s.trim() !== '');
  }
  if (config.state) {
    for (const [k, v] of Object.entries(config.state)) {
      const arr = (v ?? []).filter((s) => s.trim() !== '');
      if (arr.length > 0) {
        cleaned.state[k as PetState] = arr;
      }
    }
  }

  if (cleaned.common.length === 0 && Object.keys(cleaned.state).length === 0) {
    delete custom[skinId];
  } else {
    custom[skinId] = cleaned;
  }
  saveCustom(custom);
}

/** 重读自定义配置（跨窗口同步：编辑器保存后 pet 窗口收到 pet-custom-updated 调它） */
export function reloadBubbleConfig(): void {
  // 自定义配置在 pickBubble 时实时从 localStorage 读取，无需缓存刷新；
  // 本函数仅作为事件回调的显式入口，保留与 usePetBadges 一致的跨窗口同步范式。
}

/** 重置所有自定义气泡配置 */
export function resetAllCustomPhrases(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch (e) {
    console.warn('[pet] 重置自定义气泡配置失败', e);
  }
}
