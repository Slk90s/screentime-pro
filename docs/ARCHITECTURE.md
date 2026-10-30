# ScreenTime Pro — 架构文档

<!--
备份记录：2026-09-04 备份至 bak/docs/ARCHITECTURE.md.bak（操作类型：局部更新 - 版本号对齐
  v0.7.5、修正 IPC 命令名、补 classifier.rs / system_load/ 模块、更新桌宠文件树；
  本文件为文档无 SemVer 版本号，版本声明跟随 tauri.conf.json）

修改历史：
  - 2026-09-04 @v0.7.5: 修复 - 头部版本声明由 v0.7.0 更新为 v0.7.5（实际代码已是 v0.7.5）
  - 2026-09-04 @v0.7.5: 修复 - §5 IPC 契约表命令名与实际代码不一致（list_rules→get_rules、
    get_today_summary→get_overview、get_app_summaries→get_app_ranking）
  - 2026-09-04 @v0.7.5: 新增 - §3.1 补 classifier.rs 与 system_load/ 两个模块地图条目
  - 2026-09-04 @v0.7.5: 修正 - §3.3 桌宠文件树与 §11 文件清单同步至当前实际结构
  - 2026-09-14 @v0.7.11: 修正 - 头部「最后更新」推进至 v0.7.11；§12 的 releaseBody 现值说明
    由「仍是 v0.7.0 旧文案」更正为「当前为 v0.7.11 文案，但发版前仍须逐版改写」
  - 2026-09-16 @v0.8.0: 新增 - 「屏幕截图」子系统（`screenshot/` 模块、`capture` 窗口、
    `screenshots` 表、10 个 IPC 命令）；桌宠点击穿透重写（`pet/hit_mask.rs` 32×32 alpha 命中网格
    + 全局光标轮询）；macOS 前台全屏真识别（CGWindowList / CGDisplayBounds）；依赖 windows 0.58→0.62
  - 2026-09-16 @v0.8.0: 修订 - 截图遮罩窗由「透明透桌面」改为「**冻结帧 + 标注画布**」：
    新增 IPC `screenshot_frame`（截图上交 11 个、总数 77→78）；`screenshot_commit` 改收前端合成 PNG；
    `screenshot_set_config` 改返 `ApplyResult`（快捷键注册失败可回传）；删除 `compose_export` /
    `sd_rounded_box`（合成职责移交前端 canvas）。§3.1 / §5 / §11 计数同步。
  - 2026-09-17 @v0.8.1: 新增 - 「取字」子模块 `screenshot/ocr.rs`（Windows WinRT `Windows.Media.Ocr`，
    离线；COM 与 WinRT 调用必须同线程 → `spawn_blocking` + `CoInitializeEx(COINIT_MULTITHREADED)`）；
    新增 IPC `screenshot_ocr` / `screenshot_copy_text`（截图上交 13 个、总数 78→**80**）；
    依赖 windows crate 增特性 `Media_Ocr` / `Graphics_Imaging` / `Storage_Streams` /
    `Globalization` / `Foundation` / `Win32_System_Com`。§3.1 / §5 / §11 计数同步。
  - 2026-09-17 @v0.8.1: 修正 - 标注工具去重（删「模糊（打码）」，与「马赛克」功能重复）；
    工具栏按钮仍 13 个（删 1 加 1）。
  - 2026-09-17 @v0.8.2: 新增 - 浮窗截图按钮（可见条件 = 截图开启 && 浮窗开启，走 1Hz 配置轮询
    **零新增 IPC**，总数保持 80）；桌宠右键菜单 ↔ 设置页状态同步。
  - 2026-09-18 @v0.9.0: 新增 - 取字「增强引擎」`screenshot/ocr_onnx.rs`（PaddleOCR v4 det/rec，
    ONNX，跨平台）+ 引擎调度 `screenshot/ocr_engine.rs`（`EngineKind` / `EnginePaths::resolve`
    多候选探测 / `ocr_engine_info` IPC）；截图上交 14 个、总数 80→**81**。**资源分发改为分平台**：
    Windows 运行库+模型随包；macOS / Linux 模型随包、运行库 `download_runtime_lib_if_missing()`
    首次使用时后台下载（默认源 GitHub Release `ocr-runtime`，`SD_OCR_DOWNLOAD_BASE` 可覆盖，
    带 SHA256 校验）。§12 的 releaseBody 现值说明更新为「v0.9.0 文案」。
  - 2026-09-18 @v0.9.0: **重切补充** - macOS「标准」取字引擎落地：`screenshot/ocr_vision.rs`
    （系统 Vision `VNRecognizeTextRequest`；objc2 0.6 / objc2-vision 0.3 —— 与 Tauri 已引入的
    objc2 谱系**同代**，零新增版本谱系）。`ocr.rs` 降为「系统引擎的平台分发壳」
    （Win→WinRT / mac→Vision / Linux 明确降级）；`system_engine_available()` 在 macOS 为真，
    `default_engine()` 在 macOS 由此回落 `system` —— **mac 开箱取字、零下载**。
    `EngineInfo` 增 `system_engine` / `enhanced_models_ready` / `enhanced_runtime_ready`：
    把「模型齐备」与「运行库就绪」**拆成两个字段**，修掉 macOS 上「增强」被永久锁死的死锁
    （前端判据由 `enhanced_ready` 改为 `enhanced_models_ready`）。**IPC 总数不变（仍 81）**。
    同批合并进重切版：mac / Linux 取字**三重不可用**修复（运行库落盘路径曾返回目录而非文件名
    → 运行库永远落不了盘；下载超时过短；平台感知默认引擎 + 存量配置自愈）。
    **本版为同名 tag 重切**：v0.9.0 首发版对 macOS / Linux 的取字链路不可用，故修复后重切发布。
  - 2026-10-29 @v1.2.12: 新增 - 桌宠整改批次 1/2 落地（`REMEDIATION-PLAN-2026-10-29.md`）。
    §3.1 补 `pet/drag.rs`（macOS / 兜底路径的 Rust 侧拖拽循环）与 `window_bounds.rs`
    （float 与 pet 共用的离屏几何 util）两行，并更新 `pet/hit_mask.rs` 行（Linux X 连接缓存 +
    「`RustConnection` 无 `Clone`」这条探针实锤）；§5 补「桌宠拖拽（`pet::drag`）」IPC 行；
    §9 加两条不变量（**非主线程循环只准投递不准查询窗口 getter**、拖拽期窗口保持静态）；
    §11 文件树补 `window_bounds.rs` 与 capabilities 全部 5 个文件（旧文档只写了 capture.json）。
    **IPC 89→91 是重新数出来的**（48+22+14+3+3+1），不是照文档递推 —— 本文件 2026-10-25
    那条记账纠正讲的就是这件事。头部版本声明推进至 v1.2.12，v1.2.11 段按「只留当前版全量 +
    上一版压缩」收成 6 行。
  - 2026-10-27 @v1.2.11: 新增 - 「应用内自动升级」子系统。新模块 `src-tauri/src/app_update.rs`
    （与 `commands::check_for_update` **并存不替代**：后者只告知新版本并跳浏览器，前者真的下装）；
    前端 `src/api/appUpdate.ts` + 设置页 `nativeUpdate` 分支；IPC 85→**88**
    （`app_update_supported` / `check_app_update` / `install_app_update`）。平台门控
    `install_supported()`：Windows nsis/msi、Linux 仅 AppImage、macOS 主动关闭。
    §3.1 / §5 / §11 计数同步为 88；§5 新增「应用内升级」行。
  - 2026-10-27 @v1.2.11: 修复 - macOS「辅助功能已授权却报未授权」（P0，三层叠加）。
    设置页提示条件写反；「重置并重启」只清 `ScreenCapture` 不清 `Accessibility`（`tccutil reset`
    的**服务名按服务不同且大小写敏感**）→ 按服务分流并给辅助功能行补独立重置入口；
    真因是 TCC 按进程快照 + ad-hoc 签名 cdhash 每次 CI 都变（记录按「路径 + cdhash」绑定）。
    同时确认辅助功能**非功能必需**（空闲检测走 `CGEventSourceSecondsSinceLastEventType`），
    未授权不影响计时准确性。
  - 2026-10-27 @v1.2.11: 构建 - updater 依赖钉死 `=2.12.0`（实测 2.13.0+ 要 Rust 1.90 +
    edition 2024，裸写 `"2"` 会解析到 2.14.x 直接编不过）；`tauri.conf.json` 增
    `bundle.createUpdaterArtifacts` 与 `plugins.updater`（pubkey 是**公钥内容**不是路径）。
    CI：三个 `tauri-action` 一律 `includeUpdaterJson: false`（`uploadUpdaterJson` **不是真实
    输入名**，会被静默忽略导致三平台互相覆盖），改由独立 `updater-json` job 汇总；
    新增 `.github/scripts/gen_latest_json.py`；`pub_date` 严格 RFC3339（解析错误变 `DeError`
    会中断整个检查）。`.gitignore` 的 `scripts/` 改为 `/scripts/`（会吞掉 `.github/scripts/`）。
  - 2026-10-26 @v1.2.10: 修复 - macOS 交互体验专项（六修复 + 一新增，Windows / Linux 零功能改动）。
    a) 浮窗浅色外观反色（白字 1.36:1 → 全过 AA），只挂 `.has-blur` 即仅 macOS；
    b) 浮窗高度/圆角归一到单一源（Rust `set_size`），修「底部露 8px 材质 + 纵向双弧」；
    c) 菜单栏图标改 template 剪影（浅色栏 1.91:1 → 跟系统染色）+ 新增 `trayTemplate*.png`；
    d) 更新检测三成因（只查一次 / 时间戳烧掉窗口 / `system-proxy` 丢失）+ 三通道抓取；
    e) 权限弹窗反复弹（启动预请求 + 单信号判据 + focus 回调非边沿触发）；
    f) 浮窗材质层只增不减（`window-vibrancy` apply 不去重、clear 一次只摘一层）；
    g) 新增设置页权限中心；⑧ `tauri.conf.json` float 窗口补 x/y（height/minHeight 刻意维持 36）。
    头部「最后更新」推进至 v1.2.10；§1 当前版本号同步；§5 IPC 表「权限」行补 `screen_capture`
    字段与双信号口径说明。IPC 总数不变（**85**）。
  - 2026-10-20 @v1.2.1: 修复 - 三处跨平台实机问题（前两处 Windows，第三处 macOS）。
    a) 「状态栏开了却看不到」：localStorage `float_pos` 存的是副屏还在时的负坐标，副屏移除后
       被无校验恢复，窗口离屏（实测物理 (-695,159)，单屏 1536×864）。`float_window.rs` 新增
       `clamp_to_visible_area`，show/move/resize 后用**物理像素**枚举显示器，重叠不足窗口宽 1/3
       即回落主屏右上角。
    b) 「记录中 screentime-pro · ScreenTime Float」：辅助窗口与主窗口同进程同 exe，在其上按下
       （浮窗 startDragging）会短暂成为前台，旧实现无差别采集入库（macOS 浮窗不夺 frontmost）。
       `tracker/windows.rs` 新增 `SELF_HELPER_TITLES` 标题过滤，命中即 NoForeground 跳过；
       主窗口不过滤。
    c) 「macOS 状态灯恒定橙色不变色」：v1.2.0 负载取 peak=max(cpu,mem)，macOS 内存长期
       70~80%（wired/compressor/文件缓存，系统常态）把峰值钉在橙档、CPU 回落不变绿。
       `float/FloatBar.vue` 的 loadLevel 改为 CPU 瞬时主信号，mem < 80%（MEM_WARN）不参与、
       ≥80% 才抬升档位；五档阈值不变。IPC 总数不变（82）。
