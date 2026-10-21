<!--
  src/float/FloatBar.vue
  悬浮指标条窗口（v0.7.6 引入，v0.7.7 重做）：透明置顶无边框 webview，1Hz 显示系统指标。

  设计：
  - 由 App.vue 按 webview label === 'float' 分流渲染（与 pet / pet-menu 同模式）
  - onMounted 局部把 html/body/#app 背景设为 transparent（学 PetMenuWindow.vue，
    只动本 webview 文档，绝不用全局 CSS，避免污染主窗口——见 beta.17 回归根因）
  - 数据：1Hz invoke get_status_bar_config + get_system_metrics（与托盘采样独立，
    多一组轻量 IPC，代价可忽略）；show_cpu / show_mem / show_disk / show_net 与托盘共用配置
  - 拖拽：整条都是拖拽区，指针按下即 startDragging()（OS 接管零延迟）；结束后读
    outerPosition/scaleFactor 换算逻辑坐标写 localStorage（float_pos），下次挂载恢复
  - 显隐由 Rust 侧管理（托盘菜单/设置页开关 + 全屏自动隐藏），本组件只负责渲染与位置

  修改历史：
  - 2026-09-10 @v0.7.6: 初始创建
  - 2026-09-10 @v0.7.7: ① 修 bug——cpu_usage / memory_usage 后端返回的是 0~1 分数，
      旧代码 Math.round(x) 直接当百分比，导致 77% 显示成 1%（漏乘 100）；
    ② 新增内存 / 磁盘显示（后端已补齐三平台采样，不再限 macOS）；
    ③ 宽度自适应：窗口固定 340px 而内容常有 ~190px，右侧大片空白被用户反馈「浮窗过长」。
       改为测量内容宽度后调 resize_float_window，并在 show 之前完成测量（用户看不到宽态）；
    ④ 视觉重做：36px 高度、更紧凑的间距、tabular-nums 防数字跳动、
       标签弱化 / 数值加粗、网速上下行分色、指标组之间加分隔线。
  - 2026-09-10 @v0.7.7（续）: ⑤ 宽度贴合加固——用户反馈「网速内容溢出（被裁）」。三处：
      a) 测量改为 rect.width 与 scrollWidth 取较大者，余量 +1px → +2px。
         原因：窗口宽度与内容同为逻辑像素，但 set_size 落到物理像素时会被取整
         （Windows 125%/150% 缩放常见），视口可能比内容少 1px，正好裁掉末位字符的尾巴；
      b) 新增 ResizeObserver 兜底：字体异步就绪 / DPI 变化 / 语言切换 / 网速位数变多
         都会改变内容真实宽度，只靠 1Hz tick 测量会漏掉「中间态」，用户会看到一闪的裁切。
         元素是 width:max-content（固有宽度与视口无关），窗口 resize 不会反向触发它，
         不存在 RO → resize → RO 死循环，外加 appliedW 阈值判断双重保险；
      c) 百分比与网速数值定宽 + 右对齐：位数变化（7%→100%、↓75K→↓12.7M）不再每秒
         改变内容宽度——旧实现整条会随数字「呼吸」，窗口跟着 resize 视觉上很毛躁。
         定宽后浮窗宽度稳定，数字也排成整齐的右对齐列；真超出时仍由 fitWidth 兜底跟随。
  - 2026-09-17 @v0.8.2: 尾部新增「截图」按钮。可见 = 截图开启 && 浮窗开启（cfg 1Hz
      轮询自动联动，设置页翻转开关 ≤1s 生效，无新增 IPC/事件）；pointerdown.stop
      防止点按钮误触发整条拖拽；点击走 screenshot_trigger（与快捷键同路径），
      begin_capture 会先隐藏 pet/float 再抓屏、结束后自动恢复。配置来源：StatusBarConfig
      新增 screenshot_enabled 字段（权威源仍是 screenshot 模块的 screenshot_enabled 键，
      set_status_bar_config 用 DB 权威值兜底，防设置页旧快照回写）。
  - 2026-10-17 @v1.2.0: 移除 - 取消浮窗尾部「重复上次区域」按钮（v1.1.0 引入的
      onShotRepeat + 图标）。用户反馈浮窗按钮与设置页入口重复、且无历史区域时点了没
      反应易困惑；设置页的同名功能保留不受影响。
      ⚠️ v1.2.2 更新：设置页那个入口随后也一并删除了，「重复上次截图」功能现已整体下线
      （后端 `screenshot_repeat_region` 与 `last_region` 状态同时移除）。
  - 2026-10-17 @v1.2.0: 新增 - 最左侧状态点 `.f-dot` 由固定 accent 橙改为**按系统负载
      动态变色**。负载值取 CPU/内存使用率的较高者，划分 5 档（idle 绿 / light 黄绿 /
      busy 橙 / heavy 深橙 / overload 红）+ unknown 灰（数据源不可用兜底），阈值
      0.15/0.35/0.65/0.85（左闭右开），CSS 0.35s 平滑过渡避免 1Hz 采样闪烁；
      tooltip 显示档位 + 实时 CPU/内存百分比。颜色集中在 `LOAD_COLORS` 单一来源。
  - 2026-10-20 @v1.2.1: 修复 - macOS「状态灯不变色」。v1.2.0 的 peak=max(cpu,mem)，
      而内存占用变化慢、Mac 又长期维持 70~80%（wired/compressor/缓存），状态灯被内存
      长期钉在橙档、CPU 回落也不变绿（Windows 内存读数更低更跳故正常）。改为**以 CPU
      瞬时值为主信号**：内存 < 80%（MEM_WARN）时只跟 CPU；≥80% 才参与抬升（80~85% 深橙、
      ≥85% 红）。既让灯随实时活动变色，又保留内存快占满时变红。
  - 2026-10-20 @v1.2.2: 修复 - macOS 状态灯**仍然恒橙**（v1.2.1 未真正修好）。
      v1.2.1 把内存门槛设成 `>= 0.8`，与 macOS 内存**日常基线（70~80%）正好重合**
      → 实测 MEM 80% / CPU 23% 时仍判 heavy 亮橙。现重新定标：
      ① CPU 独立决定基础档位（0.15/0.35/0.65/0.85 不变）；
      ② 内存只在**高水位**抬升——`MEM_HEAVY = 0.9` 抬到 heavy、`MEM_OVERLOAD = 0.95` 直接红；
      ③ 内存**绝不降档**，也不会把 CPU 的低负载误判成繁忙。
      即 MEM 80% 这类 macOS 常态不再影响灯色，CPU 23% 会如实显示「轻载」黄绿。
      教训：跨平台阈值必须定在**各平台常态基线之上**，压在基线上等于永远触发的死阈值。
  - 2026-10-21 @v1.2.2: 新增 - 液态玻璃外观（`cfg.float_glass` 为真时生效）。
      ① 几何：36px/圆角 18 → **44px/圆角 22**（完全胶囊形）。44px 是折射可辨的下限，
         36px 时折射扭曲区只有数像素宽，肉眼几乎看不出（见可行性评估 §4.2）。
         ⚠️ 三个数值必须与 `float_glass.rs` 的 `FLOAT_HEIGHT_GLASS` / `FLOAT_RADIUS_GLASS`
         以及 Rust 侧 `set_size` 保持一致，否则会出现「窗口 44 但内容按 36 排版」的错位。
      ② **背景改为近乎全透明 + 去掉 backdrop-filter**：玻璃模式下模糊/折射由系统合成器
         （macOS NSGlassEffectView）在窗口背后渲染，属于**窗口外**的采样；CSS
         `backdrop-filter` 只采样**同文档 DOM 背景**，而本组件 onMounted 已把
         html/body/#app 全设 transparent → 那条 blur 在本窗口里一直是空转（不是本次改动
         引入的问题，是从来没生效）。玻璃模式下保留深色底反而会把系统模糊完全盖住，
         故降到 rgba(255,255,255,0.06) 只留一点点「玻璃自身的乳白」。
      ③ 文字配色不变（#f5f5f7 白系），靠 `.is-glass` 的细描边 + 文字阴影兜住对比度。
      ④ 由根节点 `:class` 绑定切换，不新增任何 IPC；配置随既有 1Hz `get_status_bar_config`
         轮询自动跟随，设置页翻开关 ≤1s 生效（与截图按钮同机制）。
  - 2026-10-21 @v1.2.2 修订（真机实测后）：**Windows 上系统材质不可用，改为纯 CSS
      高透光**。实测 Win11 25H2 build 26200：DWM backdrop 对本无边框窗口完全不透光
      （浮窗亮度在背景 255/70 时都是 212，透光率 ρ=0.001），而纯 CSS 的 ρ=0.850。
      也就是说原来的 `apply_acrylic` 反而把高透光盖成了不透明亮板（纯负优化）。
      Windows 侧已停用系统材质（见 `float_glass.rs` 模块头），因此这里的 CSS 是
      **Windows 上唯一的视觉来源**，必须自己撑起玻璃质感：
        - 底色从 0.06 提到 0.10：Windows 下背后是**未模糊**的原始桌面，像素噪声大，
          过低的底色会让统计数字直接溶进背景（白墙前尤其明显）。
        - 描边升到 0.28 + 外阴影 0 6px 20px：无系统材质时，胶囊轮廓只能靠这两条读出来。
        - 文字阴影加深到 0.55：白墙前 #f5f5f7 若不加阴影几乎不可读。
      macOS 仍是真系统玻璃（底色 0.06 已够），故用 `@media` 无法区分平台 → 由 Rust 侧
      `body` 的 `data-platform` 不太值当；这里取**两边都能接受**的折中值，
      以 Windows（当前唯一实测平台）的可读性为准。
  - 2026-10-21 @v1.2.3（用户截图实测可读性）：**液态玻璃对比度不达标，重做配色**。
      用户提供 Win11 液态玻璃浮窗截图，逐像素解析后量到胶囊内底色 **#7f9bea**（中浅蓝壁纸
      透过 `rgba(255,255,255,0.10)` 的结果），WCAG 对比度：
      主数字 2.69:1、标签（白 50%）1.70:1、网速↓ 1.60:1、网速↑ 1.54:1 —— **全部远低于 4.5:1**。
      根因：底色写的是「加白」，在浅色壁纸上把胶囊推到接近白，白字与底亮度差被压到几乎为零；
      深色壁纸下反而正常（5.25:1），**问题只在浅/中色背景暴露**，极易漏测。
      `text-shadow: 0 1px 3px .55` 也救不回来——3px 模糊半径偏散，只让字变"糊"而非变清楚。
      修复（两条腿同时走）：
        ① **底色由「纯加白」改为「压暗 + 顶部白高光」**：
           `linear-gradient(白 0.16 → 0.02)` 顶高光 + `rgba(0,0,0,a)` 压暗。
           压暗是让亮壁纸变暗、暗壁纸更暗（单向前进），高光仍保留玻璃的"上边缘反光"，
           比原 `rgba(255,255,255,0.10)` 单向加白的覆盖面大得多。
        ② **文字整体提亮 + 网速色改亮色系**：标签 50%→82%、网速 ↓#6fd3ff→#9ee8ff、
           ↑#ffb46b→#ffd8a8（原色在浅底上只有 1.5:1，属"彩色好看但看不清"）。
        ③ 文字阴影由 `0 1px 3px .55` 收紧为 `0 1px 2px .62, 0 0 1px .5`：边缘更实，
           不再把 11px 的墨迹糊成 17px 的虚影。
        ④ 几何：玻璃模式字号 **12→13px**（文字/胶囊占比 27.3%→29.5%，接近普通模式手感；
           44px 胶囊配 12px 字显得"框胖字小"）；`.f-item > b` 槽位 `2.7em`→`2.45em`
           （实测 `100%` 仍放得下）。**实测宽度 400.7→384.2px**（槽位收窄量超过了字号增量，
           净变窄），仍在 fitWidth 安全区。
      ⑤ **压暗系数经真实渲染定标：0.30 → 0.48，标签 72%→82%**（用户截图壁纸 #7190e8 上实测）。
         用 Edge 无头渲染本文件**真实的 CSS**（从 .vue 提取，非手工复刻）后逐像素采样：
           0.30 时：白主数字 4.51:1（刚过线）、标签 3.25:1、网速↓ 4.27:1、网速↑ 4.09:1 —— **三项不达标**。
         根因是数学硬约束：0.30 时胶囊底色 L≈0.183，要达 4.5:1 文字亮度须 ≥0.9686，
         **纯白 L=1.0 就是唯一天花板**，任何带色文字（#9ee8ff L=0.72）物理上不可能达标。
         解法二选一：把网速调成近白（丢颜色语义），或加深压暗。选后者。
         ⚠️ 定标踩过的坑（记录下来防重犯）：先按 CSS alpha 合成公式**纯数学推算**得 0.44
         （理论值 6.16:1），但真实渲染只有 5.49:1 — 公式忽略了白渐变在文字带的实际值
         与 inset 高光叠加，**系统性高估对比度约 0.6**。改逐档真实渲染扫描后确认：
         0.44 时网速↓ 只有 4.48:1（差 0.02），**0.48 才是真下限**。
         0.48 实测：白 6.59 / 网速↓ 4.85 / ↑ 4.91 / 标签(82%) 5.08 —— **四项全部过 AA**。
         另一处纠错：0.30 方案曾写成「网速 3.4:1 达标」，是**误用了大字号 3:1 阈值**；
         13px 属正文本，阈值是 4.5:1，3.4:1 不达标。
      ⚠️ **已知边界（实测数据，不是估计）**：0.48 覆盖深/中色壁纸（用户截图那张全部达标）；
      **浅灰/纯白壁纸仍不达标**（主数字约 1.9:1）——亮壁纸本身漏光多，
      要覆盖需压暗到 0.60 以上，那样已接近深色磨砂、不再是液态玻璃。
      若后续要全壁纸达标，唯一正解是按壁纸亮度**自适应**压暗系数
      （需 Rust 侧抓屏采样，属新增能力，不在本版范围）。
      另：`overflow: hidden` 保留（防极端情况裁空），但 `fitWidth()` 每秒随内容伸缩窗口，
      实测 12 种数值组合 overflow 全为 false（13px + 2.45em 下最宽 457.2px）。
