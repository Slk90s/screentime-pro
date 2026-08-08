<!--
  PetSpriteEditor.vue
  3D 桌宠「表情（emoji）」编辑器（v0.6.2-beta.6 重构）。

  重构说明：
  - v0.6.2-beta.5 起 2D 熊猫皮肤已移除，原「部件合成 + 拖拽 + 文件替换 + 导入导出」编辑器失去意义
  - 现仅保留「按状态配置 emoji 表情」能力（3D Pop Mart 桌宠的状态差异本就由 emoji 浮层表达）
  - 已删除：Body/Eyes/Mouth/Brows/Decorations 素材面板、拖拽合成、文件替换、配置导入导出

  交互：
  1. 左侧实时预览：复用注册表当前活跃皮肤的渲染器（skinRegistry.active().renderer，与真实桌宠同一渲染器，
     切换皮肤自动重建），按选中状态显示 emoji 浮层；下方「互动预览」可触发各点击反应（含蜘蛛侠射蛛丝）。
  2. 右侧：状态选择 + emoji 输入框 + 常用表情快速点选
  3. 保存 → 写入 localStorage 并广播 pet-custom-updated，实时桌宠（独立 webview）跨窗口同步

  修改历史：
    - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 2D 部件合成编辑器
    - 2026-07-24 @v0.6.2-beta.6: 重构 - 删除 2D 部件编辑，改为 3D 桌宠按状态配置 emoji 表情
    - 2026-08-07 @v0.6.2-beta.31: 优化 - 预览改为皮肤感知（修掉写死熊猫导致蜘蛛侠皮肤预览错误）；
      新增「互动预览」区（触发 jump/squash/jolt/web 反应）；清理 i18n（补全缺失 key、修正标题）
    - 2026-08-08 @v0.6.2-36: 优化 - 蜘蛛侠皮肤新增「待机小动作」预览（荡丝 / 摆 pose / 蹲防 / 射蛛丝），
      通过 previewAction prop 驱动姿势图切换；与「点击反应」分区，动作覆盖更全面。
