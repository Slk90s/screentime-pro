<template>
  <!-- 设置页：设备名 / 空闲阈值 / 保留天数 / 开机自启 / 备份与多设备合并 -->
  <div class="settings">
    <!-- 操作反馈条：保存/导出/导入/清理的结果都显示在这里，确保有可见反馈（不再依赖原生 alert） -->
    <transition name="fade">
      <div v-if="toast.show" class="toast" :class="toast.type">
        <span>{{ toast.msg }}</span>
        <button
          v-if="toast.path"
          class="toast-btn"
          @click="reveal(toast.path!)"
        >
          在访达中显示
        </button>
        <button v-if="toast.path" class="toast-btn" @click="copy(toast.path)">
          复制路径
        </button>
        <button class="toast-close" @click="toast.show = false">×</button>
      </div>
    </transition>

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

      <div class="field">
        <label>按设备清理（可选）</label>
        <input v-model="pruneDeviceId" placeholder="输入设备 ID（12 位），留空=清全部" />
        <p class="hint">多设备合并场景下输入某台设备的 ID，只清该台旧数据；不影响其它设备。</p>
      </div>

      <div class="field row">
        <label>开机自启</label>
        <input v-model="autostart" type="checkbox" @change="onAutostart" />
      </div>

      <button class="save" @click="onSave">保存设置</button>
    </section>

    <section class="card">
      <h3>备份与多设备合并</h3>
      <p class="hint">把本机全量数据导出为 JSON 备份；或从其他设备导出的文件合并进来（按时间+应用+设备去重）。</p>
      <div class="btns">
        <button @click="onExport">导出备份</button>
        <button @click="pickImport">导入合并</button>
        <input ref="fileInput" type="file" accept="application/json,.json" hidden @change="onImport" />
      </div>
      <button class="danger" @click="onPrune">清理 {{ retention }} 天前的旧数据</button>
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
        <div v-if="updateResult" class="update-result" :class="{ outdated: updateResult.has_update }">
          <template v-if="updateResult.has_update">
            发现新版本 <b>v{{ updateResult.latest }}</b>（当前 v{{ updateResult.current }}）
            <a :href="updateResult.url" target="_blank" rel="noopener">前往下载 →</a>
          </template>
          <template v-else>
            已是最新版本（v{{ updateResult.current }}）
          </template>
        </div>
        <div><span>设备 ID</span><b class="mono">{{ settings.device_id || "—" }}</b></div>
        <div><span>数据存储</span><b>本地 SQLite · 零上传 · 隐私优先</b></div>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { tracker } from "../api/tracker";
import type { SettingsOut } from "../types";

const settings = ref<SettingsOut>({
  device_id: "",
  device_name: "",
  idle_threshold: 300,
  data_retention_days: 365,
  sample_interval: 2,
  autostart: false,
});

// 应用版本：动态读取打包版本（tauri.conf.json），避免 UI 写死导致与实际不符
const version = ref("0.3.0");

// 检查更新状态
const checking = ref(false);
const updateResult = ref<{
  current: string;
  latest: string;
  has_update: boolean;
  url: string;
  notes: string;
} | null>(null);

// 点「检查更新」：调 Rust check_for_update 拉 GitHub Releases API
async function onCheckUpdate() {
  checking.value = true;
  updateResult.value = null;
  try {
    updateResult.value = await tracker.checkUpdate();
    if (updateResult.value.has_update) {
      showToast("ok", `发现新版本 v${updateResult.value.latest}，点击下方链接下载`);
    } else {
      showToast("ok", `已是最新版本 v${updateResult.value.current}`);
    }
  } catch (e: any) {
    showToast("err", `检查更新失败：${e?.message || e}`);
  } finally {
    checking.value = false;
  }
}

// 表单绑定（空闲阈值以分钟展示）
const deviceName = ref("");
const idleMin = ref(5);
const retention = ref(365);
const autostart = ref(false);
// 按设备清理输入框（12 位 hex，空=清全部）
const pruneDeviceId = ref("");

const fileInput = ref<HTMLInputElement>();

