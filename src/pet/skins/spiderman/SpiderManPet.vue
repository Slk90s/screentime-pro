<!--
  pet/skins/spiderman/SpiderManPet.vue
  "Spider-Man 风格" 桌宠皮肤渲染器（v0.6.2-beta.30 新增）。

  工艺（与 popmart3d 完全一致）：
  - 源图由多模态模型生成「红蓝战衣 + 蛛网纹 + 蛛形徽章」Q 版英雄（绿底 → PIL 绿键抠真透明 PNG），
    存为 spiderman-single.png；帧序列由同一张源图 PIL 合成，绝对一致。
  - 6 帧透明 PNG 序列循环播放，营造呼吸微动态（面罩角色不画眯眼，呼吸即"活着"，避免盲画弧线风险）。
  - 状态差异：emoji 浮层 + CSS 动画 + 整体 art 动效（复用 popmart3d 的状态动画体系）。
  - 透明底 + drop-shadow 投影；投影放在静态 img 层（不随 transform 动画重绘，避免透明置顶窗每帧重算投影卡顿）。

  解耦点：父组件 PetSkinRenderer 把 props.state 传入即可；状态机/前台监听/喂食都不感知这是哪种皮肤。

  修改历史：
    - 2026-08-06 @v0.6.2-beta.30: 新增 - Spider-Man 风格皮肤（绿底生成 + 绿键抠图 + 6 帧呼吸序列），
      照搬 popmart3d 的动画/emoji/状态逻辑，类名前缀由 popmart-pet/pm- 改为 spidey-pet/spidey-。
    - 2026-08-07 @v0.6.2-beta.31: 优化 - 蜘蛛侠点击/待机/开心/惊讶一律不旋转（移除 idle/happy-spin/click-spin/
      click-jolt/startle 的 rotate，双击与单击仅做弹跳/位移），保留 listening/slacking 的轻微 ambient 摆动。
      新增 pet-anim-web：跳起射蛛丝网表情（内联 SVG 蛛丝网 + 跳跃），由 usePetInteractions 在蜘蛛侠皮肤下注入。
    - 2026-08-07 @v0.6.2-beta.32: 多动作 - 新增待机随机小动作调度器（荡丝 swing / 摆 pose / 射蛛丝 web / 蹲防 crouch），
      仅 state=idle 且非拖拽时触发（进入非 idle 立即收动作）；射蛛丝由静态网格弹出改为丝线绘制射出（dashoffset）
      + 末端蛛网团 + 角色投掷预备/甩出，不再死板。
    - 2026-08-07 @v0.6.2-33: 真姿势精灵 - swing/pose/web/crouch 不再对单张 idle 图做整体 transform，而是切换 AI
      生成的对应姿势图 + 轻量动画；idle 动作期间暂停底层呼吸帧，避免双动画叠加，角色肢体真正改变姿态。
    - 2026-08-08 @v0.6.2-34: 修透明底 - 将姿势图由 RGB 转为 RGBA 并移除棋盘格白底；优化射蛛丝 SVG 坐标
      （从手掌射出）、动画节奏（0.65s 更利落）与蛛网团弹出幅度。
    - 2026-08-08 @v0.6.2-35: 修复 - 动作触发时隐藏底层 idle 呼吸帧，避免姿势图与 idle 帧叠加重影；
      射蛛丝点击动效（pet-anim-web）也切换到 poseWeb 姿势图并隐藏 idle 帧，不再只是叠加图层。
    - 2026-08-08 @v0.6.2-36: 优化 - 表情编辑器新增 previewAction prop，可外部强制预览待机小动作；
      swing 动作隐藏叠加的静态 SVG 绳索（姿势图本身已含绳索并随摆动），避免“白丝不动”；
      清理 poseWeb 中残留的静态蛛网碎片。
