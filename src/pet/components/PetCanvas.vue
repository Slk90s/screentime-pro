<!--
  PetCanvas.vue
  桌宠合成画布：state → 部件组合 → 渲染 body + eye + mouth + brow + decorations

  设计思路：
  - 接收 state prop（PetState），调 usePetSprites.getMergedComposition() 拿到 4 元组
    （customCompositions 优先 → 回退 stateMachine.getComposition，与编辑器预览一致）
  - 每层 src 经 getCustomSprite(id) 解析：用户上传的自定义素材优先，否则用内置 PNG
  - body 始终在最底层，呼吸动画用 SVG 关键帧（不依赖外部资源）
  - 部件层用 PNG，定位按身体几何中心校准（眼睛 ~y=48%，嘴巴 ~y=58%，眉毛 ~y=42%，装饰看具体）
  - 装饰层根据 composition.decorations[] 数组顺序叠加
  - 切换 state 时部件层 src 切换，CSS transition 200ms 让切换平滑

  修改历史：
    - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 部件合成 + 状态机驱动
    - 2026-07-24 @v0.6.1-beta.1: 重构 - 部件层定位改用共享 spriteLayout（LAYER_SPECS/DECO_SPECS），装饰先过滤非 none 再渲染
    - 2026-07-24 @v0.6.2: 修复 - 实时桌宠消费 usePetSprites 合并路径（含 customCompositions + 自定义素材），编辑器保存组合/素材即时生效
-->
<template>
  <div class="pet-canvas" :class="{ dragging: isDragging }">
    <!-- 1. 阴影层已完全移除（用户反馈阴影残留）——body_base.png 本身不含阴影，保持纯净透明 -->

    <!-- 2. 部件层叠加（定位走共享 LAYER_SPECS） -->
    <PetLayer :src="bodySrc" :pos-x="L.body.x" :pos-y="L.body.y" :width-pct="L.body.w" :alt="`body`" />

    <PetLayer :src="eyeSrc" :pos-x="L.eye.x" :pos-y="L.eye.y" :width-pct="L.eye.w" :alt="`eyes`" />

    <PetLayer :src="browSrc" :pos-x="L.brow.x" :pos-y="L.brow.y" :width-pct="L.brow.w" :alt="`brows`" />

    <PetLayer :src="mouthSrc" :pos-x="L.mouth.x" :pos-y="L.mouth.y" :width-pct="L.mouth.w" :alt="`mouth`" />

    <!-- 装饰层（按数组顺序，定位走共享 spriteLayout；src 走 getCustomSprite 优先） -->
    <template v-for="d in activeDecos" :key="d">
      <PetLayer
        :src="decoSrc(d)"
        :pos-x="decoSpec(d)!.x"
        :pos-y="decoSpec(d)!.y"
        :width-pct="decoSpec(d)!.w"
        :scale="decoSpec(d)!.scale ?? 1"
        :alt="d"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
// 直接 import 所有 sprite（Vite 会把图打进 chunk，运行时无路径解析问题）
import bodyBase from '../assets/sprites/body_base.png';
import eyeOpenNormal from '../assets/sprites/eye_open_normal.png';
import eyeHappySmile from '../assets/sprites/eye_happy_smile.png';
import eyeClosedSleep from '../assets/sprites/eye_closed_sleep.png';
import eyeDizzy from '../assets/sprites/eye_dizzy.png';
import mouthSmile from '../assets/sprites/mouth_smile.png';
import mouthNeutral from '../assets/sprites/mouth_neutral.png';
import mouthFrown from '../assets/sprites/mouth_frown.png';
import mouthSurprised from '../assets/sprites/mouth_surprised.png';
import mouthEating from '../assets/sprites/mouth_eating.png';
import mouthPout from '../assets/sprites/mouth_pout.png';
import browNormal from '../assets/sprites/brow_normal.png';
import browAngry from '../assets/sprites/brow_angry.png';
import browSad from '../assets/sprites/brow_sad.png';
import glasses from '../assets/sprites/glasses.png';
import headphone from '../assets/sprites/headphone.png';
import controller from '../assets/sprites/controller.png';
import pencil from '../assets/sprites/pencil.png';
import speechBubble from '../assets/sprites/speech_bubble.png';
import heart from '../assets/sprites/heart.png';
import zzz from '../assets/sprites/zzz.png';
import sweat from '../assets/sprites/sweat.png';
import PetLayer from './PetLayer.vue';
import { LAYER_SPECS, decoSpec } from '../engine/spriteLayout';
import { usePetSprites } from '../composables/usePetSprites';
import type { EyeSprite, MouthSprite, BrowSprite } from '../engine/stateMachine';
import type { PetState, PetDecoration } from '../types';

