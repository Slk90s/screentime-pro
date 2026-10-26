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
  - 2026-10-26 @v1.2.10: 修正 - 头部推进至 v1.2.10（macOS 交互体验专项：浅色反色 / 高度单一源 /
    菜单栏 template 图 / 更新检测三成因 / 权限弹窗 / 材质层泄漏 + 权限中心 + conf 补 x/y）；
    按「只留当前版 + 上一版一行」压缩，v1.2.8 收成一行；§3.1「当前 releaseBody 现值」由 v1.2.8
    更新为 **v1.2.10**（82 行 / 9265 字节 × 3 份逐字节相同）；§7 版本表补 v1.2.10 行。
    ⚠️ 本版**尚未发布**，§7 行的「发布落点」待 CI 跑完后回填（第二个提交，不得移动 tag）。
  - 2026-10-26 @v1.2.8: 修正 - v1.2.8 双端发布后对账。① 头部「最后更新」推进至 v1.2.8，
    并把**累积了 8 个版本（v0.9.5 ~ v1.2.7）的「上一版」堆叠段落压回 v1.2.7 一行**——
    原头部占 46 行、越滚越长已不可读，本文件价值在「怎么发」而非「发过什么」（后者归 §7 与
    README 版本历史表）。② §3.1「当前 releaseBody 现值」由 **v1.0.0 更正为 v1.2.8**
    （此前连续 7 个版本未同步此句，属文档描述滞后而非代码问题），并补「缩进块扫描」校验法
    与正则抽取的历史失败症状。③ §7 版本表补 v1.2.8 行（材质换档 / 双层职责分离 / native_blur 分档）。
-->

> 目的：定义 **版本号从哪来、发版前改哪些地方、CI 怎么跑、出问题怎么回滚**。
> 与 README 区别：README 是用户面（去哪下载、每个版本有什么），本文件是维护者面（怎么发出去）。
> 最后更新：2026-10-26（**v1.2.10 待发布** —— macOS 交互体验专项修复；
> 版本号同步见 §1.1，IPC 总数 **85**）。
> 本版内容：**六项修复 + 一项新增，全部落在 macOS 观感与授权体验；Windows / Linux 零功能改动，可不升级**。
> ① **浮窗浅色外观白字看不清（P0）** —— CSS 全按深色背景定标、**没跟随 `prefers-color-scheme`**，
> 浅色壁纸的玻璃底上白字仅 **1.36:1**（AA 正文线 4.5:1）；**降 alpha 治不了**（压暗 0.48 仍 3.40:1、
> 加到 0.80 也才 3.9），**唯一出路是反色**。新增 light 档（白遮罩 + `#1c1c1e`）。
> ⚠️ 反色**只挂 `.has-blur`（仅 macOS）**：Windows 无系统材质跟随 appearance，且反色在 Win11 Bloom
> 深壁纸上会从 16.78:1 掉到 **6.32:1**（净亏 10.5）并把胶囊变成灰白板。
> ② **浮窗高度/圆角两个源** —— Rust `set_size` 下发值与 CSS 硬编码并存，冷启动/开关翻转时底部露
> 8px 材质、radius 22 vs 18 呈双弧；改 `height:100%` + `border-radius:999px`，高度只剩一处定义。
> ③ **菜单栏图标浅色栏显脏灰** —— 彩色 App 图标在浅色栏仅 **1.91:1**；改用 template 剪影图，
> 系统按栏明暗自动染色（新增 `trayTemplate.png`/`@2x`，覆盖率 25.0%/25.1% vs 理论 24.8%）。
> ④ **检测不到更新（三成因）** —— 只查一次（`setTimeout`→6h `setInterval`）/ 时间戳在「已是最新」
> 时也写掉 24h 窗口 / `reqwest` 关掉 `default-features` **连带丢 `system-proxy`**（只认环境变量、
> 不读系统代理，Clash·Surge 用户必中）。另加三通道抓取（atom→REST→Gitee）+ `is_version_tag` 校验。
> ⑤ **权限弹窗反复弹** —— 删启动预请求；`check_permissions` 屏幕录制改双信号口径；横幅 dismiss
> 标记落盘 + focus 回调改边沿触发。⑥ **浮窗材质层只增不减** —— `window-vibrancy` 的 `apply` 不去重、
> `clear` 一次只摘一层，改循环清到 `Ok(false)` 收尾。
> **新增**：设置页「权限中心」（仅 macOS，只读、不轮询）。**另**：`tauri.conf.json` float 窗口补
> `x/y: 24`（`tao` 无 position 时 `ns_window.center()` 会让首开残影压在主窗口正中），
> `height`/`minHeight` **刻意维持 36**（`show()` 早于 `fitWidth`，写 44 会看见一帧 44px 胶囊）。
> ⚠️ **本版 macOS 侧改动未经真机目视复验**：浅色档实际材质亮度、模板图在菜单栏的观感、
> 权限中心文案在紧凑弹窗里是否被截断，均须在 Mac 上确认。
> 上一版 v1.2.8：**macOS 15 悬浮窗玻璃右端外溢 3px + 双弧** —— 两层同尺寸且都带 `border-radius`
> （曲率中心不重合 → 双弧）+ 外层 `width:100%` 在 `border-box` 下让内容区比窗口窄 2px、
> `NSVisualEffectView` 按窗口矩形绘制 → 右端 3px 材质原色带。修法：双层职责分离（外层只管铺满+居中、
> 内层 `width:max-content` 只管真实尺寸，圆角只保留外层一次）+ `WIDTH_SLACK=2` 语义明确为
> 「2 × 外层边框宽」+ 材质 `HudWindow`(13)→`UnderWindowBackground`(21) + 新增 `native_blur` 做配色分档。
> tag / commit `bd58503`；CI run `40266414622` 三平台全 success；GitHub Release `423843748`
> **6 资产 / 195.90 MiB**；Gitee Release `1241841` **5 附件 / 96.75 MiB**（AppImage 超 100 MB 仅 GitHub）。
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

