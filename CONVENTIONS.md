# CONVENTIONS — 代码核心约束（防 agent 幻视）

> 本文件是 AI 智能体（WorkBuddy / Codex / Claude Code / Cursor 等）在修改本项目时的强制规范。
> 与 `AGENTS.md`（记忆机制 + 工作流 + 红线）共同构成项目 AI 协作规范。
> 目的是让所有 AI（含下次会话）能从文件头注释快速理解文件意图 + 历史，避免 hallucination。

---

## 1. 文件头部注释（防 agent 幻视，强制）

所有 `.ts` / `.vue` **功能实现文件**必须在文件顶部写「设计思路 + 修改历史」：

```typescript
/**
 * 文件名.ts
 * 设计思路：描述这个文件的功能和设计原因（防 agent 看到旧代码就整体重写）
 * 修改历史：
 *   - 2026-01-01 @v0.1.0: 初始创建 - xxx
 */
```

Vue SFC 顶部用 `<!-- -->`，禁止用 `/* */` 且内容含 `<`（否则模板 build 失败）。

**非常规操作必写头部说明**：数据归并 / 字段重命名 / 接口废弃 / 架构调整 / 旧代码清理 / 第三方替换 / 临时绕过方案 / 实验性 feature flag——必须注明「修改原因 + 注意事项 + 不再适用的旧约定」。

Rust 功能实现文件用 `//!` 文档注释写同样结构：
```rust
//! 文件名.rs
//! 设计思路：xxx
//! 修改历史：
//!   - 2026-07-09 @v0.1.0: 初始创建 - xxx
```

---

## 2. 修改历史格式

每次实质性修改追加一条：
```
- YYYY-MM-DD @vX.Y.Z: 类型 - 说明
```
**类型枚举**：`新增` / `修复` / `重构` / `优化` / `废弃`。

**不记录**：格式化、import 顺序调整、纯注释微调、错别字。

---

## 3. 命名规范

| 语言 | 规则 | 例子 |
|------|------|------|
| TS 变量 / 函数 | `camelCase` | `const deviceName`, `function onSave()` |
| TS 类型 / 类 / Vue 组件 | `PascalCase` | `interface DeviceStats`, `export default Modal` |
| TS 常量 | `UPPER_SNAKE_CASE` | `const BASE = '/api/v1'` |
| Rust 函数 / 变量 / 模块 | `snake_case` | `pub fn get_devices()`, `let device_id` |
| Rust 类型 / Trait / Enum | `PascalCase` | `pub struct DeviceStats`, `pub enum Category` |
| Rust 常量 | `SCREAMING_SNAKE_CASE` | `const MAX_SAMPLES: usize = 100;` |
| 数据库字段 / SQL 建表 | `snake_case` | `device_id`, `data_retention_days` |

**接口字段**：前端 TS interface 用 `camelCase`，后端 entity/DTO 用 `snake_case` 列名 + `camelCase` 属性（ORM 自动映射）。
> Tauri 专项见 §10：命令返回值 `snake_case`，JS 参数 `camelCase`。

---

## 4. 注释语言

- **强制中文**：公开 API、关键设计决策、复杂算法、函数/方法注释、TODO/FIXME。
- **允许英文**：标准库/外部 API 对接说明、行业通用术语、编译/数据库错误信息原文。

---

## 5. SemVer 版本同步（强制）

每次实质性改动必须升版本号 + 同步所有引用点（`package.json` / `tauri.conf.json` / `package-lock.json` / `README.md` / 文件头修改历史 / `CHANGELOG.md`）。

**决策矩阵**：
- `patch`：bug fix + 文档优化
- `minor`：新依赖、新路由、新端点、新模块
- `major`：破坏性变更（权限模型重构、字段重命名、API 协议变更）

**验证**：改完后 `grep -rn "旧版本号"` 全项目（排除 node_modules/dist/.workbuddy），必须无残留。

---

## 6. 禁止模式

- 禁止 `any` 类型（用 `unknown` + 类型收窄代替）。
- 禁止 UI 硬编码版本号（用运行时读取或 changelog 数据源）。
- 禁止将 `.env` / 含密码的部署脚本提交 Git。
- 禁止单文件部署（必须全量打包）。
- 禁止在 `client/` 下用系统 Node（用 managed Node 或 `unset NODE_OPTIONS`）。
- 禁止修改已合入的数据库迁移脚本。
- 禁止 Rust 非测试代码滥用 `unwrap()` / `panic!()` 兜底——返回 `Result<T, String>`。

---

## 7. 错误处理

