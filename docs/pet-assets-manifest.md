# 桌宠素材清单（AI 生产用）

> 版本：v0.6.0-beta.1 | 最后更新：2026-07-21
> 用途：给 AI 图像生成一份**可精确生产**的透明底 PNG 素材规格。文件名必须与代码 `import` 完全一致，否则编译失败。

---

## 0. 一句话结论

桌宠 = **1 张身体 + 22 张部件**（共 23 张独立透明 PNG），在 140×140 的透明窗内按百分比坐标叠拼而成。
AI 需按本文档的 **命名 / 坐标系 / 风格** 生产，**眼睛和眉毛是成对图（一张图里左右两个）**，且所有部件**共用同一 512×512 坐标系**才能严丝合缝。

---

## 1. 渲染机制速览（为什么这样拆）

| 机制 | 说明 |
|---|---|
| 部件层 | `PetLayer.vue`：单张透明 PNG，`position:absolute` + 百分比定位（相对 140×140 容器），CSS `transform: translate(-50%,-50%)` |
| 合成顺序 | `body`(最底) → `eye` → `brow` → `mouth` → `decorations[]`（数组顺序从下到上叠加） |
| 眼睛/眉毛 | **成对图**：代码把它们当单个图层定位到脸中央（x=50%, y=48% / 42%），所以一张图里要画**左右两只** |
| 身体 | 整体（含头、身、耳、四肢、眼圈、内耳），不含表情 |
| 装饰层 | 9 个独立小图标，按 `decorationSpec` 各自定位 |
| 坐标系 | 所有图统一 **512×512 透明画布**，部件画在"整只熊猫在那里时该部件的真实位置" |

> 代码入口：`src/pet/components/PetCanvas.vue`（import 全部 sprite）、`src/pet/engine/stateMachine.ts`（状态→部件组合表）。

---

## 2. 素材总表（目标 23 张）

> ✅ = 项目已存在（2026-07-17 生成）｜ ⬜ = 待生产（预留）
> 定位列 `(x%, y%, 宽%)` = 部件图**中心**在 140 容器中的水平/垂直百分比位置 + 显示宽度占容器百分比。

### 2.1 身体（1 张）

| 文件名 | 图层 | 成对 | 定位 (x,y,宽) | 用途 | 现状 |
|---|---|---|---|---|---|
| `body_base.png` | body | 否 | (50, 65, 95) | 熊猫主体（头+身+耳+四肢+黑眼圈+内耳），所有状态共用 | ✅ |

### 2.2 眼睛（4 张，成对）

| 文件名 | 定位 (x,y,宽) | 表情 | 对应状态 | 现状 |
|---|---|---|---|---|
| `eye_open_normal.png` | (50, 48, 58) | 睁眼（圆/椭圆，正常） | idle / working / developing / designing / chatting / meeting / shopping / sad / angry | ✅ |
| `eye_happy_smile.png` | (50, 48, 58) | 开心眯眼（^ ^ 弧线） | gaming / listening / eating / slacking / happy | ✅ |
| `eye_closed_sleep.png` | (50, 48, 58) | 闭眼（— — 横线） | sleeping | ✅ |
| `eye_dizzy.png` | (50, 48, 58) | 晕/惊讶（@ @ 或 × × 螺旋） | surprised | ✅ |

### 2.3 眉毛（3 张，成对）

| 文件名 | 定位 (x,y,宽) | 形态 | 对应状态 | 现状 |
|---|---|---|---|---|
| `brow_normal.png` | (50, 42, 50) | 平直/微弯 | 除 sad/angry 外全部 | ✅ |
| `brow_angry.png` | (50, 42, 50) | 倒八（内低外高，怒） | angry | ✅ |
| `brow_sad.png` | (50, 42, 50) | 八字下垂（哀） | sad | ✅ |

### 2.4 嘴巴（6 张，单件）

| 文件名 | 定位 (x,y,宽) | 形态 | 对应状态 | 现状 |
|---|---|---|---|---|
| `mouth_smile.png` | (50, 60, 22) | 微笑弧 | idle / working / developing / designing / gaming / chatting / meeting / listening / shopping / slacking / happy | ✅ |
| `mouth_neutral.png` | (50, 60, 22) | 平/微弯一线 | sleeping | ✅ |
| `mouth_frown.png` | (50, 60, 22) | 向下撇 | sad | ✅ |
| `mouth_surprised.png` | (50, 60, 22) | 小 O 形 | surprised | ✅ |
| `mouth_eating.png` | (50, 60, 22) | 张嘴/咀嚼 | eating | ✅ |
| `mouth_pout.png` | (50, 60, 22) | 嘟嘴撅起 | angry | ✅ |