**当前仓库的三份 `releaseBody` 已是 v1.2.10 的文案**（v0.7.6 起每版发版时同步改写），
三份**逐字节相同**（实测各 **9265 字节 / 82 行**），可用「缩进块扫描」而非正则校验一致性：
取 `releaseBody: |` 的基准缩进，正文缩进为基准 +2，遇到首个非空且缩进 ≤ 基准的行即视为块结束。
（历史上用正则 `releaseBody: \|\n(.*?)(?=\n      - name:)` 抽取，会吞掉后续兄弟键，
症状是三份「长度分别为 3757/3757/3758」且差异恰好落在 `release: true` vs `release: false`。）
v1.2.10 的 body 结构为：标题行 `### 屏幕时间管家 ScreenTime Pro — v1.2.10` →
`🐞 修复一~六`（浅色反色 / 高度双源 / 菜单栏图标 / 更新检测三成因 / 权限弹窗 / 材质层泄漏）→
`✨ 新增`（权限中心）→ `📌 已知局限`（**明确写出 macOS 侧未经真机目视复验**）→ `✅ 验证`。
上一版 v1.2.8 的 body 为 `🐞 P0 修复`（玻璃右端 3px 灰边 + 右角双弧的根因与四步修法）→
`🔍 为什么会这样`（材质按窗口矩形绘制、CSS 只覆盖内容区的机制说明）→ `✅ 验证` →
`📌 已知局限`（2423 chars / 37 行）。更早的 v1.2.7 为「累积修复版（v1.2.3 ~ v1.2.6 四版合并）」——
三类静默失效（设置页初始化竞态 / 截图遮罩窗抢前台 / OCR 行排序 panic）+ 体积 −62.6%。
更早的 v1.0.0 为两段：「#### ✨ 新功能（本版重点）」滑动截长图 + mac 画中画 + 文字标注可拖动，
「#### 🧹 工程收口」proc::hidden / cfg 收口 / IPC 88→92，末尾「#### 📊 包体积」表。
历史教训：v0.7.5 及以前长期停留在 v0.7.0 的旧文案（"整合 0.6.2 全部 Beta 修复"、
日历月视图、喂食系统修复等），与当版内容完全无关。
**v1.2.8 发版时另发现一处长期偏差**：`releaseBody` 现值说明上一版还写着「v1.0.0 的文案」，
实际早已随 v1.2.0 / v1.2.1 / v1.2.2 / v1.2.7 改写——**说明性文字比代码更容易滞后**，
本节点的核对应与 `grep -c 'releaseBody' build.yml` 一起做，不能只看文档描述。


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
| **v1.2.10** | 2026-10-26 | 🐞 **修复版（macOS 交互体验专项；Windows / Linux 零功能改动）** | **六项修复 + 一项新增，全部落在 macOS 观感与授权体验**。① **浮窗浅色外观白字看不清（P0）**：CSS 全按深色背景定标、**未跟随 `prefers-color-scheme`**，浅色壁纸玻璃底上白字仅 **1.36:1**（AA 正文线 4.5:1）；**降 alpha 无解**（0.48 仍 3.40:1、0.80 也才 3.9，数学上过不了 4.5），**唯一出路是反色**。新增 light 档（白遮罩 + `#1c1c1e` = NSColor.labelColor 浅色值），**只挂 `.has-blur`（仅 macOS）**——Windows 无系统材质跟随 appearance，且反色在 Win11 Bloom `rgb(46,32,78)` 上从 16.78:1 掉到 **6.32:1**（净亏 10.5）并把胶囊变成 `srgb(161,155,175)` 灰白板。配色按 v1.2.8 深色真机实测**反推材质后底下界**（L≈95/71 ÷ 0.52 → 183/137，取 137 保守）扫 130~255 取最差：主数字 10.38 / 标签 5.16 / 网速↓ 4.74 / 网速↑ 4.82 / 截图 5.49，全过 AA（网速色因此加深 `#0a658c`→`#055a70`、`#8a4d08`→`#7a4405`）。② **浮窗高度/圆角两个源**：Rust `set_size` 下发 36/44 与 CSS 硬编码 `height:36px`/`.is-glass{height:44px}` 并存 → 冷启动与开关翻转时胶囊底部露 **8px 系统材质**、材质 radius 22 vs CSS 18 呈纵向双弧；改 `height:100%` + `border-radius:999px` 并删 `.is-glass` 里的 44/22，**高度只剩一处定义**（与 v1.2.8 对宽度的改造同构）。③ **菜单栏图标浅色栏显脏灰**：托盘取彩色 App 图标（alpha=255 占 90.6%），浅色栏 **1.91:1**；改 template 剪影图（纯黑 + alpha 圆环 + 圆心），系统按栏明暗染色。新增 `icons/trayTemplate.png`(22²)/`@2x`(44²)，4x 超采样 + LANCZOS，覆盖率 **25.0%/25.1%**（理论 24.8%，误差 <0.3%）；macOS 分支内嵌 **@2x**（`tray-icon 0.24.2` 的 macOS 实现是单张 `NSImage::initWithData`，无法表达 1x/2x；系统压到固定 18pt，44px 下采样更锐利）。④ **检测不到更新（三独立成因）**：**(a)** 前端只有一个 `setTimeout` → 常驻托盘等于「进程一生只查一次」，改 6h `setInterval`；**(b)** 时间戳在「抓取成功」后就写、**含「已是最新」**，烧掉接下来 24h → 改为「确认远端状态」后才写、节流 24h→6h；**(c)** `reqwest` 关掉 `default-features` **连带丢 `system-proxy`**（`reqwest-0.12.28/Cargo.toml:101-106`），只认 `HTTP_PROXY` 环境变量、**不读系统代理** —— Clash/Surge 默认改的正是系统代理，用户以为「开着代理」而进程直连出境 → 补回该 feature（`cargo tree -e features` 复核 `client-proxy-system` 已启用；代价 4 个纯系统绑定 crate，无新 TLS）。另：三通道抓取（GitHub atom → GitHub REST → **Gitee 镜像**）全失败才算失败；每条通道加 `is_version_tag` 校验（`ocr-runtime` 这类资源型 Release 会让前端弹「发现新版本 ocr-runtime」死链）；`semver_gt` 修正 dismissed 自愈（依赖「相等返回 false」）；错误正文按**字符**截断（`String::truncate` 按字节切非 char 边界会 panic）。⑤ **权限弹窗反复弹**：删启动 3 秒预请求（单信号 `CGPreflight` 判据，已授权者也被再弹），改首次截图由 `ensure_ready` 按需请求、启动只静默查询；`check_permissions` 屏幕录制改**双信号口径**（`CGPreflight` ∨ 窗口标题探针）与截图侧一致，修正「升级后失效」被误判成「从未授权」；横幅 dismiss 标记落盘 localStorage、focus 回调由「accessibility 为真就清标记」改**边沿触发**。⑥ **浮窗材质层只增不减**：`window-vibrancy 0.8.1` 的 `apply` **不去重**（每次新建 `NSVisualEffectView` + `addSubview`）、`clear` 走 `viewWithTag:` **一次只摘一层** → apply N 次 = N 层，关玻璃后剩余层永久留下（「擦不掉的灰板」）；改 apply 前循环清到干净、`!glass` 分支同样循环清，以 `Ok(false)` 收尾（**有界**）。**新增**：设置页**权限中心**（仅 macOS，只读）—— 屏幕录制/辅助功能真实状态 + 手动刷新 + 「授予」（跳系统设置）/「重置并重启」，刻意**不轮询**（未放行时 `check_permissions` 会 fork `codesign`/`xattr`，1Hz 代价过高），含 ad-hoc 签名专项提示（升级后授权失效的物理成因是 cdhash 每次构建都变）。**另**：`tauri.conf.json` 给 float 补 `x/y: 24`（`tao-0.35.3/platform_impl/macos/window.rs:331` 无 position 时兜底 `ns_window.center()`，而 float 340 / main 1040 且 main 自身 center → 首开残影压在主窗口正中）；`height`/`minHeight` **刻意维持 36**（`window.show()` @lib.rs:835 早于 webview `onMounted→fitWidth` @FloatBar.vue:545，conf 写 44 会让非玻璃用户看见一帧 44px；玻璃档 44 已由 setup 的 `set_size(cur_w,44)` 在 show 前落定）。**验证**：`cargo check` 零警告 / `cargo test --lib` **35 passed / 0 failed / 4 ignored**（本版新增 11 单测）/ `vue-tsc --noEmit` 零错误；macOS 代码经 `aarch64-apple-darwin` 交叉探针 + 双向反证（`new_owned`→`new_owned_XXX` E0599、`into_raw()`→`as_raw()` E0308）；CSS 特异性矩阵脚本核验 light 块 **9 条全挂 `.has-blur`**（Windows 零波及）且每条都赢过暗色档（含 2 条同特异性靠源码顺序取胜）；i18n zh/en 各 **444** 叶子键、差异 0（本版净增 14）。**IPC 总数 85**（本版未增减）。⚠️ **macOS 侧未经真机目视复验**（浅色档实际材质亮度 / 模板图观感 / 权限中心文案是否被截断）。**发布落点**：⏳ 待发布后回填（tag / commit / CI run / 两端 Release id 与资产字节 / Gitee 配额）—— 该项**不属于发版提交**，须等 CI 跑完再单独补一个提交，且不得移动 tag。 |
| **v1.2.8** | 2026-10-26 | 🐞 **修复版（macOS 15 悬浮窗玻璃右端 3px + 双弧）** | **macOS 15 悬浮窗玻璃「右端溢 3px 灰边 + 右角双弧」P0 修复**。根因：**两层同尺寸且都带 `border-radius`** —— 外层 `.float-bar`（铺满窗口、画圆角 + 边框）与内层 `.f-content`（又画一次圆角）**曲率中心不重合** → 右端呈双弧；同时外层 `width:100%` 在全局 `box-sizing:border-box` 下把内容区撑得比窗口窄 2px，而 `NSVisualEffectView` 按**窗口矩形**绘制、CSS 只覆盖到内容区 → 右端露出一条约 3px **未被 CSS 覆盖的材质原色带**（浅色壁纸上是灰边，深色上不易察觉）。修法：① **双层职责分离** —— 外层只管铺满 + 居中（`width:100%` / `justify-content:center` / 不参与测量），内层只管真实尺寸（`width:max-content` / `flex:none` / 唯一被 `ResizeObserver` 测量的元素），**圆角与边框只保留外层一次**；② `WIDTH_SLACK = 2` 语义明确为「2 × 外层边框宽度」，窗口宽 = 内层实测宽 + 2，右端缝隙消失；③ 材质换档 `HudWindow`(13) → **`UnderWindowBackground`(21)** —— 前者本身偏灰、暗色遮罩重，浅色壁纸上呈「灰板」而非玻璃；④ 新增 `native_blur: bool` 做**配色分档**：原生模糊可用（Liquid / Vibrancy）时压低 CSS 背景不透明度、对比度交给系统材质，不可用（Acrylic / None）时由 CSS 全权承担（沿用 v1.2.2 结论：Windows 不调系统材质）；⑤ `translucency` 随 `FloatGlassSupport` 下发（Liquid 0.85 / Vibrancy 0.55 / 其余 0.0），**本版前端未消费**，留待 macOS 26 真机对比度数据到位后再接。IPC 总数 **85**（本版未增减；⚠️ 此前 `CONVENTIONS` 写的 84 是**错的**，已按 `generate_handler!` 实测校正）。**验证**：`cargo check` 零警告 / `vue-tsc --noEmit` 零错误；macOS 分支在本机（Windows）无法直接编译，另建 **`float-glass-probe` 交叉编译探针**（`cargo check --target aarch64-apple-darwin` + `#[path]` 引真实源文件）编译通过，并用两次**注入错误反向验证**确认探针确实在编译 macOS 分支而非跳过（`UnderWindowBackground`→`...XXX` 报 E0599；`Some(FLOAT_RADIUS_GLASS)`→`Some("22")` 报 E0308）。⚠️ **macOS 15 真机观感未复验**：右端灰边是否消失 / 玻璃通透度 / 13px 文字在浅色壁纸上的对比度（≥4.5:1），须在 Mac 上目视确认。**发布落点**：tag / commit `bd58503`（轻量 tag `v1.2.8`）；CI run `40266414622` 三平台全 success；GitHub Release `423843748` **6 资产 / 195.90 MiB**（AppImage 99.15 / exe 21.70 / rpm 18.92 / dmg 18.91 / deb 18.91 / app.tar.gz 18.30 MiB）；Gitee Release `1241841` **5 附件 / 96.75 MiB**（+2 自动源码包；AppImage 99.15 MiB 超 Gitee 单文件 100 MB 上限，仅 GitHub 提供）；Gitee 按「只留最新一版 + ocr-runtime」约定已删旧版 v1.2.7（id 1232618，释放 96.74 MiB），现存 `ocr-runtime` + `v1.2.8`，配额 **166.24 / 1024 MiB（16.2%）**。5 个下载链接 HEAD 复核 HTTP 200 且 `Content-Length` 与本地字节逐项相符。 |
| **v1.2.7** | 2026-10-23 | 🐞 **累积修复版（v1.2.3 ~ v1.2.6 四版合并发布）** | **本版跳过 v1.2.3 ~ v1.2.6 四个中间版本号**（远端上一版为 v1.2.2），全部改动一次交付。**三类「静默失效」**：① **设置页初始化竞态（丢更新 + 反向数据损坏，三处同源）** —— `ref.value = await load()` 整体替换冲掉用户改动；提交时又发整个本地对象（初值全是默认值），把后端本是 `true` 的开关一并写成 `false` 并持久化（实测只点「显示网速」→ 另 4 个字段被静默关闭写库）。统一改 **「权威基线 + 用户差量」**（`baseline` + `dirty: Set<keyof T>`），覆盖状态栏（2 写入点 + 托盘推送）、截图配置（4 写入点 + 2 个 v-model 数值项）、通用设置表单、自动备份开关。② **截图遮罩窗抢前台致采集静默停止（P0）** —— `visible:false` + `focus:true` 的隐藏窗创建即成为前台且不让出，`GetForegroundWindow()` 恒返回它、采样每 tick 命中白名单跳过（修复前 6 分钟跳过 **833 次**、采集 **0 次**；修复后 40 秒跳过 **0 次**、采集 **32 次**）。修复：`focus` 改 `false` + 采集侧「前台不可见即跳过」兜底 + 收尾主动让出焦点。③ **OCR 行排序违反全序 panic** —— 成对阈值 `(a.h+b.h)/2×0.6` 致 A≈B、B≈C 却 A<C，改三段式（y 中心排 → 全局常量阈值归并行号 → 按 (行号,x) 排）。**其他修复**：液态玻璃浮窗中浅色壁纸对比度（2.69:1 → 逐像素定标压暗 0.48，四项全过 WCAG AA）；macOS 设置面板图标彩色噪点（`icp4/icp5`+PNG → `ic04/ic05`+ARGB，与 `iconutil` 字节级等价）；桌宠皮肤选择被静默改回默认（注册期误校验 `activeId`，非首张皮肤必丢）。**优化**：安装包 **23.31MB → 8.72MB（−62.6%）**（补 `[profile.release]`：lto/strip/opt-level=s/panic=abort）+ 桌宠图 PNG→WebP **5.30MB → 0.64MB（−88%，alpha MAE=0.000）**；WAL 稳态 **4.13MB → 1.05MB（−75%）**；写库失败不再静默丢弃；日志噪音治理（9376 条占 98% 的常态 WARN 降噪）；停写 `daily_summaries` 死表。**新增**：启动 15s 后自动检查更新 + 首页可关闭提示横条（24h 后端节流、失败全静默、同版本只提示一次；刻意**不做应用内自动升级** —— updater 需长期密钥托管，丢失即全体用户只能重装）。IPC 总数 **84**（本批未增减）。**验证**：`verify_v127_init_race.py` 18/18、`verify_v127_residual_race.py` 6/6、`verify_capture_no_focus.py` 4/4、`human_regression.py` 24/25（唯一失败 = QuickAssist 占前台锁的环境干扰，非产品缺陷）。**发布落点**：tag / commit `e704b2d`（轻量 tag `v1.2.7`）；CI run `39906361489` 三平台全 success；GitHub Release `421483547` **6 资产 / 195.89 MiB**（AppImage 99.15 / exe 21.69 / app.tar.gz 18.30 / deb 18.91 / dmg 18.91 / rpm 18.91 MiB）；Gitee Release `1232618` **7 附件**（exe 21.69 / dmg 18.91 / app.tar.gz 18.30 / deb 18.91 / rpm 18.91 MiB，合计 96.74 MiB + 2 自动源码包；AppImage 99.15 MiB 超 Gitee 单文件 100 MB 上限，仅 GitHub 提供）；Gitee 按「只留最新两版 + ocr-runtime」约定已删旧版 v1.2.1（id 1221109）与 v1.2.2（id 1225950），现存 ocr-runtime + v1.2.7，配额 166.23 / 1024 MiB（16.2%） |
| **v1.2.2** | 2026-10-21 | ✨+🐞 **液态玻璃 + 五项修复 + 三项功能精简** | **悬浮窗液态玻璃（新功能）**：浮窗玻璃开关打开后从 36px/圆角18 放大为 **44px/圆角22 胶囊形** + 高透光材质。macOS 26+ 走 `NSGlassEffectView`（半透明+背景模糊+**真折射**），< 26 回落 `NSVisualEffectView`；**Windows 明确不调任何系统材质**——真机像素实测 Acrylic/DWM backdrop 对无边框窗口渲染成**不透明色块**（背景亮度 255 与 70 时浮窗恒 212，透光率 **0.001**），反而不如纯 CSS（**0.850**），故停用系统材质、视觉 100% 交给 CSS（实测玻璃 0.800 vs 普通 0.207 = 3.86 倍）。**修复四项**：① **macOS 截图遮罩缺菜单栏行 / 「全屏变画中画」（P0）**——tao 的 `always_on_top` 只映射到 `NSFloatingWindowLevel`(3)，低于菜单栏 24 级，AppKit 遂把 frame 约束进 `visibleFrame`（扣掉菜单栏那条），一条规则产生两个症状；新增 `screenshot/capture_window.rs` 把 level 提到 `NSScreenSaverWindowLevel`(1000)，`show()` 前调用、`hide` 时恢复（非 macOS 空实现）。② **Windows 液态玻璃矩形边框阴影**——浮窗窗口 `width:max-content` + 高度恰好 44px，窗口边界**紧贴胶囊**，外扩 `box-shadow` 被窗口矩形裁掉只剩四角暗块，改为只用 inset 内高光。③ **macOS 状态灯仍恒橙**——v1.2.1 的内存门槛 0.8 用 `>=` 判断，而 macOS 内存日常基线即 70~80%，门槛与常态线重合成了死阈值；改为 CPU 独立决定基础档位、内存只做高水位抬升。④ **macOS Dock 图标偏大**——`icon.icns` 图形 100% 铺满画布，Big Sur 起规范要求约 80.5%，已按规范重绘。**功能精简（用户要求）三项**：移除「重复上次截图」整链（设置页入口 + IPC `screenshot_repeat_region` + `last_region` 状态 + `screenshot_commit` 的 `region` 入参 + `save_cropped`，**截图 IPC 22→21**）、移除设置页「立即截图」按钮（触发入口保留浮窗与快捷键）、**截图投影功能整体下线**（`ScreenshotConfig.shadow` 配置键 8→7、`default_shadow`、工具栏开关、canvas 投影绘制与四周留白全删，导出图恒无投影）。**IPC 总数 82**（+`float_glass_support` −`screenshot_repeat_region`）。**验证**：`cargo check` 零警告、`vue-tsc --noEmit` 零错误、IPC 实测复核 82、i18n zh/en 各 425 键对齐。**发布落点**：tag / commit `0a0cb4d`（轻量 tag `v1.2.2`）；CI run `39574377148`（#71）三平台全 success；GitHub Release `419351846` **6 资产 / 241.6 MiB**（AppImage 107.59 / rpm 27.30 / deb 27.29 / exe 27.22 / dmg 26.56 / app.tar.gz 25.62 MiB）；Gitee Release `1225950` **5 附件 / 134.0 MiB**（+2 自动源码包，AppImage 仅 GitHub）；Gitee 已同步（按「只留最新两版」约定已删旧版 v1.2.0 / id 1214841，现存 ocr-runtime + v1.2.1 + v1.2.2） |
| **v1.2.1** | 2026-10-20 | 🐞 **修复版（状态栏离屏 + 自家浮窗被录制 + macOS 灯不变色）** | **三处跨平台实机修复**（前两处 Windows，第三处 macOS）。① **状态栏开了却看不到（P0）**：浮窗 localStorage `float_pos` 保存的是副屏还在时的负坐标（历史 `x=-643/-900/-2157`），副屏移除/布局变化/分辨率缩小后被**无校验恢复**，窗口整个落到屏幕外（实测单屏 1536×864 机器上物理坐标 `(-695,159)`），用户无抓手拖回。修复：`float_window.rs` 新增 `clamp_to_visible_area`，在 show/move/resize 三路径后用**物理像素**枚举显示器，窗口与任一屏水平重叠不足自身宽 1/3 即回落主屏右上角（右/上各留 24px）；物理像素判定规避多屏混合 DPI 歧义，屏内不动用户位置，已本机移回验证显示正常。② **自家浮窗被录制成应用**：顶部「记录中 screentime-pro · ScreenTime Float」。辅助窗口与主窗口同进程同 exe，在其上按下（浮窗 startDragging）会短暂成为前台，旧实现无差别采集入库（macOS 浮窗不夺 frontmost 故无此问题）。修复：`tracker/windows.rs` 新增 `SELF_HELPER_TITLES` 标题过滤，命中即 NoForeground 跳过、不建会话；主窗口不过滤。③ **macOS 状态灯恒定橙色不变色**：v1.2.0 负载取 `peak=max(cpu,mem)`，macOS 内存长期 70~80%（wired/compressor/文件缓存，系统常态）把峰值钉在橙档、CPU 回落不变绿。修复：`float/FloatBar.vue` loadLevel 改为 CPU 瞬时主信号，内存 `<80%`（`MEM_WARN`）不参与、`≥80%` 才抬升档位（80~85% 深橙、≥85% 红），五档阈值不变。IPC 总数不变（**82**）。**发布落点**：tag / commit `15114da`（annotated）；CI run `39366037767` 三平台全 success；GitHub Release `417979506` **6 资产 / 242.5 MiB**（rpm 27.31 / dmg 27.16 / app.tar.gz 25.87 / deb 27.30 / exe 27.22 / AppImage 107.60 MiB）；Gitee Release `1221109` **5 附件 / 134.9 MiB**（+2 自动源码包，AppImage 仅 GitHub）；发版后按约定删 Gitee 旧版 v1.1.1（id 1212859），现存 ocr-runtime + v1.2.0 + v1.2.1 |
| **v1.2.0** | 2026-10-18 | ✨ **功能增强版（多屏截图 + 浮窗负载状态灯）** | **多屏截图「鼠标在哪截哪屏」+ 浮窗负载状态灯 + 代码体检清理**。① **多屏截图（微信式 Alt+A）**：截图从「固定主屏」改为「抓光标停靠的那块屏」，新增 `screenshot/cursor.rs`（平台原生坐标的全局光标位置：Windows `GetCursorPos` / macOS `CGEventGetLocation` / Linux X11 `query_pointer`），xcap 用 `Monitor::from_point`、macOS SCK 用 `SCDisplay::frame().contains` 选屏，均回落主屏/首屏；顺带修复多屏异 DPI 下抓副屏拿错 scale 的遮罩窗错位隐患。② **浮窗负载状态灯**：最左侧状态点由固定 accent 橙改为按系统负载动态变色（负载取 CPU/内存较高者，5 档 + unknown 灰，阈值 0.15/0.35/0.65/0.85，0.35s 平滑过渡，tooltip 显示档位 + 实时百分比）。③ **代码体检清理（凌霄渡体检后续）**：删 3 处死代码（孤儿组件 `PetBody.vue`、`system_load/linux.rs` 占位函数 + `procfs` 依赖、`float_window.rs` 两个死 IPC 命令）；消除 33 处 `Mutex::lock().unwrap()` 违规；清理 `error.rs` 半死代码；浮窗移除「重复上次区域」按钮。IPC 84 → **82**。**发布落点**：tag / commit `4ad2ca3`；CI run `39096674417` 三平台全 success；GitHub Release `416228297` **6 资产 / 242.4 MiB**（rpm 27.30 / dmg 27.17 / app.tar.gz 25.88 / deb 27.30 / exe 27.22 / AppImage 107.59 MiB）；Gitee Release `1214841` **4 附件 / 108.7 MiB**（AppImage 超单文件 100 MB 上限，仅 GitHub 提供）；发版后按约定删 Gitee 旧版 v1.0.0（id 1173888），现存 ocr-runtime + v1.1.1 + v1.2.0 |
| **v1.1.1** | 2026-10-17 | 🐞 **修复版（macOS 出图尺寸 + 移除长图）** | **修复 macOS 截图「小窗口快照」（P0）+ 移除滑动截长图**。① **macOS 出图尺寸（P0）**：`screenshot/macos_sck.rs` 构建 `SCStreamConfiguration` 时未设宽高，Apple 默认 1920×1080，Retina 屏（真实 3456×2234）冻结帧恒为 1920×1080、遮罩窗只盖左上角；现按「`contentRect`（点）× `point_pixel_scale`」显式设 `with_width/with_height`（向上取整），抓后做 ±2px 尺寸自检，超差回落 xcap。② **多显示器原点透传**：遮罩窗坐标由写死 `0,0` 改为透传显示器真实原点（副屏在主屏左侧/上方时原点为负）。③ **几何口径统一物理像素**：原先混用「逻辑点/物理像素」致高分屏冻结帧对不齐。④ **移除滑动截长图**（v1.0.0 引入）：遮罩窗是置顶窗，本身即滚动目标屏幕的遮挡物，用户无法滚动下方页面，架构内不可达；删除 `screenshot/long.rs` 全量（-1084 行）。⑤ **工程收口**：`console→log` 统一日志通道、规则新增 `enabled` 补传、「今天」按钮单日刷新、5 个死命令清理；IPC 95→**84**（移除长图 4 + 清理 commands 7）。**发布落点**：tag / commit `ac8a172`；CI run `39021234836` 三平台全 success；GitHub Release `415702872` **6 资产 / 254.4 MiB**（exe 28.55 / dmg 28.49 / app.tar.gz 27.14 / deb 28.63 / rpm 28.63 / AppImage 112.82 MiB）；Gitee Release `1212859` **4 附件 / 113.9 MiB**（AppImage 超单文件 100 MB 上限，仅 GitHub 提供）；发版后按约定删 Gitee 旧版 v0.9.6（id 1159180），现存 ocr-runtime + v1.0.0 + v1.1.1 |
| **v1.0.0** | 2026-09-29 | 🎉 **首个正式版（滑动截长图）** | **新增滑动截长图 + 修复 macOS 截图画中画（Retina 几何）+ 文字标注敲定后可再拖动**（v0.9.7 后 5 个提交）。① **滑动截长图**：截图工具条新增「长图」按钮——框选后进入长图面板，手动滚动页面逐屏续捕，Rust 侧重叠检测自动拼接（`screenshot/long.rs`：探针行=新图顶 1/8、底图底部 600px 带内滑动、列采样 8px、通道容差 12、取最低匹配=最短滚动；公式 `skip_in_new = base.height − (match_y − probe_y)`），四件套 IPC `screenshot_long_start/capture/finish/cancel`（88→**92**）；**首屏必须用冻结帧**（遮罩亮着抓屏会拍到遮罩自身），后续帧先藏遮罩 180ms 再抓；完成走 `screenshot_commit` 复用剪贴板/存盘/DB/历史全链路；前端 Enter=续捕、Shift+Enter=完成、Esc=退出。② **macOS 截图画中画**：Retina 屏逻辑像素↔物理像素 2x 混用导致框选几何错位，统一换算后落帧（commit `d7ee13a`）。③ **文字标注**：双击敲定后可继续拖动调整位置。④ **工程收口**：子进程统一 `proc::hidden`（裸 Command::new 清零）；跨平台 cfg 收口到`system_load/platform` 与 `screenshot/platform`。**发布落点**：tag / commit `c703284`；CI run `36530432030` 三平台全 success；GitHub Release `398870240` **6 资产 / 241.1 MiB**（exe 27.27 / dmg 27.22 / app.tar.gz 25.93 / deb 27.26 / rpm 27.26 / AppImage 107.56 MiB）；Gitee Release `1173888` **5 附件 / 134.9 MiB**（AppImage 112.8MB 超单文件上限，仅 GitHub 提供）；发版后按约定删 Gitee 旧版 v0.9.4（id 1157170），现存 ocr-runtime + v0.9.6 + v1.0.0 |
| **v0.9.7** | 2026-09-22 | 🐞 **四缺陷修复版** | **修复 Windows 应用名重复 + macOS「Item-0」+ 截图重入 + 截图历史单选失效**（四项均为用户实测反馈，2026-09-22 修复）。① **Windows 录制应用名重复**：列表显示「ScreenTime Pro · ScreenTime Pro」——根因 `tracker/windows.rs` 把 `name` 填成窗口标题，而 UI 渲染「name · 窗口标题」，两者必然相同；现 `name` 改用进程名（去 `.exe` 后缀），窗口标题匹配职责由 `window_title` 字段独立承担（分类器匹配 5 字段含 process_name，规则不受影响）。② **macOS 状态栏「Item-0」**：前台应用带 NSStatusItem 时，系统给状态栏项生成的占位标题「Item-0」被当成窗口标题记进库；现 `get_foreground_window_title` 加 `kCGWindowLayer==0` 过滤（与 `probe_foreign_titles` 口径一致）。③ **截图重入工具条入镜**：遮罩亮着时再按快捷键，旧冻结帧的工具条被截进新图；现 `begin_capture` 加 `CAPTURE_IN_FLIGHT` RAII 重入闸门（覆盖启动阶段）+ `SELF_OVERLAY_LABELS` 纳入 `capture` 窗口（覆盖框选期）。④ **截图历史单选失效**：`.shot-check` 是 `position:absolute` 但 `.shot-item` 漏 `position:relative`，勾选框全部脱锚到页面左上角叠成一团点不中；补锚 + 整卡可点。**发布落点**：见 git log（本版按用户要求仅产出 macOS 安装包，Gitee 同步与 Windows 安装暂缓） |
| **v0.9.6** | 2026-09-22 | 🐞 **弹窗交互修复版** | **修复设置页弹窗「确定」关不掉（P1）+「检查更新」标题缺版本号**。根因一：v0.9.4 为 macOS 权限弹窗加「重启应用」按钮时用 footer slot **覆盖**了 Modal 自带按钮，自定义「确定」只执行确认回调、**丢失了 Modal 自带按钮的 close() 行为**——v0.9.4 起设置页所有弹窗点「确定」均无反应（只能右上角 X / 点遮罩 / Esc 关闭）。修复（仅改 `src/views/Settings.vue`）：`onAlertConfirm` 补回 `alertOpen=false`，且**先关弹窗再跑回调**（回调里常再弹「已清理」等结果弹窗，顺序反了会被误关）。根因二：「检查更新」弹窗标题 `t("settings.upToDate")` **漏传 `{current}` 插值参数**，标题渲染成「已是最新版本（v）」；补传后正确显示「已是最新版本（v0.9.6）」。本版应用功能与 v0.9.5 完全一致。**发布落点**：tag / commit `c2bff2e`（轻量 tag `v0.9.6`）；CI run `35682125178` 三平台全 success（9m20s）；GitHub Release `393430375`（**6 资产 / 241.5 MiB**：exe 28.51 / dmg 28.42 / app.tar.gz 27.07 / deb 28.47 / rpm 28.47 MB / AppImage 112.66 MB）；Gitee Release `1159180`（**5 附件 / 134.4 MiB**，AppImage 超 Gitee 单文件 100 MB 上限仅 GitHub 提供）；本机已静默安装并验证（注册表 / exe 版本串 / 启动日志均 0.9.6）；Gitee 旧版 v0.9.3（id 1156896）已按「只留最新两版」约定删除 |
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

