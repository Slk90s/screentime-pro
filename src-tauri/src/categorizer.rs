//!
//! categorizer.rs — 软件自动归类（本地字典 + 联网搜索 + other 兜底）
//!
//! 设计思路：
//! - 本地字典（内置 60+ 常见软件）覆盖 90% 装机场景，离线即可
//! - 联网搜索用 Wikipedia API（opensearch）+ 摘要关键词匹配，弥补字典盲区
//! - LRU 缓存避免重复联网（同一进程名只用查一次）
//! - 三层都失败时兜底返回 "other"
//!
//! ⚠️ 必须保持**同步**实现！
//!   旧版本用 `async fn` + `tauri::async_runtime::block_on` 在 `sampling_loop` 里调用，
//!   会让 tokio runtime worker thread 死锁（runtime 内 block_on nested async），
//!   导致整个采样循环卡住——已记录时间不再更新、数据库不再写入。
//!   现在改用 `reqwest::blocking::Client`，同步调用，零死锁风险。
//!
//! 修改历史：
//!   - 2026-07-09 @v0.4.0: 初始创建（async 版本） - 本地字典 + Wikipedia API + LRU 缓存
//!   - 2026-07-09 @v0.4.1: 修复 - 改为同步实现，避免 block_on 嵌套死锁（关键 bugfix）

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
/// **同步实现**，绝不能在 async 上下文里用 `block_on` 包这个函数！
fn lookup_wikipedia(client: &reqwest::blocking::Client, query: &str) -> Option<String> {
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
        .ok()?;
    let val: serde_json::Value = resp.json().ok()?;
    let title = val.get(1)?.get(0)?.as_str()?;
    // 再用 page summary API 拿摘要
    let url = format!(
        "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
        title
    );
    let resp2 = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .ok()?;
    #[derive(serde::Deserialize)]
    struct Summary {
        extract: Option<String>,
    }
    let sum: Summary = resp2.json().ok()?;
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
/// - 用 Arc<Mutex<...>> 而不是裸 Mutex<...>，方便 spawn_blocking 时 clone 引用
///
/// 🔴 v1.2.11（P1，2026-10-29）：**区分「确定的分类」与「查询失败的兜底」**。
///
/// 旧实现把「wikipedia 查询失败 → 回落 other」的结果也 `put` 进 LRU，
/// 于是同一条记录**永久**返回 `other`：网络恢复、VPN 断开重连之后都不会重试。
/// 真机日志里 `OPPO 互联` / `WorkBuddy` / `Clash Verge` / `UURemote` 反复刷
/// `wikipedia 分类失败，回落 other`，正是因为缓存里已经是 `other`
/// （命中缓存不会走网络，但**失败条目被不断重新写入**，看起来像一直在重试）。
///
/// 但也不能「失败就不缓存」：采样是 2s 一次，断网时会对同一个应用每 2 秒
/// 发起一次 8s 超时的网络请求，`spawn_blocking` 线程池会被打满。
///
/// 折中：失败结果进**独立的负缓存**，带短 TTL（10 分钟）。
/// - 命中负缓存且未过期 → 直接返回 other，不打网络
/// - 过期 → 允许重新查询（网络恢复后 10 分钟内自动纠正）
/// - 拿到确定分类（本地字典 / wikipedia 成功）→ 写入正缓存，**并清除负缓存**
#[derive(Clone)]
pub struct CategoryCache {
    map: std::sync::Arc<Mutex<lru::LruCache<String, String>>>,
    /// 失败兜底（`other`）的标记与时间戳：key → 写入时刻
    negative: std::sync::Arc<Mutex<lru::LruCache<String, std::time::Instant>>>,
}

/// 负缓存有效期。取 10 分钟的理由：
/// - 太短（如 1 分钟）→ 断网期间仍会频繁发起 8s 超时请求；
/// - 太长（如 1 小时）→ 网络恢复后分类长时间停在 other，用户感知为「分类坏了」。
/// 采样粒度是 2s，10 分钟内最多重试 1 次，代价可忽略。
const NEGATIVE_TTL: Duration = Duration::from_secs(600);

impl CategoryCache {
    pub fn new() -> Self {
        Self {
            map: std::sync::Arc::new(Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(256).expect("capacity > 0"),
            ))),
            negative: std::sync::Arc::new(Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(256).expect("capacity > 0"),
            ))),
        }
    }
    pub fn get(&self, key: &str) -> Option<String> {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }
    pub fn put(&self, key: String, val: String) {
        let mut g = self.map.lock().unwrap_or_else(|e| e.into_inner());
        // 拿到确定的分类后，负缓存必须失效，否则会继续被 other 挡住。
        self.negative
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop(&key);
        g.put(key, val);
    }

    /// 记录一次「查询失败，兜底 other」。
    pub fn put_negative(&self, key: String) {
        let mut g = self.negative.lock().unwrap_or_else(|e| e.into_inner());
        g.put(key, std::time::Instant::now());
    }

    /// 该 key 是否仍处于「刚失败过」的冷却窗口内。
    pub fn is_negative(&self, key: &str) -> bool {
        let mut g = self.negative.lock().unwrap_or_else(|e| e.into_inner());
        match g.get(key).copied() {
            Some(t) if t.elapsed() < NEGATIVE_TTL => true,
            Some(_) => {
                // 过期即清掉，让下一次查询真正走网络
                g.pop(key);
                false
            }
            None => false,
        }
    }
}

