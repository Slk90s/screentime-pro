# v0.6.0-beta 桌面宠物（桌宠）功能规划

> **状态**：草案，待用户对 §6 决策点拍板后启动实现
> **目标版本**：`v0.6.0-beta.1`（SemVer pre-release）
> **影响面**：新增独立子系统，**零侵入** v0.5.0 现有功能
> **灵感**：早期 QQ 企鹅宠物风格（透明背景 / 常驻桌面 / 主动交互）

---

## 1. 设计目标

| 维度 | 目标 |
|------|------|
| **视觉** | 黑白可爱卡通熊猫（参考用户提供的橘色卫衣吉他手 + 闭眼抱膝风格），橙蓝主色调 |
| **行为** | 待机呼吸/眨眼、随机走动、点击反应、根据当前前台应用切换表情状态 |
| **性能** | CPU 闲置时 < 0.5%，动起来时 < 2%；首屏 < 200ms |
| **资源** | 全部素材 < 200KB（PNG 部件 + 少量 base64 内嵌 emoji 装饰） |
| **架构** | 复用 v0.5.0 的 Rust tracker / 前端 store / i18n，**新增 `pet/` 子系统** |

---

## 2. 架构总览（独立子系统，不污染主应用）

```
screentime-pro/
├── src/
│   ├── pet/                              # 【新增】桌宠子系统（独立目录）
│   │   ├── PetWindow.vue                 # 桌宠窗口容器（透明/置顶/穿透）
│   │   ├── PetCanvas.vue                 # 渲染层（合成基础身体 + 表情部件层）
│   │   ├── components/
│   │   │   ├── PetBody.vue               # 基础身体 + CSS keyframe 待机动画
│   │   │   ├── PetMouth.vue              # 嘴巴部件层
│   │   │   ├── PetEyes.vue               # 眼睛部件层（含眨眼动画）
│   │   │   ├── PetEyebrows.vue           # 眉毛部件层
│   │   │   ├── PetDecorations.vue        # 装饰层（腮红/汗珠/爱心/耳机/眼镜...）
│   │   │   ├── PetContextMenu.vue        # 右键菜单（喂食/关闭/状态切换）
│   │   │   └── PetSpeechBubble.vue       # 偶尔说话的气泡（可选 v0.6.1）
│   │   ├── engine/
│   │   │   ├── stateMachine.ts           # 状态机（13 状态 + 转移规则）
│   │   │   ├── appToState.ts             # 前台应用 → 状态 映射表
│   │   │   ├── idleBehavior.ts           # 随机行为（每 30~60s 触发走动/打盹）
│   │   │   ├── feedingSystem.ts          # 喂食系统（增/减饱食度、升级）
│   │   │   └── petLevels.ts              # 好感度/等级计算
│   │   ├── composables/
│   │   │   ├── usePetDrag.ts             # 拖拽逻辑（PointerEvents + position save）
│   │   │   ├── usePetAnimation.ts        # CSS 动画绑定 + rAF 节流
│   │   │   └── usePetCursorPassthrough.ts# 鼠标穿透（仅身体区域响应事件）
│   │   ├── stores/
│   │   │   └── petStore.ts               # Pinia：开关/位置/好感度/level/饱食度
│   │   └── i18n-keys.ts                  # pet 命名空间的 key 集中导出
│   └── ...（既有 v0.5.0 模块不受影响）
│
├── src-tauri/src/
│   ├── pet/                              # 【新增】桌宠 Rust 端
│   │   ├── mod.rs                        # pet 模块入口
│   │   ├── window.rs                     # 创建/管理 pet 窗口（透明/置顶/穿透）
│   │   ├── foreground_watcher.rs         # 前台应用变化事件 emit
│   │   └── platform/                     # 各平台实现
│   │       ├── macos.rs                  # NSWorkspace.didActivateApplicationNotification
│   │       ├── windows.rs                # SetWinEventHook(EVENT_SYSTEM_FOREGROUND)
│   │       └── linux.rs                  # X11 _NET_ACTIVE_WINDOW / D-Bus
│   └── ...（既有 v0.5.0 模块不受影响）
│
├── src-tauri/tauri.conf.json             # 新增 pet 窗口配置（透明/置顶/无装饰/无任务栏）
└── src-tauri/capabilities/               # 【新增】pet 窗口权限（独立 capability）
    └── pet.json
```

