<!--
  CaptureOverlay.vue
  截图遮罩窗（label='capture'，v0.8.0）：冻结帧 + 框选 + 标注编辑器 + QQ 式工具栏。

  设计思路：
  - **冻结帧**：Rust 抓屏后把整屏 PNG 交给本窗（`screenshot_frame`），窗口铺一张静止的桌面图。
    屏幕上是「被冻住的桌面」而不是「透出实时桌面的透明窗」——这是能做标注与圆角实时预览的前提。
    ⚠️ v0.8.0 初版是纯透明遮罩（没有图像），所以画笔/OCR 只能是灰掉的占位按钮，
    「想加文字只能关掉重截」就是这么来的。
  - **合成在本地 canvas**：裁剪 → 标注 → 圆角 → 投影 全部在本窗画好后整体交给 Rust 落剪贴板/磁盘，
    保证「预览到的」与「导出的」逐像素一致。
  - **两层画布**：`base`（离屏，只画一次背景图）供马赛克/模糊取样；`anno`（可见）按顺序重放所有标注。
    马赛克从 base 取样 → 每次重绘结果稳定，不受先前标注影响。
  - **QQ 式工具栏**：尺寸胶囊只显示选区尺寸；**圆角与投影收进工具栏的「圆角」按钮浮层**
    （初版把圆角滑杆直接摊在选区上方，看起来像「一截图就让你选圆角」）。
  - **剪贴板优先**：✓ / Enter 的默认动作是把 PNG 写进系统剪贴板，落盘是可选项。
  - **坐标换算**：前端拿到的是 CSS 像素，canvas 用物理像素（= CSS × k，k = 物理宽 / 窗口 CSS 宽），
    高 DPI（125%/Retina）下标注与文字不会偏移或发虚。

  修改历史：
    - 2026-09-16 @v0.8.0: 初始创建 - 透明遮罩 / 框选 / 尺寸 / 圆角 / 阴影 / 工具栏
    - 2026-09-16 @v0.8.0: 重写 - 改冻结帧编辑器；标注（矩形/箭头/画笔/马赛克/模糊/文字）+ 撤销；
      圆角与阴影移入工具栏浮层；选区支持 8 手柄缩放与整体拖动
    - 2026-09-17 @v0.8.1: 新增 - 工具栏「取字」（本地离线 OCR）+ 结果面板（识别中/文本/复制/语言标签）；
      删除与「马赛克」重复的「模糊（打码）」按钮；焦点在文本框时全局按键不接管、Esc 逐级退出
    - 2026-09-17 @v0.8.1: 性能 - 重绘改为 **rAF 按帧合并**（原 `deep` 监听逐 pointermove 全幅重绘，
      高刷屏下每帧上千次 2M 像素合成 → 掉帧）；`paintOps` 去掉「每条马赛克新建一张 canvas」的临时分配；
      无标注时跳过全幅 `clip()`；顺带修正「取字」图标里 A 字不居中（字形与外框非同心中轴）
    - 2026-09-17 @v0.9.0: 新增 - 取字结果面板显示**本次生效的引擎徽标**（标准引擎显示系统语言标签，
      增强引擎显示「本地增强引擎 · N 行」）。引擎由设置页决定，遮罩窗只负责如实展示，
      失败时清空徽标只留错误原因（避免"引擎写着增强、其实报错了"的误导）
