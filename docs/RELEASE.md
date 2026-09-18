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
  - 2026-09-14 @v0.7.11: 修正 - 头部「最后更新」推进至 v0.7.11；示例版本号（0.7.5 / 0.7.10）
    统一为当前版；§3.1 删除已失效的「三份 releaseBody 仍是 v0.7.0 旧文案」表述（现为 v0.7.11 文案）；
    产物文件名由连字符更正确为点号（`ScreenTime.Pro_*`）；§7 版本历史表由止于 v0.7.5 补齐到 v0.7.11。
  - 2026-09-16 @v0.8.0: 修正 - 头部「最后更新」推进至 v0.8.0；示例版本号统一为 0.8.0；
    §3.1 现值说明更新为「v0.8.0 文案」；§7 版本历史表补 v0.8.0 行（截图 + 桌宠点击穿透）。
  - 2026-09-18 @v0.9.0: 修正 - v0.9.0 三端正式发布后对账；§3.1 现值说明由「v0.8.0 文案」更新为
    「v0.9.0 文案」；§7 版本表补 v0.8.2 行与 v0.9.0 行（取字增强引擎 + 桌宠拖拽/隐藏语义修复 +
    资源分发分平台）；记录 CI 三端全绿与 GitHub Release 6 资产。
  - 2026-09-18 @v0.9.0（**重切**）: 修正 - 发现首发版对 macOS / Linux 的取字链路不可用（3 个 P0），
    修复后**删除并重切同名 v0.9.0**（tag / Release 双端重建）；新增 §4.1「重切同名版本」操作步骤；
    §3.1 现值说明补「三份 releaseBody 已并入 macOS 系统 Vision 与 mac / Linux 取字修复」；
    §7 版本表 v0.9.0 行补重切说明。
  - 2026-09-18 @v0.9.0（**重切发布完成**）: 新增 - §4.2「CI 建 Release 会 403 —— 必须先建 Release
    再让 CI 上传」（含 `Resource not accessible by integration` 根因、run 权限冻结、`POST /releases`
    + `target_commitish` 自动建 tag 并触发 CI、以及误探测会把已发布版转草稿的危险）；
    修正 §4.1 中「CI 面对已存在 Release 行为不确定 → 必须删掉重建」的旧判断（实测 CI 走
    「找到既存 Release → 上传」且三次成功）；§3.1 补重切实测资产体积（MiB 口径）；
    §7 v0.9.0 行补 tag/commit 与两端 Release id、资产数与体积。
-->

> 目的：定义 **版本号从哪来、发版前改哪些地方、CI 怎么跑、出问题怎么回滚**。
> 与 README 区别：README 是用户面（去哪下载、每个版本有什么），本文件是维护者面（怎么发出去）。
> 最后更新：2026-09-18（**v0.9.1 已发布**：macOS 屏幕录制权限闸门 + 截图历史多选；详见 §7 版本表末行）

---

## 1. 版本号的唯一真实来源

**`src-tauri/tauri.conf.json` 的 `version` 字段** 是唯一权威。

原因：CI 的 `.github/workflows/build.yml` 使用 `tauri-apps/tauri-action@v0`，其
`tagName: "v__VERSION__"` / `releaseName: "v__VERSION__"` 占位符由 tauri-action
**从 `tauri.conf.json` 读取**并替换。改这里 = 改 tag 名 = 改产物文件名。

```json
// src-tauri/tauri.conf.json
{
  "version": "0.8.0"   // ← 改这里，其他都是同步
}
```

### 1.1 版本同步点清单（发版前逐项 grep 核对）

