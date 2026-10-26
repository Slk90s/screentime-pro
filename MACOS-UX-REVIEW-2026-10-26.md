# macOS 交互体验审查与优化建议

> 审查对象：screentime-pro v1.2.8（Tauri 2 + Rust + Vue3）
> 审查日期：2026-10-26　｜　审查方式：源码全链路追溯 + 本机数据实测 + 截图逐像素量化
> 审查范围：① 权限弹窗　② 首开胶囊残影　③ macOS 15 玻璃适配　④ 自动更新　⑤ 状态栏浅色偏灰
>
> ⚠️ 本机为 Windows，所有涉及 macOS 运行时渲染的结论均标注了「需真机复核」。标注为「实测」的数据来自截图逐像素采样与本机数据库，非推测。

---

## 结论摘要

| # | 问题 | 根因（一句话） | 等级 | 改动面 |
|---|---|---|---|---|
| 1 | 权限弹窗反复、多层级 | 启动即请求屏幕录制（且用会撒谎的单信号判定）；辅助功能是「永远授权不了」的死路；自家模态与系统框同屏 | **P0** | Rust 主 + 前端 |
| 2 | 首开胶囊残影 | float 窗口在 webview 首帧渲染前就被 `show()`，此刻窗口里只有一块 340×44 的系统材质（无文字） | **P0** | Rust + 前端 |
| 3 | macOS 15 玻璃适配 | 宽度方向已结构性修好（`width:100%`），**高度方向仍是双源**（CSS 硬编码 36/44 vs Rust 独立 `set_size`）；材质 apply 不幂等，只叠不清 | **P1** | Rust + 前端 |
| 4 | v1.2.7 未检测到更新 | 检查只在「进程启动 + 15 秒」跑一次，而应用是常驻托盘的（关窗不退出）+ 24h 节流会把重启后的补救也吞掉 | **P0** | 前端 1 处 + Rust 3 处 |
| 5 | 状态栏浅色偏灰 | 白字固定 `#f5f5f7` + 0.26 压暗，浅色壁纸下实测对比度仅 **1.36:1**（深色下 12.1:1）。降 alpha 治不了，必须反色 | **P0** | 前端 + Rust 少量 |

**一句话总结**：五个问题里有四个是同一个毛病——**「单一来源」没有坚持到底**。权限状态有两个源、浮窗高度有两个源、更新检查有三个触发点却只有一个真的会跑、配色只有一个亮度假设却要适配所有壁纸。

---

## 一、权限弹窗：从「6 个系统框 + 2 个自制提示」降到「0~1 个」

### 1.1 首次安装的真实时序（macOS 15 / ad-hoc 签名）

| # | 时机 | 用户看到的 | 触发点 |
|---|---|---|---|
| 0 | 双击 DMG 里的 .app | 「Apple 无法验证此 App 是否包含恶意软件」 | Gatekeeper（无 Developer ID） |
| 0.5 | 按说明执行 `修复门禁.command` | 再来一次「无法验证开发者」 | 脚本自身也带 quarantine |
| 1 | 启动 +3s | 「想要录制此电脑的屏幕」系统授权框 | `lib.rs:369` → `screenshot/macos.rs:548` |
| 2 | 同一秒 | 自家全屏模态「缺少屏幕录制权限」 | `screenshot/macos.rs:556` emit → `App.vue:371` |
| 3 | 同时 | 顶部黄色横幅「未授予辅助功能权限」 | `App.vue:292` → `tracker/macos.rs:230` |
| 4 | 点横幅「前往设置」 | 系统设置辅助功能面板，**但列表里找不到本应用** | `tracker/macos.rs` 永远传 `prompt=false` |
| 5 | 首次截图 | macOS 15 独有的「允许一个月 / 打开系统设置」 | `macos_sck.rs:143` `SCShareableContent::get()` |
| 6 | 授权后 | 自家弹窗翻成「重启应用」→ 重启又回到 #1 | `lib.rs:370` 无条件执行 |

**净效果：4 个系统弹窗（其中 2 个是 Gatekeeper）+ 2 个自制提示同屏打架。**

### 1.2 三个必须纠正的事实（与代码注释相反）

**事实 A：全项目实际只需要 1 张 TCC（屏幕录制）。**

| 功能 | 真需要的权限 | 证据 |
|---|---|---|
| 应用时长统计（App 名） | **无** | `tracker/macos.rs:47` 只调 `NSWorkspace.frontmostApplication()` |
| 应用时长统计（窗口标题） | 屏幕录制 | `tracker/macos.rs:98` 未授权返回 `None`，回退到进程名 |
| 空闲检测 | **不需要辅助功能** | 全仓 `AXUIElement / AXObserver / CGEventTapCreate` **零命中**，只用 `CGEventSourceSecondsSinceLastEventType` |
| 截图取字 | 屏幕录制 | OCR 走 Vision，不需要权限 |
| 桌宠 / 开机自启 | **无** | 自启走 `LaunchAgent` 写 plist，不走 `osascript` |