-->。
<template>
  <div
    class="cap-root"
    :class="{ 'cap-root--draw': tool !== 'none' }"
    @pointerdown="onDown"
    @pointermove="onMove"
    @pointerup="onUp"
    @pointercancel="onUp"
  >
    <!-- 冻结的桌面（与真实桌面逐像素一致，但静止） -->
    <img v-if="imgUrl" class="shot" :src="imgUrl" alt="" draggable="false" />

    <!-- 压暗：在选区处挖一个洞（box-shadow 外扩把四周压暗，圆角同步） -->
    <div v-if="box" class="hole" :style="holeStyle"></div>
    <div v-else class="dim-all"></div>

    <!-- 标注层（只画在选区内，带圆角裁切） -->
    <canvas
      ref="annoEl"
      class="anno"
      :width="meta ? meta.physical_width : 1"
      :height="meta ? meta.physical_height : 1"
    ></canvas>

    <!-- 选区边框 + 手柄（仅在「选择」状态出现，标注时锁边避免误拖） -->
    <div v-if="box" class="sel" :style="selStyle"></div>
    <template v-if="box && tool === 'none'">
      <span
        v-for="h in HANDLES"
        :key="h"
        :class="['handle', `handle--${h}`]"
        :style="handlePos(h)"
        @pointerdown.stop="onHandleDown($event, h)"
      ></span>
    </template>

    <!-- 尺寸胶囊 -->
    <div v-if="box" class="pills" :style="pillStyle">
      <span class="pill pill--size">{{ Math.round(box.w) }} × {{ Math.round(box.h) }} px</span>
      <span v-if="radius > 0 || shadow" class="pill pill--tag">
        <template v-if="radius > 0">{{ t("shot.radius") }} {{ radius }}</template>
        <template v-if="radius > 0 && shadow"> · </template>
        <template v-if="shadow">{{ t("shot.shadow") }}</template>
      </span>
    </div>

    <!-- 样式条：仅在绘制类工具激活时出现（打码显示强度，其余显示颜色 / 粗细） -->
    <div v-if="box && isDrawTool" class="style-bar" :style="styleBarStyle" @pointerdown.stop>
      <template v-if="tool !== 'mosaic'">
        <button
          v-for="c in COLORS"
          :key="c"
          class="swatch"
          :class="{ on: color === c }"
          :style="{ background: c }"
          @click="color = c"
        ></button>
        <span class="tb-sep"></span>
        <button
          v-for="w in WIDTHS"
          :key="w"
          class="wbtn"
          :class="{ on: strokeWidth === w }"
          @click="strokeWidth = w"
        >
          <span class="wdot" :style="{ width: `${w + 2}px`, height: `${w + 2}px` }"></span>
        </button>
      </template>
      <template v-else>
        <span class="sb-label">{{ t("shot.strength") }}</span>
        <button
          v-for="s in STRENGTHS"
          :key="s.block"
          class="sbtn"
          :class="{ on: mosaicBlock === s.block }"
          @click="mosaicBlock = s.block"
        >
          {{ t(s.label) }}
        </button>
      </template>
    </div>

    <!-- QQ 式工具栏 -->
    <div v-if="box" class="bar" :style="barStyle" @pointerdown.stop>
      <button class="tb" :class="{ on: tool === 'rect' }" :title="t('shot.toolRect')" @click="pick('rect')">
        <svg viewBox="0 0 20 20" width="18" height="18"><rect x="3.5" y="4.5" width="13" height="11" rx="1.5" /></svg>
      </button>
      <button class="tb" :class="{ on: tool === 'arrow' }" :title="t('shot.toolArrow')" @click="pick('arrow')">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M4 16L16 4M16 4h-6M16 4v6" /></svg>
      </button>
      <button class="tb" :class="{ on: tool === 'pen' }" :title="t('shot.toolPen')" @click="pick('pen')">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M4 16l1-4 8-8 3 3-8 8-4 1z" /></svg>
      </button>
      <button class="tb" :class="{ on: tool === 'mosaic' }" :title="t('shot.toolMosaic')" @click="pick('mosaic')">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M3 3h5v5H3zM12 3h5v5h-5zM3 12h5v5H3zM12 12h5v5h-5z" /></svg>
      </button>
      <button class="tb" :class="{ on: tool === 'text' }" :title="t('shot.toolText')" @click="pick('text')">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M4 5h12M10 5v11" /></svg>
      </button>
      <button
        class="tb"
        :class="{ on: activePanel === 'ocr', busy: ocrBusy }"
        :title="t('shot.ocr')"
        @click="runOcr"
      >
        <svg viewBox="0 0 20 20" width="18" height="18">
          <path d="M3 7V3h4M17 7V3h-4M3 13v4h4M17 13v4h-4" />
          <path d="M10 7L7.6 13M10 7l2.4 6M8.7 11.2h2.6" />
        </svg>
      </button>

      <span class="tb-sep"></span>
      <button class="tb" :disabled="ops.length === 0" :title="t('shot.undo')" @click="undo">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M8 7H13a4 4 0 0 1 0 8H8M8 7L5 4M8 7L5 10" /></svg>
      </button>

      <span class="tb-sep"></span>
      <button class="tb" :title="t('shot.radiusShadow')" :class="{ on: activePanel === 'radius' }" @click="togglePanel('radius')">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M4 16V9a5 5 0 0 1 5-5h7" /></svg>
      </button>

      <span class="tb-sep"></span>
      <button class="tb" :title="t('shot.fullscreen')" @click="selectAll">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M4 8V4h4M16 12v4h-4M4 12v4h4M16 8V4h-4" /></svg>
      </button>
      <button class="tb" :title="t('shot.save')" @click="commit(false, true)">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M10 3v9m0 0l-3.5-3.5M10 12l3.5-3.5M4 16h12" /></svg>
      </button>
      <button class="tb" :title="t('shot.copy')" @click="commit(true, false)">
        <svg viewBox="0 0 20 20" width="18" height="18"><rect x="7.5" y="7.5" width="9" height="9" rx="1.5" /><path d="M12.5 4.5h-8v9" /></svg>
      </button>
      <button class="tb tb--cancel" :title="t('shot.cancel')" @click="cancel">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M6 6l8 8M14 6l-8 8" /></svg>
      </button>
      <button class="tb tb--ok" :title="t('shot.confirm')" @click="commit(true, autoSave)">
        <svg viewBox="0 0 20 20" width="18" height="18"><path d="M5 10.5l3.5 3.5L15 6.5" /></svg>
      </button>
    </div>

    <!-- 圆角 / 投影浮层 -->
    <div v-if="box && activePanel === 'radius'" class="panel" :style="panelStyle" @pointerdown.stop>
      <div class="panel-row">
        <span class="pill-label">{{ t("shot.radius") }}</span>
        <input v-model.number="radius" class="radius" type="range" min="0" max="40" step="2" />
        <span class="pill-num">{{ radius }}</span>
      </div>
      <div class="panel-row">
        <span class="pill-label">{{ t("shot.shadow") }}</span>
        <button class="switch" :class="{ on: shadow }" @click="shadow = !shadow"><span class="knob"></span></button>
      </div>
    </div>

    <!-- 取字结果（本地离线 OCR，不联网、不上传） -->
    <div
      v-if="box && activePanel === 'ocr'"
      class="panel panel--ocr"
      :style="ocrPanelStyle"
      @pointerdown.stop
    >
      <div class="ocr-head">
        <span class="pill-label">{{ t("shot.ocr") }}</span>
        <span
          v-if="ocrBadge"
          class="ocr-lang"
          :class="{ 'ocr-lang--enh': ocrEngine === 'enhanced' }"
        >{{ ocrBadge }}</span>
        <span v-if="ocrStale" class="ocr-lang ocr-lang--warn">{{ t("shot.ocrStale") }}</span>
      </div>
      <p v-if="ocrBusy" class="ocr-hint">{{ t("shot.ocrBusy") }}</p>
      <p v-else-if="ocrErr" class="ocr-hint ocr-hint--err">{{ ocrErr }}</p>
      <textarea
        v-else
        class="ocr-text"
        readonly
        spellcheck="false"
        :value="ocrText"
      ></textarea>
      <div class="ocr-actions">
        <button class="mini" :disabled="ocrBusy || !ocrText" @click="copyOcr">
          {{ ocrCopied ? t("shot.ocrCopied") : t("shot.ocrCopy") }}
        </button>
        <button class="mini mini--ghost" @click="activePanel = null">{{ t("shot.close") }}</button>
      </div>
    </div>

    <!-- 文字输入（就地编辑，Enter 提交 / Esc 取消） -->
    <input
      v-if="textEdit"
      ref="textEl"
      v-model="textValue"
      class="text-edit"
      :style="textStyle"
      :placeholder="t('shot.textPlaceholder')"
      @pointerdown.stop
      @keydown.stop="onTextKey"
      @blur="commitText"
    />

    <!-- 提示条（未框选时） -->
    <div v-if="!box" class="hint">{{ t("shot.hint") }}</div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { screenshot } from "../api/screenshot";
