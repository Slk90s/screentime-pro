# ScreenTime Pro — 架构文档

<!--
备份记录：2026-09-04 备份至 bak/docs/ARCHITECTURE.md.bak（操作类型：局部更新 - 版本号对齐
  v0.7.5、修正 IPC 命令名、补 classifier.rs / system_load/ 模块、更新桌宠文件树；
  本文件为文档无 SemVer 版本号，版本声明跟随 tauri.conf.json）

修改历史：
  - 2026-09-04 @v0.7.5: 修复 - 头部版本声明由 v0.7.0 更新为 v0.7.5（实际代码已是 v0.7.5）
  - 2026-09-04 @v0.7.5: 修复 - §5 IPC 契约表命令名与实际代码不一致（list_rules→get_rules、
    get_today_summary→get_overview、get_app_summaries→get_app_ranking）
  - 2026-09-04 @v0.7.5: 新增 - §3.1 补 classifier.rs 与 system_load/ 两个模块地图条目
  - 2026-09-04 @v0.7.5: 修正 - §3.3 桌宠文件树与 §11 文件清单同步至当前实际结构
-->

> 目的：给项目维护者与其他 Agent 提供"一张图看懂全貌"和"改哪里、不改哪里"指南。  
> 与 README 区别：README 是用户面（怎么装、怎么用），本文件是工程面（怎么搭、怎么扩）。  
> 最后更新：2026-09-04（同步至 v0.7.5；补齐 RELEASE.md / CONVENTIONS 死链）

---

## 1. 项目一句话

跨平台应用使用时长追踪（macOS / Windows / Linux），对标 iOS「屏幕使用时间」。**数据 100% 本地（SQLite bundled），零上传，隐私优先**。

栈：**Tauri 2 + Rust + Vue 3 + TypeScript + Vite + Chart.js 4 + vue-i18n 9**。当前版本 **v0.7.6**（已发布，详见 [`RELEASE.md`](RELEASE.md)）。

> ⚠️ 本节版本号在 v0.7.0 → v0.7.5 期间**长期未更新**（曾停留在 v0.7.0），与 `tauri.conf.json` 脱节。
> 版本号唯一真实来源是 **`src-tauri/tauri.conf.json` 的 `version`**，改版本时务必回来同步本节。

---

## 2. 三层架构

```
┌─────────────────────────────────────────────────────────────────┐
│  Renderer (Tauri WebView, Vue 3 frontend)                       │
│  - 主窗口 Dashboard / Trends / Rules / Settings                 │
│  - 桌宠独立 webview: PetWindow → PetSkinRenderer → PetSkin      │
│  - API 抽象: src/api/*（tracker / config / db 等的 invoke 包装） │
└────────────────────┬────────────────────────────────────────────┘
                     │ Tauri IPC (snake_case 入参 → camelCase 参数 / 原样返回值)
┌────────────────────┴────────────────────────────────────────────┐
│  Core (Rust, src-tauri/src)                                     │
│  - commands.rs: 主 IPC 命令路由（+ pet/ 子模块的桌宠窗口/菜单命令）                    │
│  - tracker/{macos,windows,linux}.rs: 平台采样器                 │
│  - categorizer.rs: 自动归类（Wikipedia + 本地字典 + LRU 缓存）   │
│  - db/: SQLite (rusqlite bundled, prepared statements)          │
│  - logging.rs + tauri-plugin-log: 统一日志                      │
└────────────────────┬────────────────────────────────────────────┘
                     │ 原生 API
┌────────────────────┴────────────────────────────────────────────┐
│  OS (macOS Accessibility API / Win32 / /proc)                    │
└─────────────────────────────────────────────────────────────────┘
```

**关键不变量**：所有跨进程数据流走 Tauri IPC。**Tauri v2 仅把命令**参数** camelCase ↔ snake_case，命令**返回值**原样序列化（Rust struct 字段名直接到前端）。**`types.ts` 注释里"自动转 camelCase"的旧说法是错的，已修正。

---

## 3. 模块地图（"改哪里"）

