<template>
  <!-- 总览卡片：单日/范围聚合总使用时长大数字 -->
  <section class="card overview">
    <div class="label">{{ isRange ? t("overview.titleRange", { range }) : t("overview.titleDay") }}</div>
    <div class="big">{{ formatDuration(overview.total_seconds) }}</div>
    <div class="sub">
      {{ isRange ? t("overview.accumulated") : t("overview.dailyAvg") }} {{ overview.app_count }} {{ t("overview.appUnit") }} · {{ t("overview.mostUsed", { app: overview.most_used_app || "—" }) }}
    </div>
    <div class="stats">
      <div class="stat">
        <span class="num">{{ overview.app_count }}</span>
        <span class="lbl">{{ t("overview.appCount") }}</span>
      </div>
      <div class="stat">
        <span class="num">{{ overview.pickup_count }}</span>
        <span class="lbl">{{ t("overview.switchCount") }}</span>
      </div>
      <div class="stat">
        <span class="num">{{ formatDuration(overview.most_used_seconds) }}</span>
        <span class="lbl">{{ t("overview.longestApp") }}</span>
      </div>
      <div class="stat" v-if="isRange">
        <span class="num">{{ formatDuration(overview.avg_daily_seconds || 0) }}</span>
        <span class="lbl">{{ t("overview.avgDuration") }}</span>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { formatDuration } from "../utils/format";
import type { OverviewOut } from "../types";

const { t } = useI18n();
const props = defineProps<{ overview: OverviewOut; range?: number }>();
// range > 0 表示范围聚合模式（今天/近N天），range=0 表示单日
const isRange = computed(() => (props.range ?? 0) > 0);
</script>

<style scoped>
.overview {
  padding: 20px 24px;
}
.label {
  font-size: 13px;
  color: var(--muted);
}
.big {
  font-size: 40px;
  font-weight: 600;
  margin: 6px 0;
  letter-spacing: -0.5px;
}
.sub {
  font-size: 13px;
  color: var(--muted);
  margin-bottom: 16px;
}
.stats {
  display: flex;
  gap: 28px;
}
.stat {
  display: flex;
  flex-direction: column;
}
.num {
  font-size: 20px;
  font-weight: 600;
}
.lbl {
  font-size: 12px;
  color: var(--muted);
}
</style>
