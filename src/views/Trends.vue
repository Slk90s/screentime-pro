<!--
  src/views/Trends.vue
  趋势对比视图（周/月 环比 + 同比 + 锚点周期每日序列）。
-->
<template>
  <div class="trends">
    <div class="toolbar">
      <div class="seg">
        <button :class="{ active: view === 'usage' }" @click="view = 'usage'">{{ t("keyboard.usageView") }}</button>
        <button :class="{ active: view === 'keyboard' }" @click="view = 'keyboard'">{{ t("keyboard.keyboardView") }}</button>
      </div>
      <div class="seg" :aria-label="t('trends.selectPeriod')">
        <button :class="{ active: period === 'week' }" @click="period = 'week'">{{ t("trends.week") }}</button>
        <button :class="{ active: period === 'month' }" @click="period = 'month'">{{ t("trends.month") }}</button>
      </div>
      <div class="period-nav">
        <button class="icon-btn" :title="t('trends.previousPeriod')" :aria-label="t('trends.previousPeriod')" @click="shiftPeriod(-1)">‹</button>
        <button class="anchor-btn" @click="pickerOpen = true">{{ anchorTitle }}</button>
        <button
          class="icon-btn"
          :title="t('trends.nextPeriod')"
          :aria-label="t('trends.nextPeriod')"
          :disabled="isCurrentPeriod"
          @click="shiftPeriod(1)"
        >›</button>
      </div>
      <DeviceSwitcher v-model="localDevice" :devices="devices" />
    </div>

    <template v-if="view === 'usage'">
    <div class="period-meta">
      <span v-if="data.current_from">{{ data.current_from }} — {{ data.current_to }}</span>
      <span v-if="loading" class="loading">{{ t("common.loading") }}</span>
      <button v-if="loadFailed" class="retry" @click="load">{{ t("trends.retry") }}</button>
    </div>
    <p v-if="loadFailed" class="error" role="alert">{{ t("trends.loadFailed") }}</p>

    <div class="cards" :class="{ updating: loading }">
      <div class="stat">
        <div class="label">{{ t("trends.totalDuration", { label: periodLabel("current") }) }}</div>
        <div class="val">{{ formatDuration(data.current.total_seconds) }}</div>
      </div>
      <div class="stat">
        <div class="label">{{ t("trends.prevSuffix", { label: periodLabel("prev") }) }}</div>
        <div class="val" :class="deltaClass">{{ deltaText }}</div>
        <div class="sub">{{ t("trends.prevTotal", { dur: formatDuration(data.prev.total_seconds) }) }}</div>
      </div>
      <div class="stat" v-if="data.yoy">
        <div class="label">{{ t("trends.yoyLabel") }}</div>
        <div class="val up">{{ t("trends.lastYear", { dur: formatDuration(data.yoy!.total_seconds) }) }}</div>
      </div>
      <div class="stat">
        <div class="label">{{ t("trends.appCount") }}</div>
        <div class="val">{{ data.current.app_count }}</div>
      </div>
    </div>

    <div class="row" :class="{ updating: loading }">
      <section class="card">
        <h3>{{ t("trends.dailyTitle", { label: periodLabel("current") }) }}</h3>
        <div class="wrap">
          <canvas ref="cmpCv"></canvas>
          <div v-if="emptyCurrent && !loading" class="chart-empty">{{ t("trends.emptyChart") }}</div>
        </div>
      </section>
      <section class="card">
        <h3>{{ t("trends.catTitle", { label: periodLabel("current") }) }}</h3>
        <div class="wrap">
          <canvas ref="catCv"></canvas>
          <div v-if="data.current.by_category.length === 0 && !loading" class="chart-empty">{{ t("trends.empty") }}</div>
        </div>
      </section>
    </div>

    <section class="card" :class="{ updating: loading }">
      <h3>{{ t("trends.topTitle", { label: periodLabel("current") }) }}</h3>
      <ul class="apps">
        <li v-for="a in data.current.top_apps" :key="a.app_name">
          <span class="dot" :style="{ background: colorOf(a.category_id) }"></span>
          <span class="name">{{ a.app_name }}</span>
          <span class="cat">{{ nameOf(a.category_id) }}</span>
          <span class="dur">{{ formatDuration(a.total_seconds) }}</span>
        </li>
        <li v-if="data.current.top_apps.length === 0" class="empty">{{ t("trends.empty") }}</li>
      </ul>
    </section>
    </template>

    <template v-else>
      <div class="period-meta">
        <span v-if="keyboardData.current_from">{{ keyboardData.current_from }} — {{ keyboardData.current_to }}</span>
        <span v-if="loading" class="loading">{{ t("common.loading") }}</span>
        <button v-if="loadFailed" class="retry" @click="load">{{ t("trends.retry") }}</button>
      </div>
      <p v-if="loadFailed" class="error" role="alert">{{ t("trends.loadFailed") }}</p>

      <div v-if="keyboardData.status !== 'ready'" class="keyboard-state">
        <p>{{ keyboardStatusText }}</p>
        <button v-if="keyboardData.status !== 'unsupported'" class="open-btn" @click="emit('configure')">{{ t("keyboard.configure") }}</button>
      </div>
      <template v-else>
        <div class="cards" :class="{ updating: loading }">
          <div class="stat">
            <div class="label">{{ t("keyboard.currentTotal") }}</div>
            <div class="val">{{ formatNumber(keyboardData.total_count) }}</div>
          </div>
          <div class="stat">
            <div class="label">{{ t("keyboard.prevTotal") }}</div>
            <div class="val" :class="keyboardDeltaClass">{{ keyboardDeltaText }}</div>
            <div class="sub">{{ formatNumber(keyboardData.prev_total_count) }}</div>
          </div>
        </div>
        <section class="card" :class="{ updating: loading }">
          <h3>{{ t("keyboard.dailyTitle", { label: periodLabel("current") }) }}</h3>
          <div class="wrap">
            <canvas ref="keyCv"></canvas>
            <div v-if="keyboardEmpty && !loading" class="chart-empty">{{ t("keyboard.noInput") }}</div>
          </div>
        </section>
        <section class="card heatmap-card" :class="{ updating: loading }">
          <KeyboardHeatmap :keys="keyboardData.keys" :total="keyboardData.total_count" :loading="loading" />
        </section>
      </template>
    </template>

    <DatePicker
      :visible="pickerOpen"
      :value="pickerValue"
      :mode="period === 'month' ? 'month' : 'date'"
      :show-summary="false"
      @select="selectPeriod"
      @goto-today="goCurrentPeriod"
      @close="pickerOpen = false"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, computed, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import Chart from "chart.js/auto";
