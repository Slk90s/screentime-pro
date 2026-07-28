/**
 * pet/skins/types.ts
 * PetSkin 接口（v0.6.2 引入）。
 *
 * 设计目的：
 * - 把"桌宠外观（怎么画）"和"桌宠引擎（状态机 + 前台监听 + 拖拽 + 点击交互）"彻底解耦
 * - 任何满足 PetSkin 接口的模块都可以注册到皮肤注册表，新皮肤不必触碰现有组件
 * - 后续若引入 Live2D / Video / 三维模型 桌宠，只需新增 skins/<name>/{ index.ts + 组件 } 即可
 *
 * 设计原则：
 * - 状态机仍由核心引擎（engine/stateMachine）拥有，皮肤只接收"当前应该表现的状态"
 * - 皮肤拥有自己的资源/动画/渲染策略，但**不**允许反向修改 petStore（单向数据流）
 * - 生命周期由注册表负责（unmount 时皮肤可释放资源，但当前阶段多为无状态组件，无需清理）
 *
 * 修改历史：
 *   - 2026-07-24 @v0.6.2: 初始创建 - 解耦皮肤接口（响应"后续类似功能要解耦"诉求）
 */
import type { Component } from 'vue';
import type { PetState } from '../types';

/** 皮肤向容器注入的渲染器组件（实现 mount=挂到 rootEl；update=响应 state 变化） */
export type PetSkinRendererComponent = Component<{ state: PetState }>;

/**
 * 皮肤清单（manifest）。
 * - id: 全局唯一，存到 localStorage 'screentime-pet-skin'
 * - name: UI 显示名（i18n key 可选）
 * - renderer: 接受 state prop 的 Vue 组件，由 PetSkinRenderer 动态挂载
 * - decorations?: 状态 → 装饰层提示（如 "💭" "♥" "Zz"），仅用来在菜单/状态卡片展示，皮肤内部自行决定如何呈现
 */
export interface PetSkinManifest {
  /** 皮肤唯一 id（kebab-case，推荐 "<theme>-<style>"，例 "panda-2d" / "popmart-3d"） */
  id: string;
  /** 皮肤名（直接展示给用户） */
  name: string;
  /** 版本号（仅声明用，运行时不做校验） */
  version: string;
  /** 简短说明（hover tooltip / 设置页描述） */
  description?: string;
  /** 期望窗口尺寸（逻辑像素）；不声明则用默认 140×140。2D 固定 140×140，竖图皮肤（如 Pop Mart）可声明更高窗 */
  preferredSize?: { w: number; h: number };
  /** 渲染器组件（接收 prop { state: PetState }） */
  renderer: PetSkinRendererComponent;
  /**
   * 状态装饰（可选），用于右键菜单/调试面板等需要快速表达状态的场合。
   * 皮肤组件本身可视情况忽略这个 map 用自己的渲染逻辑。
   */
  decorations?: Partial<Record<PetState, string[]>>;
}

/**
 * 注册表内部形态（仅 export 给 registry.ts 用）。
 */
export interface PetSkinRegistry {
  register(manifest: PetSkinManifest): void;
  get(id: string): PetSkinManifest | undefined;
  list(): PetSkinManifest[];
  active(): PetSkinManifest;
  setActive(id: string): boolean;
  subscribe(listener: (activeId: string) => void): () => void;
  /**
   * 从 localStorage 重新读取 activeId 并更新内部状态 + 通知本地监听器。
   * 用于跨窗口同步：Tauri 多窗口各自持有独立 JS 上下文，模块级 reactive 不共享；
   * 主窗口 setActive 后通过 Tauri 全局事件 'pet-skin-changed' 广播，PetWindow/Settings
   * 收到后调用本方法把本地副本对齐到持久化的 activeId。
   */
  reloadActive(): void;
}
