//!
//! float_glass.rs
//! 悬浮指标条的液态玻璃材质（v1.2.2，2026-10-21）。
//!
//! 设计：
//! - **只服务 `float` 窗口**。pet / pet-menu / capture / main 四个窗口一律不调用本模块，
//!   它们保持原有视觉（见需求边界）。`apply()` 内有 label 断言兜底。
//! - 系统材质路线：不自己画模糊，而是让操作系统合成器（DWM / WindowServer）在窗口背后
//!   渲染真实模糊。原因见 LIQUIDGLASS-FEASIBILITY-2026-10-21.md §1.3——CSS
//!   `backdrop-filter` 的取样源是**同一文档内的 DOM 背景**，采样不到操作系统桌面；
//!   本项目又把 html/body/#app 全设成了 transparent，所以那条 blur 实际是空转。
//! - 分平台能力分档（tier），由 `float_glass_support` 命令回传前端，用于设置页
//!   禁用开关 + 给出原因，避免"开了没效果"的死开关：
//!     macOS 26+          → liquid   （系统液态玻璃，含真实折射）
//!     macOS < 26         → vibrancy （NSVisualEffectView，有模糊无折射）
//!     Windows 10 1809+   → acrylic  （能力语义；**实际不调系统材质**，见下节）
//!     其他 / Linux       → none     （不支持，前端置灰）
//! - **幂等**：apply(true/false) 可反复调用；关闭时把两种材质都清一遍，
//!   因为历史状态可能残留其一。
//! - 失败**不阻断**：材质只是视觉增强，任何一步失败都只记 warn，浮窗退化为 CSS 兜底样式，
//!   绝不能让浮窗整体不可用。
//!
//! 修改历史：
//!   - 2026-10-21 @v1.2.2: 新增 - 液态玻璃材质层。含 support() 能力探测 + apply() 材质
//!     切换 + height_for() 模式相关高度。分平台实现：macOS 走 window-vibrancy 0.8.1 的
//!     apply_liquid_glass（NSGlassEffectView，需 26+）/ apply_vibrancy 兜底；
//!     Windows 走 apply_acrylic。Linux 返回不支持。
//!   - 2026-10-21 @v1.2.2 修订（真机实测后推翻初版）：**Windows 不再调用任何系统材质**，
//!     降级为纯 CSS 高透光。实测证据见下方「Windows 为什么不走系统材质」。
//!   - 2026-10-24 @v1.2.8: 修订 - macOS < 26 的材质由 `HudWindow`(13) 换为
//!     `UnderWindowBackground`(21)；新增 `GlassTier::native_blur()` 与
//!     `FloatGlassSupport.native_blur` 字段（**IPC 命令数不变**，仍是 85：本次只给
//!     `float_glass_support` 的返回体加字段，不新增命令）。原因：用户 macOS 15 截图
//!     实测发现 HudWindow 遮蔽过强，胶囊内亮度与正上方壁纸亮度相关系数仅 r≈0.02，
//!     桌面内容形状完全不可见，视觉上就是一块灰板。详见 `platform_apply` 内注释。
//!
//! ⚠️ 落地注意（血泪点，均由本次方案扫描发现，勿改回去）：
//!   1. 本模块的 apply() **不能**塞进 `float_window::ensure_float_window()`。
//!      因为 tauri.conf.json 已预创建 float 窗口，ensure 开头 `if exists { return Ok(()) }`
//!      会直接早返回 → 材质永远不生效。必须由调用方在 ensure 之后单独调用。
//!   2. macOS 的 apply_liquid_glass 内部用 `MainThreadMarker::new()` 硬校验主线程，
//!      非主线程返回 `Error::NotMainThread`。所以 `apply_via_app()` 内部统一用
//!      `run_on_main_thread` 派发（调用方不必自己包）。
//!   3. radius 必须与 FloatBar.vue 的 border-radius 取同一个数值（FLOAT_RADIUS_GLASS）。
//!      玻璃圆角由系统绘制，CSS 圆角若不一致会出现"系统圆角 + CSS 圆角"双层错角。
//!   4. macOS 这里**刻意不传** `content_view()`（不重父化 WKWebView）：
//!      - `with_webview` 的回调是 `FnOnce + Send + 'static`，闭包内无法借用外层
//!        window 句柄，重父化写法要额外绕一圈克隆 + 通道回传，复杂度与风险都高；
//!      - 玻璃视图以 `addSubview Below` 方式垫在窗口底层，而本浮窗 html/body/#app
//!        全透明 → 玻璃能正常透出，视觉目标已达成。
//!      - 若后续确认需要更"贴合"的折射（content 参与透镜计算），再单独加该优化，
//!        风险点见方案文档 P1-5（webview 层级可能被 Tauri 生命周期假设破坏）。
//!   5. **Windows 不要加回 apply_acrylic / Mica / backdrop-type**（见下节）。
//!
//! ⚠️ Windows 为什么不走系统材质（2026-10-21 真机实测，勿凭直觉改回）
//!   环境：Win11 25H2 build 26200.9711，125% 缩放，浅色主题，EnableTransparency=1。
//!   测法：把浮窗分别压在实测亮度 255（白）与 70（深色）两处背景上，各做
//!   「隐藏 → 截背景 / 显示 → 截浮窗」与「移开 → 截背景 / 移回 → 截浮窗」两套
//!   独立测量（两套结果一致且可复现），用透光率 ρ = 浮窗亮度差 / 背景亮度差 评判。
//!     纯 CSS（无系统材质）          ρ = 0.850   真透光
//!     SWCA Acrylic（自定义 tint）   ρ = 0.146   基本不透
//!     DWM TRANSIENTWINDOW          ρ = 0.001   完全不透（背景 255/70 时浮窗都是 212）
//!   `window-vibrancy` 0.8.1 在 build ≥ 22523 上走 `DWMWA_SYSTEMBACKDROP_TYPE =
//!   DWMSBT_TRANSIENTWINDOW` 分支，该分支**直接把 color 参数丢掉**（源码
//!   windows.rs:63-70 只传 kind），而对 `decorations:false` 的无边框窗口，
//!   DWM backdrop 不采样桌面，只铺一层主题色面板。结果是：开了"玻璃"反而把原本
//!   能透 85% 的 CSS 透明**盖成一块不透明亮板** —— 纯负优化。
//!   故 Windows 上 tier 仍报 `Acrylic`（能力层面系统确实支持 Acrylic），但
//!   `apply()` 明确不做任何系统调用，视觉 100% 由 `FloatBar.vue` 的 `.is-glass`
//!   决定。这样开关在所有 Windows 上都有稳定、可预期的高透光效果，也没有
//!   Acrylic 已知的拖拽/resize 掉帧风险（无需再验 W2）。
//!   代价：Windows 上没有**背景模糊**（只有高透光 + 描边 + 内高光）。要真模糊
//!   只能自研「截桌面 → 高斯模糊 → 当窗口背景」，涉及截屏权限、隐私与性能，
//!   需求方已明确暂不做。