| # | 位置 | 是否影响构建 | 说明 |
|---|------|--------------|------|
| 1 | `src-tauri/tauri.conf.json` → `version` | ✅ **决定 tag / 产物名** | 唯一真实来源 |
| 2 | `package.json` → `version` | ❌ 不影响 | 需与 #1 一致，否则前端 `getVersion()` 与安装包名对不上 |
| 3 | `package-lock.json` → `version`（根节点 + `packages[""].version` 两处） | ❌ | 与 package.json 保持一致 |
| 4 | `README.md` → 顶部 Version 徽章 | ❌ | 用户第一眼看到的版本号 |
| 5 | `README.md` → 版本历史表新增一行 | ❌ | 用户判断要不要升级的依据 |
| 6 | `docs/ARCHITECTURE.md` → 头部版本声明 | ❌ | 维护者面，易遗漏 |
| 7 | `CHANGELOG.md`（本地，不入库）→ 追加 | ❌ | 详细变更留底 |

一键核对（应只剩「预期版本」一种输出）：

```bash
grep -rn '"version"' src-tauri/tauri.conf.json package.json
grep -n '"version"' package-lock.json | head -3
grep -n 'badge/version' README.md
```

### 1.2 ⚠️ `src-tauri/Cargo.toml` 的 `version` **不在**同步范围内

当前 `Cargo.toml` 是 `version = "0.1.0"`，与 `tauri.conf.json` 的 `0.8.0` 长期脱节。
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

**当前仓库的三份 `releaseBody` 已是 v0.9.1 的文案**（v0.7.6 起每版发版时同步改写）。
v0.9.1 的 body 为三段：标题下「#### 🔧 修复」写 **macOS 截图后应用全部消失、只剩桌面（P0）**
（未预检「屏幕录制」TCC 权限 → 底层只拿得到桌面壁纸层；现已预检 + 主动请求 + 弹窗引导到系统设置）、
「#### ✨ 新增：截图历史多选」写设置页「选择」模式（全选 / 取消 / 批量删除，一律进回收站）、
以及「#### 📊 包体积」表（三份 body 仍逐字相同）。
历史教训：v0.7.5 及以前长期停留在 v0.7.0 的旧文案（"整合 0.6.2 全部 Beta 修复"、
日历月视图、喂食系统修复等），与当版内容完全无关。

> 后果：若打新版 tag 却忘了改这里，线上 Release Notes 会显示**上一版**的内容，
> 用户看到的更新说明完全错误。

**发版前必须**：把三处 `releaseBody` 全部替换为 §5 的 NOTES 模板内容。建议三份保持一致
（Linux 那份可以精简，但关键修复项不能少）。

### 3.2 CI 行为要点（与直觉不同，务必记住）

| 现象 | 真相 |
|------|------|
| macOS job 里 `release: false` | Release 由 **Windows / Linux** job 创建；macOS job 先注入门禁修复脚本，再用 `gh release upload --clobber` 覆盖上传 DMG |
| macOS job 会等待 Release 就绪 | 轮询 `gh release view`，最多等 30 × 10s ≈ 5 分钟，超时则该步失败（DMG 不会更新） |
| 产物文件名 | `ScreenTime.Pro_{ver}_aarch64.dmg` / `ScreenTime.Pro_{ver}_x64-setup.exe` / `ScreenTime.Pro_{ver}_amd64.AppImage` / `ScreenTime.Pro_{ver}_amd64.deb` / `ScreenTime.Pro-{ver}-1.x86_64.rpm` |
| DMG 内被额外注入了文件 | `修复门禁.command` + `首次打开必读.txt`（无 Apple 公证，靠 `xattr -dr com.apple.quarantine` 绕过 Gatekeeper） |
| 触发方式 | push `v*` tag，或 `workflow_dispatch` 手动触发 |

---

## 4. 发布步骤

```bash
# ① 改版本号（唯一真实来源）
#    src-tauri/tauri.conf.json → version: "0.8.0"
#    同步 package.json / package-lock.json / README 徽章 / README 版本历史表 / ARCHITECTURE 头部

# ② 改写 build.yml 三处 releaseBody（§3.1，最容易漏）

# ③ 本地构建验证
npm run build
npm run tauri build

# ④ 提交
git add -A
git commit -m "release: v0.8.0"

# ⑤ 打 tag 并推送（这一步会触发 CI 三平台构建）
git tag v0.8.0
git push origin main
git push origin v0.8.0

# ⑥ 观察 CI：https://github.com/Slk90s/screentime-pro/actions
#    三平台全绿后，核对 Release 页面的 Notes 与产物
```

