# ScreenTime Pro

> 跨平台电脑应用使用时长追踪工具（macOS / Windows / Linux），对标 iOS「屏幕使用时间」。
> 数据 **100% 本地存储、零上传**，隐私优先。

[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](https://github.com/)
[![License](https://img.shields.io/badge/license-MIT-green)](./LICENSE)
[![Version](https://img.shields.io/badge/version-0.2.0-orange)](./release)

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

---

## 📦 下载 / Release

每个发布版本的文件名都带 **版本号 + 平台架构**，便于区分：

| 平台 | 文件名格式 | 说明 |
|------|-----------|------|
| macOS (Apple Silicon) | `ScreenTime Pro_{ver}_aarch64.dmg` | 拖入「应用程序」即可，需授予「辅助功能」权限 |
| Windows (x64) | `screentime-pro_{ver}_x86_64.exe` | 双击运行，需系统已装 **WebView2 运行时**（Win10/11 通常自带） |
| Linux (x64) | `screentime-pro_{ver}_amd64.AppImage` / `.deb` | 由 CI 在 Linux 环境构建（详见下方「从源码构建」） |

> 当前本仓库已包含 **macOS 0.2.0** 与 **Windows 0.2.0** 的构建产物（见 `release/v0.2.0/`）。
> Linux 因本机构建环境限制，需在你自己的 Linux 机器或 GitHub Actions CI 中产出（见 `.github/workflows/build.yml`）。

---

## 🚀 快速开始

### macOS
1. 下载 `ScreenTime Pro_0.2.0_aarch64.dmg`，打开并拖入「应用程序」。
2. 首次运行：系统设置 › 隐私与安全性 › **辅助功能** 中授予本应用权限（空闲检测必需）。
3. 程序默认开机自启、启动即开始追踪，菜单栏/托盘常驻。

### Windows
1. 下载 `screentime-pro_0.2.0_x86_64.exe`（NSIS 安装包）。
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
├── release/v0.2.0/       # 已构建的带版本号安装包（不入库，走 GitHub Releases）
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