### 3.1 Rust 后端（src-tauri/src/）
| 模块 | 职责 | 改我会影响 |
|------|------|------------|
| `commands.rs` (~968 行) | 主 IPC 命令路由（48 个 `#[tauri::command]`；桌宠窗口/菜单命令 9 个在 `pet/` 子模块，合计 **57** 个） | 前端 `api/*` 必须对齐字段名 |
| `lib.rs` (~478 行) | Tauri Builder、插件注册、`generate_handler!` 命令注册、系统托盘 | **新命令必须在此注册**，否则 invoke 报不存在 |
| `tracker/{macos,windows,linux}.rs` | 平台采样器（10s tick） | 跨平台数据一致性 |
| `tracker/mod.rs` | trait 抽象 + 后台循环 | — |
| `tracker/platform.rs` | `RawApp` 跨平台原始数据结构 | `classifier.rs` 入参 |
| `db/mod.rs` (~1148 行) | SQLite 连接池 + schema 管理 + 迁移队列 | 数据库迁移 |
| `db/models.rs` | 所有 DTO（前后端对接处） | `src/types.ts` |
| **`classifier.rs`** (~181 行) | **分类规则引擎（纯逻辑，不依赖 DB）**：给定 `classification_rules` 规则 + 应用 → 分类 id。规则由上层内存缓存后传入 | 规则匹配语义。**`pattern` 与 `value` 两侧都必须小写化**，否则 `equals` 永不命中（v0.6.2-beta.17 踩坑） |
| `categorizer.rs` (~347 行) | 兜底自动归类：本地字典（60+ 软件）+ Wikipedia API + LRU 缓存 → `other`。**同步实现！** | 采样循环性能 |
| **`system_load/`** (~472 行) | **v0.7.5 新增**：`MetricsSampler` 统一指标采样器，macOS 托盘显示 CPU / 内存 / 磁盘占用率（1s 采样、磁盘 30s 缓存、变化 <1% 不重绘托盘）。Win/Linux 默认隐藏 | 仅 macOS 托盘，不影响采样主流程 |
| `logging.rs` | 统一日志订阅器 | 日志路径 / 级别 |

> **`classifier.rs` 与 `categorizer.rs` 是两个不同东西**，勿混淆：
> `classifier` 按**用户定义的规则**匹配（纯计算、可单测）；`categorizer` 是规则都没命中时的**兜底**
> （本地字典 → 联网 Wikipedia → `other`），且**必须保持同步实现**（v0.4.1 死锁教训，见 §9）。

### 3.2 前端结构（src/）
| 目录 | 职责 |
|------|------|
| `src/api/` | Tauri invoke 的薄封装（tracker / config / db） |
| `src/views/` | 主窗口 4 个页面（Dashboard/Trends/Rules/Settings） |
| `src/components/` | 共用组件（Chart 系、AppIcon、DailyBarChart 等） |
| `src/pet/` | **桌宠子系统**（独立 webview） |
| `src/lib/` | 通用工具（logger、format 等） |
| `src/types.ts` | **前后端对接**类型（snake_case 镜像 Rust struct） |

### 3.3 桌宠子系统（src/pet/）

