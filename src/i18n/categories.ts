/**
 * 设计思路：按稳定分类 ID 查询本地化名称，建立语言响应依赖，并在缺少词条时回退原 ID。
 * 修改历史：2026-11-03 @v1.2.12: 新增 - 补充文件头设计说明与修改历史。
 */
// 分类 id → 本地化显示名
// 分类 id 为英文中性键（social/productivity/...），后端只返回 id 与中文 name；
// 前端统一按 id 经 i18n 取显示名，切换语言时自动更新。
import { i18n } from "./index";

// 访问 locale 以注册响应式依赖，确保语言切换时调用处重新渲染
function touchLocale(): string {
  return i18n.global.locale.value as string;
}

export function categoryName(id: string): string {
  touchLocale();
  const key = `categories.${id}`;
  const val = i18n.global.t(key) as string;
  // vue-i18n 在缺少 key 时返回 key 本身（如 "categories.xxx"），此时回退到 id
  return val.startsWith("categories.") ? id : val;
}