---

## 附：macOS 签名与公证改造清单（未实施）

> 本节是**待采购**事项的落地清单，不是当前流程的一部分。放在文档末尾，`§1 ~ §9` 的既有流程完全不变。
> 来源：`MACOS-UX-REVIEW-2026-10-26.md` §1.5（P2 根治项）。

### A.1 现状与后果

| 项 | 现值 | 位置 |
|---|---|---|
| `bundle.macOS` | 只有 `{"minimumSystemVersion": "12.0"}` | `src-tauri/tauri.conf.json:121-123` |
| `signingIdentity` | **未设置** | 同上 |
| `hardenedRuntime` | **未设置**（Tauri 默认 `false`） | 同上 |
| `entitlements` | **无文件** | 同上 |
| `build.yml` 的 `APPLE_*` | **零命中** | `.github/workflows/build.yml` |

**签名实际状态：ad-hoc（`codesign -s -`）**。Tauri 在没有 `signingIdentity` 时对 macOS 产物做 ad-hoc 签名，只保证「二进制可执行」，不保证「身份稳定」。

**这一条是所有「权限反复」问题的物理成因**：

1. ad-hoc 签名的 **cdhash 由产物字节算出**，每次构建字节都不同 → cdhash 每次都变；
2. macOS 的 TCC（授权数据库）按「bundle 路径 + 签名指纹」记账，**指纹变了系统就当成了一个新应用**；
3. 于是升级后：系统设置里那两条开关**仍然显示为「开」**（那是上一个版本的记录），但新进程实际拿不到授权；
4. 用户看到的是「升级完就要重新授权一次」，而且重新勾选开关也常常无效（旧记录还在，清不掉），只能靠 `tccutil reset`。

