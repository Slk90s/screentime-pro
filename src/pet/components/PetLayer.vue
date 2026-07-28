<!--
  PetLayer.vue
  通用部件层组件：渲染单个 PNG sprite + 灵活定位 + 缩放。

  设计思路：
  - Phase 2 用 inline import 把 22 张 sprite 全部打包进 chunk，避免运行时路径解析（file:// / asset URL）
  - 用 CSS transform 而非 img width/height，便于动画（呼吸/眨眼）
  - 部件层位置用百分比定位（相对 140×140 容器），方便后续调位置不重画图

  修改历史：
    - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 通用部件层组件
-->
<template>
  <img
    v-if="src"
    :src="src"
    class="pet-layer"
    :class="{ 'with-anim': animate }"
    :style="{
      left: posX + '%',
      top: posY + '%',
      width: widthPct + '%',
      transform: `translate(-50%, -50%) scale(${scale})`,
    }"
    draggable="false"
    :alt="alt"
  />
</template>

<script setup lang="ts">
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{
    src: string | null;
    /** 水平位置百分比（0~100，相对 140×140 容器） */
    posX?: number;
    /** 垂直位置百分比 */
    posY?: number;
    /** 宽度百分比（相对容器） */
    widthPct?: number;
    /** 缩放（默认 1） */
    scale?: number;
    /** 是否带待机动画（如眨眼用） */
    animate?: boolean;
    alt?: string;
  }>(),
  {
    posX: 50,
    posY: 50,
    widthPct: 50,
    scale: 1,
    animate: false,
    alt: '',
  },
);

const src = computed(() => props.src);
</script>

<style scoped>
.pet-layer {
  position: absolute;
  pointer-events: none;
  user-select: none;
  -webkit-user-select: none;
  transform-origin: center center;
  transition: transform 200ms ease;
  /* Retina 屏保持 sprite 锐利边缘（扁平2D风格不需要抗锯齿模糊） */
  image-rendering: -webkit-optimize-contrast;
  image-rendering: crisp-edges;
}
.pet-layer.with-anim {
  /* 部件层内的待机动画（如眨眼）由父组件通过 class 控制 */
}
</style>