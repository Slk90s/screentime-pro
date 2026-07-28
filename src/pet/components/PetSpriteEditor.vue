<!--
  PetSpriteEditor.vue
  3D 桌宠「表情（emoji）」编辑器（v0.6.2-beta.6 重构）。

  重构说明：
  - v0.6.2-beta.5 起 2D 熊猫皮肤已移除，原「部件合成 + 拖拽 + 文件替换 + 导入导出」编辑器失去意义
  - 现仅保留「按状态配置 emoji 表情」能力（3D Pop Mart 桌宠的状态差异本就由 emoji 浮层表达）
  - 已删除：Body/Eyes/Mouth/Brows/Decorations 素材面板、拖拽合成、文件替换、配置导入导出

  交互：
  1. 左侧实时预览：直接复用 PopMartPandaPet（与真实桌宠同一渲染器），按选中状态显示 emoji 浮层
  2. 右侧：状态选择 + emoji 输入框 + 常用表情快速点选
  3. 保存 → 写入 localStorage 并广播 pet-custom-updated，实时桌宠（独立 webview）跨窗口同步

  修改历史：
    - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 2D 部件合成编辑器
    - 2026-07-24 @v0.6.2-beta.6: 重构 - 删除 2D 部件编辑，改为 3D 桌宠按状态配置 emoji 表情
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
          <!-- 左侧：实时预览（与真实桌宠同渲染器） -->
          <div class="preview-area">
            <div class="preview-label">{{ t('pet.editor.preview', 'Preview') }}</div>
            <div class="preview-stage">
              <PopMartPandaPet :state="selectedState" :override-badges="previewBadges" />
            </div>
            <div class="preview-state-name">{{ t(`pet.state.${selectedState}`, selectedState) }}</div>
            <p class="preview-tip">
              <AppIcon name="paw" /> {{ t('pet.editor.previewTip', 'Live preview of the 3D pet') }}
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
import { ref, computed, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import PopMartPandaPet from '../skins/popmart3d/PopMartPandaPet.vue';
import { ALL_STATES } from '../engine/stateMachine';
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
  overflow: hidden;
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
