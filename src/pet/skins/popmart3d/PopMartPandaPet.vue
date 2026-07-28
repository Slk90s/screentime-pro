<!--
  pet/skins/popmart3d/PopMartPandaPet.vue
  "Pop Mart 3D 潮玩" 皮肤渲染器（v0.6.2 起）。

  v0.6.2-beta.25 设计变更：
  - 原单张烤死表情的 PNG 拆为四层独立透明 PNG：
      body(无眼/无鼻底图) + eyes(两眼弧) + nose(小鼻子) + mouth(合成小嘴)
    渲染时按 body→mouth→nose→eyes 顺序叠放，每层同 499×640 同坐标。
  - 眼睛可动（v0.6.2-beta.25）：
      · JS 定时器每 2s 微调视线 (随机 ±2px)
      · JS 定时器每 3~5s 眨眼 (scaleY 收缩 150ms)
      · 不同 PetState 叠加持续视线偏移 (surprised/sad/angry/...)
  - 嘴可动（v0.6.2-beta.25）：
      · 状态为 happy/chatting/surprised/angry/developing → 张合 (scaleY 打开)
      · 状态为 idle/sleeping/sad → 闭合
  - 保留原有 16 状态 CSS 动画与点击 jump/squash/jolt（作用于整体 .popmart-pet__art，
    四层一起运动；点击动画作用于根 .popmart-pet）

  设计：
  - 主视觉：透明底的 3D 熊猫（戴黄帽抱吉他），浮动在桌宠窗内
  - 状态差异：emoji 浮层 + CSS 动画 + 眼睛/嘴动效
  - 透明底生成：原图 → PIL 抠出 eyes/nose 透明层 + 底图对应区域羽化填肤色
  - 投影：filter: drop-shadow 增强浮动立体感

  解耦点：父组件 PetSkinRenderer 把 props.state 传入即可，状态机/前台监听/喂食都不感知这是哪种皮肤。

  修改历史：
    - 2026-07-24 @v0.6.2-beta.1: 初始创建 - 参考图整图 + emoji 装饰浮层 + CSS 动画
    - 2026-07-24 @v0.6.2-beta.2: 透明底 - ImageGen 出绿底图 + PIL 色键抠真透明 PNG
    - 2026-07-24 @v0.6.2-beta.6: 表情 - usePetBadges + overrideBadges 预览 prop
    - 2026-07-24 @v0.6.2-beta.7: 动效 - 复合 idle + 6 个状态专属动画
    - 2026-07-25 @v0.6.2-beta.15: 拖拽走路 + 系统过载升温
    - 2026-07-28 @v0.6.2-beta.25: 分层 - 拆 body/eyes/nose/mouth 四层，眼神+嘴动态化
-->
<template>
  <div class="popmart-pet" :class="containerClass">
    <!--
      四层同坐标叠放：body(底图) → mouth(嘴) → nose(鼻) → eyes(眼)
      每层都 width:100% height:100% object-fit:contain，container 也是，
      故四层在同一可视矩形内对齐（assets 尺寸一致 499×640）。
      整体 .popmart-pet__art 承载 idle / 状态 / 点击的复合动画。
    -->
    <div class="popmart-pet__art">
      <img
        :src="bodyUrl"
        class="popmart-pet__layer popmart-pet__body"
        :alt="`Pop Mart panda body ${state}`"
        draggable="false"
      />
      <img
        :src="mouthUrl"
        class="popmart-pet__layer popmart-pet__mouth"
        :class="{ 'is-talking': isTalking }"
        :style="mouthStyle"
        alt="mouth"
        draggable="false"
      />
      <img
        :src="noseUrl"
        class="popmart-pet__layer popmart-pet__nose"
        :style="noseStyle"
        alt="nose"
        draggable="false"
      />
      <img
        :src="eyesUrl"
        class="popmart-pet__layer popmart-pet__eyes"
        :class="{ 'is-blinking': isBlinking }"
        :style="eyeStyle"
        :alt="`Pop Mart panda eyes ${state}`"
        draggable="false"
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
import { computed, onMounted, onBeforeUnmount, ref } from 'vue';
import type { PetState } from '../../types';
import bodyUrl from './assets/popmart-panda-body.png';
import eyesUrl from './assets/popmart-panda-eyes.png';
import noseUrl from './assets/popmart-panda-nose.png';
import mouthUrl from './assets/popmart-panda-mouth.png';
import { usePetBadges } from '../../composables/usePetBadges';

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