-->
<template>
  <div ref="rootRef" class="spidey-pet" :class="[containerClass, effectiveAction, { 'is-action': !!effectiveAction }]">
    <div class="spidey-pet__art">
      <img
        v-for="(url, i) in frameUrls"
        :key="i"
        :src="url"
        class="spidey-pet__frame"
        :alt="`Spider-Man frame ${i}`"
        draggable="false"
        :style="{ animationDelay: `-${(FRAME_COUNT - i) * FRAME_DURATION}s` }"
      />
      <img
        v-if="actionPoseUrl"
        :src="actionPoseUrl"
        class="spidey-pet__pose"
        alt="Spider-Man action pose"
        draggable="false"
      />
    </div>

    <!-- 状态装饰浮层（聚到头顶上方，与 popmart3d beta.28 同款位置） -->
    <div class="spidey-pet__badges" :class="badgeLayout">
      <span
        v-for="(badge, i) in badges"
        :key="state + '-' + i"
        class="spidey-pet__badge"
        :style="{ animationDelay: i * 0.12 + 's' }"
        >{{ badge }}</span
      >
    </div>

    <!-- 荡丝绳（spidey-action-swing 时显示，从顶部垂下） -->
    <div class="spidey-pet__rope" aria-hidden="true">
      <svg viewBox="0 0 10 100" preserveAspectRatio="none" xmlns="http://www.w3.org/2000/svg">
        <line x1="5" y1="0" x2="5" y2="100" />
      </svg>
    </div>

    <!-- 蛛丝网（射蛛丝表情；默认隐藏，pet-anim-web / spidey-action-web 时从手部"射出"） -->
    <div class="spidey-pet__web" aria-hidden="true">
      <svg viewBox="0 0 160 160" xmlns="http://www.w3.org/2000/svg">
        <!-- 丝线：从右手掌心射向右上空，pathLength 归一化做"绘制射出"动画；
             坐标按 poseWeb 右手掌心在 150×330 竖窗中的位置校准（viewBox 约 95,68）。 -->
        <line class="web-silk" x1="95" y1="68" x2="150" y2="12" pathLength="100" />
        <!-- 末端蛛网团 -->
        <g class="web-clump" transform="translate(150,12)">
          <polygon points="34,0 17,29.4 -17,29.4 -34,0 -17,-29.4 17,-29.4" />
          <polygon points="22,0 11,19 -11,19 -22,0 -11,-19 11,-19" />
          <polygon points="11,0 5.5,9.5 -5.5,9.5 -11,0 -5.5,-9.5 5.5,-9.5" />
          <line x1="0" y1="0" x2="34" y2="0" />
          <line x1="0" y1="0" x2="17" y2="29.4" />
          <line x1="0" y1="0" x2="-17" y2="29.4" />
          <line x1="0" y1="0" x2="-34" y2="0" />
          <line x1="0" y1="0" x2="-17" y2="-29.4" />
          <line x1="0" y1="0" x2="17" y2="-29.4" />
        </g>
      </svg>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onMounted, onBeforeUnmount } from 'vue';
import type { PetState } from '../../types';
import frame0 from './assets/spiderman-frame-0.png';
import frame1 from './assets/spiderman-frame-1.png';
import frame2 from './assets/spiderman-frame-2.png';
import frame3 from './assets/spiderman-frame-3.png';
import frame4 from './assets/spiderman-frame-4.png';
import frame5 from './assets/spiderman-frame-5.png';
import poseSwing from './assets/spiderman-pose-swing.png';
import poseHero from './assets/spiderman-pose-hero.png';
import poseWeb from './assets/spiderman-pose-web.png';
import poseCrouch from './assets/spiderman-pose-crouch.png'; // v0.6.2-33: fix typo crouch→crouch
import { usePetBadges } from '../../composables/usePetBadges';

/** 帧动画总时长（秒） */
const FRAME_TOTAL_DURATION = 1.2;
/** 帧数 */
const FRAME_COUNT = 6;
/** 单帧时长 */
const FRAME_DURATION = FRAME_TOTAL_DURATION / FRAME_COUNT;
const frameUrls = [frame0, frame1, frame2, frame3, frame4, frame5];

