# ScreenTime Pro — 分步整改方案（2026-10-29）

> 依据：2026-10-29 架构核查 + 缺陷复核结论。每条都带 `文件:行`，均为**逐行读码确认**项；
> agent 报告里我复核后否掉的条目不列入本方案（见文末「已排除」）。
> 现状前提：`package.json`/`tauri.conf.json` 均为 **1.2.11**，`git tag` 无 v1.2.11（未发版），
> 工作区有 16 文件 / +1698 行的**未提交 WIP**（OCR 运行库自检 + 截图遮罩竞态修复）。

---

## 0. 总原则

1. **先止血 WIP，再动桌宠。** 本批 P1 里有 4 条就在未提交的 WIP 里；发版即回归，代价最高。
2. **一次批次一个 commit，桌宠改动与 OCR/截图改动绝不混提。** pet 相关文件不在当前 WIP 的 16 个文件内，天然可隔离。
3. **每批次必须过既有门禁**：`cargo fmt` + `cargo clippy -- -D warnings` + `cargo test` + 前端 `vue-tsc --noEmit`；
   涉真机的走 `scripts/cdp.py`（WebView2 CDP 打真实鼠标事件 + 像素比对），不新造工具。
4. **改动前先看是不是竞态类**。本项目历史上三次（v1.2.5 / v1.2.6 / v1.2.7）都是"修竞态反而引入数据损坏"，
   所以批次 0 每条都要求"能写出一个失败测试再修"。

---

## 批次 0 · v1.2.11 发版前必修（预计 0.5 天，范围：当前 WIP 的 16 个文件）

| # | 缺陷 | 位置 | 改法 | 级别 |
|---|---|---|---|---|
| 0.1 | 缺 VC++ 运行库时，随包 `onnxruntime.dll` 被判"损坏"删除；Windows 无下载通道 → 只能重装 | `screenshot/ocr_engine.rs:247-267`（`remove_file`）+ `:357`（`probe_ort_api`） | ①删除权限收窄到 `user_ort_dir`，随包根目录失败**只跳过不删**；②`Library::new` 的失败原因分类：依赖缺失(ERROR_MOD_NOT_FOUND) ≠ 文件损坏，前者保留"请安装 VC++ 2015-2022"提示且不删文件 | **P1 破坏性** |
| 0.2 | 遮罩开着再按快捷键 → 新帧被吞；用户在旧图上框选、OCR 裁的是新帧，导出旧图 | `src/screenshot/CaptureOverlay.vue:1223`（`if (sessionStarted) return`）、`:1259`；`screenshot/mod.rs:845-848` | 去重从布尔改成 **Rust 侧随 payload 自增的 `seq`**；前端记 `lastConsumedSeq`，`seq` 不同即必须重启会话；且只有 `startSession` 真正开始后才 take 槽位 | **P1** |
| 0.3 | 取消/提交后上一轮 `takeReady()` 载荷迟到回灌 → `sessionStarted` 残留 true → 下一次截图白屏 | `CaptureOverlay.vue:1094-1098`（cancel）、`:1104-1110`（commit）、`:1267`（onMounted 拉取） | 同 0.2 的 `seq` 机制统一覆盖；`endSession()` 顺带 `invoke` 清 Rust 槽位 | **P1**（与 0.2 同族，一次改掉） |
| 0.4 | 多盘枚举是死代码，且 macOS 版接上必然返回空 | `system_load/platform/mod.rs:24,57`；`platform/macos.rs:290-296`；生产调用者为 0（仅 `linux.rs:230/262/269/281`、`windows.rs:172/186/209` 测试块） | 二选一：**本轮不交付**（release note 删掉该条），或改 `MNT_LOCAL` 判定 + 按 `f_fsid` 对 APFS 容器去重、保留 `/` 与 `/System/Volumes/Data`，并接进 `get_system_metrics` | **P1 交付口径** |
| 0.5 | `cargo check --lib` 出 3 条 dead_code → 若 CI 带 `-D warnings` 直接断链 | 同上 | 随 0.4 一并解决（接进生产路径或 `#[cfg(test)]` 收口） | P1 |
| 0.6 | `ORT_DYLIB_PATH` 逃生通道被自己重新堵死，与注释矛盾 | `ocr_engine.rs:239-241`（注释）vs `ocr_onnx.rs:115-124`（`ensure_env` 再跑一遍闸门） | 闸门只在 `resolve_ort_lib` 内生效；`ensure_env` 跳过来自 env 的路径 | P2 |
| 0.7 | `panic="abort"` 时期遗留在 `spawn_blocking` 任务里的 panic 会 abort 全进程，注释不成立 | `ocr_engine.rs:872-874,902`；panic 站点 `mod.rs:1143,1174` | 把 panic 点从 `spawn_blocking` 挪回 `guard_ort_panic` 覆盖范围，或修正注释为"该路径仍不可恢复" | P2 |
| 0.8 | `panic::set_hook` 每调用一次追加一层钩子 → 日志按 OCR 调用次数重复打印 | `logging.rs:264`（叠加）+ `lib.rs:436`；`ocr_onnx.rs:198` | 加 `PANIC_HOOK_INSTALLED: Once`，或把 hook 安装全量收回 `lib.rs` 单点 | P2 |
| 0.9 | `common.error` / `common.loading` 两个 i18n key 中英文两侧都缺 → 界面渲染原始 key | `Settings.vue:1169,1185`；`DatePicker.vue:54`；`i18n/zh-CN.ts:4-13`、`en-US.ts` 的 `common` 段 | 两份各补 `error` / `loading` 词条（`error` 用于桌宠 IPC 失败弹窗标题） | P2 |
| 0.10 | `App.vue` 主窗初始化在所有 webview 无条件执行 | `App.vue:347-356`（无 label 门槛）；`mod.rs:812` 每次截图 `set_focus` 都会触发一遍 `check_permissions` | `onMounted` 顶部加 `if (isPetWindow \|\| isPetMenuWindow \|\| isFloatWindow \|\| isCaptureWindow) return;` | P2 |
| 0.11 | Settings 的 `await listen` unlisten 在卸载后才赋值 → 快切页监听器泄漏 | `Settings.vue:1102-1138`（首个 `await appUpdate.isSupported()` 拉长窗口）、`:2148-2155`（含 60 张缩略图加载后才注册） | `let disposed=false`；`onBeforeUnmount(()=>disposed=true)`；`const un = await listen(...); disposed ? un() : (unlistenX = un)` | P2 |

