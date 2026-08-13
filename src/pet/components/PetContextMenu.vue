<!--
  PetContextMenu.vue
  桌宠右键菜单（v0.6.0-beta.1 UI 重写版）。
  
  关键变更（UI优化版）：
  - 通过 Teleport 渲染到 body，不再受 pet 窗口 overflow:hidden 裁剪
  - 接收屏幕绝对坐标（outerPosition + clientX*scaleFactor）
  - 圆角毛玻璃卡片风格，机械师橙主题
  - 15 状态分 3 组：常用 / 情绪 / 特殊动作
  - 当前状态高亮 + ✓ 标识
  - 喂食 + 操作 分区

  功能：
  1. 手动切换 15 状态（覆盖自动监听）
  2. 喂食（竹叶/苹果/糖果，3 种食物）
  3. 重置位置 / 关闭桌宠

  修改历史：
    - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 右键菜单 + 喂食入口
    - 2026-07-17 @v0.6.0-beta.1: UI重写 - Teleport适配+分组美化+橙色主题+屏幕坐标
    - 2026-07-24 @v0.6.2: 解耦 - 加皮肤切换段（持久化由 skinRegistry 负责）
    - 2026-07-24 @v0.6.2-beta.5: 废弃 - 移除 panda-2d，皮肤列表由 skinRegistry.list() 动态渲染（现仅 popmart-3d）
    - 2026-08-08 @v0.6.2-34: 修复 - 菜单窗口也监听 pet-skin-changed 并调用 skinRegistry.reloadActive()，
      解决设置页切皮肤后右键菜单高亮不同步的问题。
-->
<template>
  <Transition name="menu-fade">
    <div
      v-if="visible"
      ref="menuEl"
      class="pet-menu"
      :style="menuStyle"
      :class="{ 'is-dragging': didMove }"
      @contextmenu.prevent
      @wheel.stop
      @pointerdown="onPointerDown"
      @click.capture="onCaptureClick"
    >
      <!-- v0.6.2-beta.17：右上角 × 只关闭菜单（不关桌宠），与底部「Hide Pet」分离 -->
      <button class="menu-close-btn" :title="t('common.close', 'Close')" @click="onCloseMenuOnly">
        <AppIcon name="x" />
      </button>

      <!-- 头部：当前状态 -->
      <div class="menu-header">
        <AppIcon name="paw" class="menu-icon" />
        <span class="menu-title">{{ t('pet.menu.title', 'ScreenTime Pet') }}</span>
        <span class="menu-current-state">{{ currentStateLabelText }}</span>
      </div>

      <!-- 自动模式按钮 -->
      <button
        v-if="store.override"
        class="menu-action auto-mode"
        @click="onClearOverride"
      >
        <AppIcon name="rotateCw" /> {{ t('pet.menu.autoMode', 'Auto Mode') }}
      </button>

      <!-- 分隔线 -->
      <div class="menu-divider" />

      <!-- 状态切换：分组显示 -->
      <div class="menu-section">
        <div class="section-label">{{ t('pet.menu.switchState', 'Expression') }}</div>

        <!-- 第一行：核心状态 -->
        <div class="state-grid">
          <button
            v-for="s in coreStates"
            :key="s"
            class="state-btn"
            :class="{ active: isActive(s) }"
            :title="t(`pet.state.${s}`)"
            @click="onPickState(s)"
          >
            <AppIcon :name="stateIcon(s)" class="state-emoji" />
            <span class="state-name">{{ t(`pet.state.${s}`) }}</span>
          </button>
        </div>

        <!-- 第二行：情绪状态 -->
        <div class="state-grid">
          <button
            v-for="s in emotionStates"
            :key="s"
            class="state-btn"
            :class="{ active: isActive(s) }"
            :title="t(`pet.state.${s}`)"
            @click="onPickState(s)"
          >
            <AppIcon :name="stateIcon(s)" class="state-emoji" />
            <span class="state-name">{{ t(`pet.state.${s}`) }}</span>
          </button>
        </div>

        <!-- 第三行：特殊状态 -->
        <div class="state-grid">
          <button
            v-for="s in specialStates"
            :key="s"
            class="state-btn"
            :class="{ active: isActive(s) }"
            :title="t(`pet.state.${s}`)"
            @click="onPickState(s)"
          >
            <AppIcon :name="stateIcon(s)" class="state-emoji" />
            <span class="state-name">{{ t(`pet.state.${s}`) }}</span>
          </button>
        </div>
      </div>

      <div class="menu-divider" />

      <!-- 喂食区 -->
      <div class="menu-section">
        <div class="section-label">
          {{ t('pet.feed.title', 'Food') }}
          <span class="fullness-bar">
            <span class="fullness-fill" :style="{ width: `${store.fullness}%` }"></span>
            {{ store.fullness }}/100
          </span>
        </div>
        <div class="feed-row">
          <button
            v-for="f in foods"
            :key="f.id"
            class="feed-btn"
            :disabled="!store.canFeedToday"
            @click="onFeed(f.value, f.id)"
          >
            <AppIcon :name="f.icon" class="feed-emoji" />
            <span>{{ t(`pet.feed.${f.id}`, f.id) }}</span>
            <span class="feed-value">+{{ f.value }}</span>
          </button>
        </div>
        <!-- v0.7.0：喂食即时反馈——菜单不自动关闭，飘出 +N 提示，饱食度条同步增长 -->
        <transition name="feed-toast">
          <div v-if="feedToast > 0" class="feed-toast">
            <AppIcon name="utensils" :size="13" />
            {{ t('pet.feed.success', { n: feedToast }) }}
          </div>
        </transition>
        <p v-if="!store.canFeedToday" class="menu-hint">
          {{ t('pet.feed.todayLimit', 'Daily limit reached') }}
        </p>
      </div>

      <div class="menu-divider" />

      <!-- v0.6.2：皮肤切换段（解耦皮肤机制；持久化由 skinRegistry 负责） -->
      <div class="menu-section">
        <div class="section-label">{{ t('pet.settings.skinTitle', 'Skin') }}</div>
        <div class="skin-row">
          <button
            v-for="s in skins"
            :key="s.id"
            class="skin-btn"
            :class="{ active: isActiveSkin(s.id) }"
            :title="s.description || s.name"
            @click="onPickSkin(s.id)"
          >
            <span class="skin-name">{{ s.name }}</span>
          </button>
        </div>
      </div>

      <div class="menu-divider" />

      <!-- 操作区 -->
      <div class="menu-actions">
        <button class="action-btn" @click="onResetPos">
          <AppIcon name="rotateCcw" /> {{ t('pet.settings.resetPos', 'Reset Position') }}
        </button>
        <button class="action-btn danger" @click="onClose">
          <AppIcon name="x" /> {{ t('pet.settings.close', 'Hide Pet') }}
        </button>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { emit as emitEvent, listen } from '@tauri-apps/api/event';