- 前端 `async` 调用必须 `try/catch` + `console.error` + 用户可见提示（如 Modal），禁止空 `catch {}` 吞错。
- 后端用结构化错误返回（Rust `Result<T, String>`），禁止在 guard/interceptor 裸抛 `new Error`。

---

## 8. 文档备份红线（先备份，集中落 `bak/` 文件夹）

当**无法确认一个文档是要「覆盖重写」还是「追加」**时（含任何可能改动既有 `.md` / 文档类内容的操作），必须先备份、再动手：

1. **建专用备份文件夹**：仓库根目录 `bak/`，与源文件所在目录**分离、集中存放**；禁止再把 `.bak` 散落在源文件旁边。
   - `bak/` 建议加入 `.gitignore`（`bak/` 是临时安全网，非发布内容；与 `output/` / `generated-images/` 同属不入库产物，见 §12.2）。
2. **复制进 `bak/`**：文件名保留原意并带日期后缀，例如 `bak/CONVENTIONS.md.2026-08-14.bak`。
3. **写备份头注释（强制）**：在备份文件**头部**注明备份时间与备份版本，便于溯源：
   - Markdown / 纯文本：首行写 HTML 注释 `<!-- 备份时间: 2026-08-14 16:15:50 备份版本: vX.Y.Z 源文件: CONVENTIONS.md -->`（渲染不可见）。
   - 代码类（`.ts` / `.vue` / `.rs`）：用对应注释风格，首行写「备份时间 + 备份版本」。
4. 再对源文件执行覆盖 / 追加。
5. **用户确认改动无误前，不得删除 `bak/` 中对应备份**；确认后由用户决定保留或删除。

> 原则：备份是安全网，只要用户未明确丢弃，就留在 `bak/`。

---

## 9. 提交信息（Conventional Commits）

```
<type>(<scope>): <subject>
```
- `type`：`feat` / `fix` / `chore` / `docs` / `refactor` / `perf` / `test`
- `subject` 中文 50 字内
- `body` 写 **Why** 而非 What（与 §1 文件头注释呼应）

---

## 10. Tauri 命令命名规范

| 位置 | 风格 | 例子 |
|------|------|------|
| Rust `#[tauri::command]` 函数名 | `snake_case` | `pub fn get_devices()` |
| 前端 `invoke()` 命令名 | `snake_case` | `invoke("get_devices")` |
| 前端 `tracker.xxx()` 方法名 | `camelCase` | `tracker.getDevices()` |
| Tauri 参数 | **Rust snake_case → JS camelCase**（自动转换） | Rust: `device_id`，JS: `deviceId` |

**已踩坑案例（v0.3.0 → v0.3.1 修复）**：
- 前端 `addRule({ match_type: "contains", category_id: "other" })` → 缺参被吞，按钮无反应
- 修复：前端必须传 camelCase `addRule({ matchType: "contains", categoryId: "other" })`
- 返回值同样：Rust `keep_days` / `last_date` → JS 读 `keepDays` / `lastDate`

---

## 11. GitHub Releases 发布规范（2026-07-10 @v0.4.1）

详见 `docs/RELEASE.md`，三条总原则（用户拍板）：

| 原则 | 实现 |
|------|------|
| 🟢 全部保留（不删旧版本） | `gh release create` 默认行为 |
| 📝 写清版本区别（每个 Release Notes 必含与上版对比） | `release/v{ver}/NOTES.md` 模板 |
| ⭐ Latest 自动（新版本自动获徽章） | 默认行为，**不要加 `--latest=false`** |

一键发布：`bash scripts/release-github.sh`（支持 `--draft` / `--notes`）。
> ⚠️ 当前 CI 由推 `v*` tag 触发（见 AGENTS.md §4.4 / §4.6）。macOS DMG 门禁脚本注入 + `--clobber` 覆盖上传流程已在 `build.yml` 落地。

---

## 12. 项目专属约束（ScreenTime Pro · 代码 / 架构）

### 12.1 桌宠（pet）架构约束
- **视觉**：`popmart-3d` 用单张透明 PNG（`popmart-panda-single.png`），禁四层切图。
- **渲染**：`transparent + always_on_top` WebView2 窗，DWM 每帧重合成 → 动画只做 `transform` 进合成层；**投影/滤镜绝不与 `transform` 同元素**（filter 破坏合成层掉帧）；升温用 transform 抖动非 hue-rotate。
- **拖拽**：OS 原生 `getCurrentWindow().startDragging()`（零 IPC 跟手），4px 阈值区分拖拽/点击；不可用时回退 rAF + `move_pet_window`。见 `usePetDrag.ts`。
  - ⚠️ **macOS 坑**：无边框透明窗 `startDragging` 不可靠（窗口不真正跟手），`usePetDrag` 加 `IS_MAC` 判断，macOS 强制走手动 `move_pet_window`（与右键菜单一致）；Windows/Linux 仍走原生。
