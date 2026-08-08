<!--
  pet/skins/popmart3d/PopMartPandaPet.vue
  "Pop Mart 3D 潮玩" 皮肤渲染器（v0.6.2 起）。

  v0.6.2-beta.27 设计变更：
  - 放弃四层 body / eyes / nose / mouth 切图：切片错位 + body 底图羽化填色，
    会在五官边缘产生重影/光晕，是「图片割裂」的根因；CSS 投影只能缓解阴影浮层。
  - 改为单张透明 PNG 渲染，从根本上消除层间错位与重影。
  - 眼睛/嘴的动态由 emoji 浮层与整体状态动画替代；保留整体 idle 浮动、状态专属动画、
    点击动效、拖拽走路、系统过载升温等全部交互。

  设计：
  - 主视觉：透明底的 3D 熊猫（戴黄帽抱吉他），以 6 帧序列循环播放，营造呼吸 + 眯眼的微动态
  - 状态差异：emoji 浮层 + CSS 动画 + 整体 art 动效
  - 透明底：由多模态模型生成 + PIL 色键输出统一透明 PNG；帧序列由同一张源图 PIL 合成，绝对一致
  - 投影：filter: drop-shadow 作用于每帧静态 img，增强浮动立体感

  解耦点：父组件 PetSkinRenderer 把 props.state 传入即可，状态机/前台监听/喂食都不感知这是哪种皮肤。

  修改历史：
    - 2026-07-24 @v0.6.2-beta.1: 初始创建 - 参考图整图 + emoji 装饰浮层 + CSS 动画
    - 2026-07-24 @v0.6.2-beta.2: 透明底 - ImageGen 出绿底图 + PIL 色键抠真透明 PNG
    - 2026-07-24 @v0.6.2-beta.6: 表情 - usePetBadges + overrideBadges 预览 prop
    - 2026-07-24 @v0.6.2-beta.7: 动效 - 复合 idle + 6 个状态专属动画
    - 2026-07-25 @v0.6.2-beta.15: 拖拽走路 + 系统过载升温
    - 2026-07-28 @v0.6.2-beta.25: 分层 - 拆 body/eyes/nose/mouth 四层，眼神+嘴动态化
    - 2026-08-05 @v0.6.2-beta.26: 优化 - 将四层独立投影改为整体统一投影，消除五官割裂感
    - 2026-08-06 @v0.6.2-beta.27: 重构 - 单张透明 PNG 替代四层切图，从根上修复割裂
    - 2026-08-06 @v0.6.2-beta.27: 性能 - 投影由动画 art 层下移静态 sprite，消除透明置顶窗每帧重算投影的卡顿；升温态去掉 hue-rotate 滤镜动画（仅保留 transform 抖动）
    - 2026-08-06 @v0.6.2-beta.28: 体验 - 表情 emoji 浮层由压在脸上改为聚到熊猫头顶上方；idle 增加呼吸缩放让桌宠更"活"
    - 2026-08-06 @v0.6.2-beta.29: 动画 - 单张 PNG 升级为 6 帧序列（PIL 合成：呼吸缩放 + 眯眼），CSS 逐帧播放；idle 仅保留 translate/rotate 浮动，避免与帧序列双重缩放
-->
<template>
  <div class="popmart-pet" :class="containerClass">
    <!--
      6 帧透明 PNG 序列：同一只熊猫源图 PIL 合成，帧间仅有呼吸缩放 + 眯眼差异。
      用 CSS opacity + steps(1) 做逐帧播放；.popmart-pet__art 仍承载 idle / 状态 / 点击 / 拖拽走路 / 过载升温 的复合动画。
    -->
    <div class="popmart-pet__art">
      <img
        v-for="(url, i) in frameUrls"
        :key="i"
        :src="url"
        class="popmart-pet__frame"
        :alt="`Pop Mart panda frame ${i}`"
        draggable="false"
        :style="{ animationDelay: `-${(FRAME_COUNT - i) * FRAME_DURATION}s` }"
      />
    </div>

    <!-- 状态装饰浮层 -->
    <div class="popmart-pet__badges" :class="badgeLayout">
      <span
        v-for="(badge, i) in badges"
        :key="state + '-' + i"
        class="popmart-pet__badge"
        :style="{ animationDelay: i * 0.12 + 's' }"
        >{{ badge }}</span
      >
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { PetState } from '../../types';
import frame0 from './assets/popmart-panda-frame-0.png';
import frame1 from './assets/popmart-panda-frame-1.png';
import frame2 from './assets/popmart-panda-frame-2.png';
import frame3 from './assets/popmart-panda-frame-3.png';
import frame4 from './assets/popmart-panda-frame-4.png';
import frame5 from './assets/popmart-panda-frame-5.png';
import { usePetBadges } from '../../composables/usePetBadges';

