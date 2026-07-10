-- ScreenTime Pro 数据库 schema (SQLite)
-- 由 rusqlite bundled 在首次启动时自动创建

CREATE TABLE IF NOT EXISTS apps (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  name          TEXT NOT NULL,
  process_name  TEXT NOT NULL,
  exe_path      TEXT,
  icon_blob     BLOB,
  category_id   TEXT DEFAULT 'other',
  platform      TEXT NOT NULL DEFAULT 'unknown',
  created_at    DATETIME DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(process_name, platform)
);
CREATE INDEX IF NOT EXISTS idx_apps_proc ON apps(process_name, platform);

CREATE TABLE IF NOT EXISTS sessions (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  app_id          INTEGER NOT NULL REFERENCES apps(id),
  start_at        DATETIME NOT NULL,
  end_at          DATETIME NOT NULL,
  duration_seconds INTEGER NOT NULL,
  date            TEXT NOT NULL,
  window_title    TEXT,
  device          TEXT NOT NULL DEFAULT 'default'
);
CREATE INDEX IF NOT EXISTS idx_sessions_app_date ON sessions(app_id, date);
CREATE INDEX IF NOT EXISTS idx_sessions_date ON sessions(date);

CREATE TABLE IF NOT EXISTS daily_summaries (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  date          TEXT NOT NULL,
  app_id        INTEGER NOT NULL REFERENCES apps(id),
  total_seconds INTEGER NOT NULL DEFAULT 0,
  session_count INTEGER DEFAULT 0,
  UNIQUE(date, app_id)
);
CREATE INDEX IF NOT EXISTS idx_daily_date ON daily_summaries(date);

CREATE TABLE IF NOT EXISTS categories (
  id    TEXT PRIMARY KEY,
  name  TEXT NOT NULL,
  color TEXT NOT NULL
);

-- 分类规则表：采集到的应用按「字段 + 匹配方式 + 匹配值」自动归入分类
-- 支持的 field：process_name | window_title | exe_path | bundle_id | name
-- 支持的 match_type：contains | equals | prefix | suffix | regex
CREATE TABLE IF NOT EXISTS classification_rules (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  field       TEXT NOT NULL,
  match_type  TEXT NOT NULL DEFAULT 'contains',
  pattern     TEXT NOT NULL,
  category_id TEXT NOT NULL,
  priority    INTEGER NOT NULL DEFAULT 0,
  enabled     INTEGER NOT NULL DEFAULT 1,
  UNIQUE(field, match_type, pattern)
);
CREATE INDEX IF NOT EXISTS idx_rules_cat ON classification_rules(category_id);

-- 简单键值配置（如 autostart 用户偏好：true/false）
CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT
);