桌宠是独立透明置顶 webview，与 `main` 主窗口经 Tauri 全局事件 + `localStorage` 跨窗口同步（见 §7.4）。当前共 3 个窗口：`main`（设置）、`pet`（桌宠本体）、`pet-menu`（右键菜单，v0.6.2-beta.17 起独立窗口）。
```
src/pet/
├── PetWindow.vue          ← 桌宠 webview 根：拖拽 / 交互 / 监听 / 路由（381 行）
├── PetMenuWindow.vue      ← 右键菜单**独立窗口**根（v0.6.2-beta.17 起独立成窗，119 行）
├── components/
│   ├── PetSkinRenderer.vue      （皮肤路由器：:is 动态组件 + :key 强制重建）
│   ├── PetContextMenu.vue        （右键菜单主体：皮肤切换 / 喂食 / 状态，855 行最重）
│   ├── PetCanvas.vue             （原 2D 渲染，150 行）
│   ├── PetBody.vue / PetLayer.vue（部件叠加层）
│   ├── PetBubble.vue             （气泡对话，104 行）
│   ├── PetSpriteEditor.vue       （部件编辑器，546 行）
│   └── PetPreviewStage.vue       （编辑器预览，63 行）
├── composables/
│   ├── usePetDrag.ts             （拖拽：越 4px 阈值才交 OS startDragging）
│   ├── usePetCursorPassthrough.ts（鼠标穿透）
│   ├── usePetInteractions.ts     （点击：跳跃 / 压扁 / 抖动）
│   ├── useForegroundWatcher.ts   （每 2s 轮询 tracker.current()）
│   ├── usePetSprites.ts          （自定义部件合成）
│   ├── usePetBadges.ts           （徽章状态）
│   ├── usePetBubble.ts           （气泡文案 / 随机弹出）
│   └── useSystemOverloadWatcher.ts（系统过载 → 桌宠升温抖动，v0.7.0 阈值已上调）
├── engine/                     （未改动）
│   ├── stateMachine.ts          （16 状态 → 四元组）
│   ├── appToState.ts
│   ├── stateIcons.ts
│   └── spriteLayout.ts          （部件坐标唯一数据源）
├── config/bubble-phrases.json   （气泡台词词条）
├── assets/sprites/              （23 个旧版部件精灵：eye_* / mouth_* / brow_* / 装饰）
├── stores/petStore.ts          （状态 + localStorage 持久化，289 行）
├── types.ts
└── skins/                      ← 解耦皮肤层（注册表模式，即插即用）
    ├── types.ts                （PetSkinManifest 接口）
    ├── registry.ts             （注册表 + 持久化 + 订阅，137 行）
    ├── index.ts                （引导：注册内置皮肤）
    ├── popmart3d/              （Pop Mart 3D 熊猫）
    │   ├── PopMartPandaPet.vue
    │   └── assets/            （14 个文件：single / transparent / body / eyes / mouth /
    │                           nose / frame-0~5 待机帧 / reference.jpg / debug-stack）
    └── spiderman/              （蜘蛛侠）
        ├── SpiderManPet.vue
        └── assets/            （10 个：frame-0~5 + pose-{hero,swing,web,crouch}）
```

> 早期"4 层精灵拼贴"（body/eyes/mouth/nose 分别切图）因切片羽化导致五官割裂，已废弃；现行方案为**单张透明 PNG + 表情/姿势精灵叠加**。旧规格 `docs/pet-assets-manifest.md` 已从仓库移除（仅本地保留）。

> ⚠️ **本节文件树曾滞后于实际结构**（2026-09-04 同步）：此前未收录 `PetMenuWindow.vue`、
> `PetBubble.vue` 与 `usePetBadges` / `usePetBubble` / `useSystemOverloadWatcher` 三个 composables；
> 且 `popmart3d` 描述为"单张透明 PNG"，实际目录含 14 个素材文件（v0.7.1 为修复"皮肤源码资产
> 缺失入库"而补齐）。**以实际目录为准。**

---

## 4. 数据流示例

### 4.1 主窗口实时指示（已修的 HIGH bug）
```
Rust: get_current_foreground() → CurrentForegroundOut {
  name: String, process_name, window_title?, category_id,
  idle_seconds: u64, session_seconds: u64, tracking: bool
}
（snake_case 原样序列化）

↓ invoke

src/api/tracker.ts:        tracker.current() = invoke('get_current_foreground')

↓ ref

src/App.vue:               live.value.process_name / window_title / session_seconds
                           （注意是 snake_case，不是 camelCase）
```

### 4.2 桌宠状态联动
```
tracker.current() 每 2s 轮询
    ↓
composables/useForegroundWatcher.ts
    ↓ 匹配规则 engine/appToState.ts
petStore.setState(newState)
    ↓ reactive 传播
PetWindow.effectiveState (computed)
    ↓ props
PetSkinRenderer.state
    ↓ :is 动态组件
当前激活皮肤的 renderer(state)
```

### 4.3 皮肤切换（v0.6.2 NEW）
```
用户在右键菜单点击"Pop Mart 3D"
    ↓
PetContextMenu.onPickSkin('popmart-3d')
    ↓
skinRegistry.setActive('popmart-3d')
    ↓ 持久化 localStorage['screentime-pet-skin']
    ↓ notify listeners
PetSkinRenderer.watchEffect → skinTick++
    ↓ :key 强制重建 <component :is>
新皮肤的渲染器接管渲染
    ↓
旧皮肤的实例被销毁（无资源需清理，皮肤组件目前无状态）
```

---

## 5. IPC 契约（当前 57 个命令）

