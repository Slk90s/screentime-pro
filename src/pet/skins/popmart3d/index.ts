/**
 * pet/skins/popmart3d/index.ts
 * "Pop Mart 3D 潮玩" 皮肤（v0.6.2 注册；v0.6.2-beta.2 启用透明底浮动版）。
 *
 * 适用：偏好「一张完整潮玩角色浮在桌面」风格、不在意部件自定义的场景。
 * 视觉：透明底的戴黄帽抱吉他 3D 熊猫（已 color-key 抠图）+ drop-shadow 投影，状态差异通过 emoji 浮层 + CSS 动画表现。
 *
 * 使用完整角色图与 emoji 浮层；状态自定义经 usePetBadges 同步。
 *
 * 修改历史：
 *   - 2026-07-24 @v0.6.2-beta.1: 初始注册 - Pop Mart 3D 风皮肤（橙底参考图）
 *   - 2026-07-24 @v0.6.2-beta.2: 透明底 - 切到 popmart-panda-transparent.png，浮动显示
 */
import type { PetSkinManifest } from '../types';
import PopMartPandaPet from './PopMartPandaPet.vue';

const manifest: PetSkinManifest = {
  id: 'popmart-3d',
  name: 'Pop Mart 3D',
  version: '0.6.2',
  description: '潮玩 3D 卡：戴黄帽抱吉他的透明底熊猫浮在桌面，状态用 emoji 浮层表达',
  // v0.6.2：竖图声明更高窗，避免被 140×140 方形窗裁切；2D 皮肤不受影响
  preferredSize: { w: 150, h: 330 },
  renderer: PopMartPandaPet,
};

export default manifest;
