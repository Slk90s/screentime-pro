<!--
  PetMenuWindow.vue
  桌宠右键菜单独立窗口的根组件（v0.6.2-beta.17）。

  关键变更：
  - 之前菜单 Teleport 到 pet webview body，position:fixed 受限于 pet 视口
    （150×330），菜单无法拖到全桌面。本组件跑在独立的 "pet-menu" Tauri 窗口里，
    窗口本身就是全桌面范围，菜单可自由拖到屏幕任意位置。
  - 通过 Tauri 全局事件：pet 窗口右键时 → invoke('show_pet_menu_window') →
    这里由 PetWindow 主控流程控制显隐；本组件只在 onMounted 时挂载监听。
  - 自己窗口的拖动用 move_pet_menu_window（不再仅靠 transform）。

  修改历史：
    - 2026-07-25 @v0.6.2-beta.17: 初始创建 - 独立窗口解决菜单无法跨桌面拖拽
-->
<template>
  <!--
    整窗背景透明；PetContextMenu（mode='windowed'）渲染在 0,0 位置；
    菜单外的整窗区域响应 pointerdown 关闭菜单（外部点击）
  -->
  <div class="pet-menu-window-root" @pointerdown="onOutsidePointerDown">
    <PetContextMenu
      v-if="visible"
      :x="0"
      :y="0"
      :mode="'windowed'"
      @close="onMenuClose"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { syncLocaleFromStorage } from '../i18n';
import PetContextMenu from './components/PetContextMenu.vue';
// v0.6.2-35: 显式触发皮肤副作用注册，确保 pet-menu 独立 webview 中 skinRegistry.list() 不为空，
// 菜单皮肤按钮与高亮状态与主窗口/设置页保持一致。
import './skins';

const visible = ref(false);
let unlistenShown: (() => void) | null = null;

onMounted(async () => {
  // 透明窗体：局部覆盖本窗口 document 的 html/body/#app 背景（仅影响 pet-menu 这个 webview，
  // 不会污染主窗口）。注意：绝不能用全局（非 scoped）CSS 设 html/body/#app —— 本组件被 App.vue
  // 顶层 import，其全局样式会注入主窗口文档，把主窗口 html/body/#app 设成 overflow:hidden 而
  // 无法滚动（beta.17 回归根因）。
  const els = [document.documentElement, document.body, document.getElementById('app')].filter(
    (el): el is HTMLElement => el != null,
  );
  for (const el of els) {
    el.style.background = 'transparent';
    el.style.margin = '0';
    el.style.padding = '0';
    el.style.overflow = 'hidden';
  }
  try {
    unlistenShown = await listen('pet-menu-shown', () => {
      // 菜单打开时重读语言：避免独立窗口错过 locale-changed 事件而停留在旧语言
      syncLocaleFromStorage();
      visible.value = true;
    });
    // 竞态兜底：若 show 事件在监听注册前已发出（窗口首次动态创建、加载慢于右键），
    // 检查本窗口当前是否可见，可见则直接显示菜单（pet-menu 由 conf 预创建时通常不会触发）。
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    if (await getCurrentWindow().isVisible()) {
      syncLocaleFromStorage();
      visible.value = true;
    }
  } catch {
    /* 非 Tauri 环境 */
  }
});

onBeforeUnmount(() => {
  if (unlistenShown) unlistenShown();
});

/** 菜单外部点击（pet-menu 窗口内、非菜单区域）→ 关闭菜单 */
function onOutsidePointerDown(e: PointerEvent): void {
  const target = e.target as HTMLElement;
  if (target.closest('.pet-menu')) return;
  hide();
}

function onMenuClose(): void {
  hide();
}

async function hide(): Promise<void> {
  visible.value = false;
  // 关自己窗口（隐藏，不销毁；下次 show 零延迟）
  try {
    await invoke('hide_pet_menu_window');
  } catch {
    /* ignore */
  }
}
</script>

<style scoped>
/* 纯透明画布：仅作用于本组件根节点。
   注意：不能用全局 html/body/#app 规则（会被主窗口加载时注入，导致主窗口无法滚动）。
   透明背景在 onMounted 用 JS 局部覆盖本 webview 的 document。 */
.pet-menu-window-root {
  width: 100vw;
  height: 100vh;
  background: transparent;
  overflow: hidden;
}
</style>