> 快捷脚本见 §6，可存为本地 `scripts/release.sh`（`scripts/` 已被 .gitignore 排除，不入库）。

### 4.1 重切同名版本（已发布的版本修完 P0 后重发同版本号）

用在「版本**已发布**、但随后发现该版有 P0、且该版几乎没有真实下载」的场合（v0.9.0 即为首例）。
**不升版本号**，直接删掉旧的 tag / Release 后重切：好处是用户不必理解两个版本号，
代价是同一版本号存在过两份二进制 —— **必须在 CHANGELOG 与 Release 正文里讲明**。

```bash
# ① 代码与文档改完，先跑完 §3 全部检查（P0 硬性）。版本号不用动。

# ② 推 main（build.yml 有改动时 PAT 会被拒 → 一律走 SSH over 443）
git push git@github.com:Slk90s/screentime-pro.git refs/heads/main:refs/heads/main

# ③ ⚠️ 先删 Release，再删 tag —— 顺序反了会留下指向空 tag 的 Release
TOKEN=$(gh auth token)
#    release_id 取：GET https://api.github.com/repos/Slk90s/screentime-pro/releases/tags/vX.Y.Z
curl -s -X DELETE -H "Authorization: token $TOKEN" \
  "https://api.github.com/repos/Slk90s/screentime-pro/releases/<release_id>"
git push git@github.com:Slk90s/screentime-pro.git :refs/tags/vX.Y.Z

# ④ 本地重打 tag 指向新提交并重推（触发 CI 三平台）
git tag -f vX.Y.Z <新 sha>
git push git@github.com:Slk90s/screentime-pro.git refs/tags/vX.Y.Z:refs/tags/vX.Y.Z

# ⑤ 盯 CI；出包后核对 target_commitish == 新 sha、6 资产 size
curl -s -H "Authorization: token $TOKEN" \
  "https://api.github.com/repos/Slk90s/screentime-pro/releases/tags/vX.Y.Z" \
  | python -c "import sys,json;d=json.load(sys.stdin);print(d['target_commitish']);[print(a['name'],a['size']) for a in d['assets']]"

# ⑥ Gitee 侧同法重建：先删 Release 再删 tag → 重推 tag → 重建 Release → 重传资产
```

- ✅ **只重推 tag 而不删 Release 是可行的**（v0.9.0 重切实测）：Release 已存在时，CI 的
  release 步骤走的是**「找到既存 Release → 上传资产」**路径（日志先打
  `Found release with tag vX.Y.Z.` 再 `Uploading ...`），三次运行均成功。
  真正会失败的是**反向情形**：Release 不存在时 CI 会去 **create**，而它拿到的
  `GITHUB_TOKEN` 建 Release 会 **403**（详见 §4.2）。
- ⚠️ **Gitee 侧删 Release 会释放附件配额**，重传后总量不变，无需额外腾空间；但重建后要复核
  `assets` 数 = 上传数 + 2（自动源码包）与逐文件字节数。
- 记录要求：CHANGELOG 写清「首发版有什么问题 + 重切后指向哪个 commit」；
  README 本版行标注「本版为重切版」；**三份 `releaseBody` 也要在标题下补一行重切提示**。

### 4.2 ⚠️ CI 建 Release 会 403 —— 必须"先建 Release，再让 CI 上传"

v0.9.0 重切时踩到（run `35313764939`）：CI 三平台都**构建成功**，但都在
`Build Tauri (*)` 这一步的最后倒下：

```
Couldn't find release with tag v0.9.0. Creating one.
##[error]Resource not accessible by integration -
         https://docs.github.com/rest/releases/releases#create-a-release
```

排查结论与操作约定：

- **不是代码/构建问题**，是 `GITHUB_TOKEN` 调 `POST /releases` 被拒。当时 workflow 里
  **已显式写了 `permissions: contents: write`**，仓库 `default_workflow_permissions`
  也已改成 `write`，仍然 403 → **不要再指望靠改权限让它能建 Release**。