import { petStore as store } from '../stores/petStore';
import { STATE_ICON } from '../engine/stateIcons';
import { skinRegistry } from '../skins/registry';
import type { PetSkinManifest } from '../skins/types';
import type { PetState } from '../types';

const { t } = useI18n();

const props = withDefaults(
  defineProps<{
    x: number;
    y: number;
    /**
     * 渲染模式：
     * - 'embedded'（默认）：嵌在 PetWindow webview 内，drag 用 transform: translate3d；
     * - 'windowed'：独立 pet-menu 窗口，drag 调用 invoke('move_pet_menu_window') 改外层窗口位置
     */
    mode?: 'embedded' | 'windowed';
  }>(),
  { mode: 'embedded' },
);
const isWindowed = computed<boolean>(() => props.mode === 'windowed');
const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'feed', result: { ok: boolean; reason?: string }): void;
}>();

const visible = ref(true);
const menuEl = ref<HTMLElement | null>(null);
// v0.7.0：喂食即时反馈提示（+N 饱食度），1.3s 后自动消失
const feedToast = ref(0);

// ---- 状态分组 ----
const coreStates: PetState[] = ['idle', 'working', 'developing', 'designing', 'gaming'];
const emotionStates: PetState[] = ['happy', 'sad', 'angry', 'surprised', 'slacking'];
const specialStates: PetState[] = ['chatting', 'meeting', 'listening', 'eating', 'sleeping'];

function isActive(s: PetState): boolean {
  return store.override === s || (store.override === null && store.state === s);
}

function currentStateLabel(): string {
  const s = store.override || store.state;
  return t(`pet.state.${s}`, s);
}
const currentStateLabelText = computed<string>(() => currentStateLabel());

// ---- 状态 → 矢量图标名 ----
function stateIcon(s: PetState): string {
  return STATE_ICON[s] ?? 'tool';
}

