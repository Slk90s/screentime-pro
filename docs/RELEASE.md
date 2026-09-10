# ScreenTime Pro — 发布规范（RELEASE）

<!--
修改历史：
  - 2026-09-04 @v0.7.5: 新增 - 补写本文件。此前 README（§下载/Release、§版本历史与发布规范）
    与 docs/ARCHITECTURE.md（§11 文件清单）均引用 `docs/RELEASE.md`，但该文件从未入库：
    根因是 .gitignore 第 38-40 行 `docs/*.md` + `docs/**/*.md` 整体排除，仅白名单了
    `!docs/ARCHITECTURE.md`。本文件已同步在 .gitignore 追加 `!docs/RELEASE.md` 例外，
    否则写了也会被 git 忽略（这是本文件"消失了还被引用"的直接原因）。
  - 2026-09-04 @v0.7.5: 修正 - ARCHITECTURE §5 与 README 中"完整列表见 RELEASE.md"一类
    交叉引用，统一指向本文件；本文件与 README 版本历史表保持一致。
-->

> 目的：定义 **版本号从哪来、发版前改哪些地方、CI 怎么跑、出问题怎么回滚**。
> 与 README 区别：README 是用户面（去哪下载、每个版本有什么），本文件是维护者面（怎么发出去）。
> 最后更新：2026-09-04（对账 v0.7.5 实际状态）

---

## 1. 版本号的唯一真实来源

**`src-tauri/tauri.conf.json` 的 `version` 字段** 是唯一权威。

原因：CI 的 `.github/workflows/build.yml` 使用 `tauri-apps/tauri-action@v0`，其
`tagName: "v__VERSION__"` / `releaseName: "v__VERSION__"` 占位符由 tauri-action
**从 `tauri.conf.json` 读取**并替换。改这里 = 改 tag 名 = 改产物文件名。

```json
// src-tauri/tauri.conf.json
{
  "version": "0.7.5"   // ← 改这里，其他都是同步
}
```

### 1.1 版本同步点清单（发版前逐项 grep 核对）

| # | 位置 | 是否影响构建 | 说明 |
|---|------|--------------|------|
| 1 | `src-tauri/tauri.conf.json` → `version` | ✅ **决定 tag / 产物名** | 唯一真实来源 |
| 2 | `package.json` → `version` | ❌ 不影响 | 需与 #1 一致，否则前端 `getVersion()` 与安装包名对不上 |
| 3 | `README.md` → 顶部 Version 徽章 | ❌ | 用户第一眼看到的版本号 |
| 4 | `README.md` → 版本历史表新增一行 | ❌ | 用户判断要不要升级的依据 |
| 5 | `docs/ARCHITECTURE.md` → 头部版本声明 | ❌ | 维护者面，易遗漏 |
| 6 | `CHANGELOG.md`（本地，不入库）→ 追加 | ❌ | 详细变更留底 |

一键核对（应只剩「预期版本」一种输出）：

```bash
grep -rn '"version"' src-tauri/tauri.conf.json package.json
grep -n 'badge/version' README.md
```

### 1.2 ⚠️ `src-tauri/Cargo.toml` 的 `version` **不在**同步范围内

当前 `Cargo.toml` 是 `version = "0.1.0"`，与 `tauri.conf.json` 的 `0.7.5` 长期脱节。
**这是已知且无害的**：Tauri 2 的打包版本、DMG/NSIS 文件名、Git tag 全部取自
`tauri.conf.json`，Cargo 版本号不参与。

- **不要**为了"看起来对齐"而单独改 Cargo.toml（会造成"版本号已升"的错觉）。
- 若未来确实要对齐，请在发版清单里显式加一行，并理解它**不改变任何产物行为**。

---

## 2. SemVer 策略

当前处于 `0.x.y` 阶段，采用「宽松 SemVer」：

| 变更类型 | 版本号动作 | 示例 |
|----------|-----------|------|
| 新功能 / 较大 UI 改动 | **minor** +1，patch 归零 | 0.6.2 → 0.7.0 |
| bug 修复 / 细节优化 / 文案 | **patch** +1 | 0.7.3 → 0.7.5 |
| 破坏性变更（数据结构、IPC 契约） | major +1（0.x 阶段即 0.x+1.0） | — |
| 内部自测 / 灰度 | 加预发布后缀 | `0.6.2-beta.1`、`0.6.2-36` |

规则：

- **Beta 后缀**只用于本地自测包，不进 README 正式版本历史表的「🚀 正式版」列。
- 一旦打正式版 tag，**不再复用该版本号**；发现问题就升 patch 重发（参考 v0.7.0 → v0.7.1 的修复重发）。
- 预发布包在 CI 中把 `prerelease: true`，正式版 `prerelease: false`。

---

