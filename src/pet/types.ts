/**
 * pet/types.ts
 * 桌宠子系统的核心类型定义。
 *
 * 设计思路：
 * - 状态用 union literal 收窄，避免运行时 typo（`PetState` 仅允许以下值）
 * - 部件 ID 命名约定：`{layer}_{variant}`，方便素材批量替换
 * - 拖拽坐标用逻辑像素（与 Tauri LogicalPosition 对齐），避免高 DPI 屏偏移
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 15 状态 + 部件 ID 类型
 *   - 2026-07-17 @v0.6.0-beta.1: 修复 - 新增 `working` 合法状态（原未命中规则误用导致回退 idle）
 */

/** 桌宠状态枚举（15 种：idle + 14 个表情/行为） */
export type PetState =
  | 'idle' // 默认待机
  | 'working' // 工作中（未命中具体规则的常见应用兜底）
  | 'developing' // VSCode/IntelliJ
  | 'designing' // Photoshop/Figma
  | 'gaming' // Steam/Epic
  | 'chatting' // WeChat/Slack
  | 'meeting' // TencentMeeting/Teams
  | 'listening' // Music/Spotify
  | 'shopping' // Taobao/JD
  | 'eating' // 吃饭（手动或定时）
  | 'sleeping' // 长时无操作
  | 'slacking' // 摸鱼中
  | 'happy' // 开心（情绪反馈）
  | 'sad' // 难过
  | 'angry' // 生气
  | 'surprised'; // 惊讶

/** 部件层枚举 */
export type PetLayer = 'eye' | 'mouth' | 'brow' | 'cheek' | 'acc';

/** 装饰部件 ID（叠加在身体之上，按状态选择性显示） */
export type PetDecoration =
  | 'none'
  | 'glasses' // 编程/设计
  | 'headphone' // 听歌/会议
  | 'controller' // 游戏
  | 'pencil' // 设计
  | 'speech_bubble' // 聊天
  | 'heart' // 开心情绪
  | 'zzz' // 睡觉
  | 'sweat' // 紧张/生气
  | 'coin'; // 购物

/** 桌宠持久化数据结构（localStorage key = 'screentime-pet'） */
export interface PetPersisted {
  /** 桌宠开关 */
  enabled: boolean;
  /** 窗口位置（逻辑像素，与 Tauri LogicalPosition 对齐） */
  position: { x: number; y: number };
  /** 当前状态 */
  state: PetState;
  /** 手动状态覆盖：null = 自动监听，'manual:xxx' = 手动锁定 */
  override: PetState | null;
  /** 饱食度 0~100 */
  fullness: number;
  /** 累计喂食次数（决定 level） */
  feedCount: number;
  /** 等级 */
  level: number;
  /** 今日已喂食次数（每日 5 次上限） */
  todayFeedCount: number;
  /** 今日喂食日期（YYYY-MM-DD，每日 0 点重置） */
  todayFeedDate: string;
}

/** 应用 → 状态 映射规则的输入（bundle_id 或 process_name） */
export interface ForegroundAppInfo {
  /** 应用名（如 "Visual Studio Code"） */
  name: string;
  /** process name（如 "code" / "Code Helper"，跨平台兜底匹配用） */
  process: string;
  /** bundle id（macOS = com.microsoft.VSCode；Windows/Linux 为 undefined，回退 process_name） */
  bundleId?: string;
  /** 窗口标题（可能为空） */
  windowTitle?: string;
}