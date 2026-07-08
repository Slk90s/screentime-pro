// 时长格式化与日期工具
// 用于把「秒数」转换成「X小时Y分钟」等中文展示，以及生成 YYYY-MM-DD 日期串。

export function formatDuration(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}小时${m}分钟`;
  if (m > 0) return `${m}分钟`;
  return `${s}秒`;
}

export function formatHours(totalSeconds: number): string {
  return (totalSeconds / 3600).toFixed(1);
}

export function todayStr(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate()
  ).padStart(2, "0")}`;
}
