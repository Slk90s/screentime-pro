<!--
  PetWindow.vue
  桌宠窗口根容器（v0.6.0-beta.1 UI优化版）：
  - 透明背景 + 置顶（JS 覆盖全局 style.css body 背景）
  - PointerEvents rAF 拖拽（丝滑）
  - 鼠标穿透默认关闭（桌宠可交互）
  - 右键菜单 → Teleport 到 body 避免被 overflow:hidden 裁剪
  - 点击交互（单击/双击/长按 → 临时状态）
  - 前台应用轮询 → 自动状态（useForegroundWatcher）

  设计思路：
  - 容器 background transparent，JS 强制覆盖 html/body/#app 背景
  - 整个 140×140 区域响应 pointerdown（拖拽 + 点击交互）
  - overflow:hidden 防止内容溢出出滚动条
  - 右键菜单必须 Teleport 到 body（否则被 hidden 裁剪看不到）
  - 桌宠默认接收鼠标事件（passthrough:false）

  修改历史：
    - 2026-07-17 @v0.6.0-beta.1: 完整版 - 拖拽 + 菜单 + 交互 + 自动联动
    - 2026-07-17 @v0.6.0-beta.1: 修复 - 穿透死锁、working状态、HiDPI坐标
    - 2026-07-17 @v0.6.0-beta.1: UI优化 - 全局body背景/滚动条/Toggle/rAF拖拽
    - 2026-07-17 @v0.6.0-beta.1: 修复 - 移除SVG阴影+Teleport右键菜单(overflow:hidden裁剪bug)
  - 2026-07-24 @v0.6.2: 解耦 - 渲染层改用 PetSkinRenderer 路由，皮肤接口固定（现仅 popmart-3d 皮肤）
  - 2026-07-24 @v0.6.2: 更高窗 - 皮肤 preferredSize 驱动动态 setSize，Pop Mart 用 150×330，2D 仍 140×140
  - 2026-07-24 @v0.6.2: 修复 - 监听 pet-shown / pet-custom-updated 调 reloadConfig，跨窗口同步编辑器自定义素材/组合到实时桌宠
  - 2026-07-24 @v0.6.2-beta.3: 修复 - 监听 pet-skin-changed 调 skinRegistry.reloadActive，把 Settings 切皮广播到桌宠（Tauri 多窗口 JS 上下文隔离，module-level reactive 不共享；与编辑→桌宠同步同根因）
  - 2026-07-24 @v0.6.2-beta.6: 修复 - 拖拽加 setPointerCapture + 拖拽期间暂停持久化（解决拖动卡顿）；右键菜单改为「临时放大窗口容纳菜单」修正坐标错位/被裁切导致菜单不显示；pet-custom-updated 新增 reloadBadges 同步 3D 表情
  - 2026-08-08 @v0.6.2-34: 气泡按皮肤配置 - pickBubble 传入当前 skinId，
    监听 pet-custom-updated 同步自定义短语。
-->
<template>
  <div
    ref="rootEl"
    class="pet-window"
    :style="windowStyle"
    @pointerdown="onPointerDown"
    @contextmenu.prevent="onContextMenu"
    @wheel="onWheel"
  >
    <!-- v0.6.2-beta.15：随机中文气泡（不影响点击/拖拽；pointer-events:none） -->
    <PetBubble v-if="bubbleVisible" :message="bubbleMessage" :duration="2500" />
    <!-- v0.6.2-beta.15：系统过载指示（头顶温度计，仅 angry 状态下叠加） -->
    <div v-if="petStore.isHeating" class="pet-overheat-badge" aria-hidden="true">🔥</div>
    <!-- v0.6.2：渲染协议改用 PetSkinRenderer 路由，按 skinRegistry.active() 动态挂载皮肤；
         桌宠现仅 Pop Mart 3D 皮肤（v0.6.2-beta.5 移除 panda-2d）；渲染器按 skinRegistry.active() 动态挂载 -->
    <PetSkinRenderer :state="effectiveState" :is-dragging="isDragging" :class="[animClass, { 'is-walking': isDragging, 'is-heating': petStore.isHeating, 'is-fed': justFed }]" />
  </div>

  <!-- v0.6.2-beta.17：菜单不再在本 webview 渲染，改在独立 pet-menu Tauri 窗口
       （解决 Teleport to body 受限于 pet webview 视口的问题，菜单可拖到全桌面） -->
</template>