import type { CaptureReadyPayload, OcrEngineKind } from "../types";

const { t } = useI18n();

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 标注操作（坐标一律是「窗口内 CSS 像素」，导出时再按 k 放大） */
type Op =
  | { k: "rect"; x: number; y: number; w: number; h: number; color: string; width: number }
  | { k: "arrow"; x1: number; y1: number; x2: number; y2: number; color: string; width: number }
  | { k: "pen"; pts: number[][]; color: string; width: number }
  | { k: "mosaic"; x: number; y: number; w: number; h: number; block: number }
  | { k: "text"; x: number; y: number; text: string; size: number; color: string };

type Tool = "none" | "rect" | "arrow" | "pen" | "mosaic" | "text";
type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

type Drag =
  | { kind: "new"; sx: number; sy: number }
  | { kind: "move"; sx: number; sy: number; from: Box }
  | { kind: "resize"; handle: Handle; sx: number; sy: number; from: Box }
  | { kind: "draw"; sx: number; sy: number; op: Op }
  | null;

const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const COLORS = ["#ef4444", "#f59e0b", "#22c55e", "#2f6bff", "#111827", "#ffffff"];
const WIDTHS = [2, 4, 7];
const STRENGTHS = [
  { block: 8, label: "shot.strengthWeak" },
  { block: 14, label: "shot.strengthMedium" },
  { block: 24, label: "shot.strengthStrong" },
] as const;
/** 判定为「拖拽框选」的最小位移（小于此值视为单击 → 全屏） */
const DRAG_MIN = 4;
/** 标注数量上限（防止误操作堆出上千条） */
const MAX_OPS = 300;

const meta = ref<CaptureReadyPayload | null>(null);
const imgUrl = ref("");
const box = ref<Box | null>(null);
const ops = ref<Op[]>([]);
const draft = ref<Op | null>(null);
const tool = ref<Tool>("none");
const color = ref(COLORS[0]);
const strokeWidth = ref(4);
const mosaicBlock = ref(14);
const radius = ref(8);
const shadow = ref(false);
const autoSave = ref(false);
const activePanel = ref<"radius" | "ocr" | null>(null);
const textEdit = ref<{ x: number; y: number } | null>(null);
const textValue = ref("");
/** 取字（本地离线 OCR）：忙碌态 / 结果 / 错误 / 生效语言 / 引擎 / 行数 / 已复制提示 / 选区变更后结果过期 */
const ocrBusy = ref(false);
const ocrText = ref("");
const ocrErr = ref("");
const ocrLang = ref("");
/** 本次真正生效的引擎（v0.9.0）。空串 = 还没取过字 */
const ocrEngine = ref<OcrEngineKind | "">("");
/** 增强引擎识别出的行数（标准引擎只有整段文本，恒 0） */
const ocrLines = ref(0);
const ocrCopied = ref(false);
const ocrStale = ref(false);

/**
 * 面板头部的引擎徽标：
 * - 标准引擎 → 显示系统给的语言标签（zh-Hans-CN 这类）
 * - 增强引擎 → 显示「本地增强引擎 · N 行」（语言标签 `ch` 对用户没意义，行数才有）
 */
const ocrBadge = computed(() => {
  if (ocrEngine.value === "enhanced") {
    return ocrLines.value > 0
      ? `${t("shot.ocrEngineEnhanced")} · ${t("shot.ocrLines", { n: ocrLines.value })}`
      : t("shot.ocrEngineEnhanced");
  }
  return ocrLang.value;
});
const textSize = 20;
const vw = ref(window.innerWidth);
const vh = ref(window.innerHeight);

const annoEl = ref<HTMLCanvasElement | null>(null);
const textEl = ref<HTMLInputElement | null>(null);
/** 离屏背景层：只画一次整屏图，供马赛克 / 模糊取样（保证重绘结果稳定） */
let base: HTMLCanvasElement | null = null;

let drag: Drag = null;
let unlisten: (() => void) | null = null;

const clamp = (v: number, min: number, max: number) => Math.min(Math.max(v, min), max);

/** CSS 像素 → canvas 物理像素的比例（= 物理宽 / 窗口 CSS 宽，高 DPI 下 ≈ dpr） */
const k = computed(() => {
  const m = meta.value;
  if (!m || vw.value <= 0) return 1;
  return m.physical_width / vw.value;
});

const isDrawTool = computed(() => tool.value !== "none" && tool.value !== "text");

const selStyle = computed(() => {
  const b = box.value!;
  return { left: `${b.x}px`, top: `${b.y}px`, width: `${b.w}px`, height: `${b.h}px`, borderRadius: `${radius.value}px` };
});

/** 压暗层：一个与选区同形（含圆角）的洞，外扩大 shadow 把四周压暗 */
const holeStyle = computed(() => {
  const b = box.value!;
  return {
    left: `${b.x}px`,
    top: `${b.y}px`,
    width: `${b.w}px`,
    height: `${b.h}px`,
    borderRadius: `${radius.value}px`,
  };
});

function handlePos(h: Handle) {
  const b = box.value!;
  const cx = b.x + b.w / 2;
  const cy = b.y + b.h / 2;
  const map: Record<Handle, [number, number]> = {
    nw: [b.x, b.y],
    n: [cx, b.y],
    ne: [b.x + b.w, b.y],
    e: [b.x + b.w, cy],
    se: [b.x + b.w, b.y + b.h],
    s: [cx, b.y + b.h],
    sw: [b.x, b.y + b.h],
    w: [b.x, cy],
  };
  const [x, y] = map[h];
  return { left: `${x}px`, top: `${y}px` };
}

// 工具条/胶囊尽量贴在选区外侧；贴不下就翻到另一侧
const pillStyle = computed(() => {
  const b = box.value!;
  const top = b.y >= 40 ? b.y - 36 : Math.min(b.y + b.h + 10, vh.value - 36);
  return { left: `${clamp(b.x, 8, Math.max(8, vw.value - 260))}px`, top: `${top}px` };
});

const barTop = computed(() => {
  const b = box.value!;
  const below = b.y + b.h + 12;
  return below + 46 <= vh.value ? below : Math.max(8, b.y - 58);
});

