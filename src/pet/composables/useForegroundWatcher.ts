/**
 * pet/composables/useForegroundWatcher.ts
 * 前台应用轮询 + 自动状态推断（v0.6.0-beta 引入）。
 *
 * 设计思路：
 * - 每 2s 调用一次 get_current_foreground（Rust 端已有命令），
 *   把前台应用 → 状态映射推断结果写回 store.state
 * - 仅当 process 变化时重新推断（节流）
 * - 用户手动 override 时不覆盖（store.effectiveState 已实现优先级）
 * - 桌宠开关关闭时停止轮询（onBeforeUnmount）
 * - Phase 4 GA 时可升级为 Rust 端 NSWorkspace/SetWinEventHook 事件订阅（≈0 延迟）
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 2s 轮询 + 自动推断
 */
import { onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { petStore } from '../stores/petStore';
import { inferStateFromApp, shouldReinfer } from '../engine/appToState';
import type { ForegroundAppInfo } from '../types';

const POLL_INTERVAL_MS = 2000;

interface ForegroundOut {
  name: string;
  process_name: string;
  bundle_id?: string | null;
  exe_path?: string | null;
  window_title?: string | null;
}

export function useForegroundWatcher(): void {
  let timer: number | null = null;
  let prev: ForegroundAppInfo | null = null;

  async function tick(): Promise<void> {
    // 桌宠关闭 / 用户手动 override 时跳过
    if (!petStore.enabled) return;
    if (petStore.override !== null) return;

    try {
      const fg = (await invoke('get_current_foreground')) as ForegroundOut;
      const info: ForegroundAppInfo = {
        name: fg.name,
        process: fg.process_name,
        bundleId: fg.bundle_id ?? undefined,
        windowTitle: fg.window_title ?? undefined,
      };
      if (!shouldReinfer(prev, info)) return;
      prev = info;
      const newState = inferStateFromApp(info);
      if (newState !== petStore.state) {
        petStore.setState(newState);
      }
    } catch (e) {
      // 静默失败（采样循环主路径有重试，桌宠轮询是辅助）
      console.warn('[pet] 前台应用轮询失败', e);
    }
  }

  onMounted(() => {
    // 立即跑一次（桌宠启动时不延迟 2s）
    tick();
    timer = window.setInterval(tick, POLL_INTERVAL_MS);
  });

  onBeforeUnmount(() => {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  });
}