-->

> 目的：给项目维护者与其他 Agent 提供"一张图看懂全貌"和"改哪里、不改哪里"指南。  
> 与 README 区别：README 是用户面（怎么装、怎么用），本文件是工程面（怎么搭、怎么扩）。  
> 最后更新：2026-10-30（同步至 **v1.2.12**——**一次交付 v1.2.11 + v1.2.12 两版内容**：
> 截图链路两个 P0 + 应用内自动升级 + 桌宠拖拽重构 + 磁盘口径三端统一）。
> v1.2.11 从未打过 tag，故合并出包；README 版本表已合并该行，`releaseBody` 同口径。
> 本版主题是「桌宠拖拽终于跟手」，外加一件更值得记住的事：
> 一条被文档写成「已根治」的路径，可能因为**少一个 ACL 权限**而从未真正跑过。
> **总纲：凡是「只在某个时刻才成立」的状态，都必须留一个能重新求值的信号，不能只信一次快照；
> 而凡是「声称已修」的结论，都要先问一句——它真的被执行过吗？**
> ① **桌宠原生拖拽从未生效（整改 1.1，一行修复）**：`capabilities/pet.json` 缺
> `core:window:allow-start-dragging` → 前端 `startDragging()` 每次被 ACL 拒 → catch → 回退到
> 「每帧 `invoke('move_pet_window')` 搬窗」。v0.6.2-beta.28 那句「已根治」修的是**回退路径**的竞态，
> 而入口一直是死的。对照证据：`float.json` 早已有同一权限、悬浮条原生拖拽一直可用；而
> `gen/schemas/acl-manifests.json` 里 `core:window:default` 全 28 项**不含**它。
> ② **macOS 走 Rust 侧拖拽循环（整改批次 2，新模块 `pet/drag.rs`）**：双轨 —— Win/Linux 用原生
> （① 之后才真正生效），macOS 与「原生抛错」兜底走 8ms 循环读**全局光标**自己搬窗。
> 不上 mac 原生的理由（读 tao 0.35.3）：mac 的 `start_dragging` 是**合成一个假 `LeftMouseDown`**
> （`NSEventSubtype::WindowExposed`、pressure 硬编码 1.0）再调 `performWindowDragWithEvent:`，
> 对无边框透明窗不稳定，且**异步返回** → Promise 在拖拽刚开始就 resolve，前端立刻读位置收尾，
> 状态必然错乱。这套形态顺带解决旧手动路径的结构性缺陷：**没有 `setPointerCapture`、监听挂在
> `window` 上**，光标一甩出宠物矩形就收不到 pointermove → 桌宠掉队、松手回跳。
> ③ **线程约束（源码核实，非推测）**：tauri-runtime-wry 2.11.4 的 `send_user_message`
> （lib.rs:235-255）在主线程**内联执行**、否则 `proxy.send_event` **fire-and-forget**，
> `WindowMessage::SetPosition` 最终在主线程落地 ⇒ 循环里调 `set_position` 安全，
> **不需要** `run_on_main_thread`；而 `getter!` / `window_getter!`（:197-211）一律
> 「发事件 + `rx.recv()` 阻塞等回」⇒ **循环内零窗口 getter**，`end_drag` 回传的是循环自己记的
> 最后一个目标值，不是 `outer_position()` 的查询结果（`set_position` 异步投递，刚投的几帧还没落到
> NSWindow，去问会拿到历史值，写进持久化就是「拖完回跳」）。
> ④ **离屏校验抽成公共 util（整改 1.3）**：新增 `window_bounds.rs`，float 与 pet 共用同一判据
> ——**两轴各 ≥1/3 重叠**才算可见（旧 float 实现只有 `overlap_x`，竖着甩出屏外一路放行）。
> `move_pet_window` 新增 `clamp` 参数且**默认 false**：拖拽每帧调用时纠偏会让窗口在用户手里弹跳，
> 只有恢复 / 开关切换才传 true。
> ⑤ **启动恢复桌宠（整改 1.2，⚠️ 与整改方案的字面写法有偏差）**：方案原写「`lib.rs` setup 读
> 持久化 enabled」，实际做不到——桌宠开关只存在 localStorage `screentime-pet`（`PetWindow.vue`
> 深看 + `persistSuspended`），Rust 侧读不到，故改为**前端驱动**：pet webview 的 `onMounted` 末尾
> `restorePetOnStartup()`（enabled → setSize → `move_pet_window{clamp:true}` → show）。
> 修前的现状是 `enabled:true` 但重启后窗口不出现、设置页开关还显示「开」。
> ⑥ **Linux X 连接缓存（整改 1.4）**：`cursor_logical` 的 linux 分支旧写法每次
> `x11rb::connect(None)` 新建 socket（被穿透轮询以 ~18Hz 调用，拖拽循环再加一条 125Hz 线程），
> 改为 `thread_local!` + `Arc<RustConnection>`，读失败即弃缓存、下轮重连自愈。
> ⚠️ 探针实锤一个坑：`RustConnection`（x11rb 0.13.2，`rust_connection/mod.rs:65` 是 socket +
> 若干 Mutex）**没有实现 `Clone`**，首版 `conn.clone()` 在 Linux 上直接 E0599 编不过。这段
> cfg 门控代码在 Windows 本机 `cargo check` **根本不编译**，整包交叉 check 又被 libdbus-sys 的
> build.rs 假失败（LINUX_EXIT=101），只能靠独立探针 crate（`.tmp-probe-linux-cursor`，gitignored）
> + 反证（注入 `conn.clone()` → 101 报 E0599）——**没有探针，这个 bug 会一路带到 Linux 发版**。
> ⑦ **磁盘口径三端统一（v1.2.12 原生内容，不属桌宠整改）**：`system_load/` 三端一律
> `used = total - available_to_user`。此前 Linux 用 `f_bavail`（对的）、macOS 用 `f_bfree`
> （含 root 保留块）、Windows 用第 3 出参 `lpTotalNumberOfFreeBytes`（未扣配额），后果是同一块
> 默认 5% root reserved 的 ext4 在 Linux 上会比另两端**显示得更满**。⚠️ 这是**用户可见数值变化**，
> 也正是它不并入 v1.2.11 的原因（见 [`RELEASE.md`](RELEASE.md)）。多盘枚举（`DiskVolume` +
> 三端 `ffi_read_disks`，macOS 侧靠 `getfsstat` + 「类型/挂载点/只读位」三重过滤避开 APFS 同容器子卷）
> 已落在 Rust 侧，**前端消费方尚未接**，故带 `#[allow(dead_code)]`，接入时必须移除该 allow。
> **验证**：`cargo test --lib` **61 passed / 0 failed / 5 ignored**（本批次 +10：`window_bounds` 7 例、
> `pet/drag` 3 例）/ `vue-tsc --noEmit` 零错误 / `cargo fmt -- --check` 干净（本轮 fmt 只重写两个新文件，零附带改动）/
> `cargo clippy --lib -- -D warnings` **62 条 = 批次 0 基线，本批次新增 0**（`drag.rs` 首版自带的 3 条已修）；
> Linux 分支走上述探针（正证 exit 0 + 反证 exit 101）。
> IPC 对账 **91 = 91**（48 commands + 22 screenshot + 14 pet〔窗口/菜单 9 + 命中 3 + 拖拽 2〕
> + 3 float_window + 3 app_update + 1 float_glass）；⚠️ 89→91 是**重新数出来的**，不是照文档递推。
> 真机验收（发版前必做）：Windows 拖动桌宠后运行日志 grep `原生拖拽不可用` **0 命中**；
> 杀进程重启后桌宠按上次位置出现；把持久化位置手改 `(-5000,-5000)` 重启应落回可见区；
> macOS 三条用例（快速横甩 400px 且光标一度离开宠物矩形 / 帧延迟对比 / 拖拽中光标出体后穿透态正确）。
> 同批交付的 v1.2.11 内容（**该版号从未发布**，v1.2.12 是在其 WIP 之上继续改，两版合并出包）：
> 应用内自动升级
> （新模块 `app_update.rs`，仅 Windows NSIS / Linux AppImage；macOS 主动关闭、deb/rpm 不参与）
> + 截图链路两个 P0（截完图进程消失 / 首次截图半屏，详见 RELEASE.md §3.4）
> + macOS「辅助功能已授权却报未授权」三层修复 + updater 依赖钉 `=2.12.0` + CI 出 `latest.json`
> （`includeUpdaterJson` 才是真实输入名，`uploadUpdaterJson` 不存在会被静默忽略）
> + `.gitignore` 的 `scripts/` 锚定为 `/scripts/`；IPC 85→89。
> 上一版 v1.2.10：macOS 交互体验专项（浮窗浅色反色 / 高度单一源 / 托盘 template 图 /
> 更新检测三成因 / 权限弹窗 / 材质层泄漏 + 权限中心 + conf 补 x/y），Windows / Linux 零功能改动。