**验收**：批次 0.1/0.2/0.3 各写一个能失败的测试（0.1 用一份"体积/魔数都对但不可 dlopen"的假 dll；0.2/0.3 用 seq 单调性断言）；
Windows 真机卸掉 VC++ 运行库跑一次截图，确认 **dll 未被删除且提示文案正确**；遮罩开着连按两次快捷键，确认新帧刷新。
**提交**：一个 commit（`fix(v1.2.11): 发版前收口 …`），随 v1.2.11 发出。

### 批次 0 实施结果（2026-10-29，代码已落盘、未提交）

| # | 状态 | 说明 |
|---|---|---|
| 0.1 | ✅ 已改 + 4 测试 | `probe_ort_lib` 三态化（Usable / EnvBlocked / Unusable），删除权只给 `user_ort_dir` 下的自己下载件；`missing_dependency_is_not_corruption`、`only_prunes_the_downloaded_copy` 锁死回归 |
| 0.2 / 0.3 | ✅ 已改 | Rust 侧 `ScreenshotState::capture_seq`（AtomicU64）随 payload 下发 `seq`；前端 `lastSessionSeq` 单调判定替代布尔 `sessionStarted`；`screenshot_cancel` 清槽位。事件路径顺序也按方案改了：**先 `startSession(ev.payload)` 再 `takeReady()` 清槽**，清出来的载荷若序号更大就再开一轮（旧写法先 take 后 start 会把「遮罩开着时第二次按键」的那一帧直接丢弃）。**行为需真机验证**（本机是 Windows，遮罩链路要 macOS/Windows 双端跑） |
| 0.4 / 0.5 | ✅ 无需本轮改动 | WIP 已用 `#[allow(dead_code)]` + 交付口径推到 v1.2.12 收口；实测 `cargo clippy --all-targets` 现在 **0 条 dead_code/never used** |
| 0.6 | ✅ 已改 | `ensure_env` 只对 `Unusable` 提前返回 Err，`EnvBlocked` 放行给 `ort::init_from` → 「请安装 VC++ 2015-2022 运行库」这句提示第一次真正可达 |
| 0.7 | ✅ 已改（改的是注释） | 原定位的行号/文件对不上代码，重新定位到真缺陷：`Cargo.toml` 里 `libloading` 注释仍称 `[profile.release] panic = "abort"`，实际第 269 行是 `panic = "unwind"`；注释按当前事实重写 |
| 0.8 | ❌ **撤回，误报** | 全仓只有一个 `panic::set_hook`（`lib.rs:341`，在 `.setup()` 内，每进程一次），`logging::init` 也只在 `lib.rs:298` 调一次 → 不存在「按 OCR 调用次数叠加」。`logging.rs:264` 那处引用是行号漂移 |
| 0.9 | ✅ 已改 | `zh-CN.ts` / `en-US.ts` 的 `common` 段各补 `error` / `loading` |
| 0.10 | ✅ 已改 | `App.vue` 加 `isMainWindow`，非主窗 `onMounted` 直接 return |
| 0.11 | ✅ 已改 | 4 处 `await listen` 全部改 `const un = …; disposed ? un() : (unlistenX = un)`，`onBeforeUnmount` 首行置 `disposed = true` |

