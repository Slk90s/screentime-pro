<template>
  <!-- iOS 风格「按天」堆叠柱状图：每天一根柱，按分类着色堆叠；附周几标签与日均参考线 -->
  <section class="card">
    <div class="head">
      <h3>近 {{ summaries.length }} 天每日使用时长</h3>
      <span class="avg">日均 {{ formatDuration(avgSeconds) }}</span>
    </div>
    <div class="wrap"><canvas ref="cv"></canvas></div>
    <div class="legend" v-if="categories.length">
      <span v-for="c in categories" :key="c.id" class="lg">
        <i :style="{ background: c.color }"></i>{{ c.name }}
      </span>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import Chart from "chart.js/auto";
import type { CategoryOut, DailySummaryOut, DayCategoryOut } from "../types";
import { formatDuration, formatHours } from "../utils/format";

const props = defineProps<{
  // 每天的总时长与日期（用于标签、日均线、排序）
  summaries: DailySummaryOut[];
  // 每天 × 分类的明细（用于堆叠）
  byCategory: DayCategoryOut[];
  // 分类字典（提供颜色与名称）
  categories: CategoryOut[];
}>();

// 点击某天柱子：把日期回传给父组件（用于联动下方「当天详情」）
const emit = defineEmits<{ select: [date: string] }>();

const cv = ref<HTMLCanvasElement>();
let chart: Chart | null = null;

const WEEK = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

// 由 YYYY-MM-DD 求周几 + M/D 短标签
function dayLabel(date: string): string {
  const d = new Date(date + "T00:00:00");
  if (isNaN(d.getTime())) return date.slice(5);
  const md = `${d.getMonth() + 1}/${d.getDate()}`;
  return `${WEEK[d.getDay()]}\n${md}`;
}

// 计算日均（秒）
const avgSeconds = ref(0);

function render() {
  if (!cv.value) return;
  chart?.destroy();

  // 按日期聚合分类 -> 秒数
  const byDate = new Map<string, Record<string, number>>();
  for (const s of props.summaries) byDate.set(s.date, {});
  for (const dc of props.byCategory) {
    const m = byDate.get(dc.date) ?? {};
    m[dc.category_id] = (m[dc.category_id] ?? 0) + dc.total_seconds;
    byDate.set(dc.date, m);
  }

  // 日期升序（旧 -> 新），与 iOS 一致
  const dates = [...props.summaries.map((s) => s.date)].reverse();
  const labels = dates.map(dayLabel);

  // 构建每个分类一个 dataset（堆叠）
  const orderedCats = props.categories.length
    ? props.categories
    : [...new Set(props.byCategory.map((d) => d.category_id))].map((id) => ({
        id,
        name: id,
        color: "#888780",
      }));

  const datasets = orderedCats.map((c) => ({
    label: c.name,
    // 堆叠数据：该分类在每天的总秒数（缺失为 0）
    data: dates.map((d) => {
      const m = byDate.get(d);
      return m ? +(m[c.id] ?? 0) / 3600 : 0;
    }),
    backgroundColor: c.color,
    stack: "usage",
    borderRadius: 3,
    maxBarThickness: 34,
  }));

  // 日均参考线（总时长均值，小时）
  const totals = dates.map((d) => {
    const m = byDate.get(d);
    if (!m) return 0;
    return Object.values(m).reduce((a, b) => a + b, 0) / 3600;
  });
  const avgH =
    totals.length > 0
      ? totals.reduce((a, b) => a + b, 0) / totals.length
      : 0;
  avgSeconds.value = Math.round(avgH * 3600);

  chart = new Chart(cv.value, {
    type: "bar",
    data: { labels, datasets },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      interaction: { mode: "index", intersect: false },
      // 点击某天柱子 -> 回传该天日期，供父组件联动下方「当天详情」
      onClick: (_evt, elements) => {
        if (elements.length > 0) emit("select", dates[elements[0].index]);
      },
      plugins: {
        legend: { display: false },
        tooltip: {
          callbacks: {
            // 多分类堆叠时汇总显示总时长
            title: (items) => {
              const i = items[0].dataIndex;
              return dates[i];
            },
            label: (c) => `${c.dataset.label}：${formatHours((c.parsed.y ?? 0) * 3600)} 小时`,
            footer: (items) => {
              const tot = items.reduce((a, b) => a + (b.parsed.y ?? 0), 0);
              return `合计：${formatHours(tot * 3600)} 小时`;
            },
          },
        },
      },
      scales: {
        x: {
          stacked: true,
          ticks: { font: { size: 11 }, autoSkip: false, maxRotation: 0 },
          grid: { display: false },
        },
        y: {
          stacked: true,
          beginAtZero: true,
          ticks: { callback: (v) => `${v}h` },
          grid: { color: "rgba(0,0,0,0.05)" },
        },
      },
    },
  });
}

onMounted(render);
watch(() => [props.summaries, props.byCategory, props.categories], render, {
  deep: true,
});
onBeforeUnmount(() => chart?.destroy());
</script>

<style scoped>
.card {
  padding: 16px 20px;
}
.head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-bottom: 12px;
}
h3 {
  font-size: 14px;
  font-weight: 500;
  margin: 0;
}
.avg {
  font-size: 12px;
  color: var(--muted);
}
.wrap {
  height: 220px;
  position: relative;
}
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 12px;
}
.lg {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  color: var(--muted);
}
.lg i {
  width: 10px;
  height: 10px;
  border-radius: 3px;
  display: inline-block;
}
</style>
