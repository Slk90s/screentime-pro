// 桌宠状态 → 矢量图标名（对应 AppIcon 的 name 属性）
// 用于右键菜单 / 素材编辑器的状态选择，替代原先的 emoji。
export const STATE_ICON: Record<string, string> = {
  idle: 'meh',
  working: 'laptop',
  developing: 'code2',
  designing: 'penTool',
  gaming: 'gamepad2',
  chatting: 'messageCircle',
  meeting: 'video',
  listening: 'music',
  shopping: 'shoppingCart',
  eating: 'utensils',
  sleeping: 'moon',
  slacking: 'coffee',
  happy: 'smile',
  sad: 'frown',
  angry: 'angry',
  surprised: 'smilePlus',
};
