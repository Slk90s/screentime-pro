//!
//! categorizer.rs — 软件自动归类（本地字典 + 联网搜索 + other 兜底）
//!
//! 设计思路：
//! - 本地字典（内置 60+ 常见软件）覆盖 90% 装机场景，离线即可
//! - 联网搜索用 Wikipedia API（opensearch）+ 摘要关键词匹配，弥补字典盲区
//! - LRU 缓存避免重复联网（同一进程名只用查一次）
//! - 三层都失败时兜底返回 "other"
//!
//! 修改历史：
//!   - 2026-07-09 @v0.4.0: 初始创建 - 本地字典 + Wikipedia API + LRU 缓存

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::Duration;

/// 本地软件分类字典（60+ 常见软件）
/// - key：进程名（小写）或展示名（小写）子串匹配
/// - value：分类 id（与 categories 表对齐：efficiency/social/dev/entertainment/other/...）
/// - 匹配方式：contains（任一字段命中即返回）
fn local_dict() -> Vec<(&'static str, &'static str)> {
    vec![
        // ===== 浏览器 → 效率 =====
        ("chrome", "efficiency"),
        ("chromium", "efficiency"),
        ("firefox", "efficiency"),
        ("safari", "efficiency"),
        ("edge", "efficiency"),
        ("brave", "efficiency"),
        ("opera", "efficiency"),
        ("vivaldi", "efficiency"),
        ("arc", "efficiency"),
        // ===== 办公/效率 → 效率 =====
        ("word", "efficiency"),
        ("excel", "efficiency"),
        ("powerpoint", "efficiency"),
        ("pages", "efficiency"),
        ("numbers", "efficiency"),
        ("keynote", "efficiency"),
        ("notion", "efficiency"),
        ("obsidian", "efficiency"),
        ("logseq", "efficiency"),
        ("typora", "efficiency"),
        ("evernote", "efficiency"),
        ("onenote", "efficiency"),
        ("bear", "efficiency"),
        ("ulysses", "efficiency"),
        // ===== 通讯/社交 → 社交 =====
        ("wechat", "social"),
        ("qq", "social"),
        ("dingtalk", "social"),
        ("lark", "efficiency"), // 飞书既是沟通也是协作
        ("feishu", "efficiency"),
        ("slack", "social"),
        ("discord", "social"),
        ("telegram", "social"),
        ("whatsapp", "social"),
        ("telegram desktop", "social"),
        ("skype", "social"),
        ("zoom", "social"),
        ("teams", "social"),
        ("meet", "social"),
        ("微信", "social"),
        // ===== 开发工具 → 开发 =====
        ("vscode", "dev"),
        ("code", "dev"),
        ("intellij", "dev"),
        ("webstorm", "dev"),
        ("pycharm", "dev"),
        ("goland", "dev"),
        ("clion", "dev"),
        ("rider", "dev"),
        ("android studio", "dev"),
        ("xcode", "dev"),
        ("terminal", "dev"),
        ("iterm", "dev"),
        ("warp", "dev"),
        ("postman", "dev"),
        ("insomnia", "dev"),
        ("docker", "dev"),
        ("git", "dev"),
        ("sublime", "dev"),
        ("vim", "dev"),
        ("nvim", "dev"),
        // ===== 媒体/创作 → 创作 =====
        ("photoshop", "creative"),
        ("illustrator", "creative"),
        ("figma", "creative"),
        ("sketch", "creative"),
        ("xd", "creative"),
        ("figjam", "creative"),
        ("blender", "creative"),
        ("maya", "creative"),
        ("cinema 4d", "creative"),
        ("after effects", "creative"),
        ("premiere", "creative"),
        ("final cut", "creative"),
        ("davinci", "creative"),
        ("lightroom", "creative"),
        ("capture one", "creative"),
        // ===== 通讯/会议（已合并到 social）=====
        // ===== 娱乐/游戏 → 娱乐 =====
        ("steam", "entertainment"),
        ("epic games", "entertainment"),
        ("battle.net", "entertainment"),
        ("origin", "entertainment"),
        ("league of legends", "entertainment"),
        ("steam_app", "entertainment"),
        // ===== 音乐/视频 → 娱乐 =====
        ("spotify", "entertainment"),
        ("网易云", "entertainment"),
        ("netease", "entertainment"),
        ("qq music", "entertainment"),
        ("apple music", "entertainment"),
        ("youtube", "entertainment"),
        ("netflix", "entertainment"),
        ("bilibili", "entertainment"),
        ("哔哩哔哩", "entertainment"),
        ("腾讯视频", "entertainment"),
        ("爱奇艺", "entertainment"),
        ("youku", "entertainment"),
        ("优酷", "entertainment"),
        ("抖音", "entertainment"),
        ("tiktok", "entertainment"),
        // ===== 系统/工具 → 工具 =====
        ("finder", "tools"),
        ("explorer", "tools"),
        ("settings", "tools"),
        ("系统偏好", "tools"),
        ("系统设置", "tools"),
        ("preferences", "tools"),
        ("activity monitor", "tools"),
        ("任务管理器", "tools"),
        // ===== 学习/教育 → 学习 =====
        ("anki", "learning"),
        ("obsidian", "learning"),
        ("zotero", "learning"),
        ("marginnote", "learning"),
        ("goodnotes", "learning"),
        ("notability", "learning"),
    ]
}

