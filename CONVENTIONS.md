# CONVENTIONS — 代码约定（防 agent 幻视）

> 本文件是 AI 智能体（WorkBuddy / Codex / Claude Code / Cursor 等）在修改本项目时的强制规范。
> 目的是让所有 AI（包括下次会话）能从文件头注释快速理解文件意图 + 历史，避免 hallucination。

---

## 1. 文件头部注释（强制）

### 1.1 TypeScript / Vue（功能实现和样式界面文件）

所有 `.ts` / `.vue` **功能实现和样式界面文件**必须在文件头部添加以下注释：

```typescript
/**
 * 文件名.ts
 * 设计思路：描述这个文件的功能和设计原因（防 agent 幻视）
 * 修改历史：
 *   - 2026-07-09 @v0.1.0: 初始创建 - xxx
 */
```

Vue 示例：

```vue
<!--
  文件名.vue
  设计思路：xxx
  修改历史：
    - 2026-07-09 @v0.1.0: 初始创建 - xxx
-->
```

### 1.2 Rust

所有 `.rs` **功能实现文件**必须在文件头部添加：

```rust
//!
//! 文件名.rs
//! 设计思路：xxx
//! 修改历史：
//!   - 2026-07-09 @v0.1.0: 初始创建 - xxx
//!
```

### 1.3 例外

以下类型文件**不强制**带头部注释（但建议）：
- `src-tauri/src/main.rs`（入口函数，无复杂逻辑）
- 单纯的常量/枚举文件
- 测试文件（`tests/`、`*.test.ts`）
- 自动生成的类型定义文件（如 `src/api/types.ts` 与后端 Rust struct 1:1 映射）

---

## 2. 修改历史格式

每次**实质性修改**（新增功能、修复 bug、重构、性能优化）必须在文件头追加一条记录：

```
- YYYY-MM-DD @vX.Y.Z: 修改类型 - 说明
```

### 修改类型枚举

| 类型 | 含义 | 例子 |
|------|------|------|
| `新增` | 新增功能、文件、命令 | 2026-07-09 @v0.4.0: 新增 - 自动归类联网搜索 |
| `修复` | 修复 bug | 2026-07-08 @v0.3.1: 修复 - 检查更新 HTTP 403 |
| `重构` | 不改变行为的代码调整 | 2026-07-09 @v0.4.0: 重构 - 提取 Modal 通用组件 |
| `优化` | 性能/体验优化 | 2026-07-09 @v0.3.1: 优化 - 全操作反馈改 Modal 弹窗 |
| `废弃` | 删除功能/标记 deprecation | （暂无） |

### 示例

```
 * 修改历史：
 *   - 2026-07-08 @v0.1.0: 初始创建 - Tauri + Vue3 骨架
 *   - 2026-07-09 @v0.3.0: 新增 - 托盘唤起自动刷新使用时间
 *   - 2026-07-09 @v0.3.1: 修复 - 检查更新走 Atom feed 避开 403
 *   - 2026-07-09 @v0.4.0: 重构 - 提取 finalize_active_session 公共逻辑
```

**不记录的变更**（避免噪音）：
- 格式化（rustfmt / prettier 自动）
- import 顺序调整
- 注释微调
- 修错别字

---

## 3. 变量命名

| 语言 | 规则 | 例子 |
|------|------|------|
| **Rust 函数 / 变量 / 模块** | `snake_case` | `pub fn get_devices()`, `let device_id = "abc";` |
| **Rust 类型 / Trait / Enum** | `PascalCase` | `pub struct DeviceStats`, `pub enum Category` |
| **Rust 常量** | `SCREAMING_SNAKE_CASE` | `const MAX_SAMPLES: usize = 100;` |
| **Rust trait 方法** | `snake_case` | `trait PlatformTracker { fn get_foreground_app(); }` |
| **TypeScript 变量 / 函数** | `camelCase` | `const deviceName = ""; function onSave() {}` |
| **TypeScript 类型 / 类 / Vue 组件** | `PascalCase` | `interface DeviceStats`, `class Tracker`, `export default Modal` |
| **TypeScript 常量** | `UPPER_SNAKE_CASE` | `const BASE = '/api/v1';` |
| **数据库字段（MySQL）** | `snake_case` | `device_id`, `data_retention_days` |

**接口字段命名规则**：
- TypeScript interface 字段用 `snake_case`（与后端 Rust struct 一致）→ 如 `device_id`、`device_name`、`current_session`
- 原因是 Tauri `#[tauri::command]` 把 Rust snake_case 自动转 camelCase 给前端，反过来**前端必须传 camelCase**（如 `idleThreshold`、`deviceName`），但**返回的 struct 字段保持 snake_case**