/** 待机小动作对应的真姿势精灵图 */
const props = defineProps<{
  state: PetState;
  overrideBadges?: string[] | null;
  /** 外部强制预览某个待机小动作（表情编辑器用），优先级高于内部调度 */
  previewAction?: string;
}>();
const { getCustomBadge } = usePetBadges();

// ---- 表情层（emoji 浮层，与 popmart3d 通用） ----
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

// ---- 多动作：待机随机小动作（让蜘蛛侠"活"起来，不再只有呼吸） ----
// 仅在 state=idle 且未拖拽时触发，避免与状态动画/点击反应/拖拽冲突。
const IDLE_ACTIONS = ['spidey-action-swing', 'spidey-action-pose', 'spidey-action-web', 'spidey-action-crouch'] as const;
const idleAction = ref('');
/** 待机小动作对应的真姿势精灵图 */
const ACTION_POSES: Record<string, string> = {
  'spidey-action-swing': poseSwing,
  'spidey-action-pose': poseHero,
  'spidey-action-web': poseWeb,
  'spidey-action-crouch': poseCrouch,
};
const effectiveAction = computed(() => props.previewAction || idleAction.value);
const actionPoseUrl = computed(() => (effectiveAction.value ? ACTION_POSES[effectiveAction.value] : ''));
const rootRef = ref<HTMLElement | null>(null);
let idleTimer: number | null = null;
let actionTimer: number | null = null;

function clearIdleTimers(): void {
  if (idleTimer !== null) { clearTimeout(idleTimer); idleTimer = null; }
  if (actionTimer !== null) { clearTimeout(actionTimer); actionTimer = null; }
}
function scheduleNextIdleAction(): void {
  idleTimer = window.setTimeout(() => {
    const dragging = rootRef.value?.classList.contains('is-walking') ?? false;
    if (props.state === 'idle' && !dragging && !idleAction.value) {
      idleAction.value = IDLE_ACTIONS[Math.floor(Math.random() * IDLE_ACTIONS.length)];
      // 动作播完（取最长 swing 1.6s，留点余量）恢复待机，再排下一轮
      actionTimer = window.setTimeout(() => {
        idleAction.value = '';
        scheduleNextIdleAction();
      }, 1800);
    } else {
      scheduleNextIdleAction();
    }
  }, 4500 + Math.random() * 5000);
}
// 一旦进入非 idle（点状态/点击反应）立即收掉当前小动作，避免叠加冲突
watch(
  () => props.state,
  (s) => {
    if (s !== 'idle' && idleAction.value) {
      idleAction.value = '';
      if (actionTimer !== null) { clearTimeout(actionTimer); actionTimer = null; }
    }
  },
);
onMounted(scheduleNextIdleAction);
onBeforeUnmount(clearIdleTimers);
</script>

<style scoped>
/* ========== 容器 ========== */
.spidey-pet {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  user-select: none;
  -webkit-user-select: none;
  pointer-events: none;
  transition: transform 0.2s ease;
}

/* ========== 单张精灵 art 容器 ========== */
.spidey-pet__art {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  animation: spidey-idle 4.2s ease-in-out infinite;
  transform-origin: 50% 90%;
  will-change: transform;
}

.spidey-pet__frame {
  position: absolute;
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
  pointer-events: none;
  filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.28))
          drop-shadow(0 1px 2px rgba(0, 0, 0, 0.18));
  opacity: 0;
  animation: spidey-frame-play 1.2s infinite;
}
@keyframes spidey-frame-play {
  0%, 16.66% { opacity: 1; }
  16.67%, 100% { opacity: 0; }
}