/// 在本地字典里查
/// - 字段：process_name / exe_path / display_name 都参与匹配（小写 contains）
fn lookup_local(process_name: &str, exe_path: Option<&str>, display_name: &str) -> Option<String> {
    let pn = process_name.to_lowercase();
    let dn = display_name.to_lowercase();
    let ep = exe_path.map(|s| s.to_lowercase()).unwrap_or_default();
    for (key, cat) in local_dict() {
        let k = key.to_lowercase();
        if pn.contains(&k) || dn.contains(&k) || (ep.contains(&k) && !ep.is_empty()) {
            return Some(cat.to_string());
        }
    }
    None
}

/// 联网查 Wikipedia API：根据进程名/展示名拿摘要，再按关键词匹配分类
/// 失败/超时/无摘要 → 返回 None
async fn lookup_wikipedia(
    client: &reqwest::Client,
    query: &str,
) -> Option<String> {
    // opensearch API：返回最匹配的一篇 wiki 标题
    let resp = client
        .get("https://en.wikipedia.org/w/api.php")
        .query(&[
            ("action", "opensearch"),
            ("search", query),
            ("limit", "1"),
            ("namespace", "0"),
            ("format", "json"),
        ])
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    #[derive(serde::Deserialize)]
    struct OpensearchResp {
        // [query, [titles], [descriptions], [urls]]
        // 我们只需要 titles[0]
        // 用 serde_json::Value 直接解析更稳，这里简化
    }
    let val: serde_json::Value = resp.json().await.ok()?;
    let title = val.get(1)?.get(0)?.as_str()?;
    // 再用 page summary API 拿摘要
    let url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", title);
    let resp2 = client
        .get(&url)
        .send()
        .await
        .ok()?;
    #[derive(serde::Deserialize)]
    struct Summary {
        extract: Option<String>,
    }
    let sum: Summary = resp2.json().await.ok()?;
    let text = sum.extract?.to_lowercase();
    // 关键词匹配（简单粗暴；改进可换成 category 黑名单 + 关键词权重）
    let keywords: &[(&str, &str)] = &[
        ("web browser", "efficiency"),
        ("search engine", "efficiency"),
        ("office suite", "efficiency"),
        ("word processor", "efficiency"),
        ("spreadsheet", "efficiency"),
        ("presentation", "efficiency"),
        ("messaging app", "social"),
        ("chat", "social"),
        ("social media", "social"),
        ("instant messaging", "social"),
        ("video calling", "social"),
        ("video conferencing", "social"),
        ("integrated development environment", "dev"),
        ("code editor", "dev"),
        ("source-code editor", "dev"),
        ("version control", "dev"),
        ("image editing", "creative"),
        ("photo editing", "creative"),
        ("vector graphics", "creative"),
        ("3d modeling", "creative"),
        ("video editing", "creative"),
        ("video player", "entertainment"),
        ("music streaming", "entertainment"),
        ("music player", "entertainment"),
        ("streaming media", "entertainment"),
        ("video game", "entertainment"),
    ];
    for (kw, cat) in keywords {
        if text.contains(kw) {
            return Some(cat.to_string());
        }
    }
    None
}

/// LRU 缓存（容量 256，超出淘汰最早插入）
pub struct CategoryCache {
    map: Mutex<lru::LruCache<String, String>>,
}

impl CategoryCache {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(256).expect("capacity > 0"),
            )),
        }
    }
    pub fn get(&self, key: &str) -> Option<String> {
        self.map.lock().unwrap().get(key).cloned()
    }
    pub fn put(&self, key: String, val: String) {
        self.map.lock().unwrap().put(key, val);
    }
}

impl Default for CategoryCache {
    fn default() -> Self {
        Self::new()
    }
}

/// 整体查询入口：本地 → 缓存 → 联网 → 兜底 other
/// - cache：跨调用复用（建议在 AppState 里持有）
pub async fn lookup_category(
    process_name: &str,
    exe_path: Option<&str>,
    display_name: &str,
    cache: &CategoryCache,
) -> String {
    // 1. 先看缓存
    let cache_key = format!(
        "{}|{}|{}",
        process_name.to_lowercase(),
        display_name.to_lowercase(),
        exe_path.unwrap_or("").to_lowercase()
    );
    if let Some(cat) = cache.get(&cache_key) {
        return cat;
    }

    // 2. 本地字典
    if let Some(cat) = lookup_local(process_name, exe_path, display_name) {
        cache.put(cache_key, cat.clone());
        return cat;
    }

    // 3. 联网 Wikipedia（异步；失败/超时返回 None → 兜底 other）
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("ScreenTime-Pro-Categorizer/0.4")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    // 取最有可能搜到的关键词：display_name 优先（人话），其次 process_name
    let query = if !display_name.is_empty() {
        display_name
    } else {
        process_name
    };
    if let Some(cat) = lookup_wikipedia(&client, query).await {
        cache.put(cache_key, cat.clone());
        return cat;
    }

    // 4. 兜底
    let fallback = "other".to_string();
    cache.put(cache_key, fallback.clone());
    fallback
}