# ScreenTime Pro — 架构文档

> 目的：给项目维护者与其他 Agent 提供"一张图看懂全貌"和"改哪里、不改哪里"指南。  
> 与 README 区别：README 是用户面（怎么装、怎么用），本文件是工程面（怎么搭、怎么扩）。  
> 最后更新：2026-07-24（v0.6.2 解耦皮肤系统）

---

## 1. 项目一句话

跨平台应用使用时长追踪（macOS / Windows / Linux），对标 iOS「屏幕使用时间」。**数据 100% 本地（SQLite bundled），零上传，隐私优先**。

栈：**Tauri 2 + Rust + Vue 3 + TypeScript + Vite + Chart.js 4**。当前版本 **v0.6.2-beta.1**（HEAD；尚未发布）。

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
│  - commands.rs: 42 个 #[tauri::command] 路由                    │
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
| `commands.rs` (~1482 行) | 42 个 IPC 命令路由 | 前端 `api/*` 必须对齐字段名 |
| `tracker/{macos,windows,linux}.rs` | 平台采样器（10s tick） | 跨平台数据一致性 |
| `tracker/mod.rs` | trait 抽象 + 后台循环 | — |
| `db/mod.rs` (~1062 行) | SQLite 连接池 + schema 管理 | 数据库迁移 |
| `db/models.rs` | 所有 DTO（前后端对接处） | `src/types.ts` |
| `categorizer.rs` | 自动归类（同步实现！） | 采样循环性能 |
| `logging.rs` | 统一日志订阅器 | 日志路径 / 级别 |

### 3.2 前端结构（src/）
| 目录 | 职责 |
|------|------|
| `src/api/` | Tauri invoke 的薄封装（tracker / config / db） |
| `src/views/` | 主窗口 4 个页面（Dashboard/Trends/Rules/Settings） |
| `src/components/` | 共用组件（Chart 系、AppIcon、DailyBarChart 等） |
| `src/pet/` | **桌宠子系统**（独立 webview） |
| `src/lib/` | 通用工具（logger、format 等） |
| `src/types.ts` | **前后端对接**类型（snake_case 镜像 Rust struct） |

### 3.3 桌宠子系统（src/pet/）— v0.6.2 起分层
```
src/pet/
├── PetWindow.vue          ← webview 根：拖拽 / 交互 / 监听 / 路由
├── components/
│   ├── PetCanvas.vue              （原 2D 皮肤，未改动）
│   ├── PetBody/PetLayer.vue       （部件叠加层，未改动）
│   ├── PetSpriteEditor.vue        （部件编辑器，未改动）
│   ├── PetPreviewStage.vue        （编辑器预览，未改动）
│   ├── PetContextMenu.vue         （右键菜单，新增皮肤切换）
│   └── PetSkinRenderer.vue        （NEW: 皮肤路由器）
├── composables/                （未改动）
│   ├── usePetDrag.ts
│   ├── usePetCursorPassthrough.ts
│   ├── usePetInteractions.ts
│   ├── useForegroundWatcher.ts
│   └── usePetSprites.ts
├── engine/                     （未改动）
│   ├── stateMachine.ts          （16 状态 → 四元组）
│   ├── appToState.ts
│   ├── stateIcons.ts
│   └── spriteLayout.ts          （部件坐标唯一数据源）
├── stores/petStore.ts          （未改动）
├── types.ts                    （未改动）
└── skins/                      ← v0.6.2 NEW: 解耦皮肤层
    ├── types.ts                （PetSkin 接口）
    ├── registry.ts             （注册表 + 持久化 + 订阅）
    ├── index.ts                （引导：注册内置皮肤）
    ├── （panda2d/ 已于 v0.6.2-beta.5 移除：旧 PetCanvas 暴露的 2D 皮肤）
    └── popmart3d/
        ├── index.ts
        ├── PopMartPandaPet.vue
        └── assets/popmart-panda-reference.jpg
```

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

## 5. IPC 契约（42 命令）

Rust 端在 `commands.rs` 注册；前端通过 `src/api/*` 调用。**改 IPC 必须同时改两端**。

| 类别 | 命令 | 说明 |
|------|------|------|
| 追踪 | `start_tracking` / `stop_tracking` / `is_tracking` | 启动/停止/查询采样循环 |
| 实时 | `get_current_foreground` | 当前前台应用（v0.5→v0.6.1 字段名 bug 已修） |
| 数据 | `get_today_summary` / `get_daily_summaries` / `get_app_summaries` | Dashboard/Trends 用 |
| 规则 | `list_rules` / `add_rule` / `update_rule` / `delete_rule` | 用户归类规则 CRUD |
| 桌宠 | `show_pet_window` / `hide_pet_window` / `move_pet_window` / `set_pet_passthrough` | 桌宠窗口控制 |
| 权限 | `check_permissions` / `open_privacy_settings` | macOS 辅助功能 |
| 元 | `check_for_update` / `export_logs` / `get_log_path` | 升级、日志导出 |