→ 短期只能靠「重置并重启」按钮（v0.9.3 起）+ 权限中心卡片（v1.2.10 起）让用户自救；**根治必须换稳定签名**。

### A.2 需要采购什么

| 项 | 说明 | 费用 |
|---|---|---|
| **Apple Developer Program**（个人 / 组织） | 提供 **Developer ID Application** 证书，用于对 `.app` / `.dmg` 做稳定的发行版签名 | **US$99 / 年** |

拿到证书后需在构建机上建立 `Developer ID Application: <姓名或公司> (<TEAMID>)` 身份，并导出为 `.p12`（CI 用）。

### A.3 需要改的文件与字段

#### ① `src-tauri/tauri.conf.json` → `bundle.macOS`

```jsonc
"macOS": {
  "minimumSystemVersion": "12.0",
  // 证书的 Common Name（不含 "Developer ID Application:" 前缀也可，
  // 但写全名更稳 —— 构建机上可能同时装着多个身份）
  "signingIdentity": "Developer ID Application: Your Name (TEAMID)",
  // 公证流程的硬性前提：开启加固运行时。Tauri 默认 false，必须显式打开。
  "hardenedRuntime": true
  // 不写 entitlements = 不注入 entitlements 文件，走 hardened runtime 默认基线。
  // 当前 A.4 结论是「一项都不需要」，所以这里**保持不写**。
  // 只有当 A.4 的例外条款被触发（将来引入 CGEventTap）时，才补：
  //   "entitlements": "entitlements.plist"
  // 注意：写 "entitlements": null 与「不写这个键」语义等价，但前者容易让人
  // 以为「必须配一个文件、只是暂时置空」，实践中已多次被误读 → 一律不写。
}
```

