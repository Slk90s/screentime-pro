//! 本地数据库层（SQLite）
//!
//! 所有使用记录仅保存在本机 SQLite，零上传，隐私优先。
//! 数据库文件位于应用数据目录（macOS: ~/Library/Application Support/com.screentime.pro/screentime.db）。
//! 建表语句见 `sql/schema.sql`，分类种子见 `sql/seed_categories.sql`，规则种子见 `sql/seed_rules.sql`。
//!
//! ## daily_summaries 已停写（2026-10-22 @v1.2.5）
//!
//! `daily_summaries` 原本是 `(date, app_id)` 的预聚合表，每次 `insert_session`
//! 与备份导入都会额外跑一条 UPSERT 维护它。**但全仓库自始至终没有任何 SELECT 从它读**：
//! `get_daily_summaries()` 虽然同名，实际 SQL 是从 `sessions` 现场 `GROUP BY` 聚合的
//! （见本文件 `get_daily_summaries` 实现）。也就是说这张表只写不读，
//! 每笔记录白白多付一次写放大 + B-tree 维护 + WAL 帧增长。
//!
//! 处置：**停写**（两处 INSERT 已移除），**保留表结构与 `prune_old` 的 DELETE**——
//! 老库里的历史行继续留着不碍事（无人读），将来若真要做预聚合可直接复用该表。
//! ⚠️ 若将来启用预聚合，必须同时补 `device` 列并同步补齐所有读取路径，
//!    否则会重现「按设备清理 / 多设备合并统计错乱」的问题（旧注释曾提及此隐患）。
//!
//! ## 修改历史
//!
//! - 2026-10-22 @v1.2.5: 停写 `daily_summaries`（理由见上）——每笔记录少一次写放大。
//! - 2026-10-23 @v1.2.6: 连接级加固（`AppDb::open`）——设 `busy_timeout=5s` 避免撞锁即丢；
//!   设 `wal_autocheckpoint=256` + `journal_size_limit=1MB` 收敛 WAL 体积；启动时回读
//!   三者实际生效值并打 INFO 日志（这三项都是**连接级**，外部工具读不到，只能自证）。
//!   ⚠️ 踩坑记录：最初只设了 `journal_size_limit=8MB`，**完全无效**——WAL 的均衡点由
//!   `wal_autocheckpoint`（默认 1000 页 ≈ 4.1MB）决定，8MB 上限永远够不到，截断逻辑
//!   一次都不会触发。真机库副本实测修正为两档对齐后：4,132,392 B → 1,048,576 B（−75%）。

mod models;
pub use models::*;

use chrono::{Duration, Local};
use rusqlite::{params, Connection, OptionalExtension, ToSql};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use crate::classifier::{classify_app, Rule};
use crate::tracker::platform::RawApp;

/// 单个待写入会话，避免把多个独立字段散成一长串位置参数。
pub struct NewSession<'a> {
    pub app_id: i64,
    pub category_id: &'a str,
    pub start_rfc3339: &'a str,
    pub end_rfc3339: &'a str,
    pub duration: i64,
    pub date: &'a str,
    pub window_title: Option<&'a str>,
    pub device: &'a str,
}

/// SQLite 封装（bundled 编译，无需系统依赖）
pub struct AppDb(pub Mutex<Connection>);

