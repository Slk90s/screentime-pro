// 时长格式化与日期工具
// 用于把「秒数」转换成「X小时Y分钟」等本地化展示，以及生成 YYYY-MM-DD 日期串。
import { i18n } from "../i18n";

// 访问 locale 以注册响应式依赖：语言切换时，模板中调用本函数的位置会重新渲染
function currentLocale(): string {
  return (i18n.global.locale.value as string) ?? "zh-CN";
}

export function formatDuration(totalSeconds: number): string {
  // 触发语言依赖收集
  void currentLocale();
  const s = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return i18n.global.t("duration.hm", { h, m });
  if (m > 0) return i18n.global.t("duration.m", { m });
  return i18n.global.t("duration.s", { s: sec });
}

export function formatHours(totalSeconds: number): string {
  // 触发语言依赖收集
  void currentLocale();
  return (totalSeconds / 3600).toFixed(1);
}

export function todayStr(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate()
  ).padStart(2, "0")}`;
}