const barStyle = computed(() => {
  const b = box.value!;
  const center = clamp(b.x + b.w / 2, 240, Math.max(240, vw.value - 240));
  return { left: `${center}px`, top: `${barTop.value}px`, transform: "translateX(-50%)" };
});

const styleBarStyle = computed(() => {
  const b = box.value!;
  const center = clamp(b.x + b.w / 2, 200, Math.max(200, vw.value - 200));
  const up = barTop.value >= 44;
  return {
    left: `${center}px`,
    top: `${up ? barTop.value - 42 : barTop.value + 46}px`,
    transform: "translateX(-50%)",
  };
});

const panelStyle = computed(() => {
  const b = box.value!;
  const center = clamp(b.x + b.w / 2, 140, Math.max(140, vw.value - 140));
  const up = barTop.value >= 44;
  return {
    left: `${center}px`,
    top: `${up ? barTop.value - 104 : barTop.value + 46}px`,
    transform: "translateX(-50%)",
  };
});

/** 取字面板比圆角面板高得多，单独算落点：优先放工具栏下方，放不下才翻到上方 */
const ocrPanelStyle = computed(() => {
  const b = box.value!;
  const center = clamp(b.x + b.w / 2, 190, Math.max(190, vw.value - 190));
  const panelH = 214;
  const below = barTop.value + 46;
  const top = below + panelH <= vh.value ? below : Math.max(8, barTop.value - panelH - 6);
  return { left: `${center}px`, top: `${top}px`, transform: "translateX(-50%)" };
});

const textStyle = computed(() => {
  if (!textEdit.value) return {};
  return {
    left: `${textEdit.value.x}px`,
    top: `${textEdit.value.y}px`,
    color: color.value,
    fontSize: `${textSize}px`,
  };
});

// ===== canvas 绘制 =====

function roundRectPath(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  const rr = Math.max(0, Math.min(r, w / 2, h / 2));
  g.beginPath();
  g.moveTo(x + rr, y);
  g.lineTo(x + w - rr, y);
  g.arcTo(x + w, y, x + w, y + rr, rr);
  g.lineTo(x + w, y + h - rr);
  g.arcTo(x + w, y + h, x + w - rr, y + h, rr);
  g.lineTo(x + rr, y + h);
  g.arcTo(x, y + h, x, y + h - rr, rr);
  g.lineTo(x, y + rr);
  g.arcTo(x, y, x + rr, y, rr);
  g.closePath();
}

/**
 * 把一组标注画到 ctx 上。
 * @param s    CSS 像素 → 目标画布像素的比例
 * @param offX/offY  目标画布坐标偏移（预览为 0；导出时把选区左上角搬到留白之后）
 */
function paintOps(g: CanvasRenderingContext2D, s: number, offX: number, offY: number, list: Op[]) {
  g.lineJoin = "round";
  g.lineCap = "round";
  for (const op of list) {
    switch (op.k) {
      case "rect": {
        g.strokeStyle = op.color;
        g.lineWidth = op.width * s;
        g.strokeRect(op.x * s + offX, op.y * s + offY, op.w * s, op.h * s);
        break;
      }
      case "arrow": {
        const x1 = op.x1 * s + offX;
        const y1 = op.y1 * s + offY;
        const x2 = op.x2 * s + offX;
        const y2 = op.y2 * s + offY;
        const lw = Math.max(1, op.width * s);
        g.strokeStyle = op.color;
        g.lineWidth = lw;
        g.beginPath();
        g.moveTo(x1, y1);
        g.lineTo(x2, y2);
        g.stroke();
        const ang = Math.atan2(y2 - y1, x2 - x1);
        const head = Math.max(lw * 4, 10 * s);
        g.fillStyle = op.color;
        g.beginPath();
        g.moveTo(x2, y2);
        g.lineTo(x2 - head * Math.cos(ang - Math.PI / 7), y2 - head * Math.sin(ang - Math.PI / 7));
        g.lineTo(x2 - head * Math.cos(ang + Math.PI / 7), y2 - head * Math.sin(ang + Math.PI / 7));
        g.closePath();
        g.fill();
        break;
      }
      case "pen": {
        if (op.pts.length < 2) break;
        g.strokeStyle = op.color;
        g.lineWidth = Math.max(1, op.width * s);
        g.beginPath();
        g.moveTo(op.pts[0][0] * s + offX, op.pts[0][1] * s + offY);
        for (let i = 1; i < op.pts.length; i++) {
          g.lineTo(op.pts[i][0] * s + offX, op.pts[i][1] * s + offY);
        }
        g.stroke();
        break;
      }
      case "mosaic": {
        if (!base) break;
        const dx = op.x * s + offX;
        const dy = op.y * s + offY;
        const dw = Math.max(1, op.w * s);
        const dh = Math.max(1, op.h * s);
        const block = Math.max(1, Math.round(op.block * s));
        const bw = Math.max(1, Math.round(dw / block));
        const bh = Math.max(1, Math.round(dh / block));
        // 全局复用一张 scratch 画布；尺寸变化时重新分配并清空
        if (!scratch) scratch = document.createElement("canvas");
        if (scratch.width !== bw || scratch.height !== bh) {
          scratch.width = bw;
          scratch.height = bh;
        } else {
          scratch.getContext("2d")?.clearRect(0, 0, bw, bh);
        }
        const tg = scratch.getContext("2d");
        if (!tg) break;
        // 最近邻缩放 → 硬边马赛克块（开插值就变成「模糊」，那是另一种效果）
        tg.imageSmoothingEnabled = false;
        // 从「纯背景层」取样（不含先前标注），保证每次重绘结果一致
        tg.drawImage(base, op.x * s, op.y * s, dw, dh, 0, 0, bw, bh);
        g.imageSmoothingEnabled = false;
        g.drawImage(scratch, 0, 0, bw, bh, dx, dy, dw, dh);
        g.imageSmoothingEnabled = true;
        break;
      }
      case "text": {
        g.font = `600 ${op.size * s}px -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif`;
        g.fillStyle = op.color;
        g.textBaseline = "top";
        g.fillText(op.text, op.x * s + offX, op.y * s + offY);
        break;
      }
    }
  }
}

