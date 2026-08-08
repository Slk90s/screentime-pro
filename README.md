# ScreenTime Pro

> 跨平台电脑应用使用时长追踪工具（macOS / Windows / Linux），对标 iOS「屏幕使用时间」。
> 数据 **100% 本地存储、零上传**，隐私优先。

[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](https://github.com/)
[![License](https://img.shields.io/badge/license-MIT-green)](./LICENSE)
[![Version](https://img.shields.io/badge/version-0.7.1-blue)](./release)

---

## ✨ 功能特性

- **实时前台追踪**：后台持续采样当前使用的应用、窗口标题与使用时长。
- **设备总览**：当日总使用时长、解锁/离开分布一目了然。
- **按天柱状图**：iOS 风格的每日分类堆叠柱，点选某天即可下钻当天详情。
- **24 小时 × 分类堆叠**：看清一天里各类应用（社交 / 效率 / 开发 / 娱乐…）的时间分布。
- **App 使用时长排行**：当天哪些应用最耗时，按分类着色。
- **周 / 月同比**：对比本周与上周、本月与上月的使用变化。
- **分类规则引擎**：按进程名 / 窗口标题 / 路径等自动归类，规则可增删改、一键重算历史。
- **多设备合并**：不同电脑的数据按「时间 + 应用 + 设备」去重后合并查看。
- **系统托盘常驻**：关闭窗口不退出，后台继续采样；支持开机自启。
- **数据导出 / 导入**：JSON 全量备份与多设备合并。
- **桌宠（🧪 Beta 测试版功能）**：透明置顶、可自由拖拽的 3D 潮玩熊猫——随当前前台应用自动切换表情状态，点击可触发跳跃 / 压扁 / 抖动互动，随机弹出气泡对话，并具备眼神漂移 + 眨眼、嘴部张合等微表情。⚠️ 该功能仍处于 Beta 测试阶段（可能不稳定），可在「设置」页随时开关。

---

## 📦 下载 / Release

### ⬇️ 官方下载地址

> **👉 [https://github.com/Slk90s/screentime-pro/releases](https://github.com/Slk90s/screentime-pro/releases)**

每个版本都附带 **完整的 Release Notes**（修了什么 / 已知问题 / 与上版对比），请下载前看一眼。

> 📜 **完整发布规范**：见 [`docs/RELEASE.md`](docs/RELEASE.md)（含版本历史、SemVer 策略、NOTES 模板、一键发布脚本）

### 📋 版本历史

| 版本 | 发布时间 | 状态 | 关键说明 |
|------|----------|------|----------|
| **v0.7.1** | 2026-08-08 | 🚀 **正式版（重新发布）** | **v0.7.0 的修复重发**：补齐此前缺失入库的桌宠皮肤源码资产（spider-man 皮肤目录、bubble-phrases.json、popmart3d PNG 资源），修复因这些文件未纳入版本管理导致 CI 三端构建在类型检查阶段失败、GitHub Release 未生成、安装包未上传的问题；应用功能与 v0.7.0 完全一致 |
| **v0.7.0** | 2026-08-08 | 旧版 | **整合 0.6.2 全部 Beta 修复 + 新功能**：① 日历新增「月视图切换 + 本月统计概况」（月总时长 / 活跃天数 / 日均 / 最常使用 App / 最忙的一天），点击「今天 / 回到今天」日历同步跳到当月；② 主界面桌宠设置移除「已经吃饱啦 → 喂食」冗余提示，避免误导；③ 修复桌宠喂食系统——喂食后无反馈 → 右键菜单保留「+N」吐司 + 桌宠进食点头动画 + 飘「好吃！」气泡；饿度值不变 → `feed()` 同步 `todayFeedCountRef` 并新增饱食度随时间自然衰减（每 2 分钟 −1）；④ 优化桌面抖动：Rust 端系统过载判定阈值上调（≥90% 持续 20s 才升温、<55% 持续 15s 冷却），避免频繁翻转触发 0.18s 剧烈抖动；⑤ 算法梳理：饿度默认 100（0–100，<30 为饥饿），心情由前台应用推断（`inferStateFromApp`），加热态强制 angry；⑥ 蜘蛛侠待机小动作（swing/pose/crouch/web）、姿势图去底、气泡 MCU 台词等 0.6.2-33~36 全部修复并入本版 |
| **v0.6.2-36** | 2026-08-08 | 旧版 | 蜘蛛侠体验细化：① 表情编辑器新增「待机小动作」预览区，可单独预览荡丝 / 摆 pose / 蹲防 / 射蛛丝四个姿势，动作覆盖更全面；② 吊蛛丝（swing）时隐藏叠加的静态 SVG 绳索，避免“白色蛛丝一直没动”的穿帮，改由姿势图自带的绳索随摆动；③ 清理射蛛丝姿势图里的静态蛛网碎片；④ 蜘蛛侠气泡台词大量替换为漫威电影宇宙经典台词（能力越大责任越大、友好的邻居蜘蛛侠、斯塔克先生我感觉不太好、我不想走、皇后区小子、爱你 3000 遍等） |
| **v0.6.2-35** | 2026-08-08 | 旧版 | 桌宠渲染与同步修复：① 修复设置页切皮肤后右键菜单高亮仍不同步的问题——PetContextMenu 的 activeSkinId 改为 computed 直接跟踪 skinRegistry，并在 PetMenuWindow 显式触发皮肤注册；② 修复 Spider-Man 动作触发时底层 idle 呼吸帧未隐藏导致的重影/叠加；③ 优化射蛛丝效果：点击动效/表情编辑器预览现在真正切换到 poseWeb 姿势图并隐藏 idle 帧，蛛丝起点重新按手掌位置校准，不再只是叠加图层 |
| v0.6.2-34 | 2026-08-08 | 旧版 | 桌宠体验修复与配置扩展：① Spider-Man 姿势图去底——将 AI 生成的 RGB 姿势图处理为 RGBA 真透明，移除运行时在透明桌宠窗上露出的棋盘格白底；② 修复设置页切皮肤后右键菜单高亮不同步（pet-menu 窗口也监听 pet-skin-changed 并重读 localStorage 对齐）；③ 优化射蛛丝动作：丝线起点对准手掌、动画时长缩短到 0.65s、投掷预备/甩出更有力、蛛网团弹出幅度加大；④ 新增按皮肤独立的气泡短语配置 `src/pet/config/bubble-phrases.json`，popmart-3d / spiderman 各有专属文案，并支持 localStorage 运行时自定义覆盖 |
| v0.6.2-33 | 2026-08-07 | 旧版 | 桌宠动画与同步修复：① Spider-Man 升级"多动作"桌宠——待机随机小动作调度器（荡丝 swing / 摆 pose / 射蛛丝 web / 蹲防 crouch），仅 idle 且非拖拽时触发；② 射蛛丝由静态网格弹出改为丝线绘制射出（dashoffset）+ 末端蛛网团 + 角色投掷预备/甩出，不再死板；③ 修复状态同步 bug：右键菜单在独立 pet-menu 窗口 setOverride/feed 后桌宠窗口不接收，导致菜单选状态/喂食桌宠表情不变（新增 pet-store-updated 事件 + reload 同步 override/喂食字段） |
| v0.6.2-beta.30 | 2026-08-06 | 旧版 | 新增 Spider-Man 风格桌宠皮肤：右键菜单「皮肤」切换（spiderman / popmart-3d）；红蓝战衣蛛网纹 Q 版英雄，绿底生成 + PIL 绿键抠透明 + 6 帧呼吸序列，动画/emoji/状态/交互体系与 popmart3d 完全复用；沿用皮肤注册表即插即用 |
| v0.6.2-beta.29 | 2026-08-06 | 旧版 | 桌宠动起来：单张透明 PNG 升级为 6 帧序列（PIL 合成呼吸缩放 + 眯眼），CSS 逐帧播放；idle 仅保留 translate/rotate 浮动，避免与帧序列双重缩放；表情 emoji 浮层继续聚在熊猫头顶上方；拖拽仍用 Tauri 原生 startDragging |
| v0.6.2-beta.28 | 2026-08-06 | 旧版 | 桌宠交互优化：表情 emoji 浮层由压在脸上改为聚到熊猫头顶上方；idle 加入呼吸缩放更"活"；拖拽改用 Tauri 原生 startDragging（OS 搬窗），根治透明置顶窗每帧 IPC 导致的"不跟手" |
| v0.6.2-beta.27 | 2026-08-06 | 旧版 | 桌宠 Pop Mart 3D 熊猫素材重构：单张透明 PNG 替代 body/eyes/nose/mouth 四层切图，从根上消除五官割裂/重影；保留 idle 浮动、状态动画、点击/拖拽/过载等交互 |
| v0.6.2-beta.27 | 2026-08-05 | 旧版 | 优化桌宠 Pop Mart 3D 投影：将五官独立 drop-shadow 改为整体统一投影，减弱割裂感 |
| v0.6.2-beta.25 | 2026-07-28 | 旧版 | 桌宠熊猫「重切图 + 眼神/嘴动态化」：将烤死表情的单张 PNG 拆为 body/eyes/nose/mouth 四层对齐透明图（眼神漂移+眨眼、嘴 5 态张合、鼻缩放）；修 `PetWindow` 140×140 硬编码导致熊猫被压进小盒、四周留大白边；气泡改贴窗顶内侧更贴近熊猫。⚠️ 桌宠为测试版功能，可能不稳定，可在「设置」页关闭 |
| **v0.6.2-beta.1** | 2026-07-24 | 旧版 | 解耦皮肤系统 + 新增 Pop Mart 3D 潮玩桌宠：右键菜单"皮肤"切换 2D 部件合成（Panda-2D）和 Pop Mart 3D 卡（戴黄帽弹吉他熊猫，1282×2850 portrait），状态用 emoji 浮层表达；不变动任何原 v0.6.1-beta.1 组件，新皮肤按 `skins/<id>/` 即插即用，便于后续 Live2D / 待办小组件 / 番茄钟等扩展 |
| v0.5.0 | 2026-07-14 | 旧版 | 多语言国际化（i18n）：新增 zh-CN / en-US 双语切换，设置页下拉即时切换无需重载；前端自生成周期标签 / 分类名 / 时长格式化；图表随语言重渲染（零后端改动） |
| v0.4.5 | 2026-07-14 | 旧版 | 统计概述时间范围联动：切换「今天/近7/14/30天」时「设备使用时间」与「App 使用时长排行」同步按范围聚合刷新（后端 overview/ranking 新增 days 参数，前端 loadDetails 传 range） |
| v0.4.4 | 2026-07-11 | 旧版 | 修「跨天今天按钮」+ Linux CI 三端通过：Dashboard 今天按钮实时取系统日期 + linux.rs 完整适配 x11rb 0.13 GetPropertyReply 新 API |
| v0.4.3 | 2026-07-10 | 旧版 | 修「default 幽灵设备」：迁移回填改用真实 device_id（取代字面量 'default'）+ schema 补 device 列 |
| v0.4.2 | 2026-07-10 | 旧版 | 新增统一日志系统（4 处核心埋点 + Settings 一键导出 + 15MB 上限） |
| v0.4.1 | 2026-07-09 | 旧版 | 修复采样循环死锁、macOS 权限 API bug、Mutex poison 雪崩等 11 项 |
| v0.4.0 | — | ⚠️ **不推荐** | 已知严重 bug：采样循环 `block_on` 嵌套导致时间不统计 |
| v0.3.1 | 2026-07-08 | 旧版 | UI/UX 优化（按设备清理、自动归类、弹窗化） |
| v0.3.0 | 2026-07-08 | 旧版 | 首版公开 |

### 💾 文件命名规则

每个发布版本的文件名都带 **版本号 + 平台架构**，便于区分：

| 平台 | 文件名格式 | 说明 |
|------|-----------|------|
| macOS (Apple Silicon) | `ScreenTime-Pro_{ver}_aarch64.dmg` | 拖入「应用程序」即可，需授予「辅助功能」权限 |
| Windows (x64) | `ScreenTime-Pro_{ver}_x64-setup.exe` | 双击运行（NSIS 安装包），需系统已装 **WebView2 运行时**（Win10/11 通常自带） |
| Linux (x64) | `ScreenTime-Pro_{ver}_amd64.AppImage` / `.deb` | 由 CI 在 Linux 环境构建（详见下方「从源码构建」） |

> 各平台最新安装包（macOS / Windows / Linux 三端）统一发布在 **[GitHub Releases](https://github.com/Slk90s/screentime-pro/releases)**（⭐ v0.7.1 Latest）。
> 本地 `release/v0.7.1/` 仅作带版本号归档（不入库）；Linux 因本机构建环境限制需在 CI 中产出（见 `.github/workflows/build.yml`）。

---

## 🚀 快速开始

> 💡 **下载入口**：所有平台的最新版本请从 **[GitHub Releases](https://github.com/Slk90s/screentime-pro/releases)** 下载（⭐ Latest 自动指向 v0.7.1）。

### macOS
1. 从 [GitHub Releases](https://github.com/Slk90s/screentime-pro/releases) 下载 `ScreenTime-Pro_0.7.1_aarch64.dmg`，打开并拖入「应用程序」。
2. 首次运行：系统设置 › 隐私与安全性 › **辅助功能** 中授予本应用权限（空闲检测必需）。
3. 程序默认开机自启、启动即开始追踪，菜单栏/托盘常驻。

### Windows
1. 从 [GitHub Releases](https://github.com/Slk90s/screentime-pro/releases) 下载 `ScreenTime-Pro_0.7.1_x64-setup.exe`（NSIS 安装包）。
2. **首次安装**：若系统未装 WebView2 运行时，安装器会**自动下载并安装**（需联网，几秒到几分钟）。Win10 1809+ / Win11 通常已内置，无需此步。
3. 托盘右键「退出」可彻底关闭；「设置」页可开关开机自启。

> **手动安装 WebView2 永驻版**（系统未联网或安装失败时）：<br>
> [Microsoft Edge WebView2 Runtime（Evergreen Standalone）](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) › 「Evergreen Standalone Installer」下载 `MicrosoftEdgeWebView2RuntimeInstallerX64.exe`。<br>
> 检查命令：浏览器打开 `edge://version/`，看到「Microsoft Edge」即代表已就绪。

---

## ❓ 常见错误

| 症状 | 原因 | 解决 |
|------|------|------|
| Windows 启动后窗口黑屏 / 立刻闪退 | 缺 WebView2 运行时 | 安装包会自动下载；若失败按上方链接手动装永驻版 |
| Windows 双击 exe 弹出「无法启动此程序，因为计算机中丢失 WebView2Loader.dll」 | 安装包损坏或被解压了 | 用 NSIS 安装包（exe），不要用 7-Zip 解压后运行 |
| Windows SmartScreen 拦截「未识别的应用」 | 包未签名 | 点击「更多信息 › 仍要运行」 |
| macOS 启动提示「无法打开，因为开发者无法验证」 | 未公证（Apple Developer 账号未配置） | 终端执行 `xattr -d com.apple.quarantine "/Applications/ScreenTime Pro.app"` 后重试 |
| macOS 辅助功能请求反复弹出 | 权限未真正授予或被系统重置 | 系统设置 › 隐私与安全性 › 辅助功能中确认勾选 + 重启应用 |
| 追踪不到任何活动 | macOS 未授权辅助功能；或系统在空闲 | 退出 app，授予权限后重启；登录界面不计 |

---

## 🐛 用户报 bug 流程（v0.4.2+）

遇到问题时，**导出一份日志发给我**，10x 加快排查速度：

1. 打开 ScreenTime Pro → 顶部菜单「设置」/ 左侧栏「Settings」
2. 滚到页面底部 → **「🛠 日志与诊断」** 区域
3. 点击 **「📋 导出日志到桌面」** → 弹窗显示「✅ 日志已导出」
4. 点击 **「在访达中显示」** → 文件管理器会打开
5. 把 `screentime-pro-logs-{时间戳}.txt` 附件发送给我
6. 同时简要描述：问题现象 / 操作系统 / 复现步骤

> 📋 **日志内容说明**：
> - ✅ 包含：错误摘要、应用切换统计、权限检查结果、关键操作记录
> - ❌ **不包含**：窗口标题、聊天内容、密码、token 等敏感信息
> - 💾 体积：约 5MB / 天，最多保留 3 天（约 15MB 上限），不会撑爆你的硬盘

---

## 🛠 从源码构建

### 环境要求

| 依赖 | 版本 / 说明 |
|------|------------|
| Node.js | ≥ 18 |
| Rust (stable) | ≥ 1.77（`rustup` 安装） |
| **macOS** | Xcode Command Line Tools（`xcode-select --install`）；打包需 macOS 本机 |
| **Windows** | Visual Studio Build Tools + WebView2 运行时；交叉编译需 MinGW-w64 |
| **Linux** | `webkit2gtk-4.1-dev`、`libappindicator3-dev`、`librsvg2-dev` 等系统包 |

### 安装与运行

```bash
npm install
npm run tauri dev      # 开发模式（前端热重载 + Rust 重新编译）
npm run tauri build    # 打包为当前平台的安装包
```

> **纯前端预览**（无 Tauri 运行时）会自动使用内置 mock 数据，便于 UI 调试：
> ```bash
> npm run dev           # 浏览器打开 http://localhost:1420
> ```

### 各平台产出

- **macOS**：`src-tauri/target/release/bundle/dmg/ScreenTime Pro_{ver}_aarch64.dmg`
- **Windows（交叉编译，在 macOS/Linux 上）**：
  ```bash
  rustup target add x86_64-pc-windows-gnu
  brew install mingw-w64
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    cargo build --release --target x86_64-pc-windows-gnu
  # 产物：src-tauri/target/x86_64-pc-windows-gnu/release/screentime-pro.exe
  ```
- **Linux**：需在 Linux 环境（或 CI）运行 `npm run tauri build`，产出 `.AppImage` / `.deb`。

### 一键收集带版本号的发布包

```bash
bash scripts/package-release.sh
# 自动读取 tauri.conf.json 的 version，把已产出的三端产物
# 复制到 release/v{ver}/ 并按版本号重命名
```

---

## 📖 使用说明

### 权限

| 平台 | 权限要求 |
|------|---------|
| **macOS** | ① **辅助功能**（必需，空闲检测 + 稳定取前台应用名）；② **屏幕录制**（可选，用于采集窗口标题做更细粒度分类，未授权则回退到进程名） |
| **Windows** | 通常无需特殊权限，默认可取前台窗口标题 |
| **Linux** | 需对应桌面环境（X11/Wayland）的窗口与空闲检测权限 |

### 窗口与托盘

- 点击窗口关闭按钮（红点）**不会退出程序**，而是隐藏到系统托盘，后台采样继续运行。
- 托盘菜单提供「显示主窗口」与「退出」；macOS 左键点击托盘图标在显示/隐藏间切换。
- 若要从托盘真正退出，请使用菜单的「退出」。

### 开机自启

- 集成 `tauri-plugin-autostart`（macOS 写 LaunchAgent / Windows 写注册表 / Linux 写 systemd --user）。
- **首次运行默认开启**；可在「**设置**」页开关。
- 启动即自动追踪，无需手动点击「开始」。

### 分类规则引擎

- 采集到的应用按「字段 + 匹配方式 + 匹配值」自动归入分类，无需人工整理。
- 字段：`process_name` / `window_title` / `exe_path` / `bundle_id` / `name`
- 匹配方式：`contains` / `equals` / `prefix` / `suffix` / `regex`
- 规则存于 `classification_rules` 表（带优先级与启停），「分类规则」页可增删改、一键重算历史分类。

### 数据隐私

- 所有数据仅存于本机 SQLite（macOS 位于 `~/Library/Application Support/com.screentime.pro/`），默认不上传任何服务器。
- 窗口标题可在后续版本增加脱敏规则。

---

## 📂 项目结构

```
screentime-pro/
├── src/                  # Vue 3 前端
│   ├── api/              # Tauri invoke 封装 + 浏览器 mock
│   ├── components/       # OverviewCard / DailyBarChart / HourlyStackedChart / AppRanking / DeviceSwitcher / DatePicker
│   ├── views/            # Dashboard / Trends / Settings / Rules
│   ├── utils/format.ts   # 时长格式化
│   ├── types.ts          # 与后端结构对应的 TS 类型
│   ├── App.vue / main.ts / style.css
├── src-tauri/            # Rust 后端
│   ├── src/
│   │   ├── tracker/      # 平台采集（macos / windows / linux + trait 抽象）
│   │   ├── db/           # SQLite 封装 + 聚合查询
│   │   ├── commands.rs   # Tauri commands + 采样循环
│   │   ├── lib.rs / main.rs / error.rs
│   ├── Cargo.toml / tauri.conf.json / capabilities/
│   └── icons/
├── scripts/
│   └── package-release.sh   # 按版本号收集发布包
├── .github/workflows/
│   └── build.yml            # 三端自动构建（macOS / Windows / Linux）
├── sql/                  # schema.sql / seed_categories.sql / seed_rules.sql
├── release/v0.7.1/ # 已构建的带版本号安装包（不入库，走 GitHub Releases）
├── README.md / LICENSE / .gitignore
└── package.json / vite.config.ts / tsconfig*.json
```

---

## 🧱 技术栈

| 层 | 选型 | 说明 |
|----|------|------|
| 桌面壳 | **Tauri 2.x** | Rust 后端 + 系统 WebView，体积小、安全 |
| 前端 | **Vue 3 + TypeScript + Vite** | 组件化、类型安全 |
| 图表 | **Chart.js 4** | 柱状图 / 堆叠图 |
| 后端 | **Rust** | 平台 API 采集 + 采样聚合 |
| 存储 | **SQLite (rusqlite bundled)** | 零系统依赖，本地隐私优先 |

---

## 🗺 开发路线

| 阶段 | 内容 | 状态 |
|------|------|------|
| **P0** | macOS 实装 + 前端 Dashboard + SQLite 存储 + 采样聚合 + 系统托盘 | ✅ 完成 |
| **P1** | Windows / Linux 适配 + 开机自启 + 菜单栏纯后台 + 分类规则引擎 | ✅ 完成 |
| **P2** | 多设备数据合并 + 周/月同比 + 设置页 + 通知提醒 | ✅ 完成 |
| **P3** | 窗口标题脱敏规则 + 更丰富的图表/导出格式（CSV/Excel） | 🚧 规划中 |

---

## 📜 版本历史与发布规范

发布版本号、Release Notes 模板、版本同步清单、异常处理见 **[`docs/RELEASE.md`](./docs/RELEASE.md)**。

| 当前版本 | 历史摘要 |
|----------|----------|
| ⭐ **v0.7.1**（正式版） | 修复发布：补齐此前 v0.7.0 缺失入库的桌宠皮肤源码资产（spider-man 皮肤、bubble-phrases.json、popmart3d PNG 资源），修复因源码未入库导致 CI 三端构建失败、Release 未生成的问题；功能与 v0.7.0 完全一致 |
| **v0.7.0**（正式版） | 整合 0.6.2 全部 Beta 修复 + 新功能：日历月视图切换与本月统计概况 + 今天同步跳转；桌宠设置移除「已经吃饱啦→喂食」冗余提示；修复喂食无反馈与饿度值不变（菜单吐司 + 桌宠进食点头 + 好吃气泡 + 饱食度自然衰减）；优化桌面抖动（过载阈值上调）；梳理饿度/心情算法 |
| v0.6.2-beta.27（已发布） | 桌宠 Pop Mart 3D 熊猫素材重构：单张透明 PNG 替代四层切图，从根上消除五官割裂/重影；保留 idle 浮动、状态动画、点击/拖拽/过载等交互 |
| v0.6.2-beta.25（已发布） | 桌宠熊猫「重切图 + 眼神/嘴动态化」：拆 body/eyes/nose/mouth 四层对齐 PNG，眼神漂移+眨眼、嘴 5 态张合；修 PetWindow 140×140 硬编码留白边；气泡贴窗顶更贴近熊猫。⚠️ 桌宠为测试版功能 |
| **v0.6.2-beta.1** | 解耦皮肤系统 + 新增 Pop Mart 3D 潮玩桌宠：右键菜单"皮肤"切换 2D 部件合成（Panda-2D）和 Pop Mart 3D 卡（戴黄帽弹吉他熊猫 portrait），状态用 emoji 浮层表达；新皮肤按 `skins/<id>/` 即插即用，便于后续 Live2D / 待办小组件 / 番茄钟等扩展 |
| v0.6.1-beta.1 | 桌面宠物（QQ 企鹅风格）：透明置顶可拖拽熊猫，22 张 sprite + 部件合成渲染，14 状态自动联动（监听前台应用）+ 右键菜单/喂食/点击交互，零后端改动 |
| v0.5.0 | 多语言国际化（i18n）：新增 zh-CN / en-US 双语切换，设置页下拉即时切换无需重载；前端自生成周期标签 / 分类名 / 时长格式化；图表随语言重渲染（零后端改动） |
| v0.4.4 | 修「跨天今天按钮」+ Linux x11rb 0.13 完整适配（CI 三端通过） |
| v0.4.3 | 修「default 幽灵设备」：迁移回填改用真实 device_id + schema 补 device 列 |
| v0.4.1 | 修采样循环死锁 + 修 macOS 权限检测 + 三端完整构建 |
| ⚠️ v0.4.0 | 自动归类联网搜索（已知 bug，已被 v0.4.1 取代） |
| v0.3.1 | 检查更新 Atom feed + 设置改 Modal 弹窗 |
| v0.3.0 | 首次公开发布 |

---

## 🤝 贡献

欢迎 Issue 与 PR！

1. Fork 本仓库并创建特性分支（`git checkout -b feat/xxx`）。
2. 提交改动（建议中文注释，遵循既有代码风格）。
3. 确保 `npm run tauri build`（对应平台）可成功构建。
4. 发起 Pull Request，描述改动动机与验证方式。

> 代码规范要求：所有源码须带中文注释；新增界面/模块先写注释再实现。

---

## 📄 许可证

本项目基于 [MIT License](./LICENSE) 开源。

---

## ⚠️ 免责声明

本工具仅用于个人电脑使用时长统计与自我管理。采集的数据保留在本地，开发者不会也无法访问。
macOS 的「辅助功能 / 屏幕录制」权限属系统级授权，请在系统设置中按需管理。