### 2.5 装饰（9 张，单件）

| 文件名 | 定位 (x,y,宽) | 含义 | 出现状态 | 现状 |
|---|---|---|---|---|
| `glasses.png` | (50, 48, 50) | 框架眼镜（横跨双眼） | developing / designing | ✅ |
| `headphone.png` | (50, 38, 80) | 头戴耳机（罩双耳+头梁） | gaming / meeting / listening | ✅ |
| `controller.png` | (50, 92, 55) | 游戏手柄（身前） | gaming | ✅ |
| `pencil.png` | (82, 30, 30) | 铅笔（斜插右上） | designing | ✅ |
| `speech_bubble.png` | (80, 22, 32) | 对话气泡（右上） | chatting | ✅ |
| `heart.png` | (22, 22, 22) | 爱心（左上） | shopping / happy | ✅ |
| `zzz.png` | (80, 18, 26) | ZZZ（头顶） | sleeping | ✅ |
| `sweat.png` | (80, 45, 14) | 汗珠（右侧脸颊） | slacking / sad / angry | ✅ |
| `coin.png` | (50, 25, 18) | 金币（预留，当前无状态使用） | —（预留） | ⬜ |

**合计：23 张（22 已存在 + 1 预留 coin）。**

---

## 3. 坐标系与对齐约束（最重要，务必遵守）

所有素材统一在 **512×512 透明画布** 上绘制（@2x 母版，运行时自动缩放到 140 容器）。
部件须画在「若整只熊猫完整呈现时，该部件在画布上的真实位置」，换算公式：画布坐标 = 百分比 × 512。

| 部件 | 容器定位 (x%, y%) | 512 画布参考中心 (x, y) | 备注 |
|---|---|---|---|
| 身体 | (50, 65) | (256, 333) | 脸中心略低于画布中 |
| 眼睛 | (50, 48) | (256, 246) | **一对**，左右对称于 x=256 |
| 眉毛 | (50, 42) | (256, 215) | **一对**，左右对称 |
| 嘴巴 | (50, 60) | (256, 307) | 单件，居中 |
| 眼镜 | (50, 48) | (256, 246) | 覆盖双眼 |
| 耳机 | (50, 38) | (256, 195) | 跨头顶+双耳 |
| 手柄 | (50, 92) | (256, 471) | 身体下方 |
| 铅笔 | (82, 30) | (420, 154) | 右上 |
| 气泡 | (80, 22) | (410, 113) | 右上 |
| 爱心 | (22, 22) | (113, 113) | 左上 |
| ZZZ | (80, 18) | (410, 92) | 右上 |
| 汗珠 | (80, 45) | (410, 230) | 右脸 |
| 金币 | (50, 25) | (256, 128) | 头顶 |

**对齐铁律**：先画好 `body_base.png`（含黑眼圈位置），其余部件图必须让眼睛/眉毛/嘴与 body 上的黑眼圈、脸型对齐——否则叠加后会"对不上眼"。建议生产时以 body 图为参照底，在同一坐标系下导出各部件。

---

## 4. 统一风格规范

| 维度 | 规范 |
|---|---|
| 画风 | 卡通扁平（flat 2D）、粗描边、圆角、可爱治愈系 |
| 主体配色 | 身体白 `#FFFFFF`｜黑耳/黑眼圈 `#2B2B2B`｜内耳粉 `#FFD3D3` |
| 描边 | 深灰 `#1A1A1A`，线宽统一（建议 12–16px @512 画布） |
| 腮红 | 可选 `#FF9EB0`（开心/害羞态） |
| 视角 | 正面、居中、微微 3/4 头身比（头略大） |
| 背景 | **必须透明（alpha=0）**，无任何底纹/阴影/白块 |
| 光影 | 扁平即可，避免写实渐变（与 `image-rendering: crisp-edges` 锐利渲染匹配） |
| 分辨率 | 512×512 PNG-24（透明），可出 1024×1024 母版后缩 |
| 单张大小 | 若走「编辑器上传」路径须 ≤ 256KB；直接覆盖源码图无此限 |

---

## 5. AI 生产 Prompt 模板（可直接喂图生图）

通用前缀（每句加）：
> `flat 2D cartoon giant panda mascot, clean bold outline (#1A1A1A), white body (#FFFFFF), dark ears and eye-patches (#2B2B2B), pink inner ears (#FFD3D3), transparent background, no shadow, centered, isolated on empty canvas, game asset style, vector-style crisp, 512x512`

按部件替换主体描述：

