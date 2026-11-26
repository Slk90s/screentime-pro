<template>
  <section class="card keyboard-summary" :class="{ disabled: summary.status !== 'ready' }">
    <template v-if="summary.status === 'ready'">
      <div class="summary-main">
        <div class="label">{{ range > 0 ? t("keyboard.rangeTotal", { range }) : t("keyboard.todayTotal") }}</div>
        <div class="value">{{ formatNumber(summary.total_count) }}</div>
        <div class="sub">
          <template v-if="range > 0">{{ t("keyboard.dailyAverage", { n: formatNumber(summary.daily_average) }) }}</template>
          <template v-else-if="summary.top_key">{{ t("keyboard.topKey", { key: topKeyLabel }) }}</template>
          <template v-else>{{ t("keyboard.noInput") }}</template>
        </div>
      </div>
      <button class="open-btn" @click="emit('open')">{{ t("keyboard.viewHeatmap") }} →</button>
    </template>
    <template v-else>
      <div class="summary-main">
        <div class="label">{{ t("keyboard.title") }}</div>
        <div class="status">{{ statusText }}</div>
      </div>
      <button v-if="summary.status !== 'unsupported'" class="open-btn" @click="emit('configure')">
        {{ t("keyboard.configure") }}
      </button>
    </template>
    <span v-if="loading" class="loading">{{ t("common.loading") }}</span>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { KeyboardSummaryOut } from "../types";

const props = withDefaults(defineProps<{
  summary: KeyboardSummaryOut;
  range: number;
  loading?: boolean;
}>(), { loading: false });
const emit = defineEmits<{ open: []; configure: [] }>();
const { t } = useI18n();

const topKeyLabel = computed(() => {
  const id = props.summary.top_key?.key_id;
  if (id === undefined) return "—";
  if (id >= 0x04 && id <= 0x1d) return String.fromCharCode(65 + id - 0x04);
  if (id === 0x2c) return t("keyboard.keys.space");
  if (id === 0x28) return t("keyboard.keys.enter");
  return t("keyboard.keyFallback", { id: `0x${id.toString(16).toUpperCase()}` });
});
const statusText = computed(() => {
  if (props.summary.status === "disabled") return t("keyboard.disabled");
  if (props.summary.status === "permission_required") return t("keyboard.permissionRequired");
  return t("keyboard.unsupported");
});
function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}
</script>

<style scoped>
.keyboard-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  position: relative;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 14px 18px;
  margin-bottom: 16px;
}
.summary-main { min-width: 0; }
.label { color: var(--text-dim); font-size: 12px; margin-bottom: 4px; }
.value { color: var(--text); font-size: 26px; font-weight: 650; line-height: 1.15; font-variant-numeric: tabular-nums; }
.sub { color: var(--text-dim); font-size: 12px; margin-top: 5px; }
.status { color: var(--text); font-size: 14px; margin-top: 6px; }
.open-btn { flex: none; border: 1px solid var(--brand); border-radius: 8px; background: transparent; color: var(--brand); padding: 7px 12px; cursor: pointer; }
.open-btn:hover { background: rgba(255, 126, 39, 0.08); }
.loading { position: absolute; right: 14px; top: 10px; color: var(--brand); font-size: 11px; }
.disabled { opacity: 0.82; }
</style>
