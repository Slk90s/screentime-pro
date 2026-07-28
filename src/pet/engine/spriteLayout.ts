/**
 * pet/engine/spriteLayout.ts
 * 桌宠部件层定位的「唯一数据源」（v0.6.1-beta.1 抽出，防编辑器/真实桌宠坐标漂移）。
 *
 * 设计思路：
 * - 真实桌宠 PetCanvas 与编辑器预览 PetPreviewStage 都必须 import 本文件的坐标，
 *   禁止各自写死百分比，否则会出现「编辑器预览与桌宠实际渲染错位」的 bug
 *   （v0.6.0-beta.1 的编辑器正是因此把装饰层全堆在左上角）。
 * - 坐标基于 140×140 逻辑容器；x/y 是部件几何中心百分比，w 是宽度百分比。
 * - scale 默认 1，仅个别装饰（pencil 0.9）需要微调。
 *
 * 修改历史：
 *   - 2026-07-24 @v0.6.1-beta.1: 初始创建 - 从 PetCanvas 内联坐标抽出为单一数据源
 */

import type { PetDecoration } from '../types';

/** 单个部件层的定位规格（百分比，相对 140×140 容器） */
export interface PartSpec {
  /** 水平位置百分比（几何中心，0~100） */
  x: number;
  /** 垂直位置百分比（几何中心，0~100） */
  y: number;
  /** 宽度百分比（相对容器） */
  w: number;
  /** 缩放倍数（默认 1） */
  scale?: number;
}

/**
 * 基础部件层定位（body / eye / brow / mouth）。
 * 与真实桌宠渲染 100% 一致，任何改动都会同时影响桌宠与编辑器预览。
 */
export const LAYER_SPECS = {
  body: { x: 50, y: 65, w: 95 },
  eye: { x: 50, y: 48, w: 58 },
  brow: { x: 50, y: 42, w: 50 },
  mouth: { x: 50, y: 60, w: 22 },
} as const satisfies Record<string, PartSpec>;

/**
 * 装饰层定位（手工校准，基于身体几何中心）。
 * 用 Partial 以便按 PetDecoration 索引得到 `PartSpec | undefined`，
 * 模板里用 `v-if` 兜底未定义项（如 'none'）。
 */
export const DECO_SPECS: Partial<Record<PetDecoration, PartSpec>> = {
  glasses: { x: 50, y: 48, w: 50 }, // 眼镜横跨双眼
  headphone: { x: 50, y: 38, w: 80 }, // 耳机罩双耳，头顶
  controller: { x: 50, y: 92, w: 55 }, // 手柄放身体前方
  pencil: { x: 82, y: 30, w: 30, scale: 0.9 }, // 铅笔斜插头部右上
  speech_bubble: { x: 80, y: 22, w: 32 }, // 对话气泡头部右上
  heart: { x: 22, y: 22, w: 22 }, // 爱心头部左上
  zzz: { x: 80, y: 18, w: 26 }, // ZZZ 头顶
  sweat: { x: 80, y: 45, w: 14 }, // 汗珠头部右侧
  coin: { x: 50, y: 25, w: 18 }, // 钱币（预留，当前状态机未使用）
};

/** 按装饰 id 取定位规格（'none' / 未定义返回 undefined） */
export function decoSpec(d: PetDecoration): PartSpec | undefined {
  return DECO_SPECS[d];
}