完整列表：`rg "#\[tauri::command\]" src-tauri/src/commands.rs`

---

## 6. SQLite 表结构（db/mod.rs / sql/*.sql）

| 表 | 关键列 | 用途 |
|----|--------|------|
| `app_usage` | id, process_name, started_at, ended_at, duration_secs, category_id | 主采样表（append-only） |
| `categories` | id, name, kind ('user'/'auto'/'system') | 归类字典 |
| `rules` | id, match_type('app'/'title'), pattern, category_id | 用户归类规则 |
| `daily_aggregates` | date, total_secs, top_process | 缓存 Dashboard 日聚合 |
| `pet_state` | id (=1), enabled, position_json, fullness, feed_count, level, today_feed_count | 桌宠状态持久化（key 单行） |

**注意**：单行 pet_state 由前端 `petStore` 通过 `tracker` API 间接读写，不直接挂 SQL 命令。**新功能若需持久化表，先在 `sql/` 加迁移文件，db/mod.rs 注册到迁移队列。**

---

## 7. 桌宠皮肤系统（v0.6.2 解耦）

### 7.1 接口契约
```ts
interface PetSkinManifest {
  id: string;                          // 例 'popmart-3d'（v0.6.2-beta.5 起唯一皮肤）
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
| 采样循环**绝不**嵌套 `block_on` 同步函数 | v0.4.0 → v0.4.1 死锁；改 `spawn_blocking` |
| 命令**返回值** snake_case，命令**参数** camelCase | Tauri v2 行为：仅参数转换；`types.ts:66` 旧注释是错的 |
| 所有 `Mutex::lock()` → `unwrap_or_else(|e| e.into_inner())` | poison 雪崩 |
| macOS 权限用 `AXIsProcessTrustedWithOptions({prompt:false})` | ad-hoc 签名下 `AXIsProcessTrusted` 因 identifier 漂移永远返回 false |
| 日志走 `logging.rs` + tauri-plugin-log | 散打日志抓不到；前后端同文件 |
| `usePetSprites` 的 customCompositions **目前**未被 `PetCanvas` 消费 | v0.6.1-beta.1 已知断点（编辑器保存≠桌宠实际生效） |
| 单一坐标数据源 `spriteLayout.ts` | 防止预览/实宠错位（v0.6.0-beta.1 editor bug 教训） |
| 文件头部必带修改历史 (`- YYYY-MM-DD @vX.Y.Z: 类型 - 说明`) | 防 agent 幻视（CONVENTIONS §1.1, §2） |

---

## 10. 约定（详见 CONVENTIONS.md）

- **命名**：Rust snake_case / TS camelCase / DB 字段 snake_case；类型 PascalCase / 常量 UPPER_SNAKE
- **注释**：所有源码强制中文（公开 API、关键设计、复杂算法、行内、注释、TODO/FIXME）
- **文件头**：必带修改历史（`YYYY-MM-DD @vX.Y.Z: 类型 - 说明`）
- **版本同步点**：`src-tauri/tauri.conf.json` 的 `version` 为唯一真实来源；UI 硬编码版本号用 `app.getVersion()`
- **发布红线**：构建成功 ≠ 可发布；**必须等用户明确「发布」** 才能 push/tag/release

---

## 11. 文件清单速查

```
src-tauri/
├── Cargo.toml           ← Rust 依赖
├── tauri.conf.json      ← VERSION IS HERE
└── src/
    ├── main.rs          ← App 入口
    ├── commands.rs      ← 42 IPC 命令
    ├── tracker/         ← 平台采样
    ├── db/              ← SQLite
    ├── categorizer.rs
    └── logging.rs

src/
├── App.vue              ← 主窗口/桌宠分支根
├── types.ts             ← DTO 镜像（snake_case）
├── api/                 ← invoke 包装
├── views/               ← 主窗口 4 页
├── components/          ← 通用组件
├── pet/                 ← 桌宠子系统
└── lib/                 ← 工具

docs/
├── ARCHITECTURE.md      ← 本文件
├── RELEASE.md           ← 版本管理与一键发布
└── pet-assets-manifest.md ← 桌宠素材清单

release/v0.x.y/          ← 历史版本归档
release/v0.x.y/NOTES.md  ← 本版本变更说明
sql/                     ← SQLite 迁移
```

---

## 12. 自我测试 / 自测入口

| 路径 | 用途 |
|------|------|
| `output/2026-07-21/pet-self-test-checklist.md` | v0.6.0-beta 自测清单（7 段） |
| `output/2026-07-24/pet-sprite-debug.html` | 23 素材逐张 + 合成预览 |
| `release/v0.X.Y/` | 本地 dmg / AppImage / deb 自测包 |

---

**遇到不确定**：先读本文件 §3 模块地图 + §8 扩展指南，再动手改。改完务必补 §10 注释与修改历史。