---

## 1. 项目一句话

跨平台应用使用时长追踪（macOS / Windows / Linux），对标 iOS「屏幕使用时间」。**数据 100% 本地（SQLite bundled），零上传，隐私优先**。

栈：**Tauri 2 + Rust + Vue 3 + TypeScript + Vite + Chart.js 4 + vue-i18n 9**。当前版本 **v1.2.12**（详见 [`RELEASE.md`](RELEASE.md)）。

> ⚠️ 本节版本号在 v0.7.0 → v0.7.5 期间**长期未更新**（曾停留在 v0.7.0），与 `tauri.conf.json` 脱节。
> 版本号唯一真实来源是 **`src-tauri/tauri.conf.json` 的 `version`**，改版本时务必回来同步本节。

---

## 2. 三层架构

```
┌─────────────────────────────────────────────────────────────────┐
│  Renderer (Tauri WebView, Vue 3 frontend)                       │
│  - 主窗口 Dashboard / Trends / Rules / Settings                 │
│  - 桌宠独立 webview: PetWindow → PetSkinRenderer → PetSkin      │
│  - 截图独立 webview: CaptureOverlay（冻结帧 + 选区 + 标注 + 工具栏）│
│  - API 抽象: src/api/*（tracker / config / db / screenshot 包装）│
└────────────────────┬────────────────────────────────────────────┘
                     │ Tauri IPC (snake_case 入参 → camelCase 参数 / 原样返回值)
┌────────────────────┴────────────────────────────────────────────┐
│  Core (Rust, src-tauri/src)                                     │
│  - commands.rs: 主 IPC 命令路由（+ pet/ 子模块的桌宠窗口/菜单命令）                    │
│  - tracker/{macos,windows,linux}.rs: 平台采样器                 │
│  - categorizer.rs: 自动归类（Wikipedia + 本地字典 + LRU 缓存）   │
│  - db/: SQLite (rusqlite bundled, prepared statements)          │
│  - logging.rs + tauri-plugin-log: 统一日志                      │
└────────────────────┬────────────────────────────────────────────┘
                     │ 原生 API
┌────────────────────┴────────────────────────────────────────────┐
│  OS (macOS Accessibility API / Win32 / /proc)                    │
└─────────────────────────────────────────────────────────────────┘
```