Rust 端在 `commands.rs` / `pet/` 定义，**在 `lib.rs` 的 `generate_handler!` 注册**；前端通过 `src/api/*` 调用。**改 IPC 必须同时改两端，并记得注册。**

| 类别 | 命令 | 说明 |
|------|------|------|
| 追踪 | `start_tracking` / `stop_tracking` / `is_tracking` | 启动/停止/查询采样循环 |
| 实时 | `get_current_foreground` | 当前前台应用（v0.5→v0.6.1 字段名 bug 已修） |
| 数据 | `get_overview` / `get_daily_summaries` / `get_daily_categories` / `get_month_summary` / `get_hourly_buckets` / `get_app_ranking` / `get_sessions` | Dashboard / Trends 用 |
| 趋势 | `get_trends` | 周 / 月同比 |
| 规则 | `get_rules` / `add_rule` / `update_rule` / `delete_rule` / `reclassify_all` | 归类规则 CRUD + 一键重算历史 |
| 分类 | `get_categories` | 分类字典 |
| 设置 | `get_settings` / `save_settings` / `set_idle_threshold` / `get_idle_threshold` | 配置与空闲阈值 |
| 自启 | `set_autostart` / `is_autostart` / `get_autostart_pref` | 开机自启 |
| 数据管理 | `export_all` / `export_data` / `import_data` / `prune_data` / `backup_and_prune_device` / `get_backup_config` / `save_backup_config` / `run_backup_now` | 导出导入、清理、本地自动备份 |
| 多设备 | `get_devices` / `list_devices_with_stats` | 多设备合并 |
| 桌宠（`pet::`） | `create_pet_window` / `show_pet_window` / `hide_pet_window` / `move_pet_window` / `set_pet_cursor_passthrough` / `create_pet_menu_window` / `show_pet_menu_window` / `hide_pet_menu_window` / `move_pet_menu_window` | 桌宠窗口 + 右键菜单窗口（共 9 个） |
| 系统指标 | `get_system_metrics` / `get_system_metrics_enabled` / `set_system_metrics_enabled` | v0.7.5 新增，macOS 托盘 CPU/内存/磁盘 |
| 权限 | `check_permissions` / `open_privacy_settings` | macOS 辅助功能 |
| 环境 | `check_webview2` / `open_webview2_download` / `reveal_path` | Windows WebView2 检测等 |
| 元 | `check_for_update` / `open_url` / `export_logs` / `get_log_size` / `get_log_dir` | 升级、日志导出 |

完整列表（权威）：`src-tauri/src/lib.rs` 的 `.invoke_handler(tauri::generate_handler![...])`。

> ⚠️ **本节曾长期与实际代码不符**，以下旧写法会直接报「命令不存在」，勿再沿用：
> `list_rules` → 实际 `get_rules`；`get_today_summary` → 实际 `get_overview`；
> `get_app_summaries` → 实际 `get_app_ranking`；`set_pet_passthrough` → 实际 `set_pet_cursor_passthrough`；
> `get_log_path` → 实际 `get_log_dir`。

---

## 6. SQLite 表结构（sql/schema.sql 为唯一权威）

数据库由 `rusqlite` bundled 在首次启动时按 `sql/schema.sql` 自动建表；`seed_categories.sql` / `seed_rules.sql` 注入初始分类与归类规则。**以 `sql/schema.sql` 为准**，下表为当前结构：

| 表 | 关键列 | 用途 |
|----|--------|------|
| `apps` | id, name, process_name(UNIQUE+platform), exe_path, icon_blob, category_id, platform | 应用主表（每进程每平台一行） |
| `sessions` | id, app_id(FK), start_at, end_at, duration_seconds, date, window_title, device | 主采样表（append-only，每次使用一段） |
| `daily_summaries` | id, date, app_id, total_seconds, session_count(UNIQUE date+app_id) | 日聚合缓存（Dashboard/Trends 用） |
| `categories` | id(TEXT PK), name, color | 归类字典（social/productivity/...） |
| `classification_rules` | id, field(process_name/window_title/exe_path/bundle_id/name), match_type(contains/equals/prefix/suffix/regex), pattern, category_id, priority, enabled | 自动归类规则（约 40 条种子） |
| `settings` | key(TEXT PK), value | 简单键值配置（如 autostart 用户偏好） |