---

## 4. 注释语言

### 4.1 强制中文注释场景

| 场景 | 例子 |
|------|------|
| 公开 API（pub fn / export function） | `/// 计算总时长（含跨午夜拆分）` |
| 关键设计决策 | `// 用 priority=0 让用户规则先匹配；新软件自动归类兜底` |
| 复杂算法 | `// Ritter 最小包围圆：O(n)，先 P0 → 扫一遍 → 再扫边界` |
| 行内注释 | `let cap = 0; // 0 = 全量` |
| 函数/方法注释 | `/** 单设备聚合统计（用于「按设备清理」弹窗） */` |
| Rust doc comment（/// ...） | 中文 |
| TODO / FIXME | 中文 |

### 4.2 英文注释允许场景

| 场景 | 例子 |
|------|------|
| 标准库/外部 API 的对接说明 | `// reqwest::Client::builder().timeout(8s)` |
| 行业通用术语（保留英文） | `// TODO: refactor for better performance` |
| Rust 编译错误信息（直接抄原文） | `// expect("设备名不能为空")` |

---

## 5. 其他强制规范

### 5.1 SemVer 版本同步

每次**实质性改动**必须升版本号 + 同步所有版本引用点（详见 `.workbuddy/memory/MEMORY.md`）：

- screentime-pro 同步点：`src-tauri/tauri.conf.json` `version` + README.md 5 处 + UI 兜底字符串 + 0 残留验证
- 决策矩阵：patch（bug fix + UI 优化）/ minor（新依赖、新命令、新字段）/ major（破坏性变更）

### 5.2 禁止模式

- 禁止：`any` 类型（TypeScript）；`unwrap()` 在非测试代码中滥用（Rust）
- 禁止：UI 硬编码版本号（如 `<b>0.1.0</b>`），用 `getVersion()` 运行时读
- 禁止：把 `.env` / 含 SSH 密码的文件提交 Git
- 禁止：在 Rust 中 `panic!()` 兜底错误，返回 `Result<T, String>`
- 禁止：把采样循环里的同步阻塞 IO（用 `tauri::async_runtime::spawn` 异步化）

### 5.3 错误处理

- **Rust**：返回 `Result<T, String>` 让前端能拿到具体错误（前端 catch 后用 Modal 弹窗显示）
- **TypeScript**：invoke 调用必须 try/catch，错误要 console.error + 用户可见提示
- **前端 UI**：永远不要吞错（`catch {}` 不留空），至少有 console.error

---

## 6. Tauri 命令命名规范

| 位置 | 风格 | 例子 |
|------|------|------|
| Rust `#[tauri::command]` 函数名 | `snake_case` | `pub fn get_devices()` |
| 前端 `invoke()` 命令名 | `snake_case` | `invoke("get_devices")` |
| 前端 `tracker.xxx()` 方法名 | `camelCase` | `tracker.getDevices()` |
| Tauri 参数命名 | **Rust snake_case → JS camelCase**（自动转换） | Rust: `device_id`，JS: `deviceId` |

**已踩坑案例（v0.3.0 → v0.3.1 修复）**：
- 前端 `addRule({ match_type: "contains", category_id: "other" })` → 缺参被吞，按钮无反应
- 修复：前端必须传 camelCase `addRule({ matchType: "contains", categoryId: "other" })`
- 后端 Rust 端参数声明不变：`pub fn add_rule(state, field, match_type: String, pattern: String, category_id: String, priority: i32)`

---

## 7. 提交信息规范

遵循 Conventional Commits：

```
<type>(<scope>): <subject>

<body>

<footer>
```

| type | 用途 |
|------|------|
| `feat` | 新功能 |
| `fix` | bug 修复 |
| `chore` | 构建/工具/版本号变更（非业务） |
| `docs` | 文档 |
| `refactor` | 重构（无功能变化） |
| `perf` | 性能优化 |
| `test` | 测试 |

**subject**：中文 50 字内，简明说明做什么。
**body**：解释 Why 而非 What，写设计决策和取舍。

---

## 8. 沉淀位置

- **本文件**（CONVENTIONS.md）：项目级强约束，所有 AI 必读
- **`.workbuddy/memory/`**：跨项目复用的踩坑经验（每次任务完成更新）
- **`README.md`**：用户面向的功能/构建说明
- **`docs/`**：架构/数据流等深度文档（如有）

---

**维护**：每次新增约束时追加；过期约束标 ⚠️ 一年内未触发可删除。
**最后更新**：2026-07-09 @v0.4.0（建立）