/* 待机小动作的姿势精灵图：默认隐藏，动作时叠在 idle 帧之上并接管动画 */
.spidey-pet__pose {
  position: absolute;
  width: 100%;
  height: 100%;
  object-fit: contain;
  opacity: 0;
  z-index: 3;
  pointer-events: none;
  filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.28))
          drop-shadow(0 1px 2px rgba(0, 0, 0, 0.18));
}
/* 动作期间暂停底层 idle 呼吸动画，并彻底隐藏 idle 帧，
   避免姿势图与仍在循环的 6 帧 idle 图片叠加重影（v0.6.2-35）。
   表情编辑器 previewAction 同样属于“动作期间”，需要一并隐藏 idle 帧。 */
.spidey-pet.is-action .spidey-pet__art,
.spidey-pet.pet-anim-web .spidey-pet__art {
  animation: none;
}
.spidey-pet.is-action .spidey-pet__frame,
.spidey-pet.pet-anim-web .spidey-pet__frame {
  opacity: 0 !important;
  animation: none;
}

/* ========== 状态级动画（作用于整体 art） ========== */
.spidey-pet.state-sleeping .spidey-pet__art {
  animation: spidey-breathe 3.6s ease-in-out infinite;
}
.spidey-pet.state-sleeping .spidey-pet__frame {
  filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.28))
          drop-shadow(0 1px 2px rgba(0, 0, 0, 0.18))
          brightness(0.92);
}
.spidey-pet.state-working .spidey-pet__art,
.spidey-pet.state-developing .spidey-pet__art,
.spidey-pet.state-designing .spidey-pet__art {
  animation: spidey-focus 1.8s ease-in-out infinite;
}
.spidey-pet.state-happy .spidey-pet__art {
  animation: spidey-pop 0.6s ease 1;
}
.spidey-pet.state-angry .spidey-pet__art {
  animation: spidey-shake 0.4s ease 3;
}
.spidey-pet.state-surprised .spidey-pet__art {
  animation: spidey-startle 0.5s ease 1;
}
.spidey-pet.state-slacking .spidey-pet__art {
  animation: spidey-slouch 5.5s ease-in-out infinite;
}
.spidey-pet.state-listening .spidey-pet__art {
  animation: spidey-sway 2.6s ease-in-out infinite;
}
.spidey-pet.state-gaming .spidey-pet__art {
  animation: spidey-shake-fast 0.35s ease infinite;
}
.spidey-pet.state-chatting .spidey-pet__art,
.spidey-pet.state-meeting .spidey-pet__art {
  animation: spidey-bob 1.8s ease-in-out infinite;
}
.spidey-pet.state-eating .spidey-pet__art {
  animation: spidey-nod 0.9s ease-in-out 3;
}
.spidey-pet.state-shopping .spidey-pet__art {
  animation: spidey-bob 2.4s ease-in-out infinite;
}

/* 拖拽走路 */
.spidey-pet.is-walking .spidey-pet__art {
  animation: spidey-walk 0.42s linear infinite;
}
/* 系统过载升温：仅 transform 抖动（合成层便宜，透明窗每帧零重绘） */
.spidey-pet.is-heating .spidey-pet__art {
  animation: spidey-overheat-shake 0.18s linear infinite;
}
/* v0.7.0：喂食反应（菜单 pet-fed → 桌宠进食点头 + 飘「好吃」气泡） */
.spidey-pet.is-fed .spidey-pet__art {
  animation: spidey-nod 0.7s ease-in-out 2;
}