<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { petStore } from './stores/petStore';
import { usePetDrag } from './composables/usePetDrag';
import { usePetCursorPassthrough } from './composables/usePetCursorPassthrough';
import { useForegroundWatcher } from './composables/useForegroundWatcher';
import { usePetInteractions } from './composables/usePetInteractions';
import PetSkinRenderer from './components/PetSkinRenderer.vue';
import PetBubble from './components/PetBubble.vue';
import { usePetSprites } from './composables/usePetSprites';
import { usePetBadges } from './composables/usePetBadges';
import { useSystemOverloadWatcher } from './composables/useSystemOverloadWatcher';
import { pickBubble, nextGapMs, reloadBubbleConfig } from './composables/usePetBubble';
// v0.6.2：副作用导入，触发两个内置皮肤注册到 skinRegistry
import './skins';
// v0.6.2：皮肤自描述窗口尺寸（2D 140×140；Pop Mart 等竖图皮肤更高窗）
import { skinRegistry } from './skins/registry';
import { LogicalSize } from '@tauri-apps/api/dpi';

const { t } = useI18n();

const rootEl = ref<HTMLElement | null>(null);
const effectiveState = computed(() => petStore.effectiveState.value);

// v0.6.2-beta.15：随机中文气泡
const bubbleMessage = ref('');
const bubbleVisible = ref(false);
let bubbleTimer: number | null = null;

// v0.7.0：喂食反应（菜单发 pet-fed 事件 → 桌宠进食+开心 + 飘「好吃」气泡）
const justFed = ref(false);
let feedTimer: number | null = null;

function showFeedReaction(): void {
  justFed.value = true;
  bubbleMessage.value = t('pet.feed.reaction');
  bubbleVisible.value = true;
  if (feedTimer !== null) clearTimeout(feedTimer);
  feedTimer = window.setTimeout(() => {
    justFed.value = false;
    bubbleVisible.value = false;
    // 进食气泡结束后恢复常规随机气泡节奏
    scheduleNextBubble();
  }, 1800);
}

function scheduleNextBubble(): void {
  if (bubbleTimer !== null) clearTimeout(bubbleTimer);
  // 首次出现延迟 3s 让用户先看到桌宠，再开始说话；之后 8~22s 一次
  const gap = bubbleMessage.value === '' ? 3000 : nextGapMs();
  bubbleTimer = window.setTimeout(() => {
    bubbleMessage.value = pickBubble(effectiveState.value, skinRegistry.active().id);
    bubbleVisible.value = true;
    // 2.5s 后自动隐藏，2.5s 后再次调度下一次
    window.setTimeout(() => {
      bubbleVisible.value = false;
      scheduleNextBubble();
    }, 2500);
  }, gap);
}

// v0.6.2：皮肤自描述窗口尺寸；2D 默认 140×140，Pop Mart 声明更高窗
const skinSize = computed(() => skinRegistry.active().preferredSize ?? { w: 140, h: 140 });
// v0.6.2-beta.15：滚轮缩放后的实际窗口尺寸（Math.round 防 onResize 抖动）
const scaledSize = computed(() => {
  const k = petStore.scale;
  return {
    w: Math.round(skinSize.value.w * k),
    h: Math.round(skinSize.value.h * k),
  };
});
const windowStyle = computed(() => ({
  width: scaledSize.value.w + 'px',
  height: scaledSize.value.h + 'px',
}));
// 切换皮肤 / 缩放时用 Tauri setSize 调整物理窗口（权限：capabilities/pet.json 的 core:window:allow-set-size）
function applySkinSize(): void {
  const s = scaledSize.value;
  const win = getCurrentWindow();
  win
    .setSize(new LogicalSize(s.w, s.h))
    .catch((e: unknown) => console.warn('[pet] 窗口尺寸调整失败', e));
}

/** v0.6.2-beta.15：滚轮缩放（Ctrl/⌘ + 滚轮 才生效，避免误操作） */
function onWheel(e: WheelEvent): void {
  if (!(e.ctrlKey || e.metaKey)) return;
  e.preventDefault();
  const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
  petStore.setScale(petStore.scale * factor);
  applySkinSize();
}

// 拖拽
const { isDragging, onPointerDown } = usePetDrag(
  () => petStore.position,
  (x, y) => petStore.setPosition(x, y),
  // 拖拽期间暂停 localStorage 持久化（deep watch 每帧写盘会卡顿），结束时恢复一次
  () => petStore.setPersistSuspended(true),
  () => petStore.setPersistSuspended(false),
);

// 鼠标穿透（默认 false，桌宠可交互）
usePetCursorPassthrough();

// 自动联动：每 2s 检查前台应用，更新状态
useForegroundWatcher();

// v0.6.2-beta.15：监听 Rust 端 pet-system-overload 事件 → 桌宠"暴躁升温"
useSystemOverloadWatcher();