impl AppDb {
    /// 在应用数据目录打开（或创建）数据库并执行迁移
    pub fn open(app_data_dir: &Path) -> rusqlite::Result<Self> {
        fs::create_dir_all(app_data_dir).ok();
        let path = app_data_dir.join("screentime.db");
        let conn = Connection::open(path)?;
        // PRAGMA journal_mode=WAL 会返回一行结果，必须用 pragma_update（execute 会报 "Execute returned results"）
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;

        // ── v1.2.6 加固（此前完全没设，全靠 SQLite 默认值）──
        //
        // 1) busy_timeout：WAL 下仍可能有「写入者互斥」。当前进程内用 Mutex 串行化，
        //    但备份导入、日志清理、外部进程（用户自己开 DB 工具）都可能并发。
        //    不设这个值时默认 0 → 一旦撞上就立刻返回 SQLITE_BUSY("database is locked")，
        //    而我们的 insert_session 是 `let _ = ...` **静默吞错** → 会悄悄丢记录。
        //    设 5s 让 SQLite 自己重试等待，比丢数据好。
        //    官方说明：https://www.sqlite.org/c3ref/busy_timeout.html
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        //
        // 2) WAL 体积：靠 wal_autocheckpoint + journal_size_limit 两个旋钮共同决定。
        //
        //    ⚠️ 先纠正一个常见误解（我最初也写错过）：**只设 journal_size_limit 没用**。
        //    实测（SQLite 3.4x，page_size=4096，4000 次提交）：
        //      ac=1000, jsl=-1      → WAL 稳定在 4,120,032 B（1005 页）
        //      ac=1000, jsl=8MB     → WAL 稳定在 4,120,032 B  ← 完全没变
        //      ac=256,  jsl=4MB     → WAL 稳定在 1,054,752 B
        //      ac=256,  jsl=1MB     → WAL 稳定在 1,048,576 B
        //      ac=128,  jsl=1MB     → WAL 稳定在   527,392 B
        //
        //    原因：WAL 的**均衡点由 `wal_autocheckpoint` 决定**——默认 1000 页，
        //    写满就 checkpoint 并从头复用，所以 4.1MB 是「均衡态」而非「涨上去没降」。
        //    而 `journal_size_limit` 是 checkpoint 完成时的**截断上限**，
        //    只有「当前 WAL > 该上限」时才起作用。默认均衡点 4.1MB < 8MB → 截断永不触发。
        //    这也解释了实测 screentime.db 只有 385KB、旁边却常驻 4.1MB `-wal`。
        //
        //    取值：ac=256 页（1MB）+ jsl=1MB。均衡点与截断上限对齐在同一档，
        //    稳态 WAL ≈ 1MB（较默认 4.1MB 降 75%）。写入开销实测每笔仅 +0.1ms
        //    （4000 次提交 1.55s → 1.93s），对「采样循环偶发落库」这个量级完全无感。
        //    参考：https://www.sqlite.org/wal.html 「Checkpointing」/「Avoiding Excessively Large WAL Files」
        conn.pragma_update(None, "wal_autocheckpoint", 256)?;
        conn.pragma_update(None, "journal_size_limit", 1024 * 1024)?;

        // 回读自证：busy_timeout / journal_size_limit / wal_autocheckpoint 都是**连接级**设置，
        // 外部工具（DB Browser、python sqlite3）新开一条连接读到的只会是它自己的默认值，
        // 看不到本连接的真实状态。所以这里在设置完成后原样读回来打日志，
        // 让「加固真的生效了」可被事后核验，而不是靠读代码相信。
        let pragma_i64 = |name: &str| -> i64 {
            conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))
                .unwrap_or(-1)
        };
        tracing::info!(
            journal_mode = %conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0)).unwrap_or_default(),
            busy_timeout_ms = pragma_i64("busy_timeout"),
            journal_size_limit = pragma_i64("journal_size_limit"),
            wal_autocheckpoint = pragma_i64("wal_autocheckpoint"),
            page_size = pragma_i64("page_size"),
            "SQLite 连接加固已生效"
        );

        Self::migrate(&conn)?;
        Ok(AppDb(Mutex::new(conn)))
    }

    fn migrate(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(include_str!("../../../sql/schema.sql"))?;
        conn.execute_batch(include_str!("../../../sql/seed_categories.sql"))?;
        conn.execute_batch(include_str!("../../../sql/seed_rules.sql"))?;
        // 向后兼容：旧库 sessions 可能没有 device 列（多设备合并特性之前），增补之
        let has_device: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='device'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if has_device == 0 {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN device TEXT NOT NULL DEFAULT 'default'",
                [],
            )?;
        }
        let has_key_updated_at: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('key_usage_daily') WHERE name='updated_at'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if has_key_updated_at == 0 {
            conn.execute(
                "ALTER TABLE key_usage_daily ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(())
    }

    /// 将迁移产生的「device='default'」遗留行回填为本机真实 device_id。
    ///
    /// 多设备合并特性上线前（旧版本）写入的 session 在被 `migrate()` 的
    /// `ALTER TABLE ... ADD COLUMN device DEFAULT 'default'` 补列时，会落到字面量
    /// `'default'`。这里在 device_id 已知后统一纠正，避免界面出现幽灵「default / 未命名」设备。
    /// 已带真实 device_id 的行不会被影响。
    pub fn backfill_device_column(&self, device_id: &str) -> rusqlite::Result<usize> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE sessions SET device = ?1 WHERE device = 'default' OR device IS NULL",
            params![device_id],
        )
    }

    /// 写入/更新应用记录，返回 app id（按 process_name + platform 去重）
    pub fn upsert_app(
        &self,
        name: &str,
        process_name: &str,
        exe_path: Option<&str>,
        category_id: &str,
        platform: &str,
    ) -> rusqlite::Result<i64> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO apps (name, process_name, exe_path, category_id, platform)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(process_name, platform) DO UPDATE SET
               name=excluded.name, category_id=excluded.category_id, exe_path=excluded.exe_path",
            params![name, process_name, exe_path, category_id, platform],
        )?;
        conn.query_row(
            "SELECT id FROM apps WHERE process_name=?1 AND platform=?2",
            params![process_name, platform],
            |r| r.get(0),
        )
    }

    /// 写入一条使用时段
    ///
    /// `device` 为来源设备 id（多设备合并用，默认本机）。
    ///
    /// ⚠️ v1.2.5（2026-10-22）：**不再同步写 `daily_summaries`**。
    /// 该表此前每次落库都要多跑一条 UPSERT，但全仓库**没有任何 SELECT 从它读**
    /// （`get_daily_summaries()` 名字相似，实际是从 `sessions` 现场聚合的）。
    /// 即：纯写入、零读取的死数据，白付一倍写放大与 B-tree 维护成本。
    /// 表结构与其 `DELETE` 保留不动（老库兼容 + 将来若真要做预聚合可直接复用）。
    /// 详见 `db::mod` 模块头「daily_summaries 已停写」条。
    pub fn insert_session(&self, session: NewSession<'_>) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO sessions (app_id, start_at, end_at, duration_seconds, date, window_title, device)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                session.app_id,
                session.start_rfc3339,
                session.end_rfc3339,
                session.duration,
                session.date,
                session.window_title,
                session.device
            ],
        )?;
        // category_id 冗余写入 apps（若用户手动改过分类，以 apps 为准；此处不动）
        let _ = session.category_id;
        Ok(())
    }

    // ===================== 键盘输入统计 =====================

    /// 在单个事务内累计一批键盘输入；空批次不获取数据库锁。
    pub fn increment_key_usage(
        &self,
        device: &str,
        entries: &[(String, i32, i64)],
    ) -> rusqlite::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now_ms = Local::now().timestamp_millis();
        let tx = conn.transaction()?;
        let mut stmt = tx.prepare(
            "INSERT INTO key_usage_daily (device, date, key_id, count, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(device, date, key_id) DO UPDATE SET
               count = count + excluded.count,
               updated_at = excluded.updated_at",
        )?;
        for (date, key_id, count) in entries {
            stmt.execute(params![device, date, key_id, count, now_ms])?;
        }
        drop(stmt);
        tx.commit()
    }

    /// 键盘输入总览；`days == 0` 查询指定日，`days > 0` 查询近 N 天。
    ///
    /// 总量、日均和最高频键均排除 CapsLock 与修饰键；范围模式的日均按活跃天数计算。
    pub fn keyboard_summary(
        &self,
        days: u32,
        date: &str,
        device: &Option<String>,
    ) -> rusqlite::Result<(i64, i64, Option<KeyCountOut>)> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let (date_clause, date_param): (&str, &dyn ToSql) = if days == 0 {
            ("k.date = ?1", &date as &dyn ToSql)
        } else {
            (
                "k.date >= date('now', '-' || (?1 - 1) || ' days')",
                &days as &dyn ToSql,
            )
        };
        let (dev_clause, dev_param): (&str, Option<&dyn ToSql>) = match device {
            Some(d) if !d.is_empty() => (" AND k.device = ?2", Some(d as &dyn ToSql)),
            _ => ("", None),
        };
        let mut p: Vec<&dyn ToSql> = vec![date_param];
        if let Some(dp) = dev_param {
            p.push(dp);
        }
        let countable_keys = "k.key_id != 57 AND k.key_id NOT BETWEEN 224 AND 231";

        let (total_count, active_days): (i64, i64) = conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(k.count), 0), COUNT(DISTINCT k.date)
                 FROM key_usage_daily k
                 WHERE {date_clause}{dev_clause} AND {countable_keys}"
            ),
            p.as_slice(),
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let top_key = conn
            .query_row(
                &format!(
                    "SELECT k.key_id, SUM(k.count)
                     FROM key_usage_daily k
                     WHERE {date_clause}{dev_clause} AND {countable_keys}
                     GROUP BY k.key_id
                     ORDER BY SUM(k.count) DESC, k.key_id ASC LIMIT 1"
                ),
                p.as_slice(),
                |r| {
                    Ok(KeyCountOut {
                        key_id: r.get(0)?,
                        count: r.get(1)?,
                    })
                },
            )
            .optional()?;
        let daily_average = if days > 0 && active_days > 0 {
            total_count / active_days
        } else {
            0
        };

        Ok((total_count, daily_average, top_key))
    }

    /// 查询包含首尾日期的键盘统计，返回有记录的每日总量、全键聚合和总量。
    ///
    /// 每日数据与总量使用相同的修饰键排除口径；热力图键聚合保留所有键。
    pub fn keyboard_period(
        &self,
        from: &str,
        to: &str,
        device: &Option<String>,
    ) -> rusqlite::Result<(Vec<KeyboardDay>, Vec<KeyCountOut>, i64)> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let (dev_clause, dev_param): (&str, Option<&dyn ToSql>) = match device {
            Some(d) if !d.is_empty() => (" AND k.device = ?3", Some(d as &dyn ToSql)),
            _ => ("", None),
        };
        let mut p: Vec<&dyn ToSql> = vec![&from, &to];
        if let Some(dp) = dev_param {
            p.push(dp);
        }

        let daily = {
            let mut stmt = conn.prepare(&format!(
                "SELECT k.date, SUM(k.count)
                 FROM key_usage_daily k
                 WHERE k.date >= ?1 AND k.date <= ?2{dev_clause}
                   AND k.key_id != 57 AND k.key_id NOT BETWEEN 224 AND 231
                 GROUP BY k.date ORDER BY k.date ASC"
            ))?;
            let rows = stmt.query_map(p.as_slice(), |r| {
                Ok(KeyboardDay {
                    date: r.get(0)?,
                    count: r.get(1)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let keys = {
            let mut stmt = conn.prepare(&format!(
                "SELECT k.key_id, SUM(k.count)
                 FROM key_usage_daily k
                 WHERE k.date >= ?1 AND k.date <= ?2{dev_clause}
                 GROUP BY k.key_id
                 ORDER BY SUM(k.count) DESC, k.key_id ASC"
            ))?;
            let rows = stmt.query_map(p.as_slice(), |r| {
                Ok(KeyCountOut {
                    key_id: r.get(0)?,
                    count: r.get(1)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let total_count = daily.iter().map(|day| day.count).sum();

        Ok((daily, keys, total_count))
    }

    /// 近 N 天每日总览（按 device 过滤；空 device = 全部设备合并）
    pub fn get_daily_summaries(
        &self,
        days: u32,
        device: &Option<String>,
    ) -> rusqlite::Result<Vec<DailySummaryOut>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut p: Vec<&dyn ToSql> = vec![&days];
        let dev = if let Some(d) = device {
            if !d.is_empty() {
                p.push(d);
                " AND s.device = ?2"
            } else {
                ""
            }
        } else {
            ""
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT s.date, SUM(s.duration_seconds), COUNT(DISTINCT s.app_id)
             FROM sessions s WHERE 1=1{dev}
             GROUP BY s.date ORDER BY s.date DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map(p.as_slice(), |r| {
            Ok(DailySummaryOut {
                date: r.get(0)?,
                total_seconds: r.get(1)?,
                app_count: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// 按天 × 分类的时长明细（iOS 风格堆叠柱状图：每天一根柱，按分类着色堆叠）
    pub fn get_daily_categories(
        &self,
        days: u32,
        device: &Option<String>,
    ) -> rusqlite::Result<Vec<DayCategoryOut>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut p: Vec<&dyn ToSql> = vec![&days];
        let dev = if let Some(d) = device {
            if !d.is_empty() {
                p.push(d);
                " AND s.device = ?2"
            } else {
                ""
            }
        } else {
            ""
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT s.date, a.category_id, SUM(s.duration_seconds)
             FROM sessions s JOIN apps a ON s.app_id = a.id
             WHERE s.date >= date('now', '-' || ?1 || ' days'){dev}
             GROUP BY s.date, a.category_id
             ORDER BY s.date DESC"
        ))?;
        let rows = stmt.query_map(p.as_slice(), |r| {
            Ok(DayCategoryOut {
                date: r.get(0)?,
                category_id: r.get(1)?,
                total_seconds: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// 指定日期的 24 小时 × 分类堆叠桶（按 device 过滤）
    pub fn get_hourly_buckets(
        &self,
        date: &str,
        device: &Option<String>,
    ) -> rusqlite::Result<Vec<HourlyBucketOut>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut p: Vec<&dyn ToSql> = vec![&date];
        let dev = if let Some(d) = device {
            if !d.is_empty() {
                p.push(d);
                " AND s.device = ?2"
            } else {
                ""
            }
        } else {
            ""
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT CAST(strftime('%H', s.start_at) AS INTEGER) AS hr, a.category_id, SUM(s.duration_seconds)
             FROM sessions s JOIN apps a ON s.app_id = a.id
             WHERE s.date = ?1{dev}
             GROUP BY hr, a.category_id
             ORDER BY hr ASC"
        ))?;
        let rows = stmt.query_map(p.as_slice(), |r| {
            Ok(HourlyBucketOut {
                hour: r.get(0)?,
                category_id: r.get(1)?,
                total_seconds: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// App 使用时长排行
    ///
    /// - `days == 0`：单日排行（按 `date` 精确匹配，用于「今日 / 点选某天详情」）
    /// - `days > 0`：范围聚合排行（`date >= date('now','-N days')`，用于「近7/14/30天」）
    ///   两种模式均按 device 过滤（空 device = 全部设备合并）。
    pub fn get_app_ranking(
        &self,
        days: u32,
        date: &str,
        device: &Option<String>,
    ) -> rusqlite::Result<Vec<AppRankingOut>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // 日期过滤分支：单日用精确匹配，范围用日期窗口
        let (date_clause, date_param): (&str, &dyn ToSql) = if days == 0 {
            ("s.date = ?1", &date as &dyn ToSql)
        } else {
            (
                "s.date >= date('now', '-' || ?1 || ' days')",
                &days as &dyn ToSql,
            )
        };
        let (dev_clause, dev_param): (&str, Option<&dyn ToSql>) = match device {
            Some(d) if !d.is_empty() => (" AND s.device = ?2", Some(d as &dyn ToSql)),
            _ => ("", None),
        };
        let mut p: Vec<&dyn ToSql> = vec![date_param];
        if let Some(dp) = dev_param {
            p.push(dp);
        }
        let mut stmt = conn.prepare(&format!(
            "SELECT a.id, a.name, a.category_id, SUM(s.duration_seconds), COUNT(*), a.icon_blob
             FROM sessions s JOIN apps a ON s.app_id = a.id
             WHERE {date_clause}{dev_clause}
             GROUP BY a.id
             ORDER BY SUM(s.duration_seconds) DESC"
        ))?;
        let rows = stmt.query_map(p.as_slice(), |r| {
            let icon: Option<Vec<u8>> = r.get(5)?;
            Ok(AppRankingOut {
                app_id: r.get(0)?,
                app_name: r.get(1)?,
                category_id: r.get(2)?,
                total_seconds: r.get(3)?,
                session_count: r.get(4)?,
                icon_base64: icon.as_deref().map(to_base64),
            })
        })?;
        rows.collect()
    }

    /// 总览卡片数据
    ///
    /// - `days == 0`：单日总览（按 `date` 精确匹配，用于「今日 / 点选某天详情」）
    /// - `days > 0`：范围聚合总览（`date >= date('now','-N days')`，用于「近7/14/30天」）
    ///   两种模式均按 device 过滤（空 device = 全部设备合并）。
    ///   范围模式额外计算 `avg_daily_seconds`（按有数据的实际天数均摊）。
    pub fn get_overview(
        &self,
        days: u32,
        date: &str,
        device: &Option<String>,
    ) -> rusqlite::Result<OverviewOut> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // 日期过滤分支：单日用精确匹配，范围用日期窗口
        let (date_clause, date_param): (&str, &dyn ToSql) = if days == 0 {
            ("s.date = ?1", &date as &dyn ToSql)
        } else {
            (
                "s.date >= date('now', '-' || ?1 || ' days')",
                &days as &dyn ToSql,
            )
        };
        let (dev_clause, dev_param): (&str, Option<&dyn ToSql>) = match device {
            Some(d) if !d.is_empty() => (" AND s.device = ?2", Some(d as &dyn ToSql)),
            _ => ("", None),
        };
        let mut p: Vec<&dyn ToSql> = vec![date_param];
        if let Some(dp) = dev_param {
            p.push(dp);
        }

        let (total, count): (i64, i64) = conn
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(s.duration_seconds),0), COUNT(DISTINCT s.app_id)
                     FROM sessions s WHERE {date_clause}{dev_clause}"
                ),
                p.as_slice(),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));

        let top: Option<(String, i64)> = conn
            .query_row(
                &format!(
                    "SELECT a.name, SUM(s.duration_seconds)
                     FROM sessions s JOIN apps a ON s.app_id=a.id
                     WHERE {date_clause}{dev_clause} GROUP BY a.id ORDER BY SUM(s.duration_seconds) DESC LIMIT 1"
                ),
                p.as_slice(),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        let pickups: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM sessions s WHERE {date_clause}{dev_clause}"),
                p.as_slice(),
                |r| r.get(0),
            )
            .unwrap_or(0);

        // 日均：仅范围模式 days>0 时计算，按有数据的实际天数均摊（避免空档日拉低均值）
        let avg_daily_seconds: i64 = if days > 0 {
            let active_days: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(DISTINCT s.date) FROM sessions s WHERE {date_clause}{dev_clause}"
                    ),
                    p.as_slice(),
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if active_days > 0 {
                total / active_days
            } else {
                0
            }
        } else {
            0
        };

        Ok(OverviewOut {
            date: if days == 0 {
                date.to_string()
            } else {
                format!("近{}天", days)
            },
            total_seconds: total,
            app_count: count,
            most_used_app: top.as_ref().map(|(n, _)| n.clone()),
            most_used_seconds: top.map(|(_, s)| s).unwrap_or(0),
            pickup_count: pickups,
            avg_daily_seconds,
        })
    }

    /// 某月统计概括（日历月视图）
    ///
    /// 按 `date LIKE 'YYYY-MM-%'` 过滤该月全部 session，返回月总时长、活跃天数、
    /// 月天数、日均（按活跃天数均摊）、月内最常用 App、月内最忙的一天。
    pub fn get_month_summary(
        &self,
        year: i32,
        month: i32,
        device: &Option<String>,
    ) -> rusqlite::Result<MonthSummaryOut> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let prefix = format!("{:04}-{:02}-", year, month);
        let (dev_clause, dev_param): (&str, Option<&dyn ToSql>) = match device {
            Some(d) if !d.is_empty() => (" AND s.device = ?2", Some(d as &dyn ToSql)),
            _ => ("", None),
        };
        let mut p: Vec<&dyn ToSql> = vec![&prefix as &dyn ToSql];
        if let Some(dp) = dev_param {
            p.push(dp);
        }

        let (total, active_days): (i64, i64) = conn
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(s.duration_seconds),0), COUNT(DISTINCT s.date)
                     FROM sessions s WHERE s.date LIKE ?1 || '%'{dev_clause}"
                ),
                p.as_slice(),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));

        let top: Option<(String, i64)> = conn
            .query_row(
                &format!(
                    "SELECT a.name, SUM(s.duration_seconds)
                     FROM sessions s JOIN apps a ON s.app_id=a.id
                     WHERE s.date LIKE ?1 || '%'{dev_clause}
                     GROUP BY a.id ORDER BY SUM(s.duration_seconds) DESC LIMIT 1"
                ),
                p.as_slice(),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        let busiest: Option<(String, i64)> = conn
            .query_row(
                &format!(
                    "SELECT s.date, SUM(s.duration_seconds)
                     FROM sessions s WHERE s.date LIKE ?1 || '%'{dev_clause}
                     GROUP BY s.date ORDER BY SUM(s.duration_seconds) DESC LIMIT 1"
                ),
                p.as_slice(),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        // 该月天数（闰年安全）
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let month_days = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let days_in_month = month_days[(month - 1) as usize] as i64;

        let avg_daily_seconds = if active_days > 0 {
            total / active_days
        } else {
            0
        };

        Ok(MonthSummaryOut {
            year,
            month,
            total_seconds: total,
            active_days,
            days_in_month,
            avg_daily_seconds,
            top_app: top.as_ref().map(|(n, _)| n.clone()),
            top_app_seconds: top.map(|(_, s)| s).unwrap_or(0),
            busiest_date: busiest.as_ref().map(|(d, _)| d.clone()),
            busiest_seconds: busiest.map(|(_, s)| s).unwrap_or(0),
        })
    }

    pub fn get_categories(&self) -> rusqlite::Result<Vec<CategoryOut>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT id, name, color FROM categories ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(CategoryOut {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// 加载全部分类规则（含禁用项），供采样循环在内存缓存后匹配
    pub fn load_rules(&self) -> rusqlite::Result<Vec<Rule>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, field, match_type, pattern, category_id, priority, enabled
             FROM classification_rules ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Rule {
                id: r.get(0)?,
                field: r.get(1)?,
                match_type: r.get(2)?,
                pattern: r.get(3)?,
                category_id: r.get(4)?,
                priority: r.get(5)?,
                enabled: r.get::<_, i64>(6)? != 0,
            })
        })?;
        rows.collect()
    }

    /// 以可序列化结构返回全部分类规则（供前端管理界面展示）
    pub fn get_rules_out(&self) -> rusqlite::Result<Vec<RuleOut>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, field, match_type, pattern, category_id, priority, enabled
             FROM classification_rules ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(RuleOut {
                id: r.get(0)?,
                field: r.get(1)?,
                match_type: r.get(2)?,
                pattern: r.get(3)?,
                category_id: r.get(4)?,
                priority: r.get(5)?,
                enabled: r.get::<_, i64>(6)? != 0,
            })
        })?;
        rows.collect()
    }

    /// 按关键字筛选并稳定分页读取分类规则，不改动采样使用的规则热路径。
    ///
    /// `page_size` 收敛到 1..=100，`page` 从 1 开始并收敛到过滤后的末页；
    /// 无结果时返回第 1 页。排序字段和方向只通过白名单映射为 SQL 片段。
    pub fn get_rules_page(
        &self,
        keyword: &str,
        page: i64,
        page_size: i64,
        sort_key: &str,
        sort_dir: &str,
    ) -> rusqlite::Result<RulePageOut> {
        let page_size = page_size.clamp(1, 100);
        let keyword = keyword.trim();
        let like_pattern = format!("%{}%", escape_like_literal(keyword));
        let sort_key = match sort_key {
            "field" | "match_type" | "pattern" | "category_id" | "priority" | "enabled" => sort_key,
            _ => "category_id",
        };
        let sort_column = match sort_key {
            "field" => "r.field",
            "match_type" => "r.match_type",
            "pattern" => "r.pattern",
            "priority" => "r.priority",
            "enabled" => "r.enabled",
            _ => "r.category_id",
        };
        let direction = if sort_dir == "desc" { "DESC" } else { "ASC" };
        let order_by = if sort_key == "pattern" {
            format!("{sort_column} {direction}, r.id ASC")
        } else {
            format!("{sort_column} {direction}, r.pattern ASC, r.id ASC")
        };

        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let total: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM classification_rules r
             LEFT JOIN categories c ON c.id = r.category_id
             WHERE (?1 = '' OR LOWER(r.pattern) LIKE LOWER(?2) ESCAPE '\\'
                    OR LOWER(r.category_id) LIKE LOWER(?2) ESCAPE '\\'
                    OR LOWER(COALESCE(c.name, '')) LIKE LOWER(?2) ESCAPE '\\')",
            params![keyword, like_pattern],
            |r| r.get(0),
        )?;
        let last_page = if total == 0 {
            1
        } else {
            (total - 1) / page_size + 1
        };
        let page = page.clamp(1, last_page);
        let offset = (page - 1) * page_size;
        let mut stmt = conn.prepare(&format!(
            "SELECT r.id, r.field, r.match_type, r.pattern, r.category_id, r.priority, r.enabled
             FROM classification_rules r
             LEFT JOIN categories c ON c.id = r.category_id
             WHERE (?1 = '' OR LOWER(r.pattern) LIKE LOWER(?2) ESCAPE '\\'
                    OR LOWER(r.category_id) LIKE LOWER(?2) ESCAPE '\\'
                    OR LOWER(COALESCE(c.name, '')) LIKE LOWER(?2) ESCAPE '\\')
             ORDER BY {order_by}
             LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = stmt.query_map(params![keyword, like_pattern, page_size, offset], |r| {
            Ok(RuleOut {
                id: r.get(0)?,
                field: r.get(1)?,
                match_type: r.get(2)?,
                pattern: r.get(3)?,
                category_id: r.get(4)?,
                priority: r.get(5)?,
                enabled: r.get::<_, i64>(6)? != 0,
            })
        })?;
        let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(RulePageOut {
            items,
            total,
            page,
            page_size,
        })
    }

    /// 新增规则；若 (field, match_type, pattern) 已存在则更新其分类/优先级并重新启用。
    /// `enabled` 由用户显式传入（前端新增表单有该复选框），缺省 true 仅作兜底。
    pub fn insert_rule(
        &self,
        field: &str,
        match_type: &str,
        pattern: &str,
        category_id: &str,
        priority: i32,
        enabled: bool,
    ) -> rusqlite::Result<i64> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO classification_rules (field, match_type, pattern, category_id, priority, enabled)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(field, match_type, pattern) DO UPDATE SET
               category_id=excluded.category_id, priority=excluded.priority, enabled=excluded.enabled",
            params![field, match_type, pattern, category_id, priority, if enabled { 1i64 } else { 0i64 }],
        )?;
        conn.query_row(
            "SELECT id FROM classification_rules WHERE field=?1 AND match_type=?2 AND pattern=?3",
            params![field, match_type, pattern],
            |r| r.get(0),
        )
    }

    /// 自动建规则专用：**仅当不存在** `(field, match_type, pattern)` 时才插入。
    ///
    /// 与 `insert_rule` 的区别（后者是 UPSERT）：
    /// - 已存在则**完全不动**——不覆盖用户改过的分类，也**不重新启用**用户手动禁用的规则。
    ///   （旧实现走 `insert_rule` 的 `ON CONFLICT ... DO UPDATE SET enabled=1`，会在
    ///   进程下次成为前台时把用户禁用过的自动规则又打开，属状态被反向覆盖。）
    /// - 天然幂等：即便多路并发调用也不会产生重复行、不会互相打架。
    ///
    /// 返回 `true` 表示本次真的插入了新行。
    pub fn ensure_rule(
        &self,
        field: &str,
        match_type: &str,
        pattern: &str,
        category_id: &str,
        priority: i32,
    ) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let n = conn.execute(
            "INSERT OR IGNORE INTO classification_rules (field, match_type, pattern, category_id, priority, enabled)
             VALUES (?1,?2,?3,?4,?5,1)",
            params![field, match_type, pattern, category_id, priority],
        )?;
        Ok(n > 0)
    }

    /// 更新规则（按 id）
    pub fn update_rule(&self, rule: &RuleOut) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE classification_rules
             SET field=?2, match_type=?3, pattern=?4, category_id=?5, priority=?6, enabled=?7
             WHERE id=?1",
            params![
                rule.id,
                rule.field,
                rule.match_type,
                rule.pattern,
                rule.category_id,
                rule.priority,
                if rule.enabled { 1i64 } else { 0i64 }
            ],
        )?;
        Ok(())
    }

    /// 删除规则（按 id）
    pub fn delete_rule(&self, id: i64) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM classification_rules WHERE id=?1", params![id])?;
        Ok(())
    }

    /// 按当前规则重算所有已记录应用的分类（窗口标题/包名未存储，仅用进程名/路径/展示名）
    ///
    /// 返回被更新的应用数量。历史 session 通过 JOIN apps 自动跟随新分类。
    pub fn reclassify_all(&self, rules: &[Rule]) -> rusqlite::Result<usize> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt =
            conn.prepare("SELECT id, name, process_name, exe_path, category_id FROM apps")?;
        let apps = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut updated = 0;
        for (id, name, process_name, exe_path, old_cat) in apps {
            let app = RawApp {
                name: name.clone(),
                process_name: process_name.clone(),
                exe_path: exe_path.clone(),
                bundle_id: None,
                window_title: None,
            };
            let cat = classify_app(&app, rules);
            if cat != old_cat {
                conn.execute(
                    "UPDATE apps SET category_id=?1 WHERE id=?2",
                    params![cat, id],
                )?;
                updated += 1;
            }
        }
        Ok(updated)
    }

    // ===================== 周/月对比（同比分析）=====================

    /// 计算某时间区间（start_at 闭区间，end_at 开区间）的聚合统计
    ///
    /// `device` 为空表示合并全部设备；否则只统计该设备。
    pub fn period_summary(
        &self,
        start: &str,
        end: &str,
        device: &Option<String>,
        label: &str,
    ) -> rusqlite::Result<PeriodStat> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut p: Vec<&dyn ToSql> = vec![&start, &end];
        let dev = if let Some(d) = device {
            if !d.is_empty() {
                p.push(d);
                " AND s.device = ?3"
            } else {
                ""
            }
        } else {
            ""
        };

        let total: i64 = conn
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(s.duration_seconds),0) FROM sessions s
                     WHERE s.start_at >= ?1 AND s.start_at < ?2{dev}"
                ),
                p.as_slice(),
                |r| r.get(0),
            )
            .unwrap_or(0);

        let app_count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(DISTINCT s.app_id) FROM sessions s
                     WHERE s.start_at >= ?1 AND s.start_at < ?2{dev}"
                ),
                p.as_slice(),
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mut stmt = conn.prepare(&format!(
            "SELECT a.category_id, SUM(s.duration_seconds)
             FROM sessions s JOIN apps a ON s.app_id=a.id
             WHERE s.start_at >= ?1 AND s.start_at < ?2{dev}
             GROUP BY a.category_id ORDER BY SUM(s.duration_seconds) DESC"
        ))?;
        let by_category = stmt
            .query_map(p.as_slice(), |r| {
                Ok(CategorySeconds {
                    category_id: r.get(0)?,
                    total_seconds: r.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;

        let mut stmt2 = conn.prepare(&format!(
            "SELECT a.name, a.category_id, SUM(s.duration_seconds)
             FROM sessions s JOIN apps a ON s.app_id=a.id
             WHERE s.start_at >= ?1 AND s.start_at < ?2{dev}
             GROUP BY a.id ORDER BY SUM(s.duration_seconds) DESC LIMIT 10"
        ))?;
        let top_apps = stmt2
            .query_map(p.as_slice(), |r| {
                Ok(AppSeconds {
                    app_name: r.get(0)?,
                    category_id: r.get(1)?,
                    total_seconds: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;

        Ok(PeriodStat {
            label: label.to_string(),
            total_seconds: total,
            app_count,
            by_category,
            top_apps,
            daily: Vec::new(),
        })
    }

    /// 按 sessions.date 汇总区间内每日时长；缺日期由命令层按自然日补齐。
    ///
    /// 与 `period_summary` 使用相同的 start_at 闭/开边界和 device 过滤口径。
    pub fn period_daily(
        &self,
        start: &str,
        end: &str,
        device: &Option<String>,
    ) -> rusqlite::Result<Vec<PeriodDay>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut p: Vec<&dyn ToSql> = vec![&start, &end];
        let dev = if let Some(d) = device {
            if !d.is_empty() {
                p.push(d);
                " AND s.device = ?3"
            } else {
                ""
            }
        } else {
            ""
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT s.date, SUM(s.duration_seconds)
             FROM sessions s
             WHERE s.start_at >= ?1 AND s.start_at < ?2{dev}
             GROUP BY s.date ORDER BY s.date ASC"
        ))?;
        let rows = stmt.query_map(p.as_slice(), |r| {
            Ok(PeriodDay {
                date: r.get(0)?,
                total_seconds: r.get(1)?,
            })
        })?;
        rows.collect()
    }

    // ===================== 多设备合并 =====================

    /// 列出所有出现过的设备（含本机，即使暂无数据也列出），名称优先取 settings
    pub fn get_devices(
        &self,
        current_id: &str,
        current_name: &str,
    ) -> rusqlite::Result<Vec<DeviceInfo>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT DISTINCT device FROM sessions")?;
        let mut ids: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if !ids.iter().any(|i| i == current_id) {
            ids.push(current_id.to_string());
        }
        let mut out = Vec::new();
        for id in ids {
            let name = if id == current_id {
                current_name.to_string()
            } else {
                conn.query_row(
                    "SELECT value FROM settings WHERE key=?1",
                    params![format!("device_name:{}", id)],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .unwrap_or_else(|| id.clone())
            };
            out.push(DeviceInfo { id, name });
        }
        Ok(out)
    }

    /// 导出全量数据（应用 + 时段 + 设备名映射），用于跨设备合并
    pub fn export_all(&self) -> rusqlite::Result<ExportBundle> {
        self.export_all_filtered(None)
    }

    /// 导出全量数据（应用 + 时段 + 设备名映射）
    /// - `device_filter`：None 或空字符串 → 导出全部设备；Some(id) → 仅导出该设备的 sessions
    /// - apps 表所有应用都导出（不分设备）；device 名映射始终全量
    pub fn export_all_filtered(
        &self,
        device_filter: Option<&str>,
    ) -> rusqlite::Result<ExportBundle> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt =
            conn.prepare("SELECT name, process_name, exe_path, category_id, platform FROM apps")?;
        let apps = stmt
            .query_map([], |r| {
                Ok(ExportApp {
                    name: r.get(0)?,
                    process_name: r.get(1)?,
                    exe_path: r.get(2)?,
                    category_id: r.get(3)?,
                    platform: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;

        // sessions：按设备过滤（Some(id) 加 WHERE；None/空：全量）
        let (session_sql, had_filter) = match device_filter {
            Some(id) if !id.is_empty() => (
                "SELECT a.process_name, a.platform, s.start_at, s.end_at, s.duration_seconds, s.date, s.window_title, s.device
                 FROM sessions s JOIN apps a ON s.app_id=a.id
                 WHERE s.device = ?1",
                true,
            ),
            _ => (
                "SELECT a.process_name, a.platform, s.start_at, s.end_at, s.duration_seconds, s.date, s.window_title, s.device
                 FROM sessions s JOIN apps a ON s.app_id=a.id",
                false,
            ),
        };
        let mut stmt2 = conn.prepare(session_sql)?;
        let sessions = if had_filter {
            stmt2
                .query_map([device_filter.unwrap()], |r| {
                    Ok(ExportSession {
                        app_process: r.get(0)?,
                        app_platform: r.get(1)?,
                        start_at: r.get(2)?,
                        end_at: r.get(3)?,
                        duration_seconds: r.get(4)?,
                        date: r.get(5)?,
                        window_title: r.get(6)?,
                        device: r.get(7)?,
                    })
                })?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?
        } else {
            stmt2
                .query_map([], |r| {
                    Ok(ExportSession {
                        app_process: r.get(0)?,
                        app_platform: r.get(1)?,
                        start_at: r.get(2)?,
                        end_at: r.get(3)?,
                        duration_seconds: r.get(4)?,
                        date: r.get(5)?,
                        window_title: r.get(6)?,
                        device: r.get(7)?,
                    })
                })?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?
        };

        // key_usage_daily 与 sessions 使用相同的设备过滤语义。
        let key_device_id = device_filter.unwrap_or_default();
        let mut key_params: Vec<&dyn ToSql> = Vec::new();
        let key_device_clause = if key_device_id.is_empty() {
            ""
        } else {
            key_params.push(&key_device_id);
            " WHERE device = ?1"
        };
        let mut key_stmt = conn.prepare(&format!(
            "SELECT device, date, key_id, count, updated_at FROM key_usage_daily{key_device_clause}
             ORDER BY device, date, key_id"
        ))?;
        let key_usage = key_stmt
            .query_map(key_params.as_slice(), |r| {
                Ok(ExportKeyUsage {
                    device: r.get(0)?,
                    date: r.get(1)?,
                    key_id: r.get(2)?,
                    count: r.get(3)?,
                    updated_at: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;

        // 收集设备名映射（settings 中 device_name:<id>）
        let mut dev_stmt =
            conn.prepare("SELECT key, value FROM settings WHERE key LIKE 'device_name:%'")?;
        let mut devices: HashMap<String, String> = HashMap::new();
        let rows =
            dev_stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (k, v) = row?;
            if let Some(id) = k.strip_prefix("device_name:") {
                devices.insert(id.to_string(), v);
            }
        }

        drop(dev_stmt);
        drop(key_stmt);
        drop(stmt2);
        drop(stmt);
        // 当前连接已持锁，不能调用会再次获取 self.0 的 get_setting_result。
        let pet_state = conn
            .query_row(
                "SELECT value FROM settings WHERE key='pet_state'",
                [],
                |r| r.get(0),
            )
            .optional()?;

        Ok(ExportBundle {
            version: 2,
            exported_at: Local::now().to_rfc3339(),
            devices,
            pet_state,
            apps,
            sessions,
            key_usage,
        })
    }

    /// 导入全量数据并合并（按 start_at+app_id+device 去重，幂等）
    ///
    /// 返回新导入的时段条数。
    pub fn import_data(&self, bundle: &ExportBundle) -> rusqlite::Result<usize> {
        if bundle.version > 2 {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("不支持的备份版本: {}（当前最高支持 2）", bundle.version),
                ),
            )));
        }
        // 备份里的桌宠状态必须是 JSON object。校验必须先于任何 SQL 写入：
        // 否则非法 pet_state 会在导入后污染设置，甚至已经写入部分设备/应用数据。
        if let Some(json) = &bundle.pet_state {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json)
                .map(|_| ())
                .map_err(|error| {
                    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("bundle.pet_state must be a JSON object: {error}"),
                    )))
                })?;
        }

        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // 写入设备名映射
        for (id, name) in &bundle.devices {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1,?2) ON CONFLICT(key) DO NOTHING",
                params![format!("device_name:{}", id), name],
            )?;
        }
        if let Some(json) = &bundle.pet_state {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('pet_state', ?1)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![json],
            )?;
        }
        // 先 upsert 应用（按 process_name+platform 去重，与设备无关）
        for a in &bundle.apps {
            conn.execute(
                "INSERT INTO apps (name, process_name, exe_path, category_id, platform)
                 VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(process_name, platform) DO UPDATE SET
                   name=excluded.name, category_id=excluded.category_id, exe_path=excluded.exe_path",
                params![a.name, a.process_name, a.exe_path, a.category_id, a.platform],
            )?;
        }
        // 再插入时段，跳过已存在的
        let mut imported = 0;
        for s in &bundle.sessions {
            let app_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM apps WHERE process_name=?1 AND platform=?2",
                    params![s.app_process, s.app_platform],
                    |r| r.get(0),
                )
                .optional()?;
            let app_id = match app_id {
                Some(id) => id,
                None => continue, // 找不到对应应用则跳过该时段
            };
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sessions WHERE start_at=?1 AND app_id=?2 AND device=?3",
                    params![s.start_at, app_id, s.device],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if exists > 0 {
                continue; // 已存在则跳过，保证可重复导入
            }
            conn.execute(
                "INSERT INTO sessions (app_id, start_at, end_at, duration_seconds, date, window_title, device)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    app_id,
                    s.start_at,
                    s.end_at,
                    s.duration_seconds,
                    s.date,
                    s.window_title,
                    s.device
                ],
            )?;
            // v1.2.5：原此处同步写 daily_summaries，已移除（该表只写不读，
            // 原因详见 insert_session 的文档注释）。导入只落 sessions 即可。
            imported += 1;
        }
        // 同一设备/日期/键位是快照口径：较新的 updated_at 获胜；重复导入不翻倍，
        // 删除后新增的行也不会被旧备份覆盖。
        for usage in &bundle.key_usage {
            conn.execute(
                "INSERT INTO key_usage_daily (device, date, key_id, count, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(device, date, key_id) DO UPDATE SET
                   count = CASE
                     WHEN excluded.updated_at > key_usage_daily.updated_at THEN excluded.count
                     ELSE key_usage_daily.count
                   END,
                   updated_at = MAX(key_usage_daily.updated_at, excluded.updated_at)",
                params![
                    usage.device,
                    usage.date,
                    usage.key_id,
                    usage.count,
                    usage.updated_at
                ],
            )?;
        }
        Ok(imported)
    }

    /// 清理超过 retention_days 天的旧时段（按日期）
    /// - `device_filter`：可选设备 ID 过滤。Some(id) → 仅清该设备的旧数据；None → 清全部设备的旧数据
    ///   用于多设备合并场景下「只想清某一台历史数据」
    pub fn prune_old(
        &self,
        retention_days: u32,
        device_filter: Option<&str>,
    ) -> rusqlite::Result<usize> {
        let mut conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let cutoff = (Local::now() - Duration::days(retention_days as i64))
            .format("%Y-%m-%d")
            .to_string();
        let tx = conn.transaction()?;
        let n: usize = match device_filter {
            Some(dev) if !dev.is_empty() => {
                // 仅清指定设备的旧数据（用于「按设备清理」入口）
                tx.execute(
                    "DELETE FROM sessions WHERE date < ?1 AND device = ?2",
                    params![cutoff, dev],
                )?
            }
            _ => tx.execute("DELETE FROM sessions WHERE date < ?1", params![cutoff])?,
        };
        match device_filter {
            Some(dev) if !dev.is_empty() => {
                tx.execute(
                    "DELETE FROM key_usage_daily WHERE date < ?1 AND device = ?2",
                    params![cutoff, dev],
                )?;
            }
            _ => {
                tx.execute(
                    "DELETE FROM key_usage_daily WHERE date < ?1",
                    params![cutoff],
                )?;
            }
        }
        tx.execute(
            "DELETE FROM daily_summaries WHERE date < ?1",
            params![cutoff],
        )?;
        tx.commit()?;
        Ok(n)
    }

    // ===================== 简易键值配置 =====================

    /// 删除指定设备的全部 sessions 和键盘计数（不限时间）。
    /// 用于「按设备清理」组合命令（先备份再全删）。
    pub fn delete_all_sessions_for_device(&self, device_id: &str) -> rusqlite::Result<usize> {
        let mut conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;
        let n: usize = tx.execute("DELETE FROM sessions WHERE device = ?1", params![device_id])?;
        tx.execute(
            "DELETE FROM key_usage_daily WHERE device = ?1",
            params![device_id],
        )?;
        tx.commit()?;
        Ok(n)
    }

    /// 清空所有设备的键盘输入统计，不影响应用使用时段。
    pub fn clear_keyboard_stats(&self) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM key_usage_daily", [])?;
        Ok(())
    }

    /// 读取配置项（不存在返回 None）
    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.get_setting_result(key).ok().flatten()
    }

    /// 读取配置项并保留数据库错误，迁移流程不得把错误当成“尚未迁移”。
    pub fn get_setting_result(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.query_row(
            "SELECT value FROM settings WHERE key=?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
    }

    /// 只在配置不存在时写入，避免多窗口用旧快照覆盖新状态。
    pub fn set_setting_if_absent(&self, key: &str, value: &str) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let changed = conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1,?2) ON CONFLICT(key) DO NOTHING",
            params![key, value],
        )?;
        Ok(changed > 0)
    }

    /// 写入/覆盖配置项
    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ===== v0.8.0（2026-09-16）：截图索引 CRUD =====
    // 设计：DB 只存元信息，PNG 本体落在 <app_data_dir>/screenshots/。
    // 删除走「先查文件名 → 调用方移入回收站 → 再删行」，避免文件删了行还在（或反之）。

    /// 新增一条截图索引，返回自增 id
    pub fn insert_screenshot(
        &self,
        file_name: &str,
        width: u32,
        height: u32,
        bytes: i64,
        created_at: &str,
        device: &str,
    ) -> rusqlite::Result<i64> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO screenshots (file_name, width, height, bytes, created_at, device)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![file_name, width, height, bytes, created_at, device],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 截图历史（按时间倒序，限量 + 偏移）
    ///
    /// v1.1.1：增加 `offset` 支持「加载更多」分页，与 `max_count`（FIFO 上限）解耦——
    /// 用户设了较大 `max_count` 时也能翻页看到全部历史，不再硬编码只取最近 60 张。
    pub fn list_screenshots(
        &self,
        limit: u32,
        offset: u32,
    ) -> rusqlite::Result<Vec<ScreenshotOut>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, file_name, width, height, bytes, created_at
             FROM screenshots ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |r| {
            Ok(ScreenshotOut {
                id: r.get(0)?,
                file_name: r.get(1)?,
                width: r.get(2)?,
                height: r.get(3)?,
                bytes: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// 取单条截图的文件名（缩略图 / 定位文件用）
    pub fn screenshot_file_name(&self, id: i64) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.query_row(
            "SELECT file_name FROM screenshots WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
    }

    /// 删除一条截图索引，返回其文件名（调用方负责把文件移入回收站）
    pub fn delete_screenshot(&self, id: i64) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let name: Option<String> = conn
            .query_row(
                "SELECT file_name FROM screenshots WHERE id=?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        if name.is_some() {
            conn.execute("DELETE FROM screenshots WHERE id=?1", params![id])?;
        }
        Ok(name)
    }

    /// FIFO 清理：保留最新 keep 条，返回超出需删除的 (id, file_name) 列表
    pub fn screenshots_exceeding(&self, keep: u32) -> rusqlite::Result<Vec<(i64, String)>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM screenshots
             WHERE id NOT IN (SELECT id FROM screenshots ORDER BY id DESC LIMIT ?1)",
        )?;
        let rows = stmt.query_map(params![keep], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// 删除指定 id（FIFO 清理用，不返回文件名）
    pub fn delete_screenshot_by_id(&self, id: i64) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM screenshots WHERE id=?1", params![id])?;
        Ok(())
    }
}