/** 帧动画总时长（秒） */
const FRAME_TOTAL_DURATION = 1.2;
/** 帧数 */
const FRAME_COUNT = 6;
/** 单帧时长 */
const FRAME_DURATION = FRAME_TOTAL_DURATION / FRAME_COUNT;
const frameUrls = [frame0, frame1, frame2, frame3, frame4, frame5];

const props = defineProps<{ state: PetState; overrideBadges?: string[] | null }>();
const { getCustomBadge } = usePetBadges();

// ---- 表情层（emoji 浮层） ----
const STATE_BADGES: Record<PetState, string[]> = {
  idle: ['♡'],
  working: ['💼'],
  developing: ['{ }'],
  designing: ['✎'],
  gaming: ['🎮'],
  chatting: ['💬'],
  meeting: ['📊'],
  listening: ['♪'],
  shopping: ['♥'],
  eating: ['🍎'],
  sleeping: ['z', 'Z', 'Z'],
  slacking: ['…'],
  happy: ['♥', '✨'],
  sad: ['💧'],
  angry: ['!', '💢'],
  surprised: ['!'],
};
const STATE_BADGE_LAYOUT: Record<PetState, 'single' | 'double' | 'triple' | 'scatter'> = {
  idle: 'single', working: 'single', developing: 'double', designing: 'double',
  gaming: 'single', chatting: 'single', meeting: 'single', listening: 'single',
  shopping: 'double', eating: 'single', sleeping: 'triple', slacking: 'single',
  happy: 'scatter', sad: 'single', angry: 'double', surprised: 'single',
};
const badges = computed<string[]>(() => {
  if (props.overrideBadges && props.overrideBadges.length) return props.overrideBadges;
  const custom = getCustomBadge(props.state);
  if (custom) return Array.from(custom);
  return STATE_BADGES[props.state] ?? [];
});
const badgeLayout = computed(() => STATE_BADGE_LAYOUT[props.state] ?? 'single');
const containerClass = computed(() => `state-${props.state}`);
</script>

<style scoped>
/* ========== 容器 ========== */
.popmart-pet {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  background: transparent;
  user-select: none;
  -webkit-user-select: none;
  pointer-events: none;
  transition: transform 0.2s ease;
}

/* ========== 单张精灵 art 容器 ========== */
.popmart-pet__art {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  /* 复合 idle + 状态 + 点击 + 拖拽 + 过载 动画 全部作用于此层（仅 transform，确保进入 GPU 合成层、不每帧重绘） */
  animation: pm-panda-idle 4.2s ease-in-out infinite;
  transform-origin: 50% 90%;
  will-change: transform;
}

.popmart-pet__frame {
  position: absolute;
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
  pointer-events: none;
  /* 投影放在静态 img 上（img 不跑动画）：投影只光栅化一次并缓存；
     若放到上方 .art 会与 transform 动画同层，导致透明置顶窗每帧重算投影 → 卡顿 */
  filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.28))
          drop-shadow(0 1px 2px rgba(0, 0, 0, 0.18));
  /* 帧序列：默认不可见，由 pm-frame-play 在各自时间窗内显示一帧；
     不用 steps() —— 此处是逐帧 opacity 硬切，steps(1) 反而会让每帧整周期常显 */
  opacity: 0;
  animation: pm-frame-play 1.2s infinite;
}
@keyframes pm-frame-play {
  0%, 16.66% { opacity: 1; }
  16.67%, 100% { opacity: 0; }
}

