# ScreenTime Pro

> 跨平台电脑应用使用时长追踪工具（macOS / Windows / Linux），对标 iOS「屏幕使用时间」。
> 数据 **100% 本地存储、零上传**，隐私优先。

[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](https://github.com/)
[![License](https://img.shields.io/badge/license-MIT-green)](./LICENSE)
[![Version](https://img.shields.io/badge/version-0.7.5-blue)](./release)

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
| **v0.7.5** | 2026-08-23 | 🚀 **正式版** | **macOS 状态栏系统指标监测（新增）**：可选在菜单栏实时显示 CPU / 内存 / 磁盘占用率。复用 `system_load` 基础设施新增统一 `MetricsSampler`——CPU 走已有 `kern.cp_time` 差分采样，内存用 `host_statistics64 + hw.memsize`（口径对齐活动监视器：`available = free + inactive + speculative`），磁盘用 `statfs("/")`（对齐 Finder 读取信息），均为 macOS 原生 API、零 webview 开销；采样线程 1s 一次、磁盘 30s 缓存，变化 < 1% 不更新托盘 title（减少 NSStatusItem 重绘），开关关闭即恢复纯图标；设置页新增开关（仅 macOS 显示）+ `get/set_system_metrics_enabled`、`get_system_metrics` IPC 命令。Windows/Linux 默认隐藏该功能 |
| **v0.7.3** | 2026-08-13 | 🚀 **正式版** | **macOS 日志/导出修复 + 桌宠开关同步 + macOS 拖拽跟手 + DMG 门禁脚本正式生效**：① 修复 macOS 无运行日志；② 修复导出日志 txt 正文全空；③ 修复桌宠状态三处不同步；④ 修复 macOS 桌宠拖动不跟手；⑤ DMG 门禁脚本正式注入并覆盖上传到 Release |
| **v0.7.2** | 2026-08-09 | 旧版 | **本地自动备份 + macOS 门禁修复 + 设备ID稳定化** |
| **v0.7.1** | 2026-08-08 | 旧版 | **v0.7.0 的修复重发**：补齐缺失入库的桌宠皮肤源码资产 |
| **v0.7.0** | 2026-08-08 | 旧版 | **整合 0.6.2 全部 Beta 修复 + 新功能** |
| **v0.6.2-36** | 2026-08-08 | 旧版 | 蜘蛛侠体验细化 |
| v0.5.0 | 2026-07-14 | 旧版 | 多语言国际化（i18n） |
| v0.4.5 | 2026-07-14 | 旧版 | 统计概述时间范围联动 |
| v0.4.1 | 2026-07-09 | 旧版 | 修复采样循环死锁、macOS 权限 API bug 等 11 项 |
| v0.4.0 | — | ⚠️ **不推荐** | 已知严重 bug：采样循环 `block_on` 嵌套导致时间不统计 |
| v0.3.1 | 2026-07-08 | 旧版 | UI/UX 优化 |
| v0.3.0 | 2026-07-08 | 旧版 | 首版公开 |

### 💾 文件命名规则

每个发布版本的文件名都带 **版本号 + 平台架构**，便于区分：

| 平台 | 文件名格式 | 说明 |
|------|-----------|------|
| macOS (Apple Silicon) | `ScreenTime-Pro_{ver}_aarch64.dmg` | 拖入「应用程序」即可，需授予「辅助功能」权限 |
| Windows (x64) | `ScreenTime-Pro_{ver}_x64-setup.exe` | 双击运行（NSIS 安装包），需系统已装 **WebView2 运行时** |
| Linux (x64) | `ScreenTime-Pro_{ver}_amd64.AppImage` / `.deb` | 由 CI 在 Linux 环境构建 |

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
| **P3** | 窗口标题脱敏规则 + 更丰富的图表/导出格式 | 🚧 规划中 |

---

## 📜 版本历史与发布规范

发布版本号、Release Notes 模板、版本同步清单见 **[`docs/RELEASE.md`](./docs/RELEASE.md)**。

---

## 🤝 贡献

欢迎 Issue 与 PR！

1. Fork 本仓库并创建特性分支（`git checkout -b feat/xxx`）。
2. 提交改动（建议中文注释，遵循既有代码风格）。
3. 确保 `npm run tauri build`（对应平台）可成功构建。
4. 发起 Pull Request，描述改动动机与验证方式。

---

## 📄 许可证

本项目基于 [MIT License](./LICENSE) 开源。