-->
<template>
  <Transition name="editor-fade">
    <div v-if="visible" class="editor-overlay" @click.self="onClose" @wheel.stop>
      <div class="editor-panel">
        <!-- 头部 -->
        <div class="editor-header">
          <h3><AppIcon name="smile" /> {{ t('pet.editor.title', 'Expression Editor') }}</h3>
          <button class="close-btn" @click="onClose"><AppIcon name="x" /></button>
        </div>

        <!-- 工具栏 -->
        <div class="editor-toolbar">
          <label class="toolbar-label">{{ t('pet.editor.state', 'State') }}</label>
          <select v-model="selectedState" class="state-select">
            <option v-for="s in ALL_STATES" :key="s" :value="s">{{ t(`pet.state.${s}`, s) }}</option>
          </select>
          <span class="toolbar-spacer" />
          <span v-if="savedTip" class="saved-tip"><AppIcon name="check" /> {{ t('pet.editor.saved', 'Saved') }}</span>
          <button class="tool-btn" @click="onSave">
            <AppIcon name="save" /> {{ t('pet.editor.save', 'Save') }}
          </button>
          <button class="tool-btn danger" :disabled="!hasCustomContent()" @click="onResetAll">
            <AppIcon name="trash" /> {{ t('pet.editor.reset', 'Reset All') }}
          </button>
        </div>

        <!-- 主区域：左预览 + 右表情配置 -->
        <div class="editor-body">
          <!-- 左侧：实时预览（与真实桌宠同渲染器，皮肤感知） -->
          <div class="preview-area">
            <div class="preview-label">{{ t('pet.editor.preview', 'Preview') }}</div>
            <div class="preview-stage">
              <component
                v-if="previewRenderer"
                :is="previewRenderer"
                :key="previewSkinTick"
                :state="selectedState"
                :override-badges="previewBadges"
                :class="previewAnim"
                v-bind="isSpiderman ? { 'preview-action': previewIdleAction } : {}"
              />
            </div>
            <div class="preview-state-name">{{ t(`pet.state.${selectedState}`, selectedState) }}</div>

            <!-- 互动预览：触发各点击反应，直观看到表情动画（蜘蛛侠含射蛛丝） -->
            <div class="reaction-test">
              <div class="reaction-test__title">{{ t('pet.editor.reactionTest', '点击反应') }}</div>
              <div class="reaction-test__row">
                <button class="react-btn" @click="triggerReaction('pet-anim-jump')">{{ t('pet.editor.reactionJump', '跳跃') }}</button>
                <button class="react-btn" @click="triggerReaction('pet-anim-squash')">{{ t('pet.editor.reactionSquash', '弹压') }}</button>
                <button class="react-btn" @click="triggerReaction('pet-anim-jolt')">{{ t('pet.editor.reactionJolt', '抖动') }}</button>
                <button v-if="isSpiderman" class="react-btn web" @click="triggerReaction('pet-anim-web')">{{ t('pet.editor.reactionWeb', '射蛛丝') }}</button>
              </div>
              <p v-if="isSpiderman" class="reaction-note">
                {{ t('pet.editor.skinWebNote', '蜘蛛侠皮肤：单击桌宠会轮流触发 跳跃 / 弹压 / 抖动 / 射蛛丝') }}
              </p>
            </div>

            <!-- 蜘蛛侠待机小动作预览：让编辑者能直接看到 swing / pose / crouch / web 四个姿势（v0.6.2-36） -->
            <div v-if="isSpiderman" class="reaction-test idle-actions">
              <div class="reaction-test__title">{{ t('pet.editor.idleTest', '待机小动作') }}</div>
              <div class="reaction-test__row">
                <button class="react-btn" @click="triggerIdleAction('spidey-action-swing')">{{ t('pet.editor.idleSwing', '荡丝') }}</button>
                <button class="react-btn" @click="triggerIdleAction('spidey-action-pose')">{{ t('pet.editor.idlePose', '摆 pose') }}</button>
                <button class="react-btn" @click="triggerIdleAction('spidey-action-crouch')">{{ t('pet.editor.idleCrouch', '蹲防') }}</button>
                <button class="react-btn web" @click="triggerIdleAction('spidey-action-web')">{{ t('pet.editor.idleWeb', '射蛛丝') }}</button>
              </div>
            </div>

            <p class="preview-tip">
              <AppIcon name="paw" /> {{ t('pet.editor.previewTip', '实时预览当前皮肤的桌宠') }}
            </p>
          </div>

          <!-- 右侧：表情配置 -->
          <div class="config-area">
            <div class="config-title">{{ t('pet.editor.expression', 'Expression') }}</div>
            <input
              v-model="editingEmoji"
              class="emoji-input"
              :placeholder="t('pet.editor.emojiPlaceholder', 'Type emoji(s), e.g. 💼 ♥ ✨')"
              maxlength="12"
              @input="onEmojiInput"
            />
            <p class="config-hint">
              {{ t('pet.editor.emojiHint', 'Leave empty to use the built-in default. Multiple emoji are allowed.') }}
            </p>

            <!-- 常用表情快速点选 -->
            <div class="quick-title">{{ t('pet.editor.quickPick', 'Quick pick') }}</div>
            <div class="quick-grid">
              <button
                v-for="q in quickEmojis"
                :key="q"
                class="quick-chip"
                :title="q"
                @click="appendEmoji(q)"
              >{{ q }}</button>
            </div>

            <button class="clear-btn" @click="onClearState">
              <AppIcon name="rotateCcw" /> {{ t('pet.editor.clearState', 'Reset this state to default') }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, computed, watch, watchEffect, type Component } from 'vue';
import { useI18n } from 'vue-i18n';
import { ALL_STATES } from '../engine/stateMachine';
import { skinRegistry } from '../skins/registry';
import type { PetState } from '../types';
import { usePetBadges } from '../composables/usePetBadges';
import { emit as emitGlobal } from '@tauri-apps/api/event';

const { t } = useI18n();
const { getCustomBadge, setCustomBadge, removeCustomBadge, resetAll, hasCustomContent, persistNow } = usePetBadges();

defineProps<{ visible: boolean }>();
const emit = defineEmits<{ (e: 'close'): void }>();

// ---- 编辑状态 ----
const selectedState = ref<PetState>('idle');
const editingEmoji = ref('');
const savedTip = ref(false);

// ---- 皮肤感知预览（复用注册表当前活跃皮肤渲染器，切换皮肤自动重建） ----
const previewRenderer = ref<Component | null>(null);
const previewSkinTick = ref(0);
const isSpiderman = ref(false);
watchEffect(() => {
  const m = skinRegistry.active();
  isSpiderman.value = m.id === 'spiderman';
  if (m.renderer !== previewRenderer.value) {
    previewRenderer.value = m.renderer;
    previewSkinTick.value++;
  }
});

// ---- 互动预览：触发点击反应动画（在预览中播放，便于直观看到表情） ----
const previewAnim = ref('');
let reactionTimer: number | null = null;
function triggerReaction(anim: string): void {
  if (reactionTimer !== null) clearTimeout(reactionTimer);
  // 点击反应与待机小动作互斥，避免 pose 图与 click 动画叠加
  previewIdleAction.value = '';
  if (idleTimer !== null) { clearTimeout(idleTimer); idleTimer = null; }
  // 先清空再于下一帧赋值，确保同一反应可重复触发
  previewAnim.value = '';
  requestAnimationFrame(() => {
    previewAnim.value = anim;
    reactionTimer = window.setTimeout(() => { previewAnim.value = ''; }, 900);
  });
}