/* ========== 状态级动画（作用于整体 art） ========== */
.popmart-pet.state-sleeping .popmart-pet__art {
  animation: pm-panda-breathe 3.6s ease-in-out infinite;
}
.popmart-pet.state-sleeping .popmart-pet__frame {
  /* 保留投影前缀，仅叠加变暗；静态 filter 在状态切换时算一次，不随动画重绘 */
  filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.28))
          drop-shadow(0 1px 2px rgba(0, 0, 0, 0.18))
          brightness(0.92);
}
.popmart-pet.state-working .popmart-pet__art,
.popmart-pet.state-developing .popmart-pet__art,
.popmart-pet.state-designing .popmart-pet__art {
  animation: pm-panda-focus 1.8s ease-in-out infinite;
}
.popmart-pet.state-happy .popmart-pet__art {
  animation: pm-panda-pop 0.6s ease 1, pm-panda-happy-spin 1.2s ease 1;
}
.popmart-pet.state-angry .popmart-pet__art {
  animation: pm-panda-shake 0.4s ease 3;
}
.popmart-pet.state-surprised .popmart-pet__art {
  animation: pm-panda-startle 0.5s ease 1;
}
.popmart-pet.state-slacking .popmart-pet__art {
  animation: pm-panda-slouch 5.5s ease-in-out infinite;
}
.popmart-pet.state-listening .popmart-pet__art {
  animation: pm-panda-sway 2.6s ease-in-out infinite;
}
.popmart-pet.state-gaming .popmart-pet__art {
  animation: pm-panda-shake-fast 0.35s ease infinite;
}
.popmart-pet.state-chatting .popmart-pet__art,
.popmart-pet.state-meeting .popmart-pet__art {
  animation: pm-panda-bob 1.8s ease-in-out infinite;
}
.popmart-pet.state-eating .popmart-pet__art {
  animation: pm-panda-nod 0.9s ease-in-out 3;
}
.popmart-pet.state-shopping .popmart-pet__art {
  animation: pm-panda-bob 2.4s ease-in-out infinite;
}

/* 拖拽走路 */
.popmart-pet.is-walking .popmart-pet__art {
  animation: pm-panda-walk 0.42s linear infinite;
}
/* 系统过载升温：仅用 transform 抖动（合成层便宜，透明窗每帧零重绘）；
   🔥 视觉提示由 PetWindow 的 .pet-overheat-badge 提供；
   不再用 hue-rotate 滤镜动画——hue-rotate 每帧重算颜色，是顶级开销且破坏合成层 */
.popmart-pet.is-heating .popmart-pet__art {
  animation: pm-panda-overheat-shake 0.18s linear infinite;
}
/* v0.7.0：喂食反应（菜单 pet-fed → 桌宠进食点头 + 飘「好吃」气泡）；
   与 is-heating 同特异性，置于其后保证进食反馈优先 */
.popmart-pet.is-fed .popmart-pet__art {
  animation: pm-panda-nod 0.7s ease-in-out 2;
}

/* ========== 表情浮层（emoji 装饰） ========== */
/* v0.6.2-beta.28：聚到窗口顶部（熊猫头顶上方），不再压在脸上 */
.popmart-pet__badges {
  position: absolute;
  top: 4px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  align-items: flex-start;
  gap: 2px;
  max-width: 92%;
  pointer-events: none;
  z-index: 6;
}
/* 不同状态仅微调间距，统一贴在头顶 */
.popmart-pet__badges.double { gap: 2px 3px; }
.popmart-pet__badges.triple { gap: 2px 3px; }
.popmart-pet__badges.scatter { gap: 3px; }
.popmart-pet__badge {
  display: inline-block;
  font-size: 12px;
  background: rgba(255, 255, 255, 0.85);
  border-radius: 8px;
  padding: 1px 4px;
  margin: 1px;
  font-family: -apple-system, "Segoe UI Emoji", sans-serif;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  animation: pm-badge-pop 0.6s ease-out backwards;
}
@keyframes pm-badge-pop {
  0% { opacity: 0; transform: scale(0.4); }
  100% { opacity: 1; transform: scale(1); }
}

/* ========== 点击动效（作用于根） ========== */
.popmart-pet.pet-anim-bounce { animation: pm-click-bounce 0.4s ease; }
.popmart-pet.pet-anim-shake  { animation: pm-click-shake 0.4s ease; }
.popmart-pet.pet-anim-spin   { animation: pm-click-spin 0.6s ease; }
.popmart-pet.pet-anim-shrink { animation: pm-click-shrink 0.5s ease; }
.popmart-pet.pet-anim-jump   { animation: pm-click-jump 0.45s cubic-bezier(0.3, 1.5, 0.5, 1); }
.popmart-pet.pet-anim-squash { animation: pm-click-squash 0.5s ease; }
.popmart-pet.pet-anim-jolt   { animation: pm-click-jolt 0.32s ease; }

/* ========== 所有 keyframes ========== */
/* v0.6.2-beta.28：加入呼吸缩放（scaleY 起伏），让桌宠静止时也"活着"；
   v0.6.2-beta.29：移除 idle 的 scale，呼吸改由 6 帧 PIL 序列承担，避免双重缩放 */
