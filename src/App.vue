<!--
  App.vue
  应用根组件：主窗口（使用时间统计 UI）+ 桌宠独立 webview 分支（PetWindow）+ 桌宠菜单独立 webview 分支（PetMenuWindow，v0.6.2-beta.17）。

  设计思路：
  - 通过 isPetWindow 区分「主窗口」与「桌宠透明置顶窗口」两个 Tauri webview
  - isPetMenuWindow 区分桌宠菜单独立 webview（独立 Tauri 窗口，菜单可拖到全桌面）
  - 顶部实时指示栏读取 get_current_foreground 返回值（snake_case），用于验证确实在采集其他软件
  - 页面切换：**没有路由库、没有 keep-alive**。主窗口只有一个 <Dashboard />，
    Dashboard 内部用 v-if 在「统计概览 / 趋势 / 分类规则 / 设置」四个 tab 间切换。
    （旧注释曾写「路由用动态组件 + keep-alive 按侧边栏切换」，与实现不符，v1.2.5 更正）

  修改历史：
    - 2026-07-08 @v0.2.0: 初始创建 - 主窗口骨架 + 桌宠窗口分支
    - 2026-07-24 @v0.6.1-beta.1: 修复 - 顶部实时栏 IPC 字段名 bug（camelCase → snake_case，与 Rust 返回值对齐）
    - 2026-07-25 @v0.6.2-beta.17: 新增 - 桌宠右键菜单独立 webview 分支
    - 2026-08-05 @v0.6.2-beta.26: 修复 - 浏览器预览时 getCurrentWebviewWindow() 抛错导致白屏
    - 2026-09-10 @v0.7.6: 新增 - 悬浮指标条独立 webview 分支（FloatBar，label='float'）
    - 2026-10-22 @v1.2.5: 文档 - 更正「路由用动态组件 + keep-alive」的过时说明（实际无路由、无 keep-alive）
