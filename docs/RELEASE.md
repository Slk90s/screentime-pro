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
1. `src-tauri/tauri.conf.json` `version`
2. `package.json`（仅在确实需要暴露给前端时，比如 About 面板用 `getVersion()` —— 这块不要手动改，运行时读）
3. `README.md` 5 处版本号引用（badge / 下载链接 / 路径示例 / 项目结构图 / 启动命令示例）
4. `docs/RELEASE.md` §3 版本历史表
5. UI 兜底字符串（如有硬编码 —— **禁止硬编码**，必须用 `getVersion()` 运行时读）

---

## 3. 版本历史

| 版本 | 发布日期 | 关键变更 | 是否推荐 |
|------|----------|----------|----------|
| v0.5.0 | 2026-07-14 | 多语言国际化（i18n）：新增 zh-CN / en-US 双语切换，设置页下拉即时切换无需重载；前端自生成周期标签 / 分类名 / 时长格式化（vue-i18n + Intl）；图表随语言重渲染。零后端改动。 | ⭐ Latest · 推荐升级 |
| v0.4.5 | 2026-07-14 | 统计概述时间范围联动：切换「今天/近7/14/30天」时「设备使用时间」与「App 使用时长排行」同步按范围聚合刷新。后端 `get_overview`/`get_app_ranking` 新增 `days` 参数（days=0 单日 / days>0 范围聚合），前端 `loadDetails()` 按 `range` 传参；OverviewCard 文案随 range 动态适配（累计/日均时长）。 | 旧版 |
| v0.4.4 | 2026-07-11 | 修「跨天今天按钮」bug（Dashboard selectedDate 缓存旧日期 → 改为每次 todayStr() 实时取值）+ linux.rs 完整适配 x11rb 0.13 GetPropertyReply 新 API（value8/value32 访问器替代私有 value/value_len 字段）→ CI 三端构建首次全通过 | 旧版 |
| v0.4.3 | 2026-07-10 | 修「default 幽灵设备」：migrate() 回填改用真实 device_id（取代字面量 'default'）+ sql/schema.sql 补 device 列 + 清理本机脏数据 | 旧版 |
| v0.4.1 | 2026-07-09 | 修 macOS 辅助功能权限检测（AXIsProcessTrustedWithOptions）+ 修采样循环 tokio 嵌套死锁 + 11 项 bug 扫描修复 + 三端构建 | 旧版 |
| v0.4.0 | 2026-07-09 | 新增按设备清理/导出/导入、自动归类联网搜索（Wikipedia + 本地字典 + LRU）、Conventions 文档 | ⚠️ 已知采样循环死锁，请立即升级 |
| v0.3.1 | 2026-07-09 | 检查更新改 GitHub Atom feed（避开 HTTP 403）；Settings 改操作反馈全部用 Modal 弹窗 | 旧版 |
| v0.3.0 | 2026-07-08 | 首次公开发布：UI/UX 框架 + 托盘唤起自动刷新 + 占用细分（应用/窗口/空闲） | 旧版 |

---

## 4. Release Notes 模板

每个版本发布前，按下面格式书写 notes（保存为 `release/v{ver}/NOTES.md`），脚本会读这个文件作为 `--notes-file`：

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
- [ ] `README.md` 5 处版本号引用已同步（badge / macOS 下载 / Windows 下载 / 项目结构图 / 启动命令示例）
- [ ] `grep -rn '<旧版本号>' src/ src-tauri/src/ README.md` 验证无残留（排除 node_modules/dist/target/.workbuddy）
- [ ] 已产出 `release/v{ver}/` 三端产物
- [ ] `release/v{ver}/NOTES.md` 按 §4 模板写好，**包含与上一版的对比**
- [ ] `docs/RELEASE.md` §3 版本历史表已更新
- [ ] UI 已禁止硬编码版本号（必须 `getVersion()` 运行时读）

---

## 6. 一键发布流程

```bash
# ── Step 1: 本机构建 macOS ──
npm install
npm run tauri:build           # 产出 dmg + app（在 src-tauri/target/...）

# ── Step 2: 收集产物 ──
bash scripts/package-release.sh   # 自动复制到 release/v{ver}/

# ── Step 3: 推 tag → CI 自动跑 Windows + Linux ──
git add .
git commit -m "chore(release): bump v{ver}"
git push origin main
git tag v{ver}
git push origin v{ver}        # 触发 .github/workflows/build.yml

# ── Step 4: 等 CI 完成 → 把下载的 artifacts 放到 release/v{ver}/ ──
# (windows-build, linux-build artifacts)

# ── Step 5: 一键发布到 GitHub ──
bash scripts/release-github.sh --notes release/v{ver}/NOTES.md

# 或：草稿模式（先审核再公开）
bash scripts/release-github.sh --draft
```

`scripts/release-github.sh` 会自动：
- 读 `tauri.conf.json` 的 `version`
- 校验 `release/v${ver}/NOTES.md` 存在
- 调 `gh release create v${ver} ...` 上传所有产物 + notes
- GitHub 自动把新 Release 标为 **Latest**

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

**最后更新**：2026-07-11 @v0.4.4
**维护人**：所有 AI / 开发者