-->
<template>
  <div
    ref="rootRef"
    :class="['float-bar', { 'is-glass': cfg.float_glass }]"
    @pointerdown="onPointerDown"
    @contextmenu.prevent
  >
    <span class="f-dot" :style="dotStyle" :title="dotTitle"></span>
    <span v-if="cfg.show_cpu" class="f-item">
      <i>CPU</i><b>{{ pct(metrics?.cpu_usage) }}</b>
    </span>
    <span v-if="cfg.show_mem" class="f-item">
      <i>MEM</i><b>{{ pct(metrics?.memory_usage) }}</b>
    </span>
    <span v-if="cfg.show_disk" class="f-item">
      <i>DSK</i><b>{{ pct(metrics?.disk_usage) }}</b>
    </span>
    <template v-if="cfg.show_net">
      <span v-if="hasAnyBefore" class="f-sep"></span>
      <span class="f-item f-net">
        <b class="down">↓{{ fmtRate(metrics?.net_rx_bps ?? 0) }}</b>
        <b class="up">↑{{ fmtRate(metrics?.net_tx_bps ?? 0) }}</b>
      </span>
    </template>
    <!-- v0.8.2：尾部「截图」按钮。可见 = 截图开启 && 浮窗开启；pointerdown.stop
         防止点按钮误触发整条拖拽；点击走 screenshot_trigger（与快捷键完全同路径） -->
    <template v-if="shotEnabled">
      <span v-if="hasAny" class="f-sep"></span>
      <button
        type="button"
        class="f-shot"
        :title="t('float.shot')"
        @pointerdown.stop
        @click="onShot"
      >
        <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
          <path d="M2 5.2V2h3.2M13.8 5.2V2h-3.2M2 10.8V14h3.2M13.8 10.8V14h-3.2" />
          <circle cx="8" cy="8" r="2.4" />
        </svg>
      </button>
    </template>
    <span v-if="!hasAny && !shotEnabled" class="f-item"><b class="f-dim">ScreenTime Pro</b></span>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
