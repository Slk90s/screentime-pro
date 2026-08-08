<template>
  <!-- 纯 CSS 日历选择器，无需第三方依赖 -->
  <div class="date-picker-overlay" v-if="visible" @click.self="$emit('close')">
    <div class="date-picker">
      <!-- 头部：上月 / 年月 / 下月 -->
      <header class="dp-header">
        <button @click="prevMonth" class="dp-nav">&lt;</button>
        <span class="dp-title">{{ dpTitle }}</span>
        <button @click="nextMonth" class="dp-nav">&gt;</button>
      </header>

      <!-- 星期标题行 -->
      <div class="dp-weekdays">
        <span v-for="w in weekdays" :key="w">{{ w }}</span>
      </div>

      <!-- 日期网格 -->
      <div class="dp-grid">
        <button
          v-for="(d, i) in days"
          :key="i"
          :class="{
            'dp-day': true,
            'dp-other': !d.current,
            'dp-today': d.isToday,
            'dp-selected': d.selected,
            'dp-disabled': d.disabled || !d.current,
            'dp-empty': d.empty
          }"
          :disabled="d.disabled || d.empty"
          @click="!d.disabled && !d.empty && $emit('select', d.value)"
        >{{ d.label }}</button>
      </div>

      <!-- 本月统计概括（v0.7.0：月视图切换时显示该月汇总） -->
      <section class="dp-summary" v-if="visible">
        <div class="dp-summary-title">
          <AppIcon name="clipboard" :size="13" />
          {{ t("dashboard.calendar.monthSummary") }}
        </div>
        <div v-if="summaryLoading" class="dp-summary-hint">{{ t("common.loading") }}</div>
        <div v-else-if="!monthSummary || monthSummary.total_seconds === 0" class="dp-summary-hint">
          {{ t("dashboard.calendar.noData") }}
        </div>
        <template v-else>
          <div class="dp-summary-grid">
            <div class="dp-stat">
              <span class="dp-stat-label">{{ t("dashboard.calendar.total") }}</span>
              <span class="dp-stat-value">{{ fmtDur(monthSummary.total_seconds) }}</span>
            </div>
            <div class="dp-stat">
              <span class="dp-stat-label">{{ t("dashboard.calendar.activeDays") }}</span>
              <span class="dp-stat-value">{{ monthSummary.active_days }}/{{ monthSummary.days_in_month }}</span>
            </div>
            <div class="dp-stat">
              <span class="dp-stat-label">{{ t("dashboard.calendar.avg") }}</span>
              <span class="dp-stat-value">{{ fmtDur(monthSummary.avg_daily_seconds) }}</span>
            </div>
          </div>
          <div class="dp-summary-foot">
            <span v-if="monthSummary.top_app">
              {{ t("dashboard.calendar.topApp") }}：<b>{{ monthSummary.top_app }}</b>
            </span>
            <span v-if="monthSummary.busiest_date">
              {{ t("dashboard.calendar.busiest") }}：<b>{{ monthSummary.busiest_date }}</b>
            </span>
          </div>
        </template>
      </section>

      <!-- 快捷操作：今天（v0.7.0：点击后日历视图同步跳转到今天所在月份） -->
      <footer class="dp-footer">
        <button @click="goToday">{{ t("datePicker.today") }}</button>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * DatePicker — 轻量日历选择器
 *
 * 设计要点：
 * - 零依赖：纯 Vue + CSS 实现，不引入 date-fns 或其他库
 * - 输出格式：YYYY-MM-DD（与后端 DB date 字段对齐）
 * - 禁用未来日期（不能查看未来的使用记录）
 * - 高亮「今天」和「已选」日期
 * - v0.7.0：① 切换月份时显示该月统计概括（月总时长/活跃天数/日均/最常用 App/最忙一天）；
 *           ② 打开日历或外部切到某日期时，视图自动跳到对应月份；点「今天」视图同步跳到今天所在月。
 */