### 与 v0.5.0 现有架构的衔接

| 复用 | 用途 |
|------|------|
| `RawApp`（已有） | `get_foreground_app()` 已跨平台实现，**新增**：监听变化时 emit `foreground-changed` 事件给前端 |
| `tracker::PlatformTracker` trait（已有） | 加一个 `subscribe_foreground_change()` 方法（默认 no-op，三平台分别实现） |
| `tauri-plugin-log`（已有） | pet 模块共用 |
| `vue-i18n` + `i18n/zh-CN.ts/en-US.ts`（已有） | 新增 `pet.*` 命名空间（约 30~40 个 key） |
| `Modal.vue` 组件（已有） | pet 设置弹窗复用 |
| 现有分类规则的 `bundle_id` / `process_name`（已有） | 直接用映射状态，**不新增依赖** |

---

## 3. 核心功能模块

### 3.1 窗口（Tauri 2 多窗口）

```json
// tauri.conf.json 新增（app.windows 数组中追加）
{
  "label": "pet",
  "url": "/pet",                  // 单独路由
  "width": 140,
  "height": 140,
  "decorations": false,           // 无标题栏/边框
  "transparent": true,            // 背景透明
  "alwaysOnTop": true,            // 始终置顶
  "skipTaskbar": true,            // 不在任务栏/Dock 显示
  "resizable": false,
  "shadow": false,                // 去掉窗口阴影
  "visible": false                // 默认隐藏，由用户在设置页开启
}
```

**鼠标穿透**（仅熊猫图层接收事件，空白区域不挡下层应用）：
- Tauri 2 API `webview.set_ignore_cursor_events(true)` 全局穿透
- 前端监听 `mousemove`，绘制一个 `mapArea`（基于 `<canvas>` 像素 alpha 通道 > 200 的区域）
- 当鼠标进入熊猫身体区域时，调用 `set_ignore_cursor_events(false)` 让该窗口接收事件
- 离开区域恢复穿透

### 3.2 状态机（13 个状态）

```
                ┌─────────┐
                │  idle   │ ◄──────────────┐
                └─────────┘                │
                    ▲ ▼                    │
            ┌───────┴───────┐              │
        [工作中]──[开发中]──[设计中]   [待机 30s+]
            │       │       │              │
            └───┬───┘       │              ▼
                ▼           ▼          [打盹/困倦]
            [摸鱼中]──[游戏中]         (5min 后转睡觉)
                │
                ▼
            [聊天中]──[吃饭中]──[困倦]
                │
                ▼
            [开心]──[难过]──[生气]──[惊讶]  (情绪状态，触发后回 idle)
```

**状态触发源**（双通道）：
1. **自动**：`foreground-changed` 事件 → `appToState.ts` 查表（vscode→developing / chrome→browsing...）
2. **手动**：右键菜单强制切换（覆盖自动推断）
3. **随机**：每 30~60s 触发 `idleBehavior.ts` 随机选 idle/sleep/wander

**映射表示例**（`appToState.ts`）：

| bundle_id / process_name | 状态 | 装饰部件 |
|----------|------|----------|
| `com.microsoft.VSCode` / `code.exe` | `developing` | 眼镜 + 键盘提示 |
| `com.adobe.Photoshop` / `photoshop.exe` | `designing` | 调色板 + 铅笔 |
| `com.valvesoftware.steam` / `steam.exe` | `gaming` | 游戏手柄 + 耳机 |
| `com.tencent.xinWeChat` / `WeChat.exe` | `chatting` | 聊天气泡 + 爱心 |
| `com.tencent.meeting` / `Teams.exe` | `meeting` | 笔记本 + 咖啡 |
| `com.apple.Music` / `QQMusic.exe` | `listening` | 耳机 + 音符 |
| `com.taobao.taobao` / `jd.exe` | `shopping` | 钱袋 |
| `*`（其他非白名单） | `working` | 默认 |
| 长时无变化（>5min） | `slacking` | 摸鱼 + 可乐 |
| 用户主动空闲（系统 idle > 30min） | `sleeping` | ZZZ 气泡 |

### 3.3 动画系统