/// 转义 SQLite LIKE 的字面量关键字；反斜杠是查询中显式声明的 ESCAPE 字符。
fn escape_like_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// 标准 base64 编码（内联实现，避免额外依赖）
fn to_base64(data: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(CHARS[(n & 63) as usize] as char);
        }
    }
    let pad = (3 - data.len() % 3) % 3;
    for _ in 0..pad {
        out.push('=');
    }
    out
}

// ===================== 单设备聚合（按设备清理弹窗用） =====================

/// 单设备聚合统计（用于「按设备清理」弹窗）
#[derive(serde::Serialize, Clone)]
pub struct DeviceStats {
    /// 设备 ID（12 位 hex）
    pub device_id: String,
    /// 设备名（来自 settings，若无则用 id 兜底）
    pub device_name: String,
    /// 该设备的总累计秒数
    pub total_seconds: i64,
    /// 该设备的 session 数
    pub session_count: i64,
    /// 该设备最早一条 session 的日期（YYYY-MM-DD）
    pub earliest_date: String,
    /// 该设备最新一条 session 的日期（YYYY-MM-DD）
    pub latest_date: String,
}

impl AppDb {
    /// 列出所有出现过数据的设备聚合统计（含本机，本机即使无数据也列出）
    /// - 用 `sessions` 表的 `device` 列做分组
    /// - 设备名按 `settings` 表的 `device_name:<id>` key 读取，无则用 id 兜底
    pub fn list_devices_with_stats(
        &self,
        current_id: &str,
        current_name: &str,
    ) -> rusqlite::Result<Vec<DeviceStats>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // 1) 先取 sessions 中所有出现过的 device
        let mut stmt = conn.prepare("SELECT DISTINCT device FROM sessions")?;
        let mut ids: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // 2) 本机即使暂无 session 也列出
        if !ids.iter().any(|i| i == current_id) {
            ids.push(current_id.to_string());
        }