**门禁实测（本机 Windows）**：
- `cargo fmt`：通过。**副作用**：仓库此前不是 rustfmt-clean，本次 fmt 重写了 34 个文件，其中 **32 个在批次 0 范围外**（含 `pet/` 4 个，纯格式、零语义）→ 提交时把 fmt 改动单独放一个 `chore: cargo fmt` commit，别混进 0.x 修复。
- `cargo clippy --lib -- -D warnings`：**不通过，62 条**（46 条是中文 doc 注释的列表缩进风格 lint，16 条是既存代码风格：`sort_by_key`/参数过多/同型 cast/`div_ceil` 等）。**本轮新增 0 条**，本轮触碰过的文件里 `ocr_engine.rs`/`ocr_onnx.rs`/`platform/macos.rs` 已 0 warning；`screenshot/mod.rs` 剩 19 条全在模块头部 changelog（`//!` 29-73、622-623、957-958）。这条门禁从 HEAD 起就没绿过，清理属批次 4。
- `cargo test --lib`：**51 passed / 0 failed / 5 ignored**（本轮 +5 条新测试）。
- `vue-tsc --noEmit`：**0 错误**（本机没有独立 node，借 `ELECTRON_RUN_AS_NODE=1` 调 Qoder 自带 node v24.18.0 跑的）。
- `cargo check --target aarch64-apple-darwin --all-targets`：**跑不起来**（不是代码问题：`cc-rs` 找不到 `cc`，Windows 交叉编译缺 Apple 工具链）→ 本轮 macOS 侧改动（`platform/macos.rs` 的过滤谓词 + 兜底 + 2 条新测试）**只做过人工类型核对，没有任何编译器验证过**。在 Apple 机器上第一次 `cargo test --lib` 之前，不能算它成立。
- `build.yml` 里三处「`cargo test --lib` **46 passed** / 0 failed / 5 ignored」（133/275/412 行）是**手写文案、CI 并不执行 cargo test**，现已过时 → 发版前要么改成 51，要么去掉具体数字。

### 批次 0 遗留的真机验证项（不作为结论，发版前跑）
- 自检本身在 `catch_unwind` **之外**跑可疑 dylib（`ocr_engine.rs:363-385`，由 async 命令 `mod.rs:1402` 调用）：魔数+体积过关但段数据损坏时 dlopen 成功、调用即 SIGSEGV —— unwind 拦不住。macOS 造一份截断 dylib 实测。
- Windows 锁屏检测依赖前台名出现 `LockApp`/`LogonUI`，而系统常报 `explorer.exe`（`commands.rs:797`）→ 实测锁屏是否真的停止计时。macOS 侧已核对可用（`tracker/macos.rs:75-80`）。

---

## 批次 1 · 桌宠止血（预计 0.5 天，独立 commit）

