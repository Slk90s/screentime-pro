// i18n 入口：创建实例 + 读写 localStorage 持久化语言偏好
// 仅前端负责所有用户可见文案；后端只返回数据，零改动。
import { createI18n } from "vue-i18n";
import zhCN from "./zh-CN";
import enUS from "./en-US";

// 支持的语种（BCP-47）
export const SUPPORTED_LOCALES = ["zh-CN", "en-US"] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];
export const STORAGE_KEY = "app-locale";

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
}