import { i18n } from "../i18n";
import { categoryName } from "../i18n/categories";
import DeviceSwitcher from "../components/DeviceSwitcher.vue";
import DatePicker from "../components/DatePicker.vue";
import KeyboardHeatmap from "../components/KeyboardHeatmap.vue";
import { tracker } from "../api/tracker";
import { formatDuration, todayStr } from "../utils/format";
import type { CategoryOut, DeviceInfo, KeyboardStatsOut, PeriodStat, TrendsOut } from "../types";

const { t } = useI18n();
const props = defineProps<{ device: string; devices: DeviceInfo[] }>();
const view = defineModel<"usage" | "keyboard">("view", { default: "usage" });
const emit = defineEmits<{ configure: [] }>();
const localDevice = ref(props.device);
watch(() => props.device, (value) => (localDevice.value = value));

const period = ref<"week" | "month">("week");
const weekAnchor = ref(todayStr());
const monthAnchor = ref(todayStr().slice(0, 7));
const pickerOpen = ref(false);
const categories = ref<CategoryOut[]>([]);
const loading = ref(false);
const loadFailed = ref(false);
let requestGeneration = 0;

function emptyStat(): PeriodStat {
  return { label: "", total_seconds: 0, app_count: 0, by_category: [], top_apps: [], daily: [] };
}
const data = ref<TrendsOut>({
  period: "week",
  anchor: "",
  current_from: "",
  current_to: "",
  prev_from: "",
  prev_to: "",
  current: emptyStat(),
  prev: emptyStat(),
  yoy: null,
  delta_total_pct: 0,
});

const cmpCv = ref<HTMLCanvasElement>();
const catCv = ref<HTMLCanvasElement>();
const keyCv = ref<HTMLCanvasElement>();
let cmpChart: Chart | null = null;
let catChart: Chart | null = null;
let keyChart: Chart | null = null;
const keyboardData = ref<KeyboardStatsOut>({
  status: "disabled",
  anchor: "",
  current_from: "",
  current_to: "",
  prev_from: "",
  prev_to: "",
  total_count: 0,
  prev_total_count: 0,
  delta_pct: 0,
  daily: [],
  keys: [],
});