use tauri::{Manager, WebviewWindow};

/// 浮窗固定 label（与 tauri.conf.json / float_window.rs 一致）
const FLOAT_WINDOW_LABEL: &str = "float";

/// 普通模式高度（逻辑像素）。与 `float_window::FLOAT_HEIGHT` 同值。
///
/// 为什么不直接复用 float_window 的常量：本模块是"模式 → 尺寸"的唯一口径来源，
/// `float_window` 通过 `height_for()` 取值。两个常量若分散两处，改一处漏一处就会出现
/// 「窗口 44px 但 CSS 按 36px 排版」的错位，故在此统一定义。
pub const FLOAT_HEIGHT_PLAIN: f64 = 36.0;

/// 液态玻璃模式高度（逻辑像素）。44px 是"胶囊形 + 折射可辨"的下限：
/// 36px 时折射扭曲区域只有数像素宽，肉眼几乎看不出（见可行性评估 §4.2）。
pub const FLOAT_HEIGHT_GLASS: f64 = 44.0;

/// 玻璃圆角 = 高度 / 2 = 完全胶囊形。
/// ⚠️ 必须与 `FloatBar.vue` 的 `.is-glass { border-radius }` 取同值。
///
/// Windows 侧不读它（Acrylic 无圆角参数，圆角由 CSS `border-radius` 决定），
/// 故非 macOS 编译时加 `allow(dead_code)` 避免死代码告警。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const FLOAT_RADIUS_GLASS: f64 = 22.0;

