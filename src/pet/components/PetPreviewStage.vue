<!--
  PetPreviewStage.vue
  编辑器专用预览舞台（v0.6.1-beta.1 新增）。

  设计：
  - 与真实桌宠 PetCanvas 共用同一套坐标数据源（spriteLayout.ts），
    因此编辑器里看到的部件位置 == 桌宠实际渲染位置，杜绝错位。
  - 通过 `resolver(id)` 取素材 URL，默认取内置 sprite；
    编辑器传入 usePetSprites 的 getSpriteUrl，即可实时反映「自定义替换素材」。
  - 不做任何交互，纯渲染；落点高亮 / 拖拽由外层编辑器容器处理。

  修改历史：
    - 2026-07-24 @v0.6.1-beta.1: 初始创建 - 编辑器预览统一坐标
-->
<template>
  <div class="pet-stage" :style="{ width: size + 'px', height: size + 'px' }">
    <PetLayer :src="resolver('body_base')" :pos-x="L.body.x" :pos-y="L.body.y" :width-pct="L.body.w" alt="body" />
    <PetLayer :src="resolver(composition.eye)" :pos-x="L.eye.x" :pos-y="L.eye.y" :width-pct="L.eye.w" alt="eye" />
    <PetLayer :src="resolver(composition.brow)" :pos-x="L.brow.x" :pos-y="L.brow.y" :width-pct="L.brow.w" alt="brow" />
    <PetLayer :src="resolver(composition.mouth)" :pos-x="L.mouth.x" :pos-y="L.mouth.y" :width-pct="L.mouth.w" alt="mouth" />
    <template v-for="d in composition.decorations" :key="d">
      <PetLayer
        v-if="d !== 'none' && decoSpec(d as PetDecoration)"
        :src="resolver(d)"
        :pos-x="decoSpec(d as PetDecoration)!.x"
        :pos-y="decoSpec(d as PetDecoration)!.y"
        :width-pct="decoSpec(d as PetDecoration)!.w"
        :scale="decoSpec(d as PetDecoration)!.scale ?? 1"
        :alt="d"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import PetLayer from './PetLayer.vue';
import { LAYER_SPECS, decoSpec } from '../engine/spriteLayout';
import type { PetDecoration } from '../types';

defineProps<{
  /** 当前要预览的部件组合（eye/mouth/brow/decorations 的 sprite id） */
  composition: { eye: string; mouth: string; brow: string; decorations: string[] };
  /** 素材 id → URL 解析器（编辑器传 getSpriteUrl 以含自定义替换） */
  resolver: (id: string) => string;
  /** 舞台边长（px），默认 160 */
  size?: number;
}>();

const L = LAYER_SPECS;
</script>

<style scoped>
.pet-stage {
  position: relative;
  pointer-events: none; /* 让外层包装容器接收拖拽落点 */
  user-select: none;
  -webkit-user-select: none;
}
.pet-stage :deep(img) {
  image-rendering: -webkit-optimize-contrast;
  image-rendering: crisp-edges;
}
</style>