**关键不变量**：所有跨进程数据流走 Tauri IPC。**Tauri v2 仅把命令**参数** camelCase ↔ snake_case，命令**返回值**原样序列化（Rust struct 字段名直接到前端）。**`types.ts` 注释里"自动转 camelCase"的旧说法是错的，已修正。

---

## 3. 模块地图（"改哪里"）

### 3.1 Rust 后端（src-tauri/src/）
| 模块 | 职责 | 改我会影响 |
|------|------|------------|
| `commands.rs` (~1148 行) | 主 IPC 命令路由（48 个 `#[tauri::command]` 在本文件）；`lib.rs` 的 `generate_handler!` 合计注册 **91** 个 = 本文件 48 + `screenshot/` 22 + `pet/` 14（窗口/菜单 9 + 命中 3 + 拖拽 2）+ `float_window.rs` 3 + `app_update.rs` 3 + `float_glass.rs` 1 | 前端 `api/*` 必须对齐字段名 |
| `lib.rs` (~700 行) | Tauri Builder、插件注册（含 `tauri-plugin-global-shortcut`）、`generate_handler!` 命令注册、系统托盘（含「截图」菜单项） | **新命令必须在此注册**，否则 invoke 报不存在 |
| `tracker/{macos,windows,linux}.rs` | 平台采样器（10s tick） | 跨平台数据一致性 |
| `tracker/mod.rs` | trait 抽象 + 后台循环 | — |
| `tracker/platform.rs` | `RawApp` 跨平台原始数据结构 | `classifier.rs` 入参 |
| `db/mod.rs` (~1236 行) | SQLite 连接池 + schema 管理 + 迁移队列 | 数据库迁移 |
| `db/models.rs` | 所有 DTO（前后端对接处，含 `ScreenshotOut`） | `src/types.ts` |
| **`classifier.rs`** (~181 行) | **分类规则引擎（纯逻辑，不依赖 DB）**：给定 `classification_rules` 规则 + 应用 → 分类 id。规则由上层内存缓存后传入 | 规则匹配语义。**`pattern` 与 `value` 两侧都必须小写化**，否则 `equals` 永不命中（v0.6.2-beta.17 踩坑） |
| `categorizer.rs` (~347 行) | 兜底自动归类：本地字典（60+ 软件）+ Wikipedia API + LRU 缓存 → `other`。**同步实现！** | 采样循环性能 |
| **`screenshot/`** | **v0.8.0 新增**：`ScreenshotState`（缓存帧 / 隐藏窗口记录 / 配置）+ `begin_capture`（隐自身→延时→`xcap` 抓主屏→显示 capture 窗）+ `screenshot_frame`（下发整屏 PNG 给遮罩窗）+ `screenshot_commit`（解码前端合成 PNG→剪贴板/保存→FIFO 清理→恢复窗口）+ `shortcut::apply`（`screenshot/mod.rs`）。**v0.8.1**：+`ocr.rs` 取字（Windows WinRT `Windows.Media.Ocr`，离线；`tidy_text()` 去中文单字间空格） | 截图主流程。**合成（裁剪/标注/圆角/阴影）已移交遮罩窗 canvas**，本模块只做编解码与落地；改这里不影响导出画质。**OCR 必须在 `spawn_blocking` 里跑**：COM 初始化与 WinRT 调用要求同线程 |
| **`pet/hit_mask.rs`** | **v0.8.0 新增**：桌宠点击穿透。持 32×32 alpha 命中网格，55ms 轮询全局光标（Win `GetCursorPos` / macOS `CGEventGetLocation` / Linux `x11rb query_pointer`），仅命中状态翻转时切换 `set_ignore_cursor_events`。**v1.2.12（整改 1.4）**：三平台 `cursor_logical` 改 `pub(crate)` 供拖拽循环复用；Linux 分支加 **X 连接缓存**（`thread_local!` + `Arc<RustConnection>`，失效即弃、下轮重连）—— 旧写法每次调用都新建连接，被轮询以 ~18Hz 触发。<br>⚠️ `RustConnection`（x11rb 0.13.2）**没有实现 `Clone`**，`conn.clone()` 在 Linux 上直接编不过；该分支只能靠 `.tmp-probe-linux-cursor` 探针交叉 check（整包交叉 check 会被 libdbus-sys 的 build.rs 假失败） | 桌宠点击可交互性；误改会导致「点不到 / 挡路」；拖拽循环的光标读取也走这里 |
| **`pet/drag.rs`** | **v1.2.12 新增（整改批次 2）**：macOS / 兜底路径的桌宠拖拽循环。`begin_drag` 起一条 8ms 线程，读**全局光标**（不再依赖 webview 收到 pointermove）自己 `set_position`，两轴位移都 <4px 时不发；`end_drag` 停循环并回传最终逻辑位置给前端写持久化。会话号 `generation` 让新的 `begin_drag` **接管**旧循环 | 桌宠拖拽流畅性。<br>⚠️ 循环里**不得出现窗口 getter**（`outer_position()` 等）：getter 走「发事件 + 阻塞等回」，在非主线程上会把主线程拖住；`set_position` 则是投递到主线程执行，安全。详见本文件 §9 |
| **`window_bounds.rs`** | **v1.2.12 新增（整改 1.3）**：跨窗口共用的离屏几何 util（`monitor_rects` / `rect_visible` / `offscreen_target` / `clamp_to_visible_area` / `rect_from_logical`）。判「可见」要**两轴各 ≥1/3 重叠**；`offscreen_target` 是纯函数，便于单测 | 悬浮指标条与桌宠的位置恢复；改阈值会让 v1.2.1 那类 `(-695,159)` 陈旧坐标重新放行 |
| **`system_load/`** | **v0.7.5 新增**：`MetricsSampler` 统一指标采样器，产原始系统指标（CPU / 内存 / 磁盘 / 网速，1s 采样、磁盘 30s 缓存）。**三端统一**只由「悬浮指标条」展示（v0.7.11 起；原 macOS 菜单栏文字指标 `set_title` 已整体移除）。**v0.8.0**：`fullscreen.rs` 补 macOS 真全屏识别（`CGWindowListCopyWindowInfo` + `CGDisplayBounds`） | 只影响悬浮指标条与桌宠全屏隐藏，不影响采样主流程 |
| `logging.rs` | 统一日志订阅器 | 日志路径 / 级别 |

> **`classifier.rs` 与 `categorizer.rs` 是两个不同东西**，勿混淆：
> `classifier` 按**用户定义的规则**匹配（纯计算、可单测）；`categorizer` 是规则都没命中时的**兜底**
> （本地字典 → 联网 Wikipedia → `other`），且**必须保持同步实现**（v0.4.1 死锁教训，见 §9）。

### 3.2 前端结构（src/）
| 目录 | 职责 |
|------|------|
| `src/api/` | Tauri invoke 的薄封装（tracker / config / db / screenshot） |
| `src/views/` | 主窗口 4 个页面（Dashboard/Trends/Rules/Settings） |
| `src/components/` | 共用组件（Chart 系、AppIcon、DailyBarChart 等） |
| `src/pet/` | **桌宠子系统**（独立 webview） |
| `src/screenshot/` | **截图子系统**（独立 `capture` webview，`CaptureOverlay.vue`） |
| `src/lib/` | 通用工具（logger、format 等） |
| `src/types.ts` | **前后端对接**类型（snake_case 镜像 Rust struct） |

