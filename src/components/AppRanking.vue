<template>
  <!-- App 使用时长排行榜（按当天时长降序） -->
  <section class="card">
    <h3>App 使用时长排行</h3>
    <ul class="list" v-if="ranking.length">
      <li v-for="(r, i) in ranking" :key="r.app_id">
        <span class="idx">{{ i + 1 }}</span>
        <span class="icon">
          <img v-if="r.icon_base64" :src="`data:image/png;base64,${r.icon_base64}`" alt="" />
          <span v-else class="ph">{{ r.app_name.slice(0, 1) }}</span>
        </span>
        <span class="name">{{ r.app_name }}</span>
        <span
          class="tag"
          :style="{ background: catColor(r.category_id) + '22', color: catColor(r.category_id) }"
          >{{ catName(r.category_id) }}</span
        >
        <span class="dur">{{ formatDuration(r.total_seconds) }}</span>
      </li>
    </ul>
    <p class="empty" v-else>{{ isRange ? "该时间范围内暂无使用记录" : "当天暂无使用记录" }}</p>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { formatDuration } from "../utils/format";
import type { AppRankingOut, CategoryOut } from "../types";

const props = defineProps<{ ranking: AppRankingOut[]; categories: CategoryOut[]; range?: number }>();
const isRange = computed(() => (props.range ?? 0) > 0);

function catColor(id: string): string {
  return props.categories.find((c) => c.id === id)?.color || "#888780";
}
function catName(id: string): string {
  return props.categories.find((c) => c.id === id)?.name || "其他";
}
</script>

<style scoped>
.card {
  padding: 16px 20px;
}
h3 {
  font-size: 14px;
  font-weight: 500;
  margin: 0 0 8px;
}
.list {
  list-style: none;
  margin: 0;
  padding: 0;
}
.list li {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 0;
  border-bottom: 1px solid var(--border);
}
.list li:last-child {
  border-bottom: none;
}
.idx {
  width: 18px;
  color: var(--muted);
  font-size: 13px;
  text-align: center;
}
.icon {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  background: #f0f0f3;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  flex-shrink: 0;
}
.icon img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.icon .ph {
  font-weight: 600;
  font-size: 14px;
  color: var(--muted);
}
.name {
  flex: 1;
  font-size: 14px;
}
.tag {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 6px;
  white-space: nowrap;
}
.dur {
  font-size: 13px;
  font-weight: 500;
  min-width: 84px;
  text-align: right;
}
.empty {
  padding: 24px 0;
  text-align: center;
  color: var(--muted);
  font-size: 13px;
}
</style>