// ---- 待机小动作预览（蜘蛛侠专属）：直接驱动 SpiderManPet 的姿势图切换 ----
const previewIdleAction = ref('');
let idleTimer: number | null = null;
const IDLE_ACTION_DURATIONS: Record<string, number> = {
  'spidey-action-swing': 1700,
  'spidey-action-pose': 1000,
  'spidey-action-crouch': 1000,
  'spidey-action-web': 700,
};
function triggerIdleAction(action: string): void {
  if (idleTimer !== null) clearTimeout(idleTimer);
  // 与点击反应互斥
  previewAnim.value = '';
  if (reactionTimer !== null) { clearTimeout(reactionTimer); reactionTimer = null; }
  previewIdleAction.value = '';
  requestAnimationFrame(() => {
    previewIdleAction.value = action;
    idleTimer = window.setTimeout(() => { previewIdleAction.value = ''; }, IDLE_ACTION_DURATIONS[action] ?? 1000);
  });
}

// 预览：把当前输入按字形切分喂给渲染器（空串 → 不覆盖，渲染器回退默认）
const previewBadges = computed<string[] | null>(() => {
  const v = editingEmoji.value.trim();
  return v ? Array.from(v) : null;
});

// 切换状态时载入已保存的自定义表情（无则空串 = 默认）
function loadState(): void {
  const saved = getCustomBadge(selectedState.value);
  editingEmoji.value = saved ?? '';
  savedTip.value = false;
}
watch(selectedState, loadState, { immediate: true });

function onEmojiInput(): void {
  savedTip.value = false;
}

// ---- 常用表情 ----
// 长度 ≤ 1 个 emoji 字符；多字符组合如 "(＾▿＾)" 放 chip 会撑爆 grid 单元，应在输入框手写。
const quickEmojis = ['💼', '💬', '🎮', '✎', '♥', '✨', '💧', '💢', '🍎', '♪', '…', '!', 'Z', 'z'];
function appendEmoji(q: string): void {
  const cur = editingEmoji.value;
  editingEmoji.value = (cur + q).slice(0, 12);
  savedTip.value = false;
}

// ---- 操作 ----
function notifyPet(): void {
  persistNow();
  emitGlobal('pet-custom-updated').catch(() => {});
}
function onSave(): void {
  setCustomBadge(selectedState.value, editingEmoji.value);
  notifyPet();
  savedTip.value = true;
  setTimeout(() => { savedTip.value = false; }, 1500);
}
function onClearState(): void {
  removeCustomBadge(selectedState.value);
  editingEmoji.value = '';
  notifyPet();
}
function onResetAll(): void {
  if (!confirm(t('pet.editor.resetConfirm', 'Reset all custom expressions?'))) return;
  resetAll();
  loadState();
  notifyPet();
}
function onClose(): void {
  emit('close');
}
</script>

<style scoped>
.editor-overlay {
  position: fixed;
  inset: 0;
  z-index: 100000;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  backdrop-filter: blur(8px);
}
.editor-panel {
  width: min(720px, 94vw);
  max-height: 88vh;
  background: rgba(28, 28, 30, 0.97);
  border: 1px solid rgba(255, 126, 39, 0.2);
  border-radius: 16px;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(255, 255, 255, 0.04) inset;
  overflow: hidden;
  font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif;
  animation: editor-in 0.2s ease-out;
}
@keyframes editor-in {
  from { opacity: 0; transform: scale(0.95) translateY(12px); }
  to { opacity: 1; transform: scale(1) translateY(0); }
}

/* 头部 */
.editor-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}
.editor-header h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
  color: #fff;
  display: flex;
  align-items: center;
  gap: 8px;
}
.close-btn {
  width: 28px; height: 28px;
  border-radius: 50%;
  border: none;
  background: rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.7);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}