### 3.3 桌宠子系统（src/pet/）

桌宠是独立透明置顶 webview，与 `main` 主窗口经 Tauri 全局事件 + `localStorage` 跨窗口同步（见 §7.4）。当前共 4 个窗口：`main`（设置）、`pet`（桌宠本体）、`pet-menu`（右键菜单，v0.6.2-beta.17 起独立窗口）、`capture`（截图遮罩，v0.8.0 新增）。
```
src/pet/
├── PetWindow.vue          ← 桌宠 webview 根：拖拽 / 交互 / 监听 / 路由（381 行；v0.8.0 接入命中网格）
├── PetMenuWindow.vue      ← 右键菜单**独立窗口**根（v0.6.2-beta.17 起独立成窗，119 行）
├── components/
│   ├── PetSkinRenderer.vue      （皮肤路由器：:is 动态组件 + :key 强制重建）
│   ├── PetContextMenu.vue        （右键菜单主体：皮肤切换 / 喂食 / 状态，855 行最重）
│   ├── PetCanvas.vue             （原 2D 渲染，150 行）
│   ├── PetBody.vue / PetLayer.vue（部件叠加层）
│   ├── PetBubble.vue             （气泡对话，104 行）
│   ├── PetSpriteEditor.vue       （部件编辑器，546 行）
│   └── PetPreviewStage.vue       （编辑器预览，63 行）
├── composables/
│   ├── usePetDrag.ts             （拖拽：越 4px 阈值才交 OS startDragging）
│   ├── usePetCursorPassthrough.ts（透明背景 + 屏蔽原生右键 + 初始 passthrough；整窗穿透开关部分 v0.8.0 起由 usePetHitMask 接管）
│   ├── usePetHitMask.ts          （v0.8.0 新增：合成 32×32 alpha 命中网格上报 Rust，透明区自动穿透）
│   ├── usePetInteractions.ts     （点击：跳跃 / 压扁 / 抖动）
│   ├── useForegroundWatcher.ts   （每 1s 轮询 tracker.current()，v0.8.0 由 2s 提速）
│   ├── usePetSprites.ts          （自定义部件合成）
│   ├── usePetBadges.ts           （徽章状态）
│   ├── usePetBubble.ts           （气泡文案 / 随机弹出）
│   └── useSystemOverloadWatcher.ts（系统过载 → 桌宠升温抖动，v0.7.0 阈值已上调）
├── engine/                     （未改动）
│   ├── stateMachine.ts          （16 状态 → 四元组）
│   ├── appToState.ts
│   ├── stateIcons.ts
│   └── spriteLayout.ts          （部件坐标唯一数据源）
├── config/bubble-phrases.json   （气泡台词词条）
├── assets/sprites/              （23 个旧版部件精灵：eye_* / mouth_* / brow_* / 装饰）
├── stores/petStore.ts          （状态 + localStorage 持久化，289 行）
├── types.ts
└── skins/                      ← 解耦皮肤层（注册表模式，即插即用）
    ├── types.ts                （PetSkinManifest 接口）
    ├── registry.ts             （注册表 + 持久化 + 订阅，137 行）
    ├── index.ts                （引导：注册内置皮肤）
    ├── popmart3d/              （Pop Mart 3D 熊猫）
    │   ├── PopMartPandaPet.vue
    │   └── assets/            （14 个文件：single / transparent / body / eyes / mouth /
    │                           nose / frame-0~5 待机帧 / reference.jpg / debug-stack）
    └── spiderman/              （蜘蛛侠）
        ├── SpiderManPet.vue
        └── assets/            （10 个：frame-0~5 + pose-{hero,swing,web,crouch}）
```

> 早期"4 层精灵拼贴"（body/eyes/mouth/nose 分别切图）因切片羽化导致五官割裂，已废弃；现行方案为**单张透明 PNG + 表情/姿势精灵叠加**。旧规格 `docs/pet-assets-manifest.md` 已从仓库移除（仅本地保留）。

> ⚠️ **本节文件树曾滞后于实际结构**（2026-09-04 同步）：此前未收录 `PetMenuWindow.vue`、
> `PetBubble.vue` 与 `usePetBadges` / `usePetBubble` / `useSystemOverloadWatcher` 三个 composables；
> 且 `popmart3d` 描述为"单张透明 PNG"，实际目录含 14 个素材文件（v0.7.1 为修复"皮肤源码资产
> 缺失入库"而补齐）。**以实际目录为准。**

---

## 4. 数据流示例

### 4.1 主窗口实时指示（已修的 HIGH bug）
```
Rust: get_current_foreground() → CurrentForegroundOut {
  name: String, process_name, window_title?, category_id,
  idle_seconds: u64, session_seconds: u64, tracking: bool
}
（snake_case 原样序列化）

↓ invoke

src/api/tracker.ts:        tracker.current() = invoke('get_current_foreground')

↓ ref

src/App.vue:               live.value.process_name / window_title / session_seconds
                           （注意是 snake_case，不是 camelCase）
```

### 4.2 桌宠状态联动
```
tracker.current() 每 2s 轮询
    ↓
composables/useForegroundWatcher.ts
    ↓ 匹配规则 engine/appToState.ts
petStore.setState(newState)
    ↓ reactive 传播
PetWindow.effectiveState (computed)
    ↓ props
PetSkinRenderer.state
    ↓ :is 动态组件
当前激活皮肤的 renderer(state)
```

### 4.3 皮肤切换（v0.6.2 NEW）
```
用户在右键菜单点击"Pop Mart 3D"
    ↓
PetContextMenu.onPickSkin('popmart-3d')
    ↓
skinRegistry.setActive('popmart-3d')
    ↓ 持久化 localStorage['screentime-pet-skin']
    ↓ notify listeners
PetSkinRenderer.watchEffect → skinTick++
    ↓ :key 强制重建 <component :is>
新皮肤的渲染器接管渲染
    ↓
旧皮肤的实例被销毁（无资源需清理，皮肤组件目前无状态）
```

---

## 5. IPC 契约（当前 91 个命令）

Rust 端在 `commands.rs` / `pet/` 定义，**在 `lib.rs` 的 `generate_handler!` 注册**；前端通过 `src/api/*` 调用。**改 IPC 必须同时改两端，并记得注册。**