// Tauri v2：monitor API 为模块级函数（Window 实例上没有 currentMonitor）
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { tracker } from "../api/tracker";
import { screenshot } from "../api/screenshot";
import type { MetricsOut, StatusBarConfig } from "../types";
import { log } from "@/lib/logger";

// v0.8.2：按钮 tooltip 文案（浮窗此前无任何用户可见文案，首次引入 useI18n）
const { t } = useI18n();

/** 与 float_window.rs FLOAT_INITIAL_WIDTH 一致（逻辑像素），默认位计算用 */
const BAR_W = 340;
/** 宽度安全余量（逻辑像素）：抵消 set_size 物理像素取整造成的视口不足，见文件头 ⑤a */
const WIDTH_SLACK = 2;
const POS_KEY = "float_pos";

const rootRef = ref<HTMLElement | null>(null);
const cfg = ref<StatusBarConfig>({
  enabled: false,
  show_cpu: true,
  show_mem: true,
  show_disk: false,
  show_net: true,
  float_enabled: true,
  screenshot_enabled: true,
  // v1.2.2：默认 false，首次 tick 由后端配置覆盖
  float_glass: false,
});
const metrics = ref<MetricsOut | null>(null);
let timer: number | null = null;
/** 上次已应用的内容宽度，避免每秒无谓 IPC */
let appliedW = 0;
/** 拖拽进行中标记：拖拽期间不调整尺寸，避免与 OS 拖拽循环抢窗口状态 */
let dragging = false;
/**
 * 内容宽度观察器（v0.7.7 加固，见文件头 ⑤b）：
 * 字体异步就绪、DPI 变化、语言切换、网速位数变多都会改变内容真实宽度，
 * 只靠 1Hz tick 测量会漏掉「中间态」——用户会看到一闪的裁切。
 */