-->
<template>
  <!-- v0.6.0-beta 桌宠窗口分支：独立 webview 渲染 PetWindow（透明/置顶/无主 UI） -->
  <PetWindow v-if="isPetWindow" />
  <!-- v0.6.2-beta.17 桌宠菜单独立 webview 分支：在独立 Tauri 窗口内渲染菜单，
       position:fixed 相对自己 webview 视口（≈ 全桌面），菜单可自由拖到任意位置 -->
  <PetMenuWindow v-else-if="isPetMenuWindow" />
  <!-- v0.7.6 悬浮指标条独立 webview 分支：透明置顶指标条（label='float'） -->
  <FloatBar v-else-if="isFloatWindow" />
  <!-- v0.8.0 截图遮罩独立 webview 分支：全屏透明遮罩 + QQ 式工具栏（label='capture'） -->
  <CaptureOverlay v-else-if="isCaptureWindow" />
  <div v-else class="app">
    <!-- 顶部栏：品牌 + 实时记录指示（启动即自动追踪，无需手动开关） -->
    <header class="topbar">
      <div class="brand">
        <span class="logo"></span>
        <h1>ScreenTime Pro</h1>
      </div>

      <!-- 实时指示：显示「正在记录：XXX」，用于验证确实在采集其他软件 -->
      <div class="live">
        <span class="dot" :class="{ on: tracking }"></span>
        <span class="live-label">{{ tracking ? t("app.recording") : t("app.paused") }}</span>
        <span class="live-name">{{ live.name || "—" }}</span>
        <!-- 字段名与 Rust 返回值一致（snake_case）：Tauri v2 仅转换命令参数为 camelCase，
             返回值原样序列化，故此处读 window_title / session_seconds / idle_seconds -->
        <span class="live-title" v-if="live.window_title && live.window_title !== live.name">· {{ live.window_title }}</span>
        <span class="live-session" v-if="tracking && (live.session_seconds ?? 0) > 0">{{ t("app.recorded", { dur: fmtDur(live.session_seconds ?? 0) }) }}</span>
        <span class="live-idle" v-if="tracking">{{ t("app.idle", { n: live.idle_seconds ?? 0 }) }}</span>
      </div>
    </header>

    <!-- WebView2 缺失横幅：Windows 启动时若检测到未安装 WebView2 Runtime，提示用户去下载 -->
    <div v-if="showWebview2Warning && !webview2Dismissed" class="perm-banner perm-banner--warn">
      <span>{{ t("app.webview2Warning") }}</span>
      <div class="perm-actions">
        <button @click="openWebview2Download">{{ t("app.webview2Download") }}</button>
        <button class="perm-dismiss" @click="webview2Dismissed = true" :title="t('app.dismissTip')"><AppIcon name="x" /></button>
      </div>
    </div>

    <!-- 权限引导横幅：macOS 未授予辅助功能权限时提示（空闲检测需要） -->
    <!-- v-show 而非 v-if：关闭后仍保留 DOM，重新检测到未授权时可再次显示 -->
    <div v-if="showPermWarning" class="perm-banner">
      <span>{{ accessibilityNeverGranted ? t("app.permWarning") : t("app.permWarningInvalidated") }}</span>
      <div class="perm-actions">
        <button @click="openSettings">{{ t("app.openSettings") }}</button>
        <button class="perm-dismiss" @click="dismissPermBanner" :title="t('app.dismissTip')"><AppIcon name="x" /></button>
      </div>
    </div>

    <!-- 菜单栏纯后台模式下，窗口可随时关闭到托盘；此处提供「隐藏到菜单栏」按钮 -->
    <main>
      <Dashboard />
    </main>

    <!-- v0.8.0：截图完成提示（剪贴板优先，故文案强调「直接粘贴」；落盘则补一句已保存） -->
    <div v-if="shotToast" class="shot-toast">
      <span class="shot-toast-icon">✓</span>
      <span class="shot-toast-msg">{{ shotToast }}</span>
    </div>

    <!-- v0.9.2：截图失败全局提示（任何页面可见）。此前失败只在设置页弹错，
         用户用全局快捷键截图、而人不在设置页时 → 失败是静默的（仅进日志）。
         v0.9.4：权限类失败**不再走这条 toast**（改由下方紧凑授权弹窗引导），
         这里只剩非权限类错误（引擎/磁盘等），因此不再需要动作按钮。 -->
    <div v-if="shotError" class="shot-toast shot-toast--error" role="alert">
      <span class="shot-toast-icon shot-toast-icon--error">!</span>
      <span class="shot-toast-msg">{{ shotError }}</span>
      <button class="shot-toast-close" @click="shotError = null" :title="t('app.dismissTip')"><AppIcon name="x" /></button>
    </div>

    <!-- v0.9.4：macOS「屏幕录制」授权紧凑弹窗（一句话原因 + 去授权/重置并重启 +
         2s 轮询翻转「已授权点此重启」+ 详情折叠）。
         v1.2.10（2026-10-26）：**只剩一个驱动源** —— screenshot-permission-blocked
         （用户真的按了截图快捷键、被闸门拦下）。原先还有 screenshot-permission-changed
         （启动预请求后仍未授权），但那个预请求已删除（见 lib.rs setup 内说明：
         每次启动无条件请求 = 「反复弹窗」主因）。现在**启动不再自动弹本弹窗**，
         用户不截图就不会看到它。 -->
    <PermissionDialog
      v-if="permDialogReason"
      :reason="permDialogReason"
      @close="permDialogReason = null"
    />

    <!-- v1.1.0：延时截图悬浮倒计时（借鉴 Snapzy）。小巧不遮挡，Esc 可取消 -->
    <div v-if="countdown > 0" class="shot-countdown">
      <span class="shot-countdown-num">{{ countdown }}</span>
      <span class="shot-countdown-label">{{ t("shot.countdownTip") }}</span>
      <button class="shot-countdown-cancel" @click="onCountdownCancel" :title="t('app.dismissTip')">
        {{ t("shot.countdownCancel") }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
// 应用根组件
// 职责：
// 1. 顶部实时指示「正在记录哪个 App」（让用户在隐藏到托盘后也能确认在记别的软件）
// 2. 启动即自动追踪（由 Rust 端 begin_tracking 保证；此处再调用一次确保万无一失）
// 3. 首次运行默认开启「开机自启」
// 4. macOS 权限缺失时弹出引导横幅

import { ref, onMounted, onBeforeUnmount, computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isTauri } from "./api/tracker";

// v0.6.0-beta 桌宠窗口检测：当前 webview label 是 'pet' 时只渲染 PetWindow（透明/置顶）
// 浏览器预览（非 Tauri）下没有 webview，直接返回 false，避免 getCurrentWebviewWindow() 抛错导致白屏
const currentLabel = isTauri ? getCurrentWebviewWindow().label : "";
const isPetWindow = currentLabel === "pet";
// v0.6.2-beta.17 桌宠菜单窗口检测：label 是 'pet-menu' 时只渲染 PetMenuWindow
const isPetMenuWindow = currentLabel === "pet-menu";
// v0.7.6 悬浮指标条窗口检测：label 是 'float' 时只渲染 FloatBar
const isFloatWindow = currentLabel === "float";
// v0.8.0 截图遮罩窗口检测：label 是 'capture' 时只渲染 CaptureOverlay
const isCaptureWindow = currentLabel === "capture";

// 仅在桌宠窗口引入 PetWindow 组件，避免主窗口打包
import PetWindow from "./pet/PetWindow.vue";
// 仅在菜单窗口引入 PetMenuWindow 组件，避免桌宠窗口和主窗口打包
import PetMenuWindow from "./pet/PetMenuWindow.vue";
// v0.7.6：悬浮指标条分支
import FloatBar from "./float/FloatBar.vue";
// v0.8.0：截图遮罩分支
import CaptureOverlay from "./screenshot/CaptureOverlay.vue";
// v0.9.4：macOS 屏幕录制授权紧凑弹窗
import PermissionDialog from "./screenshot/PermissionDialog.vue";
import Dashboard from "./views/Dashboard.vue";
import { tracker } from "./api/tracker";
import { formatDuration } from "./utils/format";
import type { CurrentForegroundOut, PermissionStatus, ScreenshotResult } from "./types";

const { t } = useI18n();

// 是否正在追踪
const tracking = ref(false);
// 实时前台应用（每 2 秒刷新一次）
// 字段名与 Rust 端 CurrentForegroundOut 返回值一致（snake_case），Tauri v2 不转换返回值
const live = ref<CurrentForegroundOut>({
  name: "",
  process_name: "",
  category_id: "other",
  idle_seconds: 0,
  tracking: false,
  window_title: null,
  session_seconds: 0,
});
// 系统权限状态
const perm = ref<PermissionStatus>({ accessibility: true, screen_capture: true });

// ===== v1.2.10（2026-10-26）：权限横幅的「已关闭」状态落盘 =====
//
// 为什么必须落盘（用户反馈「弹窗反复、干扰」）：
//   原来两个 dismiss 标记都是纯 `ref(false)`，只活在内存里。于是：
//     ① 每次重启应用横幅必然复活（用户点过的「×」毫无记忆）；
//     ② 每次窗口 focus 都被重置（见 onMounted 的 tauri://focus 回调原先会
//        把两个标记都设回 false）—— 用户从系统设置回来、甚至只是切个窗口，
//        刚关掉的横幅又出现了。这是「反复弹」体感最直接的来源。
//
// 键名带版本号：升级后允许重新提示一次（新版可能新增了权限要求），
// 但不能每次启动都重置 —— 那等于没落盘。
const PERM_DISMISS_KEY = "perm_banner_dismissed";
const PERM_INVALID_DISMISS_KEY = "perm_banner_invalid_dismissed";

/** 读一个布尔型 localStorage（异常/无痕模式下静默返回 false，绝不抛） */
function readFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}
/** 写一个布尔型 localStorage（隐私模式 / 配额满时静默忽略） */
function writeFlag(key: string, on: boolean): void {
  try {
    if (on) localStorage.setItem(key, "1");
    else localStorage.removeItem(key);
  } catch {
    /* localStorage 不可用（隐私模式）时退化为「每次都提示」，不影响主功能 */
  }
}

