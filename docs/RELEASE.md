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
| v0.4.2 | 2026-07-10 | 新增统一日志系统（tracing + tauri-plugin-log）：4 处核心埋点、Settings 一键导出、生产环境 15MB 上限、采样循环分钟级节流 | ⭐ Latest · 推荐升级 |
| v0.4.1 | 2026-07-09 | 修 macOS 辅助功能权限检测（AXIsProcessTrustedWithOptions）+ 修采样循环 tokio 嵌套死锁 + 11 项 bug 扫描修复 + 三端构建 | 旧版 |
| v0.4.0 | 2026-07-09 | 新增按设备清理/导出/导入、自动归类联网搜索（Wikipedia + 本地字典 + LRU）、Conventions 文档 | ⚠️ 已知采样循环死锁，请立即升级 |
| v0.3.1 | 2026-07-09 | 检查更新改 GitHub Atom feed（避开 HTTP 403）；Settings 改操作反馈全部用 Modal 弹窗 | 旧版 |
| v0.3.0 | 2026-07-08 | 首次公开发布：UI/UX 框架 + 托盘唤起自动刷新 + 占用细分（应用/窗口/空闲） | 旧版 |

---

## 4. Release Notes 模板

每个版本发布前，按下面格式书写 notes（保存为 `release/v{ver}/NOTES.md`），脚本会读这个文件作为 `--notes-file`：

```markdown
## ✨ v0.4.1 (Latest · Recommended)

> 📅 发布日期：YYYY-MM-DD · 🐙 提交：${commit_sha:0:7}

### 🔧 关键修复（相对 v0.4.0）

- **修复** sampling_loop 里 `tauri::async_runtime::block_on` 嵌套导致 tokio worker 卡死 → 应用秒级使用时长全部丢失。改为同步 `lookup_category` + `spawn_blocking` 派发
- **修复** macOS 辅助功能 API 一直返回 false（ad-hoc 签名 identifier 漂移问题） → 改用 `AXIsProcessTrustedWithOptions` + `kAXTrustedCheckOptionPrompt: false`（不弹窗、不缓存）
- **修复** 并发 `Mutex` poison 导致批量统计雪崩 → 全量 `lock().unwrap()` → `unwrap_or_else(|e| e.into_inner())`
- **修复** 11 项其他 bug（详见 CONVENTIONS.md 踩坑记录）

### ➕ 优化

- 三端产物完整：macOS dmg + Windows NSIS（CI 构建）+ Linux deb/AppImage（CI 构建）

### ⚠️ 已知问题（v0.4.1 仍遗留）

- Linux 交叉编译需 webkit2gtk 系统库，请走 GitHub Actions 构建（`tag v*` 自动触发）

### 📥 下载

| 平台 | 文件 | 大小 |
|------|------|------|
| macOS (Apple Silicon) | `ScreenTime Pro_0.4.1_aarch64.dmg` | 6.3 MB |
| Windows (x64)         | `screentime-pro_0.4.1_x86_64-setup.exe` | 6.4 MB |
| Linux (x64)           | `screentime-pro_0.4.1_amd64.deb` | TBD（CI） |

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

**最后更新**：2026-07-10 @v0.4.1（建立）
**维护人**：所有 AI / 开发者
