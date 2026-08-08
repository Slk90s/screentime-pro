/**
 * pet/skins/spiderman/index.ts
 * "Spider-Man 风格" 桌宠皮肤（v0.6.2-beta.30 新增）。
 *
 * 视觉：透明底的「红蓝战衣 + 蛛网纹 + 蛛形徽章」Q 版英雄浮在桌面，6 帧呼吸序列循环；
 * 待机时会随机切换 AI 生成的真姿势精灵（荡丝 / 英雄 pose / 射蛛丝 / 蹲防），肢体真正改变姿态。
 * 源图由多模态模型生成（绿底 → PIL 绿键抠透明），姿势图由图生图锁定同一只角色。
 *
 * 与 popmart3d 的差异：仅替换素材与类名前缀，动画/emoji/状态/交互体系完全复用。
 *
 * 修改历史：
 *   - 2026-08-06 @v0.6.2-beta.30: 初始注册 - Spider-Man 风格皮肤
 */
import type { PetSkinManifest } from '../types';
import SpiderManPet from './SpiderManPet.vue';

const manifest: PetSkinManifest = {
  id: 'spiderman',
  name: 'Spider-Man',
  version: '0.6.2',
  description: '红蓝战衣蛛网纹 Q 版英雄浮在桌面，6 帧呼吸微动态，状态用 emoji 浮层表达',
  // 竖图声明更高窗，避免被 140×140 方形窗裁切（与 popmart3d 同比例）
  preferredSize: { w: 150, h: 330 },
  renderer: SpiderManPet,
};

export default manifest;