→ `Info.plist` 里的 `NSAppleEventsUsageDescription` 是**僵尸键**（全仓无 `NSAppleScript` / `osascript` 调用）；`NSAccessibilityUsageDescription` 是**永远不会展示的键**（从未以 `prompt=true` 请求过）。

**事实 B：辅助功能是「永远授权不了」的死路。**

`tracker/macos.rs:225` 恒传 `kAXTrustedCheckOptionPrompt = false` → App 从不向 tccd 发起请求 → 大概率不出现在系统设置列表里。而 `App.vue:66` 的按钮恰恰跳去辅助功能面板。用户在空列表里找不到本应用，横幅永远关不掉。

**事实 C：启动预请求用的是「会撒谎」的单信号。**

```rust
// screenshot/macos.rs:543 —— 只用 CGPreflightScreenCaptureAccess
if crate::tracker::macos::is_screen_capture_trusted() { return; }
```

而该 API「进程内一旦返回 false 就一直是 false」这个坑，本仓库自己在 `screenshot/macos.rs:15-22` 和 `mod.rs:52-55` 写了两遍。同期 `ensure_ready`（`:363`）用的是**双信号**（预检 OR 窗口标题可读）。

→ **已授权用户每次启动都可能被再弹一次系统框。**

### 1.3 可优化点

**P0-A　删掉启动预请求，延后到首次截图**（Rust，约 10 行）

- `lib.rs:369` 的 `schedule_startup_permission_request` 整段移除；
- 启动只调 `status_snapshot()` **查询**，不 `request`；
- 首次请求由 `begin_capture_inner`（`screenshot/mod.rs:665`）的既有闸门负责 —— 那里用户本来就有明确意图。

> 反方案不可取：「3 秒后请求」这个设计的初衷是避开「按快捷键时被突然弹框」，但它换来的代价是**每次启动都弹**。截图闸门处弹框时遮罩还没上屏，不存在「流程已开始才弹」的割裂。

**P0-B　交接给双信号判定**（Rust，1 行）

`macos.rs:543` 改 `collect().permitted()`，与 `ensure_ready` 同口径。

**P0-C　辅助功能：二选一**

- **方案 A（推荐）**：下架。删 `App.vue:63-69` 横幅、删 `Info.plist:7-8`、删 `PermissionStatus.accessibility`。收益只有「空闲判据更准」，且失败时 `commands.rs:733` 本身就 `unwrap_or(0)`（视为不空闲），属精度损失而非功能缺失。
- **方案 B**：改成用户点按钮时才 `prompt=true`，让 App 真正进入列表，深链不再是死路。

**P0-D　dismiss 状态落盘 + 不自动弹模态**（纯前端，约 15 行）

- `App.vue:170/175` 的 `permDismissed` / `permInvalidDismissed` 写 `localStorage`（当前 `grep localStorage src/App.vue` **零命中**）；
- `App.vue:317` 的 `tauri://focus` 回调只重查状态，**不要**重置 dismiss 标记，改为「状态 false→true→false 翻转」才重置；
- `App.vue:371` 加节流：同版本 + 同 reason 只自动弹一次。

**P1　收口成一个「权限中心」**（纯前端，约 120 行）

`Settings.vue` 里 `checkPermissions` 命中数为 0 —— 设置页完全看不到权限状态。建议加一张卡片：

```
权限中心
  屏幕录制   ● 未授权   [去授权] [重置并重启]
            用于截图取字；不授权则只能统计 App 名，无法记录窗口标题
  辅助功能   ● 未授权   [去系统设置]
```

顶部横幅只承担「通知」，操作统一进这个页面。

**P1　统一权限状态源**（Rust，2 行）

`commands/platform/macos.rs:34` 用单信号、`screenshot/macos.rs:508` 用双信号，导致 `App.vue:201` 的 `accessibilityNeverGranted` 会在「录屏其实正常」时误判为「从未授权」，把横幅文案切错。

**P1　补 macOS 15 月度 nag 的文案**（i18n，2 key × 2 语言）

`macos_sck.rs:143` 的 `SCShareableContent::get()` 在 macOS 15 必然触发「允许一个月」，15.1 的 replayd 还会「用一次就重置计时」。这是系统行为，代码无法抑制（仅 MDM 描述文件可关）。当前 UI 完全没提，用户会当成「这软件又抽风」。

→ 在 `shotPermReasonNotGranted` 补一句：「macOS 15 起系统会每月询问一次『允许一个月』，属系统行为，选『允许一个月』即可。」

### 1.4 预期效果