/**
 * 马赛克取样用的临时画布。**全局复用一张**——原实现每次 `paintOps` 都
 * `document.createElement("canvas")`，拖动时每帧新建+丢弃一张带 GPU 后端的画布，
 * 是明显的 GC / 显存抖动源。
 */
let scratch: HTMLCanvasElement | null = null;

function redraw() {
  const c = annoEl.value;
  if (!c) return;
  const g = c.getContext("2d");
  if (!g) return;
  const b = box.value;
  const list = draft.value ? [...ops.value, draft.value] : ops.value;
  g.setTransform(1, 0, 0, 1, 0, 0);
  g.clearRect(0, 0, c.width, c.height);
  // 没框选、或一条标注都还没画时直接收工：
  // 省掉一次全幅 `roundRectPath + clip()` 的路径栅格化（1920×1080 ≈ 2M 像素）
  if (!b || list.length === 0) return;
  const s = k.value;
  g.save();
  roundRectPath(g, b.x * s, b.y * s, b.w * s, b.h * s, radius.value * s);
  g.clip();
  paintOps(g, s, 0, 0, list);
  g.restore();
}

/**
 * 重绘调度器：**按帧合并**。
 *
 * 拖动时 `pointermove` 在高刷屏 / 高回报率鼠标 / 手写笔下能到 500–1000 Hz，
 * 而每次 `box` / `draft` 变化都会触发重绘 —— 一次全幅 `clearRect`(2M px) + `clip()` + 重放全部标注。
 * 逐事件重绘等于每秒做上千次全屏合成，必然掉帧。
 * 这里只「标脏」，真正的重绘交给 `requestAnimationFrame`，一帧最多一次。
 */
let redrawRaf = 0;

function scheduleRedraw() {
  if (redrawRaf) return; // 本帧已排队
  redrawRaf = requestAnimationFrame(() => {
    redrawRaf = 0;
    redraw();
  });
}

// 不再用 `deep: true`：box / draft / ops 每次变更都是**整体替换**（`xxx.value = {...}` / `[...]`），
// 引用比较足够；开 deep 会在每个 pointermove 上多走一遍深度遍历。
watch([box, radius, ops, draft], scheduleRedraw);

async function loadBase(url: string) {
  const b = meta.value;
  if (!b || !url) return;
  await new Promise<void>((resolve) => {
    const im = new Image();
    im.onload = () => {
      const c = document.createElement("canvas");
      c.width = im.naturalWidth;
      c.height = im.naturalHeight;
      c.getContext("2d")?.drawImage(im, 0, 0);
      base = c;
      resolve();
    };
    im.onerror = () => resolve();
    im.src = url;
  });
  redraw();
}

// ===== 指针交互 =====

function localPoint(e: PointerEvent) {
  return { x: e.clientX, y: e.clientY };
}

function inBox(p: { x: number; y: number }) {
  const b = box.value;
  return !!b && p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h;
}