- **多窗口同步**：`main` / `pet` / `pet-menu` 各持独立 `petStore`（仅 `localStorage` 共享），跨窗靠 Tauri 事件 + `petStore.reload()`：`pet-skin-changed` / `pet-custom-updated` / `pet-store-updated` / `pet-enabled-changed`。
- **多动作**：姿态真变化须 AI 图生图生成透明姿势精灵叠 idle 帧，勿只 CSS transform 整图。
- **抖动/菜单修复（v0.7.2）**：点击反应动画右键不触发、拖拽后置 `suppressNextClick`、拖拽开始 `clearAnim`、去掉 `is-walking` 绑定（其 per-frame transform 是透明窗抖动主因）。菜单路径本就正确（`pet-menu` 窗预声明 + idempotent 创建 + `pet-menu-shown` 事件）。

### 12.2 仓库目录规范
- **入库**：`sql/` schema+seed；`src/` / `src-tauri/` / `.github/`；`.gitignore`；根目录 `README.md` / `AGENTS.md` / `CONVENTIONS.md`（随仓库公开推送）。
- **文档本地留存范围（用户硬性要求）**：仅 `docs/*.md`（架构 / 发布 / 计划类文档）不传 GitHub，统一 gitignored；根目录 `README.md` / `AGENTS.md` / `CONVENTIONS.md` 随仓库公开。`.gitignore` 已改为 `docs/*.md` + `docs/**/*.md`；`docs/*.md` 此前已 `git rm --cached` 解除跟踪（本地文件保留）。
- **不入库**：`scripts/`、`output/`、`generated-images/`、`bak/`、`release/`、`dist/`、`node_modules/`、`target/`（gitignored）。agent 计划不传 GitHub。

### 12.3 设备 ID（硬件稳定化）
- `lib.rs::hardware_device_id()` 读硬件（macOS IOPlatformUUID / Win MachineGuid / Linux machine-id）加 `hw-` 前缀。
- 存量 ID 沿用不覆盖，禁回退随机 ID。

### 12.4 本地自动备份 + 回收站
- 每天落 `screentime_backup_YYYY-MM-DD.json`；配置 `backup_enabled` / `path` / `keep_days` / `last_date`；`commands.rs` 的 `perform_backup` / `prune_backup_files`；`lib.rs` 起 auto-backup 线程（启动 60s 后每 30min，同日跳过）。
- **回收站**：每天备份成功后，把**前一天**的备份（文件名日期 < 今天）用 `trash` crate 移到系统回收站（非致命），今日最新留导出目录供手动拷云。

### 12.5 日志（macOS 关键坑）
- `logging::init` 必须**先** `std::fs::create_dir_all(app_log_dir)`——`tracing_appender` 只 `create(true)` 建文件不建父目录，mac 全新机 `~/Library/Logs/com.screentime.pro` 不存在 → `init` 返回 Err → mac 一个日志文件都没有。
- 导出命令 `export_logs` 过滤须复用 `logging::is_our_log_file`（真实滚动名 `app.YYYY-MM-DD.log`），**勿**写 `starts_with("app.log")`（会把所有滚动日志漏掉 → 导出 txt 正文全空，同类历史 bug 已修 `dir_size` 但漏了导出）。

### 12.6 出包（MSVC / Windows 坑）
- 禁止 cargo config 加 `linker=rust-lld`（MSVC 下找不到）。
- VS 路径 `C:\Program Files\Microsoft Visual Studio\18\Community\...`。
- 临时改 build 脚本为 `vue-tsc --noEmit && vite build --emptyOutDir false` 跑 `npm run tauri:build`，完再还原。产物用 Python `shutil.copy2` 进 `output/`。

### 12.7 数据类型约定
- Tauri 命令返回值 `snake_case`（`keep_days` / `last_date`），JS 参数 `camelCase`（`keepDays`），见 §10。
- 设备 ID 统一 `hw-` 前缀字符串。

---

**维护**：每次新增约束时追加到对应章节；过期约束标 ⚠️（一年内未触发可删除）。
**最后更新**：2026-08-14 @v0.7.3（新增 §8 文档备份落 `bak/`；§12.2 改为所有 `*.md` 仅本地、不传 GitHub）