// ---- 眼神 + 眨眼（v0.6.2-beta.25） ----
// 持续视线偏移（按状态）：surprised=上看、sad=下看+左、angry=右看、gaming=盯、happy=微左
const STATE_EYE_OFFSET: Partial<Record<PetState, { x: number; y: number }>> = {
  idle: { x: 0, y: 0 },
  working: { x: -0.5, y: 1 },     // 微低
  developing: { x: 0, y: 1 },      // 盯屏幕下
  designing: { x: 1, y: -0.5 },
  gaming: { x: 1, y: 0.5 },        // 盯前方
  chatting: { x: 0, y: 0 },
  meeting: { x: 0, y: -1 },
  listening: { x: 0.5, y: 0 },
  shopping: { x: 1, y: 0 },
  eating: { x: 0, y: 1.5 },
  sleeping: { x: 0, y: 0 },        // 闭眼态（眼睛仍叠但动画被吞）
  slacking: { x: -1, y: 1 },
  happy: { x: 0.5, y: -0.5 },
  sad: { x: -1.2, y: 1.2 },        // 下左
  angry: { x: 1.5, y: -0.5 },      // 瞪右上
  surprised: { x: 0, y: -1.8 },    // 上看
};
const eyeGazeX = ref(0);
const eyeGazeY = ref(0);
let gazeTimer: number | null = null;

// idle 微漂（每 1.8s 轻微随机偏移 ±1.5px，叠加在状态偏移上）
function driftGaze(): void {
  const stateOffset = STATE_EYE_OFFSET[props.state] ?? { x: 0, y: 0 };
  eyeGazeX.value = stateOffset.x + (Math.random() - 0.5) * 1.6;
  eyeGazeY.value = stateOffset.y + (Math.random() - 0.5) * 1.2;
}

// 眨眼（每 3.2~5.0s 触发，闭 150ms）
const isBlinking = ref(false);
let blinkTimer: number | null = null;
function scheduleBlink(): void {
  const delay = 3200 + Math.random() * 1800;
  blinkTimer = window.setTimeout(() => {
    isBlinking.value = true;
    window.setTimeout(() => { isBlinking.value = false; }, 140);
    scheduleBlink();
  }, delay);
}

const eyeStyle = computed(() => ({
  transform: `translate(${eyeGazeX.value}px, ${eyeGazeY.value}px)`,
}));

// ---- 嘴部张合 ----
const TALKING_STATES: PetState[] = ['happy', 'chatting', 'surprised', 'angry', 'developing'];
const isTalking = computed(() => TALKING_STATES.includes(props.state));
// 嘴部微动：说话时小幅垂直呼吸 (scaleY 1↔1.4)
const mouthBobY = ref(0);
let mouthTimer: number | null = null;
function scheduleMouthBob(): void {
  mouthTimer = window.setTimeout(() => {
    if (isTalking.value) {
      mouthBobY.value = Math.sin(Date.now() / 120) * 0.6;
    } else {
      mouthBobY.value = 0;
    }
    scheduleMouthBob();
  }, 140);
}
const mouthStyle = computed(() => ({
  transform: `translateY(${mouthBobY.value}px)`,
}));

// ---- 鼻子：轻动（惊讶/开心时缩放） ----
const noseStyle = computed(() => {
  if (props.state === 'surprised') return { transform: 'scale(1.15)' };
  if (props.state === 'happy') return { transform: 'scale(1.06)' };
  return {};
});

onMounted(() => {
  driftGaze();
  gazeTimer = window.setInterval(driftGaze, 1800);
  scheduleBlink();
  scheduleMouthBob();
});
onBeforeUnmount(() => {
  if (gazeTimer !== null) clearInterval(gazeTimer);
  if (blinkTimer !== null) clearTimeout(blinkTimer);
  if (mouthTimer !== null) clearTimeout(mouthTimer);
});
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

/* ========== 四层叠放 art 容器 ========== */
.popmart-pet__art {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  /* 复合 idle + 状态 + 点击 动画 全部作用于此层 */
  animation: pm-panda-idle 4.2s ease-in-out infinite;
  transform-origin: 50% 90%;
  will-change: transform;
}

.popmart-pet__layer {
  position: absolute;
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
  pointer-events: none;
  filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.28))
          drop-shadow(0 1px 2px rgba(0, 0, 0, 0.18));
}
.popmart-pet__body {
  z-index: 1;
}
.popmart-pet__mouth {
  z-index: 2;
  /* 嘴默认 scaleY(1)；说话时张开 (CSS keyframe) */
  transform-origin: 50% 50%;
  transition: transform 0.18s ease;
}
.popmart-pet__mouth.is-talking {
  animation: pm-mouth-talk 0.36s ease-in-out infinite alternate;
  transform-origin: 50% 50%;
}
.popmart-pet__nose {
  z-index: 3;
  transition: transform 0.2s ease;
}
.popmart-pet__eyes {
  z-index: 4;
  transform-origin: 50% 50%;
  transition: transform 0.25s ease;
}
.popmart-pet__eyes.is-blinking {
  animation: pm-blink 0.14s ease-in-out;
  transform-origin: 50% 50%;
}