| # | 内容 | 位置 | 说明 |
|---|---|---|---|
| 1.1 | **补 `core:window:allow-start-dragging`** | `src-tauri/capabilities/pet.json` | 一行。**这是全案最高性价比的一条**：`usePetDrag.ts:94` 调 `startDragging()` 被 ACL 拒 → 每次拖拽 catch → 走每帧 IPC 回退路径，即 v0.6.2-beta.28 声称"已根治"的那条路径其实从未生效。对照 `float.json:11` 已给该权限、`FloatBar.vue:493` 的原生拖拽可用。依据：`gen/schemas/acl-manifests.json` 里 `core:window:default` 全 28 项不含它。 |
| 1.2 | 启动恢复桌宠 | `src-tauri/src/lib.rs` setup + 前端 | 现状：`create/show_pet_window` 只在 `Settings.vue:1161/1179` 和 `PetWindow.vue:171` 被调用 → 重启后 `enabled:true` 但窗口不出现（开关还显示"开"）。改：setup 读持久化 enabled → create/show/move(clamped)。 |
| 1.3 | 位置边界校验 | `pet/window.rs:89` `move_pet_window`；复用 `float_window.rs:146` `clamp_to_visible_area` | 抽成公共 util，pet 与 float 共用。float 在 v1.2.1 就踩过"副屏拔掉后窗口离屏"，pet 一字未防。 |
| 1.4 | Linux 轮询线程每次新建 X11 连接 | `pet/hit_mask.rs:237`（`x11rb::connect(None)` 被 `:80` 循环按 ~18Hz 调用） | 缓存连接/复用单例。批次 2 的拖拽循环要复用同一函数，频率更高，必须先修。 |

**验收**：Windows 真机拖动桌宠，运行日志 grep `原生拖拽不可用` **0 命中**；杀进程重启后桌宠按上次位置出现；
把持久化位置手改成 `(-5000,-5000)` 重启，桌宠应落回可见区。

---

## 批次 2 · 拖拽流畅性重构（预计 1–1.5 天，独立 commit）

**架构决策（双轨）**：Windows/Linux 用 OS 原生拖拽（批次 1.1 后即生效）；macOS 用 **Rust 侧拖拽循环**。
不采用"macOS 也上原生"的理由（读了 tao 0.35.3）：mac 的 `start_dragging` 是**合成一个假 `LeftMouseDown` NSEvent**
（`NSEventSubtype::WindowExposed`、pressure 硬编码 1.0）再调 `performWindowDragWithEvent:` —— 合成事件对无边框透明窗不稳定，
且该调用异步返回，Promise 会在拖拽刚开始就 resolve，`beginNativeDrag` 会立刻读位置并触发 `onEnd(true)`，状态必然错乱。

| # | 内容 | 落点 |
|---|---|---|
| 2.1 | 新命令 `pet::begin_drag(grab_dx, grab_dy)` / `pet::end_drag()` | `src-tauri/src/pet/window.rs` + `lib.rs` 注册 + `capabilities/pet.json` |
| 2.2 | `begin_drag` 内起 8ms 生命周期循环：`cursor_logical()` 取全局光标 → 窗口位置 = 光标 − 抓取偏移 → `set_position`；`end_drag` 收尾并把最终位置回给前端 | 复用 `pet/hit_mask.rs:189-241` 现成的三平台光标读取 |
| 2.3 | 前端职责收缩为两件事：pointerdown 记抓取偏移；越 4px 阈值 → `begin_drag`；pointerup → `end_drag` + 写 store | `src/pet/composables/usePetDrag.ts`（删掉每帧 `invoke('move_pet_window')` 分支） |
| 2.4 | 4px 位移合并节流（两轴都 <4px 不发） | 循环内判定，借 Qoder 的 `{accepted:false}` 策略 |
| 2.5 | 消除"多个 move 在途、完成顺序不保证 → 窗口回跳" | 由 2.2 天然解决（不再有每帧 IPC）；回退分支保留但加在途串行化 |
| 2.6 | `set_position` 主线程约束核实 | `move_pet_window` 现为 `async fn`（跑 tokio worker），macOS 的 NSWindow 有主线程要求 → 必要时 `app.run_on_main_thread` |

