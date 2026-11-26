<template>
  <section class="keyboard-heatmap">
    <div class="heatmap-head">
      <div>
        <span class="total-label">{{ t("keyboard.total") }}</span>
        <strong>{{ formatNumber(total) }}</strong>
      </div>
      <div class="legend">
        <span>{{ t("keyboard.less") }}</span><i class="gradient" /><span>{{ t("keyboard.more") }}</span>
      </div>
    </div>

    <div class="keyboard-stage" :class="{ loading }">
      <svg
        class="keyboard-svg"
        :viewBox="`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`"
        role="img"
        :aria-label="t('keyboard.heatmapAria')"
        tabindex="0"
        @keydown="onKeyNavigation"
        @mouseleave="hoveredIndex = null"
      >
        <g v-for="(key, index) in layout" :key="`${key.id}-${index}`">
          <rect
            :x="key.x"
            :y="key.y"
            :width="key.width"
            :height="key.height"
            rx="6"
            :fill="keyFill(key.id)"
            :stroke="focusedIndex === index || hoveredIndex === index ? '#FF7E27' : 'var(--border)'"
            :stroke-width="focusedIndex === index || hoveredIndex === index ? 2 : 1"
            @mouseenter="hoveredIndex = index"
          />
          <text
            :x="key.x + key.width / 2"
            :y="key.y + key.height / 2 + 4"
            text-anchor="middle"
            :fill="keyTextColor(key.id)"
            :font-size="key.label.length > 6 ? 9 : 11"
            pointer-events="none"
          >{{ key.label }}</text>
        </g>
      </svg>
      <div v-if="loading" class="loading-mask">{{ t("common.loading") }}</div>
    </div>

    <div class="heatmap-footer">
      <div class="key-inspector" aria-live="polite">
        <template v-if="inspected">
          <b>{{ keyName(inspected) }}</b>
          <span>{{ formatNumber(countOf(inspected.id)) }} · {{ percentage(inspected.id) }}</span>
        </template>
        <span v-else class="hint">{{ t("keyboard.inspectHint") }}</span>
      </div>
      <div v-if="isEmpty && !loading" class="empty">{{ t("keyboard.noInput") }}</div>
    </div>

    <div class="top-keys">
      <h4>{{ t("keyboard.topKeys") }}</h4>
      <ol>
        <li v-for="key in topKeys" :key="key.id">
          <span>{{ keyName(key) }}</span><b>{{ formatNumber(key.count) }}</b>
        </li>
        <li v-if="topKeys.length === 0" class="empty">{{ t("keyboard.noInput") }}</li>
      </ol>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { KeyCountOut } from "../types";

const props = withDefaults(defineProps<{ keys: KeyCountOut[]; total: number; loading?: boolean }>(), {
  loading: false,
});
const { t } = useI18n();

type KeySpec = { id: number; label: string; w?: number; h?: number };
type PositionedKey = KeySpec & { x: number; y: number; width: number; height: number };
const UNIT = 52;
const VIEW_WIDTH = 23 * UNIT;
const VIEW_HEIGHT = 340;

function row(entries: KeySpec[], y: number, startX = 0): PositionedKey[] {
  let cursor = startX;
  return entries.map((entry) => {
    const width = (entry.w ?? 1) * UNIT;
    const key = {
      ...entry,
      x: cursor * UNIT + 2,
      y: y * UNIT + 2,
      width: width - 4,
      height: (entry.h ?? 1) * UNIT - 4,
    };
    cursor += entry.w ?? 1;
    return key;
  });
}
function letters(chars: string): KeySpec[] {
  return [...chars].map((label) => ({ id: 0x04 + label.charCodeAt(0) - 65, label }));
}
const digits = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"].map((label, index) => ({
  id: 0x1e + index,
  label,
}));
const functionKeys = Array.from({ length: 12 }, (_, index) => ({ id: 0x3a + index, label: `F${index + 1}` }));