// ---- 喂食数据 ----
interface FoodItem {
  id: 'bamboo' | 'apple' | 'candy';
  value: number;
  icon: string;
}
const foods: FoodItem[] = [
  { id: 'bamboo', value: 20, icon: 'leaf' },
  { id: 'apple', value: 15, icon: 'apple' },
  { id: 'candy', value: 10, icon: 'cookie' },
];

// ---- 皮肤切换（v0.6.2） ----
const skins = computed<PetSkinManifest[]>(() => skinRegistry.list());
// v0.6.2-35: 用 computed 直接跟踪 skinRegistry.active()，避免 ref 初始化后错过跨窗口同步。
// 右键菜单运行在独立 pet-menu webview，Tauri 多窗口间模块级 reactive 不共享；
// 设置页切皮后会广播 pet-skin-changed，本窗口监听后 reloadActive() 更新 registry，
// computed 会自动反映最新 active id，无需手动维护 ref + subscribe。
const activeSkinId = computed<string>(() => skinRegistry.active().id);
function isActiveSkin(id: string): boolean {
  return activeSkinId.value === id;
}
function onPickSkin(id: string): void {
  if (skinRegistry.setActive(id)) {
    // 关菜单，让用户看到完整皮肤切换效果
    emit('close');
  }
}
// 组件挂载时监听 Tauri 全局广播（设置页/其他窗口切皮肤）
let unlistenSkin: (() => void) | null = null;
// v0.7.2：菜单打开期间，若桌宠自动状态变化 / 其他窗口切换启用状态，菜单需同步刷新显示
let unlistenStoreSync: (() => void) | null = null;
let unlistenEnabled: (() => void) | null = null;
onMounted(async () => {
  try {
    unlistenSkin = await listen('pet-skin-changed', () => {
      // 设置页切皮肤后广播，菜单窗口的 skinRegistry 需从 localStorage 重读对齐
      skinRegistry.reloadActive();
    });
    unlistenStoreSync = await listen('pet-store-updated', () => {
      // 桌宠窗/其他来源改了 override/饱食度/位置 → 菜单同步最新显示
      store.reload();
    });
    unlistenEnabled = await listen('pet-enabled-changed', () => {
      // 设置页/菜单切换桌宠启用状态 → 菜单同步
      store.reload();
    });
  } catch (e) {
    console.warn('[PetContextMenu] 监听失败', e);
  }
});
// 组件卸载时取消订阅
onBeforeUnmount(() => {
  if (unlistenSkin) unlistenSkin();
  if (unlistenStoreSync) unlistenStoreSync();
  if (unlistenEnabled) unlistenEnabled();
});

// ---- 操作 ----
/**
 * v0.6.2-beta.32：菜单跑在独立 pet-menu 窗口，store 与桌宠窗口不共享。
 * 任何改 store 的操作后广播 pet-store-updated，让桌宠窗口 reload() 同步
 * （override/喂食/位置），否则在菜单里切状态/喂食，桌宠表情与饱食度不会变。
 */
function notifyStoreSync(): void {
  try {
    void emitEvent('pet-store-updated');
  } catch {
    /* ignore */
  }
}
function onPickState(s: PetState): void {
  store.setOverride(s);
  notifyStoreSync();
  emit('close');
}
function onClearOverride(): void {
  store.setOverride(null);
  notifyStoreSync();
  emit('close');
}
function onFeed(value: number, foodId: string): void {
  const r = store.feed(value);
  emit('feed', r);
  if (r.ok) {
    // v0.7.0：菜单不自动关闭，飘出 +N 提示，让用户直接看到饱食度条增长（修复「喂食无反馈/饿度不变」）
    feedToast.value = value;
    window.setTimeout(() => {
      feedToast.value = 0;
    }, 1300);
    // 同步到桌宠窗口（饱食度 + 触发进食/开心反应）
    notifyStoreSync();
    try {
      void emitEvent('pet-fed', { value, foodId });
    } catch {
      /* ignore */
    }
    // 注意：不再 emit('close')，避免菜单瞬间关闭导致反馈不可见
  }
}
function onResetPos(): void {
  const sw = window.screen.width;
  const sh = window.screen.height;
  store.setPosition(sw - 200, sh - 240);
  invoke('move_pet_window', { x: store.position.x, y: store.position.y }).catch(() => {});
  notifyStoreSync();
  emit('close');
}
function onClose(): void {
  // v0.6.2-beta.6：菜单底部「Hide Pet」按钮 = 关闭桌宠本身
  // 翻转桌宠启用状态并持久化（修复：右键菜单关闭后，设置页桌宠开关未同步关闭）
  store.setEnabled(false);
  invoke('hide_pet_window').catch(() => {});
  // 跨窗口广播（Tauri 全局事件）：通知 Settings 重读 petStore 以联动关开关
  try {
    void emitEvent('pet-enabled-changed');
  } catch {
    /* ignore */
  }
  emit('close');
}

