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
  - 2026-10-24 @v1.2.8: 修复 - **macOS 上玻璃材质从右端溢出 + 双弧**，并重做玻璃配色分档。
      ① 根因（用户 macOS 15 真机截图逐像素定标，非推测）：
         旧结构单层 `width: max-content`，`fitWidth()` 按「内容宽 + 2px」设窗口，
         而块级元素左对齐 → 多出的 2px **全落在右侧**。`window-vibrancy` 的
         NSVisualEffectView 用 `view.bounds()` + autoresizing 铺满**整个窗口**，
         胶囊内侧没有任何东西裁它 → 右侧露出 3px 中性带（实测 rgb(138,119,110)，
         外侧壁纸仅 L≈2，带子亮度却恒定 L≈123，证明是材质自身边缘而非 CSS）；
         它自带的 `cornerRadius(22)` 画在宽 403 的矩形上，与 CSS 宽 401 的圆角错开
         → 右侧两角出现「双弧」。左侧因齐平，像素实测**零外溢**，正好反证。
      ② 结构改造：模板拆成两层 —— 外层 `.float-bar` 改 `width: 100%` + 居中
         （= 窗口矩形），内层 `.f-content` 承载 gap/padding 并作为 `fitWidth()` 与
         ResizeObserver 的测量基准。外层铺满后「胶囊 ≡ 窗口 ≡ 材质矩形」，
         三者圆角天然重合，溢出从原理上不可能再发生。
         ⚠️ `WIDTH_SLACK` 保留为 2 但**语义已变**：不再是取整余量，而是
         「外层 1px border × 2」的必需补偿（`box-sizing: border-box` 下内容区
         = 窗口宽 − 2）。改 border 宽度必须同步改它。
         ⚠️ 外层**不能**再持有 ref 给 fitWidth。它现在的宽度是"窗口宽度"这个结果，
         拿它当输入会形成自反馈（永不收敛，宽度永远卡在初值 340）。
      ③ 配色分档（`native_blur` 为真时走 `.is-glass.has-blur`）：原 `.is-glass` 的
         0.48 压暗是**为 Windows 纯 CSS 场景定标的**，却被无条件套到 macOS，把
         NSVisualEffectView 的模糊整层盖死 —— 截图实测胶囊内部是一条平滑暖灰渐变
         （上 L≈95 → 下 L≈71），与正上方壁纸亮度相关系数 **r≈0.02**，桌面内容形状
         完全看不出，等于花了一次系统模糊又亲手抹掉。有真模糊时压暗降到 0.26、
         顶部白高光提到 0.28，让模糊透出来；Windows 档 0.48 不动（已实测达标）。
      ④ `onMounted` 先拉一次 `float_glass_support` 拿 `native_blur`，**在首次 fitWidth
         之前** —— 目的是让首帧就带上正确配色（`nativeBlur` 本身不影响布局，
         见 `onMounted` 内的修正说明），避免"先画深色板再淡下去"的可见闪烁。
         取不到按 false（重压暗保守档），绝不出现"字看不清"。
-->
<template>
  <!-- v1.2.8：外层 = 窗口矩形（width:100%），内层 .f-content 承载内容与测量。
       为什么必须两层（macOS 真机实测的血泪点）：
       旧结构是单层 `width: max-content`，窗口由 fitWidth() 按「内容宽 + 2px 余量」设定，
       多出的 2px 全部落在**右侧**（块级元素左对齐）。而 window-vibrancy 的
       NSVisualEffectView 是按**窗口 bounds** 铺满的（autoresizing 跟随），胶囊内侧
       没有任何东西裁它 → 那 2px 直接把系统材质露在桌面上，并且它自带的
       cornerRadius(22) 画在「宽 403」的矩形上，与 CSS「宽 401」的圆角错开 → 右侧
       出现「双弧 + 3px 外露带」。左侧因齐平故零外溢（像素实测印证）。
       改成外层铺满窗口后，胶囊几何 ≡ 窗口矩形 ≡ 材质矩形，三者圆角天然重合，
       溢出从原理上不可能发生；测量移到内层，宽度自适应逻辑不受影响。 -->
  <div
    :class="['float-bar', { 'is-glass': cfg.float_glass, 'has-blur': cfg.float_glass && nativeBlur }]"
    @pointerdown="onPointerDown"
    @contextmenu.prevent
  >
    <div ref="contentRef" class="f-content">
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
/**
 * 宽度安全余量（逻辑像素）。
 *
 * v1.2.8 重新定义：**这里不再是"取整余量"，而是外层描边的厚度**。
 *
 * 旧结构是单层 `width: max-content`，测出的 rect.width 已含 padding 与 border，
 * 再 +2 属于"多给的余量"，于是窗口比胶囊宽 2px、全落在右侧 → 系统材质外露
 * （详见模板顶部注释）。新结构分两层后这个 2 有了精确的物理含义：
 *   - 外层 `.float-bar` 是 `width: 100%` + `border: 1px` + 全局 `box-sizing: border-box`
 *     → 其**内容区** = 窗口宽 − 2；
 *   - 内层 `.f-content` 是 `width: max-content`（不含 border），要让它完整放下，
 *     窗口必须 = 内层宽 + 2。
 * 因此 2 不是余量而是**必需值**：取 0 会让内层比内容区宽 2px，
 * 被外层 `overflow: hidden` 左右各裁 1px（虽然只裁到 padding 空白、肉眼无感，
 * 但那是"碰巧没事"，不是正确几何）。
 *
 * ⚠️ 若将来改外层 border 宽度，本常量必须同步为 2 × border-width。
 */