| 指标 | 现状 | 优化后 |
|---|---|---|
| 首次启动系统弹窗 | 4 个 | **0 个**（用户不点截图就看不到录屏请求） |
| 已授权用户每次启动 | 可能弹 1 次（预检撒谎） | **0 次** |
| 自制提示同屏 | 横幅 + 模态 + 系统框 = 3 层 | **0~1 层**（只在权限中心） |
| 关掉后是否复活 | 每次重启 + 每次 focus | **不再复活** |

### 1.5 根治项：签名与公证（P2，需采购）

`tauri.conf.json:121-123` 的 `bundle.macOS` 只有 `minimumSystemVersion`，**无 `signingIdentity`、无 `hardenedRuntime`、无 entitlements 文件**；`build.yml` 无任何 `APPLE_*` 环境变量。

后果不只是「首次打开被拦」——**ad-hoc 签名的 cdhash 每次构建都变**，TCC 按「路径 + 签名指纹」记账，所以每升级一版，旧的录屏/辅助功能授权记录还显示「开」但实际已失效。这就是「升级后又要重新授权」的物理原因。

`GET /releases` 层面无法绕过。短期只能把 DMG 里那段 `xattr` 说明提升到 **Release 正文 + README 首屏**（当前 `README.md:137` 只写「拖入应用程序即可」，与实际体验不符）。长期方案：Developer ID + `notarytool` 公证，年费 $99，一次性解决「授权漂移」+「双击被拦」。

---

## 二、首开胶囊残影：那一帧里窗口是「只有材质、没有内容」

### 2.1 残影的几何指纹（决定性证据）

截图里残影的尺寸换算回逻辑像素：

| 项 | 实测 | 代码常量 | 吻合 |
|---|---|---|---|
| 宽 | ≈337 逻辑 px | `FLOAT_INITIAL_WIDTH = 340` | ✅ |
| 高 | ≈43 逻辑 px | `FLOAT_HEIGHT_GLASS = 44` | ✅ |
| 圆角 | 全圆（=高/2） | `FLOAT_RADIUS_GLASS = 22` | ✅ |
| 内容 | **纯灰、无文字、无描边** | —— | 只有系统材质层能这样 |

**340×44 + 纯灰无字，只可能是「窗口已上屏、系统材质已铺、webview 还没画出第一个像素」。**

### 2.2 首开时的完整时序

```
T0  Tauri 依 conf 建窗：main(1040x720, center) → pet → pet-menu → float(340x36, center)
    ⚠️ float 的 conf 没有 x/y → tao 兜底 ns_window.center() → 与 main 中心重合
    WKWebView drawsBackground=false、NSWindow isOpaque=false、backgroundColor=clear
    （已排除「NSWindow 默认灰底」：tao window.rs:543-560 已做对）

T1  setup（lib.rs:683）ensure_float_window → 已存在，直接 return
    ⚠️ runtime builder 里的 inner_size(340,44)/min_inner_size(140,44) 从未生效

T2  【灰块诞生】lib.rs:694  float_glass::apply(win, true)  ★ 同步调用
    → NSVisualEffectView(frame=340x36, radius=22,
       material=UnderWindowBackground, state=Active) addSubview Below
    → 此刻窗口里只有这块材质矩形，DOM 尚未加载
    ★★ 这就是截图里那个胶囊

T3  【第 2 层】lib.rs:709  set_float_visible(true)
    → float_window.rs:201  又 apply_via_app(true)  → addSubview 再来一层
    → clamp → window.show()  ★ 窗口上屏，webview 仍空

T4  【第 3 层】lib.rs:637  1Hz 首轮 want_show=true → 再 set_float_visible(true) → 第 3 层

T5  webview 加载完成，FloatBar onMounted（FloatBar.vue:513）
    await applyPosition()          ← IPC 往返
    await getFloatGlassSupport()   ← IPC 往返
    await fitWidth()               ← 窗口从 340 收缩到内容宽
    await invoke("show_float_window")  ★ 第 4 层
    tick() → 数字出现  ★ 到这一刻才是「正常浮窗」
```

**残影产生于 T2，被用户看见于 T3（`show()` 不等 webview），直到 T5 的 `tick()` 才变成有内容的浮窗。**

关键：`screenshot/mod.rs:388` 的恢复路径更粗暴——直接 `w.show()`，绕过 `set_float_visible`，不补材质、不 clamp、不判 DOM。

### 2.3 为什么是「残留」而不只是「闪一下」

`window-vibrancy` 的 `apply_vibrancy` **不去重**：每次调用都新建 `NSVisualEffectView` 并 `addSubview`（`vibrancy.rs:42-59`）。

