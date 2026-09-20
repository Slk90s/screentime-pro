<!--
  screenshot/PermissionDialog.vue
  macOS「屏幕录制」授权紧凑弹窗（v0.9.4，2026-09-20）。

  ## 为什么重做（用户反馈 v0.9.3 弹窗「太多太丑」）
  v0.9.3 的失败提示是一大段诊断文字（三步操作 + Translocation / 隔离 / ad-hoc
  证据全糊在 toast 里），且 12 秒自动消失 —— 用户还没看完就没了，也没有
  「授权完成后怎么办」的引导。

  ## v0.9.4 的交互设计
  - **一句话原因**：按 Rust 端结构化原因码（reason）渲染一行说明，不再堆证据；
  - **两个动作**：「去授权」（打开系统设置 → 屏幕录制面板）/「重置权限并重启」
    （tccutil 清掉指向旧指纹的脏记录 + 重启，两步缺一不可）；
  - **2 秒轮询**：弹窗打开期间每 2s 调 `screenshot_permission_status`，
    一旦 `permitted=true` → 状态翻转为「✅ 已授权，点此重启」—— 用户点一下
    就完成重启（TCC 新授权只对新进程生效，不重启等于没授权）；
  - **技术详情折叠**：默认收起，点开才显示（给愿意深究的用户，不糊普通用户的脸）；
  - **不自动消失**：授权是用户要主动完成的动作，弹窗保持到用户处理或手动关闭。

  ## 事件来源（两条链路都会打开本弹窗）
  1. Rust `screenshot-permission-blocked` 事件（截图被闸门拦下，带原因码）；
  2. Rust `screenshot-permission-changed` 事件（启动预请求后仍未授权）。
  两条链路都由 App.vue 监听并控制本组件的 v-if。

  修改历史：
    2026-09-20 @v0.9.4: 初始创建。
-->
<script setup lang="ts">
import { ref, computed, onBeforeUnmount, watch } from "vue";
import { useI18n } from "vue-i18n";
import { screenshot } from "../api/screenshot";
import { tracker } from "../api/tracker";
import { isTauri } from "../api/tracker";

const { t } = useI18n();

// ===== props / emits =====
const props = defineProps<{
  /** Rust 端结构化原因码：app_translocated / quarantined / adhoc_signature / not_granted */
  reason: string;
}>();
const emit = defineEmits<{ (e: "close"): void }>();

// ===== 状态 =====
// 轮询到的最新授权状态（null = 还没查到第一次）
const permitted = ref<boolean | null>(null);
// 轮询定时器
let pollTimer: number | undefined;
// 详情折叠区展开状态
const showDetail = ref(false);
// 「重置并重启」执行中（防双击）
const resetting = ref(false);

// ===== 原因码 → 一句话文案 =====
const reasonLine = computed(() => {
  switch (props.reason) {
    case "app_translocated":
      return t("settings.shotPermReasonTranslocated");
    case "quarantined":
      return t("settings.shotPermReasonQuarantined");
    case "adhoc_signature":
      return t("settings.shotPermReasonAdhoc");
    default:
      return t("settings.shotPermReasonNotGranted");
  }
});

// ===== 动作 =====
async function openSettings() {
  try {
    await screenshot.openPermissionSettings();
  } catch {
    /* ignore */
  }
}

// 重置授权 + 重启（与 v0.9.3 语义一致：先 tccutil 再重启，两步缺一不可）
async function resetAndRestart() {
  if (resetting.value) return;
  resetting.value = true;
  try {
    await tracker.resetScreenCapturePermission();
  } catch {
    /* tccutil 失败（个别系统需 sudo）不阻断重启 */
  }
  try {
    await tracker.restartApp();
  } catch {
    /* ignore */
  }
  resetting.value = false;
}

// ===== 轮询 =====
async function pollOnce() {
  if (!isTauri) return;
  try {
    const s = await screenshot.permissionStatus();
    permitted.value = s.permitted;
    if (s.permitted) stopPoll(); // 翻转后停止轮询（等用户点重启）
  } catch {
    /* 非 macOS / 非 Tauri：静默 */
  }
}

function startPoll() {
  stopPoll();
  void pollOnce();
  pollTimer = window.setInterval(pollOnce, 2000);
}

function stopPoll() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = undefined;
  }
}

// 弹窗挂载即开始轮询；关闭时停止
watch(
  () => props.reason,
  (v) => {
    if (v) startPoll();
    else stopPoll();
  },
  { immediate: true }
);