        let mut out: Vec<DeviceStats> = Vec::with_capacity(ids.len());
        for id in ids {
            // 2.1 聚合统计
            let (total, count, earliest, latest): (i64, i64, Option<String>, Option<String>) = conn
                .query_row(
                    "SELECT COALESCE(SUM(duration_seconds), 0),
                            COUNT(*),
                            MIN(date),
                            MAX(date)
                     FROM sessions WHERE device = ?1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?;
            // 2.2 设备名（settings 优先）
            let name = if id == current_id {
                current_name.to_string()
            } else {
                conn.query_row(
                    "SELECT value FROM settings WHERE key=?1",
                    params![format!("device_name:{}", id)],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .unwrap_or_else(|| id.clone())
            };
            out.push(DeviceStats {
                device_id: id,
                device_name: name,
                total_seconds: total,
                session_count: count,
                earliest_date: earliest.unwrap_or_default(),
                latest_date: latest.unwrap_or_default(),
            });
        }
        // 按总秒数倒序，最活跃的排前面
        out.sort_by_key(|a| std::cmp::Reverse(a.total_seconds));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::AppDb;
    use rusqlite::Connection;
    use std::collections::HashSet;
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    fn rules_test_db() -> AppDb {
        let conn = Connection::open_in_memory().expect("创建内存数据库");
        AppDb::migrate(&conn).expect("初始化测试数据库");
        conn.execute("DELETE FROM classification_rules", [])
            .expect("清空种子规则以隔离分页测试");
        AppDb(Mutex::new(conn))
    }

    #[test]
    fn rules_page_counts_boundaries_and_clamps_after_last_page_delete() {
        let db = rules_test_db();
        for i in 0..34 {
            let category_id = if i % 2 == 0 { "dev" } else { "social" };
            db.insert_rule(
                "process_name",
                "contains",
                &format!("Rule-{i:02}"),
                category_id,
                0,
                true,
            )
            .expect("插入分页测试规则");
        }
        let expected: Vec<String> = (0..34)
            .filter(|i| i % 2 == 0)
            .chain((0..34).filter(|i| i % 2 != 0))
            .map(|i| format!("Rule-{i:02}"))
            .collect();

        let mut actual = Vec::new();
        for (index, expected_len) in [11, 11, 11, 1].into_iter().enumerate() {
            let out = db
                .get_rules_page("", index as i64 + 1, 11, "category_id", "asc")
                .expect("读取默认排序分页");
            assert_eq!(out.total, 34);
            assert_eq!(out.page, index as i64 + 1);
            assert_eq!(out.page_size, 11);
            assert_eq!(out.items.len(), expected_len);
            actual.extend(out.items.into_iter().map(|rule| rule.pattern));
        }
        assert_eq!(actual, expected, "默认排序必须覆盖全部规则且无重复");

        let beyond = db
            .get_rules_page("", 99, 11, "category_id", "asc")
            .expect("越界页码应收敛到末页");
        assert_eq!(beyond.page, 4);
        assert_eq!(beyond.items.len(), 1);
        assert_eq!(beyond.items[0].pattern, "Rule-33");
        let deleted_id = beyond.items[0].id;
        db.delete_rule(deleted_id).expect("删除末页唯一规则");

        let clamped = db
            .get_rules_page("", 4, 11, "category_id", "asc")
            .expect("删除后应重新收敛页码");
        assert_eq!(clamped.total, 33);
        assert_eq!(clamped.page, 3);
        assert_eq!(clamped.items.len(), 11);
        assert!(clamped.items.iter().all(|rule| rule.id != deleted_id));
        let page_zero = db
            .get_rules_page("", 0, 11, "category_id", "asc")
            .expect("非正页码应收敛到第一页");
        assert_eq!(page_zero.page, 1);
    }

    #[test]
    fn rules_page_custom_sort_is_stable_across_pages() {
        let db = rules_test_db();
        let fields = [
            "process_name",
            "name",
            "window_title",
            "exe_path",
            "bundle_id",
        ];
        for i in 0usize..34 {
            let combination = i / 5;
            db.insert_rule(
                fields[combination % fields.len()],
                if combination < fields.len() {
                    "contains"
                } else {
                    "equals"
                },
                &format!("Same-{:02}", i % 5),
                if i % 2 == 0 { "dev" } else { "social" },
                (i % 3) as i32,
                i % 2 == 0,
            )
            .expect("插入重复排序键测试规则");
        }

        let mut expected = db.get_rules_out().expect("读取排序期望值");
        expected.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.pattern.cmp(&b.pattern))
                .then_with(|| a.id.cmp(&b.id))
        });
        let expected_ids: Vec<i64> = expected.iter().map(|rule| rule.id).collect();
        let mut actual_ids = Vec::new();
        for page in 1..=6 {
            let out = db
                .get_rules_page("", page, 6, "priority", "desc")
                .expect("读取自定义排序分页");
            assert_eq!(out.total, 34);
            actual_ids.extend(out.items.into_iter().map(|rule| rule.id));
        }
        assert_eq!(
            actual_ids, expected_ids,
            "相同优先级必须按 pattern、id 稳定排序"
        );
        assert_eq!(
            actual_ids.into_iter().collect::<HashSet<_>>().len(),
            34,
            "跨页不得重复或遗漏规则"
        );