#### ② `.github/workflows/build.yml`

在 macOS 的构建步骤前注入环境变量（Tauri CLI 会自动读取，无需改命令）：

| 变量 | 内容 |
|---|---|
| `APPLE_CERTIFICATE` | `.p12` 的 **base64**（`base64 -i cert.p12 \| pbcopy`），存进 GitHub Secrets |
| `APPLE_CERTIFICATE_PASSWORD` | 导出 `.p12` 时设的密码 |
| `APPLE_SIGNING_IDENTITY` | 与 ① 的 `signingIdentity` 同值（`Developer ID Application: ...`） |
| `APPLE_ID` | Apple ID 邮箱（公证提交账号） |
| `APPLE_PASSWORD` | **App 专用密码**（appleid.apple.com 生成，不是账号登录密码） |
| `APPLE_TEAM_ID` | 10 位 Team ID（developer.apple.com → Membership 可见） |

> ⚠️ 这 6 个值**全部走 GitHub Secrets**，不得写进仓库。`build.yml` 里只写 `${{ secrets.XXX }}`。
> ⚠️ 三份 `releaseBody`（§3.1）与本改造无关，**不要**顺手改。

### A.4 entitlements 需要哪些

**结论：当前代码一行 entitlement 都不需要。**

| 功能 | 是否需要 entitlement | 依据 |
|---|---|---|
| 屏幕录制（截图 / 窗口标题） | **不需要** | TCC 授权由系统弹框授予，与 entitlements 无关 |
| 辅助功能（空闲判断） | **不需要** | 全仓 `AXUIElement` / `AXObserver` / `CGEventTapCreate` **零命中**；空闲检测用 `CGEventSourceSecondsSinceLastEventType`，该 API 不受 TCC 管 |
| 网络（检查更新） | 不需要 | 非沙盒应用无网络 entitlement |
| 读写用户目录 | 不需要 | 非沙盒应用 |