function mondayOf(dateStr: string): Date {
  const date = new Date(`${dateStr}T00:00:00`);
  date.setDate(date.getDate() - ((date.getDay() + 6) % 7));
  return date;
}
function fmtDate(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}
function currentWeekAnchor(): string {
  return fmtDate(mondayOf(todayStr()));
}
const currentAnchor = computed(() => (period.value === "month" ? monthAnchor.value : weekAnchor.value));
const isCurrentPeriod = computed(() =>
  period.value === "month"
    ? monthAnchor.value === todayStr().slice(0, 7)
    : mondayOf(weekAnchor.value).getTime() === mondayOf(todayStr()).getTime(),
);
const pickerValue = computed(() =>
  period.value === "month" ? `${monthAnchor.value}-01` : weekAnchor.value,
);
function formatAnchor(period: "week" | "month", anchor: string): string {
  const locale = i18n.global.locale.value === "en-US" ? "en-US" : "zh-CN";
  if (period === "month") {
    const [year, month] = anchor.split("-").map(Number);
    return new Intl.DateTimeFormat(locale, { year: "numeric", month: "long" }).format(
      new Date(year, month - 1, 1),
    );
  }
  const start = mondayOf(anchor);
  const end = new Date(start);
  end.setDate(start.getDate() + 6);
  const formatter = new Intl.DateTimeFormat(locale, { month: "short", day: "numeric" });
  return `${formatter.format(start)} – ${formatter.format(end)}`;
}
const anchorTitle = computed(() => formatAnchor(period.value, currentAnchor.value));
// 图表标题跟随已加载数据，而不是正在请求的新周期；失败保留旧图时也不会误标。
const displayPeriod = computed<"week" | "month">(() => {
  if (view.value === "usage") return data.value.anchor ? (data.value.period as "week" | "month") : period.value;
  return keyboardData.value.anchor ? (keyboardData.value.anchor.length === 7 ? "month" : "week") : period.value;
});
const displayAnchor = computed(() => {
  if (view.value === "usage") return data.value.anchor || currentAnchor.value;
  return keyboardData.value.anchor || currentAnchor.value;
});
const displayAnchorTitle = computed(() => formatAnchor(displayPeriod.value, displayAnchor.value));
const displayIsCurrent = computed(() =>
  displayPeriod.value === "month"
    ? displayAnchor.value === todayStr().slice(0, 7)
    : mondayOf(displayAnchor.value).getTime() === mondayOf(todayStr()).getTime(),
);

function periodLabel(role: "current" | "prev"): string {
  if (role === "prev") return t(displayPeriod.value === "week" ? "trends.lastWeek" : "trends.lastMonth");
  if (displayIsCurrent.value) return t(displayPeriod.value === "week" ? "trends.week" : "trends.month");
  return displayAnchorTitle.value;
}

function colorOf(catId: string): string {
  return categories.value.find((category) => category.id === catId)?.color || "#888780";
}
function nameOf(catId: string): string {
  return categoryName(catId);
}

const deltaText = computed(() => {
  const delta = data.value.delta_total_pct;
  return `${delta > 0 ? "+" : ""}${delta.toFixed(1)}%`;
});
const deltaClass = computed(() => (data.value.delta_total_pct >= 0 ? "up" : "down"));
const emptyCurrent = computed(() =>
  data.value.current.daily.every((day) => day.total_seconds === 0),
);
const keyboardEmpty = computed(() => keyboardData.value.daily.every((day) => day.count === 0));
const keyboardDeltaText = computed(() => {
  const delta = keyboardData.value.delta_pct;
  return `${delta > 0 ? "+" : ""}${delta.toFixed(1)}%`;
});
const keyboardDeltaClass = computed(() => (keyboardData.value.delta_pct >= 0 ? "up" : "down"));
const keyboardStatusText = computed(() => {
  if (keyboardData.value.status === "permission_required") return t("keyboard.permissionRequired");
  if (keyboardData.value.status === "unsupported") return t("keyboard.unsupported");
  return t("keyboard.disabled");
});
function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}

function shiftPeriod(direction: number): void {
  if (period.value === "month") {
    const [year, month] = monthAnchor.value.split("-").map(Number);
    const next = new Date(year, month - 1 + direction, 1);
    const value = `${next.getFullYear()}-${String(next.getMonth() + 1).padStart(2, "0")}`;
    if (value > todayStr().slice(0, 7)) return;
    monthAnchor.value = value;
    return;
  }
  const next = mondayOf(weekAnchor.value);
  next.setDate(next.getDate() + direction * 7);
  if (next.getTime() > Date.now()) return;
  weekAnchor.value = fmtDate(next);
}