| 类别 | 命令 | 说明 |
|------|------|------|
| 追踪 | `start_tracking` / `stop_tracking` / `is_tracking` | 启动/停止/查询采样循环 |
| 实时 | `get_current_foreground` | 当前前台应用（v0.5→v0.6.1 字段名 bug 已修） |
| 数据 | `get_overview` / `get_daily_summaries` / `get_daily_categories` / `get_month_summary` / `get_hourly_buckets` / `get_app_ranking` / `get_sessions` | Dashboard / Trends 用 |
| 趋势 | `get_trends` | 周 / 月同比 |
| 规则 | `get_rules` / `add_rule` / `update_rule` / `delete_rule` / `reclassify_all` | 归类规则 CRUD + 一键重算历史 |
| 分类 | `get_categories` | 分类字典 |
| 设置 | `get_settings` / `save_settings` / `set_idle_threshold` / `get_idle_threshold` | 配置与空闲阈值 |
| 自启 | `set_autostart` / `is_autostart` / `get_autostart_pref` | 开机自启 |
| 数据管理 | `export_all` / `export_data` / `import_data` / `prune_data` / `backup_and_prune_device` / `get_backup_config` / `save_backup_config` / `run_backup_now` | 导出导入、清理、本地自动备份 |
| 多设备 | `get_devices` / `list_devices_with_stats` | 多设备合并 |
| 桌宠（`pet::`） | `create_pet_window` / `show_pet_window` / `hide_pet_window` / `move_pet_window` / `set_pet_cursor_passthrough` / `create_pet_menu_window` / `show_pet_menu_window` / `hide_pet_menu_window` / `move_pet_menu_window` | 桌宠窗口 + 右键菜单窗口（共 9 个） |
| 桌宠命中（`pet::hit_mask`） | `set_pet_hit_mask` / `set_pet_drag_lock` / `clear_pet_hit_mask` | **v0.8.0**：32×32 alpha 命中网格上报 / 拖拽期锁定穿透 / 清空（共 3 个） |
| 桌宠拖拽（`pet::drag`，**v1.2.12**） | `begin_drag` / `end_drag` | macOS 与「原生拖拽不可用」兜底路径的**Rust 侧拖拽循环**：`begin_drag{grabDx,grabDy}` 记抓取偏移并起 8ms 循环（读全局光标搬窗），`end_drag` 停循环并回传**最终逻辑位置**（前端据此写持久化）。<br>⚠️ `end_drag` 回的是循环自己记的最后一个目标值，**不是** `outer_position()` 的查询结果 —— `set_position` 异步投递，刚投出去的几帧还没落到 NSWindow，去问会拿到历史值，写进持久化就是「拖完回跳」的老毛病。**入参是 camelCase**（Tauri v2 的 `snake_case` 参数 ↔ JS `grabDx`）。 |
| 截图（`screenshot::`） | `screenshot_get_config` / `screenshot_set_config` / `screenshot_trigger` / `screenshot_frame` / `screenshot_commit` / `screenshot_cancel` / `screenshot_list` / `screenshot_delete` / `screenshot_reveal` / `screenshot_thumbnail` / `screenshot_dir` / `screenshot_ocr` / `screenshot_copy_text` / `take_capture_ready` | **v0.8.0**：配置 / 触发 / 取整屏帧（标注背景）/ 提交（收前端合成 PNG）/ 取消 / 历史增删查 + 缩略图 + 目录（共 11 个）。`set_config` 返回 `ApplyResult`（快捷键真实注册结果）。<br>**v0.8.1**：+`screenshot_ocr`（对缓存帧的指定**物理像素**矩形做本地离线 OCR，入参 `x/y/width/height`，返回 `OcrOut{text,width,height,language}`）/ `screenshot_copy_text`（走 `arboard` 写系统剪贴板，避开 WebView 剪贴板权限）。<br>**v1.2.11**：+`take_capture_ready`（`.take()` 取走并清空 `ScreenshotState.pending_capture`，返回 `Option<CaptureReadyPayload>`）—— **这不是可选的便利接口，而是首次截图半屏的修复本体**：遮罩窗 `visible(false)` 懒创建时 WebView 还在加载 SPA，`emit_to` 即发即弃而监听器未注册，事件会被静默丢弃；前端必须在 `listen` 注册完成后**主动拉一次**兜底。详见 [`RELEASE.md`](RELEASE.md) §3.4(c)。合计 **14** 个 |
| 系统指标 | `get_system_metrics` / `get_system_metrics_enabled` / `set_system_metrics_enabled` / `get_status_bar_config` / `set_status_bar_config` | CPU/内存/磁盘/网速，三端统一由悬浮指标条展示（v0.7.11 起） |
| 悬浮指标条（`float_window::`） | `create_float_window` / `show_float_window` / `hide_float_window` / `move_float_window` / `resize_float_window` | 悬浮窗生命周期与位置尺寸（共 5 个） |
| 权限 | `check_permissions` / `open_privacy_settings` | macOS 辅助功能 + 屏幕录制。`check_permissions` 返回 `{ accessibility, screen_capture }`（`db/models.rs:107`），**v1.2.10 起屏幕录制改双信号口径**（`CGPreflight` ∨ 窗口标题探针）与截图侧一致 —— 单信号会把「升级后授权失效」误判成「从未授权」。设置页**权限中心**（仅 macOS）复用它，刻意不轮询（未放行时会 fork `codesign`/`xattr`，1Hz 代价过高） |
| 环境 | `check_webview2` / `open_webview2_download` / `reveal_path` | Windows WebView2 检测等 |
| 元 | `check_for_update` / `open_url` / `export_logs` / `get_log_size` / `get_log_dir` | 版本告知、日志导出 |
| 应用内升级（`app_update::`，**v1.2.11**） | `app_update_supported` / `check_app_update` / `install_app_update` | Tauri 官方 updater（ed25519 验签）。**与上面的 `check_for_update` 是两套并存的东西，别混用**：`check_for_update` 只「告知」有新版本并跳浏览器（macOS / deb / rpm 只能走这条）；`app_update::*` 才真正下载并安装（仅 Windows nsis / Linux AppImage）。<br>平台门控在 `install_supported()`：`bundle_type()` 是打包时 patch 进二进制的静态变量，**开发态为 `None`**，所以 `cargo run` 下判为不支持 —— 这是保守且正确的行为，不是 bug。<br>⚠️ **Windows 装完进程立即退出**（upstream `install_inner` 在 `ShellExecuteW` 后直接 `process::exit(0)`），`install_app_update` 的 Promise 永不 resolve，前端不得写「安装成功」提示；Linux 则由 Rust 侧装完调 `app.restart()`。 |

完整列表（权威）：`src-tauri/src/lib.rs` 的 `.invoke_handler(tauri::generate_handler![...])`。

> ⚠️ **本节曾长期与实际代码不符**，以下旧写法会直接报「命令不存在」，勿再沿用：
> `list_rules` → 实际 `get_rules`；`get_today_summary` → 实际 `get_overview`；
> `get_app_summaries` → 实际 `get_app_ranking`；`set_pet_passthrough` → 实际 `set_pet_cursor_passthrough`；
> `get_log_path` → 实际 `get_log_dir`。

---

## 6. SQLite 表结构（sql/schema.sql 为唯一权威）

数据库由 `rusqlite` bundled 在首次启动时按 `sql/schema.sql` 自动建表；`seed_categories.sql` / `seed_rules.sql` 注入初始分类与归类规则。**以 `sql/schema.sql` 为准**，下表为当前结构：

| 表 | 关键列 | 用途 |
|----|--------|------|
| `apps` | id, name, process_name(UNIQUE+platform), exe_path, icon_blob, category_id, platform | 应用主表（每进程每平台一行） |
| `sessions` | id, app_id(FK), start_at, end_at, duration_seconds, date, window_title, device | 主采样表（append-only，每次使用一段） |
| `daily_summaries` | id, date, app_id, total_seconds, session_count(UNIQUE date+app_id) | 日聚合缓存（Dashboard/Trends 用） |
| `categories` | id(TEXT PK), name, color | 归类字典（social/productivity/...） |
| `classification_rules` | id, field(process_name/window_title/exe_path/bundle_id/name), match_type(contains/equals/prefix/suffix/regex), pattern, category_id, priority, enabled | 自动归类规则（约 40 条种子） |
| `settings` | key(TEXT PK), value | 简单键值配置（如 autostart 用户偏好、截图配置 `screenshot_config`） |
| **`screenshots`** | id, file_name, width, height, bytes, created_at, device | **v0.8.0 新增**：截图历史元数据（文件实体在 `<app_data_dir>/screenshots/`，FIFO 超限移入系统回收站；索引 `idx_screenshots_created`） |

