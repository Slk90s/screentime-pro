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
> 最后更新：2026-09-22（**v0.9.6 发版中**：弹窗交互修复版 —— 修复设置页弹窗「确定」关不掉
> （v0.9.4 footer slot 覆盖 Modal 自带按钮时丢失 close() 行为；`onAlertConfirm` 补回关闭
> 且先关再跑回调，避免结果弹窗被误关）+「检查更新」标题漏传 `{current}` 插值参数
> （曾显示「已是最新版本（v）」）。本版仅改 `src/views/Settings.vue`，应用功能与 v0.9.5 一致。
> 上一版 v0.9.5：Windows 升级卸载修复（NSIS 卸载钩子重写）。再上一版 v0.9.4（二次重切）：
> tag / commit `03c676c`；CI run `35560701713` 三平台全 success；GitHub Release `392693550`
> 6 资产；Gitee Release `1157170` 4 附件；**Gitee 配额 311.9 / 1024 MiB**。
> CI run `35503786492` 三平台全 success；
> GitHub Release `392387464` **6 资产 / 240.6 MiB**（exe 27.17 / dmg 26.84 / app.tar.gz 25.59 /
> deb 27.13 / rpm 27.13 / AppImage 106.71 MiB）；Gitee Release `1155845` **5 附件 / 133.9 MiB**
> （AppImage 106.7 MB 超 Gitee 单文件上限，仅 GitHub 提供）；发版前删除 Gitee 旧版 v0.7.6/7/8
> 释放 165.2 MiB，配额现为 868.1 / 1024 MiB（余 155.9））

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