```rust
// window-vibrancy-0.8.1/src/macos/vibrancy.rs
blurred_view.setAutoresizingMask(ViewWidthSizable | ViewHeightSizable);
view.addSubview_positioned_relativeTo(&blurred_view, NSWindowOrderingMode::Below, None);
// ↑ 没有任何「已存在就复用」判断
```

而 `clear_vibrancy` 只删**一个**：

```rust
let blurred_view = view.viewWithTag(NS_VIEW_TAG_BLUR_VIEW);  // 只取最近的一个
if let Some(blurred_view) = blurred_view { blurred_view.removeFromSuperview(); return Ok(true); }
```

→ **apply N 次 = N 层叠加；clear 1 次只删 1 层。** 启动就是 3~4 层，之后每次全屏进出各 +1；关掉玻璃开关后剩余的层**永久留在窗口里**。

这同时解释了另一件事：v1.2.8 刚把压暗从 0.48 定标到 0.26，但多层 tint 是**叠乘**的（1-(1-0.26)^3 ≈ 59%），实际遮蔽接近 Windows 档 —— 等于「刚定标完就被自己叠层抹掉」。

### 2.4 修复方案（按代价排序）

**方案 1　让材质幂等（Rust，约 4 行，收益最大）**

`float_glass.rs::platform_apply` 的 `glass=true` 分支开头先清再铺：

```rust
GlassTier::Liquid => {
    let _ = clear_liquid_glass(window);   // 保证单层
    ...
}
GlassTier::Vibrancy => {
    let _ = clear_vibrancy(window);
    apply_vibrancy(...)
}
```

→ 无论 apply 多少次，窗口里永远 1 层。灰板遮蔽回到 40%，关玻璃后无残留。

**方案 2　首次 show 必须等 DOM 首帧（Rust + 前端，中等）**

- Rust：`lib.rs:709` 与 `lib.rs:637` 只「准备」（apply + set_size + clamp），**不 show**；
- 前端：`FloatBar.vue:545` 的 `fitWidth()` 之后加两帧等待再 show：

```ts
await fitWidth();
await new Promise<void>(r => requestAnimationFrame(() => requestAnimationFrame(() => r())));
await invoke("show_float_window");
```

→ 彻底消除「无文字灰胶囊」那一帧，首次显示晚约 33ms（不可感知）。

**方案 3　conf 给显式初始位置（配置，最低代价）**

```jsonc
// tauri.conf.json float 窗口
"height": 44, "minHeight": 44,   // 与 FLOAT_HEIGHT_GLASS 一致，消掉 36/44 漂移
"x": 24, "y": 24                 // 别让它 center 到主窗口正中央
```

→ 即使时序问题未修，灰块也不会出现在主窗口正中（改到屏幕左上，用户几乎注意不到）。同时修掉「conf 36 vs runtime 44」这个配置漂移。

**方案 4　前端兜底（低，单独用无效）**

```css
.float-bar { opacity: 0; }
.float-bar.is-ready { opacity: 1; transition: opacity .12s ease; }
```

在 `tick()` 拿到首份 metrics 后置 `isReady=true`。
⚠️ **它盖不住系统材质**（材质在窗口层，不在 DOM 里），只能盖住「CSS 上屏但数据未到」的空态。必须与方案 1/2 配合。

**顺手修**：`screenshot/mod.rs:388` 的 `restore_self` 改用 `float_window::set_float_visible(app, true)`，走统一路径。

---

## 三、macOS 15 玻璃适配：宽度这条腿稳了，高度和材质生命周期还挂在注释上

### 3.1 九个边界场景

| # | 场景 | 当前行为 | 状态 | 风险 |
|---|---|---|---|---|
| 1 | DPR 1.0/1.5/2.0 下 3px 修正 | 修正已**结构性**（`width:100%` + 居中），`WIDTH_SLACK=2` 语义是「1px border ×2」；测量全用 CSS px，`set_size` 全用 `LogicalSize` | ✅ 稳 | P2 |
| 2 | 文字宽度变化（10%→100%、186K→999M、中英） | 百分比槽位 `min-width: 2.45em`（≈31.85px），"100%" 实宽 ≈32.2px → **临界 +0.35px**，会微增宽再由 fitWidth 跟随 | ✅ 稳（临界） | P2 |
| 3 | 指标全关、收到 `FLOAT_MIN_WIDTH=140` | 22 = 44/2、18 = 36/2 恒为半高；CSS 规范下超界半径会等比缩小，永不退化成椭圆 | ✅ 稳 | P2 |
| 4 | macOS 26/15/14/13 版本判断 | 用 `NSAppKitVersionNumber >= 2685` 判 26；13/14/15 走 `Vibrancy`；纯 CSS 兜底一直在 | ⚠️ 部分 | **P1** |
| 5 | 浅色/深色外观切换 | 材质天然跟随（未 `setAppearance`）；但 **CSS 固定 `rgba(0,0,0,0.26)` + 白字**，无 `prefers-color-scheme` 分档 | ❌ 未处理 | **P0**（见第五节） |
| 6 | 跨屏拖动 / scale 变化 | 无监听，但材质 autoresizing + 逻辑尺寸本就不需重算。**残留风险在位置**：`float_pos` 可能是另一块不同 DPI 屏存的 | ⚠️ 位置有坑 | P2 |
| 7 | 1Hz 是否每次 resize | 双重早退：JS `|w-appliedW|<1`、Rust `|Δ|<0.5`。稳定态无 `set_size` | ✅ 稳 | P2 |
| 8 | 3px 只修右侧？ | 已对称：外层 `width:100%` + `justify-content:center`，余量左右各分一半 | ✅ 稳 | — |
| 9 | glass 开关 36↔44 过渡 | **有中间态错位，且不止一帧**（见下） | ❌ 未处理 | **P1** |