**注意**：桌宠状态（开关/位置/喂食/好感度）**不落 SQL**，由前端 `petStore` 经 Pinia persist 写入 `localStorage`，跨窗口通过 `localStorage` 共享。**新功能若需持久化表，先在 `sql/` 加迁移文件，db/mod.rs 注册到迁移队列。**

---

## 7. 桌宠皮肤系统（v0.6.2 解耦）

### 7.1 接口契约
```ts
interface PetSkinManifest {
  id: string;                          // 例 'popmart-3d' / 'spiderman'（当前内置两套皮肤）
  name: string;                        // UI 展示名
  version: string;
  description?: string;                // tooltip 用
  renderer: Component<{ state: PetState }>;  // 必须接 state prop
  decorations?: Partial<Record<PetState, string[]>>;  // 可选
}
```

### 7.2 渲染协议
```
PetWindow.vue
  └─ <PetSkinRenderer :state :is-dragging :class="animClass" />
       └─ <component :is="skinRegistry.active().renderer" :key="skinTick" :state />
            └─ 每个皮肤声明自己的 state 响应逻辑（props 透传）
```

`is-dragging` 和 `animClass` 通过 Vue `inheritAttrs` 自动落到皮肤根元素，**所有皮肤自动继承点击动效和拖拽状态**。

### 7.3 解耦边界（"不要影响"清单）

修改以下文件**不会**影响新皮肤机制：
- 任何已有的 `components/Pet*.vue`（PetCanvas/Body/Layer/SpriteEditor/PreviewStage）
- `engine/*`
- `composables/*`
- `stores/petStore.ts`
- `types.ts`

修改这些**只影响皮肤层**（无需改前端主流程）：
- `skins/<name>/*`
- `skins/registry.ts`（接口扩展时）
- `skins/index.ts`（注册新皮肤时）

修改这些**影响所有皮肤**（慎动）：
- `components/PetSkinRenderer.vue`（协议）

### 7.4 多窗口状态同步（跨 webview 陷阱）

桌宠涉及 3 个 Tauri 窗口 / webview：`main`（设置）、`pet`（桌宠本体）、`pet-menu`（右键菜单）。**每个 webview 各持一份 `petStore` 模块级实例**（仅 `localStorage` 共享），彼此不自动同步。跨窗口同步靠 Tauri 全局事件 + `petStore.reload()` 重读 `localStorage`：

| 事件 | 触发方 | 监听方动作 |
|------|--------|------------|
| `pet-skin-changed` | Settings 切皮肤 | PetWindow / pet-menu 调 `skinRegistry.reloadActive()` |
| `pet-custom-updated` | 编辑器改表情/素材 | PetWindow `reloadBadges` / `reloadConfig` |
| `pet-store-updated` | 菜单改状态/喂食/位置 | PetWindow `petStore.reload()`（不读 `state`，避免覆盖前台自动状态） |

> 改任何"某窗口写 store、另一窗口需看到"的逻辑，必须广播对应事件并在目标窗口 `reload`，否则出现"菜单操作桌宠不反应"类 bug。

### 7.5 拖拽与渲染性能

- **拖拽跟手**：越过 4px 阈值才调用 Tauri 原生 `getCurrentWindow().startDragging()`，把拖拽交给 OS（零 IPC、零延迟）；未越阈值走点击交互，不吞单击/双击。
- **透明置顶窗性能**：投影/滤镜**绝不**与 `transform` 动画放在同一元素（filter 会破坏 GPU 合成层，每帧重算投影）；投影放静态 `img` 层一次性光栅化，动画元素只做 `transform`。升温/抖动用 `transform` 抖动而非 `hue-rotate` 滤镜。

---

## 8. 扩展指南

### 8.1 新增一个宠物皮肤
```bash
1. mkdir src/pet/skins/<my-skin>/
2. 创建 <MySkin>.vue（含 <script setup> export default { props: { state: PetState } }）
3. 创建 assets/（背景图、装饰浮层等）
4. 创建 index.ts:
      import type { PetSkinManifest } from '../types';
      import MySkin from './MySkin.vue';
      export default { id: 'my-skin', name: 'My Skin', version: '0.x', renderer: MySkin };
5. 在 src/pet/skins/index.ts 加一行：skinRegistry.register(myManifest);
6. 完成。自动出现在右键菜单"皮肤"切换区。
```

### 8.2 新增一个"类似桌宠"的小部件（如待办、便签、番茄钟）
建议照搬 `skins/` 的模式，扩展为 `widgets/`：
```bash
src/widgets/
├── types.ts                 // WidgetManifest 接口
├── registry.ts              // 同样的注册模式
├── index.ts                 // 引导
├── todo/                    // 待办小部件
├── pomodoro/                // 番茄钟
└── ...
```

**接口契约**（建议初稿）：
```ts
interface WidgetManifest {
  id: string;
  name: string;
  size: { width: number; height: number };
  mount(el: HTMLElement, ctx: WidgetContext): void;
  update?(state: object, ctx: WidgetContext): void;
  unmount?(el: HTMLElement): void;
}
```

桌宠是"悬挂在桌面的任意 webview"，未来 widget 可走**同一 Tauri 窗口 + 同一 PetWindow 协议**，只是渲染层挂不同组件。

### 8.3 跨进程持久化新表
1. `sql/<feature>.sql` 写迁移
2. `db/mod.rs::migrate()` 注册该文件
3. `db/models.rs` 加对应 DTO
4. `commands.rs` 加 `#[tauri::command]` 暴露
5. `src/types.ts` 镜像同名 DTO（snake_case）
6. `src/api/<feature>.ts` 加 wrapper

---

## 9. 关键不变量（踩坑沉淀）

| 不变量 | 为什么 |
|--------|--------|
| 采样循环**绝不**嵌套 `block_on` 同步函数 | v0.4.0 → v0.4.1 死锁；改 `spawn_blocking` / `reqwest::blocking` |
| 命令**返回值** snake_case，命令**参数** camelCase | Tauri v2 行为：仅参数转换；`types.ts:66` 旧注释是错的 |
| `classifier` 比较时 `value` 与 `pattern` **两侧都小写化** | 只小写一侧则 `equals` 永不命中 → "规则没生效"（v0.6.2-beta.17） |
| 所有 `Mutex::lock()` → `unwrap_or_else(|e| e.into_inner())` | poison 雪崩 |
| macOS 权限用 `AXIsProcessTrustedWithOptions({prompt:false})` | ad-hoc 签名下 `AXIsProcessTrusted` 因 identifier 漂移永远返回 false |
| 日志走 `logging.rs` + tauri-plugin-log | 散打日志抓不到；前后端同文件 |
| `usePetSprites` 的 customCompositions **目前**未被 `PetCanvas` 消费 | v0.6.1-beta.1 已知断点（编辑器保存≠桌宠实际生效） |
| 单一坐标数据源 `spriteLayout.ts` | 防止预览/实宠错位（v0.6.0-beta.1 editor bug 教训） |
| 非主线程的循环里**只准投递、不准查询**窗口状态（`set_position` 可以，`outer_position()` 不行） | tauri-runtime-wry 2.11.4 `send_user_message`（lib.rs:235-255）在主线程**内联执行**、否则 `proxy.send_event` **fire-and-forget**；而 `getter!`/`window_getter!`（:197-211）一律「发事件 + `rx.recv()` 阻塞等回」。拖拽循环（`pet/drag.rs`）因此在 `set_position` 之后**绝不**回读窗口，收尾位置用循环自己记的目标值（v1.2.12） |
| 桌宠拖拽期窗口保持 100% 静态（`.is-dragging :deep(*){animation:none}`） | v0.9.0 为 DWM 重合成抖动加的；为做拖拽动画拆它会当场复现「拖动时整窗抽搐」 |
| 文件头部必带修改历史 (`- YYYY-MM-DD @vX.Y.Z: 类型 - 说明`) | 防 agent 幻视（CONVENTIONS §1.1, §2） |