**注意**：桌宠状态（开关/位置/喂食/好感度）**不落 SQL**，由前端 `petStore` 经 Pinia persist 写入 `localStorage`，跨窗口通过 `localStorage` 共享。**新功能若需持久化表，先在 `sql/` 加迁移文件，db/mod.rs 注册到迁移队列。**

---

## 7. 桌宠皮肤系统（v0.6.2 解耦）

### 7.1 接口契约
```ts
interface PetSkinManifest {
  id: string;                          // 例 'popmart-3d' / 'spiderman'（当前内置两套皮肤）
  name: string;                        // UI 展示名
  version: string;
  description?: string;                // tooltip 用
  renderer: Component<{ state: PetState }>;  // 必须接 state prop
  decorations?: Partial<Record<PetState, string[]>>;  // 可选
}
```

### 7.2 渲染协议
```
PetWindow.vue
  └─ <PetSkinRenderer :state :is-dragging :class="animClass" />
       └─ <component :is="skinRegistry.active().renderer" :key="skinTick" :state />
            └─ 每个皮肤声明自己的 state 响应逻辑（props 透传）
```

`is-dragging` 和 `animClass` 通过 Vue `inheritAttrs` 自动落到皮肤根元素，**所有皮肤自动继承点击动效和拖拽状态**。

### 7.3 解耦边界（"不要影响"清单）

修改以下文件**不会**影响新皮肤机制：
- 任何已有的 `components/Pet*.vue`（PetCanvas/Body/Layer/SpriteEditor/PreviewStage）
- `engine/*`
- `composables/*`
- `stores/petStore.ts`
- `types.ts`

修改这些**只影响皮肤层**（无需改前端主流程）：
- `skins/<name>/*`
- `skins/registry.ts`（接口扩展时）
- `skins/index.ts`（注册新皮肤时）

修改这些**影响所有皮肤**（慎动）：
- `components/PetSkinRenderer.vue`（协议）

### 7.4 多窗口状态同步（跨 webview 陷阱）

桌宠涉及 3 个 Tauri 窗口 / webview：`main`（设置）、`pet`（桌宠本体）、`pet-menu`（右键菜单）。**每个 webview 各持一份 `petStore` 模块级实例**（仅 `localStorage` 共享），彼此不自动同步。跨窗口同步靠 Tauri 全局事件 + `petStore.reload()` 重读 `localStorage`：

| 事件 | 触发方 | 监听方动作 |
|------|--------|------------|
| `pet-skin-changed` | Settings 切皮肤 | PetWindow / pet-menu 调 `skinRegistry.reloadActive()` |
| `pet-custom-updated` | 编辑器改表情/素材 | PetWindow `reloadBadges` / `reloadConfig` |
| `pet-store-updated` | 菜单改状态/喂食/位置 | PetWindow `petStore.reload()`（不读 `state`，避免覆盖前台自动状态） |

> 改任何"某窗口写 store、另一窗口需看到"的逻辑，必须广播对应事件并在目标窗口 `reload`，否则出现"菜单操作桌宠不反应"类 bug。

### 7.5 拖拽与渲染性能

- **拖拽跟手**：越过 4px 阈值才调用 Tauri 原生 `getCurrentWindow().startDragging()`，把拖拽交给 OS（零 IPC、零延迟）；未越阈值走点击交互，不吞单击/双击。
- **透明置顶窗性能**：投影/滤镜**绝不**与 `transform` 动画放在同一元素（filter 会破坏 GPU 合成层，每帧重算投影）；投影放静态 `img` 层一次性光栅化，动画元素只做 `transform`。升温/抖动用 `transform` 抖动而非 `hue-rotate` 滤镜。

---

## 8. 扩展指南

### 8.1 新增一个宠物皮肤
```bash
1. mkdir src/pet/skins/<my-skin>/
2. 创建 <MySkin>.vue（含 <script setup> export default { props: { state: PetState } }）
3. 创建 assets/（背景图、装饰浮层等）
4. 创建 index.ts:
      import type { PetSkinManifest } from '../types';
      import MySkin from './MySkin.vue';
      export default { id: 'my-skin', name: 'My Skin', version: '0.x', renderer: MySkin };
5. 在 src/pet/skins/index.ts 加一行：skinRegistry.register(myManifest);
6. 完成。自动出现在右键菜单"皮肤"切换区。
```

