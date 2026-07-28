//! 分类规则引擎（纯逻辑，不依赖数据库）
//!
//! 将采集到的原始应用（进程名 / 窗口标题 / 可执行路径 / 包名 / 展示名）
//! 按可配置规则自动归入某个分类（社交 / 效率 / 开发 / ...），
//! 无需导出后再人工整理。
//!
//! 规则来自数据库 `classification_rules` 表，由上层在内存缓存后传入本模块匹配；
//! 本模块只负责「给定规则 + 应用 → 分类」的纯计算，便于单测与复用。
//!
//! ## 大小写处理（重要）
//! - `value` 一律小写化（应用字段端）
//! - `pattern` **必须** 在比较前小写化（DB 端），否则自动规则 / 手动新建规则
//!   （原大小写如 `WorkBuddy`）会因 `equals` 大小写敏感而**永不命中**
//! - 历史踩坑（v0.6.2-beta.17）：旧版 `equals: value == rule.pattern` 中
//!   `value` 已 lowered，`rule.pattern` 未 lowered，导致 WorkBuddy 等
//!   进程的自动规则被创建后却不参与分类 → 用户看到"规则没生效"

use crate::tracker::platform::RawApp;

/// 单条分类规则（与数据库 `classification_rules` 表一一对应）
#[derive(Debug, Clone)]
pub struct Rule {
    pub id: i64,
    /// 匹配字段：process_name | window_title | exe_path | bundle_id | name
    pub field: String,
    /// 匹配方式：contains（包含）| equals（相等）| prefix（前缀）| suffix（后缀）| regex（正则）
    pub match_type: String,
    /// 匹配值（**比较时**统一小写，忽略大小写——DB 中可保留原样）
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

/// 单条规则匹配判断
///
/// **关键**：value 与 pattern **均**已小写（pattern 在此函数内 to_lowercase）。
/// 旧版本只 lowered value → DB 中的 `WorkBuddy` 与 lowered `"workbuddy"`
/// 永远不相等，即使规则存在也不生效。
fn rule_matches(rule: &Rule, value: &str) -> bool {
    let pattern = rule.pattern.to_lowercase();
    match rule.match_type.as_str() {
        "equals" => value == pattern,
        "prefix" => value.starts_with(&pattern),
        "suffix" => value.ends_with(&pattern),
        "regex" => {
            // 正则按用户原大小写走（大小写敏感是 regex 用户语义）；语法错误
            // 视为未命中，避免崩溃。
            regex::Regex::new(&rule.pattern)
                .map(|re| re.is_match(value))
                .unwrap_or(false)
        }
        _ => value.contains(&pattern), // 默认 contains
    }
}

#[cfg(test)]
mod tests {
    //! 分类规则匹配回归测试。
    //!
    //! 历史关键 bug（v0.6.2-beta.17 前）：`equals` 分支只 lowered value，
    //! 没 lowered pattern → DB 存 `WorkBuddy`、采集到 `"workbuddy"` 时不等。
    //!
    //! 注意：`rule_matches` 假设 `value` **已** lowered（来自 `classify_app`
    //! 的 `fields` 数组），所以单测里调用 `rule_matches` 时传 lowered value；
    //! `pattern` 的 lowering 是 `rule_matches` 内部完成（这是本模块契约）。
    use super::*;

    fn r(pattern: &str, match_type: &str) -> Rule {
        Rule {
            id: 1,
            field: "process_name".into(),
            match_type: match_type.into(),
            pattern: pattern.into(),
            category_id: "test".into(),
            priority: 0,
            enabled: true,
        }
    }

    #[test]
    fn equals_is_case_insensitive_on_pattern() {
        // 历史 bug 修复回归：DB 存 "WorkBuddy"，采集到的 lowered value 应命中
        assert!(rule_matches(&r("WorkBuddy", "equals"), "workbuddy"));
        // 任意大小写混合都能命中（pattern lowering 发生在内部）
        assert!(rule_matches(&r("WORKBUDDY", "equals"), "workbuddy"));
        assert!(rule_matches(&r("workbuddy", "equals"), "workbuddy"));
        // 不等
        assert!(!rule_matches(&r("wechat", "equals"), "workbuddy"));
    }

    #[test]
    fn contains_is_case_insensitive_on_pattern() {
        // pattern "buddy" vs value "workbuddy" → contains
        assert!(rule_matches(&r("buddy", "contains"), "workbuddy"));
        assert!(rule_matches(&r("BUDDY", "contains"), "workbuddy"));
        assert!(rule_matches(&r("Buddy", "contains"), "workbuddy"));
    }

    #[test]
    fn prefix_and_suffix_lowered() {
        assert!(rule_matches(&r("WORK", "prefix"), "workbuddy"));
        assert!(rule_matches(&r("work", "prefix"), "workbuddy"));
        assert!(rule_matches(&r("BUDDY", "suffix"), "workbuddy"));
    }

    #[test]
    fn regex_uses_raw_pattern() {
        // 正则模式按用户原大小写（大小写敏感是 regex 用户语义），syntax 错误视为未命中
        assert!(rule_matches(&r("^work.*$", "regex"), "workbuddy"));
        assert!(!rule_matches(&r("[invalid(", "regex"), "workbuddy"));
    }

    #[test]
    fn classify_app_picks_highest_priority() {
        // 端到端：两条规则都能命中但 priority 不同 → 高优先级胜出
        let rules = vec![
            Rule {
                id: 1,
                field: "process_name".into(),
                match_type: "equals".into(),
                pattern: "workbuddy".into(),
                category_id: "social".into(),
                priority: 0,
                enabled: true,
            },
            Rule {
                id: 2,
                field: "process_name".into(),
                match_type: "contains".into(),
                pattern: "buddy".into(), // 也能命中
                category_id: "other".into(),
                priority: 10,
                enabled: true,
            },
        ];
        let app = RawApp {
            process_name: "WorkBuddy".into(),
            name: "WorkBuddy".into(),
            exe_path: None,
            bundle_id: None,
            window_title: None,
        };
        assert_eq!(classify_app(&app, &rules), "other"); // priority=10 胜出
    }
}