@keyframes pm-panda-idle {
  0%   { transform: translateY(0) rotate(-0.8deg); }
  25%  { transform: translateY(-4px) rotate(0.8deg); }
  50%  { transform: translateY(-7px) rotate(-0.4deg); }
  75%  { transform: translateY(-3px) rotate(0.6deg); }
  100% { transform: translateY(0) rotate(-0.8deg); }
}
@keyframes pm-panda-breathe {
  0%, 100% { transform: scale(1); }
  50%      { transform: scale(1.025); }
}
@keyframes pm-panda-bob {
  0%, 100% { transform: translateY(0); }
  50%      { transform: translateY(-4px); }
}
@keyframes pm-panda-focus {
  0%, 100% { transform: translateY(0) scale(1); }
  50%      { transform: translateY(-1.5px) scale(1.008); }
}
@keyframes pm-panda-pop {
  0%   { transform: scale(1); }
  35%  { transform: scale(1.16); }
  65%  { transform: scale(0.96); }
  100% { transform: scale(1); }
}
@keyframes pm-panda-happy-spin {
  0%   { transform: rotate(0); }
  100% { transform: rotate(8deg); }
}
@keyframes pm-panda-shake {
  0%, 100% { transform: translateX(0); }
  25%      { transform: translateX(-3px); }
  75%      { transform: translateX(3px); }
}
@keyframes pm-panda-shake-fast {
  0%, 100% { transform: translateX(0); }
  25%      { transform: translateX(-1.5px); }
  75%      { transform: translateX(1.5px); }
}
@keyframes pm-panda-startle {
  0%   { transform: scale(1) rotate(0); }
  30%  { transform: scale(1.12) rotate(-4deg); }
  60%  { transform: scale(1.05) rotate(2deg); }
  100% { transform: scale(1) rotate(0); }
}
@keyframes pm-panda-slouch {
  0%, 100% { transform: rotate(-6deg) translateY(0); }
  50%      { transform: rotate(6deg) translateY(2px); }
}
@keyframes pm-panda-sway {
  0%, 100% { transform: rotate(-2deg); }
  50%      { transform: rotate(2deg); }
}
@keyframes pm-panda-nod {
  0%, 100% { transform: translateY(0) rotate(0); }
  50%      { transform: translateY(2px) rotate(-3deg); }
}
@keyframes pm-panda-walk {
  0%   { transform: rotate(-3deg) translateY(0); }
  25%  { transform: rotate(0deg) translateY(-2px); }
  50%  { transform: rotate(3deg) translateY(0); }
  75%  { transform: rotate(0deg) translateY(-2px); }
  100% { transform: rotate(-3deg) translateY(0); }
}
@keyframes pm-panda-overheat-shake {
  0%, 100% { transform: translate(0, 0); }
  25%      { transform: translate(-0.8px, 0.4px) rotate(-0.4deg); }
  50%      { transform: translate(0.7px, -0.5px) rotate(0.3deg); }
  75%      { transform: translate(-0.4px, -0.4px) rotate(-0.2deg); }
}
@keyframes pm-click-bounce {
  0%, 100% { transform: scale(1); }
  40% { transform: scale(1.15) translateY(-4px); }
}
@keyframes pm-click-shake {
  0%, 100% { transform: translateX(0); }
  20%, 60% { transform: translateX(-4px); }
  40%, 80% { transform: translateX(4px); }
}
@keyframes pm-click-spin {
  0% { transform: rotate(0); }
  100% { transform: rotate(360deg); }
}
@keyframes pm-click-shrink {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(0.85); }
}
@keyframes pm-click-jump {
  0%   { transform: translateY(0) scale(1, 1); }
  35%  { transform: translateY(-22px) scale(0.94, 1.08); }
  60%  { transform: translateY(-12px) scale(1.06, 0.94); }
  100% { transform: translateY(0) scale(1, 1); }
}
@keyframes pm-click-squash {
  0%   { transform: scale(1, 1); }
  30%  { transform: scale(1.35, 0.7); }
  55%  { transform: scale(0.85, 1.18); }
  75%  { transform: scale(1.08, 0.92); }
  100% { transform: scale(1, 1); }
}
@keyframes pm-click-jolt {
  0%, 100% { transform: translate(0, 0) rotate(0); }
  15%      { transform: translate(-3px, -2px) rotate(-3deg); }
  30%      { transform: translate(3px, 1px) rotate(4deg); }
  45%      { transform: translate(-2px, 1px) rotate(-2deg); }
  60%      { transform: translate(3px, -1px) rotate(3deg); }
  80%      { transform: translate(-1px, 1px) rotate(-1deg); }
}
</style>