// 用户手动关闭了权限横幅（v1.2.10：落盘，重启后仍记得）
const permDismissed = ref(readFlag(PERM_DISMISS_KEY));
// v1.0.1：授权失效横幅只提示一次（用户关闭后不再骚扰）。
// 「失效」= 屏幕录制已授权但辅助功能未授权 —— 这是升级后 TCC 身份漂移的典型特征
// （ad-hoc 签名每次重建指纹都变，旧授权对新版二进制失效，系统设置开关看着还开着）。
// 与「从未授权」不同：用户大概率以前授权过，只是需要关掉再重开一次，常驻横幅只会造成困扰。
const permInvalidDismissed = ref(readFlag(PERM_INVALID_DISMISS_KEY));
// WebView2 检测结果（仅 Windows 真正生效）
const webview2 = ref<{ os: string; available: boolean; version: string; hint: string } | null>(null);
// 用户手动关闭了 WebView2 横幅
const webview2Dismissed = ref(false);
// v0.8.0：截图完成 toast 文案（空 = 不显示）
const shotToast = ref("");
let toastTimer: number | undefined;
// v0.9.2：截图失败全局提示。v0.9.4 起只承载**非权限类**错误（权限类改走
// 下方紧凑授权弹窗 PermissionDialog），因此从 {msg,perm} 简化为纯字符串。
const shotError = ref<string | null>(null);
let errTimer: number | undefined;
// v0.9.4：紧凑授权弹窗的当前原因码（null = 关闭）。v1.2.10 起**只有一个写入源**：
// screenshot-permission-blocked（用户真按了截图快捷键、被权限闸门拦下时 Rust 发出）。
// 旧的两个写入源之一是启动预请求的 screenshot-permission-changed，该预请求已删除。
const permDialogReason = ref<string | null>(null);
let timer: number | undefined;
// v1.1.0：延时截图倒计时剩余秒数（0 = 无倒计时）。由 Rust screenshot-countdown 事件驱动
const countdown = ref(0);
let countdownKeys: { remove: () => void } | null = null;
// 已注册的 Tauri 事件监听器「注销函数」集合，统一在 onBeforeUnmount 注销（防监听器泄漏）
const unlisteners: Array<() => void> = [];

