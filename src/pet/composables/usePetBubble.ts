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
import enPhrases from '../config/bubble-phrases.en-US.json';
import { i18n } from '@/i18n';
import { log } from "@/lib/logger";

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
    log.warn('[pet] 读取自定义气泡配置失败', e);
  }
  return {};
}

/** 按当前语言选词库（v1.2.2：英文模式切到 en-US 气泡池，桌宠不再说中文） */
function baseLibrary(): PhraseLibrary {
  const loc = (i18n.global.locale.value as string) || '';
  return (loc.toLowerCase().startsWith('en') ? enPhrases : defaultPhrases) as PhraseLibrary;
}

/** 取得某个皮肤的合并后配置（default < 皮肤默认 < 自定义覆盖） */
function getSkinConfig(skinId?: string | null): SkinPhraseConfig {
  const lib = baseLibrary();
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