- **body_base**：`full panda body front view, round head + chubby body + two ears + four short limbs + dark eye-patches shaped for eyes, no facial features (eyes/mouth drawn separately), cute`
- **eye_open_normal**：`pair of two round open panda eyes (shiny black ovals with white highlight), placed symmetric left and right, transparent background, no face`
- **eye_happy_smile**：`pair of two happy closed eyes as upward arcs (^ ^), symmetric, transparent background`
- **eye_closed_sleep**：`pair of two sleepy closed eyes as horizontal lines (--), symmetric, transparent background`
- **eye_dizzy**：`pair of two dizzy swirl/spiral eyes (@ @), symmetric, transparent background`
- **brow_normal**：`pair of two flat neutral eyebrows above eye level, symmetric, transparent background`
- **brow_angry**：`pair of two angry slanted eyebrows (inner-low outer-high, like \  /), symmetric, transparent background`
- **brow_sad**：`pair of two sad drooping eyebrows (outer-low inner-up), symmetric, transparent background`
- **mouth_smile**：`small cute smiling mouth (upward curve), centered, transparent background`
- **mouth_neutral**：`small neutral flat mouth line, centered, transparent background`
- **mouth_frown**：`small frowning mouth (downward curve), centered, transparent background`
- **mouth_surprised**：`small surprised open oval mouth (O shape), centered, transparent background`
- **mouth_eating**：`open chewing mouth with tiny tongue, centered, transparent background`
- **mouth_pout**：`pouting pursed lips (duck face), centered, transparent background`
- **glasses**：`pair of round black-framed glasses spanning two eyes, transparent background, no face`
- **headphone**：`over-ear headphones with headband, transparent background`
- **controller**：`game controller (gamepad), transparent background`
- **pencil**：`single pencil tilted, transparent background`
- **speech_bubble**：`small comic speech bubble (empty), transparent background`
- **heart**：`small red/pink heart, transparent background`
- **zzz**：`stylized "Z z z" sleeping letters, transparent background`
- **sweat**：`small blue sweat drop, transparent background`
- **coin**：`small gold coin with ¥ or $ , transparent background`（预留）

> 中文可译为：「扁平卡通大熊猫吉祥物，粗黑描边，白身体，黑耳黑眼圈，粉内耳，透明背景，无阴影，居中孤立，游戏素材风，512×512」+ 部件描述。

---

## 6. 命名与接入红线

1. **文件名一字不差**：必须小写 + 下划线，与 §2 表格完全一致（如 `eye_open_normal.png`，不是 `EyeOpenNormal` / `eye-open-normal`）。代码是静态 `import`，拼错即编译失败。
2. **成对规则**：`eye_*` / `brow_*` 一张图里画**左右两只**，不要拆成 `eye_left` / `eye_right`。
3. **透明底硬要求**：任何不透明底块都会在桌面露出方块。
4. **接入方式二选一**：
   - **覆盖源码**（推荐批量替换）：直接替换 `src/pet/assets/sprites/*.png`，重新 `tauri build`。
   - **运行时上传**（用户侧）：设置页 🎨「编辑素材 & 表情」→ 上传 PNG（≤256KB，base64 存 localStorage，key `pet_custom_sprites`），不重编译。

---

## 7. 生产检查清单

- [ ] 共 23 张透明底 PNG（22 已存在 + coin 可选）
- [ ] 文件名与 §2 完全一致（小写+下划线）
- [ ] 全部共用 512×512 坐标系，眼睛/嘴与 body 黑眼圈对齐
- [ ] 风格统一（扁平、粗描边、配色一致）
- [ ] 单张 ≤ 256KB（若走编辑器上传）
- [ ] 无阴影、无白底、无多余描边溢出
- [ ] 替换后本地 `npm run build` + `tauri build` 验证叠加效果

---

### 附：状态 → 素材组合速查（16 状态）

| 状态 | eye | mouth | brow | decorations |
|---|---|---|---|---|
| idle | open_normal | smile | normal | — |
| working | open_normal | smile | normal | — |
| developing | open_normal | smile | normal | glasses |
| designing | open_normal | neutral | normal | glasses, pencil |
| gaming | happy_smile | smile | normal | headphone, controller |
| chatting | open_normal | smile | normal | speech_bubble |
| meeting | open_normal | smile | normal | headphone |
| listening | happy_smile | smile | normal | headphone |
| shopping | open_normal | smile | normal | heart |
| eating | happy_smile | eating | normal | — |
| sleeping | closed_sleep | neutral | normal | zzz |
| slacking | happy_smile | smile | normal | sweat |
| happy | happy_smile | smile | normal | heart |
| sad | open_normal | frown | sad | sweat |
| angry | open_normal | pout | angry | sweat |
| surprised | dizzy | surprised | normal | — |
