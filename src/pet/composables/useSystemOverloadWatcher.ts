/**
 * pet/composables/useSystemOverloadWatcher.ts
 * 监听 Rust 端发来的 pet-system-overload / pet-system-cool 事件（v0.6.2-beta.15 引入）。
 *
 * 行为：
 * - 收到 overload → store.isHeating = true，effectiveState 暂时覆写为 'angry'，
 *   皮肤器自动加 .is-heating class
 * - 收到 cool → 还原（保留 override 由用户控制）
 *
 * 注：Tauri 多窗口 JS 上下文隔离：本 composable 必须挂在 pet 窗口内；其他窗口
 * （主窗口的 Settings）不订阅。
 */
import { onBeforeUnmount, onMounted } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { petStore } from '../stores/petStore';

export function useSystemOverloadWatcher() {
  let unlistenOverload: (() => void) | null = null;
  let unlistenCool: (() => void) | null = null;
  onMounted(async () => {
    try {
      unlistenOverload = await listen<number>('pet-system-overload', (e) => {
        petStore.setHeating(true, e.payload);
      });
      unlistenCool = await listen<number>('pet-system-cool', () => {
        petStore.setHeating(false, null);
      });
    } catch (err) {
      console.error('[pet-overload] 监听失败', err);
    }
  });
  onBeforeUnmount(() => {
    unlistenOverload?.();
    unlistenCool?.();
    petStore.setHeating(false, null);
  });
}