function selectPeriod(value: string): void {
  if (period.value === "month") monthAnchor.value = value.slice(0, 7);
  else weekAnchor.value = value;
  pickerOpen.value = false;
}
function goCurrentPeriod(): void {
  weekAnchor.value = currentWeekAnchor();
  monthAnchor.value = todayStr().slice(0, 7);
  pickerOpen.value = false;
}

async function load(): Promise<void> {
  const generation = ++requestGeneration;
  const input = Object.freeze({ period: period.value, anchor: currentAnchor.value, device: localDevice.value });
  loading.value = true;
  loadFailed.value = false;
  try {
    if (view.value === "keyboard") {
      const next = await tracker.keyboardStats(input.period, input.anchor, input.device);
      if (generation !== requestGeneration) return;
      keyboardData.value = next;
      await nextTick();
      renderKeyboardChart();
    } else {
      const next = await tracker.trends(input.period, input.anchor, input.device);
      if (generation !== requestGeneration) return;
      data.value = next;
      await nextTick();
      renderCharts();
    }
  } catch {
    if (generation === requestGeneration) loadFailed.value = true;
  } finally {
    if (generation === requestGeneration) loading.value = false;
  }
}

function renderCharts(): void {
  if (!cmpCv.value || !catCv.value) return;
  const daily = data.value.current.daily;
  const labels = daily.map((day) => {
    if (displayPeriod.value === "month") return String(Number(day.date.slice(-2)));
    const locale = i18n.global.locale.value === "en-US" ? "en-US" : "zh-CN";
    return new Intl.DateTimeFormat(locale, { weekday: "short" }).format(new Date(`${day.date}T00:00:00`));
  });
  const values = daily.map((day) => +(day.total_seconds / 3600).toFixed(2));
  const chartData = {
    labels,
    datasets: [{
      label: t("dailyBar.hourUnit"),
      data: values,
      backgroundColor: "#FF7E27",
      borderRadius: 4,
      maxBarThickness: 32,
    }],
  };
  if (cmpChart) {
    cmpChart.data = chartData;
    cmpChart.update();
  } else {
    cmpChart = new Chart(cmpCv.value, {
      type: "bar",
      data: chartData,
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: { duration: 180 },
        plugins: {
          legend: { display: false },
          tooltip: {
            callbacks: {
              title: (items) => daily[items[0]?.dataIndex ?? 0]?.date ?? "",
              label: (context) => `${context.parsed.y} ${t("dailyBar.hourUnit")}`,
            },
          },
        },
        scales: { y: { beginAtZero: true, ticks: { callback: (value) => `${value}${t("duration.hourShort")}` } } },
      },
    });
  }

  const catData = {
    labels: data.value.current.by_category.map((category) => nameOf(category.category_id)),
    datasets: [{
      data: data.value.current.by_category.map((category) => +(category.total_seconds / 3600).toFixed(2)),
      backgroundColor: data.value.current.by_category.map((category) => colorOf(category.category_id)),
      borderWidth: 2,
    }],
  };
  if (catChart) {
    catChart.data = catData;
    catChart.update();
  } else {
    catChart = new Chart(catCv.value, {
      type: "doughnut",
      data: catData,
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: { duration: 180 },
        plugins: {
          legend: { position: "right", labels: { boxWidth: 12, font: { size: 12 } } },
          tooltip: { callbacks: { label: (context) => `${context.label}: ${context.parsed} ${t("dailyBar.hourUnit")}` } },
        },
      },
    });
  }
}

function renderKeyboardChart(): void {
  if (view.value !== "keyboard" || !keyCv.value) return;
  const daily = keyboardData.value.daily;
  const labels = daily.map((day) => {
    if (displayPeriod.value === "month") return String(Number(day.date.slice(-2)));
    const locale = i18n.global.locale.value === "en-US" ? "en-US" : "zh-CN";
    return new Intl.DateTimeFormat(locale, { weekday: "short" }).format(new Date(`${day.date}T00:00:00`));
  });
  const chartData = {
    labels,
    datasets: [{
      data: daily.map((day) => day.count),
      backgroundColor: "#FF7E27",
      borderRadius: 4,
      maxBarThickness: 32,
    }],
  };
  if (keyChart) {
    keyChart.data = chartData;
    keyChart.update();
    return;
  }
  keyChart = new Chart(keyCv.value, {
    type: "bar",
    data: chartData,
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: { duration: 180 },
      plugins: {
        legend: { display: false },
        tooltip: {
          callbacks: {
            title: (items) => daily[items[0]?.dataIndex ?? 0]?.date ?? "",
            label: (context) => `${context.parsed.y}`,
          },
        },
      },
      scales: { y: { beginAtZero: true } },
    },
  });
}

