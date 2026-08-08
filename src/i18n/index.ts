// i18n 入口：创建实例 + 读写 localStorage 持久化语言偏好
// 仅前端负责所有用户可见文案；后端只返回数据，零改动。
import { createI18n } from "vue-i18n";
import { emit } from "@tauri-apps/api/event";
import zhCN from "./zh-CN";
import enUS from "./en-US";

// 支持的语种（BCP-47）
export const SUPPORTED_LOCALES = ["zh-CN", "en-US"] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];
export const STORAGE_KEY = "app-locale";
// 跨 webview 语言同步事件名（桌宠菜单等独立窗口有各自独立的 i18n 实例）
export const LOCALE_EVENT = "locale-changed";

// 读取已保存语言；无则用浏览器/系统语言，缺省 zh-CN
function loadLocale(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && (SUPPORTED_LOCALES as readonly string[]).includes(saved)) {
      return saved as Locale;
    }
  } catch {
    /* localStorage 不可用时忽略 */
  }
  const nav = typeof navigator !== "undefined" ? navigator.language : "zh-CN";
  return nav.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

export const i18n = createI18n({
  legacy: false, // Composition API 模式（useI18n）
  locale: loadLocale(),
  fallbackLocale: "zh-CN",
  messages: {
    "zh-CN": zhCN,
    "en-US": enUS,
  },
});

// 运行时切换语言并持久化
export function setLocale(locale: Locale): void {
  i18n.global.locale.value = locale;
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    /* 存储不可用时忽略 */
  }
  // 跨 webview 同步：桌宠菜单（pet-menu）是独立 Tauri 窗口，拥有各自独立的
  // i18n 实例，仅改主窗口实例无法让它跟随语言切换。经 Tauri 全局事件广播
  // （与 pet-skin-changed / pet-enabled-changed 同模式），各窗口在 main.ts
  // 注册监听后同步本窗口 i18n 实例。非 Tauri 环境（浏览器预览）静默忽略。
  try {
    void emit(LOCALE_EVENT, locale);
  } catch {
    /* 非 Tauri 环境忽略 */
  }
}

// 各 webview 启动时调用一次：监听 locale-changed 并同步本窗口独立的 i18n 实例。
// 放在 main.ts（每个 webview 都执行），确保 pet-menu 等独立窗口跟随语言切换。
let localeSyncReady = false;
export function initLocaleSync(): void {
  if (localeSyncReady) return;
  localeSyncReady = true;
  void import("@tauri-apps/api/event")
    .then(({ listen }) =>
      listen<Locale>(LOCALE_EVENT, (e) => {
        i18n.global.locale.value = e.payload;
      }),
    )
    .catch(() => {
      localeSyncReady = false;
    });
}

// 打开菜单等场景的兜底：直接从 localStorage 重读当前语言并同步本窗口 i18n 实例。
// 覆盖"菜单窗口在语言切换之后才创建，错过 locale-changed 事件"的边界情况。
export function syncLocaleFromStorage(): void {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && (SUPPORTED_LOCALES as readonly string[]).includes(saved)) {
      const next = saved as Locale;
      if (next !== i18n.global.locale.value) i18n.global.locale.value = next;
    }
  } catch {
    /* 存储不可用时忽略 */
  }
}