// 操作反馈条（替代原生 alert，确保每次点击都有可见结果）
const toast = ref<{
  show: boolean;
  type: "ok" | "err";
  msg: string;
  path?: string;
}>({ show: false, type: "ok", msg: "" });

let toastTimer: number | undefined;
function showToast(type: "ok" | "err", msg: string, path?: string) {
  toast.value = { show: true, type, msg, path };
  if (toastTimer) clearTimeout(toastTimer);
  // 失败提示停留更久；成功提示 4 秒后自动消失（有 path 时不自动消失，等用户操作）
  const ms = type === "err" || path ? 0 : 4000;
  if (ms > 0) toastTimer = window.setTimeout(() => (toast.value.show = false), ms);
}

onMounted(async () => {
  // 动态读取应用版本（打包后即为真实版本号；浏览器预览兜底显示默认）
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
    showToast("ok", autostart.value ? "已开启开机自启" : "已关闭开机自启");
  } catch (e) {
    showToast("err", "开机自启设置失败：" + e);
  }
}

async function onSave() {
  try {
    await tracker.saveSettings({
      idleThreshold: idleMin.value * 60,
      deviceName: deviceName.value.trim() || settings.value.device_name,
      dataRetentionDays: retention.value,
    });
    showToast("ok", "设置已保存");
  } catch (e) {
    console.error("保存设置失败", e);
    showToast("err", "保存失败：" + (e instanceof Error ? e.message : String(e)));
  }
}

async function onExport() {
  try {
    const res = await tracker.exportAll();
    showToast("ok", "已导出备份", res.path);
  } catch (e) {
    console.error("导出失败", e);
    showToast("err", "导出失败：" + (e instanceof Error ? e.message : String(e)));
  }
}

// 在系统文件管理器中打开导出文件（macOS 访达 / Windows 资源管理器 / Linux 文件管理器）
async function reveal(path: string) {
  try {
    await tracker.revealPath(path);
  } catch (e) {
    showToast("err", "打开失败：" + (e instanceof Error ? e.message : String(e)));
  }
}

async function copy(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    showToast("ok", "路径已复制");
  } catch {
    showToast("err", "复制失败，路径：" + path);
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
    showToast("ok", `已合并导入 ${n} 条记录`);
  } catch (err) {
    console.error("导入失败", err);
    showToast("err", "导入失败：" + (err instanceof Error ? err.message : String(err)));
  } finally {
    // 允许重复选择同一文件
    (e.target as HTMLInputElement).value = "";
  }
}

async function onPrune() {
  const dev = pruneDeviceId.value.trim();
  const scope = dev ? `设备 ${dev}` : "全部设备";
  if (!confirm(`确定清理 ${retention.value} 天之前【${scope}】的数据吗？此操作不可恢复。`)) return;
  try {
    const n = await tracker.pruneData(retention.value, dev || undefined);
    showToast("ok", `已清理 ${n} 条旧记录（${scope}）`);
  } catch (e) {
    console.error("清理失败", e);
    showToast("err", "清理失败：" + (e instanceof Error ? e.message : String(e)));
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
/* 操作反馈条 */
.toast {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: 10px;
  font-size: 13px;
  animation: slidein 0.2s ease;
}
.toast.ok {
  background: #e8f8ee;
  border: 1px solid #a3e0b8;
  color: #1a7a3c;
}
.toast.err {
  background: #fdecec;
  border: 1px solid #f3b0b0;
  color: #c0392b;
}
.toast-btn {
  border: 1px solid currentColor;
  background: transparent;
  color: inherit;
  padding: 4px 10px;
  border-radius: 7px;
  font-size: 12px;
  cursor: pointer;
}
.toast-close {
  margin-left: auto;
  border: none;
  background: transparent;
  color: inherit;
  font-size: 18px;
  line-height: 1;
  cursor: pointer;
}
@keyframes slidein {
  from {
    opacity: 0;
    transform: translateY(-6px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
.fade-leave-active {
  transition: opacity 0.2s;
}
.fade-leave-to {
  opacity: 0;
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
.danger {
  border: 1px solid #e0a;
  background: transparent;
  color: #d9534f;
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
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
.about b {
  color: var(--text);
  font-weight: 500;
}
.about .mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}
</style>
