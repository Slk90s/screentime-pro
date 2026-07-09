<template>
  <!-- 设置页：设备名 / 空闲阈值 / 保留天数 / 开机自启 / 备份与多设备合并 / 关于
       v0.3.1：所有操作反馈改用 Modal 弹窗（避免大屏下 toast 被忽略） -->
  <div class="settings">
    <section class="card">
      <h3>设备与数据</h3>

      <div class="field">
        <label>本机设备名称</label>
        <input v-model="deviceName" type="text" placeholder="例如：我的 MacBook Pro" />
        <p class="hint">用于多设备数据合并时区分来源，导出备份中会携带此名称。</p>
      </div>

      <div class="field">
        <label>空闲阈值（分钟）</label>
        <input v-model.number="idleMin" type="number" min="1" max="60" />
        <p class="hint">超过此时长无操作视为「离开」，不计入有效使用时长。</p>
      </div>

      <div class="field">
        <label>数据保留天数</label>
        <input v-model.number="retention" type="number" min="30" max="3650" />
        <p class="hint">超过保留期的旧记录会在「清理旧数据」时删除，默认 365 天。</p>
      </div>

      <div class="field row">
        <label>开机自启</label>
        <input v-model="autostart" type="checkbox" @change="onAutostart" />
      </div>

      <button class="save" @click="onSave">保存设置</button>
    </section>

    <section class="card">
      <h3>备份与多设备合并</h3>
      <p class="hint">
        把本机全量数据导出为 JSON 备份；或从其他设备导出的文件合并进来（按时间+应用+设备去重）。
      </p>
      <div class="btns">
        <button @click="onExport">导出备份</button>
        <button @click="pickImport">导入合并</button>
        <input
          ref="fileInput"
          type="file"
          accept="application/json,.json"
          hidden
          @change="onImport"
        />
      </div>

      <div class="danger-zone">
        <h4>危险操作</h4>
        <div class="btns">
          <button class="danger" @click="confirmCleanAll">清理 {{ retention }} 天前的旧数据</button>
          <button class="danger" @click="openDevicePrune">按设备清理</button>
        </div>
        <p class="hint danger-hint">这两个操作不可恢复，请先在「导出备份」中保留一份 JSON 备份。</p>
      </div>
    </section>

    <section class="card">
      <h3>关于</h3>
      <div class="about">
        <div>
          <span>应用版本</span>
          <b class="mono">{{ version }}</b>
          <button class="check-update" @click="onCheckUpdate" :disabled="checking">
            {{ checking ? "检查中…" : "检查更新" }}
          </button>
        </div>
        <div
          v-if="updateResult"
          class="update-result"
          :class="{ outdated: updateResult.has_update }"
        >
          <template v-if="updateResult.has_update">
            发现新版本 <b>v{{ updateResult.latest }}</b>（当前 v{{ updateResult.current }}）
            <button class="link-btn" @click="goDownload(updateResult.url)">前往下载 →</button>
          </template>
          <template v-else>
            已是最新版本（v{{ updateResult.current }}）
          </template>
        </div>
        <div>
          <span>设备 ID</span>
          <b class="mono">{{ settings.device_id || "—" }}</b>
        </div>
        <div><span>数据存储</span><b>本地 SQLite · 零上传 · 隐私优先</b></div>
      </div>
    </section>

    <!-- ============ 通用反馈/确认弹窗（替代原 toast 顶部横条）============ -->
    <Modal
      v-model="alertOpen"
      :type="alertType"
      :title="alertTitle"
      :message="alertMsg"
      :confirm-text="'确定'"
      :cancel-text="alertType === 'info' ? '' : '取消'"
      width="420px"
      @confirm="onAlertConfirm"
    />

    <!-- ============ 导出成功后的弹窗（带「在访达中显示 / 复制路径」操作）============ -->
    <Modal
      v-model="exportDialogOpen"
      type="info"
      title="已导出备份"
      :message="`文件已保存到：\n${exportPath}`"
      confirm-text="确定"
      cancel-text=""
      width="520px"
    >
      <template #footer>
        <button class="modal-btn cancel" @click="reveal(exportPath)">在访达中显示</button>
        <button class="modal-btn cancel" @click="copy(exportPath)">复制路径</button>
        <button class="modal-btn primary" @click="exportDialogOpen = false">关闭</button>
      </template>
    </Modal>

    <!-- ============ 按设备清理弹窗 ============ -->
    <Modal
      v-model="pruneDialogOpen"
      type="warn"
      title="按设备清理数据"
      :message="`将删除下列选中设备【全部】sessions（不限 365 天）。\n\n系统会在删除前自动导出该设备的 JSON 备份到本机，便于误删时恢复。备份不会被自动删除，请记得手动复制到安全位置。`"
      :confirm-text="selectedDeviceIds.length === 0 ? '清全部设备（> 365 天）' : `清理 ${selectedDeviceIds.length} 台设备（全量）`"
      cancel-text="取消"
      width="640px"
      @confirm="onConfirmPruneByDevice"
    >
      <div class="device-list">
        <div v-if="deviceStats.length === 0" class="empty">
          加载中…
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
                <span v-if="d.device_id === settings.device_id" class="self-tag">本机</span>
                <span
                  v-else-if="!d.device_name || d.device_name === d.device_id"
                  class="default-tag"
                  title="该设备没有设置名称（可能是从旧版备份导入的数据）"
                >未命名</span>
              </div>
              <div class="device-meta">
                <span class="mono">{{ d.device_id }}</span>
                <span>·</span>
                <span>{{ formatSeconds(d.total_seconds) }}</span>
                <span>·</span>
                <span>{{ d.session_count }} 条 session</span>
                <span v-if="d.earliest_date">·</span>
                <span v-if="d.earliest_date">{{ d.earliest_date }} → {{ d.latest_date }}</span>
              </div>
            </div>
          </label>
          <p class="hint">
            勾选具体设备 → 清理该设备全部数据 + 自动导出 JSON 备份到本机<br />
            留空（不勾选） → 按保留天数（${retention} 天）清理全部设备的旧数据
          </p>
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
import { getVersion } from "@tauri-apps/api/app";
import Modal from "../components/Modal.vue";
import { tracker } from "../api/tracker";
import type { DeviceStats, SettingsOut, UpdateInfo } from "../types";
import { formatDuration } from "../utils/format";

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
    showAlert("warn", "加载失败", `加载设备列表失败：${e?.message || e}`);
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
      showAlert("info", "已清理", `已清理 ${n} 条旧记录（全部设备，> ${retention.value} 天）`);
      pruneDialogOpen.value = false;
    } catch (e) {
      showAlert("warn", "清理失败", "清理失败：" + (e instanceof Error ? e.message : String(e)));
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
      "已清理",
      `已清理 ${totalDeleted} 条 sessions（${ids.length} 台设备）。\n\n${ids.length} 份备份已保存到本机：\n${backups.join("\n")}\n\n⚠️ 备份不会被自动删除，请手动复制到安全位置（误删可从备份文件导入恢复）。`
    );
  } catch (e) {
    console.error("按设备清理失败", e);
    showAlert(
      "warn",
      "清理失败",
      "清理失败：" + (e instanceof Error ? e.message : String(e))
    );
  }
}