// 点击交互
const { animClass, attach: attachInteractions } = usePetInteractions();
onMounted(() => {
  if (rootEl.value) attachInteractions(rootEl.value);
});

// v0.6.2-beta.17：右键菜单改在独立 pet-menu Tauri 窗口展示。
//   - 不再临时放大本桌宠窗口（旧实现两边都别扭）
//   - 让 PetWindow 只负责「用户右键 → 创建并定位 → show 独立菜单窗口」
//   - 关闭由 PetMenuWindow 自己监听 invoke('hide_pet_menu_window')，无需回告
async function onContextMenu(_e: MouseEvent): Promise<void> {
  // 右键是浏览器默认行为，但我们要吃掉它，避免弹菜单/跳选区
  // （实际由 @contextmenu.prevent 在模板处理，这里仅保留 handler 标位）
  try {
    // 1) 创建菜单窗口（幂等）
    await invoke('create_pet_menu_window');
    // 2) 在右键位置附近定位（屏幕绝对坐标，与 pet 窗口无关）
    //    取 pet 窗口外层位置 + 小偏移作为锚点；菜单内部会自己初始化
    const win = getCurrentWindow();
    const petOuter = await win.outerPosition();
    const sf = await win.scaleFactor();
    const MENU_W = 300;
    // 注意：这里直接走逻辑像素入参（move_pet_menu_window 内部 LogicalPosition）
    const x = petOuter.x / sf + 8;
    const y = petOuter.y / sf - 8; // 菜单在 pet 上方一点点
    await invoke('move_pet_menu_window', { x, y });
    // 3) 显示菜单窗口（Rust 端 emit_to 'pet-menu' 'pet-menu-shown' 触发 PetMenuWindow 渲染）
    //    上方 await 串行避免竞态（先定位再 show，否则菜单可能瞬间出现在默认位置）
    void MENU_W; // 防止 unused
    await invoke('show_pet_menu_window');
  } catch (e) {
    console.warn('[pet] 打开菜单失败', e);
  }
}

// 点击外部关闭菜单（v0.6.2-beta.17 移除）：菜单由独立 pet-menu 窗口托管，
// 该窗口自身的「外部点击」关闭逻辑见 PetMenuWindow.vue

const { reloadConfig } = usePetSprites();
const { reloadConfig: reloadBadges } = usePetBadges();
let unlistenShown: (() => void) | null = null;
let unlistenCustom: (() => void) | null = null;
let unlistenSkin: (() => void) | null = null;
let unlistenStore: (() => void) | null = null;
let unlistenFed: (() => void) | null = null;
// v0.7.0：饱食度随时间自然衰减（之前 tickFullness 从未被调用，导致「饿度值不变」）
const FULLNESS_DECAY_MS = 120_000; // 每 2 分钟 -1
let decayTimer: number | null = null;
onMounted(async () => {
  try {
    unlistenShown = await listen('pet-shown', () => {
      // pet 重新显示时重读 localStorage，反映编辑器保存的自定义素材/组合
      reloadConfig();
    });
    // 编辑器保存/重置/导入自定义后即时同步到实时桌宠（无需重开关窗口）
    unlistenCustom = await listen('pet-custom-updated', () => {
      reloadConfig(); // 2D 自定义素材（遗留，无害）
      reloadBadges(); // 3D 表情 emoji
      reloadBubbleConfig(); // 自定义气泡短语
    });
    // v0.6.2-beta.3：Settings 切皮后通过 Tauri 全局广播通知所有窗口，桌宠收到后
    // 把本地 skinRegistry 副本对齐到持久化值（多窗口 JS 上下文隔离，模块级 reactive 不共享）
    unlistenSkin = await listen('pet-skin-changed', () => {
      skinRegistry.reloadActive();
    });
    // v0.6.2-beta.32：右键菜单在独立窗口改了 override/喂食/位置后广播，桌宠窗口重读
    // 同步（否则菜单里选状态/喂食，桌宠表情与饱食度不跟着变）
    unlistenStore = await listen('pet-store-updated', () => {
      petStore.reload();
    });
    // v0.7.0：右键菜单喂食后广播 pet-fed → 桌宠进食+开心反应（不再「喂食无反馈」）
    unlistenFed = await listen<{ value: number; foodId: string }>('pet-fed', () => {
      showFeedReaction();
    });
    reloadConfig(); // 初次创建窗口时也读一次（保险）
  } catch (e) {
    console.error('[pet] 监听 pet 事件失败', e);
  }
  // v0.7.0：启动饱食度衰减循环（仅桌宠窗口存活时运行）
  decayTimer = window.setInterval(() => {
    petStore.tickFullness();
  }, FULLNESS_DECAY_MS);
  scheduleNextBubble(); // 启动气泡周期（v0.6.2-beta.15）
});

