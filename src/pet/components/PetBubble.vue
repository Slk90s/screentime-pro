<!--
  pet/components/PetBubble.vue
  桌宠随机中文气泡（v0.6.2-beta.15 引入）。

  设计：
  - v0.6.2-beta.25 调整：原来 `bottom:100%` 把气泡推到 .pet-window 顶部之上，
    被 overflow:hidden 裁掉一半，导致气泡「远离熊猫」。
    改为 `top:6px`，让气泡贴在窗口最顶部内侧——正好是熊猫头顶的空白处，
    「贴近熊猫」且完整可见。箭头朝下指向熊猫。
  - 当前 PetState 影响短语池（happy / angry / developing 等状态有专属短句）
  - 2.5s 后自动淡出（CSS transition）+ 父组件卸载
  - 字体: -apple-system，圆角白底深色字，带小三角指向桌宠
  - 不挡角色（容器在角色上方 6px，远离角色身体）
-->
<template>
  <Transition name="bubble-fade">
    <div v-if="visible" class="pet-bubble">
      <span class="pet-bubble__text">{{ message }}</span>
      <span class="pet-bubble__arrow" />
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';

const props = defineProps<{
  message: string;
  /** 自动隐藏毫秒（默认 2500） */
  duration?: number;
}>();

const visible = ref(true);

watch(
  () => props.message,
  () => {
    visible.value = false;
    // 下一帧重新进入，做顺序过渡（避免同 message 复用时不切内容）
    requestAnimationFrame(() => {
      visible.value = true;
    });
  },
);

if (props.duration && props.duration > 0) {
  setTimeout(() => {
    visible.value = false;
  }, props.duration);
}
</script>

<style scoped>
.pet-bubble {
  position: absolute;
  left: 50%;
  /* v0.6.2-beta.25：top 定位（贴近熊猫头顶空白），箭头朝下 */
  top: 6px;
  transform: translate(-50%, 0);
  padding: 6px 10px;
  background: rgba(255, 255, 255, 0.92);
  border: 1px solid rgba(255, 126, 39, 0.4);
  border-radius: 10px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.18);
  font-size: 12px;
  color: #1f1f23;
  font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", sans-serif;
  white-space: nowrap;
  max-width: 220px;
  pointer-events: none;
  z-index: 10;
  user-select: none;
  -webkit-user-select: none;
}
.pet-bubble__text {
  display: inline-block;
  line-height: 1.3;
}
.pet-bubble__arrow {
  /* v0.6.2-beta.25：箭头放在气泡底部，朝下指向熊猫 */
  position: absolute;
  left: 50%;
  bottom: -6px;
  transform: translateX(-50%) rotate(45deg);
  width: 10px;
  height: 10px;
  background: rgba(255, 255, 255, 0.92);
  border-right: 1px solid rgba(255, 126, 39, 0.4);
  border-bottom: 1px solid rgba(255, 126, 39, 0.4);
}

.bubble-fade-enter-active,
.bubble-fade-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.bubble-fade-enter-from {
  opacity: 0;
  transform: translate(-50%, -8px);
}
.bubble-fade-leave-to {
  opacity: 0;
  transform: translate(-50%, -14px);
}
</style>