const WIDTH_SLACK = 2;
const POS_KEY = "float_pos";

/** v1.2.8：内层内容容器（宽度测量基准）。
 *  外层 `.float-bar` 已改为铺满窗口、不再持有 ref —— 它自身的宽度是"窗口宽度"
 *  这个**结果**，拿它当 fitWidth 的输入会形成自反馈（永不收敛，宽度卡在初值）。 */
const contentRef = ref<HTMLElement | null>(null);
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

/**
 * v1.2.8：系统是否**真的**提供了背景模糊（不是"能力上支持"，是"这次真的应用上了"）。
 *
 * 为什么需要这个信号：玻璃模式的配色必须按背景来定，而两种背景的最优解完全相反。
 * - 有真模糊（macOS 15 的 NSVisualEffectView / macOS 26 的 NSGlassEffectView）：
 *   背景已被系统模糊 + 平均，像素噪声低，**不需要靠压暗来保对比度**；压暗过重反而
 *   把系统模糊完全盖住 —— 用户实测截图里那块暖灰"板子"就是这么来的（内部亮度与
 *   正上方壁纸相关系数 r≈0.02，桌面内容形状完全看不出）。
 * - 无系统材质（Windows 全平台，见 float_glass.rs 模块头实测：ρ=0.001 ）：
 *   背后是**未模糊**的原始桌面，像素噪声大，必须靠 0.48 压暗才能让白字过 WCAG AA。
 *
 * 来源：`float_glass_support` 的 `native_blur` 字段（Rust 侧如实上报，
 * Windows 虽报 tier=acrylic 但 native_blur=false）。取不到时按 false 处理 ——
 * 退回"重压暗"这个已在两种背景下都不至于不可读的保守档，绝不会出现"字看不清"。
 */
const nativeBlur = ref(false);

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
 * v1.2.8 起测量目标是**内层 `.f-content`**，不再是外层 `.float-bar`：
 * 外层已改为 `width: 100%`（跟随窗口），它自身的宽度是"窗口宽度"这个结果，
 * 拿它当输入会让 fitWidth 变成自反馈（永不收敛、宽度卡在初值 340）。
 *
 * 测量仍取 getBoundingClientRect().width 与 scrollWidth 的**较大者**：
 * 前者是 border-box 宽度（内层用 max-content，即内容固有宽度），
 * 后者在「被父级挤压」时给出内容实际需要宽度——两者取大最稳，不会漏算。
 */