        for sort_key in [
            "field",
            "match_type",
            "pattern",
            "category_id",
            "priority",
            "enabled",
        ] {
            let out = db
                .get_rules_page("", 1, 100, sort_key, "asc")
                .expect("所有契约排序字段都应可查询");
            assert_eq!(out.total, 34, "sort_key={sort_key}");
            assert_eq!(out.items.len(), 34, "sort_key={sort_key}");
        }
    }

    #[test]
    fn rules_page_search_is_trimmed_ascii_case_insensitive_and_literal() {
        let db = rules_test_db();
        {
            let conn = db.0.lock().unwrap_or_else(|e| e.into_inner());
            conn.execute(
                "INSERT INTO categories (id, name, color) VALUES ('custom', 'MixedName', '#000000')",
                [],
            )
            .expect("插入英文分类名测试数据");
        }
        for (pattern, category_id) in [
            ("NeedlePattern", "dev"),
            ("plain", "productivity"),
            ("100%_done", "social"),
            ("100ZZdone", "social"),
            ("under_score", "game"),
            ("underXscore", "game"),
            ("category-name-target", "custom"),
        ] {
            db.insert_rule("process_name", "contains", pattern, category_id, 0, true)
                .expect("插入搜索测试规则");
        }
        let matches = |keyword: &str| {
            let mut patterns: Vec<String> = db
                .get_rules_page(keyword, 99, 20, "category_id", "asc")
                .expect("执行关键词搜索")
                .items
                .into_iter()
                .map(|rule| rule.pattern)
                .collect();
            patterns.sort();
            patterns
        };

        assert_eq!(matches("  nEeDlEpAtTeRn  "), ["NeedlePattern"]);
        assert_eq!(matches("DEV"), ["NeedlePattern"]);
        assert_eq!(matches("开发"), ["NeedlePattern"]);
        assert_eq!(matches("效率与财务"), ["plain"]);
        assert_eq!(matches("mixedname"), ["category-name-target"]);
        assert_eq!(matches("%"), ["100%_done"]);
        assert_eq!(matches("_"), ["100%_done", "under_score"]);
        assert_eq!(matches("100%_"), ["100%_done"]);

        let empty = db
            .get_rules_page("not-present", 99, 20, "category_id", "asc")
            .expect("空结果查询应成功");
        assert_eq!(empty.total, 0);
        assert_eq!(empty.page, 1);
        assert!(empty.items.is_empty());
        assert_eq!(
            db.get_rules_page("", 1, 0, "category_id", "asc")
                .expect("page_size 下限")
                .page_size,
            1
        );
        assert_eq!(
            db.get_rules_page("", 1, 101, "category_id", "asc")
                .expect("page_size 上限")
                .page_size,
            100
        );
    }

    fn keyboard_test_db() -> AppDb {
        let conn = Connection::open_in_memory().expect("创建内存数据库");
        AppDb::migrate(&conn).expect("初始化测试数据库");
        AppDb(Mutex::new(conn))
    }

    fn stored_key_count(db: &AppDb, device: &str, date: &str, key_id: i32) -> i64 {
        let conn = db.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.query_row(
            "SELECT COALESCE(SUM(count), 0) FROM key_usage_daily
             WHERE device = ?1 AND date = ?2 AND key_id = ?3",
            rusqlite::params![device, date, key_id],
            |r| r.get(0),
        )
        .expect("读取键盘聚合计数")
    }

    #[test]
    fn keyboard_increment_accumulates_and_accepts_empty_batches() {
        let db = keyboard_test_db();
        db.increment_key_usage("dev-a", &[])
            .expect("空批次应直接成功");
        db.increment_key_usage(
            "dev-a",
            &[
                ("2026-01-02".to_string(), 65, 2),
                ("2026-01-02".to_string(), 66, 1),
            ],
        )
        .expect("写入首批键盘计数");
        db.increment_key_usage("dev-a", &[("2026-01-02".to_string(), 65, 3)])
            .expect("重复键应累计");

        assert_eq!(stored_key_count(&db, "dev-a", "2026-01-02", 65), 5);
        assert_eq!(stored_key_count(&db, "dev-a", "2026-01-02", 66), 1);
        assert_eq!(stored_key_count(&db, "dev-b", "2026-01-02", 65), 0);
    }

    #[test]
    fn keyboard_summary_excludes_capslock_and_modifiers() {
        let db = keyboard_test_db();
        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let yesterday = (now - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let mut entries = vec![
            (today.clone(), 65, 8),
            (today.clone(), 66, 2),
            (yesterday, 65, 4),
            (today.clone(), 0x39, 100),
        ];
        for key_id in 0xE0..=0xE7 {
            entries.push((today.clone(), key_id, 50));
        }
        db.increment_key_usage("dev-a", &entries)
            .expect("写入普通键和控制键");
        db.increment_key_usage("dev-b", &[(today.clone(), 65, 100)])
            .expect("写入其他设备计数");
        let device = Some("dev-a".to_string());

        let (total, average, top) = db
            .keyboard_summary(2, "range-mode-ignores-this-date", &device)
            .expect("读取范围总览");
        assert_eq!(total, 14);
        assert_eq!(average, 7);
        assert_eq!(top.map(|key| (key.key_id, key.count)), Some((65, 12)));

        let (daily_total, daily_average, daily_top) = db
            .keyboard_summary(0, &today, &device)
            .expect("读取指定日总览");
        assert_eq!(daily_total, 10);
        assert_eq!(daily_average, 0);
        assert_eq!(daily_top.map(|key| (key.key_id, key.count)), Some((65, 8)));
    }

    #[test]
    fn keyboard_period_filters_inclusive_dates_and_devices() {
        let db = keyboard_test_db();
        db.increment_key_usage(
            "dev-a",
            &[
                ("2026-01-01".to_string(), 65, 2),
                ("2026-01-01".to_string(), 0x39, 20),
                ("2026-01-01".to_string(), 0xE0, 30),
                ("2026-01-02".to_string(), 65, 1),
                ("2026-01-02".to_string(), 66, 4),
                ("2026-01-02".to_string(), 0xE7, 40),
                ("2026-01-03".to_string(), 67, 9),
            ],
        )
        .expect("写入区间测试数据");
        db.increment_key_usage("dev-b", &[("2026-01-01".to_string(), 65, 100)])
            .expect("写入设备过滤数据");
        let device = Some("dev-a".to_string());

        let (daily, keys, total) = db
            .keyboard_period("2026-01-01", "2026-01-02", &device)
            .expect("读取包含首尾日期的区间");
        assert_eq!(total, 7);
        assert_eq!(
            daily
                .into_iter()
                .map(|day| (day.date, day.count))
                .collect::<Vec<_>>(),
            vec![("2026-01-01".to_string(), 2), ("2026-01-02".to_string(), 5)]
        );
        assert_eq!(
            keys.into_iter()
                .map(|key| (key.key_id, key.count))
                .collect::<Vec<_>>(),
            vec![(0xE7, 40), (0xE0, 30), (0x39, 20), (66, 4), (65, 3)],
            "热力图聚合必须保留 CapsLock 和修饰键"
        );

        let (all_device_daily, _, all_device_total) = db
            .keyboard_period("2026-01-01", "2026-01-01", &None)
            .expect("空设备过滤应合并全部设备");
        assert_eq!(all_device_daily.len(), 1);
        assert_eq!(all_device_total, 102);
    }

    #[test]
    fn keyboard_export_import_is_filtered_idempotent_and_legacy_compatible() {
        let source = keyboard_test_db();
        source
            .increment_key_usage(
                "dev-a",
                &[
                    ("2026-01-01".to_string(), 65, 10),
                    ("2026-01-01".to_string(), 66, 5),
                ],
            )
            .expect("写入待导出设备");
        source
            .increment_key_usage("dev-b", &[("2026-01-01".to_string(), 65, 7)])
            .expect("写入应被过滤的设备");
        let bundle = source
            .export_all_filtered(Some("dev-a"))
            .expect("按设备导出键盘统计");
        assert_eq!(bundle.version, 2);
        assert_eq!(bundle.key_usage.len(), 2);
        assert!(bundle.key_usage.iter().all(|usage| usage.device == "dev-a"));

        let target = keyboard_test_db();
        target
            .increment_key_usage(
                "dev-a",
                &[
                    ("2026-01-01".to_string(), 65, 12),
                    ("2026-01-01".to_string(), 66, 3),
                ],
            )
            .expect("写入导入前的本地计数");
        target.import_data(&bundle).expect("首次导入");
        target.import_data(&bundle).expect("重复导入仍应成功");
        assert_eq!(stored_key_count(&target, "dev-a", "2026-01-01", 65), 12);
        assert_eq!(stored_key_count(&target, "dev-a", "2026-01-01", 66), 3);
        assert_eq!(stored_key_count(&target, "dev-b", "2026-01-01", 65), 0);

        let mut legacy_json = serde_json::to_value(&bundle).expect("序列化测试备份");
        legacy_json
            .as_object_mut()
            .expect("备份应为 JSON object")
            .remove("key_usage");
        let legacy: super::ExportBundle =
            serde_json::from_value(legacy_json).expect("旧备份缺少 key_usage 时仍可解析");
        assert!(legacy.key_usage.is_empty());
        target.import_data(&legacy).expect("旧备份仍可导入");
        assert_eq!(stored_key_count(&target, "dev-a", "2026-01-01", 65), 12);
    }

    #[test]
    fn keyboard_import_newer_snapshot_wins_without_double_counting() {
        let source = keyboard_test_db();
        source
            .increment_key_usage("dev-a", &[("2026-01-01".to_string(), 65, 100)])
            .expect("写入旧快照计数");
        let backup = source
            .export_all_filtered(Some("dev-a"))
            .expect("导出旧快照");

        let target = keyboard_test_db();
        target.import_data(&backup).expect("首次恢复旧快照");
        target.clear_keyboard_stats().expect("清空后模拟分叉");
        std::thread::sleep(Duration::from_millis(2));
        target
            .increment_key_usage("dev-a", &[("2026-01-01".to_string(), 65, 5)])
            .expect("清空后新增 5 次");
        target.import_data(&backup).expect("再次导入旧备份");
        assert_eq!(stored_key_count(&target, "dev-a", "2026-01-01", 65), 5);
        target.import_data(&backup).expect("重复导入仍不翻倍");
        assert_eq!(stored_key_count(&target, "dev-a", "2026-01-01", 65), 5);

        let mut future = backup;
        future.version = 3;
        assert!(
            target.import_data(&future).is_err(),
            "未知新备份版本必须拒绝"
        );
    }

    #[test]
    fn keyboard_prune_and_device_delete_follow_session_lifecycle() {
        let db = keyboard_test_db();
        let now = chrono::Local::now();
        let old = (now - chrono::Duration::days(31))
            .format("%Y-%m-%d")
            .to_string();
        let cutoff = (now - chrono::Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();
        let recent = now.format("%Y-%m-%d").to_string();
        db.increment_key_usage(
            "dev-a",
            &[
                (old.clone(), 65, 5),
                (cutoff.clone(), 65, 7),
                (recent.clone(), 65, 11),
            ],
        )
        .expect("写入保留期测试数据");
        db.increment_key_usage("dev-b", &[(old.clone(), 65, 9), (recent.clone(), 65, 13)])
            .expect("写入其他设备保留期数据");

        db.prune_old(30, Some("dev-a"))
            .expect("按设备清理过期键盘数据");
        assert_eq!(stored_key_count(&db, "dev-a", &old, 65), 0);
        assert_eq!(stored_key_count(&db, "dev-a", &cutoff, 65), 7);
        assert_eq!(stored_key_count(&db, "dev-b", &old, 65), 9);
        db.prune_old(30, None).expect("清理全部过期键盘数据");
        assert_eq!(stored_key_count(&db, "dev-b", &old, 65), 0);
        assert_eq!(stored_key_count(&db, "dev-b", &recent, 65), 13);

        db.delete_all_sessions_for_device("dev-a")
            .expect("按设备删除全部数据");
        assert_eq!(stored_key_count(&db, "dev-a", &cutoff, 65), 0);
        assert_eq!(stored_key_count(&db, "dev-a", &recent, 65), 0);
        assert_eq!(stored_key_count(&db, "dev-b", &recent, 65), 13);
    }

    #[test]
    fn clear_keyboard_stats_clears_every_device() {
        let db = keyboard_test_db();
        db.increment_key_usage("dev-a", &[("2026-01-01".to_string(), 65, 1)])
            .expect("写入设备 A");
        db.increment_key_usage("dev-b", &[("2026-01-01".to_string(), 65, 2)])
            .expect("写入设备 B");

        db.clear_keyboard_stats().expect("清空键盘统计");
        let conn = db.0.lock().unwrap_or_else(|e| e.into_inner());
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM key_usage_daily", [], |r| r.get(0))
            .expect("读取剩余键盘记录数");
        assert_eq!(remaining, 0);
    }

    /// 回归：导出复用已持锁连接，必须完成且带上桌宠状态。
    #[test]
    fn export_all_filtered_completes_and_includes_pet_state() {
        let conn = Connection::open_in_memory().expect("创建内存数据库");
        AppDb::migrate(&conn).expect("初始化测试数据库");
        let db = Arc::new(AppDb(Mutex::new(conn)));
        let pet_state = r#"{"mood":"happy","level":7}"#;
        db.set_setting("pet_state", pet_state)
            .expect("写入桌宠状态");

        let export_db = Arc::clone(&db);
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            tx.send(export_db.export_all_filtered(None))
                .expect("返回导出结果");
        });
        let result = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("导出不得在数据库互斥锁上死锁");
        worker.join().expect("导出线程不得 panic");
        let bundle = result.expect("导出应成功");

        assert_eq!(
            bundle.pet_state.as_deref(),
            Some(pet_state),
            "导出必须包含 settings 中的 pet_state"
        );
    }

    /// 回归：非法桌宠状态必须在首条 SQL 写入前拒绝，且不得覆盖现有设置。
    #[test]
    fn import_rejects_invalid_pet_state_before_writing_settings() {
        for invalid in ["not-json", "null", "[]", r#""text""#, "42"] {
            let conn = Connection::open_in_memory().expect("创建内存数据库");
            AppDb::migrate(&conn).expect("初始化测试数据库");
            let db = AppDb(Mutex::new(conn));
            let original_pet_state = r#"{"mood":"happy","level":7}"#;
            db.set_setting("pet_state", original_pet_state)
                .expect("写入原桌宠状态");
            db.set_setting("device_name:existing", "keep-me")
                .expect("写入原设备名");

            let mut bundle = db.export_all().expect("构造导入包");
            bundle.pet_state = Some(invalid.to_string());
            bundle
                .devices
                .insert("new-device".to_string(), "must-not-import".to_string());
            bundle
                .devices
                .insert("existing".to_string(), "must-not-overwrite".to_string());

            let error = db
                .import_data(&bundle)
                .expect_err(&format!("{invalid} 不应作为桌宠状态导入"));
            assert!(
                error.to_string().contains("bundle.pet_state"),
                "错误应指出非法字段：{error}"
            );
            assert_eq!(
                db.get_setting("pet_state").as_deref(),
                Some(original_pet_state),
                "{invalid} 不得覆盖原桌宠状态"
            );
            assert_eq!(
                db.get_setting("device_name:existing").as_deref(),
                Some("keep-me"),
                "{invalid} 不得改写既有设备名"
            );
            assert_eq!(
                db.get_setting("device_name:new-device"),
                None,
                "{invalid} 不得先写入设备映射再报错"
            );
        }
    }

    /// 旧库既没有键盘统计表、sessions 也没有 device 列时，升级必须补全且保留原记录。
    #[test]
    fn migrate_legacy_database_adds_keyboard_table_without_losing_sessions() {
        let conn = Connection::open_in_memory().expect("创建旧版内存数据库");
        conn.execute_batch(
            "CREATE TABLE sessions (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              app_id INTEGER NOT NULL,
              start_at DATETIME NOT NULL,
              end_at DATETIME NOT NULL,
              duration_seconds INTEGER NOT NULL,
              date TEXT NOT NULL,
              window_title TEXT
            );
            INSERT INTO sessions (app_id,start_at,end_at,duration_seconds,date)
              VALUES (1,'2026-01-01T08:00:00+08:00','2026-01-01T08:10:00+08:00',600,'2026-01-01');
            CREATE TABLE key_usage_daily (
              device TEXT, date TEXT, key_id INTEGER, count INTEGER NOT NULL,
              PRIMARY KEY(device,date,key_id)
            );
            INSERT INTO key_usage_daily VALUES ('legacy','2026-01-01',65,7);",
        )
        .expect("构造旧版 sessions 表和记录");

        AppDb::migrate(&conn).expect("旧库无损升级");
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, i64>(0))
                .expect("旧时段仍在"),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='device'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .expect("sessions 已补 device 列"),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='key_usage_daily'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .expect("key_usage_daily 已自动建表"),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('key_usage_daily') WHERE name='updated_at'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .expect("已有键盘表已补 updated_at 列"),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT count FROM key_usage_daily WHERE device='legacy' AND key_id=65",
                [],
                |r| r.get::<_, i64>(0),
            )
            .expect("已有键盘计数保留"),
            7
        );
    }
}