**这套形态为什么比"webview 每帧 IPC"好**（与 Qoder 取证结论对齐后的判断）：Qoder 同样不用原生拖拽，而是
`setPointerCapture` + **屏幕绝对坐标** + 主进程保持偏移 —— 它解决的正是我们这条结构缺陷：
现在手动路径**既没有 `setPointerCapture`、监听又挂在 `window` 上**，光标一旦甩出 150×330 宠物矩形，DOM 不再收到
pointermove → 桌宠落后甚至卡住，光标回来又跳一下。直接改用全局光标（Rust 侧）比补 `setPointerCapture` 更彻底，
且不依赖 WebView2/WKWebView 是否真的做到 OS 级捕获。

**验收（macOS 是主战场）**：
- 用例 A：快速横甩 400px，光标一度完全离开宠物矩形 → 桌宠不掉队、不回跳、不中途变穿透。
- 用例 B：`scripts/cdp.py` 派发真实鼠标事件，量「pointerdown → 窗口实际位移」的帧延迟与丢帧数，对比批次 1.1 前后的 Windows 数据。
- 用例 C：拖拽过程中光标停在宠物身体外 → 松手后穿透态正确恢复。
- 铁律不破：拖拽期窗口保持 100% 静态（`PetWindow.vue:387` 的 `.is-dragging :deep(*){animation:none}` 是 v0.9.0 为
  DWM 重合成抖动加的，本轮不得为了做拖拽动画而拆除）。

### 批次 1 / 批次 2 实施结果（2026-10-29，代码已落盘、未提交；版本号已推 **v1.2.12**）