### 8.2 新增一个"类似桌宠"的小部件（如待办、便签、番茄钟）
建议照搬 `skins/` 的模式，扩展为 `widgets/`：
```bash
src/widgets/
├── types.ts                 // WidgetManifest 接口
├── registry.ts              // 同样的注册模式
├── index.ts                 // 引导
├── todo/                    // 待办小部件
├── pomodoro/                // 番茄钟
└── ...
```

**接口契约**（建议初稿）：
```ts
interface WidgetManifest {
  id: string;
  name: string;
  size: { width: number; height: number };
  mount(el: HTMLElement, ctx: WidgetContext): void;
  update?(state: object, ctx: WidgetContext): void;
  unmount?(el: HTMLElement): void;
}
```

桌宠是"悬挂在桌面的任意 webview"，未来 widget 可走**同一 Tauri 窗口 + 同一 PetWindow 协议**，只是渲染层挂不同组件。

### 8.3 跨进程持久化新表
1. `sql/<feature>.sql` 写迁移
2. `db/mod.rs::migrate()` 注册该文件
3. `db/models.rs` 加对应 DTO
4. `commands.rs` 加 `#[tauri::command]` 暴露
5. `src/types.ts` 镜像同名 DTO（snake_case）
6. `src/api/<feature>.ts` 加 wrapper

---

## 9. 关键不变量（踩坑沉淀）

| 不变量 | 为什么 |
|--------|--------|
| 采样循环**绝不**嵌套 `block_on` 同步函数 | v0.4.0 → v0.4.1 死锁；改 `spawn_blocking` / `reqwest::blocking` |
| 命令**返回值** snake_case，命令**参数** camelCase | Tauri v2 行为：仅参数转换；`types.ts:66` 旧注释是错的 |
| `classifier` 比较时 `value` 与 `pattern` **两侧都小写化** | 只小写一侧则 `equals` 永不命中 → "规则没生效"（v0.6.2-beta.17） |
| 所有 `Mutex::lock()` → `unwrap_or_else(|e| e.into_inner())` | poison 雪崩 |
| macOS 权限用 `AXIsProcessTrustedWithOptions({prompt:false})` | ad-hoc 签名下 `AXIsProcessTrusted` 因 identifier 漂移永远返回 false |
| 日志走 `logging.rs` + tauri-plugin-log | 散打日志抓不到；前后端同文件 |
| `usePetSprites` 的 customCompositions **目前**未被 `PetCanvas` 消费 | v0.6.1-beta.1 已知断点（编辑器保存≠桌宠实际生效） |
| 单一坐标数据源 `spriteLayout.ts` | 防止预览/实宠错位（v0.6.0-beta.1 editor bug 教训） |
| 文件头部必带修改历史 (`- YYYY-MM-DD @vX.Y.Z: 类型 - 说明`) | 防 agent 幻视（CONVENTIONS §1.1, §2） |

---

## 10. 约定（详见 [`CONVENTIONS-screentime-pro.md`](../CONVENTIONS-screentime-pro.md)）

- **命名分层**：DB 列 `snake_case` / Rust 字段与函数 `snake_case` / IPC 参数 camelCase↔snake_case 自动转 / **IPC 返回值原样 snake_case** / 前端内部 `camelCase`；类型 PascalCase / 常量 UPPER_SNAKE
- **注释**：所有源码强制中文（公开 API、关键设计、复杂算法、行内、注释、TODO/FIXME）
- **文件头**：必带修改历史（`YYYY-MM-DD @vX.Y.Z: 类型 - 说明`）
- **版本同步点**：`src-tauri/tauri.conf.json` 的 `version` 为唯一真实来源；UI 硬编码版本号用 `app.getVersion()`。**`Cargo.toml` 的 version 不参与发布，勿顺手对齐**
- **发布红线**：构建成功 ≠ 可发布；**必须等用户明确「发布」** 才能 push/tag/release。流程见 [`RELEASE.md`](RELEASE.md)

> 本条在 v0.7.5 之前写的是「详见 CONVENTIONS.md」，但该文件从未存在（死链）。
> 项目专属约束已于 2026-09-04 建立，路径为仓库根 **`CONVENTIONS-screentime-pro.md`**；
> 与 `AGENTS.md` / `CHANGELOG.md` 同属约束类文档，已由 `.gitignore` 排除，不传 GitHub。