**两层动画叠加**：
- **身体层**：CSS keyframe（呼吸 3s、眨眼 4~6s 随机间隔）
- **部件层**：基于状态切换 sprite（`@keyframe` 进场 200ms 缩放）

**rAF 节流**：状态切换触发动画时 `requestAnimationFrame` 重绘，非活跃时段用 `transition` 纯 CSS（GPU 加速）

### 3.4 交互系统

| 触发 | 反应 |
|------|------|
| 点击头部 | 眨眼 + 爱心冒出 + 抖动 200ms |
| 点击身体 | 嘟嘴 + 弹一下 |
| 双击 | 触发「开心」特殊动画（转圈 + 星星） |
| 右键 | 弹出 ContextMenu（喂食 / 切换状态 / 关闭） |
| 拖拽 | PointerEvents，松开后位置写入 localStorage（持久化） |
| 长按（3s） | 播放「撒娇」动画 + 撒娇气泡 |

### 3.5 喂食系统（v0.6.0-beta 简化版）

- **饱食度**：0~100，每 60s 自然衰减 1
- **喂食**：右键菜单选食物（竹叶/苹果/糖果）→ 饱食度 +20，**每日上限 5 次**
- **等级**：`level = floor(累计喂食次数 / 10) + 1`，影响解锁装饰（v0.6.1+）
- **状态联动**：饱食度 < 30 → 触发「饥饿」状态（趴下 + 饥肠辘辘表情）

---

## 4. 素材方案（关键决策 §6.1）

### 推荐方案 A：基础身体 + 部件层

| 部件 | 数量 | 单张尺寸 | 格式 |
|------|------|----------|------|
| `body_base.png`（无表情/中性站姿） | 1 | 256×256 | PNG @2x 透明 |
| `eye_*`（5 种：开笑/开平/闭笑/闭睡/晕） | 5 | 128×64 | PNG 透明 |
| `mouth_*`（5 种：笑/嘟/O/嚼/平） | 5 | 128×48 | PNG 透明 |
| `brow_*`（3 种：挑/紧/平） | 3 | 128×32 | PNG 透明 |
| 装饰（腮红/汗珠/爱心/耳机/眼镜/手柄/铅笔/Z气泡） | 8 | 64×64~128 | PNG 透明 |
| **合计** | **22 张** | — | **~120KB** |

**优势**：体积小、组合灵活、状态切换 = 切换部件层（无 PNG 重绘）、后续扩展只加部件
**劣势**：部件叠加对齐需要 CSS 精确调位置（一次性工作）

### 备选方案 B：每状态一张完整图

| 状态 | 张数 | 单张尺寸 |
|------|------|----------|
| 13 状态 × 1 视角 | 13 | 256×256 |
| **合计** | **13 张** | **~150KB** |

**优势**：实现简单、每张图都完整
**劣势**：状态间过渡生硬、修改单个表情需重新生成全套、扩展性差

---

## 5. 实施阶段（预计 4 个 sprint，每个 ~3~5 天）

### Phase 1 — 桌宠窗口 MVP（基础能跑）
- Tauri pet 窗口配置 + Rust 端 `window.rs` 创建/销毁
- Vue `PetWindow.vue`（透明 + 置顶 + 拖拽）
- `PetBody.vue` 显示一张静态 `body_base.png`
- 设置页新增「开启桌宠」开关
- **交付**：能拖拽的静态熊猫窗口

### Phase 2 — 表情系统
- ImageGen 批量生成 22 张素材（**首先生成 5 张探索风格，用户拍板后再批量**）
- `PetCanvas.vue` 部件合成渲染
- `stateMachine.ts` 13 状态定义
- **交付**：能切状态的熊猫（手动测试 13 状态）

### Phase 3 — 智能联动 + 自主行为
- Rust `foreground_watcher.rs`（macOS/Windows/Linux 三平台监听）
- `appToState.ts` 映射表（约 20 条规则）
- `idleBehavior.ts` 随机走动/打盹
- **交付**：根据用户软件自动切表情

### Phase 4 — 交互 + 喂食 + 完善
- 点击反应（头/身体/双击/长按）
- 右键 ContextMenu + 喂食系统
- `usePetCursorPassthrough.ts` 鼠标穿透
- i18n `pet.*` 命名空间（约 30~40 key）
- i18n `app.dismissTip` 风格补 key
- 性能优化（图片预加载、rAF 节流、状态切换 < 100ms）
- 跨平台构建（mac 本地 + CI Win/Linux）
- **交付**：可发布的 v0.6.0-beta.1

