/**
 * pet/composables/usePetBubble.ts
 * 桌宠随机中文气泡源（v0.6.2-beta.15）。
 *
 * 设计：
 * - 50+ 内置短语，按 PetState 分桶；当前状态优先级 = state > override > fallback（idle）
 * - 返回随机选中的短句与随机间隔（8~22s），父组件控制气泡显示
 * - 通用短语 vs 状态专有短语：状态命中时 70% 概率抽该桶，30% 抽通用桶
 *
 * 修改历史：
 *   - 2026-07-25 @v0.6.2-beta.15: 初始版本 - 50+ 中文短句池
 */
import type { PetState } from '../types';

/** 各状态专属短句池 */
const STATE_PHRASES: Record<PetState, string[]> = {
  idle: [
    '发呆 5 分钟了',
    '今天也要元气满满鸭',
    '竹子没到货，先撸会儿你',
    '在？给我挠挠头',
    '（啪嗒啪嗒 翻肚皮）',
    '你多久没看我一眼了',
    '瘫着好舒服 ~~~',
    '滴答滴答 时间在走',
    '刚才那个弹幕是什么',
    '在想你的事',
  ],
  working: [
    '加油！就差一点点',
    '文件别忘了 Ctrl+S',
    '今天也是元气满满鸭',
    '老板看了你一眼',
    '走神是允许的',
    '摸鱼 5 分钟应该没人发现',
    '深呼吸，三、二、一',
  ],
  developing: [
    '这个 Bug 不会自己修的',
    '记得 git commit 一下',
    'Stack Overflow 在召唤你',
    '能不能跑起来？',
    '「运行成功」！一次过！',
    '是不是又少写个分号',
    '编译失败 → 看一下错误日志',
  ],
  designing: [
    '对齐到像素',
    '这个间距再小一点？',
    '配色看着不舒服，再试试',
    '漂亮！甲方爸爸也满意',
    '对比度好像不太够',
  ],
  gaming: [
    '再来一把就睡觉',
    '对面打野不见人，小心',
    '这把赢了 就到六点了',
    '队友：你是脚本吧',
    '你打的是人机吧',
  ],
  chatting: [
    '老板来了记得切窗口',
    '已读不回 ×××',
    '打字慢悠悠！',
    '群里那位又开始了',
  ],
  meeting: [
    '老板讲了 30 分钟了',
    '会议马拉松开始',
    '假装很认真点头',
    '求求快点结束',
    '走神了 5 秒 谁发现？',
  ],
  listening: [
    '音乐起的正是时候',
    '耳朵怀孕系列',
    '下一首下一首',
    '副歌来了！',
  ],
  shopping: [
    '买它！',
    '凑个满减不亏',
    '购物车×99',
    '明天再决定',
    '钱包在偷偷抹眼泪',
  ],
  eating: [
    '嗝～ 吃饱了',
    '这道好吃 下次还点',
    '你吃你的 我吃我的',
  ],
  sleeping: [
    'Zzz…',
    '梦到一个大竹子',
    '麻烦揉一下',
    '别戳我（翻个身）',
  ],
  slacking: [
    '又拖 5 分钟',
    '任务列表：待办 99+',
    '偷偷刷个 B 站不慌',
    '假装表格很复杂',
  ],
  happy: [
    '今天也要开心鸭',
    '你笑起来了！我也开心',
    '好耶！',
    '蹦蹦跳跳 转圈圈',
    '比心 ♥',
  ],
  sad: [
    '呜…',
    '你忘记喂我了',
    '饱食度告急 T_T',
    '想看你一眼…',
  ],
  angry: [
    '喂我！',
    '怎么又没陪陪我',
    '哼！',
    '别戳我（抖抖）',
  ],
  surprised: [
    '？！',
    '哦豁',
    '！！！',
    '卧槽',
  ],
};

/** 通用短语（任何状态都可能抽中） */
const COMMON_PHRASES: string[] = [
  '今天做点不接电气的事吧',
  '给你比个 ♥',
  '你的眼睛在笑',
  '一起加油鸭',
  '想抱一下',
  '记得喝水',
  '汉堡好吃 火锅也好吃',
  '出太阳啦，出去走走',
  '你是我的光',
  '咕嘟咕嘟…',
  '一滴都不剩啦',
  '撸猫时间到',
  '窗外有云',
  '你认真工作的样子最帅',
];

/** 取得一个候选短语（v0.6.2-beta.15+） */
export function pickBubble(state: PetState): string {
  // 70% 概率返回状态桶，30% 返回通用桶
  const useState = Math.random() < 0.7;
  if (useState) {
    const bucket = STATE_PHRASES[state] ?? [];
    if (bucket.length > 0) {
      return bucket[Math.floor(Math.random() * bucket.length)];
    }
  }
  return COMMON_PHRASES[Math.floor(Math.random() * COMMON_PHRASES.length)];
}

/** 取得下一个气泡的随机间隙毫秒（8~22s） */
export function nextGapMs(): number {
  return 8000 + Math.floor(Math.random() * 14000);
}
