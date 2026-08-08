# Release 管理规范（GitHub Releases 单一权威来源）

> 本文件是 v* 版本发布的**唯一权威规范**。
> 任何 AI / 人员在打 tag、上传 GitHub Releases、改 README 版本号前，**必须**先读这一份。
>
> **GitHub Releases 地址**：[https://github.com/Slk90s/screentime-pro/releases](https://github.com/Slk90s/screentime-pro/releases)（⭐ Latest 自动指向最新稳定版）

---

## 1. 总原则（2026-07-10 用户拍板）

| 原则 | 含义 | 实现方式 |
|------|------|----------|
| 🟢 **全部保留** | 每个历史版本的 dmg/exe/deb/AppImage 都上传到 GitHub Releases，**永不删除** | 默认行为（`gh release create` 不会清旧版） |
| 📝 **写清版本区别** | 每次 Release Notes 必须说明：相对上一版修了什么 / 新增什么 / 已知问题 | 按 §4 模板写，存 `release/v{ver}/NOTES.md` |
| ⭐ **Latest 自动** | 新版本上传后**自动**获 "Latest" 徽章 | `gh release create` 默认行为，**不要加 `--latest=false`** |

---

## 2. 版本号策略（SemVer）

唯一真实来源：`src-tauri/tauri.conf.json` 的 `version` 字段。

| 档位 | 触发 | 例子 |
|------|------|------|
| **patch**（`0.4.0` → `0.4.1`） | bug 修复 / 体验优化 / 文档修正 | 修采样循环死锁、修权限检测 |
| **minor**（`0.3.x` → `0.4.0`） | 新功能 / 新依赖 / 新命令 / 新路由 | 自动归类联网搜索 |
| **major**（`X.0.0`） | 破坏性变更（协议、字段、API 重命名） | 暂无 |

同步点清单（每次升版本都查一遍）：
1. `src-tauri/tauri.conf.json` `version` —— **唯一真实来源**
2. `package.json` `version` —— 与 tauri.conf.json 保持同步（版本同步清单一环；UI 版本一律运行时 `getVersion()` 读取，**禁止硬编码**）
3. `package-lock.json` —— `npm install --package-lock-only` 同步锁文件
4. `README.md` 5 处版本号引用（badge / 下载链接 / 路径示例 / 项目结构图 / 启动命令示例）
5. `docs/RELEASE.md` §3 版本历史表

---

## 3. 版本历史

| 版本 | 发布日期 | 关键变更 | 是否推荐 |
|------|----------|----------|----------|
| v0.7.0 | 2026-08 | 大版本整合：日历月视图 + 本月统计概括（月总时长 / 活跃天数 / 日均 / 最常使用 App / 最忙的一天）+ 点「今天」同步跳回当月；桌宠设置移除冗余「已经吃饱啦 → 喂食」；喂食系统修复（饿度随喂食 +食物值、每 2 分钟 −1、每日上限 5 次、喂食反馈动画）；桌面抖动修复（过载升温阈值上调为 ≥90% 持续 20s）；中英双语（zh-CN / en-US）同步。整合 0.6.2 全部 Beta 修复。 | ⭐ Latest |
| v0.6.2-beta.1 | 2026-07-24 | 解耦皮肤系统（`src/pet/skins/` 注册表模式）+ 新增 Pop Mart 3D 潮玩桌宠（戴黄帽弹吉他熊猫 portrait）。右键菜单新增"皮肤"切换段，无后端改动。**未触动任何 v0.6.1-beta.1 原有组件**（PetCanvas/Body/Layer/编辑器/引擎/composables/types 一律禁碰）。后续 Live2D / 待办 / 番茄钟等"类似桌宠"小组件按相同 `skins/` 或 `widgets/` 模式即插即用。 | 旧版 |
| v0.6.1-beta.1 | 2026-07-21 | 桌面宠物（QQ 企鹅风格）+ 修复主窗口顶部实时栏 IPC 字段名 bug（window_title/session_seconds/idle_seconds 此前用驼峰导致恒为 undefined，已与 Rust 返回值对齐为蛇形）；其余同 v0.6.0-beta.1。Rust 端新增 5 个 pet 命令 + capabilities/pet.json。 | 旧版 |
| v0.5.0 | 2026-07-14 | 多语言国际化（i18n）：新增 zh-CN / en-US 双语切换，设置页下拉即时切换无需重载；前端自生成周期标签 / 分类名 / 时长格式化（vue-i18n + Intl）；图表随语言重渲染。零后端改动。 | 旧版 |
| v0.4.5 | 2026-07-14 | 统计概述时间范围联动：切换「今天/近7/14/30天」时「设备使用时间」与「App 使用时长排行」同步按范围聚合刷新。后端 `get_overview`/`get_app_ranking` 新增 `days` 参数（days=0 单日 / days>0 范围聚合），前端 `loadDetails()` 按 `range` 传参；OverviewCard 文案随 range 动态适配（累计/日均时长）。 | 旧版 |
| v0.4.4 | 2026-07-11 | 修「跨天今天按钮」bug（Dashboard selectedDate 缓存旧日期 → 改为每次 todayStr() 实时取值）+ linux.rs 完整适配 x11rb 0.13 GetPropertyReply 新 API（value8/value32 访问器替代私有 value/value_len 字段）→ CI 三端构建首次全通过 | 旧版 |
| v0.4.3 | 2026-07-10 | 修「default 幽灵设备」：migrate() 回填改用真实 device_id（取代字面量 'default'）+ sql/schema.sql 补 device 列 + 清理本机脏数据 | 旧版 |
| v0.4.1 | 2026-07-09 | 修 macOS 辅助功能权限检测（AXIsProcessTrustedWithOptions）+ 修采样循环 tokio 嵌套死锁 + 11 项 bug 扫描修复 + 三端构建 | 旧版 |
| v0.4.0 | 2026-07-09 | 新增按设备清理/导出/导入、自动归类联网搜索（Wikipedia + 本地字典 + LRU）、Conventions 文档 | ⚠️ 已知采样循环死锁，请立即升级 |
| v0.3.1 | 2026-07-09 | 检查更新改 GitHub Atom feed（避开 HTTP 403）；Settings 改操作反馈全部用 Modal 弹窗 | 旧版 |
| v0.3.0 | 2026-07-08 | 首次公开发布：UI/UX 框架 + 托盘唤起自动刷新 + 占用细分（应用/窗口/空闲） | 旧版 |

---

## 4. Release Notes 模板

每个版本发布前，按下面格式书写 notes（保存为 `release/v{ver}/NOTES.md`，**该目录为本地工作目录、已 gitignore，不入库**）。打 `v*` tag 时，线上 GitHub Release Notes 由 `.github/workflows/build.yml` 的 `releaseBody` 直接生成（见 §6），本模板用于本地归档草稿。

```markdown
## ✨ v0.4.4 (Latest · Recommended)

> 📅 发布日期：YYYY-MM-DD · 🐙 提交：${commit_sha:0:7}

### 🔧 关键修复（相对 v0.4.1）

- **修复** 设备不关机跨天运行时，点击 Dashboard 「今天」按钮仍显示昨日数据（根因：`const today = todayStr()` 在 `setup()` 时只算一次并永久缓存）→ 改为每次 `todayStr()` 实时取系统当前日期
- **修复** Linux CI 反复失败（x11rb 0.13 API breaking changes）→ `linux.rs` 完整适配：`GetPropertyReply.value`/`.value_len()` 私有化，改用 `value8()`/`value32()` 类型化访问器；`get_property`/`intern_atom`/`xss_query_info` 等请求全部先 `?` 解包再 `reply()`
- **修复** CI runner Rust 1.80+ `str::trim_end('\0')` 不再接受 char 参数 → 改为 `trim_end_matches('\0')`

### ➕ 优化

- 三端产物完整：macOS dmg（本机构建）+ Windows NSIS（CI 构建）+ Linux deb/AppImage（CI 构建）

### ⚠️ 已知问题（v0.4.4 仍遗留）

- Linux 需 webkit2gtk 系统库，本机 macOS 无法交叉编译，请走 GitHub Actions 构建（`tag v*` 自动触发）

### 📥 下载

| 平台 | 文件 | 大小 |
|------|------|------|
| macOS (Apple Silicon) | `ScreenTime Pro_0.4.4_aarch64.dmg` | ~6.8 MB |
| Windows (x64)         | `screentime-pro_0.4.4_x86_64-setup.exe` | TBD（CI） |
| Linux (x64)           | `screentime-pro_0.4.4_amd64.deb` | TBD（CI） |

---

## 📜 历史版本对照

### ⚠️ v0.4.0（不推荐）

- **已知严重 bug**：sampling_loop 在 async 上下文里 `block_on` 同步函数，导致应用时长不统计
- **建议**：立即升级 v0.4.1

### v0.3.1

- 修复检查更新 HTTP 403（GitHub Releases API 用 raw fetch 易触发限速，改 Atom feed）
- Settings 里所有操作反馈统一改 Modal 弹窗（以前用 Message 被吞）
- 新增 window_title 识别后自动归类（带 LRU 缓存）

### v0.3.0

- 首次公开发布（macOS/Windows）
- 基础统计：每日 / 每小时 / 应用排行 / 分类占比
- 前台应用轮询采样（默认 1s）
- 空闲检测（默认 5 分钟）
- SQLite 本地存储 + 多设备标识
```

---

## 5. 发布检查清单

每次发版本前过一遍，全部打勾才上传：

- [ ] `src-tauri/tauri.conf.json` 的 `version` 已更新（且是 commit 一部分）
- [ ] `package.json` 与 `package-lock.json` 版本已同步
- [ ] `README.md` 5 处版本号引用已同步（badge / macOS 下载 / Windows 下载 / 项目结构图 / 启动命令示例）
- [ ] `grep -rn '<旧版本号>' src/ src-tauri/src/ README.md` 验证无残留（排除 node_modules/dist/target/.workbuddy）
- [ ] `docs/RELEASE.md` §3 版本历史表已更新（新增本版本行）
- [ ] UI 已禁止硬编码版本号（必须 `getVersion()` 运行时读）
- [ ] 已打 `v{ver}` tag 并推送，确认 CI 三端构建 + GitHub Release 自动发布成功

---

## 6. 一键发布流程（CI 自动，无需本地脚本）

```bash
# ── Step 1: 本地升版本（见 §2 同步点清单）──
#   同步 tauri.conf.json / package.json / package-lock.json / README.md

# ── Step 2: 提交版本变更 ──
git add .
git commit -m "chore(release): v{ver}"
git push origin main

# ── Step 3: 打 tag 触发 CI 自动构建 + 发布 ──
git tag v{ver}
git push origin v{ver}        # 触发 .github/workflows/build.yml

# ── Step 4: 在 GitHub Releases 页面确认 Latest 指向新版本、三端产物齐全 ──
```

> 发布由 **GitHub Actions 自动完成**：推送 `v*` tag → `.github/workflows/build.yml` 三端（Windows NSIS / Linux AppImage+deb / macOS dmg）并行构建，并通过 `tauri-action` 的 `release: true` **自动创建 GitHub Release 并上传产物**（Release Notes 取自 `build.yml` 内的 `releaseBody`）。

> 旧流程依赖 `scripts/package-release.sh` 与 `scripts/release-github.sh` 本地辅助脚本；这些脚本**已设为仅本地、不入库**（gitignore）。如需完全本地发布仍可用，但**非必需**——CI tag 流程是官方发布路径。手动调整已发布 Release 仍可用 `gh` CLI（见 §7）。

---

## 7. 异常处理

| 场景 | 操作 |
|------|------|
| 当前版本有严重 bug，需把 Latest 指回旧版 | `gh release edit v0.4.0 --latest`（手动覆盖 Latest 标记） |
| 误发 release 想撤回 | `gh release delete v0.4.1 --yes`（软删，tag 仍在；可用 `gh release undelete` 恢复） |
| 真要彻底删 tag | `gh release delete v0.4.1 --yes && git push origin :refs/tags/v0.4.1` |
| CI 上传 artifact 失败 | 重新 `git push origin v{ver}` 触发 workflow；或手动 `gh release upload v{ver} ./local-file` |
| 想改已发布 release 的 notes | `gh release edit v{ver} --notes-file ./new-notes.md` |

---

**最后更新**：2026-08-08 @v0.7.0
**维护人**：所有 AI / 开发者
