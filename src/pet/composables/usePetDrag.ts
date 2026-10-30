/**
 * pet/composables/usePetDrag.ts
 * 桌宠窗口拖拽逻辑（PointerEvents，跨桌面/移动端统一）。
 *
 * v1.2.12（批次 2）重构：拖拽期间的「搬窗口」从前端彻底拿走，三档降级：
 *   ① Windows / Linux → OS 原生 `startDragging()`（批次 1.1 补了 `core:window:allow-start-dragging`
 *      权限后，这条路径**才第一次真正生效**；此前每次都被 ACL 拒掉、静默落到 ③）。
 *   ② macOS（以及 ① 抛错的兜底）→ `invoke('begin_drag', {grabDx, grabDy})`，
 *      由 `pet/drag.rs` 的循环读**全局光标**自己搬窗口，松手时 `end_drag` 回报最终位置。
 *   ③ 只有 ② 的 IPC 也失败时，才退回逐帧 `move_pet_window`（保留，但加了在途串行化）。
 * 前端职责收缩为三件事：pointerdown 记抓取偏移 → 越 4px 阈值发 begin → pointerup 发 end
 * 并把回报位置写进 store。**不再有「每帧算一次坐标再 IPC」**。
 *
 * v1.2.12 收尾修订（2026-10-30，审计 docs/AUDIT-2026-10-30.md 的 P0-1）：**取消 ① 档，三平台统一走 ②**。
 * 原 ① 档有个致命的时序错误 —— `startDragging()` 的 Promise 在「拖拽刚要开始」就 resolve
 * （见下方 macOS 那段同样的机理），于是代码立刻读 `outerPosition()` 收尾：读到的是**起点**，
 * 落盘后重启就跳回本次拖拽的起点；`onEnd` 提前触发还会把拖拽锁与 `.is-dragging` 一起放掉。
 * 为什么不能「等 pointerup 再收尾」：原生拖拽是 DefWindowProc 的**模态移动循环**，
 * 真实的 `WM_LBUTTONUP` 被它自己吃掉（tao 在 `WM_EXITSIZEMOVE` 里要**手动补发**一个，
 * 见 `tao-0.35.3/src/platform_impl/windows/event_loop.rs:1036-1041`），
 * 而 tauri 不把 `WindowEvent::MouseInput` 转发给 JS —— 前端**永远等不到** pointerup。
 * 既然收尾信号在原生档下不可得，正确的选择是不进原生档：② 不调用 `startDragging`，
 * OS 不抢捕获，`pointerup` 正常到达，`end_drag` 既有离屏纠偏又有会话接管，链路本来就是对的。
 * 代价是拖拽期最多 ~125Hz 的 `set_position`（**进程内代理投递，不是 IPC**），
 * 这条路径 macOS 已经用了很久，观感可接受。
 *
 * 为什么 ② 比 ③ 好（这是结构问题，不是调参问题）：
 * - ③ 的监听挂在 webview 的 `window` 上，光标甩出 140×140 / 150×330 这块矩形后 DOM 就不再
 *   收到 pointermove → 桌宠原地掉队，光标绕回来又跳一下（快速横甩必现）。② 读的是操作系统
 *   的全局光标，与「指针在不在窗口里」无关。
 * - ③ 每帧一次异步 IPC + 一次 set_position，往返延迟叠加 → 观感「不跟手」。
 *
 * macOS 为什么一开始就不用原生 ①（读了 tao 0.35.3 源码）：它的 `start_dragging` 是**合成一个假
 * `LeftMouseDown` NSEvent**（`NSEventSubtype::WindowExposed`、pressure 硬编码 1.0）再调
 * `performWindowDragWithEvent:` —— 合成事件对无边框透明窗不稳定，且该调用**异步返回**，
 * Promise 会在拖拽刚开始就 resolve，`beginNativeDrag` 立刻读位置并触发 `onEnd(true)`，
 * 状态必然错乱。v0.7.3 正是为此在 macOS 上手动跳过原生、改走 ③ 的。
 * **v1.2.12 收尾修订把这条判断推广到了三平台** —— 同一条「Promise 提前 resolve」在
 * Windows（`send_user_message` 只投递 `PostMessageW`）与 Linux（`window_requests_tx.send`）
 * 上同样成立，只是当初只在 macOS 上识别出了它。
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - PointerEvents + 物理像素坐标
 *   - 2026-07-17 @v0.6.0-beta.1: UI优化 - 改用 rAF 驱动，丝滑度提升
 *   - 2026-07-24 @v0.6.2-beta.6: 修复 - 增加 setPointerCapture + onStart/onEnd 暂停持久化，解决拖动卡顿
 *   - 2026-08-06 @v0.6.2-beta.28: 重构 - 阈值触发原生 startDragging 替代每帧 IPC，根治拖拽不跟手
 *   - 2026-08-07 @v0.6.2-33: 修复 - 原生拖拽回退分支补 onEnd() (BUG-6)、移除 onMoveHandler 中重复 onStart (BUG-7)
 *   - 2026-08-13 @v0.7.3: 修复 - macOS 跳过不可靠原生 startDragging，改走与菜单一致的手动 move_pet_window 拖拽；修正回退/onUpHandler 重复 onEnd 并复位 isDragging
 *   - 2026-10-29 @v1.2.12: 重构（批次 2.3/2.5）- 新增 Rust 拖拽循环档；macOS 不再逐帧 IPC；
 *     ③ 档加在途串行化（同一时刻只有一个 move_pet_window 在飞，消除「后发的先到 → 窗口回跳」）；
 *     补回 setPointerCapture（v0.6.2-beta.6 加过、beta.28 重构时丢了）
 *   - 2026-10-30 @v1.2.12: 修复（审计 P0-1）- **取消 ① 原生档**，三平台统一走 ② Rust 循环。
 *     原 ① 的 `startDragging()` 同步返回 → 当场读起点写盘（重启跳回拖拽起点）+ onEnd 提前放锁；
 *     且原生档下 pointerup 被模态移动循环吞掉，前端无法在自己这边补救。删掉 `beginNativeDrag`
 *     与 `'native'` 态，`DragMode` 由四态缩成三态
 */
