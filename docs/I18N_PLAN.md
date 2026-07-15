# 多语言适配方案（i18n）— screentime-pro v0.4.5

> 范围：本期仅支持 **中文（zh-CN）** 与 **英文（en-US）**。
> 状态：方案设计稿（2026-07-15），待确认后实施。
> 关联路线图：P3（窗口标题脱敏 + 更丰富图表/导出）之前，i18n 作为独立前置里程碑 **P3a**。

---

## 0. 现状结论（先看这个）

| 维度 | 结论 |
|------|------|
| 后端 Rust | **0 处中文**（已 `grep` 全量核查 `src-tauri/src`）。命令只返回数据（时长秒数、进程名、分类 `id`、枚举值），**无任何用户可见文案** → 多语言 **100% 落在前端，后端零改动、无多语言 API**。 |
| 前端中文量 | 约 **580 行 / 5762 字**含中文；其中 **真正 UI 文案 ≈ 4000 字 / 12 个组件**（Settings 138 行、Rules 93、App 53、Dashboard 45 等）；其余为开发者日志（`logger.ts` 199 字）、注释、`mock.ts` 演示数据，**不需翻译**。 |
| i18n 基建 | **零**（无 vue-i18n、无 `Intl` 使用、无 `$t`）。 |
| 分类名 | DB `categories.name` 存中文（`社交/效率与财务/...`），但 `id` 已是英文中性 key（`social/productivity/...`）→ 前端按 `id` 映射显示名即可。 |
| 日历 | `DatePicker.vue` 星期 `["日","一",...]`、`getMonth()` 月份、周日起始均**硬编码** → 必须 `Intl` 本地化。 |
| 时长 | `formatDuration` 返回 `${h}小时${m}分钟` 写死中文单位 → 按 locale 取单位词。 |

**一句话**：后端不动；前端引入 vue-i18n，抽取约 4000 字 UI 文案为 zh-CN/en-US 双词条；分类改「id→显示名」前端映射；时长/日历走 `Intl`。预计单人 **1~2 天**完成抽取与翻译。

---

## 1. 多语言适配方案

### 1.1 技术选型
- **vue-i18n v9**（Vue 3 官方生态，~5KB gz，支持 Composition API `useI18n`、运行时 locale 切换、插值、复数、以及基于 `Intl` 的日期/数字格式化）。
- **不**自建 map：后期增删语言、维护 key 一致性的成本远高于引入标准库。Tauri 三端 WebView（WKWebView / WebView2 / webkit2gtk-4.1）均完整支持。
- 安装：`npm i vue-i18n@9`（仅前端依赖，不改 Rust）。

### 1.2 语言文件目录结构与命名

```
src/i18n/
├── index.ts        # createI18n 实例 + 语言列表 + 持久化读写
├── zh-CN.ts        # 中文词条（默认 / fallback）
└── en-US.ts        # 英文词条
```

- **命名规范**：用 BCP-47 标签 `zh-CN` / `en-US`（与 `Intl` 同源，便于 `Intl.DateTimeFormat('en-US')` 直接复用）。
- **词条组织**：按模块**嵌套分组**（非扁平），如：
  ```ts
  // zh-CN.ts
  export default {
    common: { confirm: '确认', cancel: '取消', empty: '暂无数据' },
    nav:    { dashboard: '总览', trends: '趋势', rules: '分类规则', settings: '设置' },
    settings: { title: '设置', autostart: '开机自启', language: '语言' },
    dashboard: { overview: '设备使用时间', ranking: 'App 使用时长排行' },
    categories: { social: '社交', productivity: '效率与财务', dev: '开发', /* … */ },
    duration: { h: '小时', m: '分钟', s: '秒' },
  }
  ```
- **key 规则**：语义化英文 key（如 `settings.autostart`），**禁止用中文做 key**；每个词条上方加中文注释便于译者/校对。
- **fallback**：`fallbackLocale: 'zh-CN'`，任何缺失 key 回退中文，避免英文模式下出现空白。

### 1.3 语言切换机制（运行时动态切换）

```ts
// src/i18n/index.ts
import { createI18n } from 'vue-i18n'
import zhCN from './zh-CN'; import enUS from './en-US'

const SUPPORTED = ['zh-CN', 'en-US'] as const
function loadLocale(): string {
  const saved = localStorage.getItem('app-locale')
  if (saved && SUPPORTED.includes(saved as any)) return saved
  // 首启：跟随系统，但只认 zh-CN / en-US，其余 fallback zh-CN
  return navigator.language.startsWith('zh') ? 'zh-CN' : 'en-US'
}
export const i18n = createI18n({
  legacy: false,                 // Composition API 模式
  locale: loadLocale(),
  fallbackLocale: 'zh-CN',
  messages: { 'zh-CN': zhCN, 'en-US': enUS },
})
```