---

## 11. 文件清单速查

```
根目录（约束类文档，已 gitignore，不传 GitHub）
├── CONVENTIONS-screentime-pro.md ← 项目专属代码约束（本空间首要约束，见 §10）
├── CHANGELOG.md                  ← 本地详细变更留底
└── AGENTS.md                     ← （建议补）项目级 Agent 启动规则

src-tauri/
├── Cargo.toml           ← Rust 依赖（version 为 0.1.0，**不参与发布**，勿对齐）
├── tauri.conf.json      ← VERSION IS HERE（唯一真实来源，决定 CI tag 与产物名）
└── src/
    ├── main.rs          ← App 入口（仅转调 lib）
    ├── lib.rs           ← Tauri Builder / 插件 / generate_handler! 命令注册 / 托盘
    ├── commands.rs      ← 主 IPC 命令路由（48 个）+ pet/ 子模块桌宠命令（9 个）
    ├── pet/             ← 桌宠 Rust 端（window / menu_window）
    ├── tracker/         ← 平台采样（mod / platform / macos / windows / linux）
    ├── db/              ← SQLite（mod 1148 行 / models）
    ├── classifier.rs    ← 分类规则引擎（纯逻辑，不依赖 DB）
    ├── categorizer.rs   ← 兜底归类：字典 + Wikipedia + LRU（必须同步实现）
    ├── system_load/     ← v0.7.5 macOS 托盘系统指标（mod / macos / linux / windows / metrics）
    ├── logging.rs
    └── error.rs

src/
├── App.vue              ← 主窗口/桌宠分支根
├── types.ts             ← DTO 镜像（snake_case，Rust struct 原样）
├── api/                 ← invoke 包装 + mock（无 Tauri 环境调试用）
├── views/               ← 主窗口 4 页（Settings 1506 行最重）
├── components/          ← 通用组件 + 图表
├── pet/                 ← 桌宠子系统（含 skins/ 两套皮肤）
├── i18n/                ← vue-i18n 词条（zh-CN / en-US，必须双语同步）
├── utils/               ← 格式化
└── lib/                 ← logger

docs/                    ← ⚠️ .gitignore 默认排除 docs/*.md，仅以下两份白名单入库
├── ARCHITECTURE.md      ← 本文件（工程面）  [入库]
└── RELEASE.md           ← 版本管理与发布流程（唯一权威）  [入库]

# 以下为本地工作目录（已 gitignore，不入库，仅本机参考）：
#   release/v0.x.y/      历史版本归档与 Release Notes（本地草稿）
#   output/              本地构建产物 / 安装包
# 计划/规格类文档（pet-assets-manifest / I18N_PLAN / v060-pet-plan 等）已移出仓库，仅本地保留。

sql/                     ← SQLite 迁移（已恢复入库，为数据库唯一权威）
.github/workflows/build.yml ← 三平台 CI，push v* tag 触发
```

> **往 `docs/` 新增需公开的文档时，必须同步在 `.gitignore` 加白名单例外**
> （当前仅 `!docs/ARCHITECTURE.md`、`!docs/RELEASE.md`），否则会被 git 静默忽略——
> `RELEASE.md` 此前"被引用却不存在"正是这个原因。

---

## 12. 本地自测入口（均在本地工作目录，已 gitignore，不入库）

| 路径 | 用途 |
|------|------|
| `output/` | 本地构建产物、安装包、临时调试文件 |
| `release/v0.X.Y/` | 本地 dmg / AppImage / deb 自测包与 Release Notes 草稿 |
| 各版本 `NOTES.md` | 发布说明草稿（**需人工**回填到 `build.yml` 的 `releaseBody`，见下） |

> ⚠️ `build.yml` 的 `releaseBody` 是**硬编码死文本**，不会自动跟随版本。
> 当前仓库三份（windows / linux / macos job）仍是 **v0.7.0 的旧文案**。
> 发版前必须手动替换，否则线上 Release Notes 显示错误版本的内容。
> 模板与完整流程见 [`RELEASE.md`](RELEASE.md) §3.1 / §5。

---

**遇到不确定**：先读本文件 §3 模块地图 + §8 扩展指南，再动手改。改完务必补 §10 注释与修改历史。