async function fitWidth(): Promise<void> {
  if (dragging) return;
  await nextTick();
  if (dragging) return;
  const el = contentRef.value;
  if (!el) return;
  const need = Math.max(el.getBoundingClientRect().width, el.scrollWidth);
  // 窗口宽 = 内容宽 + 外层左右描边（WIDTH_SLACK，见常量注释）。
  // ceil 而非 round：宁可多给内层零点几像素余量，也不要把描边或末位字符裁掉。
  const w = Math.ceil(need) + WIDTH_SLACK;
  if (Math.abs(w - appliedW) < 1) return;
  // ⚠️ appliedW 只在 invoke **成功之后**才更新（2026-10-25 复查修正）。
  // 原实现先赋值再 invoke：若这次 resize 因窗口未创建/退出中失败，
  // appliedW 已被改成 w，后续每秒的 fitWidth 都会因 |w-appliedW|<1 直接早退，
  // 宽度就永久卡在"以为已经设过"的错误值上，直到内容再变化才恢复。
  // 注：这个错误会被 1Hz tick 的自愈性掩盖大半（内容一变就会重试），
  // 属于坏味道而非致命缺陷，但代价极低，顺手改对。
  try {
    await invoke("resize_float_window", { width: w });
    appliedW = w;
  } catch {
    /* 窗口尚未创建 / 退出中，忽略；不更新 appliedW，下次 tick 会重试 */
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
  // v1.2.8：先拉一次玻璃能力，拿到 native_blur 再决定配色档位。
  //
  // ⚠️ 修正一处早期注释的错误（2026-10-25 复查）：原注释说「配色会影响字号
  // /槽位宽度，必须先定档再测宽」—— 这是**错的**。实测 `.has-blur` 档只改
  // `background` 与 `border-color`，两者都不参与布局；字号 13px 与内距 14px
  // 来自 `.is-glass`，跟 `has-blur` 无关。所以顺序其实不影响首帧宽度。
  //
  // 但**仍保持在 fitWidth 之前**，理由换成更实在的一条：`nativeBlur` 只影响
  // 配色不影响布局 → 早拿晚拿宽度都一样；放在前面是为了让**首帧渲染**就已经
  // 带上正确配色，避免"先画深色板、再淡下去"的一次可见闪烁（applyPosition
  // 之后、fitWidth 之前正好是窗口还不可见的窗口期）。
  //
  // 失败按 false（重压暗保守档）处理，不阻断渲染。
  try {
    const cap = await tracker.getFloatGlassSupport();
    nativeBlur.value = cap.native_blur === true;
  } catch {
    /* 命令未注册（旧后端）/ 退出中：保持 false */
  }
  // v0.7.7：先按首帧内容收缩宽度，再显示——避免用户看到初始 340px 的宽态
  await fitWidth();
  // 内容宽度一变就立刻跟随（字体就绪 / DPI 变化 / 位数变多），不等下一个 1Hz tick
  if (typeof ResizeObserver !== "undefined" && contentRef.value) {
    resizeObs = new ResizeObserver(() => {
      void fitWidth();
    });
    resizeObs.observe(contentRef.value);
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
/* ===== v1.2.8 两层结构 =====
 *
 * 外层 .float-bar ：视觉载体（圆角/底色/描边/阴影）+ 撑满窗口
 * 内层 .f-content：内容排布（gap/padding）+ 宽度测量基准
 *
 * ⚠️ 两者分工不可合并回去。旧版把「贴合内容 + 视觉载体」放在同一层，导致窗口
 * （= 内容宽 + 2px 余量）比胶囊宽 2px，系统玻璃材质按窗口矩形铺满 → 右侧外露
 * 3px 并出现双弧（macOS 15 真机截图逐像素证实，见模板顶部注释）。
 * 现在外层 width:100% 恒等于窗口，胶囊 ≡ 窗口 ≡ 材质矩形，物理上不可能溢出。
 */
.float-bar {
  /* 铺满窗口：宽度由 Rust set_size 决定，CSS 不再参与"窗口该多宽"的决策 */
  width: 100%;
  /* v1.2.10（2026-10-26）：高度也交给 Rust 单一来源（原 `height: 36px`）。
   *
   * 为什么必须改：窗口高度由 Rust 按 `status_bar_config.float_glass` 独立
   * `set_size`（float_window.rs:277 走 current_height），CSS 另有一套硬编码
   * 36/44 —— 两条时间线不同源，三条常见路径都会破：
   *   ① glass 用户冷启动：窗口已 44，但 cfg 初值 float_glass:false（见 script 里
   *      的 ref 初值）→ 首帧 CSS 36 → **窗口底部露出 8px 系统材质**；
   *   ② 开关翻转：cfg 靠 1Hz 轮询 → 最多 1 秒内「窗口 44 / 胶囊 36」错位，
   *      且材质 radius 22 vs CSS radius 18 → **纵向双弧**；
   *   ③ 隐藏期间定时器被系统节流，②的错位时长被进一步拉长。
   * 改成 100% 后「胶囊 ≡ 窗口」，与 v1.2.8 对宽度做的改造完全同构。
   *
   * ⚠️ 反证（为什么敢用 100% 而不是 100vh）：`height:100%` 依赖父链有确定高度。
   * 本窗口的父链是 index.html 的 `<div id="app">` → body → html，
   * style.css:72-76 明写 `html, body, #app { height: 100% }`；main.ts 只做
   * `createApp(App).mount("#app")`，没有给 #app 加任何额外 class/内联高度；
   * App.vue 的 float 分支直接渲染 `<FloatBar/>`（不在 `.app` 这个
   * min-height:100vh 的包裹层里），因此 .float-bar 的**直接父级就是 #app**。
   * 若这条链哪天断了（比如 #app 外又套一层没有高度的 div），100% 会解析成 auto，
   * 胶囊高度塌成内容高度（约 13px 字高 + 无内距）→ 视觉上变成一条扁带。
   * 届时解法是退回 `height: 100vh`（视口 = 窗口内高，不依赖父链），
   * 而不是重新引入 36/44 的硬编码。 */
  height: 100%;
  display: flex;
  align-items: center;
  /* 内容居中：窗口比内容宽出取整余量时，余量均匀分到两侧，不会全堆在右边 */
  justify-content: center;
  /* v1.2.10：圆角也统一成「恒为半高」的写法（原 18px，玻璃档另写 22px）。
   * 999px 在任何高度下都会被浏览器按 CSS 规范压到 min(999px, 高/2)，
   * 即永远等于半高 —— 胶囊形状与窗口高度自动同源，不再需要按档维护两个数。
   * 这也顺手消掉「材质 radius 22 vs CSS radius 18」的纵向双弧。 */
  border-radius: 999px;
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
/* 内层：max-content 保证"有多宽就多宽"，flex:none 保证窗口万一窄 1px 时
   内容不被 flex 压缩（宁可裁掉描边，也不能把数字挤变形） */
.f-content {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 12px;
  width: max-content;
  flex: none;
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
 * 几何（v1.2.10 改口径）：44px 高 + 22px 圆角仍是「完全胶囊形」的目标，
 * 但**数值不再写在这里** —— 高度由 `float_glass.rs::FLOAT_HEIGHT_GLASS` 经
 * `set_size` 下发，CSS 用 `height:100%` 跟随；圆角用 `border-radius:999px`
 * 恒取半高。原来「两处必须同步」的约定已失效（同步这件事本身被结构性消除）。
 *
 * 背景：几乎全透明。模糊与折射由**操作系统合成器**在窗口背后绘制
 * （macOS NSGlassEffectView / Windows DWM Acrylic）；CSS `backdrop-filter`
 * 采样不到窗口外的桌面（只采样同文档 DOM 背景，而本窗口的 html/body/#app
 * 全是 transparent），留着既是空转又会盖住系统材质，故明确关闭。
 * 只保留极淡的白色底和细描边，模拟玻璃自身的乳白与高光边缘。
 */
.float-bar.is-glass {
  /* v1.2.10（2026-10-26）：**删掉原来的 `height: 44px` 与 `border-radius: 22px`**。
   *
   * 为什么删：任务是把「胶囊几何」收成单一来源。原来这里是第二份硬编码，
   * 会覆盖 `.float-bar` 上的 `height:100%` 与 `border-radius:999px` ——
   * 于是「窗口由 Rust 决定 44/36、胶囊由 CSS 决定 44/36」的双源问题原样保留：
   *   ① 玻璃档 CSS 恒 44，但 Rust 在开关关闭时把窗口压到 36 → 胶囊被窗口裁掉 8px；
   *   ② 材质 radius 由 float_glass.rs 按 FLOAT_RADIUS_GLASS=22 铺，
   *      CSS 若是 18（原 .float-bar 的值）→ 纵向出现两个不同曲率的弧。
   * 删掉后高度与圆角都只有一处定义（.float-bar 的 100% / 999px），
   * 与 Rust 的 set_size 天然同源。
   *
   * ⚠️ 这一档只剩「配色与描边」职责。若将来有人想给玻璃档单独调高度，
   * 请改 Rust 的 height_for()，不要在这里加回 height —— 那会重新制造双源。 */
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
  /* v1.2.3：玻璃模式字号 12→13px。
     44px 胶囊配 12px 字时「文字/胶囊」占比只有 27.3%（普通模式 36px 是 33.3%），
     视觉上框胖字小、中间显空。13px 把占比拉回 29.5%，接近普通模式手感。
     ⚠️ 实测宽度 400.7→**384.2px**（不是变宽而是变窄）：槽位 2.7em→2.45em 的
     收窄量（约 -12px/指标）超过了字号 +1px 的增量，净减 16.5px。 */
  font-size: 13px;
  /* v1.2.10（2026-10-26）：外观档切换的缓动。
   *
   * 背景：v1.2.10 起本组件按 `prefers-color-scheme` 分「暗底亮字 / 亮底暗字」两档
   * （见文件末尾的 light 分档）。用户改系统外观时两档会整块互换，若不加过渡，
   * 胶囊会「啪」地反色，比系统自身的外观切换动画（约 0.25s）还突兀。
   *
   * ⚠️ 为什么只写 background-color 而不是 background：CSS 无法在两个
   * linear-gradient 之间插值（离散跳变），写 `background` 也不会让那层白高光
   * 平滑过去，只是让人误以为生效。真正能插值的是底色（background-color 层）、
   * 描边色和文字色，恰好就是「反色」的主体，观感已经够连续。
   * 高光层的亮度差 < 0.1（0.16→0.10），瞬切肉眼无感。
   *
   * 副作用：hover 的提亮反馈也随之从 0.18s 变成 0.3s —— 与 macOS 原生悬停
   * 淡入的节奏一致，不算退步。 */
  transition:
    background-color 0.3s ease,
    border-color 0.3s ease,
    color 0.3s ease;
}
/* v1.2.8：内边距从外层移到内层（外层已不是内容载体）。
   略增到 14px：胶囊形两端是圆弧，文字贴边会视觉出血。 */
.float-bar.is-glass .f-content {
  padding: 0 14px;
}

/* ===== v1.2.8：macOS 有真系统模糊时的配色档 =====
 *
 * 上一条 `.is-glass` 的 0.48 压暗是**为 Windows 纯 CSS 场景定标的**（背后是未模糊的
 * 原始桌面，像素噪声大，必须重压暗才能让白字过 AA）。但它被无条件套用到了 macOS，
 * 结果把 NSVisualEffectView 的模糊整层盖死 —— 用户 macOS 15 截图实测：胶囊内部
 * 是一条平滑暖灰渐变（上 L≈95 → 下 L≈71），与正上方壁纸亮度的相关系数 r≈0.02，
 * 桌面内容的形状完全看不出来，等于花了一次系统模糊又亲手抹掉。
 *
 * 分档依据：`float_glass_support` 的 `native_blur`（Rust 如实上报，见 float_glass.rs）。
 * 有真模糊时系统已经把背景做了高斯平均，对比度天然比原始桌面稳定得多，
 * 因此可以把压暗降到 0.26，让模糊透出来；顶部白高光同步提到 0.28，
 * 因为浅色底需要更明显的"上边缘反光"才读得出玻璃边界。
 *
 * ⚠️ 0.26 未经真机对比度定标（本机无 macOS）。取值依据：
 *   - 系统 HudWindow/UnderWindowBackground 材质本身带约 35~45% 的底色遮罩，
 *     叠加本层 0.26 后总遮蔽量接近 Windows 档的 0.48~0.55；
 *   - 若 macOS 真机实测某项对比度不足 4.5:1，按 0.02 步进加深本值即可，
 *     **不要**回头去改 Windows 档的 0.48（那档已实测达标，动了会回归）。
 */
.float-bar.is-glass.has-blur {
  background:
    linear-gradient(rgba(255, 255, 255, 0.28), rgba(255, 255, 255, 0.06)),
    rgba(0, 0, 0, 0.26);
  border-color: rgba(255, 255, 255, 0.40);
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
/* v1.2.8：有真模糊档的 hover 也必须跟着本档走（0.42 是 Windows 档的 hover 值，
   套到 0.26 的 macOS 档上会变成"越悬停越暗"，方向反了） */
.float-bar.is-glass.has-blur:hover {
  background:
    linear-gradient(rgba(255, 255, 255, 0.34), rgba(255, 255, 255, 0.09)),
    rgba(0, 0, 0, 0.20);
  border-color: rgba(255, 255, 255, 0.48);
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

/* ============================================================================
 * v1.2.10（2026-10-26）：系统浅色外观下的「亮底深字」档（路线 B）
 * ============================================================================
 *
 * 为什么必须做（用户 macOS 15 真机截图逐像素采样，非估计）：
 *
 *   场景              胶囊内底色        白字对比度   判定
 *   浅色（白桌面）    rgb(210,211,211)   **1.36:1**  严重不可读
 *   深色（蓝壁纸）    rgb(17,48,83)      12.1:1      优秀
 *
 * 同一份 CSS 差 9 倍。**这不是 alpha 调错了，是「白字 + 暗底」这个假设只对暗背景成立**：
 * 压暗从 0.26 调到 0.16，浅色下底色仍约 rgb(185,185,185)，白字只有 1.6:1；
 * 调到 0 则白字落在近白底上，1.0:1 更糟。v1.2.3（0.30→0.48）与 v1.2.8（0.48→0.26）
 * 两次调参本质都是在无解的路上找平衡点 —— 所以这里换成**按系统外观分档反色**。
 *
 * 为什么不选路线 A（按壁纸亮度自动反色）：需要 Rust 侧用 CGWindowListCreateImage
 * 采样浮窗背后的桌面亮度，属新增系统能力，且本机（Windows）无法验证真实效果。
 * 路线 B 的缺口是「系统深色 + 浅壁纸」（如深色模式看白底 PDF）仍偏灰，
 * 属可接受的残留，后续可按路线 A 补。
 *
 * 分档依据：`prefers-color-scheme`（`src/style.css:34` 已有同机制先例）。
 * 它跟随的是**系统外观**而非壁纸亮度 —— 绝大多数用户「浅色系统配浅壁纸」，
 * 所以这一档能覆盖主要矛盾。
 *
 * ⚠️ **反色只给 macOS（`.has-blur`），Windows 不参与** —— 这是本档最重要的约束。
 *    完整理由见下方 `@media` 块内首条规则的注释（作用域矩阵 + 深壁纸反而变差
 *    + 与关闭玻璃时的深色胶囊外观割裂）。一句话：Windows 没有系统材质跟随
 *    appearance，反色是净亏。
 *
 * ⚠️ 下面所有规则都放在 <style> 最末尾，靠「同特异性 + 后出现」压过暗色档；
 *    刻意不改动上面任何一条暗色规则的取值（尤其 Windows 的 0.48 档已实测达标，
 *    动了会回归）。
 *
 * ⚠️ 暗色档的旧注释一律保留 —— 它们是压暗量 0.48 / 0.26 的定标记录，
 *    是「为什么不能简单调 alpha」的证据链，删了就没人知道这两次调参的教训。
 *
 * ---------------------------------------------------------------------------
 * 配色定标（WCAG 2.1 相对亮度公式实算，非估计）
 *
 *   评估底 = 白遮罩 α 叠在背景上：rgb = α*255 + (1-α)*背景
 *   背景取**材质后底**，即 NSVisualEffectView 已经把壁纸模糊平均之后的结果。
 *
 *   材质底下界怎么来的（不是拍的，是从 v1.2.8 真机实测反推）：
 *     v1.2.8 注释记录「深色模式下胶囊内部是平滑暖灰渐变，上 L≈95 → 下 L≈71」，
 *     那是 **0.48 压暗之后**的观测值 → 材质后底 = 95/0.52 ≈ 183、71/0.52 ≈ 137。
 *     浅色模式下材质跟随 appearance 只会更亮，但本机（Windows）无法实测，
 *     故**按 137 这个保守下界定档**，并扫全程 130~255 取最差值。
 *
 * ① 默认态 rgba(255,255,255,0.55)，渐变 0.24→0.06（取 band=1.0 即最暗处）：
 *      材质底 137 → 胶囊 srgb 205：主 10.70 / 标签 5.23 / ↓ 4.89 / ↑ 4.97 / 截图 5.57
 *      材质底 183 → 胶囊 srgb 224：主 12.89 / 标签 5.65 / ↓ 5.89 / ↑ 5.99 / 截图 6.01
 *      材质底 255 → 胶囊 srgb 255：主 17.01 / 标签 6.19 / ↓ 7.77 / ↑ 7.90 / 截图 6.69
 * ② hover 态 rgba(255,255,255,0.60)，渐变 0.28→0.08：
 *      材质底 137 → 胶囊 srgb 211：主 11.37 / 标签 5.39 / ↓ 5.19 / ↑ 5.28 / 截图 5.74
 *      材质底 183 → 胶囊 srgb 229：主 13.51 / 标签 5.74 / ↓ 6.17 / ↑ 6.28 / 截图 6.20
 *
 * 全程最差值（材质底 130~255 × band 0/0.5/1 × 默认+hover）：
 *      主 10.38 / 标签 5.16 / ↓ 4.74 / ↑ 4.82 / 截图 5.49  —— 全部 ≥ AA 4.5:1
 *
 * 网速色取「同色相的深档」而非直接沿用暗色档的 #9ee8ff / #ffd8a8：
 *   #9ee8ff（H=194°）→ #055a70（H=196°）；#ffd8a8（H=33°）→ #7a4405（H=30°）
 * 色相基本不变，只把明度压下来 —— 亮底上的青/橙必须靠深色墨水才读得出来。
 * ⚠️ 取值必须按「材质底 137 下仍达标」来定：更浅的候选 #0a658c / #8a4d08
 *    在材质底 <162 / <153 时会掉到 4.06 / 4.20:1（不达 AA）。
 *    题目建议的 #0b7fa8 / #a8620c 更浅，只会更差。
 *
 * 标签用 rgba(0,0,0,0.62) 而非纯黑：保留「弱于数字」的层级（数字 10~17:1），
 * 0.62 在最暗材质底上实测 5.16:1，稳过 AA。0.60 是 4.92:1（过线但余量薄）。

 */
@media (prefers-color-scheme: light) {

  /* ---- 背景与描边：白遮罩 0.55，让系统模糊透出来 ----
   *
   * ⚠️ **全部挂在 `.has-blur` 上，不是 `.is-glass`**。这条作用域是硬约束，
   * 改错会把 Windows 拖进来。矩阵（依据 `float_glass.rs::native_blur`，
   * macOS 的 Liquid/Vibrancy 全为 true，Windows 的 Acrylic/None 全为 false）：
   *
   *   `.is-glass`          → macOS 玻璃开、**Windows 玻璃开**、Linux 玻璃开
   *   `.is-glass.has-blur` → 只有 macOS 玻璃开
   *
   * Windows 不能参与反色的原因（实测数据，不是推测）：
   *   ① Windows 上**没有任何系统材质跟随 appearance**（float_glass.rs 模块头：
   *      DWM backdrop 对该无边框窗口透光率仅 0.001，已刻意不调用），
   *      CSS 是唯一视觉来源，不存在「必须跟随系统外观」的压力；
   *   ② 反色在深色壁纸上会**大幅变差**：Win11 默认 Bloom 壁纸 rgb(46,32,78)，
   *      现状（0.48 压暗 + 白字）16.78:1；把本档参数（白遮罩 0.55 + 深字）
   *      照搬过去只剩 **6.32:1**，净亏约 10.5。且胶囊会成为一块 srgb(161,155,175)
   *      的灰白板（原本能透出深紫壁纸色）。
   *      （若真按 0.86 的白遮罩算还有 12.97:1，但那是本档**没在用**的 alpha，
   *       不能拿来给自己开脱 —— 引用的数字必须是 CSS 里实际写的那个。）
   *   ③ 更关键的是**外观割裂**：Windows 关闭玻璃时走 `.float-bar` 基样式
   *      `rgba(28,30,38,0.86)`（深色）。若玻璃开时浅色模式反色成白板，
   *      同一个开关前后是两套色系，用户会觉得"开了玻璃就换了个应用"。
   *
   * Windows 的已知缺口（此处不修，如实记录）：浅色壁纸下 0.48 压暗 + 白字
   * 只有 3.40:1（纯白壁纸）/ 3.93:1（浅灰），不达 AA。这是 v1.2.3 已书面
   * 承认的边界，属"按壁纸亮度自适应压暗"的范畴（需 Rust 侧抓屏采样，
   * 是新增系统能力，不在本轮范围）。用「深壁纸变白板 + 外观割裂」去换
   * 「浅壁纸达标」是净亏。
   *
   * ⚠️ 口径声明（本节所有对比度数字）：均为**不含顶部白渐变**的底色层对算。
   * 含渐变（文字带中线 α≈0.09）时上列数字各自再降 0.46~0.59（浅壁纸）/
   * 3.53（Bloom）。方向上不改变任何结论，只是绝对值整体偏移 —— 之所以
   * 选不含渐变的口径，是因为它是**常数偏移**，跨壁纸横向比较时更干净；
   * 而「是否过 AA」这种绝对判定必须用含渐变的真实渲染值（v1.2.3 的 0.48
   * 定标就是那么做的）。两种口径不可混引。
   */
  .float-bar.is-glass.has-blur {
    background:
      linear-gradient(rgba(255, 255, 255, 0.24), rgba(255, 255, 255, 0.06)),
      rgba(255, 255, 255, 0.55);
    /* 描边由「白色高光」翻成「深色发丝线」：暗色档靠白描边从暗壁纸上浮起来，
       亮色档反过来要靠深描边界定轮廓，否则胶囊在白壁纸上边界消失。
       真模糊档的底色是「模糊后的壁纸 + 白 0.55」，比纯 CSS 档亮且更均匀，
       故描边取极淡的深色 —— 太深会在浅灰壁纸上显得像硬边框。 */
    border: 1px solid rgba(0, 0, 0, 0.08);
    color: #1c1c1e;
    /* 内阴影同步反相：顶部白色内高光保留（玻璃的物理反光在亮底上依然存在），
       底部由白 0.1 换成深 0.04，给胶囊下缘一点厚度。 */
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.85),
      inset 0 -1px 0 rgba(0, 0, 0, 0.04);
    /* 文字阴影必须关掉：暗色档的 `0 1px 2px rgba(0,0,0,.62)` 是为「白字压暗底」
       加的抗眩光描边。深字再叠暗投影会变成脏兮兮的双层描边，把 13px 的字糊掉。 */
    text-shadow: none;
  }

  /* ---- hover：继续「提亮」方向，遮罩 0.55→0.60 ----
     ⚠️ 亮色档的「被照亮」= 白遮罩更浓、底更白，与暗色档「压暗更浅」同向（都是变亮），
     但绝对手法相反。若照抄暗色档的 rgba(0,0,0,0.42)，悬停时会突然变成深色板。 */
  .float-bar.is-glass.has-blur:hover {
    background:
      linear-gradient(rgba(255, 255, 255, 0.28), rgba(255, 255, 255, 0.08)),
      rgba(255, 255, 255, 0.60);
    border-color: rgba(0, 0, 0, 0.12);
  }

  /* ---- 文字与图标 ---- */

  /* 主数字：近黑 #1c1c1e（L=0.0117）。材质底 137~255 全区间实测 10.38~17.01:1，远超 AA。
     ⚠️ 不用纯黑：纯黑在浅灰壁纸的玻璃上对比更硬，且与 macOS 原生
     浅色文字的「近黑带一点点蓝」的观感不符（#1c1c1e 正是 NSColor.labelColor 的浅色值）。 */
  .float-bar.is-glass.has-blur .f-item > b {
    color: #1c1c1e;
  }

  /* 标签：rgba(0,0,0,0.62) → 实测 5.16~6.19:1，既弱于数字又过 AA。
     0.60 在最暗材质底上只有 4.92:1（过线但余量薄），故提到 0.62。 */
  .float-bar.is-glass.has-blur .f-item > i {
    color: rgba(0, 0, 0, 0.62);
  }

  /* 网速：同色相深档。
     色相基本不变，只把明度压下来 —— 亮底上的青/橙必须靠深色墨水才读得出来。
       #9ee8ff（H=194°）→ #055a70（H=196°）；#ffd8a8（H=33°）→ #7a4405（H=30°）
     ⚠️ 取值经「材质底 137~255 全区间扫描 + 扣 0.6 安全余量」定标，不是取色器挑的：
       更浅的候选（#0a658c / #8a4d08）在材质底 <162/153 时会掉到 4.06/4.20:1。
       而材质底的下界可由 v1.2.8 的深色模式真机实测反推 —— 注释记「压暗 0.48 后
       观测 L≈95（上）/71（下）」，除以 0.52 得材质后底 ≈183/137。
       浅色模式材质只会更亮，但本机无 macOS 无法实测，故按 137 保守定档。 */
  .float-bar.is-glass.has-blur .f-net > b.down {
    color: #055a70;
  }
  .float-bar.is-glass.has-blur .f-net > b.up {
    color: #7a4405;
  }

  /* 分隔线：白 0.22 在亮底上等于没有，换成深 0.16 */
  .float-bar.is-glass.has-blur .f-sep {
    background: rgba(0, 0, 0, 0.16);
  }

  /* 截图按钮：幽灵按钮的默认/悬停对比色同样要反相，
     否则浅底上是一枚几乎看不见的白描边图标，悬停还会变纯白（彻底消失）。 */
  .float-bar.is-glass.has-blur .f-shot {
    color: rgba(0, 0, 0, 0.64);
  }
  .float-bar.is-glass.has-blur .f-shot:hover {
    background: rgba(0, 0, 0, 0.10);
    color: #1c1c1e;
  }
}
</style>