| # | 状态 | 说明 |
|---|---|---|
| 1.1 | ✅ 已改（一行） | `capabilities/pet.json` 权限列表补 `core:window:allow-start-dragging`。**已复核方案给的依据成立**：`gen/schemas/acl-manifests.json` 里 `core:window:default` 的 28 项确实不含它，而 `float.json` 一直含 —— 与「悬浮条原生拖拽可用、桌宠不可用」的现象完全对得上。**注意**：这条只在 Windows / Linux 产生收益；macOS 走批次 2 的 Rust 循环（见 2.1）。 |
| 1.2 | ✅ 已改，**⚠️ 与方案字面写法有偏差** | 方案写「`lib.rs` setup 读持久化 enabled → create/show/move」，**实现不了**：pet 的开关与坐标只存在 localStorage `screentime-pet`（`PetWindow.vue` 深看 + `persistSuspended` 写入），Rust 侧没有任何镜像能读到（就是批次 4.1 那条债）。落地改成**前端驱动**：`PetWindow.vue` 新增 `restorePetOnStartup()`，`onMounted` 末尾 `await`。顺序刻意是 `setSize → move_pet_window{clamp:true} → show_pet_window` —— 先定尺寸再定位（纠偏要按最终尺寸算），反过来会以 conf 里的 140×140 露出一帧再撑成皮肤尺寸。选 pet 窗而不是主窗做恢复：主窗可能因「启动最小化到托盘」延后加载，pet 窗的 webview 一定起来。`Settings.vue` 的 `onTogglePet` / `onShowPet` 同步改成**先 move 再 show**（原顺序会先看见宠物在旧位置闪一下）。 |
| 1.3 | ✅ 已改，比方案多做了一件 | 新增 `src-tauri/src/window_bounds.rs`，float 与 pet **真的共用**同一判据。**顺带修掉旧 float 实现只查横轴的漏洞**（原函数就叫 `overlap_x`，纵向甩出屏外一路放行）—— 新判据是两轴各与某屏 ≥1/3 重叠。`move_pet_window` 加 `clamp: Option<bool>` 且**默认 false**：拖拽每帧调用时纠偏会让窗口在用户手里弹跳，只有恢复 / 开关切换传 true。**遗留（→ 批次 4.1）**：纠偏后的正确位置没有回写前端持久化，下次重启还要再纠一次。 |
| 1.4 | ✅ 已改，**🔴 方案与首版实现都基于错误假设** | 首版按「`RustConnection` 内部是 `Rc`、不可 `Send`」写成 `conn.clone()` 存进 `thread_local` —— 探针实测 **E0599：x11rb 0.13.2 的 `RustConnection` 根本没实现 `Clone`**（`rust_connection/mod.rs:65` 是「一条 socket + 若干 `Mutex`」）。最终形态 `thread_local! + Arc<RustConnection>`（TLS 不要求 `Send`，零 unsafe；轮询线程与拖拽线程各持一条，互不阻塞），读失败即弃缓存、下轮重连自愈。<br>**这段代码为什么必须靠探针**：`#[cfg(target_os="linux")]` 分支在 Windows 本机 `cargo check` **完全不参与编译**，而整包 `--target x86_64-unknown-linux-gnu` 必然**假失败**（`libdbus-sys` 的 build.rs 要交叉 pkg-config，LINUX_EXIT=101 落在依赖里、还没轮到本 crate）→ 用 `.tmp-probe-linux-cursor`（gitignored，逐字镜像）做正证 + 反证。**没有探针，这个编译错误会一路带到 Linux 发版才被发现。** |
| 2.1 / 2.2 | ✅ 已改 | 新模块 `src-tauri/src/pet/drag.rs`：`begin_drag{grab_dx,grab_dy}` / `end_drag`，`lib.rs` 注册 + `app.manage(PetDragState)`。8ms 循环读**全局光标**（复用 1.4 的三平台 `cursor_logical`）→ 目标 = 光标 − 抓取偏移 → `set_position`。**IPC 实测 91**（48 commands + 22 screenshot + 14 pet〔窗口菜单 9 + 命中 3 + 拖拽 2〕+ 3 float + 3 app_update + 1 float_glass）—— 是**从 `generate_handler!` 重新数**的，不是照文档「89+2」递推。 |
| 2.3 | ✅ 已改 | `usePetDrag.ts` 重写：`DragMode = idle \| native \| rust \| manual`。pointerdown 只记抓取偏移，越 4px 阈值才发 `begin_drag`；`native` 抛错自动降 `rust`；`manual` 保留为最后兜底。**顺带补回 `setPointerCapture`**（v0.6.2-beta.6 加过、beta.28 重构时丢了）—— 主路径已不依赖它，但兜底路径需要。`onBeforeUnmount` 在 `rust` 态补发 `end_drag`，否则循环永久在跑。 |
| 2.4 | ✅ 已改 | 循环内 `should_send`：两轴位移**都** <4px 才滤掉（单轴大位移必须发，否则斜着甩会掉帧）。 |
| 2.5 | ✅ 已改 | 主路径不再有每帧 IPC，天然消除乱序；`manual` 兜底加**在途串行化**（同一时刻只允许一个 `move_pet_window` 在飞，期间到达的新目标只保留最新一个）。 |
| 2.6 | ✅ **已核实，结论与方案预设相反** | 方案写「必要时 `run_on_main_thread`」。**读源码后不需要**：tauri-runtime-wry 2.11.4 `send_user_message`（:235-255）在主线程调用时**内联执行**、否则 `context.proxy.send_event` **fire-and-forget**；`WindowMessage::SetPosition → window.set_outer_position`（:3543）最终在主线程落地。<br>**由此导出的真正硬约束**（已写进 `ARCHITECTURE.md` §9 与 `drag.rs` 模块头）：`getter!` / `window_getter!`（:197-211）一律「发事件 + `rx.recv()` 阻塞等回」⇒ **拖拽循环内禁止任何窗口 getter**。`end_drag` 因此回传的是**循环自己记的最后一个目标值**，不是 `outer_position()` —— `set_position` 异步投递，刚投的几帧还没落到 NSWindow，去问只能问出历史值，写进持久化就是「拖完回跳」。 |
| 铁律 | ✅ 未破 | `.is-dragging :deep(*){animation:none}` 原样保留，本轮没为拖拽动画动它。 |

