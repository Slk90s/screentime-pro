/**
 * pet/skins/index.ts
 * 默认皮肤注册表引导（v0.6.2 新增）。
 *
 * 副作用导入：在 PetWindow.vue 顶部 import './skins' 即可保证：
 *   1. popmart-3d 注册到 skinRegistry（v0.6.2-beta.5 起唯一/默认皮肤）
 *   2. 用户之前选的 active skin id 仍生效
 *   3. 若用户没选过且没注册任何皮肤，popmart-3d 会自动落位
 *
 * 新增皮肤时：在本文件追加 register 调用即可，无需修改 PetWindow / PetCanvas。
 *
 * 修改历史：
 *   - 2026-07-24 @v0.6.2: 初始创建 - 引导两个内置皮肤
 *   - 2026-07-24 @v0.6.2-beta.5: 废弃 - 移除 panda-2d 引导，仅注册 popmart-3d
 */
import popmartManifest from './popmart3d';
import spidermanManifest from './spiderman';
import { skinRegistry } from './registry';

// 注册内置皮肤（v0.6.2-beta.5 起 popmart-3d 为默认；beta.30 新增 spiderman）
skinRegistry.register(popmartManifest);
skinRegistry.register(spidermanManifest);

// 重新导出供其他模块使用
export { skinRegistry } from './registry';
export type { PetSkinManifest, PetSkinRegistry } from './types';