/* ========== 表情浮层（emoji 装饰） ========== */
.spidey-pet__badges {
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
.spidey-pet__badges.double { gap: 2px 3px; }
.spidey-pet__badges.triple { gap: 2px 3px; }
.spidey-pet__badges.scatter { gap: 3px; }
.spidey-pet__badge {
  display: inline-block;
  font-size: 12px;
  background: rgba(255, 255, 255, 0.85);
  border-radius: 8px;
  padding: 1px 4px;
  margin: 1px;
  font-family: -apple-system, "Segoe UI Emoji", sans-serif;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  animation: spidey-badge-pop 0.6s ease-out backwards;
}
@keyframes spidey-badge-pop {
  0% { opacity: 0; transform: scale(0.4); }
  100% { opacity: 1; transform: scale(1); }
}

/* ========== 点击动效（作用于根） ========== */
.spidey-pet.pet-anim-bounce { animation: spidey-click-bounce 0.4s ease; }
.spidey-pet.pet-anim-shake  { animation: spidey-click-shake 0.4s ease; }
.spidey-pet.pet-anim-spin   { animation: spidey-click-bounce 0.6s ease; } /* v0.6.2-33: renamed from spidey-click-spin (does scale, not spin) */
.spidey-pet.pet-anim-shrink { animation: spidey-click-shrink 0.5s ease; }
.spidey-pet.pet-anim-jump   { animation: spidey-click-jump 0.45s cubic-bezier(0.3, 1.5, 0.5, 1); }
.spidey-pet.pet-anim-squash { animation: spidey-click-squash 0.5s ease; }
.spidey-pet.pet-anim-jolt   { animation: spidey-click-jolt 0.32s ease; }

/* ========== 荡丝绳（spidey-action-swing 时显示，从顶部垂下） ========== */
.spidey-pet__rope {
  position: absolute;
  top: 0;
  left: 50%;
  transform: translateX(-50%);
  width: 4px;
  height: 46%;
  opacity: 0;
  pointer-events: none;
  z-index: 5;
}
.spidey-pet__rope svg { width: 100%; height: 100%; }
.spidey-pet__rope line {
  stroke: rgba(232, 240, 248, 0.7);
  stroke-width: 2;
  stroke-linecap: round;
}
/* swing 姿势图本身已含一条从顶部连到手部的绳索，并随 spidey-swing 一起摆动；
   这里不再叠加静态 SVG 绳索，避免“白色蛛丝一直没动”的穿帮（v0.6.2-36）。 */
.spidey-pet.spidey-action-swing .spidey-pet__rope { opacity: 0; }

/* ========== 蛛丝网（射蛛丝表情） ========== */
.spidey-pet__web {
  position: absolute;
  top: -4%;
  left: -4%;
  width: 108%;
  height: 108%;
  opacity: 0;
  pointer-events: none;
  z-index: 7;
}
.spidey-pet__web svg { width: 100%; height: 100%; overflow: visible; }
.spidey-pet__web .web-silk {
  fill: none;
  stroke: rgba(232, 240, 248, 0.95);
  stroke-width: 2.4;
  stroke-linecap: round;
  stroke-dasharray: 100;
  stroke-dashoffset: 100; /* 默认藏起，由 web-draw 绘制"射出" */
  filter: drop-shadow(0 0 2px rgba(120, 160, 200, 0.6));
}
.spidey-pet__web .web-clump polygon,
.spidey-pet__web .web-clump line {
  fill: none;
  stroke: rgba(232, 240, 248, 0.95);
  stroke-width: 1.6;
  stroke-linejoin: round;
}
.spidey-pet__web .web-clump {
  transform: scale(0);
  transform-origin: 0 0;
  opacity: 0;
}

/* 射蛛丝（点击动效 / 表情编辑器预览）：切换到 poseWeb 姿势图 + 蛛丝网，
   而不是在 idle 帧上整体扭动 + 叠加 SVG 线（v0.6.2-35）。
   只对姿势图本身做投掷动画，避免 art 容器与 pose 的 transform 叠加导致位移翻倍。 */
.spidey-pet.pet-anim-web .spidey-pet__pose,
.spidey-pet.spidey-action-web .spidey-pet__pose {
  opacity: 1;
  animation: spidey-web-throw 0.65s ease-out;
}
.spidey-pet.pet-anim-web .spidey-pet__web,
.spidey-pet.spidey-action-web .spidey-pet__web {
  opacity: 1;
}
.spidey-pet.pet-anim-web .web-silk,
.spidey-pet.spidey-action-web .web-silk {
  animation: web-draw 0.65s ease-out;
}
.spidey-pet.pet-anim-web .web-clump,
.spidey-pet.spidey-action-web .web-clump {
  animation: web-clump 0.65s ease-out;
}

/* ========== 待机随机小动作（仅 state=idle 触发，见脚本调度器） ========== */
/* 动作触发时显示对应姿势精灵图，并对姿势图做轻量动画（不再整体扭 idle 图） */
.spidey-pet.spidey-action-swing .spidey-pet__pose {
  opacity: 1;
  transform-origin: 50% 0%; /* 以头顶为摆轴 */
  animation: spidey-swing 1.6s ease-in-out;
}
.spidey-pet.spidey-action-pose .spidey-pet__pose {
  opacity: 1;
  animation: spidey-pose 0.9s ease;
}
.spidey-pet.spidey-action-web .spidey-pet__pose {
  opacity: 1;
  animation: spidey-web-throw 0.65s ease-out;
}
.spidey-pet.spidey-action-crouch .spidey-pet__pose {
  opacity: 1;
  animation: spidey-crouch 0.9s ease;
}

/* ========== 所有 keyframes ========== */
@keyframes spidey-idle {
  0%   { transform: translateY(0) scale(1); }
  25%  { transform: translateY(-4px) scale(1.01); }
  50%  { transform: translateY(-7px) scale(1.02); }
  75%  { transform: translateY(-3px) scale(1.01); }
  100% { transform: translateY(0) scale(1); }
}
@keyframes spidey-breathe {
  0%, 100% { transform: scale(1); }
  50%      { transform: scale(1.025); }
}
@keyframes spidey-bob {
  0%, 100% { transform: translateY(0); }
  50%      { transform: translateY(-4px); }
}
@keyframes spidey-focus {
  0%, 100% { transform: translateY(0) scale(1); }
  50%      { transform: translateY(-1.5px) scale(1.008); }
}
@keyframes spidey-pop {
  0%   { transform: scale(1); }
  35%  { transform: scale(1.16); }
  65%  { transform: scale(0.96); }
  100% { transform: scale(1); }
}
@keyframes spidey-shake {
  0%, 100% { transform: translateX(0); }
  25%      { transform: translateX(-3px); }
  75%      { transform: translateX(3px); }
}
@keyframes spidey-shake-fast {
  0%, 100% { transform: translateX(0); }
  25%      { transform: translateX(-1.5px); }
  75%      { transform: translateX(1.5px); }
}
@keyframes spidey-startle {
  0%   { transform: scale(1); }
  30%  { transform: scale(1.12); }
  60%  { transform: scale(1.05); }
  100% { transform: scale(1); }
}
@keyframes spidey-slouch {
  0%, 100% { transform: rotate(-6deg) translateY(0); }
  50%      { transform: rotate(6deg) translateY(2px); }
}
@keyframes spidey-sway {
  0%, 100% { transform: rotate(-2deg); }
  50%      { transform: rotate(2deg); }
}
@keyframes spidey-nod {
  0%, 100% { transform: translateY(0) rotate(0); }
  50%      { transform: translateY(2px) rotate(-3deg); }
}
@keyframes spidey-walk {
  0%   { transform: rotate(-3deg) translateY(0); }
  25%  { transform: rotate(0deg) translateY(-2px); }
  50%  { transform: rotate(3deg) translateY(0); }
  75%  { transform: rotate(0deg) translateY(-2px); }
  100% { transform: rotate(-3deg) translateY(0); }
}
@keyframes spidey-overheat-shake {
  0%, 100% { transform: translate(0, 0); }
  25%      { transform: translate(-0.8px, 0.4px) rotate(-0.4deg); }
  50%      { transform: translate(0.7px, -0.5px) rotate(0.3deg); }
  75%      { transform: translate(-0.4px, -0.4px) rotate(-0.2deg); }
}
@keyframes spidey-click-bounce {
  0%, 100% { transform: scale(1); }
  40% { transform: scale(1.15) translateY(-4px); }
}
@keyframes spidey-click-shake {
  0%, 100% { transform: translateX(0); }
  20%, 60% { transform: translateX(-4px); }
  40%, 80% { transform: translateX(4px); }
}
@keyframes spidey-click-bounce { /* v0.6.2-33 renamed: 实际做 scale 弹跳而非 spin 旋转 */
  0% { transform: scale(1); }
  30% { transform: scale(1.18); }
  60% { transform: scale(0.93); }
  100% { transform: scale(1); }
}
@keyframes spidey-click-shrink {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(0.85); }
}
@keyframes spidey-click-jump {
  0%   { transform: translateY(0) scale(1, 1); }
  35%  { transform: translateY(-22px) scale(0.94, 1.08); }
  60%  { transform: translateY(-12px) scale(1.06, 0.94); }
  100% { transform: translateY(0) scale(1, 1); }
}
@keyframes spidey-click-squash {
  0%   { transform: scale(1, 1); }
  30%  { transform: scale(1.35, 0.7); }
  55%  { transform: scale(0.85, 1.18); }
  75%  { transform: scale(1.08, 0.92); }
  100% { transform: scale(1, 1); }
}
@keyframes spidey-click-jolt {
  0%, 100% { transform: translate(0, 0); }
  15%      { transform: translate(-3px, -2px); }
  30%      { transform: translate(3px, 1px); }
  45%      { transform: translate(-2px, 1px); }
  60%      { transform: translate(3px, -1px); }
  80%      { transform: translate(-1px, 1px); }
}
@keyframes spidey-web-throw {
  0%   { transform: translateY(0) rotate(0); }
  10%  { transform: translateY(10px) rotate(-6deg); }   /* 快速预备：后仰蓄力 */
  30%  { transform: translateY(-20px) rotate(8deg); }  /* 猛甩出手 */
  55%  { transform: translateY(-6px) rotate(-2deg); }
  100% { transform: translateY(0) rotate(0); }
}
@keyframes web-draw {
  0%   { stroke-dashoffset: 100; opacity: 0; }
  8%   { opacity: 1; }
  42%  { stroke-dashoffset: 0; opacity: 1; }   /* 丝线瞬间射到角落 */
  72%  { stroke-dashoffset: 0; opacity: 1; }
  100% { stroke-dashoffset: 0; opacity: 0; }   /* 收丝淡出 */
}
@keyframes web-clump {
  0%, 35%  { transform: scale(0); opacity: 0; }
  50%      { transform: scale(1.35); opacity: 1; }
  72%      { transform: scale(1.1); opacity: 1; }
  100%     { transform: scale(1.1); opacity: 0; }
}
@keyframes spidey-swing {
  0%   { transform: rotate(0) translateY(0); }
  20%  { transform: rotate(-16deg) translateY(2px); }
  50%  { transform: rotate(16deg) translateY(-4px); }
  80%  { transform: rotate(-8deg) translateY(1px); }
  100% { transform: rotate(0) translateY(0); }
}
@keyframes spidey-pose {
  0%   { transform: translateY(0) scale(1, 1); }
  30%  { transform: translateY(-12px) scale(1.04, 0.97); } /* 起跳摆 pose */
  55%  { transform: translateY(-5px) scale(0.98, 1.04); }
  100% { transform: translateY(0) scale(1, 1); }
}
@keyframes spidey-crouch {
  0%   { transform: translateY(0) scale(1, 1); }
  40%  { transform: translateY(12px) scale(1.16, 0.78); } /* 蹲防 */
  70%  { transform: translateY(7px) scale(1.05, 0.95); }
  100% { transform: translateY(0) scale(1, 1); }
}
</style>