**门禁实测（本机 Windows，v1.2.12 批次 1/2 交付口径）**：
- `cargo fmt`：**已跑，且这次零副作用** —— `cargo fmt -- --check` 只显示 `pet/drag.rs`（5 处）与 `window_bounds.rs`（8 处）两个**本批次新增文件**不合格（批次 0 那轮已把其余 34 个文件刷成 fmt-clean），所以本轮 fmt 只重写了这两个新文件，**没有波及其他人的 WIP**。
- `cargo test --lib`：**61 passed / 0 failed / 5 ignored**（本批次 +10：`window_bounds` 7 例、`pet/drag` 3 例）。
- `vue-tsc --noEmit`：**0 错误**。
- `cargo clippy --lib -- -D warnings`：**62 条 —— 与批次 0 记的基线逐条相等，本批次新增 0 条**。
  新文件 `pet/drag.rs`、`window_bounds.rs` 命中 **0**；`drag.rs` 首版自带的 3 条（两处 `doc list item overindented`
  + 一处 `if let Some(x) = f().ok()` 冗余匹配）已全部修掉。剩余 62 条仍全在批次 0 记录过的那批文件与行段
  （`screenshot/mod.rs` 19、`db/mod.rs` 9、`commands.rs` 8、`float_glass.rs` 6、`lib.rs` 5、`float_window.rs` 5 等），
  本批次触碰过的 `lib.rs:16-21` / `float_window.rs:32-38` / `hit_mask.rs:137` 三处命中**都是改动前就存在的行**（已核对行号落在本次 diff 之外）。清理仍归批次 4。
- 探针：`.tmp-probe-linux-cursor` 正证 `PROBE_EXIT=0`，反证注入 `conn.clone()` → `RC_EXIT=101 / E0599`。
  ⚠️ **反证差点是假的，两个坑都值得记**：**(a)** 本机 `python` 是 **Microsoft Store 占位程序**，脚本静默 no-op，
  连跑两次「注入后仍 exit 0」差点被当成探针失灵；**(b)** **cargo 的 mtime 指纹在同秒内改文件会误判 fresh**，
  日志里没有 `Checking` 行就等于没重编。判据：反证必须确认输出里有重编行。

**本批次没有做的（不在验收里，别当已完成）**：
- 方案批次 1/2 的**全部验收项都是真机项，一条都没跑**（本机 Windows，且 pet 的三处收益只有实体机能看）：
  Windows 拖动后日志 grep `原生拖拽不可用` **0 命中**；杀进程重启后按上次位置出现；
  持久化坐标手改 `(-5000,-5000)` 重启应落回可见区；macOS 用例 A/B/C。
- 批次 1.4 的**运行时**行为（连不上 X server 时的自愈、XWayland 下的表现）只做了类型层验证。
- macOS 的 `cursor_logical` 在拖拽循环 125Hz 下的 CPU 代价未实测（它每次调用都 `CGEventCreate` + `CFRelease`）。

---

## 批次 3 · 桌宠体验（预计 1 天，可与批次 4 并行）

| # | 借鉴自 Qoder | 本项目落点 | 成本 |
|---|---|---|---|
| 3.1 | 主进程轮询全局光标，按 `max(40px, 尺寸×0.38)` 角度阈值切 look 帧；进窗 hovering→waving | `hit_mask.rs` 线程每 55ms **已经算出**光标相对宠物的位置，只需把方向/距离随穿透态一起广播给 pet 窗 | **最低**，纯复用，性价比最高的一条"活起来" |
| 3.2 | 拖拽矢量驱动 runningLeft/Right、被拎起姿态 | 守住"拖拽期静态"铁律 → 把方向反馈挪到**松手瞬间**播 `landing`（按最后一段位移定方向），一次 0.4s，不与窗口移动重叠 | 低（皮肤已有 22/27 组 keyframes） |
| 3.3 | `prefers-reduced-motion` 降级为 0 动画 | 全仓当前 **0 处**；给皮肤 idle/breathe/sway 加媒体查询 | 低 |
| 3.4 | `focusable:false` + alwaysOnTop + skipTaskbar | 现在只有创建时 `focused(false)`，点一下就激活抢焦点。Windows 走 `WS_EX_NOACTIVATE`（`system_load/platform/windows.rs` 已有 raw FFI 先例），macOS 走 nonactivating panel | 中（需三平台分别验证点击仍可用） |
| 3.5 | 首帧就绪后 rAF 内发 `ready` 才显示窗口 | 修"先弹一个空白透明窗再画"的闪烁；配合 `show_pet_window` 改由前端 ready 回执驱动 | 中 |

---

## 批次 4 · 架构偿还（预计 1.5–2 天，不阻塞发版）

