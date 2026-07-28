<script setup lang="ts">
// AppIcon —— 统一矢量图标组件（替代 emoji 表情）
// 使用 lucide-vue-next，24x24 描边风格，颜色继承 currentColor（由容器 CSS color 控制）。
// 默认 size='1em'，自动跟随所在容器字号；也可显式传 :size 数字（px）。
import { computed } from 'vue';
import {
  Palette, X, Save, Trash2, FolderOpen, Upload, Download, Eye, EyeOff,
  Calendar, AlertTriangle, Wrench, ClipboardList, RefreshCw, CheckCircle2, Check,
  PawPrint, Apple, Leaf, Cookie, Cat, RotateCcw, RotateCw,
  Meh, Laptop, Code2, PenTool, Gamepad2, MessageCircle, Video, Music,
  ShoppingCart, Utensils, Moon, Coffee, Smile, Frown, Angry, SmilePlus,
  Sparkles, Globe, Settings as Cog, Info, Power, Database, Cpu, HardDrive,
  Clock, Copy, PowerOff, KeyRound,
} from 'lucide-vue-next';

const props = withDefaults(
  defineProps<{
    name: string;
    size?: string | number;
    strokeWidth?: number;
  }>(),
  { size: '1em', strokeWidth: 2 },
);

const ICONS: Record<string, any> = {
  palette: Palette, x: X, save: Save, trash: Trash2, folder: FolderOpen,
  upload: Upload, download: Download, eye: Eye, eyeOff: EyeOff,
  calendar: Calendar, warning: AlertTriangle, tool: Wrench, clipboard: ClipboardList,
  refresh: RefreshCw, checkCircle: CheckCircle2, check: Check, paw: PawPrint,
  apple: Apple, leaf: Leaf, cookie: Cookie, cat: Cat,
  rotateCcw: RotateCcw, rotateCw: RotateCw,
  meh: Meh, laptop: Laptop, code2: Code2, penTool: PenTool, gamepad2: Gamepad2,
  messageCircle: MessageCircle, video: Video, music: Music, shoppingCart: ShoppingCart,
  utensils: Utensils, moon: Moon, coffee: Coffee, smile: Smile, frown: Frown,
  angry: Angry, smilePlus: SmilePlus, sparkles: Sparkles,
  language: Globe, settings: Cog, info: Info, power: Power, database: Database,
  cpu: Cpu, hardDrive: HardDrive, clock: Clock, copy: Copy, powerOff: PowerOff,
  keyRound: KeyRound,
};

// 未知 name 回退到 tool 图标，避免渲染空白
const icon = computed(() => ICONS[props.name] ?? Wrench);
</script>

<template>
  <component
    :is="icon"
    :size="size"
    :stroke-width="strokeWidth"
    class="app-icon"
    aria-hidden="true"
  />
</template>

<style>
.app-icon {
  display: inline-block;
  vertical-align: middle;
  flex: none;
}
</style>