import { onBeforeUnmount, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { log } from "@/lib/logger";

/** 超过该位移（px）才判定为拖拽，否则视为点击（保留点击交互） */
const DRAG_THRESHOLD = 4;

/** 拖拽实现档位：once begun, only one of these owns the window movement.
 *
 * v1.2.12 收尾修订：`'native'` 已移除 —— 原生 `startDragging()` 无法得知「何时松手」，
 * 拿不到收尾信号的档位不能当主路径（详见文件头）。 */
type DragMode = 'idle' | 'rust' | 'manual';

export interface UsePetDragReturn {
  isDragging: ReturnType<typeof ref<boolean>>;
  onPointerDown: (e: PointerEvent) => void;
}

export function usePetDrag(
  getPosition: () => { x: number; y: number },
  onMove: (x: number, y: number) => void,
  onStart?: () => void,
  onEnd?: (didDrag: boolean) => void,
): UsePetDragReturn {
  const isDragging = ref(false);

  let pointerId: number | null = null;
  // pointerdown 时的 clientX/clientY —— 对无边框窗就是「光标相对窗口左上角」的抓取偏移，
  // 也正是 begin_drag 需要的 grabDx/grabDy（CSS px = Tauri 逻辑像素，同一坐标空间）
  let startX = 0;
  let startY = 0;
  let originX = 0;
  let originY = 0;
  let moved = false; // 是否已越过拖拽阈值
  let mode: DragMode = 'idle';
  // 拖拽会话序号：每次 pointerdown 自增。用于判定「await 回来的那次 begin 是否还算数」——
  // 一次快速的点下-拖动-松开可能让 begin_drag 的 IPC 在松手之后才落到 Rust。
  let dragSeq = 0;
  // ===== ③ 档（逐帧 IPC 兜底）的在途串行化状态 =====
  // 只允许一个 move_pet_window 在飞；期间新到的目标只保留「最新一帧」。
  // 不串行化的话，多个请求的**完成顺序不保证**，后发的可能先生效 → 窗口往回跳（2.5 的病灶）。
  let inFlight: Promise<void> | null = null;
  let queued: { x: number; y: number } | null = null;

  function attachListeners(): void {
    // addEventListener 对「同 type + 同函数引用」幂等，重复调用不会挂出两份
    window.addEventListener('pointermove', onMoveHandler);
    window.addEventListener('pointerup', onUpHandler);
    window.addEventListener('pointercancel', onUpHandler);
  }

  function clearListeners(): void {
    window.removeEventListener('pointermove', onMoveHandler);
    window.removeEventListener('pointerup', onUpHandler);
    window.removeEventListener('pointercancel', onUpHandler);
  }

  /** ③ 档：合并到「最新一帧」后串行发出 */
  function manualMove(x: number, y: number): void {
    queued = { x, y };
    if (!inFlight) flushManual();
  }

  function flushManual(): void {
    if (!queued) {
      inFlight = null;
      return;
    }
    const { x, y } = queued;
    queued = null;
    inFlight = invoke('move_pet_window', { x, y })
      .catch(() => {
        /* 单帧失败不阻断拖拽 */
      })
      .then(() => {
        inFlight = null;
        if (queued) flushManual();
      });
  }

  /** 拖拽收尾：把最终落点交回 store + 恢复持久化/穿透锁 */
  function finishDrag(finalX: number, finalY: number): void {
    isDragging.value = false;
    mode = 'idle';
    onMove(Math.round(finalX), Math.round(finalY));
    onEnd?.(true);
  }

  /** ② 档：交给 Rust 侧全局光标循环 */
  async function beginRustDrag(): Promise<void> {
    mode = 'rust';
    isDragging.value = true;
    onStart?.();
    // 本次拖拽的序号：await 期间用户可能已经松手、甚至又开始了下一次拖拽，
    // 靠它判断「回来时这个 begin 还算不算数」（见下方 await 之后的两条判定）。
    const seq = dragSeq;
    try {
      // 参数名遵循 Tauri v2 约定：Rust 侧 grab_dx → JS 侧传 grabDx
      await invoke('begin_drag', { grabDx: startX, grabDy: startY });
    } catch (err) {
      log.warn('[pet] begin_drag 不可用，回退逐帧 move_pet_window', err);
      if (seq === dragSeq) mode = 'manual';
      attachListeners();
      return;
    }
    // ① 期间又开始了新的拖拽 → 那一次的 begin_drag 已经按新会话接管（generation 自增），
    //    这里什么都不做，避免把新会话收掉。
    if (seq !== dragSeq) return;
    // ② 期间已经松手（onUpHandler 把 pointerId 置空）→ **必须**补一次 end_drag。
    //    不能在 onUpHandler 里盲发：两个都是同步命令，若 end_drag 先于 begin_drag 抵达，
    //    end 只是把 running 置 false，随后 begin 又把它置 true 并起线程 → 留下一个
    //    跟着光标永远搬窗的孤儿循环。所以在 begin 落地之后再收，顺序是确定的。
    if (pointerId === null) {
      void endRustDrag(seq);
      return;
    }
    // ② 档不需要 pointermove 搬窗（Rust 读全局光标），但 pointerup 必须由我们收尾 → 监听要在。
    // 幂等：拖拽从 pointerdown 起就一直挂着，这里重复 addEventListener 不会挂出两份。
    attachListeners();
  }

  function onMoveHandler(e: PointerEvent): void {
    if (e.pointerId !== pointerId) return;
    if (!moved) {
      const dx = e.clientX - startX;
      const dy = e.clientY - startY;
      if (Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
      // 越过阈值才接管：单击/双击仍走点击交互（不吞事件）
      moved = true;
      // v1.2.12 收尾修订：三平台统一走 Rust 循环（原 Windows/Linux 的 OS 原生档已移除 ——
      // 它拿不到「松手」这个信号，见文件头）。
      void beginRustDrag();
      return;
    }
    // 只有 ③ 档需要前端逐帧搬窗；② 档由 Rust 循环负责
    if (mode !== 'manual') return;
    const nx = originX + (e.clientX - startX);
    const ny = originY + (e.clientY - startY);
    onMove(nx, ny);
    manualMove(nx, ny);
  }

  function onUpHandler(e: PointerEvent): void {
    if (e.pointerId !== pointerId) return;
    clearListeners();
    const wasDrag = moved;
    if (wasDrag && mode === 'rust') {
      // ② 档：停循环并请 Rust 回报最终位置（它还会顺手做一次离屏纠偏）
      void endRustDrag(dragSeq);
    } else if (wasDrag && mode === 'manual') {
      // ③ 档：把最后一帧发完再收尾
      const p = getPosition();
      const fx = queued?.x ?? p.x;
      const fy = queued?.y ?? p.y;
      invoke('set_pet_cursor_passthrough', { passthrough: false }).catch(() => {});
      manualMove(fx, fy);
      isDragging.value = false;
      mode = 'idle';
      onEnd?.(true);
    } else if (wasDrag) {
      // 越过阈值但 mode 还停在 'idle' —— begin_drag 的 await 尚未返回、beginRustDrag
      // 还没跑到赋值那一行就已松手（IPC 在飞）。这里只复位前端状态，
      // **不盲发 end_drag**：begin_drag 落地后会看到 pointerId === null 并自己补一次
      // end_drag，顺序因此是确定的（见 beginRustDrag 末尾）。
      isDragging.value = false;
      onEnd?.(true);
    }
    pointerId = null;
  }

  /**
   * 收尾（② 档）：请 Rust 停循环并回报最终位置。
   *
   * `forSeq` 是「这次收尾属于哪一轮拖拽」。`end_drag` 的 await 可能在下一次拖拽已经开始
   * 之后才回来 —— 那时 `finishDrag` 会把新一轮的 `isDragging` 抹成 false、并把**上一轮**的
   * 位置写进 store。位置本身是对的（Rust 回报的是真实终点），但状态机与「新一轮已经开始
   * 搬窗」冲突，所以过期的回报只丢弃、不应用。
   */
  async function endRustDrag(forSeq: number): Promise<void> {
    try {
      const [x, y] = await invoke<[number, number]>('end_drag');
      if (forSeq !== dragSeq) return; // 已开始新一轮拖拽 → 这份回报作废
      finishDrag(x, y);
    } catch (err) {
      log.warn('[pet] end_drag 失败，用 store 位置收尾', err);
      if (forSeq !== dragSeq) return;
      const p = getPosition();
      finishDrag(p.x, p.y);
      // 收尾失败意味着 Rust 侧 running 可能还是 true（循环会继续跟着光标搬窗）。
      // 兜底：下一次 begin_drag 会按新会话**接管**（旧循环自动退出），不会永久卡住；
      // 组件卸载时下面 onBeforeUnmount 也会再补发一次 end_drag。
    }
  }

  function onPointerDown(e: PointerEvent): void {
    // 仅响应鼠标左键/单指触摸
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    e.preventDefault();
    pointerId = e.pointerId;
    startX = e.clientX;
    startY = e.clientY;
    const pos = getPosition();
    originX = pos.x;
    originY = pos.y;
    moved = false;
    mode = 'idle';
    queued = null;
    dragSeq += 1;
    // ③ 档靠 DOM 事件，光标甩出宠物矩形就断供 → 补上指针捕获（v0.6.2-beta.6 加过、
    // beta.28 重构时丢了）。② 档不依赖它，但捕获对 pointerup 送达也更稳。
    const target = e.target as Element | null;
    try {
      target?.setPointerCapture?.(e.pointerId);
    } catch {
      /* 极少数环境不支持，忽略 */
    }
    attachListeners();
  }

  onBeforeUnmount(() => {
    clearListeners();
    // 卸载时还处在 ② 档 → 必须告诉 Rust 停循环，否则它会跟着光标一直搬一个已销毁的窗
    if (mode === 'rust') {
      invoke('end_drag').catch(() => {});
    }
    mode = 'idle';
    isDragging.value = false;
  });

  return {
    isDragging,
    onPointerDown,
  };
}
