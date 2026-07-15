<template>
  <!-- 设置页：设备名 / 空闲阈值 / 保留天数 / 开机自启 / 备份与多设备合并 / 关于
       v0.3.1：所有操作反馈改用 Modal 弹窗（避免大屏下 toast 被忽略） -->
  <div class="settings">
    <section class="card">
      <h3>{{ t("settings.deviceData") }}</h3>

      <div class="field">
        <label>{{ t("settings.deviceName") }}</label>
        <input v-model="deviceName" type="text" :placeholder="t('settings.deviceNamePh')" />
        <p class="hint">{{ t("settings.deviceNameHint") }}</p>
      </div>

      <div class="field">
        <label>{{ t("settings.idleThreshold") }}</label>
        <input v-model.number="idleMin" type="number" min="1" max="60" />
        <p class="hint">{{ t("settings.idleHint") }}</p>
      </div>

      <div class="field">
        <label>{{ t("settings.retention") }}</label>
        <input v-model.number="retention" type="number" min="30" max="3650" />
        <p class="hint">{{ t("settings.retentionHint") }}</p>
      </div>

      <div class="field row">
        <label>{{ t("settings.autostart") }}</label>
        <input v-model="autostart" type="checkbox" @change="onAutostart" />
      </div>

      <div class="field row">
        <label>{{ t("language.label") }}</label>
        <select :value="i18n.global.locale.value" @change="onLangChange">
          <option value="zh-CN">{{ t("language.zhCN") }}</option>
          <option value="en-US">{{ t("language.enUS") }}</option>
        </select>
      </div>

      <button class="save" @click="onSave">{{ t("settings.save") }}</button>
    </section>

    <section class="card">
      <h3>{{ t("settings.backupMerge") }}</h3>
      <p class="hint">
        {{ t("settings.backupHint") }}
      </p>
      <div class="btns">
        <button @click="onExport">{{ t("settings.export") }}</button>
        <button @click="pickImport">{{ t("settings.import") }}</button>
        <input
          ref="fileInput"
          type="file"
          accept="application/json,.json"
          hidden
          @change="onImport"
        />
      </div>

      <div class="diag-zone">
        <h4>{{ t("settings.diag") }}</h4>
        <p class="hint" v-html="t('settings.diagHint')"></p>
        <p class="hint" v-if="logSize !== null" v-html="t('settings.logSize', { size: formatBytes(logSize) })"></p>
        <div class="btns">
          <button @click="exportLogs">{{ t("settings.exportLogs") }}</button>
          <button class="reveal" @click="revealLogDir">{{ t("settings.openLogDir") }}</button>
          <button @click="refreshLogSize">{{ t("settings.refresh") }}</button>
        </div>
      </div>

      <div class="danger-zone">
        <h4>{{ t("settings.danger") }}</h4>
        <div class="btns">
          <button class="danger" @click="confirmCleanAll">{{ t("settings.cleanOld", { days: retention }) }}</button>
          <button class="danger" @click="openDevicePrune">{{ t("settings.pruneByDevice") }}</button>
        </div>
        <p class="hint danger-hint">{{ t("settings.dangerHint") }}</p>
      </div>
    </section>

    <section class="card">
      <h3>{{ t("settings.about") }}</h3>
      <div class="about">
        <div>
          <span>{{ t("settings.appVersion") }}</span>
          <b class="mono">{{ version }}</b>
          <button class="check-update" @click="onCheckUpdate" :disabled="checking">
            {{ checking ? t("settings.checking") : t("settings.checkUpdate") }}
          </button>
        </div>
        <div
          v-if="updateResult"
          class="update-result"
          :class="{ outdated: updateResult.has_update }"
        >
          <template v-if="updateResult.has_update">
            {{ t("settings.foundNew") }} <b>v{{ updateResult.latest }}</b>（当前 v{{ updateResult.current }}）
            <button class="link-btn" @click="goDownload(updateResult.url)">{{ t("settings.goDownload") }}</button>
          </template>
          <template v-else>
            {{ t("settings.upToDate", { current: updateResult.current }) }}
          </template>
        </div>
        <div>
          <span>{{ t("settings.deviceId") }}</span>
          <b class="mono">{{ settings.device_id || "—" }}</b>
        </div>
        <div><span>{{ t("settings.storage") }}</span><b>{{ t("settings.storage") }}</b></div>
      </div>
    </section>

    <!-- ============ 通用反馈/确认弹窗（替代原 toast 顶部横条）============ -->
    <Modal
      v-model="alertOpen"
      :type="alertType"
      :title="alertTitle"
      :message="alertMsg"
      :confirm-text="t('common.confirm')"
      :cancel-text="alertType === 'info' ? '' : t('common.cancel')"
      width="420px"
      @confirm="onAlertConfirm"
    />

    <!-- ============ 导出成功后的弹窗（带「在访达中显示 / 复制路径」操作）============ -->
    <Modal
      v-model="exportDialogOpen"
      type="info"
      :title="t('settings.exportedBackup')"
      :message="t('settings.exportedMsg', { path: exportPath })"
      :confirm-text="t('common.confirm')"
      cancel-text=""
      width="520px"
    >
      <template #footer>
        <button class="modal-btn cancel" @click="reveal(exportPath)">{{ t("common.revealInFM") }}</button>
        <button class="modal-btn cancel" @click="copy(exportPath)">{{ t("common.copyPath") }}</button>
        <button class="modal-btn primary" @click="exportDialogOpen = false">{{ t("common.close") }}</button>
      </template>
    </Modal>

    <!-- ============ 日志导出成功后的弹窗（v0.4.2 新增）============ -->
    <Modal
      v-model="logExportDialogOpen"
      type="info"
      :title="t('settings.logExported')"
      :message="t('settings.logExportedMsg', { path: logExportPath })"
      :confirm-text="t('common.confirm')"
      cancel-text=""
      width="520px"
    >
      <template #footer>
        <button class="modal-btn cancel" @click="reveal(logExportPath)">{{ t("common.revealInFM") }}</button>
        <button class="modal-btn cancel" @click="copy(logExportPath)">{{ t("common.copyPath") }}</button>
        <button class="modal-btn primary" @click="logExportDialogOpen = false">{{ t("common.close") }}</button>
      </template>
    </Modal>

    <!-- ============ 按设备清理弹窗 ============ -->
    <Modal
      v-model="pruneDialogOpen"
      type="warn"
      :title="t('settings.pruneTitle')"
      :message="t('settings.pruneMsg')"
      :confirm-text="selectedDeviceIds.length === 0 ? t('settings.pruneAllConfirm') : t('settings.pruneNConfirm', { n: selectedDeviceIds.length })"
      :cancel-text="t('common.cancel')"
      width="640px"
      @confirm="onConfirmPruneByDevice"
    >
      <div class="device-list">
        <div v-if="deviceStats.length === 0" class="empty">
          {{ t("settings.loading") }}
        </div>
        <div v-else>
          <label
            v-for="d in deviceStats"
            :key="d.device_id"
            class="device-row"
            :class="{ checked: selectedDeviceIds.includes(d.device_id) }"
          >
            <input
              type="checkbox"
              :value="d.device_id"
              v-model="selectedDeviceIds"
            />
            <div class="device-info">
              <div class="device-name">
                {{ d.device_name || d.device_id }}
                <span v-if="d.device_id === settings.device_id" class="self-tag">{{ t("settings.selfTag") }}</span>
                <span
                  v-else-if="!d.device_name || d.device_name === d.device_id"
                  class="default-tag"
                  title="该设备没有设置名称（可能是从旧版备份导入的数据）"
                >{{ t("settings.unnamed") }}</span>
              </div>
              <div class="device-meta">
                <span class="mono">{{ d.device_id }}</span>
                <span>·</span>
                <span>{{ formatSeconds(d.total_seconds) }}</span>
                <span>·</span>
                <span>{{ t("settings.sessions", { n: d.session_count }) }}</span>
                <span v-if="d.earliest_date">·</span>
                <span v-if="d.earliest_date">{{ d.earliest_date }} → {{ d.latest_date }}</span>
              </div>
            </div>
          </label>
          <p class="hint" v-html="t('settings.pruneHint', { days: retention })"></p>
        </div>
      </div>
    </Modal>
  </div>
