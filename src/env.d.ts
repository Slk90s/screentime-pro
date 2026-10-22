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
 *   - 2026-10-22 @v1.2.5: 新增 - *.webp 声明。桌宠帧/姿势图由 PNG 换为 WebP 后，
 *     `vue-tsc --noEmit`（`npm run build` 的第一步）会报 TS2307 找不到模块，
 *     而 `vite build` 单独跑却能过 —— 两套解析器行为不一致，必须在 Vite 侧配置
 *     `assetsInclude` 之外再补这份 TS 声明。漏了这条正式构建会直接失败。
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
declare module "*.webp" {
  const src: string;
  export default src;
}