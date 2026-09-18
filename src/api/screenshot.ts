/**
 * api/screenshot.ts
 * 截图相关的 IPC 封装（v0.8.0）。
 *
 * 设计思路：
 * - 与 tracker.ts 同约定：Tauri 内走 invoke，浏览器预览（无 __TAURI_INTERNALS__）返回安全默认值，
 *   使前端 UI 可脱离 Rust 单独预览。
 * - 只读命令（配置/列表/缩略图）在非 Tauri 环境返回空值；写命令（提交/删除）在非 Tauri 抛错，
 *   避免预览环境静默「假成功」。
 * - 参数命名遵循 Tauri v2 约定：Rust 侧 snake_case 形参，JS 侧传 camelCase（如 maxWidth / pngBase64）。
 * - **合成在前端**（v0.8.0 修订）：遮罩窗 canvas 把裁剪/标注/圆角/投影画好后整体交给
 *   `screenshot_commit`，Rust 不再做像素加工 → 预览与导出逐像素一致。
 */

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tracker";
import type {
  OcrEngineInfo,
  OcrOut,
  ScreenshotConfig,
  ScreenshotOut,
  ScreenshotResult,
  ShortcutApplyResult,
} from "../types";

/** v0.8.0 默认配置（与 Rust `ScreenshotConfig::default()` 保持一致，仅用于预览兜底） */
export const DEFAULT_SCREENSHOT_CONFIG: ScreenshotConfig = {
  enabled: true,
  shortcut: "CmdOrCtrl+Shift+A",
  auto_copy: true,
  auto_save: false,
  corner_radius: 8,
  shadow: false,
  max_count: 200,
  // v0.9.0：默认标准引擎（增强引擎要额外吃一次模型加载与 ~33MB 内存）
  ocr_engine: "system",
};

/** 提交参数：一张已经合成好的 PNG（含裁剪 / 标注 / 圆角 / 投影） */
export interface CommitArgs {
  /** PNG data URL 或裸 base64（Rust 侧两种都容忍） */
  pngBase64: string;
  /** 写入系统剪贴板 */
  copy: boolean;
  /** 落盘到截图历史 */
  save: boolean;
}

export const screenshot = {
  getConfig: (): Promise<ScreenshotConfig> =>
    isTauri
      ? invoke<ScreenshotConfig>("screenshot_get_config")
      : Promise.resolve({ ...DEFAULT_SCREENSHOT_CONFIG }),

  /** 保存配置；返回值为快捷键的**真实注册结果**（applied=false 时 error 说明原因） */
  setConfig: (config: ScreenshotConfig): Promise<ShortcutApplyResult> =>
    invoke<ShortcutApplyResult>("screenshot_set_config", { config }),

  /** 手动触发一次截图（等价于按全局快捷键） */
  trigger: (): Promise<boolean> => invoke<boolean>("screenshot_trigger"),

  /**
   * 取本次截图会话的整屏冻结帧（PNG data URL，物理像素原始分辨率）。
   * 只在遮罩窗内使用；会话结束（提交/取消）后返回空串。
   */
  frame: (): Promise<string> => invoke<string>("screenshot_frame"),

  commit: (args: CommitArgs): Promise<ScreenshotResult> =>
    invoke<ScreenshotResult>("screenshot_commit", { ...args }),

  cancel: (): Promise<void> => invoke<void>("screenshot_cancel"),

  list: (limit = 60): Promise<ScreenshotOut[]> =>
    isTauri ? invoke<ScreenshotOut[]>("screenshot_list", { limit }) : Promise.resolve([]),

  remove: (id: number): Promise<boolean> => invoke<boolean>("screenshot_delete", { id }),

  reveal: (id: number): Promise<void> => invoke<void>("screenshot_reveal", { id }),

  /** 缩略图 data URL（文件缺失时返回空串） */
  thumbnail: (id: number, maxWidth = 320): Promise<string> =>
    isTauri
      ? invoke<string>("screenshot_thumbnail", { id, maxWidth })
      : Promise.resolve(""),

  /**
   * 取字：对选区做本地离线 OCR。
   *
   * 矩形用**物理像素**（调用方自行按 `k` 换算）—— 与导出合成同一口径，
   * 免得 Rust 侧再猜一次 DPI。裁剪与识别都在 Rust 完成，不传图。
   */
  ocr: (x: number, y: number, width: number, height: number): Promise<OcrOut> =>
    invoke<OcrOut>("screenshot_ocr", { x, y, width, height }),

  /** 把取字结果写进系统剪贴板（走 Rust 的 arboard，避免依赖浏览器剪贴板权限） */
  copyText: (text: string): Promise<void> => invoke<void>("screenshot_copy_text", { text }),

  /**
   * 取字引擎信息（v0.9.0）：当前引擎 + 增强引擎资源是否齐备。
   * 非 Tauri 预览环境返回「都不可用」的安全值，避免预览里假装能用。
   */
  ocrEngineInfo: (): Promise<OcrEngineInfo> =>
    isTauri
      ? invoke<OcrEngineInfo>("ocr_engine_info")
      : Promise.resolve({
          kind: "system",
          system_available: false,
          enhanced_ready: false,
          ort_lib: null,
          models_dir: null,
          enhanced_size_mb: 0,
        }),

  dir: (): Promise<string> =>
    isTauri ? invoke<string>("screenshot_dir") : Promise.resolve(""),
};