## 3. 发布前检查清单（P0 硬性）

> 构建成功 ≠ 可发布。以下任一项未通过，**不得打 tag**。

- [ ] **本地构建通过**：`npm run build`（含 `vue-tsc --noEmit` 类型检查）+ `npm run tauri build`
- [ ] **版本号已在 §1.1 全部同步点更新**，且 `grep` 无旧版本残留
- [ ] **⚠️ `build.yml` 的 `releaseBody` 已改写为本版内容**（见 §3.1，最易漏）
- [ ] `README.md` 版本历史表已追加本版行
- [ ] 三平台功能自测通过（至少：启动采样、托盘、数据落库、导出导入）
- [ ] 无遗留调试代码 / 硬编码本地路径 / 明文凭证
- [ ] **已获得用户明确的「发布」指令**（红线：AI 不得自行 push/tag/release）

### 3.1 ⚠️ 最易漏：`releaseBody` 是硬编码死文本

`.github/workflows/build.yml` 中三个 job（windows / linux / macos）**各写了一份**
`releaseBody`，内容是**硬编码的字符串**，不会自动跟着版本变。

**当前仓库里的三份 `releaseBody` 仍是 v0.7.0 的旧文案**（"整合 0.6.2 全部 Beta 修复"、
日历月视图、喂食系统修复等），与 v0.7.5 的 macOS 状态栏指标毫无关系。

> 后果：若直接打 `v0.7.6` 的 tag 而不改这里，线上 Release Notes 会显示 v0.7.0 的内容，
> 用户看到的更新说明完全错误。

**发版前必须**：把三处 `releaseBody` 全部替换为 §5 的 NOTES 模板内容。建议三份保持一致
（Linux 那份可以精简，但关键修复项不能少）。

### 3.2 CI 行为要点（与直觉不同，务必记住）

| 现象 | 真相 |
|------|------|
| macOS job 里 `release: false` | Release 由 **Windows / Linux** job 创建；macOS job 先注入门禁修复脚本，再用 `gh release upload --clobber` 覆盖上传 DMG |
| macOS job 会等待 Release 就绪 | 轮询 `gh release view`，最多等 30 × 10s ≈ 5 分钟，超时则该步失败（DMG 不会更新） |
| 产物文件名 | `ScreenTime-Pro_{ver}_aarch64.dmg` / `_x64-setup.exe` / `_amd64.AppImage` / `.deb` |
| DMG 内被额外注入了文件 | `修复门禁.command` + `首次打开必读.txt`（无 Apple 公证，靠 `xattr -dr com.apple.quarantine` 绕过 Gatekeeper） |
| 触发方式 | push `v*` tag，或 `workflow_dispatch` 手动触发 |

---

## 4. 发布步骤

```bash
# ① 改版本号（唯一真实来源）
#    src-tauri/tauri.conf.json → version: "0.7.6"
#    同步 package.json / README 徽章 / README 版本历史表 / ARCHITECTURE 头部

# ② 改写 build.yml 三处 releaseBody（§3.1，最容易漏）

# ③ 本地构建验证
npm run build
npm run tauri build

# ④ 提交
git add -A
git commit -m "release: v0.7.6"

# ⑤ 打 tag 并推送（这一步会触发 CI 三平台构建）
git tag v0.7.6
git push origin main
git push origin v0.7.6

# ⑥ 观察 CI：https://github.com/Slk90s/screentime-pro/actions
#    三平台全绿后，核对 Release 页面的 Notes 与产物
```

> 快捷脚本见 §6，可存为本地 `scripts/release.sh`（`scripts/` 已被 .gitignore 排除，不入库）。

---

## 5. NOTES 模板（填进 `releaseBody` / Release 页面）

```markdown
### 屏幕时间管家 ScreenTime Pro vX.Y.Z — 本次更新

**✨ 新功能**
- 

**🐛 修复**
- 

**⚠️ 已知问题**
- 

**📦 下载**
| 平台 | 文件 |
|------|------|
| macOS (Apple Silicon) | `ScreenTime-Pro_X.Y.Z_aarch64.dmg` |
| Windows (x64) | `ScreenTime-Pro_X.Y.Z_x64-setup.exe` |
| Linux (x64) | `ScreenTime-Pro_X.Y.Z_amd64.AppImage` / `.deb` |

> macOS 首次打开若提示「已损坏」，请先拖入「应用程序」，再右键打开 DMG 内的
> 「修复门禁.command」。完整版本历史见仓库 README。
```

---

## 6. 一键发布脚本（本地用，不入库）