### 3.2 三个最脆的点

**脆点 1（P1，与被修的横向 bug 同源）：高度没有结构性恒等式**

```css
/* FloatBar.vue:592 / :648 —— 硬编码 */
.float-bar { height: 36px; }
.float-bar.is-glass { height: 44px; }
```

而窗口高度由 Rust 按 `status_bar_config.float_glass` **独立** `set_size`（`float_window.rs:277`）。两条时间线不同源，三种常见路径都会破：

1. **glass 用户冷启动**：窗口已 44，但 `cfg` 初值 `float_glass:false`（`FloatBar.vue:257`）→ 首帧 CSS 是 36 → **底部露出 8px 系统材质**；
2. **开关翻转**：cfg 靠 1Hz 轮询 → 最多 1 秒内窗口 44 / 胶囊 36（或反之被裁 8px），且材质 radius 22 vs CSS radius 18 → **纵向双弧**；
3. **隐藏期间定时器被节流**，这个错位时长会被拉长。

→ **一条改法根治三种**：

```css
.float-bar { height: 100%; border-radius: 999px; }
```

高度全部交给 Rust 单一来源，CSS 不再参与「窗口该多高」的决策 —— 与 v1.2.8 对宽度做的改造完全同构。

**脆点 2（P1）：材质视图只增不减**

见 §2.3。叠加污染的不只是「灰板感」，还让 `native_blur` 档的 0.26 定标失效。

**脆点 3（P1）：`native_blur` 报的是「能力」不是「事实」**

```rust
// float_glass.rs:414 —— support() 探测的是能装哪档
// apply_vibrancy 失败时只 warn（:373-379），前端仍按 has-blur 走 0.26 轻压暗
```

→ 一旦 apply 失败，就是「无模糊 + 轻压暗 = 白字溶进桌面」。应把最近一次 apply 结果写进 `AppState`，`float_glass_support` 返回**实际生效**而非能力。

### 3.3 其他边界（P2）

- `min_inner_size` 高度在 runtime builder 里写死 44 → 若窗口被重建且 glass 已关，`set_size(...,36)` 会被 OS 拒绝，导致高度差 8px 永久消不掉 → **每秒一次 set_position+set_size 抖动**。修法：min 高度恒用 `FLOAT_HEIGHT_PLAIN`。
- `translucency` 字段已上报但前端未消费（`float_glass.rs:202` 自己也写了「尚未消费」）。macOS 26 的 `Clear` 比 15 的 `UnderWindowBackground` 更清透、更需要压暗，将来上 26 必须据此分档。
- 验证脚本有结构性盲区：两个脚本都把窗口高和 `.is-glass` 一起设为 44（`verify_v128_glass_geometry.py:126`），**天生发现不了「窗口 44 / CSS 36」**。建议加断言 `bar.getBoundingClientRect().height === innerHeight`。

**总体结论：宽度方向「稳」（有脚本守着，DPR/文案/最小宽都不破）；高度与材质生命周期「脆」（双源 + 不幂等）。v1.2.8 修对了一半。**

---

## 四、自动更新：链路是通的，但「只查一次」+「常驻进程」

### 4.1 本机实测证据（决定性）

| 证据 | 值 | 来源 |
|---|---|---|
| `update_last_check_at` | `1792993434` = **2026-10-26 13:43:54** | `%APPDATA%\com.screentime.pro\screentime.db` |
| `update_dismissed_version` | **不存在**（27 个 key 里没有） | 同上 |
| 进程启动 | 13:43:40，`version=1.2.8` | `logs/app.2026-10-26.log:2032` |
| 时间戳写入时机 | 启动后 **+14 秒** | 13:43:40 → 13:43:54 |
| 10-24 / 10-25 全天 | **零条更新检查日志** | 同日日志 |
| 长期驻留进程 | v1.2.4 从 10-23 07:54Z 连续运行到 10-26 05:43Z（**3 天 22 小时，中间无重启**） | 各日日志 |
| 该驻留期内检查次数 | **0** | 同上 |

