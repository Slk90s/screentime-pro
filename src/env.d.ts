/**
 * env.d.ts
 * 全局 TypeScript 模块声明。
 *
 * 设计思路：
 * - Vite 默认会把 .png 等静态资源 import 成 URL 字符串，但 vue-tsc 不认，
 *   需要显式声明 *.png 模块返回 string（与 Vite 行为对齐）
 * - 集中放在 src/env.d.ts 而非每个资源旁，vue-tsc 自动 include
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 新增 - 桌宠 sprite PNG 模块声明
 */

declare module "*.png" {
  const src: string;
  export default src;
}
declare module "*.jpg" {
  const src: string;
  export default src;
}
declare module "*.svg" {
  const src: string;
  export default src;
}