/// 玻璃模式的窗口高度（供 `float_window` 计算 set_size）
pub fn height_for(glass: bool) -> f64 {
    if glass {
        FLOAT_HEIGHT_GLASS
    } else {
        FLOAT_HEIGHT_PLAIN
    }
}

/// 材质能力档位。
///
/// `Liquid` / `Vibrancy` 只在 macOS 侧被构造，Windows 构建里会报 never constructed
/// → 整体允许死代码（枚举变体无法逐条 allow，且这只是编译期告警、无运行期影响）。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlassTier {
    /// macOS 26+：系统液态玻璃（半透明 + 背景模糊 + 真实折射）
    Liquid,
    /// macOS < 26：NSVisualEffectView（半透明 + 背景模糊，无折射）
    Vibrancy,
    /// Windows 10 1809+ / 11：DWM Acrylic（半透明 + 背景模糊，无折射）
    Acrylic,
    /// 不支持（Linux / 过老系统）
    None,
}

impl GlassTier {
    pub fn as_str(self) -> &'static str {
        match self {
            GlassTier::Liquid => "liquid",
            GlassTier::Vibrancy => "vibrancy",
            GlassTier::Acrylic => "acrylic",
            GlassTier::None => "none",
        }
    }

    /// 是否可用（任何非 None 档位都能给出"半透明 + 背景模糊"）
    pub fn supported(self) -> bool {
        self != GlassTier::None
    }

    /// 能力说明（给前端做 tooltip / 不支持原因）
    ///
    /// ⚠️ Windows 的文案必须如实说明"无背景模糊"：本模块在 Windows 上
    /// 刻意不调系统材质（见模块头实测），只说 "Acrylic: blur without refraction"
    /// 会让用户以为真有模糊，属误导。
    pub fn reason(self) -> &'static str {
        match self {
            GlassTier::Liquid => "macOS 26+ system glass with refraction",
            GlassTier::Vibrancy => "macOS vibrancy: blur without refraction",
            #[cfg(target_os = "windows")]
            GlassTier::Acrylic => WINDOWS_REASON,
            #[cfg(not(target_os = "windows"))]
            GlassTier::Acrylic => "Windows DWM Acrylic: blur without refraction",
            GlassTier::None => "requires Windows 10 1809+ or macOS 10.14+",
        }
    }

    /// 该档位下 `apply()` **真的**会给窗口铺上一层系统背景模糊吗？
    ///
    /// 这跟 `supported()` 是两回事，必须分开问：
    /// - `supported()` 回答"能不能开这个开关"（能力层面的可用性）；
    /// - `native_blur()` 回答"开了之后背后到底糊没糊"（视觉层面的事实）。
    ///
    /// 为什么要区分：前端要根据"背景是否已被系统模糊"来决定压暗系数，两种背景的
    /// 最优解是**相反**的 —— 有模糊时可以轻压暗让模糊透出来，没模糊时必须重压暗
    /// 才能保住文字对比度。若用 `supported()` 当判据，Windows（报 Acrylic 但实际
    /// 不调任何系统材质）会被误判成"有模糊"，前端就会用轻压暗 → 白字直接溶进桌面。
    ///
    /// 当前事实：
    /// - macOS 全部档位（Liquid / Vibrancy）→ true。两者都是真系统材质。
    /// - Windows（Acrylic）→ **false**。见模块头：DWM backdrop 对无边框窗口
    ///   透光率仅 0.001，纯负优化，已刻意不调用。
    /// - None → false。
    pub fn native_blur(self) -> bool {
        match self {
            GlassTier::Liquid | GlassTier::Vibrancy => true,
            GlassTier::Acrylic | GlassTier::None => false,
        }
    }

    /// 该档位的材质**透光程度**（0.0~1.0），供前端决定压暗强度。
    ///
    /// 为什么光有 `native_blur` 还不够（2026-10-25 复查发现的遗留缺陷）：
    /// `Liquid`（macOS 26，NSGlassEffectView，`Clear` 样式）与 `Vibrancy`
    /// （macOS 15，NSVisualEffectView，`UnderWindowBackground`）**都是真模糊**，
    /// `native_blur` 都是 true，于是前端会把它们塞进**同一个** `.has-blur` 配色档
    /// （压暗 0.26）。但这两个材质的透光量差别很大：
    ///   - `Clear` 是 Apple 官方最清透的样式（它存在的意义就是"尽量别挡"），
    ///     系统给的底色遮蔽最少 → 同样 0.26 压暗下，文字对比度会比 15 上更低；
    ///   - `UnderWindowBackground` 自带较厚的窗口底色 → 0.26 已经够。
    /// 换句话说：**越清透的材质越需要压暗**。用同一个系数覆盖两者，
    /// macOS 26 上很可能出现"字看不清"，而这是我在 Windows 上无法实测发现的。
    ///
    /// 取值口径（相对值，不是物理透光率）：
    /// - `Liquid`  → 0.85（Clear 样式，几乎不挡）
    /// - `Vibrancy`→ 0.55（UnderWindowBackground，自带窗口底色）
    /// - `Acrylic` → 0.0（实际未应用任何系统材质）
    /// - `None`    → 0.0
    ///
    /// ⚠️ 前端当前**尚未消费**这个字段（v1.2.8 保持行为不变，只先如实上报）。
    /// 待 macOS 26 真机验证后再据此分档，避免在没有实测数据的情况下瞎调。
    pub fn translucency(self) -> f32 {
        match self {
            GlassTier::Liquid => 0.85,
            GlassTier::Vibrancy => 0.55,
            GlassTier::Acrylic | GlassTier::None => 0.0,
        }
    }
}