- **切换（无刷新）**：组件内 `const { t, locale } = useI18n(); locale.value = 'en-US'` → 全局响应式，所有 `t('key')` 立即刷新，**无需重载页面**。
- **持久化**：切换时 `localStorage.setItem('app-locale', locale.value)`；`main.ts` 挂载前 `app.use(i18n)`。
  - 注：WebView 同源 `localStorage` 在 Tauri 中持久生效（重启保留）。若未来需要跨窗口/托盘同步，再换 `@tauri-apps/plugin-store`。
- **入口**：设置页 `Settings.vue` 新增「语言」下拉（该页本就有大量文本，正好整合），选项 `中文 / English`。
- **图表注意点**：Chart.js 实例**非响应式**。切换 locale 后，需在图表组件内 `watch(() => locale.value, () => { chart.data.labels = …t(); chart.update() })`，重设坐标轴/tooltip 标签。

### 1.4 现有硬编码中文的识别与提取策略

**三类分流（不是所有中文都要翻）**：

| 类别 | 文件 | 处理 |
|------|------|------|
| **UI 文案（必翻）** | Settings / Rules / App / Dashboard / Trends / DatePicker / Modal / DailyBarChart / OverviewCard / AppRanking / DeviceSwitcher / HourlyStackedChart | 抽成 `t('key')`，建 zh-CN（值=原中文，零回归）+ en-US |
| **开发者日志（不翻）** | `src/lib/logger.ts`（199 字）、`main.ts` 的 `log.info("前端启动")`、所有代码注释 | 保持中文，非用户可见 |
| **Mock 数据（酌情）** | `src/api/mock.ts` 演示 App 名 | 改为中性示例名（`Safari`/`VS Code`），天然中英通用 |

**提取流程（可增量、可暂停）**：
1. 建 `zh-CN.ts`：逐文件把 UI 中文抽成 key，**值先填原中文**（保证中文用户零行为回归）。
2. 翻译 `zh-CN.ts` → `en-US.ts`（对照 key，不对照中文散串，避免歧义）。
3. 组件改造：
   - `<template>`：`{{ '中文' }}` → `{{ t('key') }}`
   - `<script setup>`：`const label = '中文'` → `const label = t('key')`（需 `const { t } = useI18n()`）
4. 验收：启动切 `en-US`，走查全页面，**无残留中文**即过关。

**工作量估算**：去重后约 **150~250 条 key / 4000 字**，单人 1~2 天。

### 1.5 后端接口返回内容的多语言处理

- **结论：后端无需改动。** 经全量核查，Rust 端无任何用户可见中文文案，命令只返回数据（时长秒数、进程名、分类 `id`、枚举值）。所有展示文案均由前端负责。
- **唯一需处理的中文化数据 —— 分类名**：
  - 现状：`categories` 表 `name` 列存中文（`社交/效率与财务/...`），`id` 已是英文中性 key。
  - **推荐方案（低成本）**：DB 继续以 `id` 为稳定键，前端用 `categories: { social: t('categories.social'), … }` 映射显示名；`name` 列可保留作默认 fallback 或逐步弃用。
  - 不推荐：DB 同时存中英文（改动大、同步成本高）。
- 进程名 / 窗口标题：属系统原始值，非 UI 文案，不参与翻译（窗口标题脱敏是 P3 另一半，与本方案无关）。

### 1.6 日期 / 数字 / 货币本地化

| 类型 | 现状 | 方案 |
|------|------|------|
| **时长 `formatDuration`** | `${h}小时${m}分钟` 写死 | 按 locale 取单位词（zh: 小时/分钟/秒；en: `h`/`m`/`s` 或 `Xh Ym`），单位词进 i18n 词条 `duration.*` |
| **日期 `todayStr` (YYYY-MM-DD)** | ISO 格式 | **保持不变**（locale 中立） |
| **日历 `DatePicker`** | 星期 `["日",…]`、`getMonth()` 月份、周日起始写死 | 月份/星期用 `Intl.DateTimeFormat(locale,{month:'short'})` / `{weekday:'short'}`；首列起始按 locale 配置（en 周日 / zh 可周一），加 `weekStartsOn` 常量 |
| **数字 `formatHours`** | `toFixed(1)` | 用 `Intl.NumberFormat(locale).format()` 处理小数点/千分位差异（en-US `.`、部分 locale `,`） |
| **货币** | 本应用**无货币场景** | 框架预留 `Intl.NumberFormat(locale,{style:'currency'})` 能力即可，不实现具体货币 UI |

