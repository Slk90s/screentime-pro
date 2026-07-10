# screentime-pro：「default / 未命名」设备根因报告

> 调查日期：2026-07-10 · 涉及版本：v0.4.2（release, debug=false）
> 调查对象：`~/Library/Application Support/com.screentime.pro/screentime.db`

## 结论（前置）

**这不是当前生产代码的 active bug。** 那条 `device='default'` 的记录是**一次性事件**，由两个因素叠加造成：

1. 一条**旧 schema 写入**的 session（来自 `tauri dev` 调试构建自动启动，落在生产 DB 路径）；
2. v0.4.2 首次打开该 DB 时执行 Schema 迁移 `ALTER TABLE sessions ADD COLUMN device TEXT NOT NULL DEFAULT 'default'`，把这条「没有 device 列」的旧行**回填成了字面量 `'default'`**。

当前代码的 `insert_session` 始终传入真实 `device_id`，**此后不会再产生 `default` 记录**。

---

## 一、证据（实测）

| 项目 | 实测结果 |
|------|----------|
| `default` 记录数 | **仅 1 条**（id=997，2026-07-10 14:37:44→14:38:02，17s，app=SecurityAgent，window_title=NULL） |
| 真实设备记录数 | 1065 条，均带 `device='157e08ae9820aaa5'`（settings 中 `device_name='mac mini'`） |
| 总记录 / 时间跨度 | 1066 条，2026-07-08 → 2026-07-10（实时仍在写，WAL 16:16 更新） |
| 插入代码 | `commands.rs:146` → `insert_session(..., &state.device_id, ...)`；`device` 字段**必传**，无代码路径会写 `'default'` |
| 唯一产生 `'default'` 的机制 | `db/mod.rs:48-52` 迁移：`ALTER TABLE sessions ADD COLUMN device TEXT NOT NULL DEFAULT 'default'`（回填既有行） |
| LaunchAgent | `~/Library/LaunchAgents/ScreenTime Pro.plist` 仅 1 个，现指向 `/Applications/ScreenTime Pro.app/...`（14:38 安装覆盖）；`RunAtLoad=true`（已装应用的正常自启） |
| 启动日志 | `app.2026-07-10.log` 第 105 行：`14:38:14 INFO ... 启动 version=0.4.2 debug=false`（无 14:37 日志——调试版来自加日志前的旧 commit，不写日志文件） |

---

## 二、完整根因时间线

1. **14:37:23 前后** —— 一个 **`tauri dev` 调试构建**（编译自「多设备特性之前」的旧 commit，其 `schema.sql` **没有 `device` 列**、`insert_session` 也不传 device）被 LaunchAgent 自动拉起。它运行约 17 秒，抓到前台进程 `SecurityAgent`，往**生产 DB 路径**写入 1 条 session（此时该表无 `device` 列）。
2. **14:38:14** —— 你安装了 v0.4.2（安装包把 LaunchAgent 重写为指向 `/Applications` 正式版）。v0.4.2 启动，`migrate()` 检测到 `sessions` 缺 `device` 列，执行 `ALTER TABLE ... ADD COLUMN device TEXT NOT NULL DEFAULT 'default'`。该 `DEFAULT` 把步骤 1 那条既有行**回填为 `'default'`**。
3. **14:38 之后** —— 全部 1065 条新 session 由 v0.4.2 写入，均带真实 `device_id='157e08ae9820aaa5'`，界面正常显示为「mac mini」。

> 推断点：步骤 1 的「调试版无 device 列」是对单一 `default` 行 + 其余 1065 条均正确的唯一自洽解释；日志缺失（旧 commit 无日志模块）与该推断一致。

---

## 三、这是 bug 吗？

| 维度 | 判定 | 说明 |
|------|------|------|
| v0.4.2 运行时写入 | ✅ 无 bug | `insert_session` 恒传真实 `device_id`，新数据正确归类，不会复发 |
| Schema 迁移回填逻辑 | ⚠️ 设计缺陷（技术债/轻微 bug） | 用字面量 `'default'` 回填旧数据，会在**任何升级前已有数据的用户**界面里制造一个幽灵「default / 未命名」设备（本机仅 17s，他人可能很大） |
| 眼前的 1 条 `default` | ✅ 一次性事件（开发环境残留） | 由过期 `tauri dev` LaunchAgent 触发，非产品缺陷；删/改后不再出现 |
| LaunchAgent 现状 | ✅ 正常 | 当前 plist 指向正式版，仅 1 个；自启是已装应用预期行为 |

**一句话**：生产代码没问题；问题在「迁移回填用 `'default'` 字面量」这一设计选择，以及你本机那个过期的调试启动器恰好在升级前写了 1 条数据。

---

## 四、建议修复（待你确认是否动手）

- **A. 代码修复（防他人复现）**：把 `db/mod.rs:48-52` 的回填改成用真实 `device_id`（从 settings 读取）而非 `'default'`；并把 `device` 列补进 `sql/schema.sql` 保持基表一致。
- **B. 清理本机这条脏数据（任选其一）**：
  - 删除：`DELETE FROM sessions WHERE id=997;`
  - 或改标签：`UPDATE sessions SET device='157e08ae9820aaa5' WHERE device='default';`
- **C. 开发环境清理**：确认无第二个调试版 LaunchAgent 残留（已核实仅 1 个且指向正式版，无需处理）。

> 注：B 属于对你生产 DB 的写操作，需你明确确认后我再执行。