/// 运行时探测当前系统能给的最高档位。纯探测、无副作用，可在任意线程调用。
pub fn support() -> GlassTier {
    platform_support()
}

// ===================== macOS =====================
#[cfg(target_os = "macos")]
fn platform_support() -> GlassTier {
    // NSAppKitVersionNumber >= 2685.0 对应 macOS 26.0（与 window-vibrancy 内部
    // 的 MIN_APPKIT_VERSION_LIQUID_GLASS 同口径）。低于此值走 NSVisualEffectView。
    const MIN_APPKIT_LIQUID_GLASS: f64 = 2685.0;
    // Safety: NSAppKitVersionNumber 是 AppKit 导出的只读全局常量，任意线程读取均安全。
    let ver = unsafe { objc2_app_kit::NSAppKitVersionNumber };
    if ver >= MIN_APPKIT_LIQUID_GLASS {
        GlassTier::Liquid
    } else if ver > 0.0 {
        // NSVisualEffectView 自 macOS 10.10 起可用（本项目 minimumSystemVersion = 12.0）
        GlassTier::Vibrancy
    } else {
        GlassTier::None
    }
}

#[cfg(target_os = "macos")]
fn platform_apply(window: &WebviewWindow, glass: bool) -> Result<(), String> {
    use window_vibrancy::{
        apply_liquid_glass, apply_vibrancy, clear_liquid_glass, clear_vibrancy,
        LiquidGlassOptions, NSGlassEffectViewStyle, NSVisualEffectMaterial, NSVisualEffectState,
    };

    if !glass {
        // 两种材质互斥，但历史状态可能残留其一，全清最稳。
        // clear_* 返回 Ok(false) 表示"本来就没应用过"，属正常情况，不算错误。
        //
        // ⚠️ v1.2.10（2026-10-26）：**改为循环清到干净**。原写法各调一次，而
        // clear_* 是 `NSView.viewWithTag:` 语义（一次只摘最近的一层），历史叠加
        // 若为 N 层，关掉玻璃开关后会残留 N-1 层灰板且不再有任何清除入口 ——
        // 这正是用户反馈「关掉玻璃还留一块灰板、重启才消失」的直接成因。
        // while 循环以 Ok(false) 收尾，有界（上限 = 现存层数），不会死循环。
        while matches!(clear_liquid_glass(window), Ok(true)) {}
        while matches!(clear_vibrancy(window), Ok(true)) {}
        return Ok(());
    }

    // v1.2.10（2026-10-26）：**apply 幂等化 —— 先清再铺，永远只留一层**。
    //
    // 为什么必须清（这是「胶囊比设计更暗」「关掉玻璃还留一块灰板」的直接成因）：
    //   `window-vibrancy 0.8.1` 的 apply 系列**不做去重** —— 每次调用都新建一个
    //   NSVisualEffectView / NSGlassEffectView 并 addSubview(Below)，见
    //   vibrancy.rs:42-59（没有任何「按 tag 复用」的判断）；而 clear 用的是
    //   `NSView.viewWithTag:`，语义只返回**最近的一个**匹配子视图，一次只删一层
    //   （vibrancy.rs:65-74）。于是 apply N 次 = N 层叠加，clear 1 次只删 1 层。
    //
    // 本应用的 apply 调用点有 4 处（float_window.rs:201 每次 show 前补一次、
    // lib.rs:694 启动恢复、commands.rs:1255 开关翻转、commands.rs:1259 关闭），
    // 实测首次启动就会叠到 3 层，之后每次进出全屏再 +1。后果有两个：
    //   ① 多层 tint 叠乘（1-(1-0.26)^3 ≈ 59%），把 v1.2.8 刚按真机定标的 0.26
    //      压暗档又抹回接近 0.48 的遮蔽量 —— 「刚定标完就失效」；
    //   ② 用户关掉玻璃开关后 clear 只删一层，剩下的层**永久留在窗口里**，
    //      表现为「浮窗上有一块擦不掉的灰板」。
    //
    // 为什么放在 match 之前：Liquid / Vibrancy 是两种互斥材质，但历史状态可能残留
    // 另一种（26 上升级/降级、或旧版本写过 Liquid），与 !glass 分支「全清最稳」同理。
    //
    // ⚠️ **必须用循环清，不能只调一次**：clear 走的是 `NSView.viewWithTag:`，只返回
    // 最近的一个匹配子视图、一次只摘一层（vibrancy.rs:65-74 /
    // liquid_glass.rs:165-181）。若窗口里已经叠了 3 层，单次 clear 后还剩 2 层，
    // 再铺一层又变 3 层 —— 等于没修。循环到返回 Ok(false)（没有可摘的了）为止才是
    // 真正的幂等。循环次数上限 = 当前叠加层数，有界，不会死循环；Err 也直接退出
    // （clear_* 的唯一 Err 来源是 UnsupportedPlatform / window_handle 获取失败，
    // 两者都属于「清不了」，继续循环无意义）。
    while matches!(clear_liquid_glass(window), Ok(true)) {}
    while matches!(clear_vibrancy(window), Ok(true)) {}

    match platform_support() {
        GlassTier::Liquid => {
            let opts = LiquidGlassOptions::new(NSGlassEffectViewStyle::Clear)
                .radius(FLOAT_RADIUS_GLASS)
                // 半透明玻璃必须 opaque(false)：否则系统垫一层实色背板，
                // 桌面完全透不进来，等于白做。
                .opaque(false);
            apply_liquid_glass(window, opts).map_err(|e| format!("apply_liquid_glass 失败: {e}"))
        }
        GlassTier::Vibrancy => {
            // macOS < 26 兜底：有模糊、无折射。
            //
            // v1.2.8 修订：材质由 HUDWindow(13) 换成 UnderWindowBackground(21)。
            // 为什么换（macOS 15 真机截图逐像素实测）：
            //   HUDWindow 是为 HUD 面板设计的**高遮蔽**材质，叠加 FloatBar 的深色底后，
            //   胶囊内部与正上方壁纸亮度的相关系数 r≈0.02 —— 桌面内容形状完全不可见，
            //   视觉上是一块半透明灰板，既不是毛玻璃也谈不上液态玻璃。用户原话
            //   「这个玻璃不能模拟液态玻璃吗」，本质是"连模糊都看不见"。
            //   UnderWindowBackground 是窗口背景之下那一层，透光率显著高于 HUDWindow，
            //   是"能看清背后被模糊的内容"这一目标最贴的语义材质（Apple 文档：
            //   "The material used under window backgrounds"）。
            // ⚠️ 备选 Popover(6) / Sidebar(7) 也可用；若真机觉得 21 还是太薄，
            //   优先试 Popover（更接近 macOS 弹窗的磨砂感），**不要**改回 HudWindow。
            //
            // state=Active 保留：浮窗长期无焦点，FollowsWindowActiveState 会让材质
            // 在失焦时塌成灰，这点与材质选择无关，不能一起换掉。
            apply_vibrancy(
                window,
                NSVisualEffectMaterial::UnderWindowBackground,
                Some(NSVisualEffectState::Active),
                Some(FLOAT_RADIUS_GLASS),
            )
            .map_err(|e| format!("apply_vibrancy 失败: {e}"))
        }
        _ => Err("macOS 无法判定玻璃材质能力".to_string()),
    }
}