</template>

<script setup lang="ts">
// 设置页
// 关键变更：
// - 所有反馈改用 Modal 弹窗（替代顶部 toast 横条）
// - 按设备清理改用弹窗 + 多选 checkbox（不再用文本输入框）
// - 检查更新失败时把错误消息放进弹窗，用户能看到具体 HTTP 码/响应

import { ref, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import Modal from "../components/Modal.vue";
import { tracker } from "../api/tracker";
import { i18n, setLocale, type Locale } from "../i18n";
import type { DeviceStats, SettingsOut, UpdateInfo } from "../types";
import { formatDuration } from "../utils/format";

const { t } = useI18n();

// 语言下拉切换：更新并持久化（图表等会经 watch(locale) 自动重绘）
function onLangChange(e: Event) {
  setLocale((e.target as HTMLSelectElement).value as Locale);
}

const settings = ref<SettingsOut>({
  device_id: "",
  device_name: "",
  idle_threshold: 300,
  data_retention_days: 365,
  sample_interval: 2,
  autostart: false,
});

// 应用版本：动态读取打包版本（tauri.conf.json），避免 UI 写死导致与实际不符
const version = ref("0.4.1");

// 检查更新状态
const checking = ref(false);
const updateResult = ref<UpdateInfo | null>(null);

// 表单绑定
const deviceName = ref("");
const idleMin = ref(5);
const retention = ref(365);
const autostart = ref(false);

const fileInput = ref<HTMLInputElement>();

// ============ 通用弹窗（替代原 showToast）============
const alertOpen = ref(false);
const alertType = ref<"info" | "confirm" | "warn">("info");
const alertTitle = ref("");
const alertMsg = ref("");
/** 通用弹窗 confirm 时执行的回调（用于「清理」「删除」等异步操作） */
let pendingConfirm: (() => void | Promise<void>) | null = null;
function showAlert(
  type: "info" | "confirm" | "warn",
  title: string,
  msg: string,
  onConfirm?: () => void | Promise<void>
) {
  alertType.value = type;
  alertTitle.value = title;
  alertMsg.value = msg;
  pendingConfirm = onConfirm ?? null;
  alertOpen.value = true;
}
function onAlertConfirm() {
  if (pendingConfirm) {
    const cb = pendingConfirm;
    pendingConfirm = null;
    void cb();
  }
}

// ============ 导出成功专用弹窗 ============
const exportDialogOpen = ref(false);
const exportPath = ref("");

// ============ 按设备清理弹窗 ============
const pruneDialogOpen = ref(false);
const deviceStats = ref<DeviceStats[]>([]);
const selectedDeviceIds = ref<string[]>([]);

async function openDevicePrune() {
  // 打开前先拉取设备列表
  pruneDialogOpen.value = true;
  selectedDeviceIds.value = [];
  try {
    deviceStats.value = await tracker.devicesWithStats();
  } catch (e: any) {
    showAlert("warn", t("settings.loadFailed"), t("settings.loadDevicesFailed", { err: e?.message || e }));
  }
}

function formatSeconds(s: number): string {
  return formatDuration(s);
}

// 按设备清理：弹窗确认后，对每个选中设备做「先备份再全删」
const backupResultPath = ref("");

async function onConfirmPruneByDevice() {
  const ids = selectedDeviceIds.value;
  if (ids.length === 0) {
    // 留空 = 清全部设备 → 走原来的 pruneData（保留天数），不做自动备份
    try {
      const n = await tracker.pruneData(retention.value);
      showAlert("info", t("settings.cleaned"), t("settings.cleanedOld", { n, days: retention.value }));
      pruneDialogOpen.value = false;
    } catch (e) {
      showAlert("warn", t("settings.cleanFailed"), t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
    }
    return;
  }
  // 选中具体设备 → 每个都先备份再全删
  pruneDialogOpen.value = false; // 关闭列表弹窗
  let totalDeleted = 0;
  const backups: string[] = [];
  try {
    for (const id of ids) {
      const res = await tracker.backupAndPruneDevice(id);
      totalDeleted += res.deleted_count;
      backups.push(res.backup_path);
    }
    // 弹结果弹窗：列出所有备份路径 + 在访达中显示 / 复制全部
    backupResultPath.value = backups.join("\n");
    showAlert(
      "info",
      t("settings.cleaned"),
      t("settings.cleanedByDeviceMsg", { n: totalDeleted, devices: ids.length, backups: backups.join("\n") })
    );
  } catch (e) {
    console.error("按设备清理失败", e);
    showAlert(
      "warn",
      t("settings.cleanFailed"),
      t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) })
    );
  }
}

