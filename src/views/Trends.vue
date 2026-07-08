<template>
  <!-- 趋势对比：周/月 的本期 vs 上期（环比）与去年同期（同比） -->
  <div class="trends">
    <div class="toolbar">
      <!-- 周期切换：本周 / 本月 -->
      <div class="seg">
        <button :class="{ active: period === 'week' }" @click="period = 'week'">本周</button>
        <button :class="{ active: period === 'month' }" @click="period = 'month'">本月</button>
      </div>
      <!-- 设备切换（多设备合并） -->
      <DeviceSwitcher v-model="localDevice" :devices="devices" />
    </div>

    <!-- 概览数字卡 -->
    <div class="cards">
      <div class="stat">
        <div class="label">{{ data.current.label }} 总时长</div>
        <div class="val">{{ formatDuration(data.current.total_seconds) }}</div>
      </div>
      <div class="stat">
        <div class="label">环比（{{ data.prev.label }}）</div>
        <div class="val" :class="deltaClass">{{ deltaText }}</div>
        <div class="sub">上期 {{ formatDuration(data.prev.total_seconds) }}</div>
      </div>
      <div class="stat" v-if="data.yoy">
        <div class="label">同比（去年同期）</div>
        <div class="val up">去年 {{ formatDuration(data.yoy!.total_seconds) }}</div>
      </div>
      <div class="stat">
        <div class="label">使用应用数</div>
        <div class="val">{{ data.current.app_count }}</div>
      </div>
    </div>

    <div class="row">
      <!-- 时长对比柱状图 -->
      <section class="card">
        <h3>时长对比（小时）</h3>
        <div class="wrap"><canvas ref="cmpCv"></canvas></div>
      </section>
      <!-- 本期分类占比环形图 -->
      <section class="card">
        <h3>{{ data.current.label }} 分类占比</h3>
        <div class="wrap"><canvas ref="catCv"></canvas></div>
      </section>
    </div>

    <!-- 本期 Top 应用 -->
    <section class="card">
      <h3>{{ data.current.label }} 使用时长 Top 应用</h3>
      <ul class="apps">
        <li v-for="a in data.current.top_apps" :key="a.app_name">
          <span class="dot" :style="{ background: colorOf(a.category_id) }"></span>
          <span class="name">{{ a.app_name }}</span>
          <span class="cat">{{ nameOf(a.category_id) }}</span>
          <span class="dur">{{ formatDuration(a.total_seconds) }}</span>
        </li>
        <li v-if="data.current.top_apps.length === 0" class="empty">该周期暂无数据</li>
      </ul>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, computed } from "vue";
import Chart from "chart.js/auto";
import DeviceSwitcher from "../components/DeviceSwitcher.vue";
import { tracker } from "../api/tracker";
import { formatDuration } from "../utils/format";
import type { CategoryOut, DeviceInfo, PeriodStat, TrendsOut } from "../types";

// 父组件传入：当前设备（"" = 全部）与设备列表
const props = defineProps<{ device: string; devices: DeviceInfo[] }>();

// 趋势页自身维护一份设备选择（与父级保持同步，但可独立切换）
const localDevice = ref(props.device);
watch(
  () => props.device,
  (v) => (localDevice.value = v),
);

const period = ref<"week" | "month">("week");
const categories = ref<CategoryOut[]>([]);

// 默认空数据，避免首屏空指针
function emptyStat(label: string): PeriodStat {
  return { label, total_seconds: 0, app_count: 0, by_category: [], top_apps: [] };
}
const data = ref<TrendsOut>({
  period: "week",
  current: emptyStat("本周"),
  prev: emptyStat("上周"),
  yoy: null,
  delta_total_pct: 0,
});

const cmpCv = ref<HTMLCanvasElement>();
const catCv = ref<HTMLCanvasElement>();
let cmpChart: Chart | null = null;
let catChart: Chart | null = null;

// 分类颜色 / 名称 查表
function colorOf(catId: string): string {
  return categories.value.find((c) => c.id === catId)?.color || "#888780";
}
function nameOf(catId: string): string {
  return categories.value.find((c) => c.id === catId)?.name || catId;
}

// 环比百分比展示
const deltaText = computed(() => {
  const d = data.value.delta_total_pct;
  const sign = d > 0 ? "+" : "";
  return `${sign}${d.toFixed(1)}%`;
});
const deltaClass = computed(() => (data.value.delta_total_pct >= 0 ? "up" : "down"));