// ===================== Windows =====================
#[cfg(target_os = "windows")]
fn platform_support() -> GlassTier {
    // 早先按 build >= 17763（Win10 1809）判定 Acrylic 可用性。现在 Windows 侧
    // 已改为纯 CSS 高透光（见模块头实测），CSS 透明不依赖任何系统版本，
    // 因此不再设版本门槛 —— 所有 Windows 上开关都可用且效果一致。
    //
    // tier 仍报 `Acrylic`：这是**能力语义**（系统本身支持 Acrylic），前端类型
    // `FloatGlassTier` 也以此为准；但 `reason()` 会如实说明实际走的是
    // "无模糊的高透光"，`platform_apply()` 也不会调用任何系统材质。
    GlassTier::Acrylic
}

/// Windows 侧支撑判定的说明文案。
///
/// 与 macOS 不同，这里要如实告诉用户"没有背景模糊"，避免开了开关期待落空。
#[cfg(target_os = "windows")]
const WINDOWS_REASON: &str =
    "Windows: high-transparency glass (no system backdrop blur); \
     Acrylic renders opaque on borderless windows";

#[cfg(target_os = "windows")]
fn platform_apply(window: &WebviewWindow, glass: bool) -> Result<(), String> {
    // 见模块头「Windows 为什么不走系统材质」：实测 DWM backdrop 对本窗口
    // 完全不透光（透光率 0.001），会把 CSS 的高透光盖成不透明亮板。
    // 因此这里**刻意什么都不做** —— 视觉全部交给 FloatBar.vue 的 `.is-glass`。
    //
    // 但历史运行可能残留过旧版本的 Acrylic 状态（用户从旧版升级上来，
    // 窗口被系统记了 backdrop），所以 glass=false 时仍主动清一次，保证幂等。
    if !glass {
        // 清不掉也不算错：本来就没应用过时 clear_acrylic 返回 Ok；
        // 若平台行为变化导致 Err，也只记 warn，不阻断浮窗。
        if let Err(e) = window_vibrancy::clear_acrylic(window) {
            tracing::debug!(error = %e, "clear_acrylic 未生效（通常表示本来就没应用过）");
        }
    }
    Ok(())
}