function onCloseMenuOnly(): void {
  // v0.6.2-beta.17：右上角 × = 只关闭菜单，不影响桌宠
  visible.value = false;
  emit('close');
}

// ---- 菜单拖动（v0.6.2-beta.15） ----
// 菜单初始位置 = props.x/y；拖动用 translate3d 偏移；松手时把累计偏移加到 baseX/baseY
// 然后清空 delta；这样菜单不会自动滑回原位，下次拖动以"最后停下的位置"为基准。
const basePos = ref({ x: 0, y: 0 });
const dragDelta = ref({ x: 0, y: 0 });
let downX = 0;
let downY = 0;
let pointerId: number | null = null;
let isDragging = false;
let didMove = false;

function onCaptureClick(e: MouseEvent): void {
  // v0.6.2-beta.15：若本次点击来源于拖动（拖松后 clickSuppressed 标志位），
  // 阻止 click 冒泡到按钮，避错误触发。
  if (clickSuppressed) {
    e.stopPropagation();
    e.preventDefault();
  }
}

function onPointerDown(e: PointerEvent): void {
  if (e.button !== 0) return;
  const target = e.target as HTMLElement;
  // 按钮/可交互控件不接管（让按钮自己处理点击）
  if (target.closest('button, input, select, label')) return;
  isDragging = true;
  didMove = false;
  pointerId = e.pointerId;
  downX = e.clientX;
  downY = e.clientY;
  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', onPointerUp, { once: true });
  window.addEventListener('pointercancel', onPointerUp, { once: true });
}

function onPointerMove(e: PointerEvent): void {
  if (!isDragging || e.pointerId !== pointerId) return;
  const dx = e.clientX - downX;
  const dy = e.clientY - downY;
  if (!didMove && Math.hypot(dx, dy) < 6) return;
  didMove = true;
  if (isWindowed.value) {
    // 独立窗口模式：直接移动整窗（basePos 记录窗口屏幕逻辑坐标），菜单本地 (0,0) 跟随窗口；
    // 不再叠加 translate3d（窗口移动已带动菜单，叠加会导致双倍位移/抖动）。
    const nextX = basePos.value.x + dx;
    const nextY = basePos.value.y + dy;
    basePos.value = { x: nextX, y: nextY };
    dragDelta.value = { x: 0, y: 0 };
    invoke('move_pet_menu_window', { x: nextX, y: nextY }).catch(() => {});
  } else {
    // 嵌入模式：改 transform: translate3d
    dragDelta.value = { x: dx, y: dy };
  }
}

function onPointerUp(e: PointerEvent): void {
  if (e.pointerId !== pointerId) return;
  isDragging = false;
  pointerId = null;
  window.removeEventListener('pointermove', onPointerMove);
  if (didMove) {
    if (isWindowed.value) {
      // 独立窗口模式：把当前累计偏移合并到 basePos（窗口已搬到新位置，
      // 本地 basePos 对齐，下次拖动以这里为基准）
      basePos.value = {
        x: basePos.value.x + dragDelta.value.x,
        y: basePos.value.y + dragDelta.value.y,
      };
    } else {
      // 嵌入模式：合并 transform 偏移
      basePos.value = {
        x: basePos.value.x + dragDelta.value.x,
        y: basePos.value.y + dragDelta.value.y,
      };
    }
    dragDelta.value = { x: 0, y: 0 };
    suppressNextClick();
  }
}

let clickSuppressed = false;
function suppressNextClick(): void {
  clickSuppressed = true;
  setTimeout(() => {
    clickSuppressed = false;
  }, 50);
}

const menuStyle = computed(() => {
  if (isWindowed.value) {
    // 独立窗口模式：菜单固定在窗口本地 (0,0)，整窗由 move_pet_menu_window 移动。
    // 不能用屏幕坐标当 left/top（小窗 300×620 会把菜单挤出可见区 → 看似"打不开"）。
    return { left: '0px', top: '0px', transform: 'none' };
  }
  return {
    left: `${basePos.value.x}px`,
    top: `${basePos.value.y}px`,
    transform: didMove
      ? `translate3d(${dragDelta.value.x}px, ${dragDelta.value.y}px, 0)`
      : 'none',
  };
});