const props = defineProps<{ state: PetState; isDragging?: boolean }>();

// v0.6.2 修复：实时桌宠消费 usePetSprites 的合并路径（custom 优先 → 默认），
// 与编辑器预览 PetPreviewStage 一致，解决「编辑器保存组合/素材不生效于实时桌宠」的遗留 bug。
const { getCustomSprite, getMergedComposition } = usePetSprites();

// ---- sprite 映射表 ----
const EYE_MAP: Record<EyeSprite, string> = {
  eye_open_normal: eyeOpenNormal,
  eye_happy_smile: eyeHappySmile,
  eye_closed_sleep: eyeClosedSleep,
  eye_dizzy: eyeDizzy,
};
const MOUTH_MAP: Record<MouthSprite, string> = {
  mouth_smile: mouthSmile,
  mouth_neutral: mouthNeutral,
  mouth_frown: mouthFrown,
  mouth_surprised: mouthSurprised,
  mouth_eating: mouthEating,
  mouth_pout: mouthPout,
};
const BROW_MAP: Record<BrowSprite, string> = {
  brow_normal: browNormal,
  brow_angry: browAngry,
  brow_sad: browSad,
};
const DECO_MAP: Record<Exclude<PetDecoration, 'none'>, string> = {
  glasses,
  headphone,
  controller,
  pencil,
  speech_bubble: speechBubble,
  heart,
  zzz,
  sweat,
  coin: '', // 当前未使用（stateMachine 暂无 coin 状态），保留以满足类型
};

// ---- 计算当前 composition + 各层 src（custom 优先，回退默认） ----
const composition = computed(() => getMergedComposition(props.state));
// 每层优先取用户自定义替换素材（getCustomSprite），否则用内置 PNG
const bodySrc = computed(() => getCustomSprite('body_base') || bodyBase);
const eyeSrc = computed(() => getCustomSprite(composition.value.eye) || EYE_MAP[composition.value.eye]);
const mouthSrc = computed(() => getCustomSprite(composition.value.mouth) || MOUTH_MAP[composition.value.mouth]);
const browSrc = computed(() => getCustomSprite(composition.value.brow) || BROW_MAP[composition.value.brow]);
function decoSrc(d: Exclude<PetDecoration, 'none'>): string {
  return getCustomSprite(d) || DECO_MAP[d];
}

// 装饰层：过滤 'none' 且存在坐标，供模板安全渲染
const activeDecos = computed(() =>
  composition.value.decorations.filter(
    (d): d is Exclude<PetDecoration, 'none'> => d !== 'none' && !!decoSpec(d),
  ),
);

// 基础部件层定位走共享 LAYER_SPECS（与编辑器预览同一数据源）
const L = LAYER_SPECS;
</script>

<style scoped>
.pet-canvas {
  position: relative;
  width: 140px;
  height: 140px;
  pointer-events: none;
  user-select: none;
  -webkit-user-select: none;
}
/* PNG sprite 渲染优化：Retina 屏保持锐利边缘 */
.pet-canvas :deep(img) {
  image-rendering: -webkit-optimize-contrast;
  image-rendering: crisp-edges;
}
</style>