// v0.6.2：初始按当前皮肤尺寸调整窗口 + 订阅皮肤切换动态 resize
let unsubSkinSize: (() => void) | null = null;
onMounted(() => {
  applySkinSize();
  unsubSkinSize = skinRegistry.subscribe(() => applySkinSize());
});
onBeforeUnmount(() => {
  if (unlistenShown) unlistenShown();
  if (unlistenCustom) unlistenCustom();
  if (unlistenSkin) unlistenSkin();
  if (unlistenStore) unlistenStore();
  if (unlistenFed) unlistenFed();
  if (unsubSkinSize) unsubSkinSize(); // v0.6.2-33 (BUG-8): 取消皮肤尺寸订阅
  if (bubbleTimer !== null) clearTimeout(bubbleTimer);
  if (feedTimer !== null) clearTimeout(feedTimer);
  if (decayTimer !== null) clearInterval(decayTimer);
});

// v0.6.2-beta.17：菜单「外部点击关闭」由 PetMenuWindow 独立窗口负责。
// PetWindow 不再监听 document.pointerdown，避免与菜单窗口重复或冲突。
</script>

<style scoped>
.pet-window {
  /* v0.6.2-beta.25：填满 Tauri 窗口（皮肤 preferredSize 决定的实际尺寸，如 150×330）。
     旧版硬编码 140×140 导致 3D 熊猫被压在小框里、四周空白大。 */
  width: 100%;
  height: 100%;
  background: transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: grab;
  overflow: hidden; /* 内容不溢出，但菜单已 Teleport 出去不受影响 */
}
.pet-window:active {
  cursor: grabbing;
}

/* 点击交互动画 */
:deep(.pet-anim-bounce) {
  animation: bounce 0.4s ease;
}
:deep(.pet-anim-shake) {
  animation: shake 0.4s ease;
}
:deep(.pet-anim-spin) {
  animation: spin 0.6s ease;
}
:deep(.pet-anim-shrink) {
  animation: shrink 0.5s ease;
}
/* v0.6.2-beta.15：单击轮流三动画（jump / squash / jolt） */
:deep(.pet-anim-jump) {
  animation: jump 0.45s cubic-bezier(0.3, 1.5, 0.5, 1);
}
:deep(.pet-anim-squash) {
  animation: squash 0.5s ease;
}
:deep(.pet-anim-jolt) {
  animation: jolt 0.32s ease;
}
@keyframes bounce {
  0%, 100% { transform: scale(1); }
  40% { transform: scale(1.15) translateY(-4px); }
}
@keyframes shake {
  0%, 100% { transform: translateX(0); }
  20%, 60% { transform: translateX(-4px); }
  40%, 80% { transform: translateX(4px); }
}
@keyframes spin {
  0% { transform: rotate(0); }
  100% { transform: rotate(360deg); }
}
@keyframes shrink {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(0.85); }
}
@keyframes jump {
  0%   { transform: translateY(0) scale(1, 1); }
  35%  { transform: translateY(-22px) scale(0.94, 1.08); }
  60%  { transform: translateY(-12px) scale(1.06, 0.94); }
  100% { transform: translateY(0) scale(1, 1); }
}
@keyframes squash {
  0%   { transform: scale(1, 1); }
  30%  { transform: scale(1.35, 0.7); }   /* 压扁 */
  55%  { transform: scale(0.85, 1.18); }  /* 反弹 */
  75%  { transform: scale(1.08, 0.92); }
  100% { transform: scale(1, 1); }
}
@keyframes jolt {
  0%, 100% { transform: translate(0, 0) rotate(0); }
  15%      { transform: translate(-3px, -2px) rotate(-3deg); }
  30%      { transform: translate(3px, 1px) rotate(4deg); }
  45%      { transform: translate(-2px, 1px) rotate(-2deg); }
  60%      { transform: translate(3px, -1px) rotate(3deg); }
  80%      { transform: translate(-1px, 1px) rotate(-1deg); }
}

/* v0.6.2-beta.15：系统过载时头顶的🔥指示 */
.pet-overheat-badge {
  position: absolute;
  top: -8px;
  right: 0;
  font-size: 16px;
  filter: drop-shadow(0 0 4px rgba(255, 120, 30, 0.55));
  animation: overheat-pulse 0.8s ease-in-out infinite alternate;
  pointer-events: none;
  user-select: none;
  -webkit-user-select: none;
  z-index: 5;
}
@keyframes overheat-pulse {
  0%   { transform: scale(0.9) translateY(0); opacity: 0.85; }
  100% { transform: scale(1.12) translateY(-2px); opacity: 1; }
}
</style>