let resizeObs: ResizeObserver | null = null;

const hasAny = computed(
  () => cfg.value.show_cpu || cfg.value.show_mem || cfg.value.show_disk || cfg.value.show_net,
);
/** 网速组之前是否有别的指标（决定要不要画分隔线） */
const hasAnyBefore = computed(
  () => cfg.value.show_cpu || cfg.value.show_mem || cfg.value.show_disk,
);
/**
 * v0.8.2：截图按钮可见性 = 截图开启 && 浮窗开启。
 * cfg 每秒轮询 get_status_bar_config，设置页翻转任一开关后 ≤1s 生效；
 * float_enabled 关闭时窗口本身会被 Rust 隐藏，这里一并判断只为语义完整
 * （防配置短暂脱节时按钮残留）。
 */
const shotEnabled = computed(() => cfg.value.screenshot_enabled && cfg.value.float_enabled);

/** 0~1 分数 → 百分比整数串（后端 cpu_usage / memory_usage / disk_usage 均为分数） */
function pct(v: number | undefined): string {
  const n = Math.round((v ?? 0) * 100);
  return `${Math.max(0, Math.min(100, n))}%`;
}

/**
 * 状态灯档位（v1.2.0 引入，v1.2.2 重新定标）：负载「空闲 → 过载」5 档 + 未知兜底。
 *
 * 负载值以 **CPU 瞬时使用率为主信号**；内存只在**真正吃紧**时才参与抬升。
 *
 * 为什么内存不能平权参与（v1.2.0 教训）：
 *   内存占用变化极慢且平台基线差异巨大。macOS 采样是
 *   `used = total - (free + inactive + speculative)`，把 wired（内核不可换出）+ compressor
 *   压缩页 + 活跃应用全算作已用，**日常就是 70~80%**，属系统内存策略而非「系统吃紧」。
 *   v1.2.0 取 `peak=max(cpu,mem)` → 灯被内存长期钉在橙档；v1.2.1 把门槛设成 `>= 0.8`，
 *   与 macOS 常态线**恰好重合** → 依旧恒橙（用户实测 MEM 80%/CPU 23% 仍亮橙）。
 *   结论：内存门槛必须显著高于各平台常态基线，且不参与「繁忙」档，只做**高水位告警**。
 *
 * 阈值划分（CPU 主信号；越高越热，边界左闭右开）：
 *   idle     < 0.15   空闲（几乎无负载）
 *   light    0.15–0.35 轻载（日常使用）
 *   busy     0.35–0.65 繁忙（中高负载，开始发热）
 *   heavy    0.65–0.85 高载（橙色告警）
 *   overload ≥ 0.85   过载/接近占满（红色）
 *   unknown  数据源不可用（supported/enabled 为假、或数值非有限）时灰色兜底，
 *            避免指标缺失时误判成「空闲」误导用户。
 */
type LoadLevel = "idle" | "light" | "busy" | "heavy" | "overload" | "unknown";

