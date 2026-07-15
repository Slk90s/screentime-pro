<template>
  <!-- 24 小时分布堆叠图（按分类堆叠，Chart.js） -->
  <section class="card">
    <h3>{{ t("hourly.title") }}</h3>
    <div class="wrap"><canvas ref="cv"></canvas></div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import Chart from "chart.js/auto";
import { i18n } from "../i18n";
import { categoryName } from "../i18n/categories";
import type { CategoryOut, HourlyBucketOut } from "../types";

const { t } = useI18n();
const props = defineProps<{ buckets: HourlyBucketOut[]; categories: CategoryOut[] }>();
const cv = ref<HTMLCanvasElement>();
let chart: Chart | null = null;

function render() {
  if (!cv.value) return;
  chart?.destroy();
  const hours = Array.from({ length: 24 }, (_, i) => i);
  const labels = hours.map((h) => String(h));
  const datasets = props.categories.map((cat) => ({
    label: categoryName(cat.id),
    data: hours.map((h) => {
      const b = props.buckets.find((x) => x.hour === h && x.category_id === cat.id);
      return b ? +(b.total_seconds / 3600).toFixed(2) : 0;
    }),
    backgroundColor: cat.color,
    stack: "s",
  }));
  chart = new Chart(cv.value, {
    type: "bar",
    data: { labels, datasets },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: {
          position: "bottom",
          labels: { boxWidth: 12, font: { size: 11 } },
        },
      },
      scales: {
        x: { stacked: true },
        y: { stacked: true, beginAtZero: true, ticks: { callback: (v) => `${v}${t("duration.hourShort")}` } },
      },
    },
  });
}

onMounted(render);
watch(
  () => [props.buckets, props.categories],
  render
);
// 语言切换时重绘（Chart.js 不响应响应式 locale）
watch(
  () => i18n.global.locale.value,
  render
);
onBeforeUnmount(() => chart?.destroy());
</script>

<style scoped>
.card {
  padding: 16px 20px;
}
h3 {
  font-size: 14px;
  font-weight: 500;
  margin: 0 0 12px;
}
.wrap {
  height: 200px;
  position: relative;
}
</style>