function confirmCleanAll() {
  showAlert(
    "confirm",
    "清理全部设备的旧数据",
    `确定清理 ${retention.value} 天之前【全部设备】的数据吗？此操作不可恢复。`,
    async () => {
      try {
        const n = await tracker.pruneData(retention.value);
        showAlert("info", "已清理", `已清理 ${n} 条旧记录（全部设备）`);
      } catch (e) {
        console.error("清理失败", e);
        showAlert(
          "warn",
          "清理失败",
          "清理失败：" + (e instanceof Error ? e.message : String(e))
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
    showAlert("info", "已更新", autostart.value ? "已开启开机自启" : "已关闭开机自启");
  } catch (e) {
    showAlert("warn", "设置失败", "开机自启设置失败：" + e);
  }
}

async function onSave() {
  try {
    await tracker.saveSettings({
      idleThreshold: idleMin.value * 60,
      deviceName: deviceName.value.trim() || settings.value.device_name,
      dataRetentionDays: retention.value,
    });
    showAlert("info", "已保存", "设置已保存");
  } catch (e) {
    console.error("保存设置失败", e);
    showAlert(
      "warn",
      "保存失败",
      "保存失败：" + (e instanceof Error ? e.message : String(e))
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
      "导出失败",
      "导出失败：" + (e instanceof Error ? e.message : String(e))
    );
  }
}

// 在系统文件管理器中打开导出文件（macOS 访达 / Windows 资源管理器 / Linux 文件管理器）
async function reveal(path: string) {
  try {
    await tracker.revealPath(path);
  } catch (e) {
    showAlert("warn", "打开失败", "打开失败：" + (e instanceof Error ? e.message : String(e)));
  }
}

async function copy(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    showAlert("info", "已复制", "路径已复制到剪贴板");
  } catch {
    showAlert("warn", "复制失败", "复制失败，路径：" + path);
  }
}

function pickImport() {
  fileInput.value?.click();
}

async function onImport(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (!file) return;
  try {
    const text = await file.text();
    const n = await tracker.importData(text);
    showAlert("info", "导入成功", `已合并导入 ${n} 条记录`);
  } catch (err) {
    console.error("导入失败", err);
    showAlert(
      "warn",
      "导入失败",
      "导入失败：" + (err instanceof Error ? err.message : String(err))
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
      "打开失败",
      `无法打开下载页：${e?.message || e}\n\n请手动访问：\n${url}`
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
        "发现新版本",
        `当前 v${updateResult.value.current} → 最新 v${updateResult.value.latest}\n\n请前往 GitHub Releases 下载最新安装包。`
      );
    } else {
      showAlert("info", "已是最新版本", `当前版本 v${updateResult.value.current} 已是最新`);
    }
  } catch (e: any) {
    console.error("检查更新失败", e);
    showAlert(
      "warn",
      "检查更新失败",
      `检查更新失败：${e?.message || e}\n\n常见原因：网络不通 / GitHub 限流 / 仓库地址错误。`
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