/* ========== 状态级动画（作用于整体 art） ========== */
.popmart-pet.state-sleeping .popmart-pet__art {
  animation: pm-panda-breathe 3.6s ease-in-out infinite;
}
.popmart-pet.state-sleeping .popmart-pet__layer {
  filter: brightness(0.92);
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
/* 系统过载升温 */
.popmart-pet.is-heating .popmart-pet__art {
  animation:
    pm-panda-overheat-shake 0.18s linear infinite,
    pm-panda-overheat-tint 1.2s ease-in-out infinite alternate;
}
.popmart-pet.is-heating .popmart-pet__layer {
  filter:
    drop-shadow(0 4px 8px rgba(0, 0, 0, 0.28))
    drop-shadow(0 1px 2px rgba(0, 0, 0, 0.18))
    hue-rotate(-12deg) saturate(1.6) brightness(1.05);
}

/* ========== 表情/眼/嘴 keyframes ========== */
@keyframes pm-blink {
  0%, 100% { transform: scaleY(1); }
  50%      { transform: scaleY(0.08); }
}
@keyframes pm-mouth-talk {
  0%   { transform: scaleY(1)   translateY(0); }
  100% { transform: scaleY(1.7) translateY(0); }
}
@keyframes pm-panda-overheat-shake {
  0%, 100% { transform: translate(0, 0); }
  25%      { transform: translate(-0.8px, 0.4px) rotate(-0.4deg); }
  50%      { transform: translate(0.7px, -0.5px) rotate(0.3deg); }
  75%      { transform: translate(-0.4px, -0.4px) rotate(-0.2deg); }
}
@keyframes pm-panda-overheat-tint {
  0%   { filter: brightness(1.0) hue-rotate(-10deg); }
  50%  { filter: brightness(1.1) hue-rotate(-18deg) saturate(1.5); }
  100% { filter: brightness(1.0) hue-rotate(-10deg); }
}
@keyframes pm-panda-walk {
  0%   { transform: rotate(-3deg) translateY(0); }
  25%  { transform: rotate(0deg) translateY(-2px); }
  50%  { transform: rotate(3deg) translateY(0); }
  75%  { transform: rotate(0deg) translateY(-2px); }
  100% { transform: rotate(-3deg) translateY(0); }
}
@keyframes pm-panda-idle {
  0%   { transform: translateY(0) rotate(-0.6deg) scale(1); }
  20%  { transform: translateY(-3px) rotate(0.8deg) scale(1.005); }
  40%  { transform: translateY(-5px) rotate(-0.4deg) scale(1.012); }
  60%  { transform: translateY(-3px) rotate(0.6deg) scale(1.008); }
  80%  { transform: translateY(-1px) rotate(-0.8deg) scale(1.002); }
  100% { transform: translateY(0) rotate(-0.6deg) scale(1); }
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

/* ========== 表情浮层（emoji 装饰） ========== */
.popmart-pet__badges {
  position: absolute;
  inset: 0;
  pointer-events: none;
}
.popmart-pet__badges.single {
  display: flex; align-items: center; justify-content: center;
}
.popmart-pet__badges.double {
  display: flex; flex-direction: column;
  align-items: flex-end; justify-content: flex-start;
  padding: 4px 6px; gap: 2px;
}
.popmart-pet__badges.triple {
  display: flex; align-items: center; justify-content: center; gap: 2px;
  padding-top: 6px;
}
.popmart-pet__badges.scatter {
  display: flex; flex-wrap: wrap; align-content: space-around; justify-content: space-around;
  padding: 8px;
}
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

/* ========== 点击动效（与原组件一致，作用于根） ========== */
.popmart-pet.pet-anim-bounce { animation: pm-click-bounce 0.4s ease; }
.popmart-pet.pet-anim-shake  { animation: pm-click-shake 0.4s ease; }
.popmart-pet.pet-anim-spin   { animation: pm-click-spin 0.6s ease; }
.popmart-pet.pet-anim-shrink { animation: pm-click-shrink 0.5s ease; }
.popmart-pet.pet-anim-jump   { animation: pm-click-jump 0.45s cubic-bezier(0.3, 1.5, 0.5, 1); }
.popmart-pet.pet-anim-squash { animation: pm-click-squash 0.5s ease; }
.popmart-pet.pet-anim-jolt   { animation: pm-click-jolt 0.32s ease; }
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