import { ref, computed, watch, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { i18n } from "../i18n";
import { tracker } from "../api/tracker";
import { formatDuration } from "../utils/format";
import type { MonthSummaryOut } from "../types";
import AppIcon from "./AppIcon.vue";

const { t } = useI18n();

const props = defineProps<{
  visible: boolean;
  value?: string; // 当前选中值 YYYY-MM-DD
}>();

const emit = defineEmits<{
  select: [dateStr: string];
  close: [];
  /** v0.7.0：点「今天」——同步跳转视图到今天所在月，但不关闭选择器 */
  "goto-today": [];
}>();

// 星期标题（按当前语言本地化；2023-01-01 为周日）
const weekdays = computed(() => {
  const localeTag = i18n.global.locale.value === "en-US" ? "en-US" : "zh-CN";
  const fmt = new Intl.DateTimeFormat(localeTag, { weekday: "short" });
  return Array.from({ length: 7 }, (_, i) => fmt.format(new Date(2023, 0, 1 + i)));
});

// 当前显示的年/月（可切换）
const viewYear = ref(new Date().getFullYear());
const viewMonth = ref(new Date().getMonth()); // 0-indexed

// 标题（年 + 月，按当前语言本地化）
const dpTitle = computed(() => {
  const localeTag = i18n.global.locale.value === "en-US" ? "en-US" : "zh-CN";
  return new Intl.DateTimeFormat(localeTag, { year: "numeric", month: "long" }).format(
    new Date(viewYear.value, viewMonth.value, 1),
  );
});

// 今天字符串（YYYY-MM-DD）
function todayStr(): string {
  const d = new Date();
  return fmt(d.getFullYear(), d.getMonth() + 1, d.getDate());
}

function fmt(y: number, m: number, day: number): string {
  return `${y}-${String(m).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

// 解析 YYYY-MM-DD → {y, m(0-indexed)}
function parseYM(dateStr?: string): { y: number; m: number } | null {
  if (!dateStr) return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateStr);
  if (!m) return null;
  return { y: Number(m[1]), m: Number(m[2]) - 1 };
}

// 判断是否是未来日期（不可选）
function isFuture(y: number, m: number, day: number): boolean {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const target = new Date(y, m - 1, day); // JS month 是 0-indexed
  return target.getTime() > today.getTime();
}

interface DayCell {
  label: string;
  value: string;
  current: boolean; // 属于当前月份
  empty: boolean;
  isToday: boolean;
  selected: boolean;
  disabled: boolean;
}

// 生成当前视图的 6x7 日历网格
const days = computed<DayCell[]>(() => {
  const y = viewYear.value;
  const m = viewMonth.value;
  // 当月第一天
  const first = new Date(y, m, 1);
  // 当月天数
  const daysInMonth = new Date(y, m + 1, 0).getDate();
  // 上月天数
  const prevDays = new Date(y, m, 0).getDate();
  // 第一天星期几（0=周日）
  let startWeekday = first.getDay();

  const result: DayCell[] = [];
  const sel = props.value || "";
  const today = todayStr();

  // 填充上月末尾
  for (let i = startWeekday - 1; i >= 0; i--) {
    const day = prevDays - i;
    result.push({
      label: String(day),
      value: fmt(y, m, day),
      current: false,
      empty: false,
      isToday: false,
      selected: false,
      disabled: true,
    });
  }

  // 当月
  for (let d = 1; d <= daysInMonth; d++) {
    const val = fmt(y, m + 1, d);
    result.push({
      label: String(d),
      value: val,
      current: true,
      empty: false,
      isToday: val === today,
      selected: val === sel,
      disabled: isFuture(y, m + 1, d),
    });
  }

  // 补齐到整行（42 格 = 6 行 x 7 列）
  while (result.length < 42) {
    const extra = result.length - startWeekday - daysInMonth + 1;
    result.push({
      label: String(extra),
      value: "",
      current: false,
      empty: true,
      isToday: false,
      selected: false,
      disabled: true,
    });
  }

  return result;
});

function prevMonth() {
  if (viewMonth.value === 0) {
    viewMonth.value = 11;
    viewYear.value--;
  } else {
    viewMonth.value--;
  }
}

function nextMonth() {
  if (viewMonth.value === 11) {
    viewMonth.value = 0;
    viewYear.value++;
  } else {
    viewMonth.value++;
  }
}

// v0.7.0：点「今天」——视图跳到今天所在月，并把今天作为选中项，但不关闭选择器
function goToday(): void {
  const now = new Date();
  viewYear.value = now.getFullYear();
  viewMonth.value = now.getMonth();
  emit("goto-today");
}

// ---- 本月统计概括（v0.7.0） ----
const monthSummary = ref<MonthSummaryOut | null>(null);
const summaryLoading = ref(false);

async function loadMonthSummary(): Promise<void> {
  summaryLoading.value = true;
  try {
    monthSummary.value = await tracker.monthSummary(
      viewYear.value,
      viewMonth.value + 1,
    );
  } catch {
    monthSummary.value = null;
  } finally {
    summaryLoading.value = false;
  }
}

function fmtDur(sec: number): string {
  return formatDuration(sec);
}

// 视图月份变化时刷新统计概括
watch([viewYear, viewMonth], () => {
  void loadMonthSummary();
});

// v0.7.0：打开日历 / 外部选中日期变化时，视图同步跳到对应月份
// （覆盖「回到今天」「点柱状图选历史日期」等场景，让所选日期始终在可视月内）
function syncViewToValue(): void {
  const ym = parseYM(props.value);
  if (ym) {
    viewYear.value = ym.y;
    viewMonth.value = ym.m;
  }
}
watch(
  () => props.value,
  () => syncViewToValue(),
);
watch(
  () => props.visible,
  (v) => {
    if (v) syncViewToValue();
  },
);

onMounted(() => {
  syncViewToValue();
  void loadMonthSummary();
});
</script>

<style scoped>
/* 遮罩层 */
.date-picker-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.25);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
}
/* 日历主体 */
.date-picker {
  background: var(--card, #fff);
  border-radius: 14px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.15);
  padding: 16px;
  width: 300px;
  user-select: none;
}
.dp-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}
.dp-title {
  font-weight: 600;
  font-size: 15px;
  color: var(--text, #333);
}
.dp-nav {
  width: 30px;
  height: 30px;
  border: none;
  background: #f0f0f5;
  border-radius: 8px;
  font-size: 16px;
  cursor: pointer;
  color: #666;
}
.dp-nav:hover { background: #e0e0ea; }

.dp-weekdays {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  text-align: center;
  font-size: 12px;
  color: #999;
  margin-bottom: 4px;
}
.dp-grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 2px;
}
.dp-day {
  width: 34px;
  height: 34px;
  border: none;
  background: transparent;
  border-radius: 8px;
  font-size: 13px;
  color: var(--text, #333);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-variant-numeric: tabular-nums;
}
.dp-day:hover:not(.dp-disabled):not(.dp-empty):not(.dp-other) {
  background: rgba(255, 126, 39, 0.08);
}
.dp-day.dp-selected:not(.dp-disabled) {
  background: var(--brand, #ff7e27) !important;
  color: #fff !important;
  font-weight: 600;
}
.dp-day.dp-today:not(.dp-selected) {
  border: 1.5px solid var(--brand, #ff7e27);
  font-weight: 600;
}
.dp-day.dp-other,
.dp-day.dp-empty {
  visibility: hidden; /* 不属于当月的隐藏 */
}
.dp-day.dp-disabled {
  color: #ccc;
  cursor: default;
}

/* v0.7.0：本月统计概括 */
.dp-summary {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px dashed rgba(0, 0, 0, 0.08);
}
.dp-summary-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text, #333);
  margin-bottom: 8px;
}
.dp-summary-title :deep(svg) {
  color: var(--brand, #ff7e27);
}
.dp-summary-hint {
  font-size: 12px;
  color: var(--text-dim, #999);
  padding: 4px 0;
}
.dp-summary-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
}
.dp-stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  background: var(--bg, #f7f7f9);
  border-radius: 8px;
  padding: 6px 8px;
}
.dp-stat-label {
  font-size: 10.5px;
  color: var(--text-dim, #999);
}
.dp-stat-value {
  font-size: 13px;
  font-weight: 600;
  color: var(--text, #333);
  font-variant-numeric: tabular-nums;
}
.dp-summary-foot {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin-top: 8px;
  font-size: 11.5px;
  color: var(--text-dim, #888);
}
.dp-summary-foot b {
  color: var(--brand, #ff7e27);
  font-weight: 600;
}

.dp-footer {
  margin-top: 10px;
  text-align: right;
}
.dp-footer button {
  border: 1px solid var(--brand, #ff7e27);
  color: var(--brand, #ff7e27);
  background: transparent;
  padding: 5px 16px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
}
.dp-footer button:hover {
  background: rgba(255, 126, 39, 0.06);
}
</style>