function onDown(e: PointerEvent) {
  if (e.button !== 0) return;
  const p = localPoint(e);
  (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  activePanel.value = null;

  if (textEdit.value) {
    // 点击别处 → 先提交正在输入的文字
    void commitText();
  }

  if (tool.value === "text") {
    if (!inBox(p)) return;
    // 掐掉 pointerdown 的默认行为：否则浏览器会在同一轮里把焦点挪到 body，
    // 让 nextTick 刚聚焦的输入框立刻 blur → commitText（空文本）→ 编辑器凭空消失。
    e.preventDefault();
    textEdit.value = { ...p };
    textValue.value = "";
    void nextTick(() => textEl.value?.focus());
    return;
  }

  if (tool.value !== "none") {
    if (!inBox(p)) return;
    const op = beginOp(tool.value, p);
    if (!op) return;
    drag = { kind: "draw", sx: p.x, sy: p.y, op };
    draft.value = op;
    return;
  }

  if (box.value && inBox(p)) {
    drag = { kind: "move", sx: p.x, sy: p.y, from: { ...box.value } };
    return;
  }
  drag = { kind: "new", sx: p.x, sy: p.y };
  box.value = { x: p.x, y: p.y, w: 0, h: 0 };
}

function beginOp(toolName: Tool, p: { x: number; y: number }): Op | null {
  switch (toolName) {
    case "rect":
      return { k: "rect", x: p.x, y: p.y, w: 0, h: 0, color: color.value, width: strokeWidth.value };
    case "arrow":
      return { k: "arrow", x1: p.x, y1: p.y, x2: p.x, y2: p.y, color: color.value, width: strokeWidth.value };
    case "pen":
      return { k: "pen", pts: [[p.x, p.y]], color: color.value, width: strokeWidth.value };
    case "mosaic":
      return { k: "mosaic", x: p.x, y: p.y, w: 0, h: 0, block: mosaicBlock.value };
    default:
      return null;
  }
}

function onMove(e: PointerEvent) {
  if (!drag) return;
  const p = localPoint(e);
  if (drag.kind === "new") {
    box.value = {
      x: Math.min(drag.sx, p.x),
      y: Math.min(drag.sy, p.y),
      w: Math.abs(p.x - drag.sx),
      h: Math.abs(p.y - drag.sy),
    };
    return;
  }
  if (drag.kind === "move") {
    const nx = clamp(drag.from.x + (p.x - drag.sx), 0, Math.max(0, vw.value - drag.from.w));
    const ny = clamp(drag.from.y + (p.y - drag.sy), 0, Math.max(0, vh.value - drag.from.h));
    box.value = { ...drag.from, x: nx, y: ny };
    return;
  }
  if (drag.kind === "resize") {
    box.value = resizeBox(drag.from, drag.handle, p.x - drag.sx, p.y - drag.sy);
    return;
  }
  // 绘制中
  const op = drag.op;
  if (op.k === "rect" || op.k === "mosaic") {
    op.x = Math.min(drag.sx, p.x);
    op.y = Math.min(drag.sy, p.y);
    op.w = Math.abs(p.x - drag.sx);
    op.h = Math.abs(p.y - drag.sy);
  } else if (op.k === "arrow") {
    op.x2 = p.x;
    op.y2 = p.y;
  } else if (op.k === "pen") {
    const last = op.pts[op.pts.length - 1];
    if (Math.abs(p.x - last[0]) + Math.abs(p.y - last[1]) >= 2) op.pts.push([p.x, p.y]);
  }
  draft.value = { ...op };
}

function onUp() {
  if (!drag) return;
  const d = drag;
  drag = null;

  if (d.kind === "new") {
    const b = box.value;
    if (!b || b.w < DRAG_MIN || b.h < DRAG_MIN) {
      selectAll();
    } else {
      box.value = clampBox(b);
    }
    return;
  }
  if (d.kind === "draw") {
    const op = draft.value;
    draft.value = null;
    if (!op) return;
    const ok =
      (op.k === "rect" || op.k === "mosaic") && op.w >= 3 && op.h >= 3
        ? true
        : op.k === "arrow"
          ? Math.abs(op.x2 - op.x1) >= 3 || Math.abs(op.y2 - op.y1) >= 3
          : op.k === "pen"
            ? op.pts.length >= 2
            : false;
    if (ok && ops.value.length < MAX_OPS) ops.value = [...ops.value, op];
    return;
  }
  if (d.kind === "resize" && box.value) {
    box.value = clampBox(box.value);
  }
}

function resizeBox(from: Box, handle: Handle, dx: number, dy: number): Box {
  let x1 = from.x;
  let y1 = from.y;
  let x2 = from.x + from.w;
  let y2 = from.y + from.h;
  if (handle.includes("w")) x1 = from.x + dx;
  if (handle.includes("e")) x2 = from.x + from.w + dx;
  if (handle.includes("n")) y1 = from.y + dy;
  if (handle.includes("s")) y2 = from.y + from.h + dy;
  if (x2 - x1 < DRAG_MIN) {
    if (handle.includes("w")) x1 = x2 - DRAG_MIN;
    else x2 = x1 + DRAG_MIN;
  }
  if (y2 - y1 < DRAG_MIN) {
    if (handle.includes("n")) y1 = y2 - DRAG_MIN;
    else y2 = y1 + DRAG_MIN;
  }
  return clampBox({ x: x1, y: y1, w: x2 - x1, h: y2 - y1 });
}

function clampBox(b: Box): Box {
  let { x, y, w, h } = b;
  w = clamp(w, DRAG_MIN, vw.value);
  h = clamp(h, DRAG_MIN, vh.value);
  x = clamp(x, 0, Math.max(0, vw.value - w));
  y = clamp(y, 0, Math.max(0, vh.value - h));
  return { x, y, w, h };
}

function onHandleDown(e: PointerEvent, h: Handle) {
  if (!box.value) return;
  (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  drag = { kind: "resize", handle: h, sx: e.clientX, sy: e.clientY, from: { ...box.value } };
}

// ===== 文字 =====

async function commitText() {
  const at = textEdit.value;
  const v = textValue.value.trim();
  textEdit.value = null;
  textValue.value = "";
  if (!at || !v) return;
  if (ops.value.length >= MAX_OPS) return;
  ops.value = [...ops.value, { k: "text", x: at.x, y: at.y, text: v, size: textSize, color: color.value }];
}

function onTextKey(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    void commitText();
  } else if (e.key === "Escape") {
    e.preventDefault();
    textEdit.value = null;
    textValue.value = "";
  }
}

// ===== 取字（本地离线 OCR）=====

/**
 * 对当前选区取字。
 *
 * 坐标先换成**物理像素**再交给 Rust —— 与导出合成共用同一个 `k`，
 * 保证「取字的范围」与「导出的范围」是同一块像素。
 * 整屏帧本就缓存在 Rust 侧，所以这里不传图，只传矩形。
 */
async function runOcr() {
  const b = box.value;
  if (!b || ocrBusy.value) return;
  activePanel.value = "ocr";
  ocrBusy.value = true;
  ocrErr.value = "";
  ocrText.value = "";
  ocrCopied.value = false;
  ocrStale.value = false;
  // 引擎徽标也清掉：失败时必须显示的是错误原因，而不是上一次的引擎
  ocrEngine.value = "";
  ocrLines.value = 0;
  try {
    const s = k.value;
    const res = await screenshot.ocr(
      Math.round(b.x * s),
      Math.round(b.y * s),
      Math.max(1, Math.round(b.w * s)),
      Math.max(1, Math.round(b.h * s)),
    );
    ocrText.value = res.text.trim();
    ocrLang.value = res.language ?? "";
    ocrEngine.value = res.engine;
    ocrLines.value = res.lines;
    if (!ocrText.value) ocrErr.value = t("shot.ocrEmpty");
  } catch (err) {
    ocrErr.value = err instanceof Error ? err.message : String(err);
  } finally {
    ocrBusy.value = false;
  }
}

/** 复制取字结果（走 Rust 的 arboard，不依赖 WebView 的剪贴板权限） */
async function copyOcr() {
  if (!ocrText.value) return;
  try {
    await screenshot.copyText(ocrText.value);
    ocrCopied.value = true;
    window.setTimeout(() => (ocrCopied.value = false), 1600);
  } catch (err) {
    ocrErr.value = err instanceof Error ? err.message : String(err);
  }
}

/** 选区一变，之前取到的文字就对不上了 → 标记过期，免得用户复制走旧文本 */
watch(
  () => box.value && `${box.value.x},${box.value.y},${box.value.w},${box.value.h}`,
  () => {
    if (activePanel.value === "ocr" && ocrText.value) ocrStale.value = true;
  },
);

// ===== 工具 / 撤销 =====

function pick(name: Tool) {
  tool.value = tool.value === name ? "none" : name;
  activePanel.value = null;
}

function togglePanel(name: "radius") {
  activePanel.value = activePanel.value === name ? null : name;
}

function undo() {
  if (ops.value.length === 0) return;
  ops.value = ops.value.slice(0, -1);
}

function selectAll() {
  tool.value = "none";
  activePanel.value = null;
  box.value = { x: 0, y: 0, w: vw.value, h: vh.value };
}

// ===== 提交 / 取消 =====

/** 把选区 + 标注 + 圆角 + 投影合成成一张 PNG（data URL） */
function compose(): string | null {
  const b = box.value;
  if (!b || !base || !meta.value) return null;
  const s = k.value;
  const cw = Math.round(b.w * s);
  const ch = Math.round(b.h * s);
  if (cw < 1 || ch < 1) return null;
  const r = radius.value * s;
  const pad = shadow.value ? Math.round(Math.max(radius.value * 1.5, 14) * s) : 0;

  const c = document.createElement("canvas");
  c.width = cw + pad * 2;
  c.height = ch + pad * 2;
  const g = c.getContext("2d");
  if (!g) return null;

  if (shadow.value) {
    g.save();
    g.shadowColor = "rgba(15, 23, 42, 0.42)";
    g.shadowBlur = Math.max(6, pad * 0.85);
    g.shadowOffsetY = 3 * s;
    g.fillStyle = "#000";
    roundRectPath(g, pad, pad, cw, ch, r);
    g.fill();
    g.restore();
  }

  g.save();
  roundRectPath(g, pad, pad, cw, ch, r);
  g.clip();
  g.drawImage(base, Math.round(b.x * s), Math.round(b.y * s), cw, ch, pad, pad, cw, ch);
  paintOps(g, s, pad - Math.round(b.x * s), pad - Math.round(b.y * s), ops.value);
  g.restore();

  return c.toDataURL("image/png");
}

async function cancel() {
  try {
    await screenshot.cancel();
  } finally {
    reset();
    await getCurrentWindow().hide();
  }
}

async function commit(copy: boolean, save: boolean) {
  const png = compose();
  if (!png) return;
  try {
    await screenshot.commit({ pngBase64: png, copy, save });
  } finally {
    reset();
    await getCurrentWindow().hide();
  }
}

function reset() {
  meta.value = null;
  imgUrl.value = "";
  box.value = null;
  ops.value = [];
  draft.value = null;
  tool.value = "none";
  activePanel.value = null;
  textEdit.value = null;
  textValue.value = "";
  ocrBusy.value = false;
  ocrText.value = "";
  ocrErr.value = "";
  ocrLang.value = "";
  ocrCopied.value = false;
  ocrStale.value = false;
  base = null;
  drag = null;
  // 丢掉上一会话残留的重绘任务，避免新会话首帧被旧状态覆盖
  if (redrawRaf) {
    cancelAnimationFrame(redrawRaf);
    redrawRaf = 0;
  }
}

// ===== 键盘 =====

function onKey(e: KeyboardEvent) {
  if (textEdit.value) return; // 正在输入文字，交给输入框自己处理
  // 焦点在输入类控件里（取字文本框 / 滑杆）：除 Esc 外一律不接管，
  // 否则用户在文本框里按 Enter 会把截图直接确认掉
  const tag = (e.target as HTMLElement | null)?.tagName;
  if ((tag === "TEXTAREA" || tag === "INPUT") && e.key !== "Escape") return;
  if (e.key === "Escape") {
    e.preventDefault();
    if (activePanel.value) {
      activePanel.value = null;
      return;
    }
    if (tool.value !== "none") {
      tool.value = "none";
      return;
    }
    void cancel();
  } else if (e.key === "Enter") {
    e.preventDefault();
    if (box.value) void commit(true, autoSave.value);
  } else if ((e.key === "a" || e.key === "A") && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    selectAll();
  } else if ((e.key === "z" || e.key === "Z") && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    undo();
  }
}

// ===== 生命周期 =====

function sync() {
  vw.value = window.innerWidth;
  vh.value = window.innerHeight;
}

onMounted(async () => {
  // 透明化：全局 style.css 的 body{background} 会把遮罩窗变成不透明黑块
  for (const el of [document.documentElement, document.body, document.getElementById("app")]) {
    if (el) {
      el.style.background = "transparent";
      el.style.overflow = "hidden";
      el.style.margin = "0";
      el.style.padding = "0";
    }
  }
  window.addEventListener("resize", sync);
  window.addEventListener("keydown", onKey);
  unlisten = await listen<CaptureReadyPayload>("capture-ready", async (ev) => {
    reset();
    sync();
    meta.value = ev.payload;
    radius.value = ev.payload.default_radius ?? 8;
    shadow.value = ev.payload.default_shadow ?? false;
    try {
      const cfg = await screenshot.getConfig();
      autoSave.value = cfg.auto_save;
      radius.value = cfg.corner_radius;
      shadow.value = cfg.shadow;
    } catch {
      /* 忽略：用事件里的默认值兜底 */
    }
    try {
      const url = await screenshot.frame();
      imgUrl.value = url;
      await loadBase(url);
    } catch (err) {
      console.warn("[capture] 拉取整屏帧失败", err);
    }
  });
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", sync);
  window.removeEventListener("keydown", onKey);
  unlisten?.();
  if (redrawRaf) cancelAnimationFrame(redrawRaf);
  redrawRaf = 0;
  scratch = null;
});
</script>

<style scoped>
.cap-root {
  position: fixed;
  inset: 0;
  cursor: crosshair;
  user-select: none;
  overflow: hidden;
}
.shot {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  -webkit-user-drag: none;
}
.anno {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
.dim-all {
  position: absolute;
  inset: 0;
  background: rgba(6, 12, 24, 0.45);
  pointer-events: none;
}
/* 挖洞：自身透明，靠外扩阴影把四周压暗（圆角随之同步） */
.hole {
  position: absolute;
  box-shadow: 0 0 0 100vmax rgba(6, 12, 24, 0.45);
  pointer-events: none;
}
.sel {
  position: absolute;
  border: 1.5px solid #2f6bff;
  pointer-events: none;
}
.handle {
  position: absolute;
  width: 9px;
  height: 9px;
  margin: -5px 0 0 -5px;
  background: #fff;
  border: 1.5px solid #2f6bff;
  border-radius: 2px;
  pointer-events: auto;
}
.handle--nw { cursor: nwse-resize; }
.handle--n { cursor: ns-resize; }
.handle--ne { cursor: nesw-resize; }
.handle--e { cursor: ew-resize; }
.handle--se { cursor: nwse-resize; }
.handle--s { cursor: ns-resize; }
.handle--sw { cursor: nesw-resize; }
.handle--w { cursor: ew-resize; }

.pills {
  position: absolute;
  display: flex;
  gap: 8px;
  pointer-events: none;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 12px;
  border-radius: 13px;
  font-size: 12px;
  color: #f1f5f9;
  background: rgba(28, 36, 50, 0.94);
  box-shadow: 0 4px 14px rgba(15, 23, 42, 0.32);
  backdrop-filter: blur(10px);
}
.pill--size {
  font-weight: 600;
  background: #2f6bff;
}
.pill--tag {
  opacity: 0.9;
}
.pill-label {
  opacity: 0.72;
}
.pill-num {
  min-width: 18px;
  text-align: center;
  font-weight: 600;
}

.bar,
.style-bar,
.panel {
  position: absolute;
  display: flex;
  align-items: center;
  gap: 4px;
  border-radius: 22px;
  background: rgba(26, 33, 46, 0.95);
  box-shadow: 0 8px 24px rgba(15, 23, 42, 0.38);
  backdrop-filter: blur(12px);
  pointer-events: auto;
  white-space: nowrap;
}
.bar {
  padding: 6px 8px;
}
.style-bar {
  height: 34px;
  padding: 0 10px;
  gap: 6px;
  border-radius: 17px;
}
.panel {
  flex-direction: column;
  align-items: stretch;
  gap: 8px;
  padding: 10px 12px;
  border-radius: 14px;
  width: 216px;
}
.panel-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: #f1f5f9;
}
.radius {
  flex: 1;
  accent-color: #2f6bff;
}
.switch {
  position: relative;
  width: 30px;
  height: 16px;
  border: none;
  border-radius: 8px;
  background: rgba(148, 163, 184, 0.5);
  cursor: pointer;
  transition: background 0.15s;
}
.switch.on {
  background: #2f6bff;
}
.switch .knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: #fff;
  transition: transform 0.15s;
}
.switch.on .knob {
  transform: translateX(14px);
}