// 首次挂载时初始化 basePos
// - embedded：取 props.x/y（pet 窗口内坐标）
// - windowed：通过 invoke 读 pet-menu 窗口的 outerPosition（屏幕绝对坐标）
onMounted(async () => {
  if (isWindowed.value) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const pos = await getCurrentWindow().outerPosition();
      // outerPosition 是物理像素；前端按 scaleFactor 转逻辑像素（拖动用逻辑像素）
      const sf = await getCurrentWindow().scaleFactor();
      basePos.value = {
        x: pos.x / sf,
        y: pos.y / sf,
      };
      dragDelta.value = { x: 0, y: 0 };
    } catch (e) {
      console.warn('[pet-menu] 初始化窗口位置失败', e);
      basePos.value = { x: 0, y: 0 };
    }
  } else if (basePos.value.x === 0 && basePos.value.y === 0) {
    basePos.value = { x: props.x, y: props.y };
  }
});

// ---- 点击外部关闭 ----
onMounted(() => {
  // 用 setTimeout 让本次 click 不触发关闭
  setTimeout(() => {
    document.addEventListener('pointerdown', onOutsideClick);
  }, 100);
});
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onOutsideClick);
});
function onOutsideClick(e: Event): void {
  const target = e.target as HTMLElement;
  if (!target.closest('.pet-menu')) {
    visible.value = false;
    emit('close');
  }
}

// ---- ESC 键关闭 ----
function onEsc(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    visible.value = false;
    emit('close');
  }
}
onMounted(() => {
  document.addEventListener('keydown', onEsc);
});
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onEsc);
});
</script>

<style scoped>
.pet-menu {
  position: fixed;
  z-index: 99999;
  min-width: 200px;
  max-width: 280px;
  background: rgba(24, 24, 27, 0.95);
  border: 1px solid rgba(255, 126, 39, 0.25);
  border-radius: 12px;
  padding: 8px 0;
  box-shadow:
    0 12px 40px rgba(0, 0, 0, 0.5),
    0 0 0 1px rgba(255, 255, 255, 0.05) inset;
  font-size: 13px;
  color: #e8e8e8;
  user-select: none;
  -webkit-user-select: none;
  font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif;
  animation: menu-in 0.15s ease-out;
}

@keyframes menu-in {
  from { opacity: 0; transform: scale(0.96) translateY(-4px); }
  to { opacity: 1; transform: scale(1) translateY(0); }
}

/* ---- 头部 ---- */
.menu-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 36px 4px 14px; /* 右侧 36px 留出 × 按钮位置 */
}
.menu-icon {
  font-size: 16px;
}
.menu-title {
  font-weight: 600;
  font-size: 13px;
  color: #fff;
  flex: 1;
}
.menu-current-state {
  font-size: 11px;
  color: #FF7E27;
  background: rgba(255, 126, 39, 0.12);
  padding: 2px 8px;
  border-radius: 10px;
  white-space: nowrap;
}

/* v0.6.2-beta.15：右上角独立关闭按钮 */
.menu-close-btn {
  position: absolute;
  top: 6px;
  right: 8px;
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 50%;
  color: rgba(255, 255, 255, 0.6);
  cursor: pointer;
  font-size: 14px;
  padding: 0;
  z-index: 2;
  transition: background 0.12s, color 0.12s;
}
.menu-close-btn:hover {
  background: rgba(255, 90, 90, 0.16);
  border-color: rgba(255, 90, 90, 0.32);
  color: #ff5e5e;
}
.menu-close-btn :deep(svg) {
  width: 12px;
  height: 12px;
}
/* 拖动中光标 */
.pet-menu.is-dragging,
.pet-menu.is-dragging * {
  cursor: grabbing !important;
}

/* ---- 自动模式按钮 ---- */
.menu-action.auto-mode {
  display: flex;
  align-items: center;
  gap: 4px;
  width: calc(100% - 28px);
  margin: 4px 14px;
  padding: 5px 10px;
  background: rgba(255, 126, 39, 0.08);
  border: 1px solid rgba(255, 126, 39, 0.2);
  border-radius: 6px;
  color: #FF7E27;
  font-size: 12px;
  cursor: pointer;
  transition: background 0.15s;
}
.menu-action.auto-mode:hover {
  background: rgba(255, 126, 39, 0.16);
}