impl Default for CategoryCache {
    fn default() -> Self {
        Self::new()
    }
}

/// 共享的 blocking HTTP 客户端（一次性建好复用，避免每次采样都建连接池）
/// - timeout: 8s 整体请求超时
/// - user_agent: Wikipedia API 要求
use std::sync::OnceLock;
static HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
fn http_client() -> &'static reqwest::blocking::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent("ScreenTime-Pro-Categorizer/0.4")
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}

/// 仅查本地字典（不查缓存、不联网）
/// - 用于「大多数已知软件」快速命中，避免 spawn_blocking 阻塞；命中即返回 Some
/// - 同步、永不阻塞
pub fn lookup_local_only(
    process_name: &str,
    exe_path: Option<&str>,
    display_name: &str,
) -> Option<String> {
    lookup_local(process_name, exe_path, display_name)
}

/// 整体查询入口：本地 → 缓存 → 联网 → 兜底 other
/// - cache：跨调用复用（建议在 AppState 里持有）
/// - **同步实现**：可在普通同步函数 / 异步函数（非 block_on 嵌套）里安全调用
/// - ⚠️ 内部含同步 HTTP 调用（最多 8s），从 async 上下文调用时**必须**用
///   `tauri::async_runtime::spawn_blocking` 包装，避免阻塞 tokio worker
pub fn lookup_category(
    process_name: &str,
    exe_path: Option<&str>,
    display_name: &str,
    cache: &CategoryCache,
) -> String {
    let start = std::time::Instant::now();
    // 1. 先看正缓存（确定分类）
    let cache_key = format!(
        "{}|{}|{}",
        process_name.to_lowercase(),
        display_name.to_lowercase(),
        exe_path.unwrap_or("").to_lowercase()
    );
    if let Some(cat) = cache.get(&cache_key) {
        tracing::trace!(source = "cache", app = %display_name, "category hit");
        return cat;
    }

    // 1'. 负缓存冷却中：刚失败过，直接兜底，不再发起 8s 网络请求。
    // v1.2.11：没有这一层就会在断网时每 2s 打一次网络，把 spawn_blocking 池占满。
    if cache.is_negative(&cache_key) {
        tracing::trace!(app = %display_name, "category negative-cache hit, skip network");
        return "other".to_string();
    }

    // 2. 本地字典
    if let Some(cat) = lookup_local(process_name, exe_path, display_name) {
        tracing::debug!(source = "local", app = %display_name, cat = %cat, "category resolved");
        cache.put(cache_key, cat.clone());
        return cat;
    }

    // 3. 联网 Wikipedia（同步；失败/超时返回 None → 兜底 other）
    let query = if !display_name.is_empty() {
        display_name
    } else {
        process_name
    };
    if let Some(cat) = lookup_wikipedia(http_client(), query) {
        // INFO 级：每次新软件联网分类都记录（用户报 bug 时一眼看到这里）
        tracing::info!(
            source = "wikipedia",
            app = %display_name,
            cat = %cat,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "category resolved via web"
        );
        cache.put(cache_key, cat.clone());
        return cat;
    }
    // 联网也失败：WARN（不是 ERROR，因为有兜底）
    tracing::warn!(app = %display_name, "wikipedia 分类失败，回落 other");

    // 4. 兜底：**只进负缓存，不进正缓存**。
    // v1.2.11（P1）：旧实现把 other 写进正缓存（`cache.put(cache_key, "other")`），
    // 于是这条记录永久返回 other —— 网络恢复后也不会再查询。
    // 现在写负缓存，10 分钟后自动失效重试；真断网时又不会每 2s 重打网络。
    cache.put_negative(cache_key);
    "other".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🐞 回归（P1，2026-10-29 @v1.2.11）：**失败兜底不能污染正缓存**。
    ///
    /// 旧实现把 `other` 写进正缓存，导致一条「查询失败」的记录永久返回 other，
    /// 网络恢复后也不会重试。现在失败只进负缓存（带 TTL），
    /// 且正缓存写入时会清掉负缓存。
    #[test]
    fn negative_cache_does_not_pollute_positive_cache() {
        let c = CategoryCache::new();
        let key = "proc|name|path".to_string();

        // 模拟一次查询失败
        c.put_negative(key.clone());
        assert!(c.is_negative(&key), "失败后应处于冷却窗口内");
        assert!(
            c.get(&key).is_none(),
            "失败结果绝不能出现在正缓存里（否则永不重试）"
        );

        // 模拟网络恢复后查到了确定分类
        c.put(key.clone(), "productivity".to_string());
        assert_eq!(c.get(&key).as_deref(), Some("productivity"));
        assert!(
            !c.is_negative(&key),
            "拿到确定分类后，负缓存必须失效，否则仍会被 other 挡住"
        );
    }

    /// 负缓存只能挡住「同一个 key」，不能影响其它应用。
    #[test]
    fn negative_cache_is_per_key() {
        let c = CategoryCache::new();
        c.put_negative("a|b|c".to_string());
        assert!(c.is_negative("a|b|c"));
        assert!(!c.is_negative("x|y|z"));
    }
}
