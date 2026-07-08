<template>
  <!-- 设备切换器：在「全部设备合并」与「单台设备」之间切换（多设备数据合并） -->
  <div class="switcher">
    <button
      :class="{ active: modelValue === '' }"
      @click="$emit('update:modelValue', '')"
      title="合并所有设备的数据"
    >
      全部设备
    </button>
    <button
      v-for="d in devices"
      :key="d.id"
      :class="{ active: modelValue === d.id }"
      @click="$emit('update:modelValue', d.id)"
      :title="d.id"
    >
      {{ d.name }}
    </button>
  </div>
</template>

<script setup lang="ts">
import type { DeviceInfo } from "../types";

// modelValue 为本机设备 id；空字符串 "" 表示合并全部设备
defineProps<{ modelValue: string; devices: DeviceInfo[] }>();
defineEmits<{ (e: "update:modelValue", v: string): void }>();
</script>

<style scoped>
.switcher {
  display: inline-flex;
  gap: 4px;
  background: #f0f0f3;
  padding: 3px;
  border-radius: 10px;
  max-width: 100%;
  overflow-x: auto;
}
.switcher button {
  border: none;
  background: transparent;
  padding: 6px 12px;
  border-radius: 8px;
  font-size: 13px;
  color: var(--muted);
  cursor: pointer;
  transition: all 0.15s;
  white-space: nowrap;
}
.switcher button.active {
  background: #fff;
  color: var(--text);
  font-weight: 500;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}
</style>