/* 取字面板：比圆角面板更宽更高；文本区必须单独允许选中（.cap-root 全局禁用了 user-select） */
.panel--ocr {
  width: 320px;
  gap: 8px;
}
.ocr-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: #f1f5f9;
  font-weight: 500;
}
.ocr-lang {
  padding: 1px 6px;
  border-radius: 6px;
  background: rgba(47, 107, 255, 0.3);
  font-size: 11px;
  font-weight: 400;
  opacity: 0.92;
}
.ocr-lang--warn {
  background: rgba(245, 158, 11, 0.28);
}
/* v0.9.0：增强引擎徽标用青绿区分（一眼看出「这次走的是本地模型」） */
.ocr-lang--enh {
  background: rgba(16, 185, 129, 0.32);
}
.ocr-hint {
  margin: 0;
  padding: 12px 0;
  font-size: 12px;
  line-height: 1.65;
  font-weight: 400;
  color: rgba(241, 245, 249, 0.72);
  white-space: normal;
}
.ocr-hint--err {
  color: #f7c1c1;
}
.ocr-text {
  height: 108px;
  padding: 8px 10px;
  border: 1px solid rgba(148, 163, 184, 0.3);
  border-radius: 8px;
  background: rgba(9, 14, 24, 0.62);
  color: #f1f5f9;
  font: 400 12px/1.6 ui-monospace, "Cascadia Mono", Consolas, monospace;
  white-space: pre-wrap;
  word-break: break-word;
  resize: none;
  user-select: text;
}
.ocr-text:focus {
  outline: none;
  border-color: #2f6bff;
}
.ocr-actions {
  display: flex;
  gap: 8px;
}
.mini {
  flex: 1;
  height: 28px;
  border: none;
  border-radius: 7px;
  background: #2f6bff;
  color: #fff;
  font-size: 12px;
  cursor: pointer;
}
.mini:disabled {
  background: rgba(148, 163, 184, 0.32);
  cursor: default;
}
.mini--ghost {
  flex: 0 0 62px;
  background: rgba(148, 163, 184, 0.22);
  color: #f1f5f9;
}
.tb.busy {
  opacity: 0.55;
}