```bash
#!/usr/bin/env bash
# scripts/release.sh — 交互式发版：改版本 → 校验 → 提交 → 打 tag
# 用法：bash scripts/release.sh 0.7.6
# 注意：脚本**不会**自动 push，最后一步需人工确认（红线：AI 不得自行发布）
set -euo pipefail

NEW="${1:-}"
[[ -z "$NEW" ]] && { echo "用法: bash scripts/release.sh <version>  例: 0.7.6"; exit 1; }

CONF="src-tauri/tauri.conf.json"
PKG="package.json"

echo "==> 写入版本 $NEW"
sed -i.bak "s/\"version\": \".*\"/\"version\": \"$NEW\"/" "$PKG" && rm -f "$PKG.bak"
# tauri.conf.json 的 version 通常是文件中第一个 "version"
python3 - "$CONF" "$NEW" <<'PY'
import json,sys,re
p,v=sys.argv[1],sys.argv[2]
s=open(p,encoding='utf-8').read()
s=re.sub(r'("version"\s*:\s*)"[^"]*"', lambda m: m.group(1)+'"%s"'%v, s, count=1)
open(p,'w',encoding='utf-8').write(s)
PY

echo "==> 校验同步点"
grep -n '"version"' "$CONF" | head -1
grep -n '"version"' "$PKG"  | head -1

echo "==> 构建（类型检查 + 打包）"
npm run build
npm run tauri build

echo
echo "⚠️  发布前请确认已完成："
echo "   1. build.yml 三处 releaseBody 已改为本版内容"
echo "   2. README 版本历史表已追加"
echo "   3. 三平台自测通过"
echo
read -r -p "确认提交并打 tag v$NEW ? [y/N] " ok
[[ "$ok" != "y" ]] && { echo "已取消"; exit 0; }

git add -A
git commit -m "release: v$NEW"
git tag "v$NEW"
echo "已本地提交并打 tag。请人工执行："
echo "   git push origin main && git push origin v$NEW"
```

---

## 7. 版本历史

> 用户面完整说明见 `README.md` 版本历史表（含每版亮点）。下表为发布状态留底。

| 版本 | 发布时间 | 状态 | 关键说明 |
|------|----------|------|----------|
| **v0.7.5** | 2026-08-23 | 🚀 正式版 | macOS 状态栏系统指标（CPU / 内存 / 磁盘），`MetricsSampler` 1s 采样、磁盘 30s 缓存，变化 <1% 不重绘托盘；新增 `get/set_system_metrics_enabled`、`get_system_metrics`。Win/Linux 隐藏该功能 |
| **v0.7.3** | 2026-08-13 | 🚀 正式版 | macOS 日志/导出修复 + 桌宠开关同步 + macOS 拖拽跟手 + DMG 门禁脚本正式生效 |
| **v0.7.2** | 2026-08-09 | 旧版 | 本地自动备份 + macOS 门禁修复 + 设备 ID 稳定化 |
| **v0.7.1** | 2026-08-08 | 旧版 | v0.7.0 的修复重发：补齐缺失入库的桌宠皮肤源码资产 |
| **v0.7.0** | 2026-08-08 | 旧版 | 整合 0.6.2 全部 Beta 修复 + 新功能 |
| v0.6.2-36 | 2026-08-08 | 旧版 | 蜘蛛侠体验细化 |
| v0.6.2-beta.1 | 2026-07-24 | 旧版 | 解耦皮肤系统 + Pop Mart 3D 潮玩桌宠 |
| v0.5.0 | 2026-07-14 | 旧版 | 多语言国际化（i18n） |
| v0.4.1 | 2026-07-09 | 旧版 | 修复采样循环死锁、macOS 权限 API bug 等 11 项 |
| v0.4.0 | — | ⚠️ 不推荐 | 已知严重 bug：采样循环 `block_on` 嵌套导致时间不统计 |
| v0.3.1 | 2026-07-08 | 旧版 | UI/UX 优化 |
| v0.3.0 | 2026-07-08 | 旧版 | 首版公开 |

---

## 8. 回滚

| 场景 | 处理 |
|------|------|
| CI 构建失败 | 删除远端 tag 与 Release，本地修好后**升 patch 重发**（不复用版本号）：`git tag -d vX.Y.Z && git push origin :refs/tags/vX.Y.Z` |
| 已发出但有严重 bug | 升 patch 修复重发，并在新 Release 顶部标注「建议 vX.Y.Z 用户立即升级」 |
| 误发预发布为正式版 | GitHub Release 页面勾选 `pre-release` 更正 |

---

## 9. 红线

1. **构建成功 ≠ 可发布**：必须等用户明确指令才能 `push` / `tag` / `release`。
2. **不得自动改写 `releaseBody`**：每次发版由维护者按 §5 模板填写。
3. **不得复用版本号**：一旦打过正式 tag，该版本号作废。
4. **不得把 `release/`、`output/`、`scripts/`、`.env` 提交入库**（已在 .gitignore 排除）。