**唯一的例外（未来）**：若将来要做全局鼠标/键盘监听（`CGEventTapCreate`），则需要
`com.apple.security.accessibility`。当前**没有**这条代码路径，不要提前加 —— 加了会让用户在
系统设置里看到并不需要的权限项。

→ 因此 `tauri.conf.json` 的 `bundle.macOS` **不写 `entitlements` 键**（不写 = 不注入），
只靠 `hardenedRuntime: true` 的默认基线。若日后触发上面的例外，再新建 `entitlements.plist`
并把键指向它 —— 届时也必须同步更新本节的结论表。

### A.5 验证方法

签名完成后，在**构建产物所在的 Mac 上**依次执行：

```bash
# ① 签名链完整且是 Developer ID（输出里应有 "Authority: Developer ID Application: ..."）
codesign -dv --verbose=4 "ScreenTime Pro.app"

# ② 加固运行时已开（输出里应有 "flags=0x10000(runtime)"）
codesign -d --verbose=2 "ScreenTime Pro.app" 2>&1 | grep flags

# ③ 公证票据已 stapled（spctl 应返回 "accepted / source=Notarized Developer ID"）
spctl -a -vv "ScreenTime Pro.app"

# ④ 票据能被 stapler 找到（输出 "The validate action worked!"）
xcrun stapler validate "ScreenTime Pro.app"

# ⑤ DMG 也要单独验（用户双击的是 DMG；文件名按实际产物替换）
spctl -a -vv -t open --context context:primary-signature "ScreenTime.Pro_<version>_aarch64.dmg"
```