.close-btn:hover { background: rgba(255, 90, 90, 0.15); color: #ff5e5e; }

/* 工具栏 */
.editor-toolbar {
  display: flex;
  gap: 8px;
  padding: 10px 14px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  align-items: center;
}
.toolbar-label {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.5);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}
.toolbar-spacer { flex: 1; }
.saved-tip {
  font-size: 11px;
  color: rgba(165, 211, 67, 0.9);
  display: inline-flex;
  align-items: center;
  gap: 3px;
}
.state-select {
  min-width: 160px;
  padding: 6px 10px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  color: #e8e8e8;
  font-size: 13px;
  outline: none;
  cursor: pointer;
}
.state-select:focus { border-color: #FF7E27; box-shadow: 0 0 0 2px rgba(255, 126, 39, 0.15); }
.tool-btn {
  padding: 6px 14px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  background: rgba(255, 126, 39, 0.08);
  color: #FF7E27;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.12s;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.tool-btn:hover:not(:disabled) { background: rgba(255, 126, 39, 0.16); }
.tool-btn:disabled { opacity: 0.35; cursor: not-allowed; }
.tool-btn.danger:hover:not(:disabled) {
  background: rgba(255, 90, 90, 0.12); color: #ff5e5e; border-color: rgba(255, 90, 90, 0.2);
}

/* 主区域 */
.editor-body {
  display: flex;
  gap: 0;
  flex: 1;
  overflow-y: auto;
  padding: 14px;
}

/* 预览区 */
.preview-area {
  width: 200px;
  min-width: 180px;
  text-align: center;
  border-right: 1px solid rgba(255, 255, 255, 0.05);
  padding-right: 14px;
  display: flex;
  flex-direction: column;
  align-items: center;
}
.preview-label {
  font-size: 10px;
  color: rgba(255, 255, 255, 0.35);
  text-transform: uppercase;
  letter-spacing: 0.8px;
  margin-bottom: 10px;
}
.preview-stage {
  width: 150px;
  height: 330px;
  margin: 0 auto 10px;
  background:
    radial-gradient(circle at 50% 45%, rgba(255, 126, 39, 0.06), transparent 70%),
    rgba(255, 255, 255, 0.03);
  border: 1px dashed rgba(255, 255, 255, 0.12);
  border-radius: 16px;
  overflow: visible; /* 让蜘蛛侠射出的蛛丝网不被裁切 */
  display: flex;
  align-items: center;
  justify-content: center;
}
.preview-state-name {
  font-size: 13px;
  color: #FF7E27;
  font-weight: 600;
}
.preview-tip {
  font-size: 10.5px;
  color: rgba(255, 255, 255, 0.4);
  margin: 8px 0 0;
  line-height: 1.4;
  display: flex; align-items: center; gap: 4px; justify-content: center;
}

/* 互动预览 */
.reaction-test {
  width: 100%;
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  text-align: center;
}
.reaction-test__title {
  font-size: 10px;
  color: rgba(255, 255, 255, 0.45);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  margin-bottom: 6px;
}
.reaction-test__row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  justify-content: center;
}
.react-btn {
  padding: 5px 10px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: rgba(255, 255, 255, 0.04);
  color: rgba(255, 255, 255, 0.85);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.12s;
}
.react-btn:hover { background: rgba(255, 126, 39, 0.16); border-color: rgba(255, 126, 39, 0.4); color: #FF7E27; }
.react-btn.web { color: #5ec8ff; border-color: rgba(94, 200, 255, 0.3); }
.react-btn.web:hover { background: rgba(94, 200, 255, 0.16); border-color: rgba(94, 200, 255, 0.5); color: #5ec8ff; }
.reaction-note {
  font-size: 10px;
  color: rgba(94, 200, 255, 0.85);
  margin: 8px 0 0;
  line-height: 1.4;
}

/* 配置区 */
.config-area {
  flex: 1;
  padding-left: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.config-title {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.55);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  font-weight: 600;
  border-left: 2px solid rgba(255, 126, 39, 0.5);
  padding-left: 6px;
}
.emoji-input {
  width: 100%;
  padding: 10px 12px;
  font-size: 20px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 10px;
  color: #fff;
  outline: none;
}
.emoji-input:focus { border-color: #FF7E27; box-shadow: 0 0 0 2px rgba(255, 126, 39, 0.15); }
.config-hint {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.4);
  margin: 0;
  line-height: 1.4;
}
.quick-title {
  font-size: 10px;
  color: rgba(255, 255, 255, 0.45);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  margin-top: 4px;
}
.quick-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(44px, 1fr));
  gap: 6px;
}
.quick-chip {
  height: 40px;
  min-width: 0; /* grid item 默认 min-width:auto，content 撑大 cell，强制按 grid 宽度 */
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  border-radius: 10px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  background: rgba(255, 255, 255, 0.03);
  font-size: 18px;
  cursor: pointer;
  transition: all 0.12s;
}
.quick-chip:hover { background: rgba(255, 126, 39, 0.12); border-color: rgba(255, 126, 39, 0.4); }
.clear-btn {
  margin-top: 12px; /* 紧跟 quick-grid；之前 margin-top:auto 会把按钮推到 config-area 底部对齐 preview-stage，造成"按钮跑到 EXPRESSION 区域下方"的视觉错位 */
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  background: rgba(255, 255, 255, 0.03);
  color: rgba(255, 255, 255, 0.7);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.12s;
  align-self: flex-start;
}
.clear-btn:hover { background: rgba(255, 255, 255, 0.07); color: #fff; }

/* Transition */
.editor-fade-enter-active, .editor-fade-leave-active { transition: opacity 0.15s ease; }
.editor-fade-enter-from, .editor-fade-leave-to { opacity: 0; }
</style>
