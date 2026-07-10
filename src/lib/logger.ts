/**
 * 前端日志封装（v0.4.2 引入）
 *
 * 通过 `@tauri-apps/plugin-log` 把前端日志写入与 Rust 后端**同一文件**。
 *
 * ## 用法
 * ```ts
 * import { log } from "@/lib/logger";
 * log.info("用户点击导出");
 * log.error("API 调用失败", { endpoint: "/api/v1/foo" });
 * ```
 *
 * ## 设计
 * - 不依赖 Vue 插件，全局可用
 * - 自动捕获未处理的 Promise rejection / window error → ERROR
 * - 失败时静默降级到 console.*（日志系统初始化失败不能影响 UI）
 */

import {
  trace as tauriTrace,
  debug as tauriDebug,
  info as tauriInfo,
  warn as tauriWarn,
  error as tauriError,
} from "@tauri-apps/plugin-log";

/** 是否在 Tauri 环境下运行（浏览器 mock 模式下日志降级到 console） */
const isTauri = typeof (window as any).__TAURI_INTERNALS__ !== "undefined";

/** 全局错误捕获器是否已挂载（避免重复） */
let globalHandlersInstalled = false;

/**
 * 日志接口（与 @tauri-apps/plugin-log 一致，但失败时降级到 console）
 */
export const log = {
  trace(msg: string, ...args: any[]) {
    safeLog("trace", msg, args);
  },
  debug(msg: string, ...args: any[]) {
    safeLog("debug", msg, args);
  },
  info(msg: string, ...args: any[]) {
    safeLog("info", msg, args);
  },
  warn(msg: string, ...args: any[]) {
    safeLog("warn", msg, args);
  },
  error(msg: string | Error, ...args: any[]) {
    const msgStr = msg instanceof Error ? `${msg.name}: ${msg.message}` : msg;
    safeLog("error", msgStr, args);
  },
};

/** 内部：单条日志写入（降级到 console） */
function safeLog(
  level: "trace" | "debug" | "info" | "warn" | "error",
  msg: string,
  args: any[]
) {
  if (!isTauri) {
    // 浏览器 mock 模式：仅 console
    const consoleFn = (console as any)[level] ?? console.log;
    consoleFn(`[${level}]`, msg, ...args);
    return;
  }
  // Tauri 环境：写入 plugin-log（失败降级到 console）
  const payload = formatArgs(args);
  const fn = {
    trace: tauriTrace,
    debug: tauriDebug,
    info: tauriInfo,
    warn: tauriWarn,
    error: tauriError,
  }[level];
  fn(payload ? `${msg} ${payload}` : msg).catch(() => {
    // plugin-log 初始化失败 → 静默降级
    const consoleFn = (console as any)[level] ?? console.log;
    consoleFn(`[${level}]`, msg, payload);
  });
}

function formatArgs(args: any[]): string {
  if (args.length === 0) return "";
  try {
    return args
      .map((a) => {
        if (a instanceof Error) return a.stack ?? `${a.name}: ${a.message}`;
        if (typeof a === "string") return a;
        return JSON.stringify(a);
      })
      .join(" ");
  } catch {
    return "[unserializable args]";
  }
}

/**
 * 安装全局错误捕获器
 *
 * - `window.onerror`：未捕获同步异常 → ERROR
 * - `unhandledrejection`：未捕获 Promise 拒绝 → ERROR
 *
 * 在 main.ts 里调用一次即可，重复调用安全（内部 flag 保护）
 */
export function installGlobalErrorHandlers() {
  if (globalHandlersInstalled) return;
  globalHandlersInstalled = true;

  window.addEventListener("error", (event) => {
    log.error(
      `Unhandled error: ${event.message}`,
      `${event.filename}:${event.lineno}:${event.colno}`
    );
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    if (reason instanceof Error) {
      log.error(`Unhandled rejection: ${reason.message}`, reason.stack);
    } else {
      log.error(`Unhandled rejection: ${String(reason)}`);
    }
  });
}