/**
 * 应用内自动升级（v1.2.11 新增）。
 *
 * 与 `tracker.autoCheckUpdate` / `checkUpdate` 的区别（**两套并存，别混用**）：
 *   - `autoCheckUpdate`：只**告知**有新版本，跳浏览器手动下载。macOS 走这条。
 *   - 本文件：真正**下载并安装**（Tauri 官方 updater，ed25519 验签）。
 *     仅 Windows(nsis) / Linux(AppImage) 可用，其余平台 `supported()` 返回 false。
 *
 * ⚠️ 两个平台差异会直接影响 UI 写法，调用前必须知道：
 *   ① Windows 装完后进程**立即退出**（updater 内部 `process::exit(0)`），
 *      `install()` 的 Promise **永远不会 resolve**。所以不要写「安装成功」提示。
 *   ② Linux 装完由 Rust 侧调用 `app.restart()`，Promise 会 resolve，但紧接着进程重启。
 */

import { Channel, invoke } from "@tauri-apps/api/core";

/** 下载进度（Rust 侧 `DownloadProgress` 的镜像）。 */
export interface DownloadProgress {
  /** 累计已下载字节（不是增量）。 */
  downloaded: number;
  /** 总字节数；服务端未给 Content-Length 时为 null。 */
  total: number | null;
}

/** `check_app_update` 的返回体。`null` 表示「已是最新」或「本平台不支持」。 */
export interface AppUpdate {
  current: string;
  version: string;
  notes: string;
  /** RFC3339 发布日；latest.json 未写时为空串。 */
  date: string;
}

/**
 * 本平台是否支持应用内升级。
 *
 * ⚠️ 判断依据在 Rust 侧（`bundle_type()` 是打包时 patch 进二进制的静态变量），
 * 前端拿不到，所以必须问后端。返回 false 时**不要渲染升级入口** —— 显示了再点出
 * 一句「不支持」是多余的一次失败交互。
 */
export function isSupported(): Promise<boolean> {
  return invoke<boolean>("app_update_supported");
}

/** 查询是否有可安装的新版本。`null` = 已是最新 / 本平台不支持。 */
export function check(): Promise<AppUpdate | null> {
  return invoke<AppUpdate | null>("check_app_update");
}

/**
 * 下载并安装。
 *
 * `onProgress` 收到的 `downloaded` 是**累计**字节数，可直接除以 `total` 算百分比。
 *
 * ⚠️ Windows 上这个 Promise **永远不会 resolve**（进程被 updater 的
 * `process::exit(0)` 干掉）。调用方不要 `await` 之后弹成功提示 —— 那段代码
 * 在 Windows 上执行不到。真正需要「装完了」的反馈，靠应用重启后的版本号对比。
 */
export function install(
  onProgress: (p: DownloadProgress) => void,
): Promise<void> {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;
  return invoke<void>("install_app_update", { onProgress: channel });
}

