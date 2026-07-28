<template>
  <!-- 设置页（v0.6.2-beta.19 重构）
       视觉：每功能一卡片，head 区域 圆角色块图标 + 标题/副标题，body 区域放交互控件。
       功能不裁：原 Settings 全部功能（设备名/空闲阈值/保留天数/语言/自启/备份导入/日志/危险区/桌宠/皮肤/编辑器/检查更新/关于）全部保留。 -->
  <div class="settings">
    <!-- ============ 顶部品牌色横条 + 标题 ============ -->
    <div class="settings-header">
      <div class="header-bar" />
      <h2>设置</h2>
    </div>

    <!-- ============ 语言 ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-orange">
          <AppIcon name="language" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.languageTitle") }}</h3>
          <p>{{ t("settings.languageDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="radio-pills">
          <label class="radio-pill" :class="{ active: i18n.global.locale.value === 'zh-CN' }">
            <input
              type="radio"
              name="lang"
              value="zh-CN"
              :checked="i18n.global.locale.value === 'zh-CN'"
              @change="onLangChange"
            />
            <span class="radio-dot" />
            <span>简体中文</span>
          </label>
          <label class="radio-pill" :class="{ active: i18n.global.locale.value === 'en-US' }">
            <input
              type="radio"
              name="lang"
              value="en-US"
              :checked="i18n.global.locale.value === 'en-US'"
              @change="onLangChange"
            />
            <span class="radio-dot" />
            <span>English</span>
          </label>
        </div>
      </div>
    </div>

    <!-- ============ 通用（设备名 / 空闲阈值 / 保留天数 / 开机自启）============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-blue">
          <AppIcon name="settings" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.generalTitle") }}</h3>
          <p>{{ t("settings.generalDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="form-row">
          <label>{{ t("settings.deviceName") }}</label>
          <input
            v-model="deviceName"
            type="text"
            class="text-input"
            :placeholder="t('settings.deviceNamePh')"
          />
          <p class="field-hint">{{ t("settings.deviceNameHint") }}</p>
        </div>

        <div class="form-row">
          <label>{{ t("settings.idleThreshold") }}</label>
          <input
            v-model.number="idleMin"
            type="number"
            min="1"
            max="60"
            class="text-input narrow"
          />
          <p class="field-hint">{{ t("settings.idleHint") }}</p>
        </div>

        <div class="form-row">
          <label>{{ t("settings.retention") }}</label>
          <input
            v-model.number="retention"
            type="number"
            min="30"
            max="3650"
            class="text-input narrow"
          />
          <p class="field-hint">{{ t("settings.retentionHint") }}</p>
        </div>

        <div class="form-row row">
          <label>{{ t("settings.autostart") }}</label>
          <label class="toggle-switch" :class="{ on: autostart }">
            <input
              type="checkbox"
              :checked="autostart"
              @change="onAutostart($event)"
            />
            <span class="toggle-slider" />
            <span class="toggle-state">
              {{ autostart ? t("settings.autostartOn") : t("settings.autostartOff") }}
            </span>
          </label>
        </div>

        <div class="card-actions">
          <button class="primary-btn" @click="onSave">
            <AppIcon name="save" :size="14" /> {{ t("settings.save") }}
          </button>
        </div>
      </div>
    </div>

    <!-- ============ 设备 ID ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-blue">
          <AppIcon name="keyRound" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.deviceIdTitle") }}</h3>
          <p>{{ t("settings.deviceIdDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="id-bar">
          <code class="id-mono">{{ settings.device_id || "—" }}</code>
          <div class="id-actions">
            <button class="ghost-btn" @click="copy(settings.device_id || '')">
              <AppIcon name="copy" :size="14" /> {{ t("common.copy") }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- ============ 备份与多设备合并 ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-green">
          <AppIcon name="database" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.backupMerge") }}</h3>
          <p>{{ t("settings.backupHint") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="btn-row">
          <button class="ghost-btn" @click="onExport">
            <AppIcon name="download" :size="14" /> {{ t("settings.export") }}
          </button>
          <button class="ghost-btn" @click="pickImport">
            <AppIcon name="upload" :size="14" /> {{ t("settings.import") }}
          </button>
          <input
            ref="fileInput"
            type="file"
            accept="application/json,.json"
            hidden
            @change="onImport"
          />
        </div>

        <div class="sub-zone">
          <h4><AppIcon name="tool" :size="14" /> {{ t("settings.diag") }}</h4>
          <p class="field-hint" v-html="t('settings.diagHint')" />
          <p class="field-hint" v-if="logSize !== null" v-html="t('settings.logSize', { size: formatBytes(logSize) })" />
          <div class="btn-row">
            <button class="ghost-btn" @click="exportLogs">
              <AppIcon name="clipboard" :size="14" /> {{ t("settings.exportLogs") }}
            </button>
            <button class="ghost-btn" @click="revealLogDir">
              <AppIcon name="folder" :size="14" /> {{ t("settings.openLogDir") }}
            </button>
            <button class="ghost-btn" @click="refreshLogSize">
              <AppIcon name="refresh" :size="14" /> {{ t("settings.refresh") }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- ============ 桌宠（开关 + 操作 + 皮肤 + 编辑器）============ -->
    <div class="setting-card pet-card">
      <div class="card-head">
        <div class="head-icon icon-pink">
          <AppIcon name="paw" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("pet.settings.title") }}</h3>
          <p>{{ t("pet.settings.enabledDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="form-row row">
          <label>{{ t("pet.settings.enabled") }}</label>
          <label class="toggle-switch" :class="{ on: petStore.enabled }">
            <input
              type="checkbox"
              :checked="petStore.enabled"
              @change="onTogglePet(($event.target as HTMLInputElement).checked)"
            />
            <span class="toggle-slider" />
            <span class="toggle-state">
              {{ petStore.enabled ? t("pet.settings.on") : t("pet.settings.off") }}
            </span>
          </label>
        </div>

        <div class="btn-row" v-if="petStore.enabled">
          <button class="ghost-btn" @click="onShowPet">
            <AppIcon name="eye" :size="14" /> {{ t("pet.settings.open") }}
          </button>
          <button class="ghost-btn" @click="onHidePet">
            <AppIcon name="eyeOff" :size="14" /> {{ t("pet.settings.close") }}
          </button>
          <button class="ghost-btn" @click="onResetPetPos">
            <AppIcon name="rotateCcw" :size="14" /> {{ t("pet.settings.resetPos") }}
          </button>
        </div>

        <!-- 皮肤 -->
        <div class="sub-zone">
          <h4><AppIcon name="sparkles" :size="14" /> {{ t("pet.settings.skinTitle") }}</h4>
          <p class="field-hint">{{ t("pet.settings.skinDesc") }}</p>
          <div class="pet-skin-grid">
            <button
              v-for="s in skinList"
              :key="s.id"
              type="button"
              class="pet-skin-tile"
              :class="{ 'is-active': s.id === activeSkinId }"
              :aria-pressed="s.id === activeSkinId"
              @click="pickSkin(s.id)"
            >
              <div class="pet-skin-head">
                <span class="pet-skin-emoji" :aria-hidden="true">
                  {{ s.id === 'popmart-3d' ? '🎁' : '🐾' }}
                </span>
                <span class="pet-skin-name">{{ s.name }}</span>
              </div>
              <div class="pet-skin-desc">{{ s.description }}</div>
            </button>
          </div>
        </div>

        <p v-if="petStore.isHungry && petStore.enabled" class="pet-hungry">
          <AppIcon name="leaf" :size="14" /> {{ t("pet.feed.full") }} → {{ t("pet.feed.title") }}
        </p>

        <div class="card-actions" v-if="petStore.enabled">
          <button class="ghost-btn" @click="showEditor = true">
            <AppIcon name="penTool" :size="14" />
            {{ t("pet.settings.editor") }}
          </button>
        </div>
      </div>
    </div>

    <!-- 桌宠编辑器（Teleport 到 body，避免被 card 裁剪） -->
    <Teleport to="body">
      <PetSpriteEditor :visible="showEditor" @close="showEditor = false" />
    </Teleport>

    <!-- ============ 检查更新 ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-blue">
          <AppIcon name="refresh" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.updateTitle") }}</h3>
          <p>{{ t("settings.updateDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="btn-row btn-row-end">
          <button class="primary-btn outline" :disabled="checking" @click="onCheckUpdate">
            <AppIcon name="refresh" :size="14" />
            {{ checking ? t("settings.checking") : t("settings.checkUpdate") }}
          </button>
        </div>
        <p v-if="updateResult" class="field-hint" :class="{ outdated: updateResult.has_update }">
          <template v-if="updateResult.has_update">
            {{ t("settings.foundNew") }} <b>v{{ updateResult.latest }}</b>（当前 v{{ updateResult.current }}）
            <button class="link-btn" @click="goDownload(updateResult.url)">
              {{ t("settings.goDownload") }}
            </button>
          </template>
          <template v-else>
            {{ t("settings.upToDate", { current: updateResult.current }) }}
          </template>
        </p>
      </div>
    </div>

    <!-- ============ 关于 ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-blue">
          <AppIcon name="info" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.about") }}</h3>
          <p>{{ t("settings.aboutDesc") }}</p>
        </div>
      </div>
      <div class="card-body about-list">
        <div class="meta-row">
          <span class="meta-label">{{ t("settings.appVersion") }}</span>
          <span class="meta-value mono">{{ version }}</span>
        </div>
        <div class="meta-row">
          <span class="meta-label">{{ t("settings.deviceId") }}</span>
          <span class="meta-value mono">{{ settings.device_id || "—" }}</span>
        </div>
        <div class="meta-row">
          <span class="meta-label">{{ t("settings.storageLabel") }}</span>
          <span class="meta-value">{{ t("settings.storage") }}</span>
        </div>
      </div>
    </div>

    <!-- ============ 危险区 ============ -->
    <div class="setting-card danger-card">
      <div class="card-head">
        <div class="head-icon icon-red">
          <AppIcon name="warning" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.danger") }}</h3>
          <p>{{ t("settings.dangerZoneDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="btn-row">
          <button class="danger-btn" @click="confirmCleanAll">
            <AppIcon name="trash" :size="14" />
            {{ t("settings.cleanOld", { days: retention }) }}
          </button>
          <button class="danger-btn" @click="openDevicePrune">
            <AppIcon name="trash" :size="14" /> {{ t("settings.pruneByDevice") }}
          </button>
        </div>
        <p class="danger-hint">{{ t("settings.dangerHint") }}</p>
      </div>
    </div>

    <!-- ============ 通用反馈/确认弹窗 ============ -->
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

    <!-- ============ 导出成功后的弹窗 ============ -->
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

    <!-- ============ 日志导出成功后的弹窗 ============ -->
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
                  :title="t('settings.unnamedTip', '该设备没有设置名称（可能是从旧版备份导入的数据）')"
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
          <p class="field-hint" v-html="t('settings.pruneHint', { days: retention })" />
        </div>
      </div>
    </Modal>
  </div>
</template>

<script setup lang="ts">
// 设置页（v0.6.2-beta.19 重构）
// - 视觉：按"图标 + 标题/描述 + 控件"卡片化重组（原 card 列表 → 多功能分卡）
// - 功能：100% 保留，script 段内方法、状态、副作用与原实现一致
// - 关键变更：
//   1. 语言 radio 改成"胶囊式"组件（截图风格），仍走 i18n.setLocale
//   2. 设备 ID 抽出独立卡片，仅"复制"按钮（截图里的"重置"按钮非原功能，移除避免调用未注册的 reset_device_id）
//   3. 备份与诊断合并到一张卡（"数据库"图标）
//   4. 桌宠卡含 开关 / 操作按钮 / 皮肤 / 编辑器入口
//   5. 危险区改为独立卡片，警示色

import { ref, onMounted, onBeforeUnmount, computed } from "vue";
import { useI18n } from "vue-i18n";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Modal from "../components/Modal.vue";
import AppIcon from "../components/AppIcon.vue";
import { tracker } from "../api/tracker";
import { i18n, setLocale, type Locale } from "../i18n";
import type { DeviceStats, SettingsOut, UpdateInfo } from "../types";
import { formatDuration } from "../utils/format";
import { petStore } from "../pet/stores/petStore";
import PetSpriteEditor from "../pet/components/PetSpriteEditor.vue";
import { skinRegistry } from "../pet/skins/registry";
import "../pet/skins";

const { t } = useI18n();

// 桌宠编辑器显示状态
const showEditor = ref(false);

// v0.6.2-beta.2：皮肤选择器
const skinList = computed(() => skinRegistry.list());
const activeSkinId = computed(() => skinRegistry.active().id);
function pickSkin(id: string): void {
  skinRegistry.setActive(id);
}

// v0.6.2-beta.3：跨窗口皮肤同步
let unlistenSkin: (() => void) | null = null;
let unlistenPetEnabled: (() => void) | null = null;
onMounted(async () => {
  try {
    unlistenSkin = await listen("pet-skin-changed", () => {
      skinRegistry.reloadActive();
    });
  } catch (e) {
    console.error("[Settings] 监听 pet-skin-changed 失败", e);
  }
  try {
    unlistenPetEnabled = await listen("pet-enabled-changed", () => {
      petStore.reload();
    });
  } catch (e) {
    console.error("[Settings] 监听 pet-enabled-changed 失败", e);
  }
});
onBeforeUnmount(() => {
  if (unlistenSkin) unlistenSkin();
  if (unlistenPetEnabled) unlistenPetEnabled();
});

// 语言切换
function onLangChange(e: Event) {
  setLocale((e.target as HTMLInputElement).value as Locale);
}

// 桌宠控制
async function onTogglePet(checked: boolean) {
  petStore.setEnabled(checked);
  if (checked) {
    invoke("create_pet_window")
      .then(() => invoke("show_pet_window"))
      .then(() => {
        const pos = petStore.position;
        return invoke("move_pet_window", { x: pos.x, y: pos.y });
      })
      .catch((err) => {
        console.error("[pet] 切换桌宠失败", err);
        showAlert("warn", t("common.error"), String(err));
      });
  } else {
    invoke("hide_pet_window").catch((err) => {
      console.error("[pet] 隐藏桌宠失败", err);
    });
  }
}
async function onShowPet() {
  try {
    await invoke("create_pet_window");
    await invoke("show_pet_window");
    const pos = petStore.position;
    await invoke("move_pet_window", { x: pos.x, y: pos.y });
  } catch (err) {
    console.error("[pet] 显示桌宠失败", err);
    showAlert("warn", t("common.error"), String(err));
  }
}
async function onHidePet() {
  try {
    await invoke("hide_pet_window");
  } catch (err) {
    console.error("[pet] 隐藏桌宠失败", err);
  }
}
function onResetPetPos() {
  const screenW = window.screen.width;
  const screenH = window.screen.height;
  petStore.setPosition(screenW - 200, screenH - 240);
  const pos = petStore.position;
  invoke("move_pet_window", { x: pos.x, y: pos.y }).catch(() => {});
}

const settings = ref<SettingsOut>({
  device_id: "",
  device_name: "",
  idle_threshold: 300,
  data_retention_days: 365,
  sample_interval: 2,
  autostart: false,
});

const version = ref("");
const checking = ref(false);
const updateResult = ref<UpdateInfo | null>(null);

const deviceName = ref("");
const idleMin = ref(5);
const retention = ref(365);
const autostart = ref(false);

const fileInput = ref<HTMLInputElement>();

// ============ 通用弹窗 ============
const alertOpen = ref(false);
const alertType = ref<"info" | "confirm" | "warn">("info");
const alertTitle = ref("");
const alertMsg = ref("");
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

// ============ 导出/按设备清理 ============
const exportDialogOpen = ref(false);
const exportPath = ref("");

const pruneDialogOpen = ref(false);
const deviceStats = ref<DeviceStats[]>([]);
const selectedDeviceIds = ref<string[]>([]);

async function openDevicePrune() {
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

async function onConfirmPruneByDevice() {
  const ids = selectedDeviceIds.value;
  if (ids.length === 0) {
    try {
      const n = await tracker.pruneData(retention.value);
      showAlert("info", t("settings.cleaned"), t("settings.cleanedOld", { n, days: retention.value }));
      pruneDialogOpen.value = false;
    } catch (e) {
      showAlert("warn", t("settings.cleanFailed"), t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
    }
    return;
  }
  pruneDialogOpen.value = false;
  let totalDeleted = 0;
  const backups: string[] = [];
  try {
    for (const id of ids) {
      const res = await tracker.backupAndPruneDevice(id);
      totalDeleted += res.deleted_count;
      backups.push(res.backup_path);
    }
    showAlert(
      "info",
      t("settings.cleaned"),
      t("settings.cleanedByDeviceMsg", { n: totalDeleted, devices: ids.length, backups: backups.join("\n") })
    );
  } catch (e) {
    console.error("按设备清理失败", e);
    showAlert("warn", t("settings.cleanFailed"), t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
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
        showAlert("warn", t("settings.cleanFailed"), t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
      }
    }
  );
}

// （v0.6.2-beta.19 移除"重置设备 ID"按钮：后端未实现 reset_device_id 命令，截图里的"重置"按钮只用作视觉示意，保留会让用户点了报错。设备 ID 不可重置：它绑定了 sessions 数据。）

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    /* 浏览器预览模式忽略 */
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

// 切换开机自启（v0.6.2-beta.23：去掉成功弹窗；v0.6.2-beta.24：乐观更新 + 失败回滚）
// 关键：<input> 是单向 :checked 绑定，vue 下次 render 会用 autostart.value 强制覆盖原生 checked，
// 若这里不翻转 autostart.value，toggle 会"弹回"，表现成"点不动"。故先乐观翻转，失败再回滚。
async function onAutostart(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  autostart.value = next; // 乐观更新：立即反映到 :checked 与 :class="{ on }"
  try {
    await tracker.setAutostart(next);
  } catch (err) {
    autostart.value = !next; // 失败回滚
    showAlert("warn", t("settings.autostartFailed"), t("settings.autostartFailedMsg", { err: err instanceof Error ? err.message : String(err) }));
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
    showAlert("warn", t("settings.saveFailed"), t("settings.saveFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function onExport() {
  try {
    const res = await tracker.exportAll();
    exportPath.value = res.path;
    exportDialogOpen.value = true;
  } catch (e) {
    console.error("导出失败", e);
    showAlert("warn", t("settings.exportFailed"), t("settings.exportFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

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
    void refreshLogSize();
  } catch (e) {
    console.error("导出日志失败", e);
    showAlert("warn", t("settings.exportFailed"), t("settings.exportFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function revealLogDir() {
  try {
    const logDir = await invoke<string>("get_log_dir");
    await tracker.revealPath(logDir);
  } catch (e) {
    console.error("打开日志目录失败", e);
    showAlert("warn", t("settings.openFailed"), t("settings.openFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

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
    showAlert("warn", t("settings.importFailed"), t("settings.importFailedMsg", { err: err instanceof Error ? err.message : String(err) }));
  } finally {
    (e.target as HTMLInputElement).value = "";
  }
}

async function goDownload(url: string) {
  try {
    await tracker.openUrl(url);
  } catch (e: any) {
    showAlert("warn", t("settings.openFailed"), t("settings.openDownloadFailed", { err: e?.message || e, url }));
  }
}

async function onCheckUpdate() {
  checking.value = true;
  updateResult.value = null;
  try {
    updateResult.value = await tracker.checkUpdate();
    if (updateResult.value.has_update) {
      showAlert("info", t("settings.foundNew"), t("settings.newVersionMsg", { current: updateResult.value.current, latest: updateResult.value.latest }));
    } else {
      showAlert("info", t("settings.upToDate"), t("settings.upToDateMsg", { current: updateResult.value.current }));
    }
  } catch (e: any) {
    console.error("检查更新失败", e);
    showAlert("warn", t("settings.checkUpdateFailed"), t("settings.checkUpdateFailedMsg", { err: e?.message || e }));
  } finally {
    checking.value = false;
  }
}
</script>

<style scoped>
/* ============ v0.6.2-beta.19 卡片化样式 ============ */
.settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 720px;
}

/* 页头：左侧品牌色横条 + 标题 */
.settings-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 4px 0 8px;
}
.settings-header h2 {
  font-size: 20px;
  font-weight: 700;
  color: var(--text);
  margin: 0;
  letter-spacing: 0.2px;
}
.header-bar {
  width: 4px;
  height: 22px;
  border-radius: 2px;
  background: var(--accent, #ff7e27);
}

/* 卡片本体 */
.setting-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 16px 18px;
  transition: box-shadow 0.18s ease, border-color 0.18s ease;
}
.setting-card:hover {
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.04);
}

/* 卡片头：图标 + 标题/副标题 */
.card-head {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 14px;
}
.head-icon {
  width: 40px;
  height: 40px;
  flex-shrink: 0;
  border-radius: 11px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
}
.icon-orange { background: linear-gradient(135deg, #ff8a3d, #ff7e27); }
.icon-blue   { background: linear-gradient(135deg, #5a9cff, #3b82f6); }
.icon-green  { background: linear-gradient(135deg, #4cd998, #34c759); }
.icon-red    { background: linear-gradient(135deg, #ff6b6b, #ef4444); }
.icon-pink   { background: linear-gradient(135deg, #ff8db3, #ff6b9d); }

.head-text { min-width: 0; }
.head-text h3 {
  font-size: 15px;
  font-weight: 600;
  margin: 0;
  color: var(--text);
  line-height: 1.3;
}
.head-text p {
  font-size: 12.5px;
  color: var(--text-dim, #86868b);
  margin: 2px 0 0;
  line-height: 1.45;
}

/* 卡片 body */
.card-body { padding: 2px 0 0; }

/* 表单行 */
.form-row { margin-bottom: 14px; }
.form-row:last-of-type { margin-bottom: 8px; }
.form-row > label {
  display: block;
  font-size: 13px;
  color: var(--text-dim, #86868b);
  margin-bottom: 6px;
  font-weight: 500;
}
.form-row.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.form-row.row > label { margin-bottom: 0; }
.field-hint {
  font-size: 12px;
  color: var(--text-dim, #86868b);
  margin: 6px 0 0;
  line-height: 1.5;
}

/* 文本输入 */
.text-input {
  width: 100%;
  max-width: 320px;
  padding: 8px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  font-size: 14px;
  background: var(--bg, #f5f5f7);
  color: var(--text);
  transition: border-color 0.15s, background 0.15s;
}
.text-input:focus {
  outline: none;
  border-color: var(--accent, #ff7e27);
  background: var(--card);
  box-shadow: 0 0 0 3px rgba(255, 126, 39, 0.12);
}
.text-input.narrow { max-width: 160px; }

/* 胶囊式单选（语言） */
.radio-pills {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}
.radio-pill {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 7px 14px;
  border: 1.5px solid var(--border);
  border-radius: 999px;
  font-size: 13px;
  color: var(--text);
  background: var(--bg, #f5f5f7);
  cursor: pointer;
  user-select: none;
  -webkit-user-select: none;
  transition: border-color 0.15s, background 0.15s, color 0.15s;
}
.radio-pill:hover { border-color: rgba(255, 126, 39, 0.45); }
.radio-pill.active {
  border-color: var(--accent, #ff7e27);
  background: rgba(255, 126, 39, 0.08);
  color: var(--accent, #ff7e27);
  font-weight: 500;
}
.radio-pill input { display: none; }
.radio-dot {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 1.5px solid var(--border);
  background: var(--card);
  position: relative;
  flex-shrink: 0;
  transition: border-color 0.15s, background 0.15s;
}
.radio-pill.active .radio-dot {
  border-color: var(--accent, #ff7e27);
  background: var(--accent, #ff7e27);
}
.radio-pill.active .radio-dot::after {
  content: '';
  position: absolute;
  inset: 3px;
  background: #fff;
  border-radius: 50%;
}

/* 卡片底部操作 */
.card-actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 8px;
  padding-top: 12px;
  border-top: 1px dashed rgba(0, 0, 0, 0.06);
}

/* 主按钮（实心品牌色） */
.primary-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: none;
  background: var(--accent, #ff7e27);
  color: #fff;
  padding: 8px 18px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s, transform 0.05s;
}
.primary-btn:hover { background: #f56f1a; }
.primary-btn:active { transform: scale(0.97); }
.primary-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.primary-btn.outline {
  background: transparent;
  color: var(--accent, #ff7e27);
  border: 1px solid var(--accent, #ff7e27);
}
.primary-btn.outline:hover { background: rgba(255, 126, 39, 0.08); }

/* Ghost 按钮（次要操作） */
.ghost-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid var(--border);
  background: var(--bg, #f5f5f7);
  color: var(--text);
  padding: 7px 14px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}
.ghost-btn:hover {
  border-color: var(--accent, #ff7e27);
  color: var(--accent, #ff7e27);
  background: rgba(255, 126, 39, 0.05);
}
.ghost-btn:active { transform: scale(0.97); }
.ghost-btn.danger:hover {
  border-color: #ef4444;
  color: #ef4444;
  background: rgba(239, 68, 68, 0.05);
}

/* 危险按钮 */
.danger-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid rgba(239, 68, 68, 0.4);
  background: rgba(239, 68, 68, 0.04);
  color: #ef4444;
  padding: 7px 14px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}
.danger-btn:hover {
  background: rgba(239, 68, 68, 0.1);
  border-color: #ef4444;
}
.danger-btn:active { transform: scale(0.97); }
.danger-hint {
  font-size: 12px;
  color: #ef4444;
  margin: 8px 0 0;
  line-height: 1.5;
  opacity: 0.8;
}

/* 按钮行 */
.btn-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 4px;
}
.btn-row-end { justify-content: flex-end; }

/* 设备 ID 条 */
.id-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.id-mono {
  flex: 1;
  min-width: 200px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 13px;
  padding: 8px 12px;
  background: var(--bg, #f5f5f7);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text);
  word-break: break-all;
}
.id-actions { display: flex; gap: 6px; flex-shrink: 0; }

/* 子区（缩进 + 顶部虚线） */
.sub-zone {
  margin-top: 16px;
  padding-top: 12px;
  border-top: 1px dashed rgba(0, 0, 0, 0.08);
}
.sub-zone h4 {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  margin: 0 0 6px;
  color: var(--text);
}

/* 关于区（meta 行） */
.about-list { display: flex; flex-direction: column; gap: 0; }
.meta-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  font-size: 13px;
  border-bottom: 1px solid var(--border);
}
.meta-row:last-child { border-bottom: none; }
.meta-label { color: var(--text-dim, #86868b); }
.meta-value { color: var(--text); font-weight: 500; }
.meta-value.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12.5px;
}

/* 桌宠卡（粉色高亮） */
.pet-card {
  border-left: 3px solid #ff6b9d;
}
.pet-hungry {
  color: var(--accent, #ff7e27);
  font-weight: 500;
  font-size: 12.5px;
  margin: 8px 0 0;
  display: flex;
  align-items: center;
  gap: 6px;
}

/* 皮肤选择器 */
.pet-skin-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-top: 8px;
}
@media (max-width: 480px) {
  .pet-skin-grid { grid-template-columns: 1fr; }
}
.pet-skin-tile {
  appearance: none;
  border: 1.5px solid var(--border, #e5e7eb);
  border-radius: 10px;
  padding: 10px 12px;
  text-align: left;
  background: var(--bg, #fff);
  color: inherit;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s, transform 0.05s;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font: inherit;
}
.pet-skin-tile:hover { border-color: rgba(255, 126, 39, 0.45); }
.pet-skin-tile:active { transform: scale(0.98); }
.pet-skin-tile.is-active {
  border-color: var(--accent, #ff7e27);
  background: rgba(255, 126, 39, 0.06);
  box-shadow: 0 0 0 3px rgba(255, 126, 39, 0.10);
}
.pet-skin-head { display: flex; align-items: center; gap: 6px; }
.pet-skin-emoji { font-size: 18px; line-height: 1; }
.pet-skin-name { font-weight: 600; font-size: 13px; }
.pet-skin-desc { font-size: 11px; opacity: 0.7; line-height: 1.3; }

/* 危险区卡片 */
.danger-card {
  border-color: rgba(239, 68, 68, 0.25);
  background: linear-gradient(to bottom, var(--card), rgba(239, 68, 68, 0.02));
}

/* Toggle Switch（iOS 风） */
.toggle-switch {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
  -webkit-user-select: none;
  flex-shrink: 0;
  white-space: nowrap;
}
.toggle-switch input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}
.toggle-slider {
  display: inline-block; /* span 默认 inline 会让 width/height 失效，必须显式声明 */
  position: relative;
  width: 44px;
  height: 24px;
  flex-shrink: 0;
  background: #d1d1d6;
  border-radius: 12px;
  transition: background 0.25s ease;
  vertical-align: middle;
}
.toggle-slider::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 20px;
  height: 20px;
  background: #fff;
  border-radius: 50%;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform 0.25s ease;
}
.toggle-switch.on .toggle-slider { background: var(--accent, #ff7e27); }
.toggle-switch.on .toggle-slider::after { transform: translateX(20px); }
.toggle-state {
  font-size: 12.5px;
  color: var(--text-dim, #86868b);
  font-weight: 500;
}
.toggle-switch.on .toggle-state { color: var(--accent, #ff7e27); }

/* 更新结果行 */
.outdated { color: var(--accent, #FF7E27) !important; }
.link-btn {
  margin-left: 8px;
  background: none;
  border: 1px solid var(--accent, #FF7E27);
  color: var(--accent, #FF7E27);
  padding: 2px 10px;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
}
.link-btn:hover { background: var(--accent, #FF7E27); color: #fff; }

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
.device-row:hover { background: var(--bg-soft, rgba(0, 0, 0, 0.03)); }
.device-row.checked {
  border-color: var(--accent, #FF7E27);
  background: rgba(255, 126, 39, 0.06);
}
.device-row input[type="checkbox"] { margin-top: 4px; cursor: pointer; }
.device-info { flex: 1; min-width: 0; }
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
  background: var(--accent, #FF7E27);
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