// 仅当辅助功能未授权时显示横幅（非 macOS 恒为 true，不显示）
// v1.0.1：区分「从未授权」（screen_capture 也没授权，常驻可关）与
// 「升级后失效」（screen_capture 已授权 = 用户以前授权过，是 TCC 漂移特征）——
// 失效横幅文案不同，且关闭后不再重复弹出。
const accessibilityNeverGranted = computed(
  () => !perm.value.accessibility && !perm.value.screen_capture
);
const showPermWarning = computed(() => {
  if (perm.value.accessibility) return false;
  return accessibilityNeverGranted.value
    ? !permDismissed.value
    : !permInvalidDismissed.value;
});

/**
 * v1.2.10：关闭权限横幅 —— 按当前横幅属于哪一档，写入对应的落盘标记。
 *
 * 为什么要抽成函数（原来是模板里的三元表达式）：现在要**同时**改 ref 和
 * localStorage，写在模板里既读不出意图、也容易漏写其中一半（漏写 localStorage
 * 就退回「每次重启都复活」的老问题）。
 * 两档分开记：用户关掉「从未授权」的提示，不应该连带把「升级后失效」的提示也永久静音。
 */
function dismissPermBanner(): void {
  if (accessibilityNeverGranted.value) {
    permDismissed.value = true;
    writeFlag(PERM_DISMISS_KEY, true);
  } else {
    permInvalidDismissed.value = true;
    writeFlag(PERM_INVALID_DISMISS_KEY, true);
  }
}
// WebView2 缺失横幅（仅 Windows + 未安装时显示）
const showWebview2Warning = computed(() => webview2.value !== null && webview2.value.os === "windows" && !webview2.value.available);

// 拉取一次实时前台应用，并同步追踪状态
async function refreshLive() {
  try {
    live.value = await tracker.current();
    tracking.value = live.value.tracking;
  } catch {
    /* 忽略单次刷新失败 */
  }
}

// 把秒数格式化为「X小时Y分钟」（菜单栏展示当前软件已运行时长）
function fmtDur(sec: number): string {
  return formatDuration(sec);
}