---

## 6. 关键决策点（2026-07-17 用户拍板）✅

| # | 决策 | 选定 | 影响 |
|---|------|------|------|
| **6.1** | **素材方案** | **A. 基础 + 部件层** | 22 张 PNG / ~120KB / 灵活扩展 |
| **6.2** | **状态切换方式** | **C. 自动监听 + 手动覆盖** | 监听前台应用（自动），右键菜单可手动 |
| **6.3** | **beta 范围** | **A. 完整 13 状态** | idle / 开发中 / 设计中 / 游戏中 / 聊天中 / 工作中 / 摸鱼中 / 开会 / 听歌 / 吃饭 / 困倦 / 开心 / 难过 / 生气 / 惊讶（实际 15 种，含 idle）|
| **6.4** | **是否带音效** | **A. 静默无音效** | 0 音频文件 / 不打扰专注 |
| **6.5** | **桌宠默认大小** | **B. 140×140px** | 介于 QQ 企鹅常规尺寸 |
| **6.6** | **beta 渠道** | **A. GitHub Releases + `v0.6.0-beta.1` tag** | 含预发布标识（GitHub 自动识别）|

---

## 7. 技术风险与缓解

| 风险 | 缓解 |
|------|------|
| Tauri 2 透明窗口鼠标穿透跨平台一致性差 | macOS 用 `set_ignore_cursor_events` + `mapArea` 双保险；Windows 用 `WS_EX_TRANSPARENT`；Linux 优先用 `shape_input_region`（GTK），降级到穿透 |
| 前台应用监听权限（macOS 屏幕录制 / Windows UIAccess） | **不申请额外权限**——`NSWorkspace.frontmostApplication()` 默认可拿 name/bundleId；窗口标题（更细粒度分类用）需要权限才拿 |
| Linux X11/Wayland 行为差异大 | Phase 3 仅实现 X11 路径，Wayland 抛 warn 但不影响其它功能 |
| 桌宠被杀毒/防火墙拦截（透明度窗口常被误判） | README 增加说明 + 提供「诊断日志」一键导出 |
| 包体积增大（22 张 PNG ~120KB + Rust 代码） | 接受；与现状 5~10MB 安装包相比可忽略 |

---

## 8. 与 v0.5.0 的兼容性保证

| 项 | 保证 |
|------|------|
| 数据库 schema | **不动**（pet 数据全走 localStorage / Pinia persist） |
| 现有命令 | **全部保留**（不删除/重命名） |
| Rust tracker trait | **仅追加方法**（不修改现有方法签名） |
| i18n | 仅**追加** `pet.*` 命名空间，不改任何现有 key |
| 设置页 | 在 v0.5.0 的「关于」区块下方新增「桌宠」区块 |
| 现有功能 | Phase 1~4 期间不删不改任何 v0.5.0 代码 |
| 性能 | 桌宠关闭时 `webviewWindow.hide()` 而非 destroy，重开零延迟 |

---

## 9. 验证清单（v0.6.0-beta.1 发布前必查）

- [ ] `npm run build`（vue-tsc + vite）EXIT 0
- [ ] `npm run tauri:build` macOS EXIT 0
- [ ] CI 三端（Win/Linux/mac）构建成功
- [ ] 桌宠窗口可正常拖拽 / 关闭 / 重开（位置持久化生效）
- [ ] 13 个状态在设置页「强制切换」菜单下全部可见
- [ ] 自动监听：vscode → developing、chrome → browsing、steam → gaming 切换 < 1s
- [ ] 鼠标穿透：桌宠空白处点击 → 下层应用正常响应
- [ ] 喂食 5 次后提示「今日上限」
- [ ] i18n：zh-CN / en-US 双语文案无 key 缺失
- [ ] 桌宠关闭时 CPU 0%（`set_ignore_cursor_events` + `window.hide`）
- [ ] `release/v0.6.0-beta.1/NOTES.md` 完整（含与 v0.5.0 对比）

---

**等待用户拍板 §6 后启动 Phase 1。**