async function load() {
  categories.value = await tracker.categories();
  data.value = await tracker.trends(period.value, localDevice.value);
  renderCharts();
}

function renderCharts() {
  if (!cmpCv.value || !catCv.value) return;
  // 1) 时长对比：本期 / 上期 / 去年同期
  cmpChart?.destroy();
  const labels = [data.value.current.label, data.value.prev.label];
  const values = [
    +(data.value.current.total_seconds / 3600).toFixed(1),
    +(data.value.prev.total_seconds / 3600).toFixed(1),
  ];
  const colors = ["#FF7E27", "#9aa0a6"];
  if (data.value.yoy) {
    labels.push(data.value.yoy.label);
    values.push(+(data.value.yoy.total_seconds / 3600).toFixed(1));
    colors.push("#378ADD");
  }
  cmpChart = new Chart(cmpCv.value, {
    type: "bar",
    data: {
      labels,
      datasets: [{ label: "小时", data: values, backgroundColor: colors, borderRadius: 6, maxBarThickness: 48 }],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: { legend: { display: false }, tooltip: { callbacks: { label: (c) => `${c.parsed.y} 小时` } } },
      scales: { y: { beginAtZero: true, ticks: { callback: (v) => `${v}h` } } },
    },
  });

  // 2) 本期分类占比环形图
  catChart?.destroy();
  const catLabels = data.value.current.by_category.map((c) => nameOf(c.category_id));
  const catData = data.value.current.by_category.map((c) => +(c.total_seconds / 3600).toFixed(1));
  const catColors = data.value.current.by_category.map((c) => colorOf(c.category_id));
  catChart = new Chart(catCv.value, {
    type: "doughnut",
    data: { labels: catLabels, datasets: [{ data: catData, backgroundColor: catColors, borderWidth: 2 }] },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { position: "right", labels: { boxWidth: 12, font: { size: 12 } } },
        tooltip: { callbacks: { label: (c) => `${c.label}: ${c.parsed} 小时` } },
      },
    },
  });
}

onMounted(load);
// 周期或设备变化时重新拉取
watch(period, load);
watch(localDevice, load);
onBeforeUnmount(() => {
  cmpChart?.destroy();
  catChart?.destroy();
});
</script>

<style scoped>
.trends {
  display: flex;
  flex-direction: column;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
  flex-wrap: wrap;
}
.seg {
  display: inline-flex;
  gap: 4px;
  background: #f0f0f3;
  padding: 3px;
  border-radius: 10px;
}
.seg button {
  border: none;
  background: transparent;
  padding: 6px 14px;
  border-radius: 8px;
  font-size: 13px;
  color: var(--muted);
  cursor: pointer;
}
.seg button.active {
  background: #fff;
  color: var(--text);
  font-weight: 500;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}
/* 概览数字卡 */
.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 12px;
  margin-bottom: 16px;
}
.stat {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 14px 16px;
}
.stat .label {
  font-size: 12px;
  color: var(--text-dim);
  margin-bottom: 8px;
}
.stat .val {
  font-size: 22px;
  font-weight: 600;
  color: var(--text);
}
.stat .val.up {
  color: #d9534f; /* 用时增加偏红（提醒） */
}
.stat .val.down {
  color: #2e9e5b; /* 用时减少偏绿（好事） */
}
.stat .sub {
  font-size: 12px;
  color: var(--text-dim);
  margin-top: 4px;
}
.row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  margin-bottom: 16px;
}
.card {
  padding: 16px 20px;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 12px;
}
h3 {
  font-size: 14px;
  font-weight: 500;
  margin: 0 0 12px;
}
.wrap {
  height: 220px;
  position: relative;
}
/* Top 应用列表 */
.apps {
  list-style: none;
  margin: 0;
  padding: 0;
}
.apps li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 0;
  border-bottom: 1px solid var(--border);
}
.apps li:last-child {
  border-bottom: none;
}
.apps .dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex: none;
}
.apps .name {
  font-weight: 500;
  color: var(--text);
}
.apps .cat {
  font-size: 12px;
  color: var(--text-dim);
}
.apps .dur {
  margin-left: auto;
  color: var(--text-dim);
}
.apps .empty {
  color: var(--text-dim);
  font-size: 13px;
  justify-content: center;
}
</style>
