//! 本地数据库层（SQLite）
//!
//! 所有使用记录仅保存在本机 SQLite，零上传，隐私优先。
//! 数据库文件位于应用数据目录（macOS: ~/Library/Application Support/com.screentime.pro/screentime.db）。
//! 建表语句见 `sql/schema.sql`，分类种子见 `sql/seed_categories.sql`，规则种子见 `sql/seed_rules.sql`。

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
        Ok(())
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
        let conn = self.0.lock().unwrap();
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

    /// 写入一条使用时段，并同步更新每日聚合
    ///
    /// `device` 为来源设备 id（多设备合并用，默认本机）。
    pub fn insert_session(
        &self,
        app_id: i64,
        category_id: &str,
        start_rfc3339: &str,
        end_rfc3339: &str,
        duration: i64,
        date: &str,
        window_title: Option<&str>,
        device: &str,
    ) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_id, start_at, end_at, duration_seconds, date, window_title, device)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![app_id, start_rfc3339, end_rfc3339, duration, date, window_title, device],
        )?;
        conn.execute(
            "INSERT INTO daily_summaries (date, app_id, total_seconds, session_count)
             VALUES (?1,?2,?3,1)
             ON CONFLICT(date, app_id) DO UPDATE SET
               total_seconds = total_seconds + excluded.total_seconds,
               session_count = session_count + 1",
            params![date, app_id, duration],
        )?;
        // category_id 冗余写入 apps（若用户手动改过分类，以 apps 为准；此处不动）
        let _ = category_id;
        Ok(())
    }

    /// 近 N 天每日总览（按 device 过滤；空 device = 全部设备合并）
    pub fn get_daily_summaries(
        &self,
        days: u32,
        device: &Option<String>,
    ) -> rusqlite::Result<Vec<DailySummaryOut>> {
        let conn = self.0.lock().unwrap();
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
        let conn = self.0.lock().unwrap();
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
        let conn = self.0.lock().unwrap();
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

    /// 指定日期的 App 使用时长排行（按 device 过滤）
    pub fn get_app_ranking(
        &self,
        date: &str,
        device: &Option<String>,
    ) -> rusqlite::Result<Vec<AppRankingOut>> {
        let conn = self.0.lock().unwrap();
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
            "SELECT a.id, a.name, a.category_id, SUM(s.duration_seconds), COUNT(*), a.icon_blob
             FROM sessions s JOIN apps a ON s.app_id = a.id
             WHERE s.date = ?1{dev}
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

    /// 当日总览卡片数据（按 device 过滤）
    pub fn get_overview(
        &self,
        date: &str,
        device: &Option<String>,
    ) -> rusqlite::Result<OverviewOut> {
        let conn = self.0.lock().unwrap();
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
        let (total, count): (i64, i64) = conn
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(s.duration_seconds),0), COUNT(DISTINCT s.app_id)
                     FROM sessions s WHERE s.date=?1{dev}"
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
                     WHERE s.date=?1{dev} GROUP BY a.id ORDER BY SUM(s.duration_seconds) DESC LIMIT 1"
                ),
                p.as_slice(),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        let pickups: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM sessions s WHERE s.date=?1{dev}"),
                p.as_slice(),
                |r| r.get(0),
            )
            .unwrap_or(0);

        Ok(OverviewOut {
            date: date.to_string(),
            total_seconds: total,
            app_count: count,
            most_used_app: top.as_ref().map(|(n, _)| n.clone()),
            most_used_seconds: top.map(|(_, s)| s).unwrap_or(0),
            pickup_count: pickups,
        })
    }

    pub fn get_categories(&self) -> rusqlite::Result<Vec<CategoryOut>> {
        let conn = self.0.lock().unwrap();
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

    pub fn get_sessions(&self, date: &str) -> rusqlite::Result<Vec<SessionOut>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.app_id, a.name, a.category_id, s.start_at, s.end_at, s.duration_seconds
             FROM sessions s JOIN apps a ON s.app_id=a.id
             WHERE s.date=?1 ORDER BY s.start_at DESC",
        )?;
        let rows = stmt.query_map(params![date], |r| {
            Ok(SessionOut {
                id: r.get(0)?,
                app_id: r.get(1)?,
                app_name: r.get(2)?,
                category_id: r.get(3)?,
                start_at: r.get(4)?,
                end_at: r.get(5)?,
                duration_seconds: r.get(6)?,
            })
        })?;
        rows.collect()
    }

    /// 导出指定日期明细为 CSV 到给定路径
    pub fn export_csv(&self, out_path: &Path, date: &str) -> Result<(), String> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT a.name, a.category_id, s.start_at, s.end_at, s.duration_seconds
                 FROM sessions s JOIN apps a ON s.app_id=a.id
                 WHERE s.date=?1 ORDER BY s.start_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![date]).map_err(|e| e.to_string())?;
        let mut csv = String::from("app_name,category_id,start_at,end_at,duration_seconds\n");
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let name: String = row.get(0).map_err(|e| e.to_string())?;
            let cat: String = row.get(1).map_err(|e| e.to_string())?;
            let start: String = row.get(2).map_err(|e| e.to_string())?;
            let end: String = row.get(3).map_err(|e| e.to_string())?;
            let dur: i64 = row.get(4).map_err(|e| e.to_string())?;
            csv.push_str(&format!(
                "{},{},{},{},{}\n",
                csv_escape(&name),
                csv_escape(&cat),
                csv_escape(&start),
                csv_escape(&end),
                dur
            ));
        }
        fs::write(out_path, csv).map_err(|e| e.to_string())?;
        Ok(())
    }

    // ===================== 分类规则引擎持久化 =====================

    /// 加载全部分类规则（含禁用项），供采样循环在内存缓存后匹配
    pub fn load_rules(&self) -> rusqlite::Result<Vec<Rule>> {
        let conn = self.0.lock().unwrap();
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
        let conn = self.0.lock().unwrap();
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

    /// 新增规则；若 (field, match_type, pattern) 已存在则更新其分类/优先级并重新启用
    pub fn insert_rule(
        &self,
        field: &str,
        match_type: &str,
        pattern: &str,
        category_id: &str,
        priority: i32,
    ) -> rusqlite::Result<i64> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO classification_rules (field, match_type, pattern, category_id, priority, enabled)
             VALUES (?1,?2,?3,?4,?5,1)
             ON CONFLICT(field, match_type, pattern) DO UPDATE SET
               category_id=excluded.category_id, priority=excluded.priority, enabled=1",
            params![field, match_type, pattern, category_id, priority],
        )?;
        conn.query_row(
            "SELECT id FROM classification_rules WHERE field=?1 AND match_type=?2 AND pattern=?3",
            params![field, match_type, pattern],
            |r| r.get(0),
        )
    }

    /// 更新规则（按 id）
    pub fn update_rule(
        &self,
        id: i64,
        field: &str,
        match_type: &str,
        pattern: &str,
        category_id: &str,
        priority: i32,
        enabled: bool,
    ) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "UPDATE classification_rules
             SET field=?2, match_type=?3, pattern=?4, category_id=?5, priority=?6, enabled=?7
             WHERE id=?1",
            params![
                id,
                field,
                match_type,
                pattern,
                category_id,
                priority,
                if enabled { 1i64 } else { 0i64 }
            ],
        )?;
        Ok(())
    }

    /// 删除规则（按 id）
    pub fn delete_rule(&self, id: i64) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM classification_rules WHERE id=?1", params![id])?;
        Ok(())
    }

    /// 按当前规则重算所有已记录应用的分类（窗口标题/包名未存储，仅用进程名/路径/展示名）
    ///
    /// 返回被更新的应用数量。历史 session 通过 JOIN apps 自动跟随新分类。
    pub fn reclassify_all(&self, rules: &[Rule]) -> rusqlite::Result<usize> {
        let conn = self.0.lock().unwrap();
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
        let conn = self.0.lock().unwrap();
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
        })
    }

    // ===================== 多设备合并 =====================

    /// 列出所有出现过的设备（含本机，即使暂无数据也列出），名称优先取 settings
    pub fn get_devices(
        &self,
        current_id: &str,
        current_name: &str,
    ) -> rusqlite::Result<Vec<DeviceInfo>> {
        let conn = self.0.lock().unwrap();
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
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, process_name, exe_path, category_id, platform FROM apps",
        )?;
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

        let mut stmt2 = conn.prepare(
            "SELECT a.process_name, a.platform, s.start_at, s.end_at, s.duration_seconds, s.date, s.window_title, s.device
             FROM sessions s JOIN apps a ON s.app_id=a.id",
        )?;
        let sessions = stmt2
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
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;

        // 收集设备名映射（settings 中 device_name:<id>）
        let mut dev_stmt =
            conn.prepare("SELECT key, value FROM settings WHERE key LIKE 'device_name:%'")?;
        let mut devices: HashMap<String, String> = HashMap::new();
        let rows = dev_stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (k, v) = row?;
            if let Some(id) = k.strip_prefix("device_name:") {
                devices.insert(id.to_string(), v);
            }
        }

        Ok(ExportBundle {
            version: 1,
            exported_at: Local::now().to_rfc3339(),
            devices,
            apps,
            sessions,
        })
    }

    /// 导入全量数据并合并（按 start_at+app_id+device 去重，幂等）
    ///
    /// 返回新导入的时段条数。
    pub fn import_data(&self, bundle: &ExportBundle) -> rusqlite::Result<usize> {
        let conn = self.0.lock().unwrap();
        // 写入设备名映射
        for (id, name) in &bundle.devices {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1,?2) ON CONFLICT(key) DO NOTHING",
                params![format!("device_name:{}", id), name],
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
            // 同步更新每日聚合
            conn.execute(
                "INSERT INTO daily_summaries (date, app_id, total_seconds, session_count)
                 VALUES (?1,?2,?3,1)
                 ON CONFLICT(date, app_id) DO UPDATE SET
                   total_seconds = total_seconds + excluded.total_seconds,
                   session_count = session_count + 1",
                params![s.date, app_id, s.duration_seconds],
            )?;
            imported += 1;
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
    let conn = self.0.lock().unwrap();
    let cutoff = (Local::now() - Duration::days(retention_days as i64))
        .format("%Y-%m-%d")
        .to_string();
    let n: usize = match device_filter {
        Some(dev) if !dev.is_empty() => {
            // 仅清指定设备的旧数据（用于「按设备清理」入口）
            conn.execute(
                "DELETE FROM sessions WHERE date < ?1 AND device = ?2",
                params![cutoff, dev],
            )?
        }
        _ => conn.execute("DELETE FROM sessions WHERE date < ?1", params![cutoff])?,
    } as usize;
    conn.execute(
        "DELETE FROM daily_summaries WHERE date < ?1",
        params![cutoff],
    )?;
    Ok(n)
}

    // ===================== 简易键值配置 =====================

    /// 读取配置项（不存在返回 None）
    pub fn get_setting(&self, key: &str) -> Option<String> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT value FROM settings WHERE key=?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// 写入/覆盖配置项
    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

/// CSV 字段转义（含逗号/引号/换行时用双引号包裹并转义内部引号）
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 标准 base64 编码（内联实现，避免额外依赖）
fn to_base64(data: &[u8]) -> String {
    const CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
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