/* ---- 分隔线 ---- */
.menu-divider {
  height: 1px;
  background: linear-gradient(to right, transparent, rgba(255,255,255,0.08), transparent);
  margin: 6px 12px;
}

/* ---- 区块标签 ---- */
.section-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 10px;
  color: rgba(255, 255, 255, 0.4);
  text-transform: uppercase;
  letter-spacing: 0.8px;
  padding: 4px 14px 6px;
  font-weight: 600;
}

/* ---- 状态网格 ---- */
.state-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 3px;
  padding: 0 10px 6px;
}
.state-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
  padding: 5px 8px;
  min-width: 56px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 7px;
  cursor: pointer;
  transition: all 0.12s ease;
  color: rgba(255, 255, 255, 0.75);
  font-size: 11px;
}
.state-btn:hover {
  background: rgba(255, 255, 255, 0.06);
  border-color: rgba(255, 255, 255, 0.08);
  color: #fff;
}
.state-btn.active {
  background: rgba(255, 126, 39, 0.12);
  border-color: rgba(255, 126, 39, 0.35);
  color: #FF7E27;
}
.state-btn .state-emoji {
  font-size: 17px;
  line-height: 1;
}
.state-btn .state-name {
  font-size: 9px;
  white-space: nowrap;
  line-height: 1.2;
}

/* ---- 饱食度条 ---- */
.fullness-bar {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  width: 60px;
  height: 4px;
  background: rgba(255, 255, 255, 0.08);
  border-radius: 2px;
  overflow: hidden;
  font-size: 0;
}
.fullness-fill {
  height: 100%;
  background: linear-gradient(90deg, #FF7E27, #ffb347);
  border-radius: 2px;
  transition: width 0.3s ease;
}

/* ---- 喂食行 ---- */
.feed-row {
  display: flex;
  gap: 4px;
  padding: 0 10px 4px;
}
.feed-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 3px;
  padding: 6px 4px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 7px;
  color: rgba(255, 255, 255, 0.8);
  font-size: 11px;
  cursor: pointer;
  transition: all 0.12s;
}
.feed-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 126, 39, 0.2);
}
.feed-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
.feed-btn .feed-emoji {
  font-size: 14px;
}
.feed-btn .feed-value {
  font-size: 9px;
  color: rgba(255, 126, 39, 0.8);
}

/* ---- 皮肤切换行（v0.6.2） ---- */
.skin-row {
  display: flex;
  gap: 4px;
  padding: 0 10px 4px;
}
.skin-btn {
  flex: 1;
  padding: 6px 4px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 7px;
  color: rgba(255, 255, 255, 0.75);
  font-size: 11px;
  cursor: pointer;
  transition: all 0.12s ease;
}
.skin-btn:hover {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 126, 39, 0.2);
}
.skin-btn.active {
  background: rgba(255, 126, 39, 0.16);
  border-color: rgba(255, 126, 39, 0.5);
  color: #FF7E27;
}
.skin-name { font-weight: 500; }

/* ---- 提示 ---- */
.menu-hint {
  font-size: 11px;
  color: rgba(255, 165, 67, 0.85);
  padding: 2px 14px 6px;
  margin: 0;
}

/* ---- 喂食即时反馈（v0.7.0） ---- */
.feed-toast {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 4px 14px 2px;
  padding: 5px 10px;
  background: rgba(255, 126, 39, 0.16);
  border: 1px solid rgba(255, 126, 39, 0.32);
  border-radius: 8px;
  color: #ffb347;
  font-size: 12px;
  font-weight: 600;
}
.feed-toast :deep(svg) { color: #ffb347; }
.feed-toast-enter-active,
.feed-toast-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}
.feed-toast-enter-from,
.feed-toast-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

/* ---- 底部操作 ---- */
.menu-actions {
  display: flex;
  gap: 6px;
  padding: 4px 10px 6px;
}
.action-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 6px 10px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 7px;
  color: rgba(255, 255, 255, 0.7);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.12s;
}
.action-btn:hover {
  background: rgba(255, 255, 255, 0.08);
  color: #fff;
}
.action-btn.danger:hover {
  background: rgba(255, 90, 90, 0.12);
  border-color: rgba(255, 90, 90, 0.25);
  color: #ff5e5e;
}

/* ---- Transition ---- */
.menu-fade-enter-active,
.menu-fade-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.menu-fade-enter-from,
.menu-fade-leave-to {
  opacity: 0;
  transform: scale(0.95);
}
</style>
