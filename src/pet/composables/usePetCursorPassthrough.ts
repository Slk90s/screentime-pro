/**
 * pet/composables/usePetCursorPassthrough.ts
 * 鼠标穿透控制 + pet 窗口环境初始化（背景/滚动条）。
 *
 * 设计思路（v0.6.0-beta.1 UI优化版）：
 * - 桌宠默认「接收事件」(passthrough:false)，使拖拽/右键/点击全部可用。
 *   透明空白处会挡住下层点击，但 140×140 小窗是可接受的取舍（QQ 企鹅同理）。
 * - **关键**：全局 style.css 的 body{background:#f5f5f7} 会污染 pet 窗口导致不透，
 *   此处在 mount 时强制将 html/body/#app 背景设为 transparent 并 hidden 滚动条。
 * - Rust 端 create_pet_window 也已默认 set_ignore_cursor_events(false)，双保险。
 * - 后续若要做「只有熊猫身体 alpha 通道接收事件」，Phase 4 改为按坐标动态切换即可。
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 占位接口（默认全穿透，导致不可交互）
 *   - 2026-07-17 @v0.6.0-beta.1: 修复 - 默认 passthrough:false，桌宠可交互
 *   - 2026-07-17 @v0.6.0-beta.1: UI优化 - mount 时覆盖全局 body 背景，消除不透明块+滚动条
 */
import { onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export function usePetCursorPassthrough(): void {
  onMounted(() => {
    // ① 鼠标穿透：false = 本窗接收鼠标事件（拖拽/右键/点击）
    invoke('set_pet_cursor_passthrough', { passthrough: false }).catch((err) => {
      console.error('[pet] 设置鼠标穿透失败', err);
    });

    // ② 覆盖全局 style.css 的 body{background:#f5f5f7}，使透明窗体真正透明
    const html = document.documentElement;
    const body = document.body;
    const app = document.getElementById('app');
    const els = [html, body, app].filter((el): el is HTMLElement => el != null);
    for (const el of els) {
      el.style.background = 'transparent';
      el.style.overflow = 'hidden';
      el.style.margin = '0';
      el.style.padding = '0';
    }
    // 隐藏滚动条（webkit 内核，macOS Safari/WebView 默认）
    document.documentElement.style.setProperty('overflow', 'hidden', 'important');

    // ③ 全局禁止原生右键菜单（确保自定义 PetContextMenu 唯一显示）
    const preventNativeMenu = (e: Event): void => e.preventDefault();
    document.addEventListener('contextmenu', preventNativeMenu);
    onBeforeUnmount(() => {
      document.removeEventListener('contextmenu', preventNativeMenu);
    });
  });
  onBeforeUnmount(() => {
    // 窗口隐藏/销毁前恢复穿透，避免残留窗口拦截下层点击
    invoke('set_pet_cursor_passthrough', { passthrough: true }).catch(() => {});
  });
}