// ===================== Linux / 其他 =====================
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_support() -> GlassTier {
    // window-vibrancy 明确 Unsupported：模糊由用户桌面合成器决定，无统一 API。
    // 前端据此置灰开关，浮窗退回 CSS 半透明兜底。
    GlassTier::None
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_apply(_window: &WebviewWindow, _glass: bool) -> Result<(), String> {
    Err("当前平台不支持系统级玻璃材质".to_string())
}

/// 对浮窗应用/清除液态玻璃材质。
///
/// **必须在主线程调用**（macOS 硬要求）。通常不直接调它，而用 `apply_via_app()`
/// ——后者已封装主线程派发与尺寸同步。
///
/// 幂等：`glass=false` 会清掉两种材质，可安全重复调用。
pub fn apply(window: &WebviewWindow, glass: bool) -> Result<(), String> {
    if window.label() != FLOAT_WINDOW_LABEL {
        // 防御性断言：本模块只服务 float 窗口。别的窗口一旦误调直接拒绝，
        // 避免 pet / capture 等窗口被意外套上材质（需求明确要求它们保持原样）。
        return Err(format!(
            "float_glass 仅支持 `{FLOAT_WINDOW_LABEL}` 窗口，收到 `{}`",
            window.label()
        ));
    }
    platform_apply(window, glass)
}

/// 便捷封装：应用材质 + 同步窗口高度（内部负责主线程派发）。
///
/// 供 `commands.rs::set_status_bar_config` 与 `lib.rs` 启动恢复段复用，
/// 避免调用方各自写一遍 run_on_main_thread 与"窗口不存在"的判空逻辑。
///
/// ⚠️ 为什么一定要在这里同步高度：窗口高度由前端 `resize_float_window` 按 1Hz 驱动，
/// 若只改配置不主动 set_size，要等下一次 tick（最多 1s）才变高，用户会看到
/// "开了开关 → 材质先出来 → 一秒后高度才跳"的割裂感。
///
/// 材质失败**不影响**尺寸切换（两者独立兜底）。
pub fn apply_via_app(app: &tauri::AppHandle, glass: bool) -> Result<(), String> {
    let window = app
        .get_webview_window(FLOAT_WINDOW_LABEL)
        .ok_or_else(|| "float 窗口尚未创建".to_string())?;
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = apply(&window, glass) {
            tracing::warn!(
                error = %e,
                glass,
                "应用液态玻璃材质失败，浮窗退化为 CSS 兜底样式"
            );
        }
        // 高度与材质同一批次落定，避免视觉跳变
        if let Some(w) = app2.get_webview_window(FLOAT_WINDOW_LABEL) {
            let scale = w.scale_factor().unwrap_or(1.0);
            let cur_w = w
                .inner_size()
                .map(|s| s.to_logical::<f64>(scale).width)
                .unwrap_or(340.0);
            let _ = w.set_size(tauri::LogicalSize::new(cur_w, height_for(glass)));
        }
    })
    .map_err(|e| format!("主线程派发失败: {e}"))
}

