<!--
  pet/components/PetSkinRenderer.vue
  皮肤渲染器（v0.6.2 新增）。

  这是新桌宠渲染协议的"路由器"：
  - 接收 PetWindow 传入的 state / isDragging / animClass
  - 查询注册表当前活跃皮肤
  - 用 <component :is="..."> 动态挂载皮肤
  - 当用户在右键菜单切换皮肤时（registry.setActive），重建组件

  关键不变量：
  - 皮肤切换时 PetWindow 完全不需要知道是哪一种皮肤
  - 旧皮肤的所有交互（点击动效/拖拽状态/喂食/前台联动）全部继承，因为只换了根组件
  - 每个皮肤只接收 { state } prop；isDragging / animClass 通过 inheritAttrs 自动落到皮肤根元素

  修改历史：
    - 2026-07-24 @v0.6.2: 初始创建 - 皮肤路由器
-->
<template>
  <component
    :is="activeRenderer"
    v-if="activeRenderer"
    :key="skinTick"
    :state="state"
  />
</template>

<script setup lang="ts">
import { ref, watchEffect, type Component } from 'vue';
import { skinRegistry } from '../skins/registry';
import type { PetState } from '../types';

// 透传给皮肤渲染器：state 显式声明（皮肤必须有），其余通过 $attrs 自动 inheritAttrs
defineProps<{ state: PetState }>();

// 强制重建键 + 当前渲染器
const skinTick = ref(0);
const activeRenderer = ref<Component | null>(null);

watchEffect(() => {
  const m = skinRegistry.active();
  // 仅当渲染器引用变化时重建皮肤组件（切换皮肤才需销毁重建），
  // 避免 watchEffect 因其它状态变化误触发组件重建（闪烁 + 额外开销）
  if (m && m.renderer !== activeRenderer.value) {
    activeRenderer.value = m.renderer;
    skinTick.value++;
  }
});
</script>