- 同一 token **上传资产是正常的**（`Uploading ...` 无报错）→ 说明写权限本身在，
  只是 create 那一步过不去。因此**绕过方式**：发版前用 **PAT** 先把 Release 建好，
  CI 就只走上传路径。
- ⚠️ **权限在 workflow run 创建时冻结**：改完权限设置后 **rerun 同一个 run 无效**
  （run_attempt=2 仍报同样 403），必须产生**全新 run**。
- ✅ 顺手的技巧：用 `POST /releases`（PAT）+ `target_commitish=<sha>` 建**已发布** Release 时，
  GitHub 会在该 commit **自动创建 tag**，并**触发 tag push 的 workflow run** ——
  省掉一步 `git push` tag。v0.9.0 重切版就是这么发起的（run `35316034016`，三平台全绿）。
- 🔴 **绝对不要拿 `POST /releases` 去"探测"一个已有 Release 的 tag**：实测会把**已发布**的
  版本变成**草稿**（当时 v0.8.2 被这样转成草稿，出现两个同 tag 记录；删掉草稿那一条即恢复
  发布态）。要查已有 Release 请用 `GET /releases/tags/<tag>`。

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
| macOS (Apple Silicon) | `ScreenTime.Pro_X.Y.Z_aarch64.dmg` |
| Windows (x64) | `ScreenTime.Pro_X.Y.Z_x64-setup.exe` |
| Linux (x64) | `ScreenTime.Pro_X.Y.Z_amd64.AppImage` / `ScreenTime.Pro_X.Y.Z_amd64.deb` |

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
| **v0.9.1** | 2026-09-18 | ✨ 功能版 | **截图历史多选 + macOS 截图权限闸门**。① **修复 macOS「截图后应用全部消失、只剩桌面」（P0）**：根因是截图前未预检系统「屏幕录制」TCC 权限 —— 无权限时 `CGWindowListCreateImage` 只返回桌面壁纸层，现象即「截完应用全不见」。现 `begin_capture` 先 `CGPreflightScreenCaptureAccess` 预检，未授权则 `CGRequestScreenCaptureAccess` 主动请求，仍未授权回**可读中文错误**（经 `screenshot-error` 事件）；设置页弹窗 + 「打开系统设置」直达「隐私与安全性 → 屏幕录制」，`open_privacy_settings` 跳转目标同步改为 `Privacy_ScreenCapture`。② **截图历史支持多选**：新增 `screenshot_delete_many(ids)` IPC（复用单删 + 回收站，返回实际删除条数），前端 `removeMany` + 设置页「选择」工具栏（全选 / 取消全选 / 批量删除），删除项一律进系统回收站。**IPC 总数 81→82**（screenshot/ 14→15）。**发布落点**：见本节末（发版后回填 tag / Release id / 资产数）。 |
| **v0.9.0** | 2026-09-18 | ✨ 功能版 | **取字增强引擎（PaddleOCR-ONNX 本地两段式 OCR）**：det 把短边 <736px 图先放大再检测，小字/深色底 8/8 全对；设置页「取字引擎」标准/增强双选；新增 `ocr_engine_info` IPC（总数 80→**81**）。**macOS「标准」引擎 = 系统 Vision**（`VNRecognizeTextRequest`，Accurate + 语言校正，零下载、断网可用）。修复：桌宠拖拽期冻结皮肤动画（与悬浮窗同机制）、菜单↔设置页「隐藏桌宠」语义统一；**macOS / Linux 取字完全不可用（P0×3：运行库落盘路径返回目录而非文件名 → 永远落不了盘；下载超时 20s 过短；默认引擎/配置归一化不感知平台）**、macOS「取字引擎」两个选项都点不了的死锁（判据改为只看随包模型）。**资源分发分平台**：Windows 运行库+模型随包；macOS / Linux 模型随包、运行库首次使用时后台静默下载（源 GitHub Release `ocr-runtime` → Gitee 镜像，`SD_OCR_DOWNLOAD_BASE` 可覆盖，带 SHA256 校验）。⚠️ **本版为同名 tag 重切版**：首发版（`4b642ab`）对 macOS / Linux 取字不可用，修复后重切（见 §4.1）。**重切版落点**：tag / commit `39a7810`；GitHub Release `391264105`（6 资产 / 240.5 MiB）；Gitee Release `1151533`（5 附件 / 133.8 MiB，AppImage 111.9 MB 超 Gitee 单文件 100 MB 上限，仅 GitHub 提供） |
| **v0.8.2** | 2026-09-17 | 🩹 修复版 | 浮窗截图按钮（可见条件 = 截图开启 && 浮窗开启，1Hz 配置轮询，**零新增 IPC**）；桌宠右键菜单 ↔ 设置页状态同步 |
| **v0.8.0** | 2026-09-16 | ✨ 功能版 | **屏幕截图（新功能）**：全局快捷键 `CmdOrCtrl+Shift+A` 唤起遮罩式全屏选区，仿 QQ 浮动工具栏（保存 / 全屏 / 圆角 / 阴影 / 确认 / 取消），**截图默认进剪贴板**，本地历史 FIFO（超限移入回收站）；新增 `screenshot/` 模块、`capture` 窗口、`screenshots` 表、10 个 IPC 命令。**桌宠点击穿透修复（P0）**：整窗开关改为 32×32 alpha 命中网格 + 全局光标轮询（`pet/hit_mask.rs`）。**macOS 前台全屏真识别**（`CGWindowListCopyWindowInfo` + `CGDisplayBounds`）。依赖 `windows` 0.58→0.62，新增 `xcap` / `arboard` / `image` / `tauri-plugin-global-shortcut` |
| **v0.7.11** | 2026-09-14 | 🚀 正式版 | 三端统一悬浮指标条（macOS 菜单栏文字指标整体移除）+ 托盘快捷项改「启用状态栏」总开关并与设置页双向同步 + 死代码清理（约 140 行 + 单测） |
| **v0.7.10** | 2026-09-13 | 🩹 修复版 | 首屏竞态重试（P1，`invoke` 层对「state not managed」瞬态错误指数退避最多 6 次）+ 设备名稳定显示 |
| **v0.7.9** | 2026-09-13 | 🩹 修复版 | macOS 托盘左键单击「只闪一下」修复（P0）+ 深色模式分段控件选中项可见修复（P1）+ Windows 设备名诊断加固 |
| **v0.7.8** | 2026-09-11 | 🩹 修复版 | macOS CPU / 内存恒 0% 连环修复（P0，改用 Mach `host_statistics` / `sysctlbyname`）+ Windows / Linux 托盘回归纯品牌图 + Windows 安装「写入错误」NSIS 钩子修复（P0） |
| **v0.7.7** | 2026-09-10 | 🩹 修复版 | macOS 闪退修复（P0，`statfs` 栈越界写）+ Windows 首启闪命令框修复 + Win/Linux 补齐内存与磁盘指标 + 浮窗数值漏乘 100 修正 + panic 兜底钩子 |
| **v0.7.6** | 2026-09-10 | 🚀 正式版 | 状态栏指标体系跨平台落地：悬浮指标条 + 托盘画字 + 托盘右键快捷菜单 + 设置页状态栏卡片；新增 `float_window` / `fullscreen` / `tray_icon` / `network` 模块与 `get/set_status_bar_config` |
| **v0.7.5** | 2026-08-23 | 🚀 正式版 | macOS 状态栏系统指标（CPU / 内存 / 磁盘），`MetricsSampler` 1s 采样、磁盘 30s 缓存，变化 <1% 不重绘托盘；新增 `get/set_system_metrics_enabled`、`get_system_metrics`。Win/Linux 隐藏该功能 |
| **v0.7.4** | 2026-08-18 | 🚀 正式版 | 日历月视图 + 本月统计概括；桌宠喂食系统修复（饱食度随喂食 / 衰减变化 + 反馈动画）；桌面频繁抖动修复；中英双语同步 |
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
