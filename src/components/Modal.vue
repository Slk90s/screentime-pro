<!--
  Modal.vue — 通用弹窗组件

  用法（基础信息提示）：
    <Modal v-model="show" type="info" title="提示" message="操作完成">
      <template #footer>
        <button @click="show = false">关闭</button>
      </template>
    </Modal>

  用法（确认对话框）：
    <Modal v-model="show" type="confirm" title="删除" message="确定删除？" @confirm="onConfirm" />

  Props:
    - modelValue: 是否显示（v-model 双向绑定）
    - type: "info" | "confirm" | "warn"（影响标题颜色与按钮）
    - title: 标题
    - message: 主文本（可用 slot 替代更复杂内容）
    - confirmText / cancelText: 按钮文案
    - width: 自定义宽度（默认 480px）

  Events:
    - confirm: 确认按钮点击
    - cancel: 取消按钮 / 遮罩点击 / Esc
    - update:modelValue: v-model 同步
-->
<template>
  <transition name="modal-fade">
    <div v-if="modelValue" class="modal-mask" @mousedown.self="onMaskClick">
      <div class="modal-container" :style="{ width }" role="dialog" aria-modal="true">
        <header class="modal-header" :class="type">
          <h3>{{ resolvedTitle }}</h3>
          <button class="modal-close" @click="onCancel" aria-label="关闭">×</button>
        </header>
        <div class="modal-body">
          <div v-if="message" class="modal-message">{{ message }}</div>
          <slot />
        </div>
        <footer class="modal-footer">
          <slot name="footer">
            <template v-if="type === 'confirm' || type === 'warn'">
              <button class="modal-btn cancel" @click="onCancel">{{ resolvedCancel }}</button>
              <button class="modal-btn primary" @click="onConfirm">{{ resolvedConfirm }}</button>
            </template>
            <template v-else>
              <button class="modal-btn primary" @click="onConfirm">{{ resolvedConfirm }}</button>
            </template>
          </slot>
        </footer>
      </div>
    </div>
  </transition>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, watch, computed } from "vue";
import { useI18n } from "vue-i18n";

const { t } = useI18n();

const props = withDefaults(
  defineProps<{
    modelValue: boolean;
    type?: "info" | "confirm" | "warn";
    title?: string;
    message?: string;
    confirmText?: string;
    cancelText?: string;
    width?: string;
    closeOnMask?: boolean;
  }>(),
  {
    type: "info",
    message: "",
    width: "480px",
    closeOnMask: true,
  }
);

// 文案默认值走 i18n；调用方显式传入时优先
const resolvedTitle = computed(() => props.title ?? t("modal.titleDefault"));
const resolvedConfirm = computed(() => props.confirmText ?? t("common.confirm"));
const resolvedCancel = computed(() => props.cancelText ?? t("common.cancel"));

const emit = defineEmits<{
  (e: "update:modelValue", val: boolean): void;
  (e: "confirm"): void;
  (e: "cancel"): void;
}>();

function close() {
  emit("update:modelValue", false);
}
function onConfirm() {
  emit("confirm");
  close();
}
function onCancel() {
  emit("cancel");
  close();
}
function onMaskClick() {
  if (props.closeOnMask) onCancel();
}
// Esc 关闭
function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape" && props.modelValue) onCancel();
}
onMounted(() => window.addEventListener("keydown", onKeydown));
onUnmounted(() => window.removeEventListener("keydown", onKeydown));

// 打开时锁定 body 滚动
watch(
  () => props.modelValue,
  (v) => {
    document.body.style.overflow = v ? "hidden" : "";
  }
);
</script>

<style scoped>
.modal-mask {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
}
.modal-container {
  background: var(--card, #fff);
  border-radius: 12px;
  max-width: 92vw;
  max-height: 88vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
  overflow: hidden;
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border, #e5e7eb);
}
.modal-header h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}
.modal-header.info h3 {
  color: var(--brand, #FF7E27);
}
.modal-header.confirm h3 {
  color: var(--text, #1f2937);
}
.modal-header.warn h3 {
  color: var(--danger-color, #c0392b);
}
.modal-close {
  background: none;
  border: none;
  font-size: 22px;
  color: var(--text-muted, #6b7280);
  cursor: pointer;
  padding: 0 6px;
  line-height: 1;
}
.modal-close:hover {
  color: var(--text, #1f2937);
}
.modal-body {
  padding: 16px 18px;
  overflow: auto;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text, #1f2937);
}
.modal-message {
  white-space: pre-wrap;
}
.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 18px;
  border-top: 1px solid var(--border, #e5e7eb);
}
.modal-btn {
  padding: 6px 16px;
  border: 1px solid var(--border, #e5e7eb);
  border-radius: 8px;
  background: var(--card, #fff);
  font-size: 13px;
  cursor: pointer;
}
.modal-btn.primary {
  background: var(--brand, #FF7E27);
  color: #fff;
  border-color: transparent;
}
.modal-btn.cancel {
  color: var(--text, #1f2937);
}
.modal-btn:hover {
  opacity: 0.92;
}
/* 过渡 */
.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity 0.18s ease;
}
.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;
}
</style>