.tb {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: #e5eaf2;
  cursor: pointer;
  transition: background 0.12s, transform 0.12s;
}
.tb svg {
  fill: none;
  stroke: currentColor;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.tb:hover:not(:disabled) {
  background: rgba(148, 163, 184, 0.22);
}
.tb:active:not(:disabled) {
  transform: scale(0.94);
}
.tb.on {
  background: #2f6bff;
  color: #fff;
}
.tb:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
.tb--cancel {
  color: #ff6b6b;
}
.tb--ok {
  background: #22c55e;
  color: #fff;
}
.tb--ok:hover {
  background: #16a34a;
}
.tb-sep {
  width: 1px;
  height: 20px;
  margin: 0 4px;
  background: rgba(148, 163, 184, 0.36);
}

.swatch {
  width: 18px;
  height: 18px;
  border: 2px solid rgba(255, 255, 255, 0.35);
  border-radius: 50%;
  cursor: pointer;
  padding: 0;
}
.swatch.on {
  border-color: #2f6bff;
  transform: scale(1.14);
}
.wbtn,
.sbtn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 24px;
  min-width: 24px;
  padding: 0 8px;
  border: none;
  border-radius: 12px;
  background: transparent;
  color: #e5eaf2;
  font-size: 12px;
  cursor: pointer;
}
.wbtn.on,
.sbtn.on {
  background: #2f6bff;
  color: #fff;
}
.wdot {
  display: block;
  border-radius: 50%;
  background: currentColor;
}
.sb-label {
  font-size: 12px;
  color: rgba(241, 245, 249, 0.72);
}

.text-edit {
  position: absolute;
  min-width: 160px;
  padding: 2px 6px;
  border: 1.5px dashed rgba(47, 107, 255, 0.9);
  border-radius: 4px;
  background: rgba(15, 23, 42, 0.45);
  font-family: -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif;
  font-weight: 600;
  outline: none;
}
.text-edit::placeholder {
  color: rgba(241, 245, 249, 0.55);
  font-weight: 400;
}
.hint {
  position: absolute;
  left: 50%;
  top: 40px;
  transform: translateX(-50%);
  padding: 8px 16px;
  border-radius: 16px;
  font-size: 13px;
  color: #f1f5f9;
  background: rgba(28, 36, 50, 0.9);
  backdrop-filter: blur(10px);
}
</style>