---

## 10. 约定（详见 [`CONVENTIONS-screentime-pro.md`](../CONVENTIONS-screentime-pro.md)）

- **命名分层**：DB 列 `snake_case` / Rust 字段与函数 `snake_case` / IPC 参数 camelCase↔snake_case 自动转 / **IPC 返回值原样 snake_case** / 前端内部 `camelCase`；类型 PascalCase / 常量 UPPER_SNAKE
- **注释**：所有源码强制中文（公开 API、关键设计、复杂算法、行内、注释、TODO/FIXME）
- **文件头**：必带修改历史（`YYYY-MM-DD @vX.Y.Z: 类型 - 说明`）
- **版本同步点**：`src-tauri/tauri.conf.json` 的 `version` 为唯一真实来源；UI 硬编码版本号用 `app.getVersion()`。**`Cargo.toml` 的 version 不参与发布，勿顺手对齐**
- **发布红线**：构建成功 ≠ 可发布；**必须等用户明确「发布」** 才能 push/tag/release。流程见 [`RELEASE.md`](RELEASE.md)

> 本条在 v0.7.5 之前写的是「详见 CONVENTIONS.md」，但该文件从未存在（死链）。
> 项目专属约束已于 2026-09-04 建立，路径为仓库根 **`CONVENTIONS-screentime-pro.md`**；
> 与 `AGENTS.md` / `CHANGELOG.md` 同属约束类文档，已由 `.gitignore` 排除，不传 GitHub。

---

## 11. 文件清单速查

```
根目录（约束类文档，已 gitignore，不传 GitHub）
├── CONVENTIONS-screentime-pro.md ← 项目专属代码约束（本空间首要约束，见 §10）
├── CHANGELOG.md                  ← 本地详细变更留底
└── AGENTS.md                     ← （建议补）项目级 Agent 启动规则

src-tauri/
├── Cargo.toml           ← Rust 依赖（version 为 0.1.0，**不参与发布**，勿对齐）
├── tauri.conf.json      ← VERSION IS HERE（唯一真实来源，决定 CI tag 与产物名）
└── src/
    ├── main.rs          ← App 入口（仅转调 lib）
    ├── lib.rs           ← Tauri Builder / 插件（含 global-shortcut）/ generate_handler! 命令注册 / 托盘
    ├── commands.rs      ← 主 IPC 命令路由（48 个）+ float_window（3）+ pet/（14：窗口/菜单 9 + hit_mask 3 + drag 2）+ screenshot/（22）+ app_update（3）+ float_glass（1）= 91
    ├── pet/             ← 桌宠 Rust 端（window / menu_window / hit_mask / drag）
    ├── screenshot/      ← v0.8.0 截图 Rust 端（mod：捕获 / 下发整屏帧 / 解码落库 / 快捷键；合成在前端 canvas）
    ├── tracker/         ← 平台采样（mod / platform / macos / windows / linux）
    ├── db/              ← SQLite（mod 1236 行 / models）
    ├── classifier.rs    ← 分类规则引擎（纯逻辑，不依赖 DB）
    ├── categorizer.rs   ← 兜底归类：字典 + Wikipedia + LRU（必须同步实现）
    ├── system_load/     ← 系统指标采样（mod / macos / linux / windows / metrics / network / fullscreen），三端统一由悬浮指标条展示
    ├── logging.rs
    ├── window_bounds.rs  ← v1.2.12 离屏几何 util（float_window 与 pet 共用；可见性判定 + 回落坐标）
    └── error.rs

src-tauri/capabilities/
├── default.json         ← 主窗口（main）权限集
├── pet.json             ← 桌宠窗口；**v1.2.12 补 `core:window:allow-start-dragging`**（缺它时 `startDragging()` 被 ACL 拒，原生拖拽从未真正生效）
├── pet-menu.json        ← 桌宠右键菜单窗权限集
├── float.json           ← 悬浮指标条（`allow-start-dragging` 早已存在，故它的原生拖拽一直可用）
└── capture.json         ← v0.8.0 截图窗口（capture）权限集

src/
├── App.vue              ← 主窗口 / 桌宠 / 截图 分支根
├── types.ts             ← DTO 镜像（snake_case，Rust struct 原样）
├── api/                 ← invoke 包装 + mock（无 Tauri 环境调试用；含 screenshot.ts）
├── views/               ← 主窗口 4 页（Settings 最重）
├── components/          ← 通用组件 + 图表
├── pet/                 ← 桌宠子系统（含 skins/ 两套皮肤）
├── screenshot/          ← v0.8.0 截图子系统（CaptureOverlay.vue）
├── i18n/                ← vue-i18n 词条（zh-CN / en-US，必须双语同步）
├── utils/               ← 格式化
└── lib/                 ← logger

docs/                    ← ⚠️ .gitignore 默认排除 docs/*.md，仅以下两份白名单入库
├── ARCHITECTURE.md      ← 本文件（工程面）  [入库]
└── RELEASE.md           ← 版本管理与发布流程（唯一权威）  [入库]

# 以下为本地工作目录（已 gitignore，不入库，仅本机参考）：
#   release/v0.x.y/      历史版本归档与 Release Notes（本地草稿）
#   output/              本地构建产物 / 安装包
# 计划/规格类文档（pet-assets-manifest / I18N_PLAN / v060-pet-plan 等）已移出仓库，仅本地保留。

sql/                     ← SQLite 迁移（已恢复入库，为数据库唯一权威）
.github/workflows/build.yml ← 三平台 CI，push v* tag 触发
```

> **往 `docs/` 新增需公开的文档时，必须同步在 `.gitignore` 加白名单例外**
> （当前仅 `!docs/ARCHITECTURE.md`、`!docs/RELEASE.md`），否则会被 git 静默忽略——
> `RELEASE.md` 此前"被引用却不存在"正是这个原因。

---

## 12. 本地自测入口（均在本地工作目录，已 gitignore，不入库）

| 路径 | 用途 |
|------|------|
| `output/` | 本地构建产物、安装包、临时调试文件 |
| `release/v0.X.Y/` | 本地 dmg / AppImage / deb 自测包与 Release Notes 草稿 |
| 各版本 `NOTES.md` | 发布说明草稿（**需人工**回填到 `build.yml` 的 `releaseBody`，见下） |

> ⚠️ `build.yml` 的 `releaseBody` 是**硬编码死文本**，不会自动跟随版本。
> 当前仓库三份（windows / linux / macos job）为 **v0.9.2 的文案**（发版时已同步）。
> 每次发版前必须手动替换为本版内容，否则线上 Release Notes 显示旧版本的内容。
> 模板与完整流程见 [`RELEASE.md`](RELEASE.md) §3.1 / §5。

---

**遇到不确定**：先读本文件 §3 模块地图 + §8 扩展指南，再动手改。改完务必补 §10 注释与修改历史。