// 打开系统设置对应权限面板（macOS）
async function openSettings() {
  try {
    await tracker.openPrivacySettings();
  } catch {
    /* ignore */
  }
}

// 打开 Microsoft WebView2 下载页（Windows）
async function openWebview2Download() {
  try {
    await tracker.openWebview2Download();
  } catch {
    /* ignore */
  }
}

// v1.1.0：取消延时截图倒计时（按钮点击 / Esc 共用）。后端置取消标志 →
// 倒计时循环下秒退出并发 countdown=0 → 本组件收到 0 后自动收起 UI。
async function onCountdownCancel() {
  countdown.value = 0;
  removeCountdownKey();
  try {
    const { screenshot } = await import("./api/screenshot");
    await screenshot.cancelCountdown();
  } catch {
    /* 非 Tauri 环境忽略 */
  }
}

// v1.1.0：倒计时期间挂 Esc 取消（document 级，capture 窗口不接管时也能生效）
function removeCountdownKey() {
  if (countdownKeys) {
    countdownKeys.remove();
    countdownKeys = null;
  }
}
watch(countdown, (v) => {
  if (v > 0 && !countdownKeys) {
    const down = (e: KeyboardEvent) => {
      if (e.key === "Escape") void onCountdownCancel();
    };
    document.addEventListener("keydown", down, true);
    countdownKeys = {
      remove: () => document.removeEventListener("keydown", down, true),
    };
  } else if (v === 0) {
    removeCountdownKey();
  }
});

// 首次运行默认开启「开机自启」：若本地无偏好记录，则开启并保存
async function ensureAutostart() {
  try {
    const pref = await tracker.getAutostartPref();
    if (pref === null) {
      await tracker.setAutostart(true);
    }
  } catch {
    /* 非桌面运行时（纯网页预览）会失败，忽略 */
  }
}

