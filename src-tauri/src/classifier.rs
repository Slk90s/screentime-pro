//! 分类规则引擎（纯逻辑，不依赖数据库）
//!
//! 将采集到的原始应用（进程名 / 窗口标题 / 可执行路径 / 包名 / 展示名）
//! 按可配置规则自动归入某个分类（社交 / 效率 / 开发 / ...），
//! 无需导出后再人工整理。
//!
//! 规则来自数据库 `classification_rules` 表，由上层在内存缓存后传入本模块匹配；
//! 本模块只负责「给定规则 + 应用 → 分类」的纯计算，便于单测与复用。

use crate::tracker::platform::RawApp;

/// 单条分类规则（与数据库 `classification_rules` 表一一对应）
#[derive(Debug, Clone)]
pub struct Rule {
    pub id: i64,
    /// 匹配字段：process_name | window_title | exe_path | bundle_id | name
    pub field: String,
    /// 匹配方式：contains（包含）| equals（相等）| prefix（前缀）| suffix（后缀）| regex（正则）
    pub match_type: String,
    /// 匹配值（比对时已统一转小写，忽略大小写）
    pub pattern: String,
    /// 命中后归入的分类 id（对应 categories 表）
    pub category_id: String,
    /// 优先级，数值大者优先评估
    pub priority: i32,
    /// 是否启用
    pub enabled: bool,
}

/// 根据规则对原始应用分类，返回分类 id；无命中返回 "other"
pub fn classify_app(app: &RawApp, rules: &[Rule]) -> String {
    // 预先取出各字段的待匹配文本（统一转小写，实现忽略大小写匹配）
    let fields: [(&str, String); 5] = [
        ("process_name", app.process_name.to_lowercase()),
        (
            "window_title",
            app.window_title.clone().unwrap_or_default().to_lowercase(),
        ),
        ("exe_path", app.exe_path.clone().unwrap_or_default().to_lowercase()),
        ("bundle_id", app.bundle_id.clone().unwrap_or_default().to_lowercase()),
        ("name", app.name.to_lowercase()),
    ];

    // 仅评估启用中的规则，按优先级降序；高优先级先匹配先返回
    let mut ordered: Vec<&Rule> = rules.iter().filter(|r| r.enabled).collect();
    ordered.sort_by(|a, b| b.priority.cmp(&a.priority));

    for rule in ordered {
        if let Some(value) = fields
            .iter()
            .find(|(f, _)| *f == rule.field)
            .map(|(_, v)| v)
        {
            if rule_matches(rule, value) {
                return rule.category_id.clone();
            }
        }
    }
    "other".to_string()
}

/// 单条规则匹配判断（value 与 pattern 均已小写）
fn rule_matches(rule: &Rule, value: &str) -> bool {
    match rule.match_type.as_str() {
        "equals" => value == rule.pattern,
        "prefix" => value.starts_with(&rule.pattern),
        "suffix" => value.ends_with(&rule.pattern),
        "regex" => {
            // 正则语法错误时视为「未命中」，避免崩溃
            regex::Regex::new(&rule.pattern)
                .map(|re| re.is_match(value))
                .unwrap_or(false)
        }
        _ => value.contains(&rule.pattern), // 默认 contains
    }
}