→ **链路可用**（13:43:40 启动 → 13:43:54 成功抓取并写库）。问题不是「没写代码」，是「跑得太少」。

### 4.2 根因排序

**R1（P0，主因）检查只在「进程启动 + 15 秒」跑一次，而进程是常驻的**

```ts
// Dashboard.vue:282
onMounted(() => {
  loadDevices(); loadRange(); loadDetails();
  updateTimer = setTimeout(checkUpdateSilently, 15_000);  // 只有 setTimeout，没有 setInterval
});
```

```rust
// lib.rs:715 —— 关窗只是 hide，进程不退
.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
})
```

用户「打开应用」= 从托盘唤出窗口 ≠ 重启进程 → `onMounted` 不会再跑。
v1.2.8 于 10-26 11:54 发布，而机器上跑的是 10-23 启动的 v1.2.4，它那天就把那一次检查用掉了 → **新版本发布时那个进程根本不知道**。

**R2（P0，并列主因）24h 节流会吞掉「重启」这条唯一的补救路径**

```rust
// commands.rs:1019 —— 命中就 return，连请求都不发
if elapsed >= 0 && elapsed < UPDATE_CHECK_INTERVAL_SECS {   // 24h
    tracing::debug!(...);   // ← debug，release 被过滤
    return Ok(no_update);
}
// commands.rs:1039 —— 写时间戳在「判有无新版本」之前
state.db.set_setting("update_last_check_at", &now.to_string())?;
// commands.rs:1044 —— 无新版本
if !semver_gt(&latest, &current) { return Ok(no_update); }
```

即：**「检查成功但没新版本」也会烧掉接下来 24 小时。**
事故序列：T 时刻启动 v1.2.7 → 检查成功、无新版 → 写 T；T+16h 发布 v1.2.8；T+17h 用户重启 → `elapsed=61200 < 86400` → 直接 return，**连网络请求都不发**。

本机印证：`update_last_check_at` 距今仅 0.16 小时，**未来 23.84 小时内任何重启都会被静默跳过**。

**R3（P1）所有失败/节流路径都是 `debug!`，release 下连日志都没有**

`logging.rs:199` release 默认级别是 `info`。`commands.rs` 里节流跳过(1024)、抓取失败(1033)、已是最新(1045)、已 dismissed(1052) **全是 debug**，只有「发现新版本」(1057) 是 info。

→ 只有成功才留痕，其余全部不可诊断。**本次排查正是被这个坑挡住。**

**R4（P1）抓取通道无兜底**

- 首选 `https://github.com/Slk90s/screentime-pro/releases.atom`（HTML 域，国内常被 RST），回落 API；
- `reqwest` 未开 `system-proxy`，**只认环境变量、不读系统代理** → 开着 Clash 系统代理的 GUI 应用走直连；
- 无 Gitee / ghproxy 镜像（对照 `ocr_engine.rs:513` 的 OCR 下载就做了 GitHub + Gitee 双源）；
- 失败静默 `return Ok(no_update)`，用户 100% 无感知。

**R5（P2）dismissed 用「严格大于」**

```rust
if !semver_gt(&latest, &dismissed) { return Ok(no_update); }
```

若 `dismissed == "1.2.8"` 且 `latest == "1.2.8"`（本项目真实存在「重切同名 tag」的发版习惯）→ **永久不再提示**。本机该键不存在，故非本次主因，但是同款定时炸弹。

### 4.3 已排除的假线索

**版本号不一致这条是假的。**`current` 取自 `app.config().version`（`commands.rs:939`），即 `tauri.conf.json`，**不是** `CARGO_PKG_VERSION`：

| 文件 | 值 | 影响 `current` |
|---|---|---|
| `tauri.conf.json:4` | **1.2.8** | ✅ 唯一权威源 |
| `package.json:4` | 1.2.8 | ❌ |
| `package-lock.json:3/9` | 1.2.8 | ❌（本次没漏改） |
| `src-tauri/Cargo.toml:3` | 0.1.0 | ❌（长期如此，`RELEASE.md §1.2` 明确要求不要对齐） |

历史日志也印证正常：`app.2026-10-22.log:224` 有 `current=1.0.0 latest=1.2.2`。

### 4.4 修复清单