| # | 问题 | 改法 |
|---|---|---|
| 4.1 | **持久化双轨**：桌宠状态在 localStorage 单 key `screentime-pet`（`petStore.ts:28`），业务数据在 SQLite。`export_all` 只导 SQLite（`commands.rs:342`）→ 等级/喂食记录**不进备份**、不随设备切换、清 WebView 数据即丢 | 桌宠状态整体迁到 settings 表；顺带把跨窗同步事件收敛（`PetWindow.vue` 目前挂 9 个 `listen`、6 种 `*-updated/*-changed`，v1.2.5/v1.2.7 的历史 bug 全部出自这一类） |
| 4.2 | **窗口"声明"与"建造"重复**：`pet/pet-menu/float/capture` 既在 `tauri.conf.json` 的 `app.windows` 里声明、又在 Rust 用 `WebviewWindowBuilder` 建。conf 预创建使 `create_pet_window` 恒为幂等复用；桌宠关闭时 webview 仍常驻，命中掩码与气泡等运行开销需要按真实生命周期复核 | 二选一：删掉 conf 里的声明（真正按需创建，与批次 1.2 的启动恢复一起改），或删掉 builder 并承认常驻。文档与代码必须一致 |
| 4.3 | **皮肤包格式**：Qoder 用目录式 `pets/<slug>/pet.json + spritesheet` + contentHash；旧 2D 自定义 sprite 链路本次已确认无消费者并移除，现行内置皮肤资源仍随前端 bundle，第三方皮肤包格式尚未统一 | 新皮肤包采用素材落文件 + hash 校验；不建议为旧 localStorage sprite 链路再做兼容层 |
| 4.4 | 孤儿文件 `（` 未被 gitignore（10-28 清过，10-29 11:35 又出现，`git status` 里是 untracked，`git add -A` 会误提交） | `.gitignore` 加规则 + 查明是哪个脚本产生的（疑似未加引号的重定向目标） |
| 4.5 | `.tmp-*` 共 **7.1 GB** 留在工作树（`.tmp-ocr` 2.4G 内含一套完整 Edge profile：History / Cookies / Login Data；`.tmp-release` 2.4G；`.tmp-upd` 2.0G） | 均已被 gitignore（无泄露风险），但应清理；凭证类目录不该长期待在开发树里 |
| 4.6 | 根目录 10+ 个带日期专项文档 | 新文档统一进 `docs/`，历史文档留底不动 |

---

## 批次 5 · 待 Ryan 决策（不排进任何批次）

1. **桌宠状态源的语义要不要改**。Qoder 桌宠是 **Agent 会话镜像**（phase = idle/running/waiting/review/failed + 头顶 activity stack）；
   本项目是**前台应用类别**（`pet/engine/appToState.ts`），即"你在用什么"而不是"你的电脑在干什么"。
   现成的雏形是 `pet-system-overload`（过热→暴躁）。要不要新增一个 `waiting` 态属产品取向，不动代码。
2. **是否把批次 1–3 合进 v1.2.11**，还是 v1.2.11 先发（只含批次 0），桌宠单开 **v1.2.12**。
   建议后者：批次 0 是发版阻塞项，桌宠是体验项，混在一起会把发版时间拖到真机三平台验证之后。

---

## 依赖与顺序

```
批次0（发版阻塞）──提交──> v1.2.11 发版
批次1.1（权限一行）─┬─> 批次2（拖拽循环，需要 1.4 的光标函数复用）
批次1.2/1.3 ───────┘        └─> 批次3.1/3.2（look 反馈 / 落地动画，复用同一条线程）
批次4.1/4.2 与上面并行，但 4.2 要和 1.2 同时改（都动 pet 窗创建/恢复路径）
```

## 已排除（复核后判定为误报，不必再查）

OCR CTC 切片越界 panic、`catch_unwind` 因线程模型失效、`libloading` 缺 target 门控（它在主 `[dependencies]`，
且 Windows CI 用 `windows-latest` 不交叉）、`load_pct` 0~1/0~100 口径不一致（该字段全仓不存在）、
`take_capture_ready` 不隐藏遮罩（前端 `1100/1112` 的 finally 已 hide）、"投喂后菜单不再弹出"（`onFeed` 明确不 close）、
"桌宠丢失无任何恢复入口"（设置页 `onResetPetPos` 与菜单 `onResetPos` 都在）、
"主窗缺 `core:event:allow-unlisten/allow-emit`"（`core:event:default` 四项俱全）、"en-US 是扁平点号 key、缺 51 条"（两份 i18n 都是嵌套对象）。
