/**
 * pet/engine/stateMachine.ts
 * 桌宠状态机（v0.6.0-beta 引入）。
 *
 * 设计思路：
 * - 15 个状态（idle + working + 13 表情/行为）→ (eye, mouth, brow, decorations[]) 四元组部件组合
 * - 纯数据驱动，组件层只负责按 spriteId 找 PNG 并定位
 * - appToState 映射规则放独立文件（Phase 3 实现前台应用监听时接入）
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 15 状态 + 部件组合表
 *   - 2026-07-17 @v0.6.0-beta.1: 修复 - 补 `working` 状态组合（原缺失导致回退 idle）
 */
import type { PetState, PetDecoration } from '../types';

/** 部件 ID（与 assets/sprites/*.png 文件名对应，无扩展名） */
export type EyeSprite = 'eye_open_normal' | 'eye_happy_smile' | 'eye_closed_sleep' | 'eye_dizzy';
export type MouthSprite = 'mouth_smile' | 'mouth_neutral' | 'mouth_frown' | 'mouth_surprised' | 'mouth_eating' | 'mouth_pout';
export type BrowSprite = 'brow_normal' | 'brow_angry' | 'brow_sad';

/** 状态 → 部件组合 */
export interface PetComposition {
  eye: EyeSprite;
  mouth: MouthSprite;
  brow: BrowSprite;
  /** 装饰层叠加（按数组顺序从下到上），'none' 不渲染 */
  decorations: PetDecoration[];
}

// 15 个状态的部件组合表（一个数据驱动，少一个 bug 源）
const COMPOSITIONS: Record<PetState, PetComposition> = {
  idle: { eye: 'eye_open_normal', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['none'] },
  working: { eye: 'eye_open_normal', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['none'] },
  developing: { eye: 'eye_open_normal', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['glasses'] },
  designing: { eye: 'eye_open_normal', mouth: 'mouth_neutral', brow: 'brow_normal', decorations: ['glasses', 'pencil'] },
  gaming: { eye: 'eye_happy_smile', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['headphone', 'controller'] },
  chatting: { eye: 'eye_open_normal', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['speech_bubble'] },
  meeting: { eye: 'eye_open_normal', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['headphone'] },
  listening: { eye: 'eye_happy_smile', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['headphone'] },
  shopping: { eye: 'eye_open_normal', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['heart'] },
  eating: { eye: 'eye_happy_smile', mouth: 'mouth_eating', brow: 'brow_normal', decorations: ['none'] },
  sleeping: { eye: 'eye_closed_sleep', mouth: 'mouth_neutral', brow: 'brow_normal', decorations: ['zzz'] },
  slacking: { eye: 'eye_happy_smile', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['sweat'] },
  happy: { eye: 'eye_happy_smile', mouth: 'mouth_smile', brow: 'brow_normal', decorations: ['heart'] },
  sad: { eye: 'eye_open_normal', mouth: 'mouth_frown', brow: 'brow_sad', decorations: ['sweat'] },
  angry: { eye: 'eye_open_normal', mouth: 'mouth_pout', brow: 'brow_angry', decorations: ['sweat'] },
  surprised: { eye: 'eye_dizzy', mouth: 'mouth_surprised', brow: 'brow_normal', decorations: ['none'] },
};

/** 状态 → 部件组合（核心 API） */
export function getComposition(state: PetState): PetComposition {
  return COMPOSITIONS[state] ?? COMPOSITIONS.idle;
}

/** 装饰 ID → PNG 文件名 */
export function decorationToSpriteId(d: PetDecoration): string | null {
  if (d === 'none') return null;
  return d; // glasses / headphone / controller / pencil / speech_bubble / heart / zzz / sweat
}

/** 所有 15 状态列表（按 i18n key 顺序） */
export const ALL_STATES: PetState[] = [
  'idle', 'working', 'developing', 'designing', 'gaming', 'chatting',
  'meeting', 'listening', 'shopping', 'eating', 'sleeping',
  'slacking', 'happy', 'sad', 'angry', 'surprised',
];