| 编号 | 优先级 | 改哪个文件 | 改成什么 | 预期 |
|---|---|---|---|---|
| F1 | **P0** | `Dashboard.vue:286` | `setTimeout(15s)` + 之后 `setInterval(6h)`；`onBeforeUnmount` 清 interval | 常驻用户 6 小时内必看到横幅 |
| F2 | **P0** | `commands.rs:993` / `:1039` | 窗口 24h→6h；写时间戳**移到「已是最新」判断之后** | 24h 内重启不再被吞 |
| F3 | **P0** | `commands.rs:1024/1033/1045/1052` | `debug!` → `info!`（节流/最新/已 dismissed）/ `warn!`（失败，带 last_err） | 下次可诊断 |
| F4 | P1 | `commands.rs:893` + `Cargo.toml:36` | 加 `connect_timeout(5s)` + 重试一次 + Gitee 镜像回落 + `system-proxy` feature | 国内成功率显著提升 |
| F5 | P1 | `commands.rs:1050` | dismiss 时同时写 `update_dismissed_at`；或启动时若 `current >= dismissed` 就清空该键 | 修「重切同名 tag 永不提示」 |
| F6 | P2 | `commands.rs:961` | `semver_gt` 内部再 `trim_start_matches('v')`，解析失败整体返回 false（不逐段丢弃） | 防 `v` 前缀与非 semver tag 错位 |
| F7 | P2 | `Dashboard.vue:45` | 横幅移出 `tab === 'stats'` 分支 | 停在设置页也能看到提示 |
| F9 | P2 | `commands.rs:945` | atom 解析时对 tag 做 semver 校验，非法（如 `ocr-runtime`）跳过 | 发资源型 release 时不会污染 latest |

### 4.5 现场分诊手段

让用户在 v1.2.7 上点一次「设置 → 检查更新」：

- **能看到 v1.2.8** → 问题在节流/只查一次（R1/R2/R3），按 F1/F2/F3 修；
- **报错** → 网络/代理（R4）；
- **说已是最新** → 版本源异常（但本次核实是干净的）。

---

## 五、状态栏浅色偏灰：降 alpha 治不了，必须反色

### 5.1 逐像素实测（这是本次最硬的证据）

| 场景 | 胶囊内部底色 | 饱和度 | 白字对比度 | 判定 |
|---|---|---|---|---|
| 浅色（餐盘/白桌面） | `rgb(210,211,211)` | **0.01** | **1.36 : 1** | 严重不可读（AA 要求 4.5:1） |
| 深色（蓝色壁纸） | `rgb(17,48,83)` | 0.81 | **12.1 : 1** | 优秀 |

**同一份 CSS，深色 12:1、浅色 1.36:1 —— 差 9 倍。这就是「深色正常、浅色太灰」的全部原因。**

### 5.2 机理

```
浅色壁纸
  → NSVisualEffectView(UnderWindowBackground) 高斯模糊 → 仍是近白 (~250)
  → 材质自带约 35~45% 中性底色遮罩 → 压向中灰并去色
  → CSS rgba(0,0,0,0.26) 再压一档 → rgb(210,211,211)
  → 白字 #f5f5f7 落在上面 → 1.36:1
```

深色壁纸走同一条链，但「暗底 + 白字」方向天然正确，所以 12:1。

**关键判断：这不是 alpha 调错了，是「白字 + 暗底」这套假设只能适配暗背景。**

验证：即使把压暗从 0.26 降到 0.16，浅色下底色仍约 `rgb(185,185,185)`，白字对比度约 1.6:1 —— **依然不达标**。而降到 0（完全不压暗），白字落在近白底上就是 1.0:1，更糟。

→ **v1.2.3（0.30→0.48）、v1.2.8（0.48→0.26）这两次反复调参，本质上是在一条无解的路上找平衡点。** 这条路走到头也不会有「浅色和深色都好看」的结果。

### 5.3 三条修复路线

**路线 A（推荐，根治）：按背景亮度自动反色**

- Rust 侧采样浮窗中心点背后的桌面亮度（可用 `CGWindowListCreateImage` 取 1×1，或直接读壁纸图平均亮度），1Hz tick 里顺带做，成本可忽略；
- 亮度 > 0.55 → **亮字档**：底 `rgba(255,255,255,0.55)` + 深色字 `#1c1c1e`（相当于 macOS 的 `NSVisualEffectMaterial::Popover` 亮色观感）；
- 亮度 ≤ 0.55 → **暗字档**：沿用现在的 `rgba(0,0,0,0.26)` + `#f5f5f7`；
- 两档用 `0.3s` 缓动过渡，避免跨屏拖动时突变。

→ 预期：浅色下对比度从 1.36 提到 **≥7:1**，深色保持 12:1，两头都达标。

**路线 B（中等，纯前端 + 一小段 Rust）：按系统外观分档**

监听 `NSApp.effectiveAppearance`（或前端 `prefers-color-scheme`，`style.css:34` 已有这套机制，但 `FloatBar.vue` **完全没用**），系统浅色时切到「亮底深字」档。