const layout: PositionedKey[] = [
  ...row([{ id: 0x29, label: "Esc" }], 0),
  ...row(functionKeys, 0, 2),
  ...row([
    { id: 0x46, label: "PrtSc" }, { id: 0x47, label: "ScrLk" }, { id: 0x48, label: "Pause" },
  ], 0, 16),
  ...row([
    { id: 0x35, label: "`" }, ...digits, { id: 0x2d, label: "-" }, { id: 0x2e, label: "=" },
    { id: 0x2a, label: "Backspace", w: 2 },
  ], 1.4),
  ...row([
    { id: 0x2b, label: "Tab", w: 1.5 }, ...letters("QWERTYUIOP"),
    { id: 0x2f, label: "[" }, { id: 0x30, label: "]" }, { id: 0x31, label: "\\", w: 1.5 },
  ], 2.4),
  ...row([
    { id: 0x39, label: "Caps", w: 1.75 }, ...letters("ASDFGHJKL"),
    { id: 0x33, label: ";" }, { id: 0x34, label: "'" }, { id: 0x28, label: "Enter", w: 2.25 },
  ], 3.4),
  ...row([
    { id: 0xe1, label: "Shift", w: 2.25 }, ...letters("ZXCVBNM"),
    { id: 0x36, label: "," }, { id: 0x37, label: "." }, { id: 0x38, label: "/" },
    { id: 0xe5, label: "Shift", w: 2.75 },
  ], 4.4),
  ...row([
    { id: 0xe0, label: "Ctrl", w: 1.25 }, { id: 0xe3, label: "Win", w: 1.25 },
    { id: 0xe2, label: "Alt", w: 1.25 }, { id: 0x2c, label: "Space", w: 6.25 },
    { id: 0xe6, label: "Alt", w: 1.25 }, { id: 0xe7, label: "Win", w: 1.25 },
    { id: 0x65, label: "Menu", w: 1.25 }, { id: 0xe4, label: "Ctrl", w: 1.25 },
  ], 5.4),
  ...row([
    { id: 0x49, label: "Ins" }, { id: 0x4a, label: "Home" }, { id: 0x4b, label: "PgUp" },
  ], 1.4, 16),
  ...row([
    { id: 0x4c, label: "Del" }, { id: 0x4d, label: "End" }, { id: 0x4e, label: "PgDn" },
  ], 2.4, 16),
  ...row([{ id: 0x52, label: "↑" }], 4.4, 17),
  ...row([{ id: 0x50, label: "←" }, { id: 0x51, label: "↓" }, { id: 0x4f, label: "→" }], 5.4, 16),
  ...row([
    { id: 0x53, label: "Num" }, { id: 0x54, label: "/" }, { id: 0x55, label: "*" }, { id: 0x56, label: "-" },
  ], 1.4, 19),
  ...row([
    { id: 0x59, label: "7" }, { id: 0x5a, label: "8" }, { id: 0x5b, label: "9" }, { id: 0x57, label: "+", h: 2 },
  ], 2.4, 19),
  ...row([{ id: 0x5c, label: "4" }, { id: 0x5d, label: "5" }, { id: 0x5e, label: "6" }], 3.4, 19),
  ...row([
    { id: 0x5f, label: "1" }, { id: 0x60, label: "2" }, { id: 0x61, label: "3" }, { id: 0x58, label: "Enter", h: 2 },
  ], 4.4, 19),
  ...row([{ id: 0x62, label: "0", w: 2 }, { id: 0x63, label: "." }], 5.4, 19),
];

const countMap = computed(() => new Map(props.keys.map((key) => [key.key_id, key.count])));
const recordedTotal = computed(() => props.keys.reduce((sum, key) => sum + key.count, 0));
const maxCount = computed(() => Math.max(0, ...props.keys.map((key) => key.count)));
const isEmpty = computed(() => maxCount.value === 0);
const hoveredIndex = ref<number | null>(null);
const focusedIndex = ref<number | null>(null);
const inspectedIndex = computed(() => hoveredIndex.value ?? focusedIndex.value);
const inspected = computed(() => inspectedIndex.value === null ? null : layout[inspectedIndex.value]);
const topKeys = computed(() =>
  props.keys
    .filter((key) => key.count > 0)
    .slice()
    .sort((a, b) => b.count - a.count || a.key_id - b.key_id)
    .slice(0, 8)
    .map((key) => ({ id: key.key_id, label: keyName({ id: key.key_id, label: "" }), count: key.count })),
);

