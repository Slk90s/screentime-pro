-- 分类规则种子数据（首次启动写入，如冲突则忽略）
-- 字段说明：
--   field       = 匹配字段（process_name / window_title / exe_path / bundle_id / name）
--   match_type  = contains（包含）/ equals（相等）/ prefix（前缀）/ suffix（后缀）/ regex
--   pattern     = 匹配值（小写）
--   category_id = 命中后归入的分类
--   priority    = 优先级（大者优先）
--   enabled     = 是否启用（1 启用）
--
-- 注：默认以进程名（process_name）为主；窗口标题（window_title）规则用于
--     Windows（默认可取标题）与 macOS（授予屏幕录制权限后）的细粒度分类，
--     例如浏览器里看 Netflix / B 站自动归为「娱乐」。

INSERT OR IGNORE INTO classification_rules (field, match_type, pattern, category_id, priority, enabled) VALUES
  -- 社交
  ('process_name', 'contains', 'wechat',      'social',        0, 1),
  ('process_name', 'contains', 'wxwork',       'social',        0, 1),
  ('process_name', 'contains', 'qq',           'social',        0, 1),
  ('process_name', 'contains', 'dingtalk',     'social',        0, 1),
  ('process_name', 'contains', 'slack',        'social',        0, 1),
  ('process_name', 'contains', 'telegram',     'social',        0, 1),
  ('process_name', 'contains', 'discord',      'social',        0, 1),
  ('process_name', 'contains', 'feishu',       'social',        0, 1),
  ('process_name', 'contains', 'lark',         'social',        0, 1),
  -- 效率与财务
  ('process_name', 'contains', 'chrome',       'productivity',  0, 1),
  ('process_name', 'contains', 'safari',       'productivity',  0, 1),
  ('process_name', 'contains', 'edge',         'productivity',  0, 1),
  ('process_name', 'contains', 'firefox',      'productivity',  0, 1),
  ('process_name', 'contains', 'excel',        'productivity',  0, 1),
  ('process_name', 'contains', 'word',         'productivity',  0, 1),
  ('process_name', 'contains', 'powerpoint',   'productivity',  0, 1),
  ('process_name', 'contains', 'numbers',      'productivity',  0, 1),
  ('process_name', 'contains', 'pages',        'productivity',  0, 1),
  ('process_name', 'contains', 'keynote',      'productivity',  0, 1),
  ('process_name', 'contains', 'notion',       'productivity',  0, 1),
  ('process_name', 'contains', 'obsidian',     'productivity',  0, 1),
  -- 开发
  ('process_name', 'contains', 'xcode',        'dev',           0, 1),
  ('process_name', 'contains', 'code',         'dev',           0, 1),
  ('process_name', 'contains', 'cursor',       'dev',           0, 1),
  ('process_name', 'contains', 'webstorm',     'dev',           0, 1),
  ('process_name', 'contains', 'idea',         'dev',           0, 1),
  ('process_name', 'contains', 'terminal',     'dev',           0, 1),
  ('process_name', 'contains', 'iterm',        'dev',           0, 1),
  -- 创意
  ('process_name', 'contains', 'photoshop',    'creative',      0, 1),
  ('process_name', 'contains', 'illustrator',  'creative',      0, 1),
  ('process_name', 'contains', 'figma',        'creative',      0, 1),
  ('process_name', 'contains', 'sketch',       'creative',      0, 1),
  ('process_name', 'contains', 'premiere',     'creative',      0, 1),
  ('process_name', 'contains', 'final',        'creative',      0, 1),
  -- 游戏 / 娱乐
  ('process_name', 'contains', 'steam',        'game',          0, 1),
  ('process_name', 'contains', 'netflix',      'entertainment', 0, 1),
  ('process_name', 'contains', 'spotify',      'entertainment', 0, 1),
  ('process_name', 'contains', 'bilibili',     'entertainment', 0, 1),
  ('process_name', 'contains', 'youtube',      'entertainment', 0, 1),
  -- 窗口标题级细粒度（需 Windows 标题 / macOS 屏幕录制权限）
  ('window_title', 'contains', 'netflix',      'entertainment', 5, 1),
  ('window_title', 'contains', 'bilibili',     'entertainment', 5, 1),
  ('window_title', 'contains', 'youtube',      'entertainment', 5, 1);