→ 覆盖「系统浅色 + 浅壁纸」这个绝大多数组合；「系统深色 + 浅壁纸」（比如深色模式看白底 PDF）仍会偏灰，属残留缺口。

**路线 C（最低代价，止血）：只做「玻璃感增强」不做反色**

- 压暗 0.26 → 0.20，顶部白高光 0.28 → 0.34；
- 描边提到 `rgba(255,255,255,0.5)`，加一层极淡的背景色微染（如 `rgba(120,140,180,0.06)`）破掉「纯中性灰」的死板感。

→ 预期：视觉上从「实心灰板」回到「半透明玻璃」，但浅色下白字对比度仍不达标（约 1.8:1）。**只能作为过渡，不能当终态。**

### 5.4 顺带：菜单栏托盘图标也是「浅色偏灰」

用户说的「状态栏」如果也包含菜单栏图标，那这里是另一个独立问题，且证据确凿：

| 项 | 现状 | 问题 |
|---|---|---|
| 图标来源 | `lib.rs:487` `app.default_window_icon()` = **`32x32.png`** | 是橙色 App 图标，不是菜单栏图标素材 |
| 实际像素 | `#FF7E27` 橙，alpha=255 占 **90.6%**，仅 4 个圆角透明 | 完全不透明色块 |
| `icon_as_template` | **全仓零命中**（历史也从未写过） | 缺 macOS 模板图语义 |
| 尺寸处理 | `tray-icon` 硬编码缩到 **18pt**（`macos/mod.rs:296`） | 32→36 上采样，边缘发虚 |
| 浅色栏对比度 | **1.91 : 1** | 人眼即「脏灰」 |
| 深色栏对比度 | 7.67 : 1 | 所以「深色下正常」 |

**修法（Rust，约 15 行 + 2 个素材文件）**：

1. 新建 `icons/trayTemplate.png`（22×22）与 `@2x`（44×44）：**纯黑 + alpha、全透明底、无圆角方块底**，造型沿用「圆环 + 圆心」；
2. `lib.rs:487` 按平台取图（macOS 走 `include_bytes!("../icons/trayTemplate.png")`）；
3. `.icon(icon)` 后加 `.icon_as_template(true)`。

→ 预期：浅色栏变**纯黑**（17.6:1），深色栏系统自动翻**纯白**，点击自动反色。且 `icon_as_template` 是 macOS-only（`tauri/src/tray/mod.rs:561` 整段 `#[cfg(target_os = "macos")]`），对 Windows/Linux 零副作用。

⚠️ **不要**把新素材写进 `tauri.conf.json` 的 `bundle.icon`（`find_icon` 取第一个 `.png`，会污染 Windows 默认图标）。

---

## 六、落地顺序建议

**第一批（一次提交，解决 3 个 P0 的 70% 体感）**

1. `float_glass.rs` apply 幂等化（4 行）→ 残影/灰板/定标失效三件事一起改善
2. `screenshot/macos.rs:543` 改双信号（1 行）+ 删 `lib.rs:369` 启动预请求（10 行）
3. `commands.rs` F2 + F3（节流 6h、写时间戳后移、日志提级）
4. `Dashboard.vue` F1（加 6h interval）
5. `App.vue` P0-D（dismiss 落盘、不自动弹模态）

**第二批（需要一点设计，解决「脆」）**

6. `FloatBar.vue` 高度改 `100%` + `border-radius: 999px`（消除纵向错位）
7. 浅色反色（路线 A 或 B）
8. 托盘 template 图标（2 个素材 + 3 行）
9. conf 给 float 显式 `x/y` 与 `height:44`

**第三批（架构性，可排期）**

10. 权限中心卡片 + 统一状态源
11. 更新抓取镜像回落 + semver 加固
12. Developer ID + 公证（一次性根治 Gatekeeper + 授权漂移）

---

## 七、需 macOS 真机复核的项

以下结论本机（Windows）无法验证，建议在真机上按顺序确认：

1. **残影帧是否真的存在**：`float_window.rs:208` 前加 `tracing::info!(?window.outer_position(), ?window.inner_size(), "float show")`，与前端 `FloatBar.vue:513/545` 的 `performance.now()` 对比时间戳。若 show 早于 mounted → 方案 2 必需。
2. **材质层数**：Debug 构建里打印 `view.subviews()` 中 `tag == 91376254` 的个数，预计首次启动 3~4。
3. **浅色反色的实际阈值**：在纯白 / 浅灰 / 米色 / 深色四类壁纸下，按 0.02 步进扫 alpha，直到白字（或反色后的深字）实测 ≥4.5:1。
4. **`UnderWindowBackground` 在 macOS 26 的观感**：`translucency` 字段该在 26 上真正消费。
5. **托盘 template 化后的观感**：浅色栏应纯黑、深色栏自动纯白、点击反色。
