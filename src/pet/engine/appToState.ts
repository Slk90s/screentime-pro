/**
 * pet/engine/appToState.ts
 * 前台应用 → 桌宠状态映射表（v0.6.0-beta 引入）。
 *
 * 设计思路：
 * - 优先级匹配：先匹配 bundle_id（最稳定），再匹配 process_name（兜底）
 * - 用 Substring 匹配窗口标题可处理"VSCode 打开了什么项目"这类细粒度状态（v0.6.1+）
 * - 规则表数据驱动，新加 App 只需追加一条
 * - 用户手动 override 时此映射表不生效（PetWindow 优先用 effectiveState）
 *
 * 修改历史：
 *   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 20 条主流应用规则
 *   - 2026-07-17 @v0.6.0-beta.1: 修复 - 未命中规则返回合法 `working` 状态（原 cast 导致回退 idle）
 */
import type { PetState, ForegroundAppInfo } from '../types';

interface AppRule {
  state: PetState;
  /** bundle_id 列表（macOS 优先，命中即返回） */
  bundleIds?: string[];
  /** process_name 列表（substring 匹配，跨平台兜底） */
  processNames?: string[];
}

/**
 * 规则表（按"使用频率 + 通用性"排序，命中即 return，避免不必要的字符串比较）
 *
 * 命名约定：
 * - 开发/设计类优先（雷神 PM 群体高频）
 * - 游戏/聊天类次之
 * - 其它业务应用兜底为 working
 */
const RULES: AppRule[] = [
  // ===== 开发 / 设计 =====
  { state: 'developing', bundleIds: ['com.microsoft.VSCode', 'com.microsoft.VSCodeInsiders', 'com.jetbrains.intellij', 'com.jetbrains.intellij-ce', 'com.apple.dt.Xcode'], processNames: ['code', 'code-insiders', 'idea', 'webstorm', 'pycharm', 'goland', 'xcode'] },
  { state: 'designing', bundleIds: ['com.adobe.Photoshop', 'com.adobe.Illustrator', 'com.adobe.xd', 'com.figma.Desktop', 'com.bohemiancoding.sketch3', 'com.zeplin.app'], processNames: ['photoshop', 'illustrator', 'figma', 'sketch', 'zeplin'] },

  // ===== 游戏 =====
  { state: 'gaming', bundleIds: ['com.valvesoftware.steam', 'com.epicgames.EpicGamesLauncher', 'com.blizzard.battlenet'], processNames: ['steam', 'epicgameslauncher', 'battle.net', 'wegame', 'uplay'] },

  // ===== 聊天 / 会议 =====
  { state: 'chatting', bundleIds: ['com.tencent.xinWeChat', 'com.tencent.QQ', 'org.telegram.desktop', 'com.slack.client', 'com.discordapp.Discord'], processNames: ['WeChat', 'QQ', 'Telegram', 'Slack', 'Discord'] },
  { state: 'meeting', bundleIds: ['com.tencent.meeting', 'com.microsoft.teams', 'us.zoom.xos', 'com.google.Chrome.helper'], processNames: ['TencentMeeting', 'Teams', 'Zoom', '钉钉'] },
  { state: 'listening', bundleIds: ['com.apple.Music', 'com.spotify.client', 'com.tencent.QQMusic'], processNames: ['Music', 'Spotify', 'QQMusic', 'NeteaseMusic'] },

  // ===== 购物 / 吃饭（手动）=====
  // 注：吃饭是手动状态，购物匹配电商 App
  { state: 'shopping', bundleIds: ['com.taobao.taobao', 'com.jingdong.app.mac', 'com.tmall.mac'], processNames: ['taobao', 'jd', 'tmall'] },
];

/**
 * 前台应用 → 桌宠状态（自动推断入口）
 *
 * 命中规则：
 * 1. bundleId 完全匹配 → 返回 state
 * 2. processName substring 匹配（大小写不敏感）→ 返回 state
 * 3. 都没命中 → 'working'（合法状态：默认工作中状态）
 */
export function inferStateFromApp(info: ForegroundAppInfo): PetState {
  const proc = info.process.toLowerCase();

  for (const rule of RULES) {
    // 优先级 1：bundle_id 完全匹配（macOS 最稳定；Windows/Linux 为 undefined 自动跳过）
    if (rule.bundleIds && info.bundleId && rule.bundleIds.includes(info.bundleId)) {
      return rule.state;
    }
    // 优先级 2：process_name substring 匹配（跨平台兜底）
    if (rule.processNames) {
      for (const p of rule.processNames) {
        if (proc.includes(p.toLowerCase())) {
          return rule.state;
        }
      }
    }
  }

  // 默认：未匹配的常见应用视为"工作中"（合法 PetState，不再回退 idle）
  return 'working';
}

/** 检查状态是否需要重新推断（用于前端节流） */
export function shouldReinfer(prev: ForegroundAppInfo | null, curr: ForegroundAppInfo): boolean {
  if (!prev) return true;
  return prev.process !== curr.process;
}