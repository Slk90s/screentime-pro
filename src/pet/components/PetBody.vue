<!--
  PetBody.vue
  桌宠基础身体组件（v0.6.0-beta Phase 1 占位实现）。

  设计思路：
  - Phase 1 用 inline SVG 描一个简版熊猫轮廓（黑白圆脸 + 圆耳朵 + 闭眼微笑），等 Phase 2 ImageGen 批量生成真实素材后整体替换。
  - 待机动画用 CSS keyframe：呼吸（scale 1 → 1.04 → 1，3s 循环）+ 眨眼（eye scaleY 1 → 0.1 → 1，4~6s 随机间隔）。
  - 所有 CSS 用 `:global` 包裹避免 scope hash 与后续 Phase 2 引入的部件层冲突。

  修改历史：
    - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 占位 SVG + 呼吸/眨眼动画
-->
<template>
  <div class="pet-body" :class="{ dragging: isDragging }">
    <svg viewBox="0 0 140 140" xmlns="http://www.w3.org/2000/svg">
      <!-- 阴影（贴底柔和椭圆，避免看起来"飘"） -->
      <ellipse cx="70" cy="128" rx="34" ry="5" fill="rgba(0,0,0,0.18)" />
      <!-- 身体（白椭圆） -->
      <ellipse class="body" cx="70" cy="92" rx="42" ry="38" fill="#ffffff" stroke="#1a1a1a" stroke-width="2.5" />
      <!-- 左耳 -->
      <ellipse class="ear ear-l" cx="36" cy="46" rx="14" ry="16" fill="#1a1a1a" />
      <!-- 右耳 -->
      <ellipse class="ear ear-r" cx="104" cy="46" rx="14" ry="16" fill="#1a1a1a" />
      <!-- 脸（白圆） -->
      <circle class="face" cx="70" cy="68" r="36" fill="#ffffff" stroke="#1a1a1a" stroke-width="2.5" />
      <!-- 眼睛黑斑（熊猫标志性） -->
      <ellipse class="eye-patch eye-patch-l" cx="56" cy="64" rx="9" ry="11" fill="#1a1a1a" />
      <ellipse class="eye-patch eye-patch-r" cx="84" cy="64" rx="9" ry="11" fill="#1a1a1a" />
      <!-- 眼睛（白色高光点，模拟睁眼） -->
      <circle class="eye eye-l" cx="56" cy="66" r="3.5" fill="#ffffff" />
      <circle class="eye eye-r" cx="84" cy="66" r="3.5" fill="#ffffff" />
      <!-- 鼻子 -->
      <ellipse class="nose" cx="70" cy="76" rx="3.5" ry="2.5" fill="#1a1a1a" />
      <!-- 嘴巴（微笑弧线） -->
      <path class="mouth" d="M 62 82 Q 70 88 78 82" stroke="#1a1a1a" stroke-width="2" fill="none" stroke-linecap="round" />
      <!-- 腮红（淡粉） -->
      <circle class="cheek cheek-l" cx="46" cy="78" r="4" fill="#ffb3b3" opacity="0.55" />
      <circle class="cheek cheek-r" cx="94" cy="78" r="4" fill="#ffb3b3" opacity="0.55" />
    </svg>
  </div>
</template>

<script setup lang="ts">
defineProps<{ isDragging?: boolean }>();
</script>

<style scoped>
.pet-body {
  width: 140px;
  height: 140px;
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none; /* 容器不接收事件，事件由 PetWindow 顶层捕获 */
  user-select: none;
  -webkit-user-select: none;
}
.pet-body svg {
  width: 100%;
  height: 100%;
  /* 呼吸动画：身体 3s 缩放循环 */
  animation: breathe 3.2s ease-in-out infinite;
  transform-origin: 70px 100px; /* 锚定身体底部，避免头部晃动 */
}
.pet-body.dragging svg {
  animation-play-state: paused; /* 拖拽时停掉呼吸，避免视觉抖动 */
  cursor: grabbing;
}
/* 眨眼：眼睛白色高光 0.15s 缩 Y，每 4~6s 触发一次 */
.eye-l,
.eye-r {
  animation: blink 5s ease-in-out infinite;
  transform-origin: center;
  transform-box: fill-box;
}
.eye-r {
  animation-delay: 0.04s;
}
@keyframes breathe {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(1.04); }
}
@keyframes blink {
  0%, 92%, 100% { transform: scaleY(1); }
  95% { transform: scaleY(0.1); }
}
</style>