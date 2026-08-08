<!--
  App.vue
  应用根组件：主窗口（使用时间统计 UI）+ 桌宠独立 webview 分支（PetWindow）+ 桌宠菜单独立 webview 分支（PetMenuWindow，v0.6.2-beta.17）。

  设计思路：
  - 通过 isPetWindow 区分「主窗口」与「桌宠透明置顶窗口」两个 Tauri webview
  - isPetMenuWindow 区分桌宠菜单独立 webview（独立 Tauri 窗口，菜单可拖到全桌面）
  - 顶部实时指示栏读取 get_current_foreground 返回值（snake_case），用于验证确实在采集其他软件
  - 路由用动态组件 + keep-alive，按侧边栏切换 Dashboard / Trends / Rules / Settings

  修改历史：
    - 2026-07-08 @v0.2.0: 初始创建 - 主窗口骨架 + 桌宠窗口分支
    - 2026-07-24 @v0.6.1-beta.1: 修复 - 顶部实时栏 IPC 字段名 bug（camelCase → snake_case，与 Rust 返回值对齐）
    - 2026-07-25 @v0.6.2-beta.17: 新增 - 桌宠右键菜单独立 webview 分支
    - 2026-08-05 @v0.6.2-beta.26: 修复 - 浏览器预览时 getCurrentWebviewWindow() 抛错导致白屏
-->
<template>
  <!-- v0.6.0-beta 桌宠窗口分支：独立 webview 渲染 PetWindow（透明/置顶/无主 UI） -->
  <PetWindow v-if="isPetWindow" />
  <!-- v0.6.2-beta.17 桌宠菜单独立 webview 分支：在独立 Tauri 窗口内渲染菜单，
       position:fixed 相对自己 webview 视口（≈ 全桌面），菜单可自由拖到任意位置 -->
  <PetMenuWindow v-else-if="isPetMenuWindow" />
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
        <span class="live-title" v-if="live.window_title">· {{ live.window_title }}</span>
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
    <div v-if="showPermWarning && !permDismissed" class="perm-banner">
      <span>{{ t("app.permWarning") }}</span>
      <div class="perm-actions">
        <button @click="openSettings">{{ t("app.openSettings") }}</button>
        <button class="perm-dismiss" @click="permDismissed = true" :title="t('app.dismissTip')"><AppIcon name="x" /></button>
      </div>
    </div>

    <!-- 菜单栏纯后台模式下，窗口可随时关闭到托盘；此处提供「隐藏到菜单栏」按钮 -->
    <main>
      <Dashboard />
    </main>
  </div>
</template>

<script setup lang="ts">
// 应用根组件
// 职责：
// 1. 顶部实时指示「正在记录哪个 App」（让用户在隐藏到托盘后也能确认在记别的软件）
// 2. 启动即自动追踪（由 Rust 端 begin_tracking 保证；此处再调用一次确保万无一失）
// 3. 首次运行默认开启「开机自启」
// 4. macOS 权限缺失时弹出引导横幅

import { ref, onMounted, onBeforeUnmount, computed } from "vue";
import { useI18n } from "vue-i18n";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isTauri } from "./api/tracker";

// v0.6.0-beta 桌宠窗口检测：当前 webview label 是 'pet' 时只渲染 PetWindow（透明/置顶）
// 浏览器预览（非 Tauri）下没有 webview，直接返回 false，避免 getCurrentWebviewWindow() 抛错导致白屏
const currentLabel = isTauri ? getCurrentWebviewWindow().label : "";
const isPetWindow = currentLabel === "pet";
// v0.6.2-beta.17 桌宠菜单窗口检测：label 是 'pet-menu' 时只渲染 PetMenuWindow
const isPetMenuWindow = currentLabel === "pet-menu";

// 仅在桌宠窗口引入 PetWindow 组件，避免主窗口打包
import PetWindow from "./pet/PetWindow.vue";
// 仅在菜单窗口引入 PetMenuWindow 组件，避免桌宠窗口和主窗口打包
import PetMenuWindow from "./pet/PetMenuWindow.vue";
import Dashboard from "./views/Dashboard.vue";
import { tracker } from "./api/tracker";
import { formatDuration } from "./utils/format";
import type { CurrentForegroundOut, PermissionStatus } from "./types";

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
// 用户手动关闭了权限横幅（关闭后不再显示，除非页面重新加载）
const permDismissed = ref(false);
// WebView2 检测结果（仅 Windows 真正生效）
const webview2 = ref<{ os: string; available: boolean; version: string; hint: string } | null>(null);
// 用户手动关闭了 WebView2 横幅
const webview2Dismissed = ref(false);
let timer: number | undefined;
// Tauri focus 事件监听器 unlisten 函数
let unlistenFocus: (() => void) | null = null;

// 仅当辅助功能未授权时显示横幅（非 macOS 恒为 true，不显示）
const showPermWarning = computed(() => !perm.value.accessibility);
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
    unlistenFocus = await listen("tauri://focus", async () => {
      // 聚焦后延迟 300ms 重检（macOS TCC 状态更新有微小延迟）
      setTimeout(async () => {
        try {
          perm.value = await tracker.checkPermissions();
          // 如果检测到权限已恢复，重置关闭标记
          if (perm.value.accessibility) {
            permDismissed.value = false;
          }
        } catch { /* ignore */ }
      }, 300);
    });
    // 监听「托盘唤起」事件：Rust 端 emit_to("main", "tray-shown")，
    // 唤起后立即拉取一次最新前台应用/已记录时长，避免看到 stale 数据
    await listen("tray-shown", () => {
      refreshLive();
    });
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
  if (unlistenFocus) unlistenFocus();
});
</script>