/**
 * 内存「吃紧」门槛（v1.2.2 定标）。高于 macOS 日常 70~80% 的常态占用，
 * 只有真正接近耗尽才参与档位，避免内存把灯长期钉死。
 */
const MEM_HEAVY = 0.9;
/** 内存「接近占满」门槛：到这一档直接判过载（红）。 */
const MEM_OVERLOAD = 0.95;

function loadLevel(m: MetricsOut | null): LoadLevel {
  if (!m || !m.supported || !m.enabled) return "unknown";
  const cpu = m.cpu_usage;
  const mem = m.memory_usage;
  if (!Number.isFinite(cpu) || !Number.isFinite(mem)) return "unknown";
  // CPU 决定基础档位（实时活动信号）
  const c = Math.max(0, Math.min(1, cpu));
  let level: LoadLevel =
    c >= 0.85 ? "overload" : c >= 0.65 ? "heavy" : c >= 0.35 ? "busy" : c >= 0.15 ? "light" : "idle";
  // 内存只做「高水位抬升」，且只抬到 heavy/overload，绝不因内存把灯压到低档
  if (mem >= MEM_OVERLOAD) return "overload";
  if (mem >= MEM_HEAVY && (level === "idle" || level === "light" || level === "busy")) {
    return "heavy";
  }
  return level;
}

/** 各档位颜色（与 set 内联 `dotStyle` 共用同一份，避免散落两处口径不一致） */
const LOAD_COLORS: Record<LoadLevel, string> = {
  idle: "#34c759", // 绿 —— 空闲
  light: "#7ed321", // 黄绿 —— 轻载（比空闲更亮一档，仍属「健康」区间）
  busy: "#ff9f0a", // 橙 —— 繁忙（中高负载）
  heavy: "#ff7e27", // 深橙 —— 高载（沿用品牌 accent 橙，接近告警）
  overload: "#ef4444", // 红 —— 过载/占满
  unknown: "#8e8e93", // 灰 —— 未知/异常
};

/** 状态点内联样式：背景色 + 同色光晕（透明度随颜色而异，统一 0.55 发光强度） */
const dotStyle = computed(() => {
  const c = LOAD_COLORS[loadLevel(metrics.value)];
  return { background: c, boxShadow: `0 0 6px ${c}` };
});

/** 状态点 tooltip：中文给具体档位 + 实时负载值 + 判定规则，方便悬停确认阈值归属 */
const dotTitle = computed(() => {
  const lv = loadLevel(metrics.value);
  const m = metrics.value;
  if (lv === "unknown") return t("float.dotUnknown");
  const cpu = Math.round((m?.cpu_usage ?? 0) * 100);
  const mem = Math.round((m?.memory_usage ?? 0) * 100);
  const map: Record<Exclude<LoadLevel, "unknown">, string> = {
    idle: "float.dotIdle",
    light: "float.dotLight",
    busy: "float.dotBusy",
    heavy: "float.dotHeavy",
    overload: "float.dotOverload",
  };
  // v1.2.2：附上判定规则——用户曾因「MEM 80% 却不变色」困惑，明确写出内存只在
  // ≥90% 才抬档，避免再被 macOS 常态高内存读数误导成 bug。
  return `${t(map[lv], { cpu, mem })}\n${t("float.dotRule")}`;
});

