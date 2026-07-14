<template>
  <!-- 总览卡片：单日/范围聚合总使用时长大数字 -->
  <section class="card overview">
    <div class="label">{{ isRange ? `近${range}天累计使用时间` : "设备使用时间" }}</div>
    <div class="big">{{ formatDuration(overview.total_seconds) }}</div>
    <div class="sub">
      {{ isRange ? "累计" : "日均" }} {{ overview.app_count }} 个 App · 最常使用 {{ overview.most_used_app || "—" }}
    </div>
    <div class="stats">
      <div class="stat">
        <span class="num">{{ overview.app_count }}</span>
        <span class="lbl">App 数量</span>
      </div>
      <div class="stat">
        <span class="num">{{ overview.pickup_count }}</span>
        <span class="lbl">切换次数</span>
      </div>
      <div class="stat">
        <span class="num">{{ formatDuration(overview.most_used_seconds) }}</span>
        <span class="lbl">最长单 App</span>
      </div>
      <div class="stat" v-if="isRange">
        <span class="num">{{ formatDuration(overview.avg_daily_seconds || 0) }}</span>
        <span class="lbl">日均时长</span>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { formatDuration } from "../utils/format";
import type { OverviewOut } from "../types";

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