**CI 里公证提交的形态**（Tauri 在检测到 `APPLE_ID` + `APPLE_PASSWORD` + `APPLE_TEAM_ID` 时会自动调用 `notarytool`）：

```bash
xcrun notarytool submit "ScreenTime Pro.dmg" \
  --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" \
  --wait          # 人工核对时去掉 --wait 拿 submission id 后查
xcrun notarytool log <submission-id> --apple-id ... # 失败时看具体拒绝原因
```

**最终验收标准（用户视角）**：

1. 双击 DMG 里的 `.app`，**不再**出现「Apple 无法验证此 App 是否包含恶意软件」；
2. 升级 v1.2.10 → v1.2.11 后，系统设置里的屏幕录制开关**保持有效**（不需要重新授权）；
3. `README.md` 里那段「必须执行 `修复门禁.command` / `xattr -dr com.apple.quarantine`」可以删除（连同 DMG 里的脚本，但脚本删除属于发版物料改动，另开一项）。

### A.6 风险与回滚

| 风险 | 影响 | 处置 |
|---|---|---|
| 证书泄露 | 他人可冒名签名本应用 | 证书只存 GitHub Secrets；`.p12` 不落本地仓库；泄露后立即在 developer.apple.com **Revoke** 并换新证书重发一版 |
| 公证超时 / 卡在 `In Progress` | CI 卡住不发版 | `notarytool` 单次通常数分钟；CI 里给公证步骤加 30 分钟超时，超时判失败并**不发版**（不要发未公证的包） |
| 加固运行时导致运行时崩溃 | 某些 JIT / 动态库加载被拦 | 本项目无 JIT；若真崩溃，先把 `hardenedRuntime: false` **临时**回滚（代价是公证失效、退回 ad-hoc 现状），并在日志里定位被拦的库 |
| 首次公证后在**旧 macOS（12.0）** 上行为差异 | 老系统对 hardened runtime 更严格 | `minimumSystemVersion` 已是 12.0；发版前在 12.x 真机跑一次截图 + 取字 + 权限流 |
| 用户仍从**旧版本**升级 | 旧 ad-hoc 版本的 TCC 脏记录还在 | 首次带签名的版本仍需一次「重置并重启」（`tccutil reset ScreenCapture com.screentime.pro`），之后不再复发 —— Release 正文要写明「本版是最后一次需要重新授权」 |

**回滚**：把 `tauri.conf.json` 的 `signingIdentity` / `hardenedRuntime` 两行删掉、`build.yml` 的 6 个 `APPLE_*` 环境变量注释掉，即回到当前 ad-hoc 状态。无需改任何业务代码 —— 签名是纯构建期配置。

### A.7 改造后的发版流程增量

在 §4「发布步骤」之外，只多两步（其余不变）：

1. **发版前**：确认 `APPLE_*` 六个 Secrets 都在（`gh secret list`）；
2. **CI 通过后**：额外核对 A.5 的 ①③④ 三条输出，任一不满足则**视为构建失败**，按 §8 回滚（删 tag + Release，升 patch 重发）。