- vue-i18n 自带 `t` / `tm` / `n`（number）/ `d`（datetime）全局格式化，可统一接管数字与日期。

---

## 2. 功能优化建议（低成本 · 高收益 优先）

基于现状（P3 规划中、单开发者、体量小、Tauri 长驻后台），按「实现成本低、用户感知强」排序。

### 2.1 交互体验
1. **语言切换入口**（即本期 i18n 落地，必做，零额外成本）。
2. **空状态 / 加载态友好化**：当前空数据可能直接空白；补多语言空态提示（如「近7天暂无数据」）。成本低、体验提升明显。
3. **破坏性操作二次确认**：「重置/重算分类规则」「清空数据」等用已有 `Modal.vue` 加确认，防误触。
4. **下拉/选择器键盘可达**：设置项、语言切换支持方向键+回车（为 a11y 打底）。

### 2.2 性能优化
5. **Chart.js 实例销毁**：组件卸载或切换 range 时 `chart.destroy()`，避免 Tauri 长驻后台下的内存泄漏（当前未见显式 destroy）。
6. **高频更新节流**：OverviewCard 等若随采样实时刷新，用 `requestAnimationFrame` 或 1s 节流，避免频繁重绘卡顿。
7. **（`prefers-reduced-motion` 预留）**：下钻/过渡动画尊重系统「减弱动态效果」。

### 2.3 边界场景
8. **首次启动无数据引导**：DB 空时给引导空态而非空白/报错。
9. **跨设备合并时区归一（高收益）**：多设备合并当前按「时间+应用+设备」去重，但未显式处理各设备本地时区差异 → 合并前统一转 UTC 或按设备时区标注，避免跨时区数据错位。
10. **历史数据膨胀**：日积月累后 `get_overview`/`get_app_ranking` 需加时间范围上限与 `(date, device)` 索引，防查询变慢。

### 2.4 可访问性（a11y）
11. **Modal 支持 Esc 关闭 + focus trap**：复用 `Modal.vue` 加 Esc 监听与初始焦点。
12. **aria-label**：图表 canvas、主要按钮加 `aria-label`，方便屏幕阅读器（时间统计类应用视障用户也有需求）。
13. **对比度达标**：若引入深浅色，满足 WCAG AA；当前 macOS 辅助功能已是系统级。

### 2.5 推荐落地优先级（P3 拆分）
- **P3a（本期 i18n，必做）**：vue-i18n + zh/en 双词条 + 运行时切换 + 分类 id→显示名映射 + 时长/日历 `Intl` 本地化。
- **P3b（紧随，低成本）**：空状态文案、破坏性操作二次确认、Chart 销毁防泄漏、跨设备时区归一、Modal Esc/aria。
- **P3c（按需）**：CSV/Excel 导出（原路线图另一半）、键盘全导航、reduce-motion。

---

## 3. 实施步骤（确认开工后）

1. `npm i vue-i18n@9`；建 `src/i18n/{index,zh-CN,en-US}.ts`。
2. `main.ts` 挂载 i18n（`app.use(i18n)`）；`index.ts` 实现 `loadLocale` + 持久化。
3. 先抽 **Settings.vue**（最大头）→ 填 zh-CN（原中文）→ 译 en-US → 自测。
4. 依次抽其余 11 个组件；Chart 组件加 `watch(locale)` 重设 label。
5. `format.ts` 时长/数字本地化；`DatePicker` 改用 `Intl`。
6. 分类：前端建 `categories` i18n 映射（按 `id`），弱化 DB `name` 中文依赖。
7. 设置页加「语言」下拉 + 持久化。
8. 全量切 `en-US` 走查 → 修残留中文 → `vue-tsc` + `build` → 三端验证（macOS 本机 + Windows/Linux CI）。

### 版本号（按你的 SemVer 规则）
- i18n 属**新功能** → **次版本位**：`0.4.5 → 0.5.0`。
- 同步点：`tauri.conf.json` version + `README.md`（badge/下载链接/版本表）+ `docs/RELEASE.md` §3 + `release/v0.5.0/NOTES.md`。
- `package.json` 的 `version` 字段（当前 `0.1.0`，长期未随版本走，建议一并校正为 `0.5.0` 或改为运行时读 `getVersion()` 不手写）。