onMounted(async () => {
  try {
    categories.value = await tracker.categories();
  } catch {
    categories.value = [];
  }
  await load();
});
watch(view, (next) => {
  if (next === "keyboard") {
    cmpChart?.destroy();
    cmpChart = null;
    catChart?.destroy();
    catChart = null;
  } else {
    keyChart?.destroy();
    keyChart = null;
  }
});
watch([period, weekAnchor, monthAnchor, localDevice, view], load);
watch(
  () => i18n.global.locale.value,
  () => view.value === "keyboard" ? renderKeyboardChart() : renderCharts(),
);
onBeforeUnmount(() => {
  requestGeneration++;
  cmpChart?.destroy();
  catChart?.destroy();
  keyChart?.destroy();
});
</script>

<style scoped>
.trends { display: flex; flex-direction: column; }
.toolbar { display: flex; align-items: center; gap: 12px; margin-bottom: 8px; flex-wrap: wrap; }
.seg { display: inline-flex; gap: 4px; background: var(--seg-bg); padding: 3px; border-radius: 10px; }
.seg button { border: none; background: transparent; padding: 6px 14px; border-radius: 8px; font-size: 13px; color: var(--muted); cursor: pointer; }
.seg button.active { background: var(--seg-active-bg); color: var(--text); font-weight: 500; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1); }
.period-nav { display: inline-flex; align-items: center; gap: 4px; }
.icon-btn { width: 30px; height: 30px; border: 1px solid var(--border); border-radius: 8px; background: var(--card); color: var(--text); cursor: pointer; font-size: 20px; line-height: 1; }
.icon-btn:disabled { opacity: 0.35; cursor: default; }
.anchor-btn { min-width: 132px; min-height: 30px; padding: 5px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--card); color: var(--text); cursor: pointer; font-size: 13px; }
.anchor-btn:hover, .icon-btn:hover:not(:disabled) { border-color: var(--brand); }
.period-meta { min-height: 22px; display: flex; align-items: center; gap: 10px; margin-bottom: 8px; color: var(--text-dim); font-size: 12px; }
.loading { color: var(--brand); }
.retry { border: none; background: transparent; color: var(--brand); cursor: pointer; padding: 0; }
.error { margin: 0 0 8px; color: var(--danger); font-size: 12px; }
.keyboard-state { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 24px; border: 1px dashed var(--border); border-radius: 12px; color: var(--text-dim); }
.open-btn { border: 1px solid var(--brand); border-radius: 8px; background: transparent; color: var(--brand); padding: 7px 12px; cursor: pointer; }
.heatmap-card { margin-top: 16px; }
.updating { opacity: 0.62; transition: opacity 0.15s ease; }
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; margin-bottom: 16px; }
.stat { background: var(--card); border: 1px solid var(--border); border-radius: 12px; padding: 14px 16px; }
.stat .label { font-size: 12px; color: var(--text-dim); margin-bottom: 8px; }
.stat .val { font-size: 22px; font-weight: 600; color: var(--text); }
.stat .val.up { color: var(--danger); }
.stat .val.down { color: var(--success); }
.stat .sub { font-size: 12px; color: var(--text-dim); margin-top: 4px; }
.row { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-bottom: 16px; }
.card { padding: 16px 20px; background: var(--card); border: 1px solid var(--border); border-radius: 12px; }
h3 { font-size: 14px; font-weight: 500; margin: 0 0 12px; }
.wrap { height: 220px; position: relative; }
.chart-empty { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; color: var(--text-dim); font-size: 13px; pointer-events: none; }
.apps { list-style: none; margin: 0; padding: 0; }
.apps li { display: flex; align-items: center; gap: 10px; padding: 8px 0; border-bottom: 1px solid var(--border); }
.apps li:last-child { border-bottom: none; }
.apps .dot { width: 10px; height: 10px; border-radius: 50%; flex: none; }
.apps .name { font-weight: 500; color: var(--text); }
.apps .cat { font-size: 12px; color: var(--text-dim); }
.apps .dur { margin-left: auto; color: var(--text-dim); }
.apps .empty { color: var(--text-dim); font-size: 13px; justify-content: center; }
@media (max-width: 900px) { .row { grid-template-columns: 1fr; } }
</style>
