-- 分类种子数据（首次启动写入，如冲突则忽略）
INSERT OR IGNORE INTO categories (id, name, color) VALUES
  ('social',        '社交',       '#FF7E27'),
  ('productivity',  '效率与财务', '#378ADD'),
  ('creative',      '创意',       '#D4537E'),
  ('entertainment', '娱乐',       '#BA7517'),
  ('dev',           '开发',       '#3B6D11'),
  ('education',     '教育',       '#534AB7'),
  ('game',          '游戏',       '#993C1D'),
  ('other',         '其他',       '#888780');