function countOf(id: number): number {
  return countMap.value.get(id) ?? 0;
}
function keyFill(id: number): string {
  const count = countOf(id);
  if (count === 0 || maxCount.value === 0) return "var(--card)";
  const intensity = Math.sqrt(count / maxCount.value);
  return `rgba(255, 126, 39, ${0.16 + intensity * 0.84})`;
}
function keyTextColor(id: number): string {
  const count = countOf(id);
  const intensity = maxCount.value === 0 ? 0 : Math.sqrt(count / maxCount.value);
  return intensity > 0.72 ? "#fff" : "var(--text)";
}
function keyName(key: KeySpec): string {
  if (key.id >= 0x04 && key.id <= 0x1d) return String.fromCharCode(65 + key.id - 0x04);
  if (key.id === 0x2c) return t("keyboard.keys.space");
  if (key.id === 0x28 || key.id === 0x58) return t("keyboard.keys.enter");
  if (key.id === 0x2a) return t("keyboard.keys.backspace");
  if (key.id === 0x2b) return t("keyboard.keys.tab");
  return key.label;
}
function percentage(id: number): string {
  if (recordedTotal.value <= 0) return "0%";
  return `${((countOf(id) / recordedTotal.value) * 100).toFixed(1)}%`;
}
function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}
function onKeyNavigation(event: KeyboardEvent): void {
  const deltas: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -10, ArrowDown: 10 };
  const delta = deltas[event.key];
  if (delta === undefined) return;
  event.preventDefault();
  hoveredIndex.value = null;
  const current = focusedIndex.value ?? 0;
  focusedIndex.value = Math.max(0, Math.min(layout.length - 1, current + delta));
}
</script>

<style scoped>
.keyboard-heatmap { display: flex; flex-direction: column; gap: 12px; }
.heatmap-head { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.total-label { color: var(--text-dim); margin-right: 8px; font-size: 12px; }
.heatmap-head strong { font-size: 24px; font-variant-numeric: tabular-nums; }
.legend { display: flex; align-items: center; gap: 7px; color: var(--text-dim); font-size: 11px; }
.gradient { width: 88px; height: 8px; border-radius: 99px; background: linear-gradient(90deg, var(--card), #ff7e27); border: 1px solid var(--border); }
.keyboard-stage { position: relative; overflow-x: auto; transition: opacity 0.15s ease; }
.keyboard-stage.loading { opacity: 0.58; }
.keyboard-svg { display: block; width: 100%; min-width: 760px; height: auto; outline: none; }
.keyboard-svg:focus-visible { filter: drop-shadow(0 0 3px rgba(255, 126, 39, 0.5)); }
.loading-mask { position: absolute; inset: 0; display: grid; place-items: center; color: var(--brand); pointer-events: none; }
.heatmap-footer { display: flex; align-items: center; justify-content: space-between; min-height: 24px; gap: 12px; }
.key-inspector { display: flex; gap: 10px; color: var(--text-dim); font-size: 12px; }
.key-inspector b { color: var(--text); }
.hint, .empty { color: var(--text-dim); font-size: 12px; }
.top-keys h4 { margin: 0 0 8px; font-size: 13px; font-weight: 500; }
.top-keys ol { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px 16px; }
.top-keys li { display: flex; justify-content: space-between; gap: 8px; padding: 5px 0; border-bottom: 1px solid var(--border); font-size: 12px; }
.top-keys li.empty { display: block; border: none; }
@media (max-width: 900px) { .top-keys ol { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