/** 字节/秒格式化（与 Rust format_bps 同口径：B / K / M / G 自适应） */
function fmtRate(bps: number): string {
  if (!Number.isFinite(bps) || bps < 0) return "0B";
  if (bps < 1024) return `${Math.round(bps)}B`;
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(bps < 10240 ? 1 : 0)}K`;
  if (bps < 1024 * 1024 * 1024) return `${(bps / 1048576).toFixed(1)}M`;
  return `${(bps / 1073741824).toFixed(1)}G`;
}

/**
 * v0.8.2：浮窗一键截图。与快捷键 / 设置页「立即截图」完全同路径（screenshot_trigger）：
 * Rust 侧 enabled=false 时直接忽略（按钮此时本就隐藏，双保险）；
 * begin_capture 会先隐藏 pet/float 再抓屏，提交/取消后自动恢复，无需本组件清理。
 */
async function onShot(): Promise<void> {
  try {
    await screenshot.trigger();
  } catch {
    /* 应用退出中偶发失败，忽略 */
  }
}

/**
 * 宽度自适应：测量内容真实宽度并收缩窗口（Rust 侧做右边缘锚定）。
 *
 * 测量取 getBoundingClientRect().width 与 scrollWidth 的**较大者**：
 * 前者是元素 border-box 宽度（width:max-content 下即内容固有宽度，含 padding/border），
 * 后者在「视口比内容窄」时会给出内容实际需要宽度——两者取大最稳，不会漏算。
 * 再 +WIDTH_SLACK 抵消 set_size 的物理像素取整（见文件头 ⑤a）。
 */
async function fitWidth(): Promise<void> {
  if (dragging) return;
  await nextTick();
  if (dragging) return;
  const el = rootRef.value;
  if (!el) return;
  const need = Math.max(el.getBoundingClientRect().width, el.scrollWidth);
  const w = Math.ceil(need) + WIDTH_SLACK;
  if (Math.abs(w - appliedW) < 1) return;
  appliedW = w;
  try {
    await invoke("resize_float_window", { width: w });
  } catch {
    /* 窗口尚未创建 / 退出中，忽略 */
  }
}

/** 恢复上次位置；无记录则放主屏右上角（顶 24px / 右 24px） */
async function applyPosition(): Promise<void> {
  const raw = localStorage.getItem(POS_KEY);
  if (raw) {
    try {
      const p = JSON.parse(raw) as { x: number; y: number };
      await invoke("move_float_window", { x: p.x, y: p.y });
      return;
    } catch {
      /* 恢复失败落默认位 */
    }
  }
  try {
    const mon = await currentMonitor();
    const sf = mon?.scaleFactor ?? 1;
    const logicalW = (mon?.size.width ?? 1920) / sf;
    await invoke("move_float_window", {
      x: Math.max(0, logicalW - BAR_W - 24),
      y: 24,
    });
  } catch {
    /* 保持创建默认位置 */
  }
}

/** 整条拖拽：按下即交给 OS（跟手），结束后读窗口位置持久化 */
async function onPointerDown(e: PointerEvent): Promise<void> {
  if (e.button !== 0) return;
  e.preventDefault();
  const win = getCurrentWindow();
  dragging = true;
  try {
    await win.startDragging();
  } catch (err) {
    log.warn("[float] 原生拖拽不可用", err);
    dragging = false;
    return;
  }
  try {
    const phys = await win.outerPosition();
    const sf = await win.scaleFactor();
    const pos = { x: Math.round(phys.x / sf), y: Math.round(phys.y / sf) };
    localStorage.setItem(POS_KEY, JSON.stringify(pos));
  } catch {
    /* 拖拽成功但记录失败：下次拖动会再写 */
  } finally {
    dragging = false;
    // 拖拽期间跳过了 resize（避免与 OS 拖拽循环抢窗口状态），结束后补测一次
    await fitWidth();
  }
}

onMounted(async () => {
  // 局部透明（学 PetMenuWindow.vue：只改本 webview 的 html/body/#app，不用全局 CSS）
  const els = [document.documentElement, document.body, document.getElementById("app")].filter(
    (el): el is HTMLElement => el != null,
  );
  for (const el of els) {
    el.style.background = "transparent";
    el.style.margin = "0";
    el.style.padding = "0";
    el.style.overflow = "hidden";
  }
  await applyPosition();
  // v0.7.7：先按首帧内容收缩宽度，再显示——避免用户看到初始 340px 的宽态
  await fitWidth();
  // 内容宽度一变就立刻跟随（字体就绪 / DPI 变化 / 位数变多），不等下一个 1Hz tick
  if (typeof ResizeObserver !== "undefined" && rootRef.value) {
    resizeObs = new ResizeObserver(() => {
      void fitWidth();
    });
    resizeObs.observe(rootRef.value);
  }
  // 就绪后再显示（Rust 启动/开关路径也会 show，幂等；透明空窗 show 本就不可见）
  try {
    await invoke("show_float_window");
  } catch {
    /* 已显示或窗口未创建 */
  }
  const tick = async () => {
    try {
      cfg.value = await tracker.getStatusBarConfig();
      metrics.value = await tracker.getSystemMetrics();
    } catch {
      /* 应用退出中偶发失败，忽略 */
    }
    await fitWidth();
  };
  void tick();
  timer = window.setInterval(tick, 1000);
});
onBeforeUnmount(() => {
  if (timer !== null) clearInterval(timer);
  resizeObs?.disconnect();
  resizeObs = null;
});
</script>

<style scoped>
.float-bar {
  /* max-content：元素宽度贴合内容，配合测量回调把窗口也收到同样宽 */
  width: max-content;
  height: 36px;
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 12px;
  border-radius: 18px;
  background: rgba(22, 24, 30, 0.78);
  -webkit-backdrop-filter: blur(18px) saturate(140%);
  backdrop-filter: blur(18px) saturate(140%);
  border: 1px solid rgba(255, 255, 255, 0.1);
  /* 内阴影充当「顶部高光」，透明窗口无系统投影，用内描边提质感 */
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.07);
  color: #f5f5f7;
  font-size: 12px;
  font-weight: 600;
  /* 数字等宽：CPU/MEM 百分比变化时宽度不跳 */
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.2px;
  cursor: grab;
  user-select: none;
  white-space: nowrap;
  overflow: hidden;
  transition:
    background 0.18s ease,
    border-color 0.18s ease;
}
.float-bar:hover {
  background: rgba(28, 30, 38, 0.86);
  border-color: rgba(255, 255, 255, 0.16);
}
.float-bar:active {
  cursor: grabbing;
}

/* ===== v1.2.2：液态玻璃模式 =====
 *
 * 几何：44px 高 + 22px 圆角 = 完全胶囊形（与 float_glass.rs 的
 * FLOAT_HEIGHT_GLASS / FLOAT_RADIUS_GLASS 严格同值，改一处必须同步另一处）。
 *
 * 背景：几乎全透明。模糊与折射由**操作系统合成器**在窗口背后绘制
 * （macOS NSGlassEffectView / Windows DWM Acrylic）；CSS `backdrop-filter`
 * 采样不到窗口外的桌面（只采样同文档 DOM 背景，而本窗口的 html/body/#app
 * 全是 transparent），留着既是空转又会盖住系统材质，故明确关闭。
 * 只保留极淡的白色底和细描边，模拟玻璃自身的乳白与高光边缘。
 */
.float-bar.is-glass {
  height: 44px;
  border-radius: 22px;
  /* ==== v1.2.3：压暗 + 顶部白高光（原为纯色 rgba(255,255,255,0.10)）====
   *
   * 为什么必须改：原底色是「加白」，在中浅色壁纸上把胶囊推到接近白，
   * 白字与底亮度差被压到近乎为零 —— 实测胶囊内 #7f9bea 上白字只有 2.69:1、
   * 网速橙 1.54:1（WCAG 正文线 4.5:1）。深色壁纸下反而正常，属「只在浅底暴露」的坑。
   *
   * 为什么是「压暗」而不是「加白」：压暗对**任意亮度**的壁纸都是单向前进
   * （亮壁纸→暗、暗壁纸→更暗），加白则会让亮壁纸撞白、暗壁纸变灰，方向不固定。
   *
   * 为什么是 0.48（不是更浅的 0.30）：这是「四项指标全过 WCAG AA 4.5:1」
   * 的最小压暗量，经真实渲染（Edge headless + 逐像素采样）定标。
   * 早期版本取 0.30，只让纯白主数字勉强过线（4.51:1），带色网速（L=0.72）
   * 物理上无法在 L=0.183 的底上达到 4.5:1 —— 要么把网速调成近白（丢颜色语义），
   * 要么加深压暗。选后者。
   * ⚠️ 定标过程有个坑：先按 CSS alpha 合成公式**纯数学推算**得到 0.44（理论 6.16:1），
   * 但真实渲染只有 5.49:1 —— 公式忽略了顶部白渐变在文字带的实际值（中线 0.09 而非
   * 平均值）与 inset 高光的叠加，**系统性高估了对比度约 0.6**。改为逐档真实渲染扫描后，
   * 实测 0.44 时网速↓ 只有 4.48:1（差 0.02 不达标），0.48 才是真下限。
   * 教训：对比度这种"差一点就是不达标"的指标，必须用真实渲染定标，不能只信公式。
   * 0.48 实测：白 6.59 / 网速↓ 4.85 / ↑ 4.91 / 标签 5.08。
   * 深色壁纸更高；代价是透光率由 70% 降到 52%，视觉上仍能透出壁纸色（已渲染比对确认）。
   *
   * 为什么还留一层白渐变：压暗会把胶囊压成一块深色板，丢掉玻璃感。
   * 顶部 0.16→底部 0.02 的白渐变模拟玻璃的「上边缘反光」，视觉上仍是玻璃而非磨砂。
   *
   * ⚠️ 已知边界：浅灰 / 纯白壁纸仍不达标（主数字约 1.9:1）。
   * 要覆盖纯白壁纸需压暗到 0.60 以上，那样已接近深色磨砂、不再通透。
   * 若需全壁纸达标，唯一正解是按壁纸亮度自适应压暗（需 Rust 侧抓屏，不在本版范围）。
   * macOS 有真系统玻璃时这层压暗会略厚，但仍在「清透」范围内，可接受。
   */
  background:
    linear-gradient(rgba(255, 255, 255, 0.16), rgba(255, 255, 255, 0.02)),
    rgba(0, 0, 0, 0.48);
  /* 显式关掉：blur 交给系统合成器（macOS）；Windows 无系统材质，本就不该有 blur */
  -webkit-backdrop-filter: none;
  backdrop-filter: none;
  /* 描边提亮到 0.34：底色压暗后亮边要更明显，否则胶囊轮廓在暗壁纸上读不出来 */
  border: 1px solid rgba(255, 255, 255, 0.34);
  /* 顶部内高光 = 玻璃的"上边缘反光"。
     ⚠️ 这里**只用 inset，绝不加外阴影**（v1.2.2 修复）：float 窗口尺寸是
     `width: max-content` + 高度恰好 44px，即窗口边界**紧贴**胶囊边缘。
     任何向外扩散的 `box-shadow` 都会被窗口矩形裁掉，只剩四个角露出小块暗色，
     在浅色桌面上看起来就是"胶囊外面套了一个带阴影的矩形边框"（用户实机截图证实）。
     既然窗口无法为投影留出扩散空间，外阴影在本窗口上原理上不可用。 */
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.4),
    inset 0 -1px 0 rgba(255, 255, 255, 0.1);
  /* 文字阴影收紧（v1.2.3）：原 `0 1px 3px .55` 的 3px 模糊半径偏散，
     把 11px 的墨迹糊成约 17px 的虚影，看着"糊"而不是"清楚"。
     改成「2px 实心偏移 + 1px 紧贴描边」，边缘更实、笔画更锐。 */
  text-shadow:
    0 1px 2px rgba(0, 0, 0, 0.62),
    0 0 1px rgba(0, 0, 0, 0.5);
  /* 内边距略增：胶囊形两端是圆弧，文字贴边会视觉出血 */
  padding: 0 14px;
  /* v1.2.3：玻璃模式字号 12→13px。
     44px 胶囊配 12px 字时「文字/胶囊」占比只有 27.3%（普通模式 36px 是 33.3%），
     视觉上框胖字小、中间显空。13px 把占比拉回 29.5%，接近普通模式手感。
     ⚠️ 实测宽度 400.7→**384.2px**（不是变宽而是变窄）：槽位 2.7em→2.45em 的
     收窄量（约 -12px/指标）超过了字号 +1px 的增量，净减 16.5px。 */
  font-size: 13px;
}
/* 悬停：只提亮一点底色，不改变材质本身。
   ⚠️ 压暗量必须同步跟着默认态走：0.48 默认态下若 hover 仍用旧的 0.24，
   悬停时底会突然变亮、对比度掉回 4.0:1 以下（比不悬停还差）。
   故 hover 取 0.42（比默认 0.48 浅 0.06），只做轻微"被照亮"的反馈。 */
.float-bar.is-glass:hover {
  background:
    linear-gradient(rgba(255, 255, 255, 0.22), rgba(255, 255, 255, 0.05)),
    rgba(0, 0, 0, 0.42);
  border-color: rgba(255, 255, 255, 0.42);
}
/* 玻璃模式下按钮底色也要相应减淡，否则深灰方块盖在玻璃上很突兀 */
.float-bar.is-glass .f-shot:hover {
  background: rgba(255, 255, 255, 0.18);
}
/* 分隔线在浅色桌面下会看不见，略提亮一档 */
.float-bar.is-glass .f-sep {
  background: rgba(255, 255, 255, 0.22);
}

.f-dot {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  /* 颜色由 `dotStyle` 内联注入（按负载档位动态切换）；这里不再写死 accent 橙 */
  /* 平滑过渡：1Hz 采样下若直接用离散色跳变，高负载附近会闪烁；0.35s 缓动让变色
     更柔和，同时保留「一眼看颜色」的直观性 */
  transition:
    background 0.35s ease,
    box-shadow 0.35s ease;
}
.f-item {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
  line-height: 1;
}
/* 标签弱化、数值加粗：一眼扫的是数字 */
.f-item > i {
  font-style: normal;
  font-size: 9.5px;
  font-weight: 600;
  letter-spacing: 0.5px;
  color: rgba(255, 255, 255, 0.5);
}
.f-item > b {
  font-weight: 600;
  color: #fff;
  /* 定宽 + 右对齐（v0.7.7 见文件头 ⑤c）：7% → 100% 这类位数变化若不定宽，
     整条会每秒改宽度、窗口跟着 resize，视觉上「呼吸」得厉害。
     定宽后浮窗宽度稳定，数字也排成整齐的右对齐列（2.7em 足够放下 "100%"）。 */
  display: inline-block;
  min-width: 2.7em;
  text-align: right;
}
.f-net {
  gap: 7px;
}
/* 网速值域比百分比宽（↓75K / ↑12.7M / ↓999K），单独给更宽的定位宽度 */
.f-net > b {
  min-width: 3.8em;
}
.f-net > b.down {
  color: #6fd3ff;
}
.f-net > b.up {
  color: #ffb46b;
}
/* v1.2.3：玻璃模式下的网速色单独提亮一档。
   原 #6fd3ff / #ffb46b 是按深色底（普通模式 rgba(22,24,30,.78)）选的，
   在浅色壁纸的玻璃底上只有 1.6 / 1.54:1（WCAG 线 4.5:1），属"颜色好看但看不清"。
   提亮到 #9ee8ff / #ffd8a8 后，在压暗 0.48 的玻璃底上达到 4.85 / 4.91:1（已过 AA），
   且与原色同色相，观感一致（不是换了个颜色，而是同一抹色的更亮档）。
   普通模式**不改**：深色底上原色对比度 7.5 / 7.2:1，本来就很好。 */
.float-bar.is-glass .f-net > b.down {
  color: #9ee8ff;
}
.float-bar.is-glass .f-net > b.up {
  color: #ffd8a8;
}
/* v1.2.3：玻璃模式标签提亮 50%→82%。
   标签是次要信息，但 50% 白在浅底上只有 1.7:1，等于没有。
   82% 在压暗 0.48 的底上量到 5.08:1（过 AA），既保住「弱于数字」的层级
   （数字是纯白 6.59:1），又能读出来。字号同时由 9.5→10px 补偿放大后的胶囊。 */
.float-bar.is-glass .f-item > i {
  color: rgba(255, 255, 255, 0.82);
  font-size: 10px;
}
/* v1.2.3：玻璃模式数字槽位 2.7em→2.45em。
   2.7em 是按 12px 算的余量，13px 下 "100%" 实测 2.42em 就能放下；
   四个指标累计省出约 12px 宽度，也顺带减轻「字右侧一片空」的观感。 */
.float-bar.is-glass .f-item > b {
  min-width: 2.45em;
}
.f-sep {
  flex: none;
  width: 1px;
  height: 14px;
  background: rgba(255, 255, 255, 0.14);
}
.f-dim {
  opacity: 0.72;
  font-weight: 500;
}
/* v0.8.2：尾部截图按钮（幽灵按钮：默认弱化、悬停点亮，贴深色玻璃底） */
.f-shot {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: 7px;
  background: transparent;
  color: rgba(255, 255, 255, 0.72);
  cursor: pointer;
  transition:
    background 0.15s ease,
    color 0.15s ease,
    transform 0.1s ease;
}
.f-shot:hover {
  background: rgba(255, 255, 255, 0.14);
  color: #fff;
}
.f-shot:active {
  transform: scale(0.9);
}
.f-shot svg {
  fill: none;
  stroke: currentColor;
  stroke-width: 1.6;
  stroke-linecap: round;
  stroke-linejoin: round;
}
</style>