**当前仓库的三份 `releaseBody` 已是 v0.9.6 的文案**（v0.7.6 起每版发版时同步改写）。
v0.9.6 的 body 为一段：「#### 🐞 弹窗交互修复（本版重点）」写**弹窗「确定」关不掉根因**
（v0.9.4 footer slot 覆盖 Modal 自带按钮时丢失 close() 行为）+ **标题漏传插值参数**
（「已是最新版本（v）」→ 补 `{current}`）+ **确认链路顺序修正**（先关再跑回调），
末尾为「#### 📊 包体积」表（三份 body 仍逐字相同）。
上一版 v0.9.5 的 body 为「#### 🔧 Windows 升级卸载修复（本版重点）」（「无法卸载!」根因 + 轮询杀进程 + 卸载尾部校验）。
上一版 v0.9.4 的 body 为「#### 🟣 macOS 截图体验重做（本版重点）」（紧凑授权弹窗 / 2s 轮询 / 启动预请求 / SCK 引擎 / 权限四件套 IPC 84→88 / Info.plist）。
上一版 v0.9.2 的 body 为四段（「🔧 修复」+「🔍 可追溯性（新增）」+「📊 包体积」）。
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
| **v0.9.6** | 2026-09-22 | 🐞 **弹窗交互修复版** | **修复设置页弹窗「确定」关不掉（P1）+「检查更新」标题缺版本号**。根因一：v0.9.4 为 macOS 权限弹窗加「重启应用」按钮时用 footer slot **覆盖**了 Modal 自带按钮，自定义「确定」只执行确认回调、**丢失了 Modal 自带按钮的 close() 行为**——v0.9.4 起设置页所有弹窗点「确定」均无反应（只能右上角 X / 点遮罩 / Esc 关闭）。修复（仅改 `src/views/Settings.vue`）：`onAlertConfirm` 补回 `alertOpen=false`，且**先关弹窗再跑回调**（回调里常再弹「已清理」等结果弹窗，顺序反了会被误关）。根因二：「检查更新」弹窗标题 `t("settings.upToDate")` **漏传 `{current}` 插值参数**，标题渲染成「已是最新版本（v）」；补传后正确显示「已是最新版本（v0.9.6）」。本版应用功能与 v0.9.5 完全一致。**发布落点**：（CI 完成后回填） |
| **v0.9.5** | 2026-09-22 | 🔧 **Windows 升级卸载修复版** | **修复「升级时提示无法卸载!」（P0）**。根因：NSIS 卸载钩子 `SCREENTIME_KILL_APP` 固定 3 轮 kill + 500ms **不复查进程是否真正退出**——安全软件挂钩 / 内核延迟释放 exe 映像句柄时，1.5 秒窗口内进程仍占用主程序文件，NSIS `Delete` **静默失败**（不报错不中断），卸载器 rc=0 返回但 exe 残留，升级安装器 `PageLeaveReinstall` 检测 `${FileExists} exe` 命中即弹「无法卸载!」并中止（2026-09-21 两次升级实测复现，0.9.1 文件/注册表/快捷方式全部原样保留）。修复（仅改 `src-tauri/windows/hooks.nsh`）：① 杀进程重写为**轮询 10 轮「kill → 500ms → 复查」**，进程消失即 `${ExitFor}`，耗尽后 `nsExec taskkill /F /T /IM` 强杀兜底再等 1s；② 新增 `SCREENTIME_ENSURE_EXE_GONE`（POSTUNINSTALL）——exe 仍在 → `SetErrorLevel 1 + Quit` 显式 rc=1，安装器视为「用户取消」走豁免路径静默返回（NSIS `Abort` 实测 rc=2 不在豁免列表，必须用 `SetErrorLevel+Quit`）。本版应用功能与 v0.9.4 完全一致。**发布落点**：tag / commit `7eb18bf`；CI run `35675298738` 三平台全 success；GitHub Release `393392844`（6 资产）；本机已静默安装验证（注册表 / exe 版本串 / 启动日志均 0.9.5） |
| **v0.9.4** | 2026-09-20 | 🟣 **macOS 截图体验重做版**（二次重切） | **授权 UX 重做 + ScreenCaptureKit 引擎**。① **失败提示精简为一句话**——`macos.rs::ensure_ready` 失败返回 `__PERM__:<reason>` 短码（reason ∈ app_translocated / quarantined / adhoc_signature / not_granted），前端按 reason 映射一句话文案，完整证据转 `tracing::warn` 日志。② **紧凑授权弹窗 `PermissionDialog.vue`**——一句话原因 + 「去授权」/「重置权限并重启」两按钮，每 2 秒轮询 `screenshot_permission_status`，勾选后自动翻转「✅ 已授权，点此重启」，详细诊断折叠进「详情」区。③ **启动预请求授权**——`schedule_startup_permission_request` 启动 3 秒后后台预检，未授权则主动触发系统弹窗并 emit `screenshot-permission-changed`，让本应用提前出现在系统设置列表。④ **macOS 14+ SCK 引擎**——`screenshot/macos_sck.rs`：`SCShareableContent::get()` → `SCContentFilter` → `SCScreenshotManager::capture_image` 单帧，替代已废弃的 `CGWindowListCreateImage`（xcap 底层）；低版本自动回落 xcap。⑤ Info.plist 补 `NSScreenCaptureUsageDescription`（SCK 触发 TCC 弹窗必需键，缺失系统直接终止应用）。⑥ 新增权限四件套 IPC（**总数 84→88**）：`screenshot_permission_status` / `screenshot_request_permission` / `screenshot_open_permission_settings` / `screenshot_restart_app`。⑦ **P0×2 修复 macOS 启动秒崩**：SCK 链强链接 `@rpath/libswift_Concurrency.dylib` 但产物无 LC_RPATH → dyld SIGABRT。首次重切用 `src-tauri/.cargo/config.toml` 的 `[target.aarch64-apple-darwin] rustflags`——**复查证实无效**（tauri CLI 在仓库根 spawn cargo，cargo 配置发现只从 cwd 向上找，永远看不到 src-tauri/.cargo/）；二次重切改 **`build.rs` 注入 `cargo:rustc-link-arg-bins`**（cwd 无关，构建脚本直传 cargo，100% 生效），Windows 本地已验证指令输出。⑧ macOS `minimumSystemVersion` 10.15→**12.0**（`libswift_Concurrency.dylib` 为 macOS 12+ 系统组件，强链接下 10.15/11 无法启动）。⚠️ **已知限制不变**：未用 Developer ID 签名（用户无法购买 Apple 证书），每次升级后授权仍可能失效需重新授权。**发布落点（二次重切）**：commit 见 git log；**一次重切落点**：tag / commit `521b61f`；CI run `35554355978`；GitHub Release `392659386`；Gitee Release 重建。**首发落点（已删）**：commit `3395626`；CI run `35513505604`；GitHub Release `392440742`（6 资产 / 252.7 MiB）；Gitee Release `1156204`（5 附件 / 134.4 MiB）。 |
| **v0.9.3** | 2026-09-20 | 🔴 **macOS 截图修复版** | **macOS 屏幕录制授权判定重写 + 一键重置权限**。① 修复「系统设置里已勾选屏幕录制、应用仍报无法截图」（P0）—— 旧实现只依据 `CGPreflightScreenCaptureAccess()` **单信号**判定，而该系统接口存在**「授权已生效、却持续返回过期 false」**的已知行为（同进程内一旦为 false 可能一直为 false），会把有效授权误判为无权限；现改为**双信号判定**：官方预检为真 **或**「窗口标题探针」可读即放行（`CGWindowListCopyWindowInfo` + `kCGWindowLayer==0`，与窗口**内容**共用同一张 TCC 授权），命中时打 `warn` 日志便于统计复现率。② 修复「截出来只有桌面壁纸」—— 真实成因是**未取得屏幕录制授权**时系统会**主动抹掉其他窗口内容**（并非没截到），失败文案讲清成因并给出可照做的处置步骤（含「完全退出再打开」这一必须步骤）。③ 新增 `reset_screen_capture_permission` IPC（**总数 83→84**）与**「重置权限并重启」一键按钮**（权限弹窗 + 截图失败提示条）：自动执行 `tccutil reset ScreenCapture com.screentime.pro` 清脏授权记录并重启应用。④ macOS 截图逻辑独立为 `src-tauri/src/screenshot/macos.rs`（权限闸门 `ensure_ready` + 证据采集 `collect` + 重置 `reset_permission` 集中一处），失败文案按**实际检测证据分档**（App Translocation / 未签名 ad-hoc / 多副本安装各给处置，并打印运行路径与 codesign 诊断）。⑤ 🐞 修复窗口枚举返回的 `CFArray` 未 `CFRelease` 造成的内存泄漏。⚠️ **已知限制**：未使用 Developer ID 签名（ad-hoc 签名每次构建 cdhash 变化 → 系统视为新应用），**每次升级后授权可能失效**、需重新授权一次，而系统设置开关仍显示为「开」；根治需签名 + 公证。**发布落点**：tag / commit `beb17fd`（轻量 tag `v0.9.3`）；CI run `35503786492` 三平台全 success；GitHub Release `392387464`（**6 资产 / 240.6 MiB**：exe 28.49 / dmg 28.15 / app.tar.gz 26.83 / deb 28.44 / rpm 28.44 MB / AppImage 111.90 MB）；Gitee Release `1155845`（**5 附件 / 133.9 MiB**，AppImage 超 Gitee 单文件 100 MB 上限，仅 GitHub 提供）；发版前删除 Gitee 旧版 v0.7.6 / v0.7.7 / v0.7.8 三个 Release（释放 165.2 MiB），配额现为 868.1 / 1024 MiB。 |
| **v0.9.2** | 2026-09-20 | 🔍 **可追溯性版** | **日志可追溯性改造 + 截图失败全局提示 + 内存·状态修复**。① 日志**每行带版本戳**（`logging.rs` 的 `VersionWriter` 包装 Writer，行首插 `vX.Y.Z`；前端经 `tauri-plugin-log.format()` 同带戳）——任意一行即可判定来源版本；② 新增**用户行为审计日志** —— `logging::audit(action, detail)` 写 `target="audit"`，落独立 `audit.<date>.log`（主 layer 用 `filter_fn` 排除、audit layer 收），9 处埋点（规则增删改 / 设置保存 / 状态栏配置 / 按设备备份清理 / 保留期清理 / 截图批量删除 / OCR 取字），**不含 window_title**（沿用隐私红线）；③ 保留期 `MAX_LOG_FILES` 3→**14**，清理由「仅启动跑一次」改为**启动 + 运行时每小时**双清理；④ **卸载前自动备份日志**：`src-tauri/windows/hooks.nsh` 的 `NSIS_HOOK_PREUNINSTALL` 把 `logs/` 复制到「文档\ScreenTimePro-Logs」（该宏在卸载器 `RmDir /r "$LOCALAPPDATA\com.screentime.pro"` **之前**执行，失败静默不阻断卸载）；⑤ **截图失败提升为主窗全局提示** —— Rust 侧原已 `emit_to("main","screenshot-error")` 但**前端无人监听**（事件空转），现改由 `App.vue` 全局监听 + 右下角 toast（权限类错误带「打开系统设置 / 重启应用」），Settings 侧重复监听已移除；⑥ 🐞 修复自动分类规则**反向覆盖**用户禁用项（`db::insert_rule` 原为 `ON CONFLICT DO UPDATE SET enabled=1`，改为新增 `db::ensure_rule()` 走 `INSERT OR IGNORE`）、3 处监听器未注销（`usePetHitMask` 2 + `Settings` 1）、选区过小时仍可取字/导出（`selTooSmall` 禁用）；⑦ 补齐 `.shot-toast` 样式（自 v0.8.0 起该提示条**完全没有 CSS**，一直是裸 div）。**IPC 总数 83**（本版未新增命令；上一版 v0.9.1 为 82）。**发布落点**：tag / commit `b5b2ae8`（轻量 tag `v0.9.2`）；CI run `35496168069` 三平台全 success；GitHub Release `392344703`（**6 资产 / 240.6 MiB**：exe 27.19 / dmg 26.87 / app.tar.gz 25.59 / deb 27.13 / rpm 27.13 / AppImage 106.71 MiB）；Gitee Release `1155479`（**5 附件 / 133.9 MiB**，AppImage 超 Gitee 单文件 100 MB 上限，仅 GitHub 提供）；Gitee 全仓附件配额 899.5 / 1024 MiB。 |
| **v0.9.1** | 2026-09-18 | ✨ 功能版 | **截图历史多选 + macOS 截图权限闸门**。① **修复 macOS「截图后应用全部消失、只剩桌面」（P0）**：根因是截图前未预检系统「屏幕录制」TCC 权限 —— 无权限时 `CGWindowListCreateImage` 只返回桌面壁纸层，现象即「截完应用全不见」。现 `begin_capture` 先 `CGPreflightScreenCaptureAccess` 预检，未授权则 `CGRequestScreenCaptureAccess` 主动请求，仍未授权回**可读中文错误**（经 `screenshot-error` 事件）；设置页弹窗 + 「打开系统设置」直达「隐私与安全性 → 屏幕录制」，`open_privacy_settings` 跳转目标同步改为 `Privacy_ScreenCapture`。② **截图历史支持多选**：新增 `screenshot_delete_many(ids)` IPC（复用单删 + 回收站，返回实际删除条数），前端 `removeMany` + 设置页「选择」工具栏（全选 / 取消全选 / 批量删除），删除项一律进系统回收站。**IPC 总数 81→82**（screenshot/ 14→15）。**发布落点**：tag / commit `9ca1e62`（轻量 tag `v0.9.1`）；CI run `35329358723` 三平台全 success；GitHub Release `391350602`（**6 资产 / 240.5 MiB**：exe 27.17 / dmg 26.83 / app.tar.gz 25.57 / deb 27.12 / rpm 27.13 / AppImage 106.71 MiB）；Gitee Release `1151864`（**5 附件 / 133.8 MiB**，AppImage 超 Gitee 单文件 100 MB 上限，仅 GitHub 提供）；Gitee 全仓附件配额 765.6 / 1024 MiB。 |
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