onBeforeUnmount(stopPoll);
</script>

<template>
  <div class="perm-dialog-mask">
    <div class="perm-dialog" role="dialog" aria-modal="true">
      <!-- 标题 + 关闭 -->
      <div class="perm-dialog-head">
        <span class="perm-dialog-icon">🔒</span>
        <span class="perm-dialog-title">{{ t("settings.shotPermTitle") }}</span>
        <button class="perm-dialog-close" @click="emit('close')" :title="t('common.close')">
          ✕
        </button>
      </div>

      <!-- 一句话原因 -->
      <p class="perm-dialog-reason">{{ reasonLine }}</p>

      <!-- 动作区：未授权时给「去授权 / 重置并重启」；已授权翻转成「点此重启」 -->
      <div class="perm-dialog-actions">
        <template v-if="!permitted">
          <button class="perm-btn perm-btn--primary" @click="openSettings">
            {{ t("settings.shotPermOpen") }}
          </button>
          <button class="perm-btn" :disabled="resetting" @click="resetAndRestart">
            {{ t("settings.shotPermFix") }}
          </button>
        </template>
        <template v-else>
          <div class="perm-dialog-granted">✅ {{ t("settings.shotPermGranted") }}</div>
          <button class="perm-btn perm-btn--primary" @click="resetAndRestart" :disabled="resetting">
            {{ t("settings.shotPermRestart") }}
          </button>
        </template>
      </div>

      <!-- 技术详情折叠区（默认收起） -->
      <button class="perm-dialog-detail-toggle" @click="showDetail = !showDetail">
        {{ showDetail ? "▾" : "▸" }} {{ t("settings.shotPermDetailToggle") }}
      </button>
      <div v-if="showDetail" class="perm-dialog-detail">
        <p>{{ t("settings.shotPermDetailSteps") }}</p>
        <p class="perm-dialog-detail-code">tccutil reset ScreenCapture com.screentime.pro</p>
        <p>{{ t("settings.shotPermDetailAdhoc") }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.perm-dialog-mask {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.35);
  backdrop-filter: blur(2px);
}
.perm-dialog {
  width: min(420px, calc(100vw - 48px));
  background: var(--bg-elev, #fff);
  color: var(--text, #1f2329);
  border-radius: 12px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.22);
  padding: 18px 20px 14px;
}
.perm-dialog-head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.perm-dialog-icon {
  font-size: 18px;
}
.perm-dialog-title {
  font-size: 15px;
  font-weight: 600;
  flex: 1;
}
.perm-dialog-close {
  border: none;
  background: transparent;
  cursor: pointer;
  color: var(--text-2, #8a8f99);
  font-size: 13px;
  padding: 2px 6px;
  border-radius: 6px;
}
.perm-dialog-close:hover {
  background: var(--bg-hover, rgba(0, 0, 0, 0.06));
}
.perm-dialog-reason {
  margin: 12px 0 14px;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text, #1f2329);
}
.perm-dialog-actions {
  display: flex;
  gap: 10px;
  align-items: center;
  flex-wrap: wrap;
}
.perm-btn {
  border: 1px solid var(--border, #dcdfe6);
  background: var(--bg, #fff);
  color: var(--text, #1f2329);
  border-radius: 8px;
  padding: 7px 14px;
  font-size: 13px;
  cursor: pointer;
}
.perm-btn:hover {
  background: var(--bg-hover, rgba(0, 0, 0, 0.04));
}
.perm-btn--primary {
  background: var(--primary, #3370ff);
  border-color: var(--primary, #3370ff);
  color: #fff;
}
.perm-btn--primary:hover {
  filter: brightness(1.06);
}
.perm-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.perm-dialog-granted {
  font-size: 13px;
  color: var(--success, #34c724);
  font-weight: 600;
}
.perm-dialog-detail-toggle {
  margin-top: 14px;
  border: none;
  background: transparent;
  color: var(--text-2, #8a8f99);
  font-size: 12px;
  cursor: pointer;
  padding: 0;
}
.perm-dialog-detail {
  margin-top: 8px;
  padding: 10px 12px;
  background: var(--bg-subtle, rgba(0, 0, 0, 0.03));
  border-radius: 8px;
  font-size: 12px;
  line-height: 1.7;
  color: var(--text-2, #5c5f66);
}
.perm-dialog-detail p {
  margin: 4px 0;
}
.perm-dialog-detail-code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11px;
  color: var(--text, #1f2329);
  user-select: text;
}
</style>