onMounted(async () => {
  // 首次加载：查询权限 + 拉取实时状态，并启动轮询
  try {
    perm.value = await tracker.checkPermissions();
  } catch {
    /* 非 macOS 会失败，已用默认值兜底 */
  }
  // 检测 WebView2 运行时（仅 Windows 真正生效，macOS/Linux 永远 available=true）
  try {
    webview2.value = await tracker.checkWebview2();
  } catch {
    /* 非桌面运行时忽略 */
  }
  // 监听窗口聚焦事件：从系统设置返回后自动重新检查权限状态
  // 这样用户授予权限后无需手动重启程序，回到应用即生效
  try {
    const { listen } = await import("@tauri-apps/api/event");
    unlisteners.push(
      await listen("tauri://focus", async () => {
        // 聚焦后延迟 300ms 重检（macOS TCC 状态更新有微小延迟）
        setTimeout(async () => {
          try {
            const wasAccessibility = perm.value.accessibility;
            perm.value = await tracker.checkPermissions();
            // v1.2.10：**只在「权限真的从无到有」时清除关闭标记**。
            //
            // 原写法是「只要当前 accessibility 为真就重置两个 dismiss 标记」——
            // 但用户从系统设置**授权失败**地回来、或压根没动设置只是切了下窗口，
            // 这个条件同样成立（accessibility 一值未变），于是刚关掉的横幅立刻复活。
            // 这是「点了 × 还在反复弹」最直接的成因之一（另一个是标记没落盘）。
            //
            // 现在改成边沿触发：false → true 才算「恢复」，此时清标记 + 清落盘，
            // 目的是让用户下次真的再次失去权限时还能收到提示（否则永久静音）。
            if (!wasAccessibility && perm.value.accessibility) {
              permDismissed.value = false;
              permInvalidDismissed.value = false;
              writeFlag(PERM_DISMISS_KEY, false);
              writeFlag(PERM_INVALID_DISMISS_KEY, false);
            }
          } catch { /* ignore */ }
        }, 300);
      })
    );
    // 监听「托盘唤起」事件：Rust 端 emit_to("main", "tray-shown")，
    // 唤起后立即拉取一次最新前台应用/已记录时长，避免看到 stale 数据
    unlisteners.push(
      await listen("tray-shown", () => {
        refreshLive();
      })
    );
    // v0.8.0：截图完成（复制 / 落盘 / 两者）→ 主窗口右下角轻提示，
    // 遮罩窗此刻已收起，用户需要在这里看到「已经进剪贴板了」的确认。
    unlisteners.push(
      await listen<ScreenshotResult>("screenshot-done", (ev) => {
        const r = ev.payload;
        shotToast.value =
          r.copied && r.saved
            ? t("shot.toastBoth")
            : r.saved
              ? t("shot.toastSaved")
              : t("shot.toastCopied");
        if (toastTimer) clearTimeout(toastTimer);
        toastTimer = window.setTimeout(() => (shotToast.value = ""), 2600);
      })
    );
    // v0.9.2：截图失败全局提示（任何页面可见）。
    // v0.9.4：权限类失败（__PERM__ 原因码语义）已改走 Rust 的
    // screenshot-permission-blocked 事件 → 紧凑授权弹窗；这条 toast 只收
    // 非权限类错误（引擎失败 / 磁盘错误等），12s 自动消失。
    unlisteners.push(
      await listen<string>("screenshot-error", (ev) => {
        const msg = ev.payload ?? "";
        shotError.value = msg;
        if (errTimer) clearTimeout(errTimer);
        errTimer = window.setTimeout(() => (shotError.value = null), 12000);
      })
    );
    // v0.9.4：截图被权限闸门拦下（Rust begin_capture_inner 发出，payload = 原因码）。
    // 打开紧凑授权弹窗；若弹窗已在（用户连按快捷键），只更新原因码。
    unlisteners.push(
      await listen<string>("screenshot-permission-blocked", (ev) => {
        permDialogReason.value = ev.payload || "not_granted";
      })
    );
    // v0.9.4：启动预请求完成后仍未授权 → 弹紧凑授权窗。
    // v1.2.10（2026-10-26）：**Rust 侧已删除启动预请求，本事件在生产路径上不再发出**
    // （唯一 emit 点随 schedule_startup_permission_request 一起移除）。
    // 这里仍保留监听做两层兜底：① 用户从旧版升级、旧进程可能还在跑；
    // ② 将来若在别处重新引入「主动请求后广播」，语义仍然是「只更新状态，不自动弹窗」——
    //    纯查询引起的状态变化不该抢用户注意力，弹窗只应由「截图被拦」这种明确意图触发。
    unlisteners.push(
      await listen<{ reason: string; permitted: boolean }>(
        "screenshot-permission-changed",
        (ev) => {
          if (ev.payload?.permitted) {
            // 授权已生效：顺手清掉可能残留的弹窗（避免用户授权后仍被挡住）
            permDialogReason.value = null;
          }
        }
      )
    );
    // v1.1.0：延时截图倒计时（Rust begin_capture_delayed 每秒发出，payload = 剩余秒数；
    // 0 = 结束或取消）。>0 显示悬浮倒计时，0 收起。Esc 可取消（见下方keydown）。
    unlisteners.push(
      await listen<number>("screenshot-countdown", (ev) => {
        const remain = ev.payload ?? 0;
        if (remain > 0) {
          countdown.value = remain;
        } else {
          countdown.value = 0;
          // 结束时同步撤掉 keydown 监听
          removeCountdownKey();
        }
      })
    );
  } catch { /* 非 Tauri 环境 */ }

  // 首次运行默认开启开机自启
  await ensureAutostart();
  // 启动即自动追踪（Rust 端也已自动启动，这里再确保一次）
  try {
    await tracker.start();
  } catch {
    /* ignore */
  }
  await refreshLive();
  timer = window.setInterval(refreshLive, 2000);
});
onBeforeUnmount(() => {
  if (timer) clearInterval(timer);
  if (toastTimer) clearTimeout(toastTimer);
  if (errTimer) clearTimeout(errTimer);
  removeCountdownKey();
  // 注销所有 Tauri 事件监听器，防止监听器累积泄漏
  for (const un of unlisteners) {
    try { un(); } catch { /* ignore */ }
  }
  unlisteners.length = 0;
});
</script>