/// IPC 返回体（与前端 `src/types.ts` 的 FloatGlassSupport 一一对应，snake_case 原样返回）
#[derive(serde::Serialize)]
pub struct FloatGlassSupport {
    pub supported: bool,
    /// "liquid" | "vibrancy" | "acrylic" | "none"
    pub tier: String,
    /// 能力说明 / 不支持原因
    pub reason: String,
    /// v1.2.8：本次是否**真的**应用了系统背景模糊（不是"能力上支持"）。
    /// 前端据此在"轻压暗（让模糊透出）"与"重压暗（保对比度）"两档配色间切换。
    /// 见 `GlassTier::native_blur()` 的完整说明。
    pub native_blur: bool,
    /// v1.2.8：材质透光程度（0.0~1.0），供前端细分压暗强度。
    /// ⚠️ 前端**当前尚未消费**，仅如实上报以便 macOS 26 真机验证后分档。
    /// 见 `GlassTier::translucency()` 的完整说明（为什么 Liquid 与 Vibrancy
    /// 不能共用同一个压暗系数）。
    pub translucency: f32,
}

/// Tauri 命令：查询当前系统的玻璃材质能力（设置页开关的可用性依据）。
#[tauri::command]
pub fn float_glass_support() -> FloatGlassSupport {
    let tier = support();
    FloatGlassSupport {
        supported: tier.supported(),
        tier: tier.as_str().to_string(),
        reason: tier.reason().to_string(),
        native_blur: tier.native_blur(),
        translucency: tier.translucency(),
    }
}