function confirmCleanAll() {
  showAlert(
    "confirm",
    t("settings.cleanAllConfirmTitle"),
    t("settings.cleanAllConfirmMsg", { days: retention.value }),
    async () => {
      try {
        const n = await tracker.pruneData(retention.value);
        showAlert("info", t("settings.cleaned"), t("settings.cleanedSimple", { n }));
      } catch (e) {
        console.error("清理失败", e);
        showAlert(
          "warn",
          t("settings.cleanFailed"),
          t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) })
        );
      }
    }
  );
}

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    /* 浏览器预览模式忽略，保留默认 */
  }
  try {
    const s = await tracker.getSettings();
    settings.value = s;
    deviceName.value = s.device_name;
    idleMin.value = Math.max(1, Math.round(s.idle_threshold / 60));
    retention.value = s.data_retention_days;
    autostart.value = s.autostart;
  } catch {
    /* 浏览器预览模式忽略 */
  }
});

// 切换开机自启（单独调用后端命令，即时生效）
async function onAutostart() {
  try {
    await tracker.setAutostart(autostart.value);
    showAlert("info", t("settings.updated"), autostart.value ? t("settings.autostartOn") : t("settings.autostartOff"));
  } catch (e) {
    showAlert("warn", t("settings.autostartFailed"), t("settings.autostartFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function onSave() {
  try {
    await tracker.saveSettings({
      idleThreshold: idleMin.value * 60,
      deviceName: deviceName.value.trim() || settings.value.device_name,
      dataRetentionDays: retention.value,
    });
    showAlert("info", t("settings.saved"), t("settings.savedMsg"));
  } catch (e) {
    console.error("保存设置失败", e);
    showAlert(
      "warn",
      t("settings.saveFailed"),
      t("settings.saveFailedMsg", { err: e instanceof Error ? e.message : String(e) })
    );
  }
}

async function onExport() {
  try {
    const res = await tracker.exportAll();
    exportPath.value = res.path;
    exportDialogOpen.value = true;
  } catch (e) {
    console.error("导出失败", e);
    showAlert(
      "warn",
      t("settings.exportFailed"),
      t("settings.exportFailedMsg", { err: e instanceof Error ? e.message : String(e) })
    );
  }
}

// 在系统文件管理器中打开导出文件（macOS 访达 / Windows 资源管理器 / Linux 文件管理器）
async function reveal(path: string) {
  try {
    await tracker.revealPath(path);
  } catch (e) {
    showAlert("warn", t("settings.openFailed"), t("settings.openFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function copy(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    showAlert("info", t("settings.copied"), t("settings.copiedMsg"));
  } catch {
    showAlert("warn", t("settings.copyFailed"), t("settings.copyFailedMsg", { err: path }));
  }
}

// ============ 日志导出（v0.4.2 引入）============
const logSize = ref<number | null>(null);
const logExportDialogOpen = ref(false);
const logExportPath = ref("");

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${units[i]}`;
}

async function refreshLogSize() {
  try {
    const size = await invoke<number>("get_log_size");
    logSize.value = size;
  } catch (e) {
    console.error("读取日志大小失败", e);
    logSize.value = null;
  }
}

async function exportLogs() {
  try {
    const res = await invoke<{ path: string }>("export_logs");
    logExportPath.value = res.path;
    logExportDialogOpen.value = true;
    // 导出后刷新大小
    void refreshLogSize();
  } catch (e) {
    console.error("导出日志失败", e);
    showAlert(
      "warn",
      t("settings.exportFailed"),
      t("settings.exportFailedMsg", { err: e instanceof Error ? e.message : String(e) })
    );
  }
}

async function revealLogDir() {
  // 通过 reveal_path 命令打开日志所在目录
  try {
    const logDir = await invoke<string>("get_log_dir");
    await tracker.revealPath(logDir);
  } catch (e) {
    console.error("打开日志目录失败", e);
    showAlert(
      "warn",
      t("settings.openFailed"),
      t("settings.openFailedMsg", { err: e instanceof Error ? e.message : String(e) })
    );
  }
}

// 挂载时拉一次日志大小
onMounted(() => {
  void refreshLogSize();
});

function pickImport() {
  fileInput.value?.click();
}

async function onImport(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (!file) return;
  try {
    const text = await file.text();
    const n = await tracker.importData(text);
    showAlert("info", t("settings.importSuccess"), t("settings.importedMsg", { n }));
  } catch (err) {
    console.error("导入失败", err);
    showAlert(
      "warn",
      t("settings.importFailed"),
      t("settings.importFailedMsg", { err: err instanceof Error ? err.message : String(err) })
    );
  } finally {
    (e.target as HTMLInputElement).value = "";
  }
}

/** 「前往下载」按钮：通过 Rust 调系统浏览器打开 URL（Tauri WebView 默认拦截 target=_blank） */
async function goDownload(url: string) {
  try {
    await tracker.openUrl(url);
  } catch (e: any) {
    showAlert(
      "warn",
      t("settings.openFailed"),
      t("settings.openDownloadFailed", { err: e?.message || e, url })
    );
  }
}

// 点「检查更新」：调 Rust check_for_update 拉 GitHub Releases
async function onCheckUpdate() {
  checking.value = true;
  updateResult.value = null;
  try {
    updateResult.value = await tracker.checkUpdate();
    if (updateResult.value.has_update) {
      showAlert(
        "info",
        t("settings.foundNew"),
        t("settings.newVersionMsg", { current: updateResult.value.current, latest: updateResult.value.latest })
      );
    } else {
      showAlert("info", t("settings.upToDate"), t("settings.upToDateMsg", { current: updateResult.value.current }));
    }
  } catch (e: any) {
    console.error("检查更新失败", e);
    showAlert(
      "warn",
      t("settings.checkUpdateFailed"),
      t("settings.checkUpdateFailedMsg", { err: e?.message || e })
    );
  } finally {
    checking.value = false;
  }
}
</script>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 720px;
}
.card {
  padding: 18px 20px;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 12px;
}
h3 {
  font-size: 15px;
  font-weight: 600;
  margin: 0 0 14px;
  color: var(--text);
}
h4 {
  font-size: 13px;
  margin: 16px 0 8px;
  color: var(--text-dim);
  font-weight: 600;
}
.field {
  margin-bottom: 16px;
}
.field > label {
  display: block;
  font-size: 13px;
  color: var(--text-dim);
  margin-bottom: 6px;
}
.field input[type="text"],
.field input[type="number"] {
  width: 100%;
  max-width: 320px;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  font-size: 14px;
  background: var(--bg, #fff);
  color: var(--text);
}
.field.row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.field.row label {
  margin: 0;
}
.hint {
  font-size: 12px;
  color: var(--text-dim);
  margin: 6px 0 0;
  line-height: 1.5;
}
.danger-hint {
  color: #c0392b;
}
.save {
  border: none;
  background: var(--brand, #ff7e27);
  color: #fff;
  padding: 9px 20px;
  border-radius: 8px;
  font-size: 14px;
  cursor: pointer;
}
.btns {
  display: flex;
  gap: 10px;
  margin: 12px 0;
  flex-wrap: wrap;
}
.btns button {
  border: 1px solid var(--border);
  background: var(--bg, #fff);
  color: var(--text);
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
}
.danger-zone {
  margin-top: 18px;
  padding: 12px 14px;
  border: 1px dashed #e0a;
  border-radius: 10px;
  background: rgba(224, 0, 170, 0.04);
}
.danger {
  border: 1px solid #e0a !important;
  background: transparent !important;
  color: #d9534f !important;
}
.about div {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  border-bottom: 1px solid var(--border);
  font-size: 13px;
}
.about div:last-child {
  border-bottom: none;
}
.about span {
  color: var(--text-dim);
}
.check-update {
  font-size: 12px;
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--card);
  cursor: pointer;
}
.check-update:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.update-result {
  font-size: 13px;
  padding: 6px 0;
  color: var(--text-muted);
}
.update-result.outdated {
  color: var(--brand, #FF7E27);
}
.update-result a {
  margin-left: 8px;
  color: var(--brand, #FF7E27);
  text-decoration: underline;
}
.update-result .link-btn {
  margin-left: 8px;
  background: none;
  border: 1px solid var(--brand, #FF7E27);
  color: var(--brand, #FF7E27);
  padding: 2px 10px;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
}
.update-result .link-btn:hover {
  background: var(--brand, #FF7E27);
  color: #fff;
}
.about b {
  color: var(--text);
  font-weight: 500;
}
.about .mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}
/* 按设备清理弹窗内设备列表 */
.device-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 360px;
  overflow: auto;
  padding: 4px 2px;
}
.device-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}
.device-row:hover {
  background: var(--bg-soft, rgba(0, 0, 0, 0.03));
}
.device-row.checked {
  border-color: var(--brand, #FF7E27);
  background: rgba(255, 126, 39, 0.06);
}
.device-row input[type="checkbox"] {
  margin-top: 4px;
  cursor: pointer;
}
.device-info {
  flex: 1;
  min-width: 0;
}
.device-name {
  font-weight: 600;
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.device-meta {
  font-size: 11px;
  color: var(--text-dim);
  margin-top: 4px;
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.self-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  background: var(--brand, #FF7E27);
  color: #fff;
}
.default-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  background: rgba(192, 57, 43, 0.12);
  color: #c0392b;
}
.empty {
  text-align: center;
  color: var(--text-dim);
  padding: 24px 0;
}
</style>