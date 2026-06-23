use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::process::Command;
use tauri::{State, AppHandle, Manager, Emitter};
use uuid::Uuid;
use chrono::{Utc, Local, Timelike};
use reqwest::Client;

// 业务域模块（从原单文件 lib.rs 逐步拆分而来）
mod airport;
mod metrics;
mod engage;
mod multi_account;
mod nurture;
mod ai;
mod campaign;
mod posts;
// ai.rs 里的 provider 抽象层被本文件多处裸调用，统一在此引入，避免逐个调用点改路径
use crate::ai::{call_gemini_api, call_openai_api, call_deepseek_api, call_qwen_api,
    ai_provider_key, ai_complete, strip_code_fence};
// posts.rs 里被本文件复用的项（内容生成/引擎调度处裸调用），统一引入
use crate::posts::{GeneratedPost, detect_media_type, load_post, publish_post, post_schedule_tick};

// ============================================================================
// 反风控安全配置 - Anti-Detection Safety Settings
// ============================================================================

/// 全局安全配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SafetyConfig {
    // 无链接模式 - 不在内容中添加URL
    pub no_link_mode: bool,

    // 随机延迟范围（秒）
    pub min_delay_seconds: i32,
    pub max_delay_seconds: i32,

    // 每日操作上限（全局）
    pub max_daily_posts: i32,
    pub max_daily_replies: i32,
    pub max_daily_likes: i32,

    // 账号预热设置
    pub warmup_enabled: bool,
    pub warmup_days: i32,           // 预热期天数
    pub warmup_multiplier: f32,     // 预热期限制乘数 (0.1 = 10% of normal)

    // 内容变体设置
    pub content_variation: bool,    // 启用内容变体
    pub variation_level: i32,       // 变体程度 1-5

    // 行为模拟
    pub simulate_reading: bool,     // 模拟阅读（停留时间）
    pub simulate_scrolling: bool,   // 模拟滚动
    pub random_breaks: bool,        // 随机休息
}

impl Default for SafetyConfig {
    fn default() -> Self {
        SafetyConfig {
            no_link_mode: false,
            min_delay_seconds: 60,
            max_delay_seconds: 300,
            max_daily_posts: 10,
            max_daily_replies: 20,
            max_daily_likes: 50,
            warmup_enabled: true,
            warmup_days: 14,
            warmup_multiplier: 0.3,
            content_variation: true,
            variation_level: 3,
            simulate_reading: true,
            simulate_scrolling: true,
            random_breaks: true,
        }
    }
}

/// 平台专属安全限制
#[derive(Clone, Debug)]
pub struct PlatformSafetyLimits {
    pub platform: &'static str,
    pub max_daily_posts: i32,
    pub max_daily_replies: i32,
    pub max_daily_follows: i32,       // 每日关注限制
    pub max_daily_likes: i32,         // 每日点赞限制
    pub min_interval_minutes: i32,
    pub max_interval_minutes: i32,
    pub warmup_days: i32,
    pub warmup_daily_limit: i32,
    pub link_allowed: bool,           // 是否允许链接
    pub link_warning_level: i32,      // 链接风险等级 1-5
    pub mention_limit: i32,           // 每日@提及限制
    pub hashtag_limit: i32,           // 每条内容hashtag限制
}

/// 平台安全限制。注意：当前仅 `link_allowed` 字段被消费（用于生成内容时决定是否带链接，
/// 见 get_link_safety / 内容生成处）。此处的 `warmup_days` 等字段**并非生效的预热闸门值**——
/// 真正的预热判定：养号走 `nurture_strategies.warmup_days`（由 NURTURE_WARMUP_DAYS 播种），
/// 发帖走 `get_publish_config().warmup_days`。改预热天数请改那两处，别动这里的默认值。
fn get_platform_safety_limits(platform: &str) -> PlatformSafetyLimits {
    match platform.to_lowercase().as_str() {
        "twitter" | "x" => PlatformSafetyLimits {
            platform: "Twitter/X",
            max_daily_posts: 5,        // 保守
            max_daily_replies: 15,     // 减半
            max_daily_follows: 10,     // 关注敏感
            max_daily_likes: 15,       // 点赞也要控制
            min_interval_minutes: 30,  // 加长间隔
            max_interval_minutes: 90,
            warmup_days: 21,           // 延长预热期
            warmup_daily_limit: 2,     // 预热期更严格
            link_allowed: false,       // X 对链接非常敏感
            link_warning_level: 5,
            mention_limit: 3,          // 减少@
            hashtag_limit: 1,          // 减少hashtag
        },
        "reddit" => PlatformSafetyLimits {
            platform: "Reddit",
            max_daily_posts: 3,
            max_daily_replies: 15,
            max_daily_follows: 5,
            max_daily_likes: 15,       // 减少
            min_interval_minutes: 30,
            max_interval_minutes: 120,
            warmup_days: 30,
            warmup_daily_limit: 1,
            link_allowed: true,
            link_warning_level: 3,
            mention_limit: 3,
            hashtag_limit: 0,
        },
        "linkedin" => PlatformSafetyLimits {
            platform: "LinkedIn",
            max_daily_posts: 3,
            max_daily_replies: 20,
            max_daily_follows: 15,     // 减少
            max_daily_likes: 25,       // 减少
            min_interval_minutes: 60,
            max_interval_minutes: 240,
            warmup_days: 7,
            warmup_daily_limit: 1,
            link_allowed: true,
            link_warning_level: 2,
            mention_limit: 5,
            hashtag_limit: 3,
        },
        "zhihu" => PlatformSafetyLimits {
            platform: "知乎",
            max_daily_posts: 2,
            max_daily_replies: 15,
            max_daily_follows: 10,
            max_daily_likes: 15,       // 减少点赞
            min_interval_minutes: 30,
            max_interval_minutes: 120,
            warmup_days: 14,
            warmup_daily_limit: 1,
            link_allowed: false,
            link_warning_level: 5,
            mention_limit: 3,
            hashtag_limit: 3,
        },
        "xiaohongshu" | "redbook" => PlatformSafetyLimits {
            platform: "小红书",
            max_daily_posts: 2,
            max_daily_replies: 15,     // 进一步减少
            max_daily_follows: 10,     // 减少关注
            max_daily_likes: 20,       // 减少点赞
            min_interval_minutes: 30,
            max_interval_minutes: 120,
            warmup_days: 14,
            warmup_daily_limit: 1,
            link_allowed: false,
            link_warning_level: 5,
            mention_limit: 3,
            hashtag_limit: 5,
        },
        "weibo" => PlatformSafetyLimits {
            platform: "微博",
            max_daily_posts: 5,
            max_daily_replies: 20,     // 减少
            max_daily_follows: 15,     // 减少
            max_daily_likes: 25,       // 减少
            min_interval_minutes: 15,  // 加长
            max_interval_minutes: 60,
            warmup_days: 7,
            warmup_daily_limit: 2,
            link_allowed: true,
            link_warning_level: 2,
            mention_limit: 5,          // 减少
            hashtag_limit: 3,
        },
        "v2ex" => PlatformSafetyLimits {
            platform: "V2EX",
            max_daily_posts: 1,
            max_daily_replies: 10,
            max_daily_follows: 5,
            max_daily_likes: 10,       // 减少
            min_interval_minutes: 60,
            max_interval_minutes: 240,
            warmup_days: 30,
            warmup_daily_limit: 0,
            link_allowed: true,
            link_warning_level: 3,
            mention_limit: 2,
            hashtag_limit: 0,
        },
        _ => PlatformSafetyLimits {
            platform: "Default",
            max_daily_posts: 3,
            max_daily_replies: 10,
            max_daily_follows: 10,
            max_daily_likes: 15,       // 更保守的默认值
            min_interval_minutes: 30,
            max_interval_minutes: 120,
            warmup_days: 14,
            warmup_daily_limit: 1,
            link_allowed: true,
            link_warning_level: 3,
            mention_limit: 3,
            hashtag_limit: 2,
        },
    }
}

/// 生成随机延迟（毫秒）
fn get_random_delay(min_seconds: i32, max_seconds: i32) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    let range = (max_seconds - min_seconds) as u64;
    let delay_seconds = min_seconds as u64 + (seed % (range + 1));
    delay_seconds * 1000
}

/// 对内容进行变体处理，避免重复检测
fn vary_content(content: &str, variation_level: i32) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut result = content.to_string();
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;

    // 简单的伪随机函数
    let pseudo_random = |max: u64, offset: u64| -> u64 {
        ((seed.wrapping_add(offset)).wrapping_mul(1103515245).wrapping_add(12345)) % max
    };

    if variation_level >= 1 {
        // 随机替换一些标点
        let punctuation_variants = [
            ("!", "！"),
            ("?", "？"),
            (".", "。"),
            (",", "，"),
        ];

        for (i, (from, to)) in punctuation_variants.iter().enumerate() {
            if pseudo_random(10, i as u64) < 3 {
                result = result.replacen(from, to, 1);
            }
        }
    }

    if variation_level >= 2 {
        // 添加表情符号变体
        let emojis = ["✨", "🔥", "💡", "⭐", "🚀", "👍", "💪", "🎯"];
        if pseudo_random(10, 100) < 2 && !result.contains(|c: char| c.is_ascii_punctuation()) {
            let emoji_idx = pseudo_random(emojis.len() as u64, 200) as usize;
            let emoji = emojis[emoji_idx];
            result = format!("{} {}", result.trim(), emoji);
        }
    }

    if variation_level >= 3 {
        // 同义词替换（简单版本）
        let synonyms = [
            ("great", &["excellent", "awesome", "fantastic", "amazing"][..]),
            ("good", &["nice", "solid", "decent", "helpful"][..]),
            ("use", &["try", "check out", "utilize", "leverage"][..]),
            ("help", &["assist", "support", "aid", "benefit"][..]),
        ];

        for (i, (word, alternatives)) in synonyms.iter().enumerate() {
            if result.to_lowercase().contains(word) && pseudo_random(10, 300 + i as u64) < 4 {
                let alt_idx = pseudo_random(alternatives.len() as u64, 400 + i as u64) as usize;
                let replacement = alternatives[alt_idx];
                result = result.replace(word, replacement);
                break; // 只替换一个
            }
        }
    }

    result
}

/// 简单的伪随机布尔值
fn pseudo_random_bool(probability: f32) -> bool {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    let threshold = (probability * 100.0) as u64;
    (seed % 100) < threshold
}

// ============================================================================
// 熔断机制 - Circuit Breaker (遇错即停)
// ============================================================================

/// 错误类型分类
#[derive(Clone, Debug, PartialEq)]
pub enum ErrorSeverity {
    /// 可忽略的轻微错误（网络抖动等）
    Minor,
    /// 警告级别（需要记录但可继续）
    Warning,
    /// 严重错误（应立即停止当前平台操作）
    Severe,
    /// 致命错误（可能账号被封，停止所有操作）
    Fatal,
}

/// 熔断状态
#[derive(Clone, Debug)]
pub struct CircuitBreaker {
    /// 平台名称
    pub platform: String,
    /// 是否触发熔断
    pub tripped: bool,
    /// 连续错误次数
    pub consecutive_errors: i32,
    /// 最后一次错误时间
    pub last_error_time: Option<String>,
    /// 错误消息
    pub last_error_message: Option<String>,
    /// 熔断级别
    pub severity: ErrorSeverity,
    /// 冷却时间（分钟）
    pub cooldown_minutes: i32,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        CircuitBreaker {
            platform: String::new(),
            tripped: false,
            consecutive_errors: 0,
            last_error_time: None,
            last_error_message: None,
            severity: ErrorSeverity::Minor,
            cooldown_minutes: 60,
        }
    }
}

/// 检测错误严重程度
fn detect_error_severity(error_msg: &str) -> ErrorSeverity {
    let error_lower = error_msg.to_lowercase();

    // 致命错误关键词（账号可能被封）
    let fatal_keywords = [
        "suspended", "banned", "disabled", "locked", "restricted",
        "violation", "spam detected", "unusual activity",
        "被封", "封禁", "限制", "违规", "异常",
    ];

    // 严重错误关键词（应停止操作）
    let severe_keywords = [
        "rate limit", "too many requests", "quota exceeded",
        "forbidden", "unauthorized", "403", "429",
        "频率限制", "操作过快", "请稍后", "访问受限",
    ];

    // 警告关键词
    let warning_keywords = [
        "timeout", "temporarily", "retry", "busy",
        "超时", "繁忙", "重试",
    ];

    for keyword in fatal_keywords {
        if error_lower.contains(keyword) {
            return ErrorSeverity::Fatal;
        }
    }

    for keyword in severe_keywords {
        if error_lower.contains(keyword) {
            return ErrorSeverity::Severe;
        }
    }

    for keyword in warning_keywords {
        if error_lower.contains(keyword) {
            return ErrorSeverity::Warning;
        }
    }

    ErrorSeverity::Minor
}

/// 处理错误并决定是否触发熔断
fn handle_error_with_circuit_breaker(
    platform: &str,
    error_msg: &str,
    current_errors: i32,
) -> CircuitBreaker {
    let severity = detect_error_severity(error_msg);
    let now = Utc::now().to_rfc3339();

    let (tripped, cooldown) = match severity {
        ErrorSeverity::Fatal => (true, 1440),        // 24小时冷却
        ErrorSeverity::Severe => (true, 120),        // 2小时冷却
        ErrorSeverity::Warning => {
            // 警告级别：3次连续错误才触发
            (current_errors >= 2, 30)
        },
        ErrorSeverity::Minor => {
            // 轻微错误：5次连续错误才触发
            (current_errors >= 4, 10)
        },
    };

    CircuitBreaker {
        platform: platform.to_string(),
        tripped,
        consecutive_errors: current_errors + 1,
        last_error_time: Some(now),
        last_error_message: Some(error_msg.to_string()),
        severity,
        cooldown_minutes: cooldown,
    }
}

/// 检查熔断器是否允许操作
fn check_circuit_breaker_allows(conn: &Connection, platform: &str) -> Result<bool, String> {
    // 查询该平台的熔断状态
    let result: Result<(bool, String, i32), _> = conn.query_row(
        "SELECT tripped, last_error_time, cooldown_minutes FROM circuit_breakers WHERE platform = ?1",
        params![platform],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    );

    match result {
        Ok((tripped, last_error_time, cooldown_minutes)) => {
            if !tripped {
                return Ok(true);
            }

            // 检查是否已过冷却期
            if let Ok(error_time) = chrono::DateTime::parse_from_rfc3339(&last_error_time) {
                let now = Utc::now();
                let minutes_since_error = (now - error_time.with_timezone(&Utc)).num_minutes();
                if minutes_since_error >= cooldown_minutes as i64 {
                    // 冷却期已过，重置熔断器
                    let _ = conn.execute(
                        "UPDATE circuit_breakers SET tripped = 0, consecutive_errors = 0 WHERE platform = ?1",
                        params![platform]
                    );
                    return Ok(true);
                }

                log::warn!(
                    "Circuit breaker tripped for {}. Cooldown: {} minutes remaining.",
                    platform,
                    cooldown_minutes as i64 - minutes_since_error
                );
                return Ok(false);
            }

            Ok(false)
        },
        Err(_) => Ok(true), // 没有记录，允许操作
    }
}

/// 记录错误到熔断器
fn record_error_to_circuit_breaker(conn: &Connection, breaker: &CircuitBreaker) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO circuit_breakers (platform, tripped, consecutive_errors, last_error_time, last_error_message, severity, cooldown_minutes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            breaker.platform,
            breaker.tripped,
            breaker.consecutive_errors,
            breaker.last_error_time,
            breaker.last_error_message,
            format!("{:?}", breaker.severity),
            breaker.cooldown_minutes
        ]
    ).map_err(|e| e.to_string())?;

    if breaker.tripped {
        log::error!(
            "🛑 CIRCUIT BREAKER TRIPPED for {} - Severity: {:?} - Error: {} - Cooldown: {} minutes",
            breaker.platform,
            breaker.severity,
            breaker.last_error_message.as_deref().unwrap_or("unknown"),
            breaker.cooldown_minutes
        );
    }

    Ok(())
}

/// 重置成功后的熔断器状态
fn reset_circuit_breaker_on_success(conn: &Connection, platform: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE circuit_breakers SET consecutive_errors = 0 WHERE platform = ?1",
        params![platform]
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// ============================================================================
// Unzoo REST API Integration
// ============================================================================

pub(crate) const UNZOO_API_BASE: &str = "http://127.0.0.1:9399/api/v1";

// Auto-start Unzoo Browser when app launches
fn auto_start_unzoo_browser() -> Result<(), String> {
    log::info!("[BROWSER] Checking if Unzoo Browser is running...");

    // First check if the service is already connected
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let check_url = format!("{}/tabs", UNZOO_API_BASE);
    match client.get(&check_url).send() {
        Ok(resp) if resp.status().is_success() => {
            log::info!("[BROWSER] Unzoo Browser is already connected");
            return Ok(());
        }
        Ok(resp) => {
            let text = resp.text().unwrap_or_default();
            if !text.contains("Not connected") {
                log::info!("[BROWSER] Unzoo Browser is already connected");
                return Ok(());
            }
            log::info!("[BROWSER] Unzoo service running but browser not connected, launching...");
        }
        Err(_) => {
            log::info!("[BROWSER] Unzoo service not responding, launching browser...");
        }
    }

    // Try to launch Unzoo Browser
    let browser_paths = [
        r"C:\Program Files\Unzoo Browser\unzoo.exe",
        r"C:\Program Files (x86)\Unzoo Browser\unzoo.exe",
    ];

    for path in browser_paths {
        if std::path::Path::new(path).exists() {
            log::info!("[BROWSER] Launching Unzoo Browser from: {}", path);

            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let _ = std::process::Command::new(path)
                    .creation_flags(0x00000008) // DETACHED_PROCESS
                    .spawn();
            }

            #[cfg(not(windows))]
            {
                let _ = std::process::Command::new(path).spawn();
            }

            // Wait for browser to initialize
            std::thread::sleep(std::time::Duration::from_secs(5));

            // Verify connection
            for attempt in 1..=3 {
                log::info!("[BROWSER] Verifying connection (attempt {}/3)...", attempt);
                std::thread::sleep(std::time::Duration::from_secs(2));

                if let Ok(resp) = client.get(&check_url).send() {
                    if resp.status().is_success() {
                        let text = resp.text().unwrap_or_default();
                        if !text.contains("Not connected") {
                            log::info!("[BROWSER] Unzoo Browser connected successfully!");
                            return Ok(());
                        }
                    }
                }
            }

            log::warn!("[BROWSER] Browser launched but connection not verified");
            return Ok(()); // Still return Ok, browser may connect later
        }
    }

    Err("Unzoo Browser not found. Please install from https://unzoo.com".to_string())
}

// Global state for active tab
static ACTIVE_TAB_ID: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();

fn get_active_tab() -> Option<String> {
    ACTIVE_TAB_ID.get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn set_active_tab(tab_id: Option<String>) {
    if let Some(mutex) = ACTIVE_TAB_ID.get_or_init(|| std::sync::Mutex::new(None)).lock().ok().as_mut() {
        **mutex = tab_id;
    }
}

// Get saved browser profile from config (without State)
fn get_saved_browser_profile() -> String {
    let db_path = get_db_path();
    if let Ok(conn) = Connection::open(&db_path) {
        conn.query_row(
            "SELECT value FROM config WHERE key = 'selected_browser_profile'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default()
    } else {
        String::new()
    }
}

// Ensure browser is connected, launch a profile if needed
pub(crate) async fn ensure_browser_connected() -> Result<String, String> {
    // Check if we already have an active tab
    if let Some(tab_id) = get_active_tab() {
        // Verify the tab is still valid
        let client = get_http_client();
        let check_url = format!("{}/tabs", UNZOO_API_BASE);
        if let Ok(resp) = client.get(&check_url).timeout(std::time::Duration::from_secs(3)).send().await {
            if resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                if !text.contains("Not connected") {
                    log::info!("[BROWSER] Using existing tab: {}", tab_id);
                    return Ok(tab_id);
                }
            }
        }
        // Tab is no longer valid, clear it
        log::warn!("[BROWSER] Previous tab no longer valid, reconnecting...");
        set_active_tab(None);
    }

    log::info!("[BROWSER] No active tab, checking browser connection...");

    let client = get_http_client();

    // Try to list profiles - this will fail if browser not connected
    let profiles_url = format!("{}/profiles", UNZOO_API_BASE);
    let resp = client.get(&profiles_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    // Check if we need to restart the browser
    let need_restart = match &resp {
        Ok(r) => {
            if r.status().is_success() {
                false // Browser is connected
            } else {
                true // API error, may need restart
            }
        }
        Err(_) => true // Connection failed, need restart
    };

    if need_restart {
        log::warn!("[BROWSER] Browser not connected, attempting to start...");

        // Try to start the browser
        if let Err(e) = auto_start_unzoo_browser() {
            log::error!("[BROWSER] Failed to start browser: {}", e);
        }

        // Retry the connection
        let resp = client.get(&profiles_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| format!("Unzoo Browser not responding after restart: {}. Please open Unzoo Browser manually.", e))?;

        if !resp.status().is_success() {
            return Err("Unzoo Browser API error after restart. Please open Unzoo Browser manually.".to_string());
        }
    }

    let resp = client.get(&profiles_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("Unzoo Browser not running: {}", e))?;

    if !resp.status().is_success() {
        return Err("Unzoo Browser API error".to_string());
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse profiles: {}", e))?;

    let profiles = data.get("data")
        .and_then(|d| d.get("profiles"))
        .or_else(|| data.get("profiles"))
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();

    // Try to get the saved profile from config first
    let saved_profile = get_saved_browser_profile();

    let profile_name = if !saved_profile.is_empty() {
        log::info!("[BROWSER] Using saved profile: {}", saved_profile);
        saved_profile
    } else if profiles.is_empty() {
        log::info!("[BROWSER] No profiles found, using default profile...");
        "Default".to_string()
    } else {
        // Use path to extract profile name (e.g., "Profile_TestProfile1" -> "TestProfile1")
        let path = profiles[0].get("path")
            .and_then(|p| p.as_str())
            .unwrap_or("");

        // Extract profile name from path
        let profile_name = path.split('\\').last()
            .or_else(|| path.split('/').last())
            .unwrap_or("Default")
            .to_string();

        // Also try to get the display name
        let display_name = profiles[0].get("name")
            .and_then(|n| n.as_str())
            .unwrap_or(&profile_name);

        log::info!("[BROWSER] Found profile: {} ({})", display_name, profile_name);
        profile_name
    };

    log::info!("[BROWSER] Launching profile: {}", profile_name);

    // Launch the profile using tab/new with the profile
    // First, try to get the current tab
    let tab_url = format!("{}/tabs", UNZOO_API_BASE);
    let tab_resp = client.get(&tab_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    // Check if we already have an active tab
    if let Ok(resp) = tab_resp {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(tabs) = data.get("tabs").or_else(|| data.get("data").and_then(|d| d.get("tabs"))) {
                    if let Some(arr) = tabs.as_array() {
                        if !arr.is_empty() {
                            if let Some(tab_id) = arr[0].get("id").and_then(|id| id.as_str()) {
                                log::info!("[BROWSER] Found existing tab: {}", tab_id);
                                set_active_tab(Some(tab_id.to_string()));
                                return Ok(tab_id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // No existing tab, create a new one
    let new_tab_url = format!("{}/mcp/tools/call", UNZOO_API_BASE);
    let body = serde_json::json!({
        "name": "tab_create",
        "arguments": {
            "profile_id": profile_name,
            "url": "about:blank"
        }
    });

    log::info!("[BROWSER] Creating new tab...");

    let resp = client.post(&new_tab_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Failed to create tab: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Browser not ready: {} - {}. Please open Unzoo Browser first.", status, text));
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();

    // Try to extract tab_id from various response formats
    let tab_id = if let Some(id) = data.get("tab_id").and_then(|t| t.as_str()) {
        id.to_string()
    } else if let Some(id) = data.get("id").and_then(|t| t.as_str()) {
        id.to_string()
    } else if let Some(content) = data.get("content").and_then(|c| c.get(0)).and_then(|c| c.get("text")).and_then(|t| t.as_str()) {
        // MCP response format - parse the text as JSON
        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(content) {
            inner.get("tab_id").and_then(|t| t.as_str()).unwrap_or("default").to_string()
        } else {
            "default".to_string()
        }
    } else {
        "default".to_string()
    };

    log::info!("[BROWSER] Tab created, tab_id: {}", tab_id);

    // Save the active tab
    set_active_tab(Some(tab_id.clone()));

    // Wait for browser to fully initialize
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    Ok(tab_id)
}

// Unzoo API Response types
#[derive(Debug, Serialize, Deserialize)]
struct UnzooProfile {
    id: String,
    name: String,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    proxy: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UnzooProfileList {
    profiles: Vec<UnzooProfile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UnzooFingerprint {
    #[serde(default)]
    gpu: Option<String>,
    #[serde(default)]
    canvas: Option<String>,
    #[serde(default)]
    audio: Option<String>,
    #[serde(default)]
    webgl: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UnzooScheduledJob {
    id: String,
    name: String,
    cron: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    last_run: Option<String>,
    #[serde(default)]
    next_run: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UnzooCookie {
    name: String,
    value: String,
    domain: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    secure: bool,
    #[serde(default, rename = "httpOnly")]
    http_only: bool,
    #[serde(default)]
    expires: Option<f64>,
}

// Frontend-facing types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserProfile {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub account_id: Option<String>,
    pub fingerprint_id: Option<String>,
    pub proxy: Option<String>,
    pub stealth_enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileCreateRequest {
    pub name: String,
    pub platform: String,
    pub account_id: Option<String>,
    pub proxy: Option<String>,
}

// Platform registration configs
struct PlatformConfig {
    name: &'static str,
    login_url: &'static str,
    register_url: &'static str,
    google_oauth: bool,
    google_button_selector: Option<&'static str>,
}

/// 平台分类元数据：场景(scene) / 地区(region) / 开通模式(mode)。
/// 单一事实来源；前端「开通账号」选择器据此分组与渲染。
/// GitHub 养号领域分类：key 唯一，topics 为真实高 population 的 GitHub topic。
/// 单一来源，前端经 gh_domains_catalog 命令拉取渲染多选框。
pub struct GhDomain {
    pub key: &'static str,
    pub label: &'static str,
    pub topics: &'static [&'static str],
}

const GH_DOMAINS: &[GhDomain] = &[
    GhDomain { key: "frontend",  label: "前端",            topics: &["frontend","react","vue","angular","typescript","nextjs","tailwindcss"] },
    GhDomain { key: "backend",   label: "后端",            topics: &["backend","api","nodejs","golang","spring-boot","microservices","graphql"] },
    GhDomain { key: "ml",        label: "AI/机器学习",     topics: &["machine-learning","deep-learning","pytorch","tensorflow","nlp","computer-vision","generative-ai"] },
    GhDomain { key: "ai_coding", label: "AI 编程/Agent/Skills", topics: &["claude","claude-code","mcp","model-context-protocol","ai-agents","langchain","rag","prompt-engineering","github-copilot","cursor","coding-assistant"] },
    GhDomain { key: "data",      label: "数据工程",        topics: &["data-science","data-engineering","data-analysis","apache-spark","etl","pandas"] },
    GhDomain { key: "devops",    label: "DevOps/云原生",   topics: &["devops","kubernetes","docker","terraform","ansible","cloud-native","observability"] },
    GhDomain { key: "mobile",    label: "移动开发",        topics: &["android","ios","flutter","react-native","swift","kotlin","jetpack-compose"] },
    GhDomain { key: "security",  label: "安全",            topics: &["security","cybersecurity","penetration-testing","cryptography","ethical-hacking","infosec"] },
    GhDomain { key: "web3",      label: "区块链/Web3",     topics: &["blockchain","ethereum","solidity","web3","smart-contracts","defi"] },
    GhDomain { key: "gamedev",   label: "游戏开发",        topics: &["gamedev","unity","godot","unreal-engine","game-engine"] },
    GhDomain { key: "database",  label: "数据库",          topics: &["database","postgresql","mysql","redis","mongodb","sqlite"] },
    GhDomain { key: "embedded",  label: "嵌入式/IoT",      topics: &["embedded","iot","arduino","raspberry-pi","esp32","microcontroller"] },
    GhDomain { key: "devtools",  label: "开发工具/效率",   topics: &["cli","developer-tools","vscode","neovim","terminal","automation"] },
];

/// 收集所选领域 key 对应的全部 topic（按出现顺序去重，未知 key 跳过）。
fn gh_domain_topics(keys: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for k in keys {
        if let Some(d) = GH_DOMAINS.iter().find(|d| d.key == *k) {
            for t in d.topics {
                if !out.contains(t) { out.push(t); }
            }
        }
    }
    out
}

/// X(Twitter) 养号方向：X 消费端 Topics 的 16 个官方顶层方向。label 双语展示。
pub struct XNiche {
    pub key: &'static str,
    pub label: &'static str,          // "English(中文)"
    pub keywords: &'static [&'static str],
}

const X_NICHES: &[XNiche] = &[
    XNiche { key: "technology",       label: "Technology(科技)",            keywords: &["technology","tech","AI","software","gadgets","innovation","programming","coding","open source","cybersecurity","cloud computing","machine learning","robotics","#tech","#AI","#technology","semiconductors","developer tools","#coding"] },
    XNiche { key: "business_finance", label: "Business & finance(商业财经)", keywords: &["business","finance","investing","stocks","markets","economy","entrepreneurship","venture capital","crypto","trading","fintech","#investing","#finance","#business","startup funding","earnings","real estate","personal finance"] },
    XNiche { key: "science",          label: "Science(科学)",               keywords: &["science","research","physics","biology","space","astronomy","climate","neuroscience","chemistry","#science","genetics","biotech","NASA","quantum physics","scientific discovery","#space","evolution"] },
    XNiche { key: "careers",          label: "Careers(职业)",               keywords: &["careers","jobs","hiring","remote work","job search","career advice","#hiring","leadership","productivity","networking","resume tips","interview tips","future of work","upskilling","#careers","layoffs","work culture"] },
    XNiche { key: "gaming",           label: "Gaming(游戏)",                keywords: &["gaming","games","#gaming","esports","indie games","game dev","PC gaming","console gaming","Nintendo","PlayStation","Xbox","Steam","RPG","FPS","game release","#gamedev","retro gaming","#indiedev"] },
    XNiche { key: "news",             label: "News(新闻)",                  keywords: &["news","breaking news","world news","politics","current events","headlines","#news","journalism","geopolitics","elections","#breakingnews","top stories","analysis"] },
    XNiche { key: "entertainment",    label: "Entertainment(娱乐)",         keywords: &["entertainment","celebrity","pop culture","#entertainment","awards","streaming","viral","trending","showbiz","red carpet","reality tv","#celebrity"] },
    XNiche { key: "arts_culture",     label: "Arts & culture(艺术文化)",    keywords: &["art","culture","design","photography","painting","museums","#art","creativity","digital art","illustration","architecture","contemporary art","#design","AI art","sculpture","graphic design"] },
    XNiche { key: "music",            label: "Music(音乐)",                 keywords: &["music","#music","new music","hip hop","pop music","rock","indie music","music production","vinyl","concerts","album","#newmusic","EDM","k-pop","songwriting","live music"] },
    XNiche { key: "movies_tv",        label: "Movies & TV(影视)",           keywords: &["movies","film","TV","cinema","#movies","streaming","trailers","box office","series","documentary","#film","TV shows","Oscars","film review","#cinema"] },
    XNiche { key: "sports",           label: "Sports(体育)",                keywords: &["sports","#sports","football","soccer","basketball","NBA","NFL","tennis","baseball","F1","olympics","#football","athletics","transfer news","match highlights","#NBA"] },
    XNiche { key: "fashion_beauty",   label: "Fashion & beauty(时尚美妆)",  keywords: &["fashion","beauty","style","makeup","skincare","#fashion","streetwear","outfit","beauty tips","fashion week","#beauty","trends","cosmetics","#OOTD","haircare"] },
    XNiche { key: "food",             label: "Food(美食)",                  keywords: &["food","cooking","recipes","foodie","#food","restaurant","baking","chef","#foodie","home cooking","cuisine","street food","dessert","healthy eating","#recipe"] },
    XNiche { key: "travel",           label: "Travel(旅行)",                keywords: &["travel","#travel","wanderlust","travel tips","destinations","backpacking","#travelphotography","vacation","adventure","solo travel","road trip","hidden gems","#travelgram","digital nomad"] },
    XNiche { key: "outdoors",         label: "Outdoors(户外)",              keywords: &["outdoors","hiking","camping","nature","#outdoors","backpacking","climbing","national parks","wildlife","#hiking","fishing","mountains","trail running","kayaking","#nature"] },
    XNiche { key: "hobbies",          label: "Hobbies & interests(兴趣爱好)", keywords: &["hobbies","DIY","crafts","woodworking","gardening","#DIY","collecting","model building","knitting","photography","#crafts","maker","3D printing","calligraphy","origami"] },
];

/// 收集所选方向 key 对应的全部关键词（按出现顺序去重，未知 key 跳过）。
fn x_niche_keywords(keys: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for k in keys {
        if let Some(n) = X_NICHES.iter().find(|n| n.key == *k) {
            for w in n.keywords {
                if !out.contains(w) { out.push(w); }
            }
        }
    }
    out
}

/// SegmentFault（思否）养号领域：key 唯一，keywords 为思否站内搜索用的中文/技术词。
/// 养号时按所选领域随机取词，走 https://segmentfault.com/search?q=<词> 搜索→浏览→读文章/问题。
/// 与 GH_DOMAINS 同构（前端经 sf_domains_catalog 渲染多选框，存 accounts.sf_domains）。
pub struct SfDomain {
    pub key: &'static str,
    pub label: &'static str,
    pub keywords: &'static [&'static str],
}

const SF_DOMAINS: &[SfDomain] = &[
    SfDomain { key: "frontend",  label: "前端",            keywords: &["前端","React","Vue","TypeScript","Webpack","Vite","CSS","浏览器渲染","前端性能优化","Next.js"] },
    SfDomain { key: "backend",   label: "后端",            keywords: &["后端","Java","Spring Boot","Node.js","Go","微服务","API 设计","高并发","分布式系统"] },
    SfDomain { key: "ml",        label: "AI/机器学习",     keywords: &["机器学习","深度学习","PyTorch","TensorFlow","大模型","NLP","计算机视觉"] },
    SfDomain { key: "ai_coding", label: "AI 编程/Agent",   keywords: &["大模型应用","LangChain","RAG","AI Agent","Prompt 工程","Copilot","Cursor"] },
    SfDomain { key: "data",      label: "数据工程",        keywords: &["数据分析","数据仓库","Spark","Flink","ETL","Pandas","数据可视化"] },
    SfDomain { key: "devops",    label: "DevOps/云原生",   keywords: &["Docker","Kubernetes","CI/CD","云原生","Linux","Nginx","运维","可观测性"] },
    SfDomain { key: "mobile",    label: "移动开发",        keywords: &["Android","iOS","Flutter","React Native","Kotlin","Swift","小程序"] },
    SfDomain { key: "security",  label: "安全",            keywords: &["网络安全","渗透测试","加密算法","漏洞","JWT","OAuth","XSS","SQL 注入"] },
    SfDomain { key: "web3",      label: "区块链/Web3",     keywords: &["区块链","以太坊","智能合约","Solidity","Web3"] },
    SfDomain { key: "database",  label: "数据库",          keywords: &["MySQL","PostgreSQL","Redis","MongoDB","数据库优化","索引优化","SQL"] },
    SfDomain { key: "devtools",  label: "开发工具/效率",   keywords: &["Git","VSCode","命令行","自动化脚本","效率工具","正则表达式"] },
    SfDomain { key: "language",  label: "编程语言/基础",   keywords: &["算法","设计模式","数据结构","Rust","Python","JavaScript","并发编程"] },
];

/// 收集所选领域 key 对应的全部关键词（按出现顺序去重，未知 key 跳过）。
fn sf_domain_keywords(keys: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for k in keys {
        if let Some(d) = SF_DOMAINS.iter().find(|d| d.key == *k) {
            for w in d.keywords {
                if !out.contains(w) { out.push(w); }
            }
        }
    }
    out
}

/// 按号龄分期返回当日 X 配额：(likes, follows, engages)。engages = 转推+回复预算。
/// X 反自动化最严 → 量级极保守、关注最克制。
fn x_daily_quota(phase: &str) -> (i64, i64, i64) {
    match phase {
        "growth" => (5, 2, 2),
        "mature" => (5, 1, 3),
        "warmup" => (2, 0, 0), // 仅点赞
        _ => (1, 0, 0),
    }
}

/// 极少量原创是否解锁：号龄走完预热期（≥ warmup_days）且已有 L1 历史（点赞/关注）。
/// 跟随养号周期缩放——用户把周期调短，发原创门槛同步前移。频率上限由调用方按周限。
fn x_l3_allowed(age_days: i64, warmup_days: i64, l1_action_count: i64) -> bool {
    age_days >= warmup_days && l1_action_count > 0
}

/// 良性、不带推广意图的短推文（仅填充时间线、绝不带链接/产品）。
fn x_benign_tweet(seed: u64) -> String {
    const POOL: &[&str] = &[
        "Spent the morning reading through some great threads here. Always learning.",
        "Underrated: taking notes by hand. Retains so much better.",
        "Quiet productive day. Small steps add up.",
        "Coffee + a clear todo list might be the real productivity hack.",
        "Good docs are worth more than clever code. Change my mind.",
        "Weekend plan: less screen, more walks.",
    ];
    POOL[(seed as usize) % POOL.len()].to_string()
}

/// 按页面文本分类 X 账号健康风险。banned(已封) | locked(锁定需验证) | restricted(只读/限流/受限) | None。
/// 兼容英文 / 简体 / 繁体界面（X UI 语言随账号设置而变）。
fn x_classify_health(text: &str) -> Option<&'static str> {
    let t = text.to_lowercase(); // 英文匹配用小写；中文不受影响
    // 已封（冻结/停用/停权）
    if t.contains("account is suspended") || t.contains("account suspended")
        || t.contains("your account is suspended")
        || text.contains("账号已被冻结") || text.contains("帐号已被冻结") || text.contains("帳號已被凍結")
        || text.contains("已被停用") || text.contains("已被停权") || text.contains("已被停權") {
        return Some("banned");
    }
    // 锁定（需验证）
    if t.contains("your account has been locked") || t.contains("account has been locked")
        || t.contains("verify your identity") || t.contains("we've detected unusual")
        || t.contains("help us confirm") || t.contains("solve this puzzle")
        || text.contains("账号已被锁定") || text.contains("帐户已被锁定") || text.contains("帳號已被鎖定")
        || text.contains("验证你的身份") || text.contains("验证您的身份") || text.contains("驗證你的身份")
        || text.contains("检测到异常活动") || text.contains("偵測到異常") {
        return Some("locked");
    }
    // 只读 / 限流 / 受限
    if t.contains("unable to perform this action") || t.contains("you are unable to")
        || t.contains("over the daily limit") || t.contains("reached your daily limit")
        || t.contains("rate limit") || t.contains("try again later")
        || t.contains("temporarily restricted")
        || text.contains("无法执行此操作") || text.contains("無法執行此動作")
        || text.contains("已达到当日上限") || text.contains("已達到每日上限")
        || text.contains("操作频率过高") || text.contains("请稍后再试") || text.contains("請稍後再試")
        || text.contains("暂时受限") || text.contains("暫時受限") {
        return Some("restricted");
    }
    None
}

/// 从形如 "1,234 Followers" / "1.2K Followers" / "3.4M" 解析粉丝数。读不出返回 None。
fn x_parse_count(s: &str) -> Option<i64> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let rest = &s[start..];
    // 数字部分（含千分位逗号/小数点），全是 ASCII，token.len() 即字节数。
    let token: String = rest.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    let num: String = token.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let base = num.parse::<f64>().ok()?;
    // 紧跟数字的倍率后缀：K/M/B 或 中文 万/亿（X 中文环境显示如「11.8万」）。
    let mult = match rest[token.len()..].trim_start().chars().next() {
        Some('K') | Some('k') => 1_000.0,
        Some('M') | Some('m') => 1_000_000.0,
        Some('B') | Some('b') => 1_000_000_000.0,
        Some('万') => 10_000.0,
        Some('亿') => 100_000_000.0,
        _ => 1.0,
    };
    Some((base * mult) as i64)
}

/// 按号龄分期返回当日 GitHub L1 配额：(stars, follows, watches)。
fn gh_daily_quota(phase: &str) -> (i64, i64, i64) {
    match phase {
        "growth" => (3, 2, 1),
        "mature" => (2, 1, 1),
        _ => (1, 0, 0), // warmup / 未知：极轻
    }
}

/// L2（评论）是否解锁：非 warmup、号龄≥7 天、且已有 L1 历史。
fn gh_l2_allowed(age_days: i64, phase: &str, l1_action_count: i64) -> bool {
    phase != "warmup" && age_days >= 7 && l1_action_count > 0
}

/// 从候选里去掉已操作项，用确定性洗牌（seed）挑至多 n 个。
/// seed 由调用方用时间派生（脚本不可用 rand，引擎里用 get_random_delay 同源时间种子）。
fn gh_pick_targets(candidates: &[String], already: &std::collections::HashSet<String>, n: usize, seed: u64) -> Vec<String> {
    let mut pool: Vec<String> = candidates.iter().filter(|c| !already.contains(*c)).cloned().collect();
    // 简单确定性洗牌（xorshift）：避免引入 rand 依赖，且 seed 相同结果稳定可测。
    let mut s = seed | 1;
    for i in (1..pool.len()).rev() {
        s ^= s << 13; s ^= s >> 7; s ^= s << 17;
        let j = (s as usize) % (i + 1);
        pool.swap(i, j);
    }
    pool.truncate(n);
    pool
}

/// 该账号是否已对某 target 执行过任何 GitHub 动作。
fn gh_already_acted(conn: &Connection, account_id: &str, target: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM gh_actions_log WHERE account_id=?1 AND target=?2 LIMIT 1",
        params![account_id, target], |_| Ok(true)).unwrap_or(false)
}

/// 记一条 GitHub 动作。
fn gh_record_action(conn: &Connection, account_id: &str, action_type: &str, target: &str) -> Result<(), String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    conn.execute(
        "INSERT INTO gh_actions_log (id, account_id, action_type, target, date) VALUES (?1,?2,?3,?4,?5)",
        params![Uuid::new_v4().to_string(), account_id, action_type, target, today])
        .map(|_| ()).map_err(|e| e.to_string())
}

/// 有多少个不同账号触碰过该 target（跨账号去同质化用）。
fn gh_target_persona_count(conn: &Connection, target: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(DISTINCT account_id) FROM gh_actions_log WHERE target=?1",
        params![target], |r| r.get(0)).unwrap_or(0)
}

/// 读账号所选领域（gh_domains JSON 数组），解析失败/空 → 空 vec。
fn account_gh_domains(conn: &Connection, account_id: &str) -> Vec<String> {
    let raw: Option<String> = conn.query_row(
        "SELECT gh_domains FROM accounts WHERE id=?1",
        params![account_id], |r| r.get(0)).ok().flatten();
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default()
}

// ===== X 养号 DB 助手（与 gh_* 同构，作用于 x_actions_log）=====

fn x_already_acted(conn: &Connection, account_id: &str, target: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM x_actions_log WHERE account_id=?1 AND target=?2 LIMIT 1",
        params![account_id, target], |_| Ok(true)).unwrap_or(false)
}

fn x_record_action(conn: &Connection, account_id: &str, action_type: &str, target: &str) -> Result<(), String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    conn.execute(
        "INSERT INTO x_actions_log (id, account_id, action_type, target, date) VALUES (?1,?2,?3,?4,?5)",
        params![Uuid::new_v4().to_string(), account_id, action_type, target, today])
        .map(|_| ()).map_err(|e| e.to_string())
}

fn x_target_persona_count(conn: &Connection, target: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(DISTINCT account_id) FROM x_actions_log WHERE target=?1",
        params![target], |r| r.get(0)).unwrap_or(0)
}

fn account_x_niches(conn: &Connection, account_id: &str) -> Vec<String> {
    let raw: Option<String> = conn.query_row(
        "SELECT x_niches FROM accounts WHERE id=?1",
        params![account_id], |r| r.get(0)).ok().flatten();
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default()
}

/// 读账号所选 SegmentFault 领域（sf_domains JSON 数组），解析失败/空 → 空 vec。
fn account_sf_domains(conn: &Connection, account_id: &str) -> Vec<String> {
    let raw: Option<String> = conn.query_row(
        "SELECT sf_domains FROM accounts WHERE id=?1",
        params![account_id], |r| r.get(0)).ok().flatten();
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default()
}

#[derive(serde::Serialize)]
pub struct GhDomainItem { pub key: String, pub label: String, pub topics: Vec<String> }

/// 给前端渲染领域多选框。
#[tauri::command]
fn gh_domains_catalog() -> Vec<GhDomainItem> {
    GH_DOMAINS.iter().map(|d| GhDomainItem {
        key: d.key.to_string(),
        label: d.label.to_string(),
        topics: d.topics.iter().map(|t| t.to_string()).collect(),
    }).collect()
}

/// 读取某账号已选领域（供前端打开多选框时回勾）。
#[tauri::command]
fn get_account_gh_domains(state: State<AppState>, account_id: String) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(account_gh_domains(&conn, &account_id))
}

/// 保存某账号所选领域（仅保留合法 key）。
#[tauri::command]
fn set_account_gh_domains(state: State<AppState>, account_id: String, domains: Vec<String>) -> Result<(), String> {
    let valid: Vec<String> = domains.into_iter()
        .filter(|k| GH_DOMAINS.iter().any(|d| d.key == k))
        .collect();
    let json = serde_json::to_string(&valid).map_err(|e| e.to_string())?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE accounts SET gh_domains=?1 WHERE id=?2", params![json, account_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct XNicheItem { pub key: String, pub label: String, pub keywords: Vec<String> }

/// 给前端渲染 X 方向多选框（16 官方方向，双语 label）。
#[tauri::command]
fn x_niches_catalog() -> Vec<XNicheItem> {
    X_NICHES.iter().map(|n| XNicheItem {
        key: n.key.to_string(),
        label: n.label.to_string(),
        keywords: n.keywords.iter().map(|w| w.to_string()).collect(),
    }).collect()
}

/// 读取某账号已选 X 方向（供前端回勾）。
#[tauri::command]
fn get_account_x_niches(state: State<AppState>, account_id: String) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(account_x_niches(&conn, &account_id))
}

/// 保存某账号所选 X 方向（仅保留合法 key）。
#[tauri::command]
fn set_account_x_niches(state: State<AppState>, account_id: String, niches: Vec<String>) -> Result<(), String> {
    let valid: Vec<String> = niches.into_iter()
        .filter(|k| X_NICHES.iter().any(|n| n.key == k))
        .collect();
    let json = serde_json::to_string(&valid).map_err(|e| e.to_string())?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE accounts SET x_niches=?1 WHERE id=?2", params![json, account_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct SfDomainItem { pub key: String, pub label: String, pub keywords: Vec<String> }

/// 给前端渲染 SegmentFault 领域多选框。
#[tauri::command]
fn sf_domains_catalog() -> Vec<SfDomainItem> {
    SF_DOMAINS.iter().map(|d| SfDomainItem {
        key: d.key.to_string(),
        label: d.label.to_string(),
        keywords: d.keywords.iter().map(|w| w.to_string()).collect(),
    }).collect()
}

/// 读取某账号已选 SegmentFault 领域（供前端回勾）。
#[tauri::command]
fn get_account_sf_domains(state: State<AppState>, account_id: String) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(account_sf_domains(&conn, &account_id))
}

/// 保存某账号所选 SegmentFault 领域（仅保留合法 key）。
#[tauri::command]
fn set_account_sf_domains(state: State<AppState>, account_id: String, domains: Vec<String>) -> Result<(), String> {
    let valid: Vec<String> = domains.into_iter()
        .filter(|k| SF_DOMAINS.iter().any(|d| d.key == k))
        .collect();
    let json = serde_json::to_string(&valid).map_err(|e| e.to_string())?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE accounts SET sf_domains=?1 WHERE id=?2", params![json, account_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 平台 → 场景类别 key（research/product/social/content/career/lifestyle）。
/// 供前端在已开通账号卡片上展示场景类别徽章（与开通选择器同一套分类）。
#[tauri::command]
fn platform_scenes() -> std::collections::HashMap<String, String> {
    PLATFORM_KEYS.iter().filter_map(|&p| {
        platform_meta(p).map(|m| (p.to_string(), m.scene.to_string()))
    }).collect()
}

/// account_id → 已选养号方向/领域 key 列表（github 取 gh_domains，twitter/x 取 x_niches）。
/// 一次性批量返回，供前端在账号卡片展示「养号方向」标签。
#[tauri::command]
fn account_niches(state: State<AppState>) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, platform, gh_domains, x_niches, sf_domains FROM accounts").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok((
        r.get::<_, String>(0)?, r.get::<_, String>(1)?,
        r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?,
        r.get::<_, Option<String>>(4)?,
    ))).map_err(|e| e.to_string())?;
    let mut out = std::collections::HashMap::new();
    for row in rows.flatten() {
        let (id, platform, gh, x, sf) = row;
        let raw = match platform.to_lowercase().as_str() {
            "github" => gh,
            "twitter" | "x" => x,
            "segmentfault" => sf,
            _ => None,
        };
        if let Some(keys) = raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()) {
            if !keys.is_empty() { out.insert(id, keys); }
        }
    }
    Ok(out)
}

#[derive(Clone, Copy)]
struct PlatformMeta {
    scene: &'static str,   // research|product|social|content|career|lifestyle
    region: &'static str,  // us|jp|kr|ru|cn|global
    mode: &'static str,    // auto(🟢 Google自动) | manual(🟡 开登录页手动)
}

/// 可在「开通账号」里分类展示的全部平台 key（28 个）。
const PLATFORM_KEYS: &[&str] = &[
    "github", "devto", "hashnode", "hackernews", "qiita", "zenn", "habr", "segmentfault", "csdn", "oschina",
    "producthunt", "betalist", "alternativeto", "indiehackers",
    "twitter", "reddit", "facebook", "telegram", "weibo", "jike", "vk",
    "medium", "note", "zhihu", "naver_blog",
    "linkedin",
    "xiaohongshu", "sspai",
];

/// 平台 → 场景/地区/开通模式。mode=auto 当且仅当支持 Google 登录且非敌意平台(X/Reddit)。
fn platform_meta(platform: &str) -> Option<PlatformMeta> {
    let (scene, region, mode) = match platform {
        "github" => ("research", "us", "auto"),
        "devto" => ("research", "us", "auto"),
        "hashnode" => ("research", "us", "auto"),
        "hackernews" => ("research", "us", "manual"),
        "qiita" => ("research", "jp", "auto"),
        "zenn" => ("research", "jp", "auto"),
        "habr" => ("research", "ru", "auto"),
        "v2ex" => ("research", "cn", "auto"),
        "segmentfault" => ("research", "cn", "auto"),
        "csdn" => ("research", "cn", "manual"),
        "oschina" => ("research", "cn", "manual"),
        "producthunt" => ("product", "us", "auto"),
        "betalist" => ("product", "us", "auto"),
        "alternativeto" => ("product", "us", "auto"),
        "indiehackers" => ("product", "us", "auto"),
        "twitter" | "x" => ("social", "us", "manual"),
        "reddit" => ("social", "us", "manual"),
        "facebook" => ("social", "us", "manual"),
        "telegram" => ("social", "global", "manual"),
        "weibo" => ("social", "cn", "manual"),
        "jike" | "okjike" => ("social", "cn", "manual"),
        "vk" => ("social", "ru", "manual"),
        "medium" => ("content", "us", "auto"),
        "note" | "note_japan" => ("content", "jp", "auto"),
        "zhihu" => ("content", "cn", "manual"),
        "naver_blog" => ("content", "kr", "manual"),
        "linkedin" => ("career", "us", "auto"),
        "xiaohongshu" | "redbook" => ("lifestyle", "cn", "manual"),
        "sspai" => ("lifestyle", "cn", "manual"),
        _ => return None,
    };
    Some(PlatformMeta { scene, region, mode })
}

#[derive(Serialize)]
struct PlatformCatalogItem {
    platform: String,
    name: String,
    scene: String,
    region: String,
    mode: String,
    /// 登录方式：google(Gmail/Google 一键) | phone(手机号验证码) | password(账号密码)。
    /// 用于区分开通/登录流程：google 可自动；phone/password 多需手动。
    login_method: String,
    /// #13 IP 策略：shared_overseas(海外机场共享可) | residential_cn(需国内住宅固定) | static_overseas(需海外固定)。
    /// 用于把平台路由到对应「IP 来源类型」的身份。
    ip_policy: String,
    provisioned: bool,
}

/// 平台登录方式标注。优先 google(凡支持 Google OAuth 者)；其余按手机号 / 账号密码区分。
/// 单一事实来源：google 由 PlatformConfig.google_oauth 推导，phone 为手机验证码主导平台。
fn platform_login_method(platform: &str, google_oauth: bool) -> &'static str {
    if google_oauth {
        return "google";
    }
    match platform.to_lowercase().as_str() {
        // 手机号 + 短信验证码为主的平台
        "zhihu" | "weibo" | "sspai" | "jike" | "okjike" | "xiaohongshu" | "redbook" | "csdn"
        | "telegram" => "phone",
        // 其余以账号密码登录（hackernews / facebook / oschina / naver_blog / vk 等）
        _ => "password",
    }
}

/// #13 平台 IP 策略（按各平台真实风控严格度逐个标注，单一事实来源）：
/// - residential_cn：国内社交/生活/内容，风控严、查异地登录、IDC IP 被标记 → 需国内住宅/4G 固定 IP、不可轮换
/// - static_overseas：海外高价值账号、对数据中心 IP 与 IP 轮换敏感 → 需稳定独享固定 IP、不可轮换
/// - shared_overseas：海外开发者/产品/内容社区 + Google 登录的国内开发社区 → 对 IP 宽松，机场共享轮换即可
fn platform_ip_policy(platform: &str) -> &'static str {
    match platform.to_lowercase().as_str() {
        // 国内固定 IP：只留「确定需要」国内住宅/4G 固定 IP 的强风控平台（小红书、微博）
        "weibo" | "xiaohongshu" | "redbook" => "residential_cn",
        // 国外固定 IP
        "reddit" | "linkedin" | "facebook" | "vk" | "naver_blog" => "static_overseas",
        // X：数据中心/VPN 级 IP 即可，per-persona 专属机场节点（区域稳定）足够，无需国外固定专线
        "twitter" | "x" => "shared_overseas",
        // 其余：机场共享轮换即可。含较宽松的国内站（知乎/即刻/CSDN/少数派/开源中国——
        //   手机号登录≠IP严，挂 Gmail 身份、走机场 IP 即可），以及海外开发者/产品/内容社区。
        _ => "shared_overseas",
    }
}

/// 各平台养号预热天数（按真实风控严格度 / 账号养成需求差异化）。
/// 越严/越吃账号年龄与声望 → 养越久；纯内容/代码托管/消息类 → 养越短。
/// 用于初始化 nurture_strategies.warmup_days（替代原来一刀切 14 天）。
const NURTURE_WARMUP_DAYS: &[(&str, i64)] = &[
    // —— 强风控 / 需积累声望年龄（30 天）——
    ("reddit", 30),        // karma + 账号年龄门槛、新号易 shadowban
    ("hackernews", 30),    // 多数功能要先攒 karma
    ("facebook", 30),      // 新号 checkpoint 极严
    ("instagram", 30),     // 极严
    ("xiaohongshu", 30),   // 极严，需慢养拟真
    ("v2ex", 30),          // 部分节点要金币/账号年龄才能发
    // —— 较严（21 天）——
    ("twitter", 5), ("x", 5),       // 默认预热 5 天（成长时长 NULL→兜底 = 5，成熟从第 10 天起）；用户可在设置里自定义
    ("linkedin", 21),      // 职场，限制新号
    ("tiktok", 21),
    ("weibo", 21),         // 较严
    // —— 中等（14 天）——
    ("habr", 14),          // 需 karma / 历史邀请制
    ("vk", 14),
    ("naver_blog", 14),
    ("youtube", 14),
    ("zhihu", 14),
    ("jike", 14), ("okjike", 14),
    // —— 宽松内容/产品社区（7 天）——
    ("medium", 7), ("producthunt", 7), ("qiita", 7), ("zenn", 7),
    ("note", 7), ("note_japan", 7), ("betalist", 7), ("alternativeto", 7),
    ("indiehackers", 7), ("sspai", 7),
    // SegmentFault：预热 3 天 + 成长 7 天（成长期 growth_days 见下方一次性 seed），总周期 10 天。
    // 思否风控偏内容质量/声望而非账号年龄，不需长预热；攒声望靠成长期持续高质量回答。
    ("segmentfault", 3),
    // —— 几乎无需养（3 天）：代码托管 / 消息 / 宽松技术站 ——
    ("github", 3), ("devto", 3), ("hashnode", 3),
    ("telegram", 3), ("csdn", 3), ("oschina", 3),
];

/// 返回某身份的平台开通目录：全部 29 个平台 + 该身份是否已开通(登录态 healthy)。
/// 供前端「开通账号」选择器分组渲染、决定哪些预选锁定。
#[tauri::command]
fn persona_platform_catalog(state: State<AppState>, persona_id: String) -> Result<Vec<PlatformCatalogItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(PLATFORM_KEYS.len());
    for &p in PLATFORM_KEYS {
        let meta = match platform_meta(p) { Some(m) => m, None => continue };
        let cfg = get_platform_config(p);
        let name = cfg.as_ref().map(|c| c.name.to_string()).unwrap_or_else(|| p.to_string());
        let google_oauth = cfg.as_ref().map(|c| c.google_oauth).unwrap_or(false);
        let login_method = platform_login_method(p, google_oauth).to_string();
        let ip_policy = platform_ip_policy(p).to_string();
        // 已开通 = 该身份下已存在这个平台的账号行（不要求 health_status，用户选定）
        let provisioned: bool = conn.query_row(
            "SELECT 1 FROM accounts WHERE persona_id=?1 AND platform=?2 LIMIT 1",
            params![persona_id, p],
            |_| Ok(true),
        ).unwrap_or(false);
        out.push(PlatformCatalogItem {
            platform: p.to_string(),
            name,
            scene: meta.scene.to_string(),
            region: meta.region.to_string(),
            mode: meta.mode.to_string(),
            login_method,
            ip_policy,
            provisioned,
        });
    }
    Ok(out)
}

/// 移除某身份下指定平台的账号（选择器里取消勾选已开通项 = 减项）。返回删除条数。
#[tauri::command]
fn persona_remove_platforms(state: State<AppState>, persona_id: String, platforms: Vec<String>) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut n = 0usize;
    for p in &platforms {
        n += conn.execute(
            "DELETE FROM accounts WHERE persona_id=?1 AND platform=?2",
            params![persona_id, p],
        ).map_err(|e| e.to_string())?;
    }
    Ok(n)
}

// Platform reply configs - 回复策略（更自然的推广方式）
#[derive(Clone)]
struct ReplyConfig {
    name: &'static str,
    search_url: &'static str,              // 搜索页面
    search_selector: &'static str,          // 搜索框
    post_link_selector: &'static str,       // 帖子链接选择器
    reply_box_selector: &'static str,       // 回复框选择器
    reply_submit_selector: &'static str,    // 回复提交按钮
    max_daily_replies: i32,                 // 每日最大回复数
    min_interval_minutes: i32,              // 回复间隔（分钟）
    max_reply_length: i32,                  // 最大回复长度
    warmup_daily_limit: i32,                // 预热期每日限制
}

fn get_reply_config(platform: &str) -> Option<ReplyConfig> {
    match platform.to_lowercase().as_str() {
        "reddit" => Some(ReplyConfig {
            name: "Reddit",
            search_url: "https://www.reddit.com/search/?q={query}&type=link&sort=new",
            search_selector: "input[name=\"q\"]",
            // 旧选择器 a[data-click-id="body"] 已随 Reddit 改版失效（命中 0）；
            // 经选择器自检：a[href*="/comments/"] 命中真实帖子链接。
            post_link_selector: "a[href*=\"/comments/\"]",
            reply_box_selector: "div[contenteditable=\"true\"]",
            reply_submit_selector: "button[type=\"submit\"]",
            max_daily_replies: 15,          // 回复比发帖宽松
            min_interval_minutes: 20,        // 20分钟间隔
            max_reply_length: 2000,
            warmup_daily_limit: 5,
        }),
        "twitter" | "x" => Some(ReplyConfig {
            name: "Twitter/X",
            // 实测(2026-06)：twitter.com 仍重定向 x.com，直接用 x.com；
            // 搜索结果里时间戳<a>就是 /status/ 永久链，但它们并非 <article> 的子元素，
            // 故必须用裸 a[href*=/status/]（article a[...] 实测命中 0）。
            search_url: "https://x.com/search?q={query}&f=live",
            search_selector: "input[data-testid=\"SearchBox_Search_Input\"]",
            post_link_selector: "a[href*=\"/status/\"]",
            reply_box_selector: "[data-testid=\"tweetTextarea_0\"]",
            // 帖子详情页内联回复用 tweetButtonInline；弹层 composer 才是 tweetButton。两者都试。
            reply_submit_selector: "[data-testid=\"tweetButtonInline\"], [data-testid=\"tweetButton\"]",
            max_daily_replies: 30,
            min_interval_minutes: 10,
            max_reply_length: 280,
            warmup_daily_limit: 10,
        }),
        "hackernews" => Some(ReplyConfig {
            name: "Hacker News",
            search_url: "https://hn.algolia.com/?q={query}&type=story&sort=byDate",
            search_selector: "input.SearchInput",
            post_link_selector: "a.Story_link",
            reply_box_selector: "textarea",
            reply_submit_selector: "input[type=\"submit\"]",
            max_daily_replies: 10,          // HN 要小心
            min_interval_minutes: 30,
            max_reply_length: 2000,
            warmup_daily_limit: 3,
        }),
        "linkedin" => Some(ReplyConfig {
            name: "LinkedIn",
            // ⚠️ 已实测：LinkedIn 内容搜索页 DOM 不暴露任何帖子永久链接
            // （无 data-urn / data-id / /feed/update/ 锚点），永久链接只藏在每条
            // 帖子「···」菜单的「复制链接」JS 行为里。因此「发现链接→跳转→回复」
            // 这套模型对 LinkedIn 不成立，需改为「在搜索信息流就地回复」（待实现）。
            // 当前选择器命中 0，keyword 回复会发现不到帖子。
            search_url: "https://www.linkedin.com/search/results/content/?keywords={query}&sortBy=\"date_posted\"",
            search_selector: "input[aria-label*=\"Search\"]",
            post_link_selector: "a[data-control-name=\"content_title\"]",
            reply_box_selector: ".ql-editor",
            reply_submit_selector: "button.comments-comment-box__submit-button",
            max_daily_replies: 20,
            min_interval_minutes: 15,
            max_reply_length: 1250,
            warmup_daily_limit: 8,
        }),
        "v2ex" => Some(ReplyConfig {
            name: "V2EX",
            search_url: "https://www.google.com/search?q=site:v2ex.com+{query}",
            search_selector: "input[name=\"q\"]",
            post_link_selector: "a[href*=\"v2ex.com/t/\"]",
            reply_box_selector: "textarea#reply_content",
            reply_submit_selector: "button[type=\"submit\"]",
            max_daily_replies: 10,
            min_interval_minutes: 30,
            max_reply_length: 2000,
            warmup_daily_limit: 3,
        }),
        "zhihu" => Some(ReplyConfig {
            name: "知乎",
            search_url: "https://www.zhihu.com/search?type=content&q={query}",
            search_selector: "input[name=\"q\"]",
            post_link_selector: "a[data-za-detail-view-path-module=\"AnswerItem\"]",
            reply_box_selector: ".public-DraftEditor-content",
            reply_submit_selector: "button.Button--primary",
            max_daily_replies: 15,
            min_interval_minutes: 20,
            max_reply_length: 5000,
            warmup_daily_limit: 5,
        }),
        // === 中国平台回复配置 ===
        "weibo" => Some(ReplyConfig {
            name: "微博",
            search_url: "https://s.weibo.com/weibo?q={query}&typeall=1&suball=1&timescope=custom:2024-01-01:&Refer=g",
            search_selector: "input.search-input",
            post_link_selector: "div.card-wrap a[href*=\"weibo.com\"]",
            reply_box_selector: "textarea.W_input",
            reply_submit_selector: "a.W_btn_a",
            max_daily_replies: 30,
            min_interval_minutes: 10,
            max_reply_length: 140,
            warmup_daily_limit: 10,
        }),
        "sspai" => Some(ReplyConfig {
            name: "少数派",
            search_url: "https://sspai.com/search/article/{query}",
            search_selector: "input.search-input",
            post_link_selector: "a.article-link",
            reply_box_selector: ".comment-editor",
            reply_submit_selector: "button.submit-comment",
            max_daily_replies: 10,
            min_interval_minutes: 30,
            max_reply_length: 2000,
            warmup_daily_limit: 5,
        }),
        "jike" | "okjike" => Some(ReplyConfig {
            name: "即刻",
            search_url: "https://web.okjike.com/search?keyword={query}",
            search_selector: "input.search-input",
            post_link_selector: "a.post-link",
            reply_box_selector: "div[contenteditable=\"true\"]",
            reply_submit_selector: "button.reply-btn",
            max_daily_replies: 20,
            min_interval_minutes: 15,
            max_reply_length: 1000,
            warmup_daily_limit: 10,
        }),
        "xiaohongshu" | "redbook" => Some(ReplyConfig {
            name: "小红书",
            search_url: "https://www.xiaohongshu.com/search_result?keyword={query}",
            search_selector: "input.search-input",
            post_link_selector: "a.note-link",
            reply_box_selector: "textarea.comment-input",
            reply_submit_selector: "button.submit-comment",
            max_daily_replies: 30,
            min_interval_minutes: 10,
            max_reply_length: 500,
            warmup_daily_limit: 10,
        }),
        "segmentfault" => Some(ReplyConfig {
            name: "SegmentFault",
            search_url: "https://segmentfault.com/search?q={query}",
            search_selector: "input#search",
            post_link_selector: "a.article-title",
            reply_box_selector: ".CodeMirror",
            reply_submit_selector: "button.submit-btn",
            max_daily_replies: 15,
            min_interval_minutes: 20,
            max_reply_length: 5000,
            warmup_daily_limit: 5,
        }),
        "csdn" => Some(ReplyConfig {
            name: "CSDN",
            search_url: "https://so.csdn.net/so/search?q={query}&t=all",
            search_selector: "input#search-input",
            post_link_selector: "a.limit_width",
            reply_box_selector: "textarea.comment-text",
            reply_submit_selector: "button.comment-submit",
            max_daily_replies: 20,
            min_interval_minutes: 15,
            max_reply_length: 1000,
            warmup_daily_limit: 10,
        }),
        // === 开发者平台回复配置 ===
        "github" => Some(ReplyConfig {
            name: "GitHub",
            search_url: "https://github.com/search?q={query}&type=discussions",
            search_selector: "input.header-search-input",
            post_link_selector: "a.Link--primary",
            reply_box_selector: "textarea#new_comment_field",
            reply_submit_selector: "button.btn-primary",
            max_daily_replies: 20,
            min_interval_minutes: 15,
            max_reply_length: 65536,
            warmup_daily_limit: 10,
        }),
        "devto" => Some(ReplyConfig {
            name: "DEV.to",
            search_url: "https://dev.to/search?q={query}",
            search_selector: "input#search-input",
            post_link_selector: "a.crayons-story__hidden-navigation-link",
            reply_box_selector: "textarea#comment_body_markdown",
            reply_submit_selector: "button[type=\"submit\"]",
            max_daily_replies: 15,
            min_interval_minutes: 20,
            max_reply_length: 10000,
            warmup_daily_limit: 5,
        }),
        "medium" => Some(ReplyConfig {
            name: "Medium",
            search_url: "https://medium.com/search?q={query}",
            search_selector: "input[type=\"search\"]",
            post_link_selector: "article a[href*=\"medium.com\"]",
            reply_box_selector: ".ProseMirror",
            reply_submit_selector: "button[data-action=\"respond\"]",
            max_daily_replies: 15,
            min_interval_minutes: 20,
            max_reply_length: 5000,
            warmup_daily_limit: 5,
        }),
        "hashnode" => Some(ReplyConfig {
            name: "Hashnode",
            search_url: "https://hashnode.com/search?q={query}",
            search_selector: "input[type=\"search\"]",
            post_link_selector: "a.post-card-title",
            reply_box_selector: "textarea.comment-textarea",
            reply_submit_selector: "button.submit-comment",
            max_daily_replies: 15,
            min_interval_minutes: 20,
            max_reply_length: 5000,
            warmup_daily_limit: 5,
        }),
        "indiehackers" => Some(ReplyConfig {
            name: "Indie Hackers",
            search_url: "https://www.indiehackers.com/search?q={query}",
            search_selector: "input.search-input",
            post_link_selector: "a.post-link",
            reply_box_selector: "textarea.comment-body",
            reply_submit_selector: "button.submit-comment",
            max_daily_replies: 15,
            min_interval_minutes: 20,
            max_reply_length: 5000,
            warmup_daily_limit: 5,
        }),
        "producthunt" => Some(ReplyConfig {
            name: "Product Hunt",
            search_url: "https://www.producthunt.com/search?q={query}",
            search_selector: "input[type=\"search\"]",
            post_link_selector: "a[href*=\"/posts/\"]",
            reply_box_selector: "textarea[placeholder*=\"comment\"]",
            reply_submit_selector: "button[type=\"submit\"]",
            max_daily_replies: 10,
            min_interval_minutes: 30,
            max_reply_length: 1000,
            warmup_daily_limit: 5,
        }),
        // === 日本平台回复配置 ===
        "qiita" => Some(ReplyConfig {
            name: "Qiita",
            search_url: "https://qiita.com/search?q={query}",
            search_selector: "input[name=\"q\"]",
            post_link_selector: "a.searchResult_itemTitle",
            reply_box_selector: "textarea#comment_body",
            reply_submit_selector: "button[type=\"submit\"]",
            max_daily_replies: 10,
            min_interval_minutes: 30,
            max_reply_length: 10000,
            warmup_daily_limit: 5,
        }),
        "zenn" => Some(ReplyConfig {
            name: "Zenn",
            search_url: "https://zenn.dev/search?q={query}",
            search_selector: "input[type=\"search\"]",
            post_link_selector: "a.ArticleCard_link",
            reply_box_selector: "textarea.comment-textarea",
            reply_submit_selector: "button.submit-comment",
            max_daily_replies: 10,
            min_interval_minutes: 30,
            max_reply_length: 5000,
            warmup_daily_limit: 5,
        }),
        // === 俄罗斯平台回复配置 ===
        "habr" => Some(ReplyConfig {
            name: "Habr",
            search_url: "https://habr.com/ru/search/?q={query}",
            search_selector: "input[name=\"q\"]",
            post_link_selector: "a.post__title_link",
            reply_box_selector: "textarea.comment-textarea",
            reply_submit_selector: "button.submit-comment",
            max_daily_replies: 10,
            min_interval_minutes: 30,
            max_reply_length: 10000,
            warmup_daily_limit: 3,
        }),
        "vk" => Some(ReplyConfig {
            name: "VK",
            search_url: "https://vk.com/search?c%5Bsection%5D=auto&q={query}",
            search_selector: "input#ts_input",
            post_link_selector: "a.wall_post_cont",
            reply_box_selector: "div[contenteditable=\"true\"]",
            reply_submit_selector: "button.send",
            max_daily_replies: 30,
            min_interval_minutes: 10,
            max_reply_length: 4096,
            warmup_daily_limit: 10,
        }),
        // === 社交平台回复配置 ===
        "facebook" => Some(ReplyConfig {
            name: "Facebook",
            search_url: "https://www.facebook.com/search/posts/?q={query}",
            search_selector: "input[type=\"search\"]",
            post_link_selector: "a[href*=\"/posts/\"]",
            reply_box_selector: "div[role=\"textbox\"]",
            reply_submit_selector: "div[aria-label=\"Comment\"]",
            max_daily_replies: 20,
            min_interval_minutes: 15,
            max_reply_length: 8000,
            warmup_daily_limit: 10,
        }),
        // === 新增平台回复配置 ===
        "toutiao" | "jinritoutiao" => Some(ReplyConfig {
            name: "今日头条",
            search_url: "https://so.toutiao.com/search?keyword={query}&pd=information",
            search_selector: "input.search-input",
            post_link_selector: "a[href*=\"toutiao.com/article/\"]",
            reply_box_selector: "textarea.comment-input",
            reply_submit_selector: "button.submit-btn",
            max_daily_replies: 20,
            min_interval_minutes: 15,
            max_reply_length: 1000,
            warmup_daily_limit: 8,
        }),
        "douban" => Some(ReplyConfig {
            name: "豆瓣",
            search_url: "https://www.douban.com/search?q={query}",
            search_selector: "input#inp-query",
            post_link_selector: "a[href*=\"douban.com/group/topic/\"]",
            reply_box_selector: "textarea#last",
            reply_submit_selector: "input.bn-flat",
            max_daily_replies: 15,
            min_interval_minutes: 20,
            max_reply_length: 3000,
            warmup_daily_limit: 5,
        }),
        "tieba" | "baidu_tieba" => Some(ReplyConfig {
            name: "百度贴吧",
            search_url: "https://tieba.baidu.com/f/search/res?qw={query}",
            search_selector: "input.search-ipt",
            post_link_selector: "a[href*=\"tieba.baidu.com/p/\"]",
            reply_box_selector: "textarea.ueditor-content",
            reply_submit_selector: "button.j_submit",
            max_daily_replies: 20,
            min_interval_minutes: 15,
            max_reply_length: 2000,
            warmup_daily_limit: 5,
        }),
        "quora" => Some(ReplyConfig {
            name: "Quora",
            search_url: "https://www.quora.com/search?q={query}",
            search_selector: "input[placeholder*=\"Search\"]",
            post_link_selector: "a[href*=\"/answer/\"]",
            reply_box_selector: "div.q-box.qu-pb--small",
            reply_submit_selector: "button[aria-label=\"Post\"]",
            max_daily_replies: 10,
            min_interval_minutes: 30,
            max_reply_length: 5000,
            warmup_daily_limit: 3,
        }),
        "instagram" => Some(ReplyConfig {
            name: "Instagram",
            search_url: "https://www.instagram.com/explore/tags/{query}/",
            search_selector: "input[placeholder=\"Search\"]",
            post_link_selector: "a[href*=\"/p/\"]",
            reply_box_selector: "textarea[aria-label*=\"comment\"]",
            reply_submit_selector: "button[type=\"submit\"]",
            max_daily_replies: 30,
            min_interval_minutes: 10,
            max_reply_length: 2200,
            warmup_daily_limit: 10,
        }),
        "pinterest" => Some(ReplyConfig {
            name: "Pinterest",
            search_url: "https://www.pinterest.com/search/pins/?q={query}",
            search_selector: "input[name=\"searchBoxInput\"]",
            post_link_selector: "a[href*=\"/pin/\"]",
            reply_box_selector: "textarea[placeholder*=\"comment\"]",
            reply_submit_selector: "button[aria-label=\"Save\"]",
            max_daily_replies: 20,
            min_interval_minutes: 15,
            max_reply_length: 500,
            warmup_daily_limit: 10,
        }),
        "threads" => Some(ReplyConfig {
            name: "Threads",
            search_url: "https://www.threads.net/search?q={query}",
            search_selector: "input[type=\"search\"]",
            post_link_selector: "a[href*=\"/post/\"]",
            reply_box_selector: "div[contenteditable=\"true\"]",
            reply_submit_selector: "div[role=\"button\"]",
            max_daily_replies: 30,
            min_interval_minutes: 10,
            max_reply_length: 500,
            warmup_daily_limit: 10,
        }),
        "mastodon" => Some(ReplyConfig {
            name: "Mastodon",
            search_url: "https://mastodon.social/search?q={query}",
            search_selector: "input[type=\"search\"]",
            post_link_selector: "a.status__relative-time",
            reply_box_selector: "textarea.autosuggest-textarea__textarea",
            reply_submit_selector: "button.compose-form__publish-button",
            max_daily_replies: 30,
            min_interval_minutes: 10,
            max_reply_length: 500,
            warmup_daily_limit: 15,
        }),
        "bluesky" => Some(ReplyConfig {
            name: "Bluesky",
            search_url: "https://bsky.app/search?q={query}",
            search_selector: "input[type=\"search\"]",
            post_link_selector: "a[href*=\"/post/\"]",
            reply_box_selector: "div[contenteditable=\"true\"]",
            reply_submit_selector: "button[data-testid=\"composerPublishBtn\"]",
            max_daily_replies: 30,
            min_interval_minutes: 10,
            max_reply_length: 300,
            warmup_daily_limit: 10,
        }),
        "discord" => Some(ReplyConfig {
            name: "Discord",
            search_url: "https://discord.com/channels/",  // Discord 不支持公开搜索
            search_selector: "input[aria-label=\"Search\"]",
            post_link_selector: "div[id^=\"message-\"]",
            reply_box_selector: "div[role=\"textbox\"]",
            reply_submit_selector: "button[type=\"submit\"]",
            max_daily_replies: 50,
            min_interval_minutes: 5,
            max_reply_length: 2000,
            warmup_daily_limit: 30,
        }),
        _ => None,
    }
}

// Platform publishing configs - 每个平台的发布策略
#[derive(Clone, Serialize)]
struct PublishConfig {
    name: &'static str,
    post_url: &'static str,                    // 发帖页面
    content_selector: &'static str,            // 内容输入框
    submit_selector: &'static str,             // 发布按钮
    max_daily_posts: i32,                      // 每日最大发帖数
    min_interval_minutes: i32,                 // 最小发帖间隔（分钟）
    max_content_length: i32,                   // 最大内容长度
    supports_images: bool,                     // 支持图片
    supports_links: bool,                      // 支持链接
    needs_title: bool,                         // 需要标题
    title_selector: Option<&'static str>,      // 标题输入框
    hashtag_style: &'static str,               // hashtag 风格: "hash", "none", "inline"
    warmup_days: i32,                          // 新账号预热天数
    warmup_daily_limit: i32,                   // 预热期每日限制
}

fn get_publish_config(platform: &str) -> Option<PublishConfig> {
    match platform.to_lowercase().as_str() {
        "reddit" => Some(PublishConfig {
            name: "Reddit",
            post_url: "https://www.reddit.com/submit",
            content_selector: "textarea[name=\"text\"]",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 5,           // Reddit 对新账号很严格
            min_interval_minutes: 120,     // 至少2小时间隔
            max_content_length: 40000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input[name=\"title\"]"),
            hashtag_style: "none",        // Reddit 不用 hashtag
            warmup_days: 30,              // 新账号需要30天预热
            warmup_daily_limit: 1,
        }),
        "producthunt" => Some(PublishConfig {
            name: "Product Hunt",
            post_url: "https://www.producthunt.com/posts/new",
            content_selector: "textarea[name=\"tagline\"]",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 1,           // PH 每天只能发一个产品
            min_interval_minutes: 1440,    // 24小时
            max_content_length: 260,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input[name=\"name\"]"),
            hashtag_style: "none",
            warmup_days: 7,
            warmup_daily_limit: 0,        // 预热期只能评论，不能发帖
        }),
        "twitter" | "x" => Some(PublishConfig {
            name: "Twitter/X",
            post_url: "https://twitter.com/compose/tweet",
            content_selector: "[data-testid=\"tweetTextarea_0\"]",
            submit_selector: "[data-testid=\"tweetButton\"]",
            max_daily_posts: 10,          // 新账号保守些
            min_interval_minutes: 30,
            max_content_length: 280,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 14,
            warmup_daily_limit: 3,
        }),
        "linkedin" => Some(PublishConfig {
            name: "LinkedIn",
            post_url: "https://www.linkedin.com/feed/",
            content_selector: ".ql-editor",
            submit_selector: "button.share-actions__primary-action",
            max_daily_posts: 3,
            min_interval_minutes: 240,     // 4小时
            max_content_length: 3000,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        "hackernews" => Some(PublishConfig {
            name: "Hacker News",
            post_url: "https://news.ycombinator.com/submit",
            content_selector: "textarea[name=\"text\"]",
            submit_selector: "input[type=\"submit\"]",
            max_daily_posts: 2,           // HN 非常严格
            min_interval_minutes: 480,     // 8小时
            max_content_length: 2000,
            supports_images: false,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input[name=\"title\"]"),
            hashtag_style: "none",
            warmup_days: 60,              // HN 需要长时间积累 karma
            warmup_daily_limit: 0,        // 预热期只能评论
        }),
        "devto" => Some(PublishConfig {
            name: "DEV.to",
            post_url: "https://dev.to/new",
            content_selector: "#article_body_markdown",
            submit_selector: "button[id=\"submit-button\"]",
            max_daily_posts: 2,
            min_interval_minutes: 360,     // 6小时
            max_content_length: 65535,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("#article-form-title"),
            hashtag_style: "inline",      // Dev.to 用 front matter tags
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        "medium" => Some(PublishConfig {
            name: "Medium",
            post_url: "https://medium.com/new-story",
            content_selector: ".ProseMirror",
            submit_selector: "button[data-action=\"publish\"]",
            max_daily_posts: 3,
            min_interval_minutes: 180,
            max_content_length: 100000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("h3[data-contents=\"true\"]"),
            hashtag_style: "none",        // Medium 用 tags
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        "weibo" => Some(PublishConfig {
            name: "微博",
            post_url: "https://weibo.com/",
            content_selector: "textarea.W_input",
            submit_selector: "a.W_btn_a",
            max_daily_posts: 10,
            min_interval_minutes: 30,
            max_content_length: 2000,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",        // #话题#
            warmup_days: 7,
            warmup_daily_limit: 3,
        }),
        "zhihu" => Some(PublishConfig {
            name: "知乎",
            post_url: "https://www.zhihu.com/creator",
            content_selector: ".public-DraftEditor-content",
            submit_selector: "button.PublishButton",
            max_daily_posts: 3,
            min_interval_minutes: 240,
            max_content_length: 100000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("textarea.Input"),
            hashtag_style: "none",
            warmup_days: 14,
            warmup_daily_limit: 1,
        }),
        // === 第一梯队平台 ===
        "github" => Some(PublishConfig {
            name: "GitHub",
            post_url: "https://github.com/new",  // 新建 repo / Discussion
            content_selector: "textarea#repo_description",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 5,
            min_interval_minutes: 60,
            max_content_length: 1000,
            supports_images: false,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input#repo_name"),
            hashtag_style: "none",
            warmup_days: 7,
            warmup_daily_limit: 2,
        }),
        "v2ex" => Some(PublishConfig {
            name: "V2EX",
            post_url: "https://www.v2ex.com/new",
            content_selector: "textarea#topic_content",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 3,
            min_interval_minutes: 360,           // V2EX 限制严格
            max_content_length: 20000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input#topic_title"),
            hashtag_style: "none",
            warmup_days: 30,                     // V2EX 需要金币
            warmup_daily_limit: 1,
        }),
        "sspai" => Some(PublishConfig {
            name: "少数派",
            post_url: "https://sspai.com/post/create",
            content_selector: ".editor-content",
            submit_selector: "button.submit-btn",
            max_daily_posts: 2,
            min_interval_minutes: 480,
            max_content_length: 50000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input.title-input"),
            hashtag_style: "none",
            warmup_days: 14,
            warmup_daily_limit: 1,
        }),
        "jike" | "okjike" => Some(PublishConfig {
            name: "即刻",
            post_url: "https://web.okjike.com/",
            content_selector: "div[contenteditable=\"true\"]",
            submit_selector: "button.publish-btn",
            max_daily_posts: 10,
            min_interval_minutes: 30,
            max_content_length: 3000,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 7,
            warmup_daily_limit: 3,
        }),
        // === 第二梯队平台 ===
        "hashnode" => Some(PublishConfig {
            name: "Hashnode",
            post_url: "https://hashnode.com/draft",
            content_selector: ".ProseMirror",
            submit_selector: "button[data-testid=\"publish-btn\"]",
            max_daily_posts: 3,
            min_interval_minutes: 240,
            max_content_length: 100000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input[placeholder*=\"title\"]"),
            hashtag_style: "none",
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        "xiaohongshu" | "redbook" => Some(PublishConfig {
            name: "小红书",
            post_url: "https://creator.xiaohongshu.com/publish/publish",
            content_selector: "div[contenteditable=\"true\"]",
            submit_selector: "button.publish-btn",
            max_daily_posts: 5,
            min_interval_minutes: 60,
            max_content_length: 1000,
            supports_images: true,
            supports_links: false,               // 小红书不支持外链
            needs_title: true,
            title_selector: Some("input.title-input"),
            hashtag_style: "hash",
            warmup_days: 7,
            warmup_daily_limit: 2,
        }),
        "indiehackers" => Some(PublishConfig {
            name: "Indie Hackers",
            post_url: "https://www.indiehackers.com/new-post",
            content_selector: "textarea.post-body",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 2,
            min_interval_minutes: 360,
            max_content_length: 10000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input.post-title"),
            hashtag_style: "none",
            warmup_days: 14,
            warmup_daily_limit: 1,
        }),
        "segmentfault" => Some(PublishConfig {
            name: "SegmentFault",
            post_url: "https://segmentfault.com/write",
            content_selector: ".CodeMirror-code",
            submit_selector: "button.publish-btn",
            max_daily_posts: 3,
            min_interval_minutes: 240,
            max_content_length: 100000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input#title"),
            hashtag_style: "none",
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        "csdn" => Some(PublishConfig {
            name: "CSDN",
            post_url: "https://editor.csdn.net/md",
            content_selector: ".editor-preview",
            submit_selector: "button.btn-publish",
            max_daily_posts: 5,
            min_interval_minutes: 120,
            max_content_length: 100000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input.title-input"),
            hashtag_style: "none",
            warmup_days: 7,
            warmup_daily_limit: 2,
        }),
        "oschina" => Some(PublishConfig {
            name: "开源中国",
            post_url: "https://my.oschina.net/blog/write",
            content_selector: ".editor-content",
            submit_selector: "button.submit",
            max_daily_posts: 3,
            min_interval_minutes: 240,
            max_content_length: 100000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input.title"),
            hashtag_style: "none",
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        // === 日本平台 ===
        "qiita" => Some(PublishConfig {
            name: "Qiita",
            post_url: "https://qiita.com/drafts/new",
            content_selector: "textarea.editor",
            submit_selector: "button[data-test=\"publish-button\"]",
            max_daily_posts: 2,
            min_interval_minutes: 360,
            max_content_length: 65535,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input[name=\"title\"]"),
            hashtag_style: "none",               // Qiita 用 tags
            warmup_days: 14,
            warmup_daily_limit: 1,
        }),
        "zenn" => Some(PublishConfig {
            name: "Zenn",
            post_url: "https://zenn.dev/articles/new",
            content_selector: ".editor-markdown",
            submit_selector: "button.publish-button",
            max_daily_posts: 2,
            min_interval_minutes: 360,
            max_content_length: 65535,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input[name=\"title\"]"),
            hashtag_style: "none",
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        "note" | "note_japan" => Some(PublishConfig {
            name: "note",
            post_url: "https://note.com/notes/new",
            content_selector: ".editor-body",
            submit_selector: "button.publish-button",
            max_daily_posts: 3,
            min_interval_minutes: 180,
            max_content_length: 50000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input.title-input"),
            hashtag_style: "hash",
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        // === 俄罗斯/其他平台 ===
        "habr" => Some(PublishConfig {
            name: "Habr",
            post_url: "https://habr.com/ru/publication/new/",
            content_selector: ".editor-content",
            submit_selector: "button.submit-btn",
            max_daily_posts: 2,
            min_interval_minutes: 480,
            max_content_length: 100000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input.title-input"),
            hashtag_style: "none",
            warmup_days: 30,                     // Habr 审核严格
            warmup_daily_limit: 0,
        }),
        "vk" => Some(PublishConfig {
            name: "VK",
            post_url: "https://vk.com/feed",
            content_selector: "div[contenteditable=\"true\"]",
            submit_selector: "button.submit",
            max_daily_posts: 10,
            min_interval_minutes: 30,
            max_content_length: 15895,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 7,
            warmup_daily_limit: 3,
        }),
        "betalist" => Some(PublishConfig {
            name: "BetaList",
            post_url: "https://betalist.com/submit",
            content_selector: "textarea#startup_description",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 1,
            min_interval_minutes: 1440,          // 每天只能提交一个
            max_content_length: 500,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input#startup_name"),
            hashtag_style: "none",
            warmup_days: 0,
            warmup_daily_limit: 1,
        }),
        "alternativeto" => Some(PublishConfig {
            name: "AlternativeTo",
            post_url: "https://alternativeto.net/add-app/",
            content_selector: "textarea#description",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 2,
            min_interval_minutes: 720,
            max_content_length: 2000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input#name"),
            hashtag_style: "none",
            warmup_days: 0,
            warmup_daily_limit: 2,
        }),
        // === 韩国平台 ===
        "naver_blog" => Some(PublishConfig {
            name: "Naver Blog",
            post_url: "https://blog.naver.com/PostWriteForm.naver",
            content_selector: ".se-component-content",
            submit_selector: "button.publish-btn",
            max_daily_posts: 3,
            min_interval_minutes: 180,
            max_content_length: 100000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input.se-placeholder-title"),
            hashtag_style: "hash",
            warmup_days: 7,
            warmup_daily_limit: 1,
        }),
        // === 东南亚平台 ===
        "facebook" => Some(PublishConfig {
            name: "Facebook",
            post_url: "https://www.facebook.com/",
            content_selector: "div[role=\"textbox\"]",
            submit_selector: "div[aria-label=\"Post\"]",
            max_daily_posts: 5,
            min_interval_minutes: 60,
            max_content_length: 63206,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 14,
            warmup_daily_limit: 2,
        }),
        "telegram" => Some(PublishConfig {
            name: "Telegram",
            post_url: "https://web.telegram.org/",
            content_selector: "div.input-message-input",
            submit_selector: "button.send",
            max_daily_posts: 20,
            min_interval_minutes: 15,
            max_content_length: 4096,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 3,
            warmup_daily_limit: 10,
        }),
        // === 新增平台 ===
        "toutiao" | "jinritoutiao" => Some(PublishConfig {
            name: "今日头条",
            post_url: "https://mp.toutiao.com/profile_v4/graphic/publish",
            content_selector: "div.ProseMirror",
            submit_selector: "button.publish-btn",
            max_daily_posts: 5,
            min_interval_minutes: 120,
            max_content_length: 50000,
            supports_images: true,
            supports_links: true,
            needs_title: true,
            title_selector: Some("input.article-title-input"),
            hashtag_style: "hash",
            warmup_days: 14,
            warmup_daily_limit: 1,
        }),
        "douban" => Some(PublishConfig {
            name: "豆瓣",
            post_url: "https://www.douban.com/",
            content_selector: "textarea#isay-cont",
            submit_selector: "button.bn-flat-green",
            max_daily_posts: 10,
            min_interval_minutes: 30,
            max_content_length: 1000,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "none",
            warmup_days: 14,
            warmup_daily_limit: 3,
        }),
        "tieba" | "baidu_tieba" => Some(PublishConfig {
            name: "百度贴吧",
            post_url: "https://tieba.baidu.com/",
            content_selector: "textarea.ueditor-content",
            submit_selector: "button.j_submit",
            max_daily_posts: 10,
            min_interval_minutes: 30,
            max_content_length: 20000,
            supports_images: true,
            supports_links: false,           // 贴吧限制外链
            needs_title: true,
            title_selector: Some("input.j_title"),
            hashtag_style: "none",
            warmup_days: 30,                 // 贴吧对新号限制严格
            warmup_daily_limit: 2,
        }),
        "quora" => Some(PublishConfig {
            name: "Quora",
            post_url: "https://www.quora.com/",
            content_selector: "div.q-box.qu-pb--small",
            submit_selector: "button[aria-label=\"Post\"]",
            max_daily_posts: 5,
            min_interval_minutes: 60,
            max_content_length: 10000,
            supports_images: true,
            supports_links: true,
            needs_title: false,              // Quora 主要是回答问题
            title_selector: None,
            hashtag_style: "none",
            warmup_days: 14,
            warmup_daily_limit: 2,
        }),
        "instagram" => Some(PublishConfig {
            name: "Instagram",
            post_url: "https://www.instagram.com/",
            content_selector: "textarea[aria-label=\"Write a caption...\"]",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 5,
            min_interval_minutes: 120,
            max_content_length: 2200,
            supports_images: true,           // Instagram 必须有图片
            supports_links: false,           // 帖子不支持可点击链接
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 14,
            warmup_daily_limit: 2,
        }),
        "pinterest" => Some(PublishConfig {
            name: "Pinterest",
            post_url: "https://www.pinterest.com/pin-builder/",
            content_selector: "textarea[placeholder*=\"description\"]",
            submit_selector: "button[data-test-id=\"board-dropdown-save-button\"]",
            max_daily_posts: 15,
            min_interval_minutes: 30,
            max_content_length: 500,
            supports_images: true,           // Pinterest 必须有图片
            supports_links: true,
            needs_title: true,
            title_selector: Some("input[placeholder*=\"title\"]"),
            hashtag_style: "hash",
            warmup_days: 7,
            warmup_daily_limit: 5,
        }),
        "threads" => Some(PublishConfig {
            name: "Threads",
            post_url: "https://www.threads.net/",
            content_selector: "div[contenteditable=\"true\"]",
            submit_selector: "div[role=\"button\"][tabindex=\"0\"]",
            max_daily_posts: 10,
            min_interval_minutes: 30,
            max_content_length: 500,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 7,
            warmup_daily_limit: 3,
        }),
        "mastodon" => Some(PublishConfig {
            name: "Mastodon",
            post_url: "https://mastodon.social/",
            content_selector: "textarea.autosuggest-textarea__textarea",
            submit_selector: "button.compose-form__publish-button",
            max_daily_posts: 20,
            min_interval_minutes: 15,
            max_content_length: 500,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 3,
            warmup_daily_limit: 10,
        }),
        "bluesky" => Some(PublishConfig {
            name: "Bluesky",
            post_url: "https://bsky.app/",
            content_selector: "div[contenteditable=\"true\"]",
            submit_selector: "button[data-testid=\"composerPublishBtn\"]",
            max_daily_posts: 15,
            min_interval_minutes: 20,
            max_content_length: 300,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "hash",
            warmup_days: 3,
            warmup_daily_limit: 5,
        }),
        "discord" => Some(PublishConfig {
            name: "Discord",
            post_url: "https://discord.com/channels/",
            content_selector: "div[role=\"textbox\"]",
            submit_selector: "button[type=\"submit\"]",
            max_daily_posts: 30,
            min_interval_minutes: 10,
            max_content_length: 2000,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "none",
            warmup_days: 3,
            warmup_daily_limit: 15,
        }),
        "slack" => Some(PublishConfig {
            name: "Slack",
            post_url: "https://app.slack.com/",
            content_selector: "div[data-qa=\"message_input\"]",
            submit_selector: "button[data-qa=\"texty_send_button\"]",
            max_daily_posts: 30,
            min_interval_minutes: 10,
            max_content_length: 4000,
            supports_images: true,
            supports_links: true,
            needs_title: false,
            title_selector: None,
            hashtag_style: "none",
            warmup_days: 0,                  // Slack 私有空间无需预热
            warmup_daily_limit: 30,
        }),
        _ => None,
    }
}

fn get_platform_config(platform: &str) -> Option<PlatformConfig> {
    match platform.to_lowercase().as_str() {
        "reddit" => Some(PlatformConfig {
            name: "Reddit",
            login_url: "https://www.reddit.com/login",
            register_url: "https://www.reddit.com/register",
            google_oauth: true,
            google_button_selector: Some("button[data-provider=\"google\"]"),
        }),
        "producthunt" => Some(PlatformConfig {
            name: "Product Hunt",
            login_url: "https://www.producthunt.com/login",
            register_url: "https://www.producthunt.com/join",
            google_oauth: true,
            google_button_selector: Some("button[data-test=\"login-with-google\"]"),
        }),
        "hackernews" => Some(PlatformConfig {
            name: "Hacker News",
            login_url: "https://news.ycombinator.com/login",
            register_url: "https://news.ycombinator.com/login",
            google_oauth: false,
            google_button_selector: None,
        }),
        "devto" => Some(PlatformConfig {
            name: "DEV.to",
            login_url: "https://dev.to/enter",
            register_url: "https://dev.to/enter?state=new-user",
            google_oauth: true,
            google_button_selector: Some("a[href*=\"oauth/google\"]"),
        }),
        "twitter" => Some(PlatformConfig {
            name: "Twitter/X",
            login_url: "https://twitter.com/i/flow/login",
            register_url: "https://twitter.com/i/flow/signup",
            google_oauth: true,
            google_button_selector: Some("button[aria-label*=\"Google\"]"),
        }),
        "linkedin" => Some(PlatformConfig {
            name: "LinkedIn",
            login_url: "https://www.linkedin.com/login",
            register_url: "https://www.linkedin.com/signup",
            google_oauth: true,
            google_button_selector: Some("button[data-tracking-control-name*=\"google\"]"),
        }),
        "medium" => Some(PlatformConfig {
            name: "Medium",
            login_url: "https://medium.com/m/signin",
            register_url: "https://medium.com/m/signin?operation=register",
            google_oauth: true,
            google_button_selector: Some("button[data-testid=\"googleButton\"]"),
        }),
        "hashnode" => Some(PlatformConfig {
            name: "Hashnode",
            login_url: "https://hashnode.com/login",
            register_url: "https://hashnode.com/onboard",
            google_oauth: true,
            google_button_selector: Some("button[data-testid=\"google-login\"]"),
        }),
        "indiehackers" => Some(PlatformConfig {
            name: "Indie Hackers",
            login_url: "https://www.indiehackers.com/sign-in",
            register_url: "https://www.indiehackers.com/sign-up",
            google_oauth: true,
            google_button_selector: Some("button.google-button"),
        }),
        "v2ex" => Some(PlatformConfig {
            name: "V2EX",
            login_url: "https://www.v2ex.com/signin",
            register_url: "https://www.v2ex.com/signup",
            google_oauth: true,
            google_button_selector: Some("a[href*=\"auth/google\"]"),
        }),
        // === 中国平台 ===
        "zhihu" => Some(PlatformConfig {
            name: "知乎",
            login_url: "https://www.zhihu.com/signin",
            register_url: "https://www.zhihu.com/signup",
            google_oauth: false,
            google_button_selector: None,
        }),
        "weibo" => Some(PlatformConfig {
            name: "微博",
            login_url: "https://weibo.com/login.php",
            register_url: "https://weibo.com/signup/signup.php",
            google_oauth: false,
            google_button_selector: None,
        }),
        "sspai" => Some(PlatformConfig {
            name: "少数派",
            login_url: "https://sspai.com/login",
            register_url: "https://sspai.com/register",
            google_oauth: false,
            google_button_selector: None,
        }),
        "jike" | "okjike" => Some(PlatformConfig {
            name: "即刻",
            login_url: "https://web.okjike.com/",
            register_url: "https://web.okjike.com/",
            google_oauth: false,
            google_button_selector: None,
        }),
        "xiaohongshu" | "redbook" => Some(PlatformConfig {
            name: "小红书",
            login_url: "https://www.xiaohongshu.com/login",
            register_url: "https://www.xiaohongshu.com/login",
            google_oauth: false,
            google_button_selector: None,
        }),
        "segmentfault" => Some(PlatformConfig {
            name: "SegmentFault",
            login_url: "https://segmentfault.com/user/login",
            register_url: "https://segmentfault.com/user/register",
            google_oauth: true,
            google_button_selector: Some("a[href*=\"oauth/google\"]"),
        }),
        "csdn" => Some(PlatformConfig {
            name: "CSDN",
            login_url: "https://passport.csdn.net/login",
            register_url: "https://passport.csdn.net/register",
            google_oauth: false,
            google_button_selector: None,
        }),
        "oschina" => Some(PlatformConfig {
            name: "开源中国",
            login_url: "https://www.oschina.net/home/login",
            register_url: "https://www.oschina.net/home/reg",
            google_oauth: false,
            google_button_selector: None,
        }),
        // === 全球平台 ===
        "github" => Some(PlatformConfig {
            name: "GitHub",
            login_url: "https://github.com/login",
            register_url: "https://github.com/signup",
            google_oauth: true,
            google_button_selector: Some("button[data-login-type=\"google\"]"),
        }),
        "facebook" => Some(PlatformConfig {
            name: "Facebook",
            login_url: "https://www.facebook.com/login",
            register_url: "https://www.facebook.com/r.php",
            google_oauth: false,
            google_button_selector: None,
        }),
        "telegram" => Some(PlatformConfig {
            name: "Telegram",
            login_url: "https://web.telegram.org/",
            register_url: "https://telegram.org/",
            google_oauth: false,
            google_button_selector: None,
        }),
        "betalist" => Some(PlatformConfig {
            name: "BetaList",
            login_url: "https://betalist.com/users/sign_in",
            register_url: "https://betalist.com/users/sign_up",
            google_oauth: true,
            google_button_selector: Some("a[href*=\"auth/google\"]"),
        }),
        "alternativeto" => Some(PlatformConfig {
            name: "AlternativeTo",
            login_url: "https://alternativeto.net/login/",
            register_url: "https://alternativeto.net/register/",
            google_oauth: true,
            google_button_selector: Some("a[href*=\"auth/google\"]"),
        }),
        // === 日本平台 ===
        "qiita" => Some(PlatformConfig {
            name: "Qiita",
            login_url: "https://qiita.com/login",
            register_url: "https://qiita.com/signup",
            google_oauth: true,
            google_button_selector: Some("a[href*=\"auth/google_oauth2\"]"),
        }),
        "zenn" => Some(PlatformConfig {
            name: "Zenn",
            login_url: "https://zenn.dev/enter",
            register_url: "https://zenn.dev/enter",
            google_oauth: true,
            google_button_selector: Some("button[data-testid=\"google-login\"]"),
        }),
        "note" | "note_japan" => Some(PlatformConfig {
            name: "note",
            login_url: "https://note.com/login",
            register_url: "https://note.com/signup",
            google_oauth: true,
            google_button_selector: Some("button[data-test=\"google-button\"]"),
        }),
        // === 韩国平台 ===
        "naver_blog" => Some(PlatformConfig {
            name: "Naver Blog",
            login_url: "https://nid.naver.com/nidlogin.login",
            register_url: "https://nid.naver.com/user2/join/agreement",
            google_oauth: false,
            google_button_selector: None,
        }),
        // === 俄罗斯平台 ===
        "habr" => Some(PlatformConfig {
            name: "Habr",
            login_url: "https://habr.com/ru/login/",
            register_url: "https://habr.com/ru/register/",
            google_oauth: true,
            google_button_selector: Some("a[href*=\"auth/google\"]"),
        }),
        "vk" => Some(PlatformConfig {
            name: "VK",
            login_url: "https://vk.com/login",
            register_url: "https://vk.com/join",
            google_oauth: false,
            google_button_selector: None,
        }),
        _ => None,
    }
}

// Unzoo CLI wrapper
fn get_unzoo_path() -> String {
    // Try user's local AppData first (most common installation)
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        // Check various possible locations
        let local_paths = [
            format!(r"{}\Unzoo\unzoo.exe", local_app_data),
            format!(r"{}\Unzoo\Unzoo.exe", local_app_data),
            format!(r"{}\Unzoo Browser\unzoo.exe", local_app_data),
            format!(r"{}\Unzoo Browser\Unzoo.exe", local_app_data),
            format!(r"{}\Unzoo\services\unzoo.exe", local_app_data),
            format!(r"{}\Programs\Unzoo\unzoo.exe", local_app_data),
            format!(r"{}\Programs\Unzoo Browser\unzoo.exe", local_app_data),
        ];
        for path in local_paths {
            if std::path::Path::new(&path).exists() {
                return path;
            }
        }
    }

    // Try common Program Files locations
    let paths = [
        r"C:\Program Files\Unzoo Browser\unzoo.exe",
        r"C:\Program Files\Unzoo Browser\Unzoo.exe",
        r"C:\Program Files\Unzoo Browser\services\unzoo.exe",
        r"C:\Program Files (x86)\Unzoo Browser\unzoo.exe",
        r"C:\Program Files (x86)\Unzoo Browser\services\unzoo.exe",
        r"C:\Program Files\Unzoo\unzoo.exe",
        r"C:\Program Files (x86)\Unzoo\unzoo.exe",
    ];

    for path in paths {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }

    // Fallback to PATH
    "unzoo".to_string()
}

// Tauri command to get detected Unzoo path for UI display
#[tauri::command]
fn detect_unzoo_path() -> Result<String, String> {
    let path = get_unzoo_path();
    // Check if path exists (not just "unzoo" fallback)
    if path == "unzoo" {
        // Check if unzoo is in PATH
        let output = std::process::Command::new("where")
            .arg("unzoo")
            .output();
        match output {
            Ok(o) if o.status.success() => {
                let found = String::from_utf8_lossy(&o.stdout);
                let first_line = found.lines().next().unwrap_or("unzoo");
                Ok(first_line.trim().to_string())
            }
            _ => {
                // Path not found, but check if API is available
                let client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(2))
                    .build();
                if let Ok(client) = client {
                    if let Ok(resp) = client.get("http://127.0.0.1:9399/api/v1/profiles").send() {
                        if resp.status().is_success() {
                            return Ok("API: http://127.0.0.1:9399 (已连接)".to_string());
                        }
                    }
                }
                Err("Unzoo not found".to_string())
            }
        }
    } else if std::path::Path::new(&path).exists() {
        Ok(path)
    } else {
        Err("Unzoo not found".to_string())
    }
}

// Windows: CREATE_NO_WINDOW flag to hide console
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x08000000;

fn unzoo_navigate(url: &str) -> Result<(), String> {
    log::info!("[UNZOO] Navigating to: {}", url);

    // Use HTTP API - correct endpoint is /navigate (not /browser/navigate or /tabs/navigate)
    let client = get_blocking_client();
    let api_url = format!("{}/navigate", UNZOO_API_BASE);

    // Get current active tab or use the stored one
    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab. Please launch a profile first.".to_string());
    }

    let body = serde_json::json!({
        "tab_id": tab_id,
        "url": url
    });

    let resp = client.post(&api_url)
        .json(&body)
        .send()
        .map_err(|e| format!("Failed to navigate: {}", e))?;

    if resp.status().is_success() {
        log::info!("[UNZOO] Navigation successful");
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        Err(format!("Navigate failed: {} - {}", status, body))
    }
}

/// 调用 Unzoo MCP 工具（/mcp/tools/call）。v1.8.4 的 browser_* 工具发出 isTrusted=true 的
/// 真实键鼠事件（每字符 WebKeyboardEvent、5 步真实滚轮、真实 mouse-move），比裸 REST 更像真人、
/// 更难被平台风控识别为自动化。
fn unzoo_mcp(name: &str, args: serde_json::Value) -> Result<String, String> {
    let client = get_blocking_client();
    let url = format!("{}/mcp/tools/call", UNZOO_API_BASE);
    let resp = client.post(&url)
        .json(&serde_json::json!({ "name": name, "arguments": args }))
        .send()
        .map_err(|e| format!("{} 请求失败: {}", name, e))?;
    if !resp.status().is_success() {
        return Err(format!("{} HTTP {}", name, resp.status()));
    }
    let v: serde_json::Value = resp.json().unwrap_or_default();
    let txt = v.get("content").and_then(|c| c.get(0)).and_then(|c| c.get("text"))
        .and_then(|t| t.as_str()).unwrap_or("").to_string();
    if v.get("isError").and_then(|b| b.as_bool()).unwrap_or(false) {
        return Err(format!("{}: {}", name, txt));
    }
    Ok(txt)
}

/// 拟人每字符打字间隔（毫秒）~45–110ms。
pub(crate) fn human_type_delay_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0) as i64;
    45 + (n % 65)
}

/// 全局输入模式开关：true=Unzoo 原生 human 模式（默认），false=旧 browser_*。
/// 由底层 sync 助手读取（无 DB 上下文），启动时从 config.unzoo_input_mode 载入。
static UNZOO_HUMAN_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn unzoo_human_mode_enabled() -> bool {
    UNZOO_HUMAN_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// 设置输入模式（运行时切换；持久化由调用方写 config）。
fn unzoo_set_human_mode(on: bool) {
    UNZOO_HUMAN_MODE.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// 运行时切换输入模式并持久化到 config（安全开关：human 翻车可一键切回 browser）。
#[tauri::command]
fn set_unzoo_input_mode(state: State<AppState>, mode: String) -> Result<(), String> {
    let m = if mode == "browser" { "browser" } else { "human" };
    unzoo_set_human_mode(m != "browser");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    engine_cfg_set(&conn, "unzoo_input_mode", m);
    log::info!("[UNZOO] 输入模式切换为 {}", m);
    Ok(())
}

/// 首次真实使用 human 模式时懒设全局行为档位（启动时浏览器可能未就绪，故懒设）。
fn ensure_human_profile() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        match unzoo_mcp("human_profile_set", serde_json::json!({ "profile": "normal" })) {
            Ok(_) => log::info!("[HUMAN] 全局行为档位已设为 normal"),
            Err(e) => log::warn!("[HUMAN] human_profile_set 失败（首次将用 Unzoo 默认档）: {}", e),
        }
    });
}

/// human 模式点击：bezier 轨迹 + 遮挡检测 + 反馈校验（v1.9.0）。selector 走 CSS（最高优先级）。
fn unzoo_human_click(selector: &str) -> Result<(), String> {
    log::info!("[HUMAN] Clicking: {}", selector);
    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab".to_string());
    }
    ensure_human_profile();
    unzoo_mcp("human_click", serde_json::json!({
        "tab_id": tab_id, "selector": selector
    })).map(|_| ())
}

fn unzoo_click(selector: &str) -> Result<(), String> {
    if unzoo_human_mode_enabled() {
        return unzoo_human_click(selector);
    }
    log::info!("[UNZOO] Clicking (trusted) selector: {}", selector);
    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab".to_string());
    }
    // browser_click：自动等待元素 + 真实鼠标事件 + 支持 text=/role=/data-testid=/ref= 选择器
    unzoo_mcp("browser_click", serde_json::json!({
        "tab_id": tab_id, "selector": selector, "timeout": 8000
    })).map(|_| ())
}

fn unzoo_get_text() -> Result<String, String> {
    log::info!("[UNZOO] Getting page text");

    let client = get_blocking_client();
    let api_url = format!("{}/get-text", UNZOO_API_BASE);

    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab".to_string());
    }

    let body = serde_json::json!({
        "tab_id": tab_id,
        "selector": "body"
    });

    let resp = client.post(&api_url)
        .json(&body)
        .send()
        .map_err(|e| format!("Get text failed: {}", e))?;

    if resp.status().is_success() {
        let result: serde_json::Value = resp.json().unwrap_or_default();
        Ok(result.get("data").and_then(|d| d.get("text")).map(|t| t.as_str().unwrap_or("").to_string()).unwrap_or_default())
    } else {
        Err(format!("Get text failed: {}", resp.status()))
    }
}

/// 取某个选择器命中元素的纯文本（用于发布后校验编辑器是否已清空等）。
fn unzoo_get_text_sel(selector: &str) -> Result<String, String> {
    let client = get_blocking_client();
    let api_url = format!("{}/get-text", UNZOO_API_BASE);
    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() { return Err("No active tab".to_string()); }
    let resp = client.post(&api_url)
        .json(&serde_json::json!({ "tab_id": tab_id, "selector": selector }))
        .send().map_err(|e| format!("Get text failed: {}", e))?;
    if resp.status().is_success() {
        let result: serde_json::Value = resp.json().unwrap_or_default();
        Ok(result.get("data").and_then(|d| d.get("text")).map(|t| t.as_str().unwrap_or("").to_string()).unwrap_or_default())
    } else {
        Err(format!("Get text failed: {}", resp.status()))
    }
}

/// 把本地文件塞进 `<input type=file>`（等价 Playwright set_input_files）。
/// 借鉴 social-auto-upload 的视频/图文上传：抖音/小红书/视频号均靠此上传媒体。
/// 走 Unzoo `POST /api/v1/upload`，得到的是 trusted File 对象（实测可用）。
fn unzoo_upload(selector: &str, file_paths: &[String]) -> Result<usize, String> {
    log::info!("[UNZOO] Uploading {} file(s) -> {}", file_paths.len(), selector);
    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() { return Err("No active tab".to_string()); }
    if file_paths.is_empty() { return Err("无文件可上传".to_string()); }
    // 校验文件存在，避免上传空指针
    for p in file_paths {
        if !std::path::Path::new(p).exists() {
            return Err(format!("文件不存在: {}", p));
        }
    }
    let client = get_blocking_client();
    let api_url = format!("{}/upload", UNZOO_API_BASE);
    let body = serde_json::json!({
        "tab_id": tab_id,
        "selector": selector,
        "file_paths": file_paths,
    });
    let resp = client.post(&api_url).json(&body).send()
        .map_err(|e| format!("upload 请求失败: {}", e))?;
    if !resp.status().is_success() {
        let st = resp.status();
        return Err(format!("upload 失败: {} - {}", st, resp.text().unwrap_or_default()));
    }
    let v: serde_json::Value = resp.json().unwrap_or_default();
    if !v.get("success").and_then(|b| b.as_bool()).unwrap_or(false) {
        return Err(format!("upload 失败: {}", v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown")));
    }
    let count = v.get("data").and_then(|d| d.get("count")).and_then(|c| c.as_u64()).unwrap_or(file_paths.len() as u64);
    Ok(count as usize)
}

/// 轮询页面文本，等到出现 `needle`（或超时）。
/// 借鉴 social-auto-upload 靠"重新上传"文本判定上传完成的做法；Unzoo 暂无 wait-for-text，故轮询 /get-text。
/// `fail_needle` 命中则立即返回 Err（如"上传失败"）。
fn unzoo_wait_text(needle: &str, fail_needle: Option<&str>, timeout_secs: u64) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        if let Ok(txt) = unzoo_get_text() {
            if let Some(f) = fail_needle {
                if !f.is_empty() && txt.contains(f) {
                    return Err(format!("页面出现失败标记: {}", f));
                }
            }
            if txt.contains(needle) { return Ok(()); }
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("等待文本超时（{}s）: {}", timeout_secs, needle));
        }
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }
}

// Database state
pub(crate) struct AppState {
    db: Mutex<Connection>,
}

// Data types
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub url: String,
    pub tagline: Option<String>,
    pub description: Option<String>,
    pub product_type: String,
    pub priority: i32,
    pub weight: i32,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub platform: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub status: String,
    pub created_at: String,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub health_status: Option<String>,
    #[serde(default)]
    pub total_nurture_seconds: i64,
    #[serde(default)]
    pub last_nurture_at: Option<String>,
    #[serde(default)]
    pub persona_id: Option<String>,      // 所属身份(Gmail)
    #[serde(default)]
    pub persona_email: Option<String>,   // 所属身份的 Gmail（展示用）
    #[serde(default)]
    pub login_method: Option<String>,    // google | phone | password（判断是否可转移归属）
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GmailStatus {
    pub connected: bool,
    pub email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistrationResult {
    pub success: bool,
    pub platform: String,
    pub username: Option<String>,
    pub error: Option<String>,
    pub needs_manual_verification: bool,
    pub verification_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub id: String,
    pub name: String,
    pub phone: bool,
    pub google_oauth: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Stats {
    pub total_posts: i32,
    pub total_views: i32,
    pub total_engagements: i32,
    pub avg_engagement_rate: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: String,
    pub posts: i32,
    pub views: i32,
    pub engagements: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContentBreakdown {
    pub articles: i32,
    pub replies: i32,
    pub reposts: i32,
    pub comments: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BestContent {
    pub id: String,
    pub title: String,
    pub content: String,
    pub platform: String,
    pub views: i32,
    pub engagements: i32,
    pub published_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HeatmapData {
    pub date: String,
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DetailedStats {
    pub total_posts: i32,
    pub total_views: i32,
    pub total_engagements: i32,
    pub avg_engagement_rate: f64,
    pub posts_change: i32,
    pub views_change: i32,
    pub engagements_change: i32,
    pub rate_change: f64,
    pub daily_data: Vec<DailyStats>,
    pub platform_stats: HashMap<String, i32>,
    pub content_breakdown: ContentBreakdown,
    pub best_content: Vec<BestContent>,
    pub heatmap_data: Vec<HeatmapData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub ai: Option<AiConfig>,
    pub browser: Option<BrowserConfig>,
    pub scheduler: Option<SchedulerConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub unzoo_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub mode: Option<String>,
    pub interval_minutes: Option<i32>,
    pub max_daily_posts: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiProvider {
    pub name: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub platform: String,
    pub language: String,
    pub product_id: String,
    pub product_name: String,
    pub body: String,
    pub hashtags: Vec<String>,
    #[serde(default)]
    pub images: Vec<String>,  // 图片路径列表
    #[serde(default)]
    pub account_id: Option<String>,  // 使用的账号ID，用于获取对应的 Browser Profile
}

// 发布预览结果
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PublishPreview {
    pub platform: String,
    pub content: String,
    pub images: Vec<String>,
    pub screenshot: Option<String>,  // Base64 截图
    pub ready: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeResult {
    pub name: String,
    pub url: String,
    pub tagline: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub product_type: Option<String>,
}

// 关键词监控
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Keyword {
    pub id: String,
    pub product_id: Option<String>,
    pub keyword: String,
    pub platforms: Vec<String>,
    pub enabled: bool,
}

// 发现的帖子
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiscoveredPost {
    pub id: String,
    pub platform: String,
    pub post_url: String,
    pub post_title: Option<String>,
    pub post_content: Option<String>,
    pub keyword_matched: Option<String>,
    pub relevance_score: f64,
    pub status: String,  // new, replied, skipped, irrelevant
    pub discovered_at: String,
}

// 回复结果
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplyResult {
    pub success: bool,
    pub platform: String,
    pub post_url: String,
    pub reply_content: Option<String>,
    pub error: Option<String>,
}

// 回复策略状态
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplyStrategy {
    pub platform: String,
    pub max_daily: i32,
    pub min_interval: i32,
    pub replies_today: i32,
    pub last_reply_time: Option<String>,
    pub can_reply_now: bool,
    pub wait_minutes: i32,
    pub discovered_posts: i32,
}

// Get database path
fn get_db_path() -> PathBuf {
    let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let app_dir = data_dir.join("unmarket");
    std::fs::create_dir_all(&app_dir).ok();
    app_dir.join("unmarket.db")
}

// Initialize database
fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS products (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            tagline TEXT,
            description TEXT,
            product_type TEXT DEFAULT 'tool',
            priority INTEGER DEFAULT 5,
            weight INTEGER DEFAULT 5,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            platform TEXT NOT NULL,
            username TEXT,
            email TEXT,
            credentials TEXT,
            profile_id TEXT,
            is_active INTEGER DEFAULT 1,
            status TEXT DEFAULT 'active',
            health_score INTEGER DEFAULT 100,
            warmup_stage TEXT DEFAULT 'none',
            proxy_id TEXT,
            last_login TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            nurture_started_at TEXT,
            total_nurture_seconds INTEGER DEFAULT 0,
            last_nurture_at TEXT
        );

        CREATE TABLE IF NOT EXISTS config (
            key TEXT PRIMARY KEY,
            value TEXT
        );

        CREATE TABLE IF NOT EXISTS session_state (
            key TEXT PRIMARY KEY,
            value TEXT,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS publish_history (
            id TEXT PRIMARY KEY,
            product_id TEXT,
            account_id TEXT,
            campaign_id TEXT,
            task_id TEXT,
            platform TEXT,
            language TEXT,
            post_type TEXT,
            content_type TEXT DEFAULT 'article',
            title TEXT,
            content TEXT,
            post_url TEXT,
            status TEXT DEFAULT 'pending',
            views INTEGER DEFAULT 0,
            engagements INTEGER DEFAULT 0,
            clicks INTEGER DEFAULT 0,
            error_message TEXT,
            published_at TEXT DEFAULT CURRENT_TIMESTAMP,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 关键词监控
        CREATE TABLE IF NOT EXISTS keywords (
            id TEXT PRIMARY KEY,
            product_id TEXT,
            keyword TEXT NOT NULL,
            platforms TEXT,
            enabled INTEGER DEFAULT 1,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 发现的相关帖子
        CREATE TABLE IF NOT EXISTS discovered_posts (
            id TEXT PRIMARY KEY,
            platform TEXT NOT NULL,
            post_url TEXT NOT NULL UNIQUE,
            post_title TEXT,
            post_content TEXT,
            keyword_matched TEXT,
            relevance_score REAL DEFAULT 0,
            status TEXT DEFAULT 'new',
            discovered_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 回复历史
        CREATE TABLE IF NOT EXISTS reply_history (
            id TEXT PRIMARY KEY,
            post_id TEXT,
            platform TEXT,
            post_url TEXT,
            reply_content TEXT,
            product_mentioned TEXT,
            status TEXT DEFAULT 'sent',
            engagement INTEGER DEFAULT 0,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 营销活动
        CREATE TABLE IF NOT EXISTS campaigns (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            product_id TEXT,
            platforms TEXT,
            status TEXT DEFAULT 'draft',
            schedule_type TEXT DEFAULT 'immediate',
            schedule_config TEXT,
            total_tasks INTEGER DEFAULT 0,
            completed_tasks INTEGER DEFAULT 0,
            started_at TEXT,
            completed_at TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 任务队列
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            campaign_id TEXT,
            task_type TEXT NOT NULL,
            platform TEXT,
            account_id TEXT,
            content TEXT,
            target_url TEXT,
            status TEXT DEFAULT 'pending',
            retry_count INTEGER DEFAULT 0,
            error_message TEXT,
            scheduled_at TEXT,
            started_at TEXT,
            completed_at TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 调度任务
        CREATE TABLE IF NOT EXISTS scheduled_jobs (
            id TEXT PRIMARY KEY,
            job_type TEXT NOT NULL,
            config TEXT,
            cron_expr TEXT,
            enabled INTEGER DEFAULT 1,
            last_run TEXT,
            next_run TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 代理池
        CREATE TABLE IF NOT EXISTS proxies (
            id TEXT PRIMARY KEY,
            name TEXT,
            protocol TEXT NOT NULL,
            host TEXT NOT NULL,
            port INTEGER NOT NULL,
            username TEXT,
            password TEXT,
            tags TEXT,
            status TEXT DEFAULT 'active',
            in_use INTEGER DEFAULT 0,
            last_tested TEXT,
            latency_ms INTEGER,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 熔断器状态（遇错即停）
        CREATE TABLE IF NOT EXISTS circuit_breakers (
            platform TEXT PRIMARY KEY,
            tripped INTEGER DEFAULT 0,
            consecutive_errors INTEGER DEFAULT 0,
            last_error_time TEXT,
            last_error_message TEXT,
            severity TEXT,
            cooldown_minutes INTEGER DEFAULT 60,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 每日操作计数（限制每日操作上限）
        CREATE TABLE IF NOT EXISTS daily_limits (
            id TEXT PRIMARY KEY,
            platform TEXT NOT NULL,
            account_id TEXT,
            date TEXT NOT NULL,
            posts_count INTEGER DEFAULT 0,
            replies_count INTEGER DEFAULT 0,
            likes_count INTEGER DEFAULT 0,
            follows_count INTEGER DEFAULT 0,
            UNIQUE(platform, account_id, date)
        );

        -- 养号策略表 (按平台配置)
        CREATE TABLE IF NOT EXISTS nurture_strategies (
            platform TEXT PRIMARY KEY,
            warmup_days INTEGER DEFAULT 14,
            growth_days INTEGER,                    -- 成长期时长；NULL 时按 warmup_days 兜底（= 旧的 2×warmup 行为）
            daily_sessions_min INTEGER DEFAULT 2,
            daily_sessions_max INTEGER DEFAULT 5,
            session_duration_min INTEGER DEFAULT 60,
            session_duration_max INTEGER DEFAULT 300,
            active_hours_start INTEGER DEFAULT 9,
            active_hours_end INTEGER DEFAULT 22,
            enabled INTEGER DEFAULT 1,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- 每日养号记录表
        CREATE TABLE IF NOT EXISTS nurture_daily_logs (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            date TEXT NOT NULL,
            sessions_completed INTEGER DEFAULT 0,
            total_seconds INTEGER DEFAULT 0,
            session_details TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(account_id, date)
        );

        -- 养号会话记录表
        CREATE TABLE IF NOT EXISTS nurture_sessions (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            duration_seconds INTEGER DEFAULT 0,
            status TEXT DEFAULT 'running',
            actions_performed TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        -- GitHub 养号动作日志（节奏控制 + 去重 + 跨账号去同质化）
        CREATE TABLE IF NOT EXISTS gh_actions_log (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            action_type TEXT NOT NULL,   -- star | follow | watch | comment
            target TEXT NOT NULL,        -- repo/user/thread 的 URL
            date TEXT NOT NULL,          -- YYYY-MM-DD
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_gh_actions_acct ON gh_actions_log(account_id, action_type);
        CREATE INDEX IF NOT EXISTS idx_gh_actions_target ON gh_actions_log(target);

        -- X 养号动作日志（节奏控制 + 去重 + 跨账号去同质化）
        CREATE TABLE IF NOT EXISTS x_actions_log (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            action_type TEXT NOT NULL,   -- like | follow | retweet | reply | tweet
            target TEXT NOT NULL,        -- tweet permalink / @handle profile url
            date TEXT NOT NULL,          -- YYYY-MM-DD
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_x_actions_acct ON x_actions_log(account_id, action_type);
        CREATE INDEX IF NOT EXISTS idx_x_actions_target ON x_actions_log(target);
        "
    )
}

// Run database migrations for existing databases
fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
    println!("[DB] Starting database migrations...");

    // Check if profile_id column exists in accounts table
    let has_profile_id: bool = conn
        .prepare("SELECT profile_id FROM accounts LIMIT 1")
        .is_ok();

    println!("[DB] profile_id column exists: {}", has_profile_id);

    if !has_profile_id {
        println!("[DB] Running migration: adding profile_id column to accounts table");
        match conn.execute("ALTER TABLE accounts ADD COLUMN profile_id TEXT", []) {
            Ok(_) => println!("[DB] Successfully added profile_id column"),
            Err(e) => println!("[DB] Failed to add profile_id column: {}", e),
        }
    }

    // Check if nurture columns exist in accounts table
    let has_nurture_columns: bool = conn
        .prepare("SELECT nurture_started_at FROM accounts LIMIT 1")
        .is_ok();

    if !has_nurture_columns {
        log::info!("[DB] Running migration: adding nurture columns to accounts table");
        // Add nurture columns if they don't exist
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN nurture_started_at TEXT", []);
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN total_nurture_seconds INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN last_nurture_at TEXT", []);
    }

    // GitHub 养号：所选领域（JSON 数组）。幂等，已存在则忽略错误。
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN gh_domains TEXT", []);
    // X 养号：所选方向（JSON 数组）
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN x_niches TEXT", []);
    // SegmentFault 养号：所选领域（JSON 数组）
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN sf_domains TEXT", []);
    // 养号「成长期时长」独立可配；老库补列，NULL 时各查询用 COALESCE(growth_days, warmup_days) 兜底
    let _ = conn.execute("ALTER TABLE nurture_strategies ADD COLUMN growth_days INTEGER", []);

    // Check if nurture_strategies table exists and add default strategies
    let has_nurture_strategies: bool = conn
        .prepare("SELECT platform FROM nurture_strategies LIMIT 1")
        .is_ok();

    if has_nurture_strategies {
        // Insert default strategies if empty
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM nurture_strategies", [], |row| row.get(0)).unwrap_or(0);
        if count == 0 {
            log::info!("[DB] Adding default nurture strategies");
            // Default strategies for common platforms
            let strategies = [
                ("twitter", 14, 2, 4, 60, 180, 9, 22),
                ("x", 14, 2, 4, 60, 180, 9, 22),
                ("reddit", 14, 2, 5, 60, 300, 9, 23),
                ("linkedin", 14, 1, 3, 60, 120, 8, 20),
                ("xiaohongshu", 14, 3, 6, 60, 180, 10, 23),
                ("zhihu", 14, 2, 4, 60, 180, 9, 22),
                ("weibo", 14, 2, 5, 60, 180, 9, 23),
                ("vk", 14, 2, 4, 60, 180, 10, 22),
                ("facebook", 14, 2, 4, 60, 180, 9, 22),
                ("instagram", 14, 3, 5, 60, 180, 10, 23),
                ("tiktok", 14, 3, 6, 60, 180, 10, 24),
                ("youtube", 14, 1, 3, 120, 300, 9, 22),
            ];
            for (platform, warmup_days, sessions_min, sessions_max, duration_min, duration_max, hours_start, hours_end) in strategies {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO nurture_strategies (platform, warmup_days, daily_sessions_min, daily_sessions_max, session_duration_min, session_duration_max, active_hours_start, active_hours_end) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![platform, warmup_days, sessions_min, sessions_max, duration_min, duration_max, hours_start, hours_end],
                );
            }
        }

        // 一次性：按平台差异化 warmup_days（替代原来一刀切 14）。flag 守护，只跑一次，
        // 不覆盖用户后续在设置里手改的值。已有行只更 warmup_days；缺的平台补一条（其余列走表默认）。
        if engine_cfg_get(&conn, "nurture_warmup_v4_seeded").is_none() {
            for (platform, days) in NURTURE_WARMUP_DAYS {
                let updated = conn.execute(
                    "UPDATE nurture_strategies SET warmup_days=?2 WHERE platform=?1",
                    params![platform, days]).unwrap_or(0);
                if updated == 0 {
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO nurture_strategies (platform, warmup_days) VALUES (?1, ?2)",
                        params![platform, days]);
                }
            }
            engine_cfg_set(&conn, "nurture_warmup_v4_seeded", "1");
        }

        // 一次性：SegmentFault 养号节奏定为「预热 3 天 + 成长 7 天」（总周期 10 天）。
        // flag 守护只跑一次，不覆盖用户后续手改；缺行则补一条。
        if engine_cfg_get(&conn, "nurture_segmentfault_cadence_seeded").is_none() {
            let updated = conn.execute(
                "UPDATE nurture_strategies SET warmup_days=3, growth_days=7 WHERE platform='segmentfault'",
                []).unwrap_or(0);
            if updated == 0 {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO nurture_strategies (platform, warmup_days, growth_days) VALUES ('segmentfault', 3, 7)",
                    []);
            }
            engine_cfg_set(&conn, "nurture_segmentfault_cadence_seeded", "1");
        }
    }

    // Check if selected_browser_profile exists in config
    let has_profile_config: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM config WHERE key = 'selected_browser_profile'",
            [],
            |row| row.get::<_, i32>(0),
        )
        .unwrap_or(0) > 0;

    if !has_profile_config {
        log::info!("[DB] Adding default browser profile config");
        let _ = conn.execute(
            "INSERT OR IGNORE INTO config (key, value) VALUES ('selected_browser_profile', '')",
            [],
        );
    }

    // Migration: store the real URL of our own published posts so Type-A
    // (reply_mention) can later monitor their comment threads.
    let has_post_url: bool = conn.prepare("SELECT post_url FROM publish_history LIMIT 1").is_ok();
    if !has_post_url {
        log::info!("[DB] Running migration: adding post_url column to publish_history");
        let _ = conn.execute("ALTER TABLE publish_history ADD COLUMN post_url TEXT", []);
    }

    // Migration: inbound comments on our own posts (source for reply_mention).
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS inbound_comments (
            id TEXT PRIMARY KEY,
            our_post_url TEXT NOT NULL,
            platform TEXT NOT NULL,
            comment_id TEXT,
            comment_author TEXT,
            comment_text TEXT,
            comment_permalink TEXT,
            account_id TEXT,
            status TEXT DEFAULT 'new',          -- new | replied | skipped
            discovered_at TEXT DEFAULT CURRENT_TIMESTAMP,
            replied_at TEXT,
            UNIQUE(our_post_url, comment_id)
        )",
        [],
    );

    // Migration: reply_history 增列，支撑半自动审核队列。
    //   reason       —— LLM 判定理由（为何相关/是否提产品）
    //   account_id   —— 批准后需用对应账号 profile 发布
    //   reply_type   —— keyword | mention，审核时区分场景
    // status 取值扩展：pending_review | approved | sent | rejected | failed
    for col in [
        "ALTER TABLE reply_history ADD COLUMN reason TEXT",
        "ALTER TABLE reply_history ADD COLUMN account_id TEXT",
        "ALTER TABLE reply_history ADD COLUMN reply_type TEXT DEFAULT 'keyword'",
        // locator：无永久链接的平台（如 LinkedIn 信息流）批准时据此在实时信息流里重新定位帖子
        "ALTER TABLE reply_history ADD COLUMN locator TEXT",
    ] {
        let _ = conn.execute(col, []); // 已存在则忽略
    }

    // 回复模式默认半自动审核（更安全，避免无人盯防时刷屏被封）。
    let _ = conn.execute(
        "INSERT OR IGNORE INTO config (key, value) VALUES ('engine_reply_mode', 'review')",
        [],
    );

    // P0-3 账号健康监测：accounts 增列。
    //   health_status: unknown | healthy | logged_out | shadowbanned | banned | locked | restricted
    for col in [
        "ALTER TABLE accounts ADD COLUMN health_status TEXT DEFAULT 'unknown'",
        "ALTER TABLE accounts ADD COLUMN karma INTEGER DEFAULT 0",
        "ALTER TABLE accounts ADD COLUMN followers INTEGER DEFAULT 0",
        "ALTER TABLE accounts ADD COLUMN last_health_check TEXT",
    ] {
        let _ = conn.execute(col, []); // 已存在则忽略
    }

    // P1-4 买家意向打分：帖子+回复各记 0-100 意向分。
    let _ = conn.execute("ALTER TABLE discovered_posts ADD COLUMN intent_score INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE reply_history ADD COLUMN intent_score INTEGER DEFAULT 0", []);
    // 默认意向阈值：低于此分判定为低购买意向、不浪费回复。
    let _ = conn.execute("INSERT OR IGNORE INTO config (key, value) VALUES ('engine_intent_min', '40')", []);
    // 市场提交时表单必填的提交者邮箱
    let _ = conn.execute("INSERT OR IGNORE INTO config (key, value) VALUES ('submitter_email', '')", []);
    // 养号期保护（默认开）：新号自动只养号、不对外，养熟自动解锁
    let _ = conn.execute("INSERT OR IGNORE INTO config (key, value) VALUES ('engine_warmup_gate', '1')", []);

    // === 市场提交（MCP/Skill 上架各大市场）===
    // products 补充上架所需字段
    let _ = conn.execute("ALTER TABLE products ADD COLUMN repo_url TEXT", []);
    let _ = conn.execute("ALTER TABLE products ADD COLUMN install_cmd TEXT", []);
    // 市场目录
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS marketplaces (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,            -- mcp | skill | both
            submit_method TEXT NOT NULL,   -- form | github_pr | cli | auto_index
            submit_url TEXT,
            homepage TEXT,
            notes TEXT,
            enabled INTEGER DEFAULT 1
        )", [],
    );
    // 提交记录（每产品×每市场×类型 一条）
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS marketplace_submissions (
            id TEXT PRIMARY KEY,
            product_id TEXT NOT NULL,
            marketplace_id TEXT NOT NULL,
            kind TEXT NOT NULL,            -- mcp | skill
            status TEXT DEFAULT 'pending', -- pending | materials_ready | prefilled | submitted | listed | failed | skipped
            listing TEXT,                  -- AI 生成的上架资料
            submit_url TEXT,
            result_url TEXT,
            error TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(product_id, marketplace_id, kind)
        )", [],
    );
    // 种子：实测整理的主流 MCP / Skill 市场（2025）
    let seeds: &[(&str,&str,&str,&str,&str,&str)] = &[
        // id, name, kind, method, submit_url, notes
        ("official-mcp-registry","Official MCP Registry","mcp","cli","https://registry.modelcontextprotocol.io","mcp-publisher CLI + GitHub/DNS 命名空间认证，需 server.json"),
        ("mcp-so","mcp.so","mcp","form","https://mcp.so/submit","站内 Submit 按钮或提 GitHub issue（~2万+ 服务）"),
        ("smithery","Smithery","mcp","cli","https://smithery.ai/new","smithery CLI + 仓库内 smithery.yaml"),
        ("glama","Glama","mcp","auto_index","https://glama.ai/mcp/servers","自动从 GitHub 索引；可表单认领/管理"),
        ("pulsemcp","PulseMCP","mcp","form","https://www.pulsemcp.com/submit","导航栏 Submit 表单（每日更新 1.6万+）"),
        ("awesome-mcp","Awesome MCP Servers","mcp","github_pr","https://github.com/punkpeye/awesome-mcp-servers","提 PR 加条目，需 README+安装说明"),
        ("mcp-market","MCP Market","mcp","form","https://mcpmarket.com/submit","提交表单"),
        ("lobehub-mcp","LobeHub MCP","mcp","github_pr","https://github.com/lobehub/lobe-chat-agents","提 PR / 表单"),
        ("docker-mcp","Docker MCP Registry","mcp","github_pr","https://github.com/docker/mcp-registry","提 PR，审核后 Docker 构建签名发布"),
        ("anthropic-skills","anthropics/skills","skill","github_pr","https://github.com/anthropics/skills","官方 Skills 仓库提 PR"),
        ("skillsmp","SkillsMP","skill","auto_index","https://skillsmp.com","GitHub 上 SKILL.md 自动同步收录"),
        ("skillhub","SkillHub","skill","auto_index","https://www.skillhub.club","GitHub 自动索引（AI 评分）"),
        ("claude-marketplaces","Claude Marketplaces","both","auto_index","https://claudemarketplaces.com","每日从 GitHub 更新收录"),
        ("lobehub-skills","LobeHub Skills","skill","form","https://lobehub.com/skills","提交"),
        ("mcpmarket-skills","MCP Market (Skills)","skill","form","https://mcpmarket.com/tools/skills","Agent Skills 目录提交"),
    ];
    for (id,name,kind,method,url,notes) in seeds {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO marketplaces (id,name,kind,submit_method,submit_url,homepage,notes,enabled) VALUES (?1,?2,?3,?4,?5,?5,?6,1)",
            params![id,name,kind,method,url,notes]);
    }

    // P1-5 转化闭环 / 轻 CRM：每条真实发出的回复 = 一条线索，跟踪对方是否回应/转化。
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS leads (
            id TEXT PRIMARY KEY,
            reply_id TEXT,                      -- 关联 reply_history.id
            platform TEXT NOT NULL,
            author TEXT,                        -- 对方（帖子作者）
            post_url TEXT,
            our_reply TEXT,
            intent_score INTEGER DEFAULT 0,
            status TEXT DEFAULT 'engaged',      -- engaged | replied_back | converted | dismissed
            keyword TEXT,
            account_id TEXT,
            notes TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            last_checked_at TEXT,
            UNIQUE(reply_id)
        )",
        [],
    );

    // 原创内容发布（借鉴 social-auto-upload）：支持图文/视频媒体 + 定时发布，全程走 Unzoo。
    // 一行 = 一条原创内容投递到「一个平台 + 一个账号」，可立即发或排期。
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS posts (
            id TEXT PRIMARY KEY,
            product_id TEXT,
            platform TEXT NOT NULL,             -- twitter/x | linkedin | reddit | xiaohongshu | douyin ...
            account_id TEXT,                    -- 绑定账号（决定 profile/登录态）；空则继承全局
            title TEXT,                         -- 标题（部分平台需要：reddit/小红书/抖音）
            body TEXT,                          -- 正文
            topics TEXT,                        -- JSON 数组：话题/标签（#xxx）
            media_paths TEXT,                   -- JSON 数组：本地图片/视频绝对路径
            media_type TEXT DEFAULT 'none',     -- none | image | video
            extra TEXT,                         -- JSON：平台特有字段（如 reddit subreddit）
            status TEXT DEFAULT 'draft',        -- draft | scheduled | publishing | published | failed | canceled
            scheduled_at TEXT,                  -- 排期时间（UTC RFC3339）；空=立即
            published_at TEXT,
            result_url TEXT,
            error TEXT,
            task_id TEXT,                       -- 关联 tasks.id（入队后回填）
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    );

    // 成效追踪（搜索排名 + 品牌提及 + 可选 Trends），全程走 Unzoo 的专用 auto/Default profile。
    // metric_keywords = 要追踪的词；metrics = 时间序列采样点。
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS metric_keywords (
            id TEXT PRIMARY KEY,
            keyword TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'longtail', -- brand(守) | longtail(攻) | mention(品牌提及)
            target_domain TEXT DEFAULT 'doaipm.com',
            enabled INTEGER DEFAULT 1,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    );
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS metrics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            keyword TEXT NOT NULL,
            kind TEXT,                              -- brand | longtail | mention
            source TEXT NOT NULL,                   -- serp | mention | trends
            region TEXT DEFAULT 'us/zh-CN',
            value INTEGER,                          -- serp:排名位次(NULL=未进前N) | mention:独立域名数 | trends:0-100
            detail TEXT,                            -- JSON：top结果 / 命中域名 / trends状态
            captured_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    );
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_metrics_kw_time ON metrics(keyword, captured_at)",
        [],
    );

    // 首次种子：品牌词(守) + 长尾攻击词(攻) + 品牌提及。已存在则不重复插。
    let kw_count: i64 = conn.query_row("SELECT COUNT(*) FROM metric_keywords", [], |r| r.get(0)).unwrap_or(0);
    if kw_count == 0 {
        let seeds: &[(&str, &str)] = &[
            // 品牌词：应永远 #1，守住
            ("doaipm", "brand"),
            ("DO AI PM", "brand"),
            ("SoloMD", "brand"),
            ("Unterm", "brand"),
            ("unfetch", "brand"),
            // 长尾攻击词：想抢排名的内容词
            ("Claude Code 做产品", "longtail"),
            ("AI 产品经理 方法论", "longtail"),
            ("AI 产品经理 转型", "longtail"),
            ("言出法随 AI 编程", "longtail"),
            ("vibe coding 第三条路", "longtail"),
            // 品牌提及（精确匹配 + 排除自家域名 统计第三方域名数）
            ("doaipm", "mention"),
            ("DO AI PM", "mention"),
        ];
        for (kw, kind) in seeds {
            let _ = conn.execute(
                "INSERT INTO metric_keywords (id, keyword, kind, target_domain, enabled) VALUES (?1,?2,?3,'doaipm.com',1)",
                params![Uuid::new_v4().to_string(), kw, kind],
            );
        }
    }

    // 多账号隔离：persona（真实Gmail为单位）+ 节点池。见 docs/multi-account-architecture.md。
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS personas (
            id TEXT PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,         -- 真实 Gmail
            profile_id TEXT,                    -- Unzoo profile（path 末段，如 Profile_um_xxx）
            node_name TEXT,                     -- 绑定的机场节点名
            local_port INTEGER,                 -- mihomo listener 本地端口（= 专属出口IP）
            status TEXT DEFAULT 'active',       -- active | disabled
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    );
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS nodes (
            name TEXT PRIMARY KEY,              -- 机场节点名（listener.proxy 引用）
            region TEXT,
            type TEXT,                          -- vless | anytls | ...
            in_use INTEGER DEFAULT 0,
            last_seen TEXT
        )",
        [],
    );
    // accounts 增加 persona 外键（已存在则忽略）
    let has_persona_col: bool = conn.prepare("SELECT persona_id FROM accounts LIMIT 1").is_ok();
    if !has_persona_col {
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN persona_id TEXT", []);
    }
    // #13 personas 增加「IP 来源类型」维度：ip_mode(airport 机场轮换 / fixed 固定IP) + 固定代理 + 地区
    for (col, ddl) in [
        ("ip_mode",     "ALTER TABLE personas ADD COLUMN ip_mode TEXT DEFAULT 'airport'"),
        ("fixed_proxy", "ALTER TABLE personas ADD COLUMN fixed_proxy TEXT"),
        ("region",      "ALTER TABLE personas ADD COLUMN region TEXT"),
    ] {
        if conn.prepare(&format!("SELECT {} FROM personas LIMIT 1", col)).is_err() {
            let _ = conn.execute(ddl, []);
        }
    }
    // #19 accounts 增加「跳过养号」标记：成熟正常账号一键结束养号 → 标正常、不再自动养
    if conn.prepare("SELECT nurture_skip FROM accounts LIMIT 1").is_err() {
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN nurture_skip INTEGER DEFAULT 0", []);
    }
    // 一次性清理历史误报：没绑 profile/persona 却被标 logged_out 的账号，复位为 unknown（"待配置"而非"被封"）
    let _ = conn.execute(
        "UPDATE accounts SET health_status='unknown' \
         WHERE health_status='logged_out' AND (profile_id IS NULL OR profile_id='') AND persona_id IS NULL",
        []);

    Ok(())
}

// Commands

#[tauri::command]
fn list_products(state: State<AppState>) -> Result<Vec<Product>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, name, url, tagline, description, product_type, priority, weight, created_at FROM products ORDER BY priority DESC")
        .map_err(|e| e.to_string())?;

    let products = stmt.query_map([], |row| {
        Ok(Product {
            id: row.get(0)?,
            name: row.get(1)?,
            url: row.get(2)?,
            tagline: row.get(3)?,
            description: row.get(4)?,
            product_type: row.get::<_, Option<String>>(5)?.unwrap_or_else(|| "tool".to_string()),
            priority: row.get(6)?,
            weight: row.get(7)?,
            created_at: row.get(8)?,
        })
    }).map_err(|e| e.to_string())?;

    products.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// 从网站抓取产品描述
async fn fetch_website_description(url: &str) -> Option<String> {
    // 确保 URL 有协议前缀
    let fetch_url = if !url.starts_with("http://") && !url.starts_with("https://") {
        format!("https://{}", url)
    } else {
        url.to_string()
    };

    log::info!("[FETCH] Fetching website: {}", fetch_url);

    // 使用较短的超时，避免阻塞太久
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("[FETCH] Failed to create client: {}", e);
            return None;
        }
    };

    match client.get(&fetch_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
    {
        Ok(resp) => {
            match resp.text().await {
                Ok(html) => {
                    let mut website_info = Vec::new();

                    // 提取 title
                    if let Some(start) = html.find("<title>") {
                        if let Some(end) = html[start..].find("</title>") {
                            let title = html[start + 7..start + end].trim();
                            if !title.is_empty() {
                                website_info.push(format!("Title: {}", title));
                            }
                        }
                    }

                    // 提取 meta description
                    let desc_patterns = [
                        r#"meta name="description" content=""#,
                        r#"meta property="og:description" content=""#,
                    ];
                    for pattern in desc_patterns {
                        if let Some(start) = html.find(pattern) {
                            let content_start = start + pattern.len();
                            if let Some(end) = html[content_start..].find('"') {
                                let meta_desc = html[content_start..content_start + end].trim();
                                if !meta_desc.is_empty() && meta_desc.len() > 20 {
                                    website_info.push(format!("Description: {}", meta_desc));
                                    break;
                                }
                            }
                        }
                    }

                    // 提取 h1
                    if let Some(start) = html.find("<h1") {
                        if let Some(tag_end) = html[start..].find('>') {
                            let h1_start = start + tag_end + 1;
                            if let Some(end) = html[h1_start..].find("</h1>") {
                                let h1_text = html[h1_start..h1_start + end].to_string();
                                // 清理 HTML 标签
                                let mut clean = String::new();
                                let mut in_tag = false;
                                for c in h1_text.chars() {
                                    if c == '<' { in_tag = true; }
                                    else if c == '>' { in_tag = false; }
                                    else if !in_tag { clean.push(c); }
                                }
                                let h1_clean = clean.trim().to_string();
                                if !h1_clean.is_empty() && h1_clean.len() < 200 {
                                    website_info.push(format!("Headline: {}", h1_clean));
                                }
                            }
                        }
                    }

                    // 提取页面文本摘要
                    let mut text_content = String::new();
                    let mut in_tag = false;
                    let mut in_script = false;
                    let mut in_style = false;
                    let html_lower = html.to_lowercase();

                    for (i, c) in html.chars().enumerate() {
                        if i < html_lower.len() {
                            if html_lower[i..].starts_with("<script") { in_script = true; }
                            if html_lower[i..].starts_with("</script") { in_script = false; }
                            if html_lower[i..].starts_with("<style") { in_style = true; }
                            if html_lower[i..].starts_with("</style") { in_style = false; }
                        }

                        if c == '<' { in_tag = true; }
                        else if c == '>' { in_tag = false; text_content.push(' '); }
                        else if !in_tag && !in_script && !in_style {
                            text_content.push(c);
                        }

                        if text_content.len() > 1500 { break; }
                    }

                    // 清理并取前100个词
                    let text_content: String = text_content
                        .split_whitespace()
                        .take(150)
                        .collect::<Vec<_>>()
                        .join(" ");

                    if !text_content.is_empty() {
                        website_info.push(format!("Content: {}", text_content));
                    }

                    if !website_info.is_empty() {
                        let result = website_info.join("\n");
                        log::info!("Fetched website description: {} chars", result.len());
                        Some(result)
                    } else {
                        log::warn!("No useful content extracted from {}", fetch_url);
                        None
                    }
                }
                Err(e) => {
                    log::warn!("Failed to read response body from {}: {}", fetch_url, e);
                    None
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch website {}: {}", fetch_url, e);
            None
        }
    }
}

#[tauri::command]
async fn create_product(
    state: State<'_, AppState>,
    name: String,
    url: String,
    tagline: Option<String>,
    description: Option<String>,
    product_type: String,
    priority: i32,
    weight: i32,
) -> Result<Product, String> {
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();

    // 如果没有提供描述，自动从网站抓取
    let final_description = if description.as_ref().map(|d| d.trim().len()).unwrap_or(0) < 50 {
        log::info!("Auto-fetching website content for new product: {}", url);
        fetch_website_description(&url).await
    } else {
        description
    };

    // 保存到数据库
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO products (id, name, url, tagline, description, product_type, priority, weight, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, name, url, tagline, final_description, product_type, priority, weight, created_at],
        ).map_err(|e| e.to_string())?;
    }

    Ok(Product {
        id,
        name,
        url,
        tagline,
        description: final_description,
        product_type,
        priority,
        weight,
        created_at,
    })
}

#[tauri::command]
fn delete_product(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM products WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 用 Unzoo 真实浏览器打开网页，返回渲染后的 HTML（SPA/反爬站点 reqwest 拿不到内容）。
/// 失败返回空串，由调用方回退 reqwest。
async fn fetch_rendered_html(url: &str) -> String {
    let _ = ensure_browser_connected().await;
    if get_active_tab().is_none() {
        let prof = get_saved_browser_profile();
        if !prof.is_empty() {
            if let Ok(tid) = unzoo_launch_profile(prof).await { set_active_tab(Some(tid)); }
        }
    }
    if get_active_tab().is_none() { return String::new(); }
    let u = url.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        if unzoo_navigate(&u).is_err() { return String::new(); }
        std::thread::sleep(std::time::Duration::from_secs(4));
        let raw = unzoo_evaluate("document.documentElement.outerHTML.slice(0,200000)").unwrap_or_default();
        serde_json::from_str::<String>(&raw).unwrap_or(raw)
    }).await.unwrap_or_default()
}

#[tauri::command]
async fn analyze_url(state: State<'_, AppState>, url: String) -> Result<AnalyzeResult, String> {
    // 获取 AI 配置用于分析
    let (provider, api_key) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let get_value = |key: &str| -> Option<String> {
            conn.query_row("SELECT value FROM config WHERE key = ?1", params![key], |row| row.get(0)).ok()
        };
        let provider = get_value("ai.provider").unwrap_or_else(|| "gemini".to_string());
        let key = match provider.as_str() {
            "gemini" => get_value("ai.key.gemini"),
            "openai" => get_value("ai.key.openai"),
            "deepseek" => get_value("ai.key.deepseek"),
            "qwen" => get_value("ai.key.qwen"),
            _ => get_value("ai.key.gemini"),
        };
        (provider, key.unwrap_or_default())
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| e.to_string())?;

    // 优先用 Unzoo 真实浏览器拿"渲染后"的 HTML（reqwest 对 SPA/反爬常拿到空白页）
    log::info!("[ANALYZE] Fetching via Unzoo: {}", url);
    let mut html = fetch_rendered_html(&url).await;
    if html.trim().len() < 200 {
        log::info!("[ANALYZE] Unzoo content thin, falling back to reqwest");
        html = match client.get(&url).send().await {
            Ok(resp) => resp.text().await.unwrap_or_default(),
            Err(e) => { log::warn!("[ANALYZE] reqwest fallback failed: {}", e); html }
        };
    }

    // 提取基本信息
    let mut name = String::new();
    let mut tagline = String::new();
    let mut description = String::new();

    // 提取 title
    if let Some(start) = html.find("<title>") {
        if let Some(end) = html[start..].find("</title>") {
            name = html[start + 7..start + end].trim().to_string();
            // 移除常见后缀
            for suffix in [" - ", " | ", " – ", " — "] {
                if let Some(pos) = name.find(suffix) {
                    name = name[..pos].trim().to_string();
                    break;
                }
            }
        }
    }

    // 提取 meta description
    let desc_patterns = [
        r#"meta name="description" content=""#,
        r#"meta property="og:description" content=""#,
        r#"meta name="twitter:description" content=""#,
    ];
    for pattern in desc_patterns {
        if let Some(start) = html.find(pattern) {
            let content_start = start + pattern.len();
            if let Some(end) = html[content_start..].find('"') {
                description = html[content_start..content_start + end].trim().to_string();
                if !description.is_empty() {
                    break;
                }
            }
        }
    }

    // 提取 og:title 或 h1 作为 tagline
    if let Some(start) = html.find(r#"meta property="og:title" content=""#) {
        let content_start = start + 35;
        if let Some(end) = html[content_start..].find('"') {
            tagline = html[content_start..content_start + end].trim().to_string();
        }
    }
    if tagline.is_empty() {
        if let Some(start) = html.find("<h1") {
            if let Some(tag_end) = html[start..].find('>') {
                let h1_start = start + tag_end + 1;
                if let Some(end) = html[h1_start..].find("</h1>") {
                    tagline = html[h1_start..h1_start + end]
                        .replace("<br>", " ")
                        .replace("<br/>", " ")
                        .replace("&nbsp;", " ")
                        .trim()
                        .to_string();
                    // 移除 HTML 标签
                    let mut clean_tagline = String::new();
                    let mut in_tag = false;
                    for c in tagline.chars() {
                        if c == '<' { in_tag = true; }
                        else if c == '>' { in_tag = false; }
                        else if !in_tag { clean_tagline.push(c); }
                    }
                    tagline = clean_tagline.trim().to_string();
                }
            }
        }
    }

    // 如果有 AI API，使用 AI 来分析网页内容
    if !api_key.is_empty() && !html.is_empty() {
        // 提取纯文本内容（限制长度）
        // 安全去标签：按字符流处理，不做按字节切片（避免多字节 UTF-8 panic）
        let mut text_content = String::new();
        let mut in_tag = false;
        let mut in_script = false;
        let mut in_style = false;
        let mut tagbuf = String::new();
        for c in html.chars() {
            if c == '<' {
                in_tag = true;
                tagbuf.clear();
            } else if c == '>' {
                in_tag = false;
                let tl = tagbuf.to_lowercase();
                if tl.starts_with("script") { in_script = true; }
                else if tl.starts_with("/script") { in_script = false; }
                else if tl.starts_with("style") { in_style = true; }
                else if tl.starts_with("/style") { in_style = false; }
                text_content.push(' ');
            } else if in_tag {
                if tagbuf.chars().count() < 16 { tagbuf.push(c); }
            } else if !in_script && !in_style {
                text_content.push(c);
            }
            if text_content.len() > 5000 { break; }
        }

        // 清理文本
        let text_content: String = text_content
            .split_whitespace()
            .take(800)
            .collect::<Vec<_>>()
            .join(" ");

        let prompt = format!(
            r#"Analyze this website and extract product information.

URL: {}
Page Content:
{}

Extract and return in this exact format:
NAME: [Product/Company name, 1-5 words]
TAGLINE: [Short catchy description, max 15 words]
DESCRIPTION: [What the product does and its key features, 2-3 sentences]
TYPE: [One of: tool, saas, app, service, platform, other]

Be concise and accurate. Extract real information from the content."#,
            url, text_content
        );

        let ai_result = match provider.as_str() {
            "gemini" => call_gemini_api(&client, &api_key, &prompt).await,
            "openai" => call_openai_api(&client, &api_key, &prompt).await,
            "deepseek" => call_deepseek_api(&client, &api_key, &prompt).await,
            "qwen" => call_qwen_api(&client, &api_key, &prompt).await,
            _ => Err("No AI provider".to_string()),
        };

        if let Ok(ai_response) = ai_result {
            for line in ai_response.lines() {
                if line.starts_with("NAME:") {
                    name = line.trim_start_matches("NAME:").trim().to_string();
                } else if line.starts_with("TAGLINE:") {
                    tagline = line.trim_start_matches("TAGLINE:").trim().to_string();
                } else if line.starts_with("DESCRIPTION:") {
                    description = line.trim_start_matches("DESCRIPTION:").trim().to_string();
                }
            }
        }
    }

    // 如果仍然没有名称，从 URL 提取
    if name.is_empty() {
        name = url.split('/').nth(2).unwrap_or("Unknown")
            .replace("www.", "")
            .split('.')
            .next()
            .unwrap_or("Unknown")
            .to_string();
    }

    log::info!("Analyzed URL: name={}, tagline={}, desc_len={}", name, tagline, description.len());

    Ok(AnalyzeResult {
        name: name.chars().take(100).collect(),
        url,
        tagline: if tagline.is_empty() { None } else { Some(tagline.chars().take(200).collect()) },
        description: if description.is_empty() { None } else { Some(description.chars().take(1000).collect()) },
        product_type: Some("tool".to_string()),
    })
}

#[tauri::command]
fn list_accounts(state: State<AppState>) -> Result<Vec<Account>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT a.id, a.platform, a.username, a.email, a.status, a.created_at, a.profile_id, \
                COALESCE(a.health_status,'unknown'), COALESCE(a.total_nurture_seconds,0), a.last_nurture_at, \
                a.persona_id, p.email \
         FROM accounts a LEFT JOIN personas p ON p.id = a.persona_id ORDER BY a.created_at DESC")
        .map_err(|e| e.to_string())?;

    let accounts = stmt.query_map([], |row| {
        let platform: String = row.get(1)?;
        let google = get_platform_config(&platform).map(|c| c.google_oauth).unwrap_or(false);
        let login_method = Some(platform_login_method(&platform, google).to_string());
        Ok(Account {
            id: row.get(0)?,
            platform,
            username: row.get(2)?,
            email: row.get(3)?,
            status: row.get(4)?,
            created_at: row.get(5)?,
            profile_id: row.get(6)?,
            health_status: row.get(7)?,
            total_nurture_seconds: row.get(8)?,
            last_nurture_at: row.get(9)?,
            persona_id: row.get(10)?,
            persona_email: row.get(11)?,
            login_method,
        })
    }).map_err(|e| e.to_string())?;

    accounts.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// 把账号归属到某个身份(persona)；persona_id 为空=解除归属。归属后该账号自动用 persona 的 profile。
#[tauri::command]
fn set_account_persona(state: State<AppState>, account_id: String, persona_id: Option<String>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match persona_id.filter(|s| !s.is_empty()) {
        Some(pid) => conn.execute("UPDATE accounts SET persona_id=?1 WHERE id=?2", params![pid, account_id]),
        None => conn.execute("UPDATE accounts SET persona_id=NULL WHERE id=?1", params![account_id]),
    }.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn add_account(state: State<AppState>, platform: String, username: String, password: String) -> Result<Account, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO accounts (id, platform, username, credentials, status, created_at) VALUES (?1, ?2, ?3, ?4, 'active', ?5)",
        params![id, platform, username, password, created_at],
    ).map_err(|e| e.to_string())?;

    let google = get_platform_config(&platform).map(|c| c.google_oauth).unwrap_or(false);
    let login_method = Some(platform_login_method(&platform, google).to_string());
    Ok(Account {
        id,
        platform,
        username: Some(username),
        email: None,
        status: "active".to_string(),
        created_at,
        profile_id: None,
        health_status: Some("unknown".to_string()),
        total_nurture_seconds: 0,
        last_nurture_at: None,
        persona_id: None,
        persona_email: None,
        login_method,
    })
}

#[tauri::command]
fn delete_account(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_gmail_status(state: State<'_, AppState>) -> Result<GmailStatus, String> {
    // First check local database cache
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT value FROM session_state WHERE key = 'gmail_email'")
            .map_err(|e| e.to_string())?;
        let email: Option<String> = stmt.query_row([], |row| row.get(0)).ok();
        if email.is_some() {
            return Ok(GmailStatus { connected: true, email });
        }
    }

    // Try to check via Unzoo MCP - call cookie_get_all for gmail.com
    // This requires Unzoo MCP server to be running
    match check_unzoo_gmail_cookies().await {
        Ok(email) => {
            // Cache the result
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO session_state (key, value, updated_at) VALUES ('gmail_email', ?1, datetime('now'))",
                params![email],
            ).ok();
            Ok(GmailStatus { connected: true, email: Some(email) })
        }
        Err(_) => Ok(GmailStatus { connected: false, email: None })
    }
}

async fn check_unzoo_gmail_cookies() -> Result<String, String> {
    // Check Unzoo Local AppData path
    let unzoo_data = dirs::data_local_dir()
        .ok_or("No local data dir")?
        .join("Unzoo")
        .join("User Data")
        .join("Default");

    log::info!("Checking Unzoo path: {:?}", unzoo_data);

    if !unzoo_data.exists() {
        log::error!("Unzoo profile not found at {:?}", unzoo_data);
        return Err("No Unzoo profile found".to_string());
    }

    // Check if cookies exist (indicates active profile)
    let cookies_path = unzoo_data.join("Network").join("Cookies");
    log::info!("Checking cookies at: {:?}", cookies_path);

    if cookies_path.exists() {
        log::info!("Cookies found, Unzoo profile is active");
        return Ok("Gmail (via Unzoo)".to_string());
    }

    // Also check Login Data for Google accounts
    let login_data = unzoo_data.join("Login Data");
    if login_data.exists() {
        log::info!("Login Data found");
        return Ok("Gmail (via Unzoo)".to_string());
    }

    log::error!("No cookies or login data found");
    Err("Unzoo profile found but no login data".to_string())
}

#[tauri::command]
async fn setup_gmail(state: State<'_, AppState>) -> Result<GmailStatus, String> {
    // Check if already connected via Unzoo
    match check_unzoo_gmail_cookies().await {
        Ok(email) => {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO session_state (key, value, updated_at) VALUES ('gmail_email', ?1, datetime('now'))",
                params![email],
            ).map_err(|e| e.to_string())?;
            Ok(GmailStatus { connected: true, email: Some(email) })
        }
        Err(_) => {
            Err("Please login to Gmail in Unzoo browser first, then click Connect Gmail again".to_string())
        }
    }
}

#[tauri::command]
fn confirm_gmail_connected(state: State<AppState>, email: String) -> Result<GmailStatus, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO session_state (key, value, updated_at) VALUES ('gmail_email', ?1, datetime('now'))",
        params![email],
    ).map_err(|e| e.to_string())?;

    Ok(GmailStatus { connected: true, email: Some(email) })
}

#[tauri::command]
fn get_register_platforms() -> HashMap<String, Vec<PlatformInfo>> {
    let mut platforms: HashMap<String, Vec<PlatformInfo>> = HashMap::new();

    // 国际社交平台
    platforms.insert("international".to_string(), vec![
        PlatformInfo { id: "twitter".to_string(), name: "Twitter/X".to_string(), phone: true, google_oauth: true },
        PlatformInfo { id: "linkedin".to_string(), name: "LinkedIn".to_string(), phone: true, google_oauth: true },
        PlatformInfo { id: "reddit".to_string(), name: "Reddit".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "facebook".to_string(), name: "Facebook".to_string(), phone: true, google_oauth: false },
    ]);

    // 开发者/科技平台
    platforms.insert("developer".to_string(), vec![
        PlatformInfo { id: "github".to_string(), name: "GitHub".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "devto".to_string(), name: "DEV.to".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "producthunt".to_string(), name: "Product Hunt".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "hackernews".to_string(), name: "Hacker News".to_string(), phone: false, google_oauth: false },
        PlatformInfo { id: "medium".to_string(), name: "Medium".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "hashnode".to_string(), name: "Hashnode".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "indiehackers".to_string(), name: "Indie Hackers".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "betalist".to_string(), name: "BetaList".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "alternativeto".to_string(), name: "AlternativeTo".to_string(), phone: false, google_oauth: true },
    ]);

    // 中国平台
    platforms.insert("chinese".to_string(), vec![
        PlatformInfo { id: "weibo".to_string(), name: "微博".to_string(), phone: true, google_oauth: false },
        PlatformInfo { id: "zhihu".to_string(), name: "知乎".to_string(), phone: true, google_oauth: false },
        PlatformInfo { id: "sspai".to_string(), name: "少数派".to_string(), phone: false, google_oauth: false },
        PlatformInfo { id: "jike".to_string(), name: "即刻".to_string(), phone: true, google_oauth: false },
        PlatformInfo { id: "xiaohongshu".to_string(), name: "小红书".to_string(), phone: true, google_oauth: false },
        PlatformInfo { id: "segmentfault".to_string(), name: "SegmentFault".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "csdn".to_string(), name: "CSDN".to_string(), phone: true, google_oauth: false },
        PlatformInfo { id: "oschina".to_string(), name: "开源中国".to_string(), phone: true, google_oauth: false },
    ]);

    // 日本平台
    platforms.insert("japanese".to_string(), vec![
        PlatformInfo { id: "qiita".to_string(), name: "Qiita".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "zenn".to_string(), name: "Zenn".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "note".to_string(), name: "note".to_string(), phone: false, google_oauth: true },
    ]);

    // 韩国平台
    platforms.insert("korean".to_string(), vec![
        PlatformInfo { id: "naver_blog".to_string(), name: "Naver Blog".to_string(), phone: true, google_oauth: false },
    ]);

    // 俄罗斯平台
    platforms.insert("russian".to_string(), vec![
        PlatformInfo { id: "habr".to_string(), name: "Habr".to_string(), phone: false, google_oauth: true },
        PlatformInfo { id: "vk".to_string(), name: "VK".to_string(), phone: true, google_oauth: false },
    ]);

    // 通讯平台
    platforms.insert("messaging".to_string(), vec![
        PlatformInfo { id: "discord".to_string(), name: "Discord".to_string(), phone: false, google_oauth: false },
        PlatformInfo { id: "telegram".to_string(), name: "Telegram".to_string(), phone: true, google_oauth: false },
        PlatformInfo { id: "slack".to_string(), name: "Slack".to_string(), phone: false, google_oauth: true },
    ]);

    platforms
}

// Helper function to check if user is logged in based on page text
fn check_login_status(text: &str, platform: &str) -> bool {
    let text_lower = text.to_lowercase();

    // Generic logged-in indicators (appear on most platforms when logged in)
    let generic_indicators = [
        "logout", "sign out", "log out", "退出登录", "退出", "登出",
        "my account", "my profile", "your profile", "我的主页", "个人中心",
        "dashboard", "settings", "设置",
    ];

    // Platform-specific indicators
    let platform_indicators: &[&str] = match platform.to_lowercase().as_str() {
        // 国际社交
        "twitter" | "x" => &["home timeline", "compose", "what is happening", "post", "for you"],
        "reddit" => &["create post", "open inbox", "u/", "karma", "your communities"],
        "linkedin" => &["my network", "messaging", "jobs", "write article"],
        "facebook" => &["create post", "messenger", "notifications", "friends"],

        // 开发者平台
        "github" => &["your repositories", "your profile", "new repository", "pull requests"],
        "producthunt" => &["your products", "submit", "streak", "upvoted", "your shoutouts"],
        "hackernews" => &["logout", "submit", "karma:"],
        "devto" => &["write a post", "reading list", "dashboard"],
        "medium" => &["write", "your stories", "new story"],
        "hashnode" => &["write an article", "my blog", "dashboard"],
        "indiehackers" => &["new post", "inbox", "your profile"],

        // 中国平台
        "zhihu" => &["写回答", "写文章", "创作中心", "我的收藏", "消息", "私信", "赞同"],
        "weibo" => &["发微博", "首页", "私信", "评论", "转发", "我的主页"],
        "v2ex" => &["登出", "节点收藏", "主题收藏", "特别关注", "设置"],
        "sspai" => &["写文章", "我的收藏", "消息通知", "创作中心"],
        "jike" | "okjike" => &["发布动态", "我的收藏", "消息", "草稿"],
        "xiaohongshu" | "redbook" => &["发布笔记", "创作中心", "我的收藏", "消息", "购物车"],
        "segmentfault" => &["写文章", "提问题", "草稿箱", "消息"],
        "csdn" => &["写文章", "创作中心", "我的博客", "消息中心"],
        "oschina" => &["写博客", "我的空间", "私信", "动弹"],

        // 日本平台
        "qiita" => &["記事を書く", "投稿する", "下書き", "マイページ", "ストック"],
        "zenn" => &["記事を書く", "本を書く", "マイページ", "下書き"],
        "note" | "note_japan" => &["投稿する", "下書き", "ダッシュボード", "マガジン"],

        // 韩国平台
        "naver_blog" => &["글쓰기", "내 블로그", "이웃"],

        // 俄罗斯平台
        "habr" => &["написать", "профиль", "настройки", "черновики"],
        "vk" => &["моя страница", "мои друзья", "сообщения", "новости"],

        _ => &[],
    };

    // Check generic indicators
    for indicator in generic_indicators {
        if text_lower.contains(indicator) {
            return true;
        }
    }

    // Check platform-specific indicators
    for indicator in platform_indicators {
        if text_lower.contains(&indicator.to_lowercase()) {
            return true;
        }
    }

    false
}

// Check if page requires phone number
fn check_needs_phone(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    text_lower.contains("phone number") ||
    text_lower.contains("enter your phone") ||
    text_lower.contains("verify your phone") ||
    text_lower.contains("mobile number") ||
    text_lower.contains("手机号") ||
    text_lower.contains("电话号码")
}

// Check if page needs email verification
fn check_needs_email_verification(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    text_lower.contains("verify your email") ||
    text_lower.contains("check your email") ||
    text_lower.contains("verification code") ||
    text_lower.contains("confirm your email") ||
    text_lower.contains("we sent") ||
    text_lower.contains("验证码") ||
    text_lower.contains("验证邮件")
}

// Try to get verification code from Gmail using Unzoo
fn get_verification_code_from_gmail(platform: &str) -> Result<String, String> {
    let unzoo = get_unzoo_path();

    // Navigate to Gmail
    let mut cmd = Command::new(&unzoo);
    cmd.args(["navigate", "--url", "https://mail.google.com", "--raw"]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()
        .map_err(|e| format!("Failed to navigate to Gmail: {}", e))?;

    if !output.status.success() {
        return Err("Failed to open Gmail".to_string());
    }

    std::thread::sleep(std::time::Duration::from_secs(3));

    // Get page text to find verification email
    let mut cmd = Command::new(&unzoo);
    cmd.args(["get-text", "--raw"]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()
        .map_err(|e| format!("Failed to get Gmail text: {}", e))?;

    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();

    // Look for verification codes (6-digit or 4-digit numbers near keywords)
    let platform_lower = platform.to_lowercase();
    if text.contains(&platform_lower) || text.contains("verification") || text.contains("code") {
        // Try to extract verification code using regex-like pattern matching
        // Look for 4-6 digit codes
        let words: Vec<&str> = text.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            // Check if word is a potential code (4-6 digits)
            if word.len() >= 4 && word.len() <= 8 && word.chars().all(|c| c.is_ascii_digit()) {
                // Check if nearby words contain "code" or "verification"
                let context_start = if i >= 5 { i - 5 } else { 0 };
                let context_end = std::cmp::min(i + 5, words.len());
                let context: String = words[context_start..context_end].join(" ");

                if context.contains("code") || context.contains("verification") ||
                   context.contains("verify") || context.contains(&platform_lower) {
                    return Ok(word.to_string());
                }
            }
        }
    }

    Err("No verification code found in Gmail".to_string())
}

// Type verification code into the current page
fn enter_verification_code(code: &str) -> Result<(), String> {
    let unzoo = get_unzoo_path();

    // Try common verification input selectors
    let selectors = [
        "input[name*=\"code\"]",
        "input[name*=\"verification\"]",
        "input[type=\"text\"]",
        "input[type=\"number\"]",
        "input[placeholder*=\"code\"]",
    ];

    for selector in selectors {
        let mut cmd = Command::new(&unzoo);
        cmd.args(["type", "--selector", selector, "--text", code, "--raw"]);

        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let output = cmd.output();

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("\"success\":true") {
                // Try to submit
                std::thread::sleep(std::time::Duration::from_millis(500));

                let mut submit_cmd = Command::new(&unzoo);
                submit_cmd.args(["click", "--selector", "button[type=\"submit\"]", "--raw"]);

                #[cfg(windows)]
                submit_cmd.creation_flags(CREATE_NO_WINDOW);

                let _ = submit_cmd.output();
                return Ok(());
            }
        }
    }

    Err("Could not find verification code input".to_string())
}

// Get the home URL for a platform (not the login URL)
fn get_platform_home_url(platform: &str) -> &'static str {
    match platform.to_lowercase().as_str() {
        // 国际社交
        "twitter" | "x" => "https://twitter.com/home",
        "reddit" => "https://www.reddit.com",
        "linkedin" => "https://www.linkedin.com/feed",
        "facebook" => "https://www.facebook.com",
        "telegram" => "https://web.telegram.org",

        // 开发者平台
        "github" => "https://github.com",
        "producthunt" => "https://www.producthunt.com",
        "hackernews" => "https://news.ycombinator.com",
        "devto" => "https://dev.to",
        "medium" => "https://medium.com",
        "hashnode" => "https://hashnode.com",
        "indiehackers" => "https://www.indiehackers.com",
        "betalist" => "https://betalist.com",
        "alternativeto" => "https://alternativeto.net",

        // 中国平台
        "zhihu" => "https://www.zhihu.com",
        "weibo" => "https://weibo.com",
        "v2ex" => "https://www.v2ex.com",
        "sspai" => "https://sspai.com",
        "jike" | "okjike" => "https://web.okjike.com",
        "xiaohongshu" | "redbook" => "https://www.xiaohongshu.com",
        "segmentfault" => "https://segmentfault.com",
        "csdn" => "https://www.csdn.net",
        "oschina" => "https://www.oschina.net",

        // 日本平台
        "qiita" => "https://qiita.com",
        "zenn" => "https://zenn.dev",
        "note" | "note_japan" => "https://note.com",

        // 韩国平台
        "naver_blog" => "https://blog.naver.com",

        // 俄罗斯平台
        "habr" => "https://habr.com",
        "vk" => "https://vk.com",

        _ => "https://www.google.com",
    }
}

/// 点完"用 Google 登录"后，完成 Google 侧的【选账号 + 授权】——老代码漏了这步，是自动登录静默失败的主因。
/// 处理同标签跳转的 OAuth（accounts.google.com 选账号页 + 授权页）。返回是否检测到并处理了 Google 页。
fn complete_google_oauth() -> bool {
    let mut handled = false;
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let raw = unzoo_evaluate("location.href").unwrap_or_default();
        let url = serde_json::from_str::<String>(&raw).unwrap_or(raw);
        if !url.contains("accounts.google.com") && !url.contains("oauth") { continue; }
        handled = true;
        // 选账号：profile 里通常只有一个已登录 Gmail → 点第一个账号条目
        for s in ["div[data-identifier]", "[data-authuser=\"0\"]", "li[class] div[role=link]",
                  "div[role=link]", "ul li div"] {
            if unzoo_click(s).is_ok() { break; }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
        // 授权/继续/允许
        for s in ["#submit_approve_access", "button:has-text(\"Continue\")", "text=Continue",
                  "text=继续", "text=Allow", "text=允许", "text=Authorize"] {
            if unzoo_click(s).is_ok() { break; }
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    handled
}

/// 点掉常见 cookie 同意弹窗（很多平台不点就遮住登录按钮）。
fn accept_cookies_best_effort() {
    for s in ["text=Accept all", "text=Accept All", "text=同意", "text=接受全部", "text=Allow all",
              "button:has-text(\"Accept\")", "#onetrust-accept-btn-handler"] {
        if unzoo_click(s).is_ok() { std::thread::sleep(std::time::Duration::from_millis(800)); break; }
    }
}

#[tauri::command]
async fn register_platform(app: AppHandle, platform: String) -> Result<RegistrationResult, String> {
    // Gmail/Unzoo 连接检查：纯文件系统访问，async 安全，留在异步上下文
    let gmail_status = check_unzoo_gmail_cookies().await;
    if gmail_status.is_err() {
        return Ok(RegistrationResult {
            success: false,
            platform: platform.clone(),
            username: None,
            error: Some("Gmail not connected. Please login to Gmail in Unzoo browser first.".to_string()),
            needs_manual_verification: true,
            verification_reason: Some("Gmail required for OAuth login".to_string()),
        });
    }
    let gmail_email = gmail_status.ok();
    // 整段注册/登录是同步阻塞的浏览器流程（unzoo_* 走阻塞 reqwest + std::thread::sleep）。
    // 必须丢进 spawn_blocking，否则会阻塞/panic 异步运行时——表现为「浏览器标签弹出但动作 stall、
    // 命令永不返回」，调用方拿不到结果（例如开通后账号列表不刷新）。
    tauri::async_runtime::spawn_blocking(move || register_platform_blocking(app, platform, gmail_email))
        .await
        .map_err(|e| format!("注册任务执行失败: {}", e))?
}

// 阻塞版注册/登录流程：全程同步 unzoo_* 浏览器操作 + thread::sleep，只能在 spawn_blocking 里跑。
fn register_platform_blocking(app: AppHandle, platform: String, gmail_email: Option<String>) -> Result<RegistrationResult, String> {
    let config = match get_platform_config(&platform) {
        Some(c) => c,
        None => return Ok(RegistrationResult {
            success: false,
            platform: platform.clone(),
            username: None,
            error: Some(format!("Platform not supported: {}", platform)),
            needs_manual_verification: false,
            verification_reason: None,
        }),
    };

    // Check if account already exists in database
    let existing_account: Option<(String, String)> = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, username FROM accounts WHERE platform = ? AND status = 'active'",
            params![platform],
            |row| Ok((row.get(0)?, row.get::<_, Option<String>>(1)?.unwrap_or_default()))
        ).ok()
    };

    log::info!("=== Starting registration flow for {} ===", config.name);

    // ============================================
    // STEP 1: Check if already logged in
    // ============================================
    let home_url = get_platform_home_url(&platform);
    log::info!("Step 1: Checking login status at {}", home_url);

    if let Err(e) = unzoo_navigate(home_url) {
        return Ok(RegistrationResult {
            success: false,
            platform: platform.clone(),
            username: None,
            error: Some(format!("Failed to navigate: {}", e)),
            needs_manual_verification: true,
            verification_reason: Some("Browser automation failed".to_string()),
        });
    }

    std::thread::sleep(std::time::Duration::from_secs(2));

    if let Ok(page_text) = unzoo_get_text() {
        if check_login_status(&page_text, &platform) {
            log::info!("✓ Already logged in to {}", platform);
            return save_account_and_return_success(&app, &platform, &existing_account, &gmail_email);
        }
        log::info!("✗ Not logged in to {}", platform);
    }

    // ============================================
    // STEP 2: Try Google OAuth login
    // ============================================
    if config.google_oauth {
        log::info!("Step 2: Attempting Google OAuth login");

        if let Err(e) = unzoo_navigate(config.login_url) {
            log::error!("Failed to navigate to login page: {}", e);
        } else {
            std::thread::sleep(std::time::Duration::from_secs(2));
            accept_cookies_best_effort(); // 点掉 cookie 弹窗，免得遮住登录按钮

            // Try to click Google OAuth button
            let google_selectors = [
                config.google_button_selector.unwrap_or(""),
                "button:has-text(\"Google\")",
                "a:has-text(\"Google\")",
                "button:has-text(\"Continue with Google\")",
                "button:has-text(\"Sign in with Google\")",
                "[data-testid=\"google-login\"]",
                "[data-provider=\"google\"]",
                "text=Continue with Google",
                "text=Sign in with Google",
            ];

            let mut clicked = false;
            for selector in google_selectors {
                if selector.is_empty() { continue; }
                if unzoo_click(selector).is_ok() {
                    log::info!("✓ Clicked Google OAuth button: {}", selector);
                    clicked = true;
                    break;
                }
            }

            if clicked {
                // 关键补齐：完成 Google 选账号 + 授权（老代码就缺这步）
                std::thread::sleep(std::time::Duration::from_secs(3));
                complete_google_oauth();
                std::thread::sleep(std::time::Duration::from_secs(2));

                // Check current page for special states
                if let Ok(page_text) = unzoo_get_text() {
                    // Check if phone number is required
                    if check_needs_phone(&page_text) {
                        log::info!("✗ Phone number required");
                        return Ok(RegistrationResult {
                            success: false,
                            platform: platform.clone(),
                            username: None,
                            error: Some("Phone number required".to_string()),
                            needs_manual_verification: true,
                            verification_reason: Some("This platform requires phone verification. Please complete manually.".to_string()),
                        });
                    }

                    // Check if email verification is needed
                    if check_needs_email_verification(&page_text) {
                        log::info!("Email verification required, checking Gmail...");

                        // Try to get verification code from Gmail
                        if let Ok(code) = get_verification_code_from_gmail(&platform) {
                            log::info!("Found verification code: {}", code);

                            // Navigate back to the platform and enter code
                            let _ = unzoo_navigate(config.login_url);
                            std::thread::sleep(std::time::Duration::from_secs(2));

                            if enter_verification_code(&code).is_ok() {
                                log::info!("✓ Entered verification code");
                                std::thread::sleep(std::time::Duration::from_secs(3));
                            }
                        } else {
                            log::info!("Could not find verification code automatically");
                            return Ok(RegistrationResult {
                                success: false,
                                platform: platform.clone(),
                                username: None,
                                error: Some("Email verification required".to_string()),
                                needs_manual_verification: true,
                                verification_reason: Some("Please check your Gmail for verification code and complete manually.".to_string()),
                            });
                        }
                    }
                }

                // Navigate to home and check if logged in now
                let _ = unzoo_navigate(home_url);
                std::thread::sleep(std::time::Duration::from_secs(2));

                if let Ok(page_text) = unzoo_get_text() {
                    if check_login_status(&page_text, &platform) {
                        log::info!("✓ OAuth login successful for {}", platform);
                        return save_account_and_return_success(&app, &platform, &existing_account, &gmail_email);
                    }
                }
            } else {
                log::info!("✗ Could not find Google OAuth button");
            }
        }
    }

    // ============================================
    // STEP 3: Try registration (if OAuth failed)
    // ============================================
    log::info!("Step 3: Attempting registration at {}", config.register_url);

    if let Err(_) = unzoo_navigate(config.register_url) {
        return Ok(RegistrationResult {
            success: false,
            platform: platform.clone(),
            username: None,
            error: Some("Failed to navigate to registration page".to_string()),
            needs_manual_verification: true,
            verification_reason: Some("Please register manually in Unzoo browser".to_string()),
        });
    }

    std::thread::sleep(std::time::Duration::from_secs(2));

    // Try Google signup if available
    if config.google_oauth {
        let signup_selectors = [
            "button:has-text(\"Google\")",
            "button:has-text(\"Sign up with Google\")",
            "button:has-text(\"Continue with Google\")",
            "a:has-text(\"Google\")",
        ];

        for selector in signup_selectors {
            if unzoo_click(selector).is_ok() {
                log::info!("✓ Clicked Google signup button");
                std::thread::sleep(std::time::Duration::from_secs(3));
                complete_google_oauth(); // 选账号 + 授权
                std::thread::sleep(std::time::Duration::from_secs(2));

                // Check for phone/email verification
                if let Ok(page_text) = unzoo_get_text() {
                    if check_needs_phone(&page_text) {
                        return Ok(RegistrationResult {
                            success: false,
                            platform: platform.clone(),
                            username: None,
                            error: Some("Phone number required for registration".to_string()),
                            needs_manual_verification: true,
                            verification_reason: Some("This platform requires phone verification. Please complete manually.".to_string()),
                        });
                    }

                    if check_needs_email_verification(&page_text) {
                        if let Ok(code) = get_verification_code_from_gmail(&platform) {
                            let _ = enter_verification_code(&code);
                            std::thread::sleep(std::time::Duration::from_secs(3));
                        } else {
                            return Ok(RegistrationResult {
                                success: false,
                                platform: platform.clone(),
                                username: None,
                                error: Some("Email verification required".to_string()),
                                needs_manual_verification: true,
                                verification_reason: Some("Please check your Gmail for verification code.".to_string()),
                            });
                        }
                    }
                }

                // Check if registration succeeded
                let _ = unzoo_navigate(home_url);
                std::thread::sleep(std::time::Duration::from_secs(2));

                if let Ok(page_text) = unzoo_get_text() {
                    if check_login_status(&page_text, &platform) {
                        log::info!("✓ Registration successful for {}", platform);
                        return save_account_and_return_success(&app, &platform, &existing_account, &gmail_email);
                    }
                }
                break;
            }
        }
    }

    // ============================================
    // STEP 4: All automatic methods failed
    // ============================================
    log::info!("✗ All automatic methods failed for {}", platform);

    Ok(RegistrationResult {
        success: false,
        platform: platform.clone(),
        username: None,
        error: Some("Automatic login/registration failed".to_string()),
        needs_manual_verification: true,
        verification_reason: Some("Please login or register manually in Unzoo browser, then try again.".to_string()),
    })
}

// Helper function to save account and return success
fn save_account_and_return_success(
    app: &AppHandle,
    platform: &str,
    existing_account: &Option<(String, String)>,
    email: &Option<String>,
) -> Result<RegistrationResult, String> {
    let username = if let Some((_, existing_user)) = existing_account {
        Some(existing_user.clone())
    } else {
        email.as_ref().map(|e| e.split('@').next().unwrap_or("user").to_string())
    };

    let st = app.state::<AppState>();
    if let Ok(conn) = st.db.lock() {
        if existing_account.is_some() {
            let _ = conn.execute(
                "UPDATE accounts SET status = 'active' WHERE platform = ?",
                params![platform],
            );
        } else {
            let id = Uuid::new_v4().to_string();
            let _ = conn.execute(
                "INSERT INTO accounts (id, platform, username, email, status, created_at) VALUES (?1, ?2, ?3, ?4, 'active', datetime('now'))",
                params![id, platform, username, email],
            );
        }
    }

    Ok(RegistrationResult {
        success: true,
        platform: platform.to_string(),
        username,
        error: None,
        needs_manual_verification: false,
        verification_reason: None,
    })
}

/// 平台登录页 URL（敌意平台用专门的登录入口；其余取 platform config，再不行用域名）。
fn platform_login_url(platform: &str) -> String {
    match platform.to_lowercase().as_str() {
        "twitter" | "x" => "https://x.com/i/flow/login".to_string(),
        "reddit" => "https://www.reddit.com/login/".to_string(),
        _ => get_platform_config(platform).map(|c| c.login_url.to_string())
            .unwrap_or_else(|| format!("https://{}/", platform)),
    }
}

/// 自动确保某账号在其身份(persona)的浏览器里已登录：切到 persona profile → 查登录 → 否则 Google 登录/注册。
/// 友好平台全自动；敌意平台(X/Reddit)做到"打开登录页 + 提示你点最后一下"。
#[tauri::command]
async fn account_auto_login(app: AppHandle, account_id: String) -> Result<String, String> {
    let platform: String = {
        let st = app.state::<AppState>();
        let guard = st.db.lock();
        let conn = guard.map_err(|e| e.to_string())?;
        conn.query_row("SELECT platform FROM accounts WHERE id=?1", params![account_id], |r| r.get::<_, String>(0))
            .map_err(|_| "账号不存在".to_string())?
    };
    let acc = Some(account_id.clone());
    ensure_browser_connected().await.map_err(|e| format!("浏览器未就绪: {}", e))?;
    // 切到该账号所属身份的 profile（同 Gmail 下账号共用一个浏览器/IP）
    engine_select_profile(&app, &platform, &acc).await?;
    // 已登录？
    let p = platform.clone();
    let logged = tauri::async_runtime::spawn_blocking(move || verify_login_blocking(&p)).await.unwrap_or(false);
    if logged {
        let st = app.state::<AppState>();
        let guard = st.db.lock();
        if let Ok(conn) = guard { let _ = conn.execute("UPDATE accounts SET health_status='healthy', last_health_check=datetime('now') WHERE id=?1", params![account_id]); }
        return Ok(format!("{}：已登录 ✓", platform));
    }
    // 设置页标记为「需手工登录」的平台：自动登录走不通（如 segmentfault 的 Google OAuth 服务端失效），
    // 不浪费时间硬试自动注册/登录，直接打开登录页让用户手动登录一次。
    let manual_required = {
        let st = app.state::<AppState>();
        let guard = st.db.lock();
        let key = format!("platform_manual_login.{}", platform.to_lowercase());
        let mut v = guard.ok().and_then(|conn| {
            conn.query_row("SELECT value FROM config WHERE key=?1", params![key], |r| r.get::<_, String>(0)).ok()
        }).map(|s| s == "true" || s == "1");
        // 未显式配置时的默认：segmentfault 需手工登录
        if v.is_none() && platform.eq_ignore_ascii_case("segmentfault") { v = Some(true); }
        v.unwrap_or(false)
    };
    // 敌意平台（X/Reddit）：自动登录有锁号风险/技术上进不去 → 不硬试，直接打开登录页让用户点最后一下
    let hostile = matches!(platform.to_lowercase().as_str(), "twitter" | "x" | "reddit");
    if !hostile && !manual_required {
        // 友好平台：先全自动（Google 登录 → 否则注册）
        let res = register_platform(app.clone(), platform.clone()).await?;
        if res.success {
            let st2 = app.state::<AppState>();
            let guard = st2.db.lock();
            if let Ok(conn) = guard { let _ = conn.execute("UPDATE accounts SET health_status='healthy', last_health_check=datetime('now') WHERE id=?1", params![account_id]); }
            return Ok(format!("{}：自动登录成功 ✓", platform));
        }
        // 没成 → 落到"打开登录页让你点一下"（产品已做完 95%）
    }
    // 打开登录页（已在对的身份 profile + 独立 IP），引导用户做最后一下人手点击
    let login_url = platform_login_url(&platform);
    let _ = tauri::async_runtime::spawn_blocking(move || { let _ = unzoo_navigate(&login_url); }).await;
    Ok(format!("MANUAL::{} 已打开登录页（已挂好这个身份的独立 IP）。请在弹出的浏览器窗口里登录一下——只需这一次，之后系统全自动复用。", platform))
}

/// 一键登录某身份(Gmail)下的所有账号：逐个走 account_auto_login。
#[tauri::command]
async fn persona_login_all(app: AppHandle, persona_id: String) -> Result<String, String> {
    let accts: Vec<String> = {
        let st = app.state::<AppState>();
        let guard = st.db.lock();
        let conn = guard.map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT id FROM accounts WHERE persona_id=?1 ORDER BY platform").map_err(|e| e.to_string())?;
        let it = stmt.query_map(params![persona_id], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
        it.flatten().collect()
    };
    if accts.is_empty() { return Err("这个身份下没有账号".into()); }
    let (mut ok, mut need, mut fail) = (0, 0, 0);
    let mut notes: Vec<String> = Vec::new();
    for aid in &accts {
        match account_auto_login(app.clone(), aid.clone()).await {
            Ok(m) if m.starts_with("MANUAL::") => need += 1, // 已打开登录页，待用户点一下
            Ok(_) => ok += 1,
            Err(e) => { fail += 1; notes.push(e); }
        }
    }
    Ok(format!("✓ 自动登录成功 {} · 需你点一下 {} · 失败 {}（已为需手动的逐个打开登录页）{}", ok, need, fail,
        if notes.is_empty() { String::new() } else { format!("\n{}", notes.join("\n")) }))
}

/// 以邮箱为单位「检查并开通账号」：逐平台 有就登录、没有就注册，账号自动挂到这个邮箱名下。
/// 这才是设计的核心流——登录一个 Gmail 后，把选中的各平台账号开出来。
/// platforms：前端选择器里用户新勾选的平台 key 列表。
#[tauri::command]
async fn persona_provision_all(app: AppHandle, persona_id: String, platforms: Vec<String>) -> Result<String, String> {
    let email: String = {
        let st = app.state::<AppState>();
        let guard = st.db.lock();
        let conn = guard.map_err(|e| e.to_string())?;
        conn.query_row("SELECT email FROM personas WHERE id=?1", params![persona_id], |r| r.get(0))
            .map_err(|_| "身份不存在".to_string())?
    };
    let (mut ok, mut need, mut fail) = (0, 0, 0);
    let mut notes: Vec<String> = Vec::new();
    for plat in &platforms {
        let plat = plat.as_str();
        // get-or-create 这个邮箱在该平台的账号行（没有就建一行，归属到这个 persona）
        let aid: String = {
            let st = app.state::<AppState>();
            let guard = st.db.lock();
            let conn = match guard { Ok(c) => c, Err(_) => continue };
            match conn.query_row("SELECT id FROM accounts WHERE persona_id=?1 AND platform=?2",
                params![persona_id, plat], |r| r.get::<_, String>(0)) {
                Ok(id) => id,
                Err(_) => {
                    let id = Uuid::new_v4().to_string();
                    let _ = conn.execute(
                        "INSERT INTO accounts (id, platform, username, persona_id, status, created_at) \
                         VALUES (?1,?2,?3,?4,'active',datetime('now'))",
                        params![id, plat, email, persona_id]);
                    id
                }
            }
        };
        // 检查→有就登录、没有就注册（友好平台全自动；X/Reddit 打开登录页让你点一下）
        match account_auto_login(app.clone(), aid).await {
            Ok(m) if m.starts_with("MANUAL::") => need += 1,
            Ok(_) => ok += 1,
            Err(e) => { fail += 1; notes.push(format!("{}: {}", plat, e)); }
        }
    }
    Ok(format!("邮箱 {} 开通完成：✓ 已登录/注册 {} · 需你点一下 {} · 失败 {}{}",
        email, ok, need, fail,
        if notes.is_empty() { String::new() } else { format!("\n{}", notes.join("\n")) }))
}

// Sync login status for all supported platforms
#[tauri::command]
async fn sync_all_platforms(state: State<'_, AppState>) -> Result<Vec<RegistrationResult>, String> {
    let platforms = [
        "reddit", "producthunt", "hackernews", "devto",
        "twitter", "linkedin", "medium", "github"
    ];

    let mut results = Vec::new();

    for platform in platforms {
        log::info!("Syncing login status for {}", platform);

        let home_url = get_platform_home_url(platform);

        // Navigate to platform home
        if unzoo_navigate(home_url).is_err() {
            results.push(RegistrationResult {
                success: false,
                platform: platform.to_string(),
                username: None,
                error: Some("Failed to navigate".to_string()),
                needs_manual_verification: false,
                verification_reason: None,
            });
            continue;
        }

        std::thread::sleep(std::time::Duration::from_secs(2));

        // Check login status
        if let Ok(page_text) = unzoo_get_text() {
            if check_login_status(&page_text, platform) {
                // User is logged in, save to database
                let existing: Option<String> = {
                    let conn = state.db.lock().map_err(|e| e.to_string())?;
                    conn.query_row(
                        "SELECT id FROM accounts WHERE platform = ?",
                        params![platform],
                        |row| row.get(0)
                    ).ok()
                };

                if existing.is_none() {
                    let conn = state.db.lock().map_err(|e| e.to_string())?;
                    let id = Uuid::new_v4().to_string();
                    let _ = conn.execute(
                        "INSERT INTO accounts (id, platform, status, created_at) VALUES (?1, ?2, 'active', datetime('now'))",
                        params![id, platform],
                    );
                }

                results.push(RegistrationResult {
                    success: true,
                    platform: platform.to_string(),
                    username: None,
                    error: None,
                    needs_manual_verification: false,
                    verification_reason: None,
                });
            } else {
                results.push(RegistrationResult {
                    success: false,
                    platform: platform.to_string(),
                    username: None,
                    error: Some("Not logged in".to_string()),
                    needs_manual_verification: false,
                    verification_reason: None,
                });
            }
        }
    }

    Ok(results)
}

#[tauri::command]
fn get_ai_providers() -> HashMap<String, AiProvider> {
    // 返回默认列表，实际模型从 API 动态获取
    let mut providers = HashMap::new();

    providers.insert("gemini".to_string(), AiProvider {
        name: "Google Gemini".to_string(),
        models: vec!["gemini-2.0-flash".to_string(), "gemini-1.5-pro".to_string()],
    });

    providers.insert("openai".to_string(), AiProvider {
        name: "OpenAI".to_string(),
        models: vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()],
    });

    providers.insert("deepseek".to_string(), AiProvider {
        name: "DeepSeek".to_string(),
        models: vec!["deepseek-chat".to_string(), "deepseek-coder".to_string()],
    });

    providers.insert("qwen".to_string(), AiProvider {
        name: "阿里千问 (Qwen)".to_string(),
        models: vec!["qwen-turbo".to_string(), "qwen-plus".to_string(), "qwen-max".to_string()],
    });

    providers
}

// 从 API 动态获取最新模型列表
#[tauri::command]
async fn fetch_available_models(state: State<'_, AppState>, provider: String) -> Result<Vec<String>, String> {
    let api_key = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let key_name = format!("ai.key.{}", provider);
        conn.query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![key_name],
            |row| row.get::<_, String>(0)
        ).ok()
    };

    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => return Err("No API key configured for this provider".to_string()),
    };

    let client = reqwest::Client::new();

    match provider.as_str() {
        "gemini" => fetch_gemini_models(&client, &api_key).await,
        "openai" => fetch_openai_models(&client, &api_key).await,
        "deepseek" => fetch_deepseek_models(&client, &api_key).await,
        "qwen" => fetch_qwen_models(&client, &api_key).await,
        _ => Err("Unknown provider".to_string()),
    }
}

// Google Gemini - 获取模型列表
async fn fetch_gemini_models(client: &reqwest::Client, api_key: &str) -> Result<Vec<String>, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        api_key
    );

    let resp = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch Gemini models: {}", e))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    let models: Vec<String> = json.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let name = m.get("name")?.as_str()?;
                    // 只保留 generateContent 支持的模型
                    let methods = m.get("supportedGenerationMethods")
                        .and_then(|m| m.as_array())?;
                    if methods.iter().any(|m| m.as_str() == Some("generateContent")) {
                        // 从 "models/gemini-2.0-flash" 提取 "gemini-2.0-flash"
                        Some(name.strip_prefix("models/").unwrap_or(name).to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    if models.is_empty() {
        return Err("No models found".to_string());
    }

    Ok(models)
}

// OpenAI - 获取模型列表
async fn fetch_openai_models(client: &reqwest::Client, api_key: &str) -> Result<Vec<String>, String> {
    let resp = client.get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch OpenAI models: {}", e))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    let mut models: Vec<String> = json.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?;
                    // 只保留 chat 模型 (gpt-*)
                    if id.starts_with("gpt-") && !id.contains("instruct") && !id.contains("vision") {
                        Some(id.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // 排序，最新的模型在前面
    models.sort_by(|a, b| b.cmp(a));
    models.dedup();

    // 限制数量
    models.truncate(10);

    if models.is_empty() {
        return Err("No models found".to_string());
    }

    Ok(models)
}

// DeepSeek - 获取模型列表
async fn fetch_deepseek_models(client: &reqwest::Client, api_key: &str) -> Result<Vec<String>, String> {
    let resp = client.get("https://api.deepseek.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch DeepSeek models: {}", e))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse DeepSeek response: {}", e))?;

    let models: Vec<String> = json.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?;
                    Some(id.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    if models.is_empty() {
        // DeepSeek 可能不支持 /models 接口，返回默认
        return Ok(vec!["deepseek-chat".to_string(), "deepseek-coder".to_string()]);
    }

    Ok(models)
}

// Qwen/DashScope - 获取模型列表
async fn fetch_qwen_models(_client: &reqwest::Client, _api_key: &str) -> Result<Vec<String>, String> {
    // 阿里云 DashScope 没有公开的 models 列表 API，返回常用模型
    Ok(vec![
        "qwen-turbo".to_string(),
        "qwen-plus".to_string(),
        "qwen-max".to_string(),
        "qwen-max-longcontext".to_string(),
        "qwen-vl-plus".to_string(),
    ])
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishResult {
    pub success: bool,
    pub platform: String,
    pub post_url: Option<String>,
    pub error: Option<String>,
    pub next_available_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishStrategy {
    pub platform: String,
    pub max_daily: i32,
    pub min_interval: i32,
    pub posts_today: i32,
    pub last_post_time: Option<String>,
    pub can_post_now: bool,
    pub wait_minutes: i32,
    pub is_warming_up: bool,
    pub warmup_days_left: i32,
}

// 获取随机延迟（模拟人类行为）- 使用新的安全配置版本
// 已移至顶部 get_random_delay(min_seconds, max_seconds)

// 检查是否可以发帖（防封禁核心逻辑）
fn check_can_publish(conn: &Connection, platform: &str, config: &PublishConfig) -> PublishStrategy {
    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();

    // 获取今日发帖数
    let posts_today: i32 = conn.query_row(
        "SELECT COUNT(*) FROM publish_history WHERE platform = ? AND date(created_at) = date('now')",
        params![platform],
        |row| row.get(0)
    ).unwrap_or(0);

    // 获取最后发帖时间
    let last_post: Option<String> = conn.query_row(
        "SELECT created_at FROM publish_history WHERE platform = ? ORDER BY created_at DESC LIMIT 1",
        params![platform],
        |row| row.get(0)
    ).ok();

    // 获取账号创建时间（判断是否在预热期）
    let account_created: Option<String> = conn.query_row(
        "SELECT created_at FROM accounts WHERE platform = ? LIMIT 1",
        params![platform],
        |row| row.get(0)
    ).ok();

    let mut is_warming_up = false;
    let mut warmup_days_left = 0;
    let mut effective_daily_limit = config.max_daily_posts;

    if let Some(created) = account_created {
        if let Ok(created_date) = chrono::DateTime::parse_from_rfc3339(&created) {
            let days_since_creation = (now - created_date.with_timezone(&Utc)).num_days() as i32;
            if days_since_creation < config.warmup_days {
                is_warming_up = true;
                warmup_days_left = config.warmup_days - days_since_creation;
                effective_daily_limit = config.warmup_daily_limit;
            }
        }
    }

    // 检查是否达到每日限制
    if posts_today >= effective_daily_limit {
        return PublishStrategy {
            platform: platform.to_string(),
            max_daily: effective_daily_limit,
            min_interval: config.min_interval_minutes,
            posts_today,
            last_post_time: last_post,
            can_post_now: false,
            wait_minutes: -1, // 今天不能再发了
            is_warming_up,
            warmup_days_left,
        };
    }

    // 检查时间间隔
    let mut wait_minutes = 0;
    if let Some(ref last) = last_post {
        if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last) {
            let minutes_since_last = (now - last_time.with_timezone(&Utc)).num_minutes() as i32;
            if minutes_since_last < config.min_interval_minutes {
                wait_minutes = config.min_interval_minutes - minutes_since_last;
            }
        }
    }

    PublishStrategy {
        platform: platform.to_string(),
        max_daily: effective_daily_limit,
        min_interval: config.min_interval_minutes,
        posts_today,
        last_post_time: last_post,
        can_post_now: wait_minutes == 0,
        wait_minutes,
        is_warming_up,
        warmup_days_left,
    }
}

#[tauri::command]
fn get_publish_strategies(state: State<AppState>) -> Result<Vec<PublishStrategy>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let platforms = ["reddit", "twitter", "linkedin", "hackernews", "devto", "medium", "producthunt", "weibo", "zhihu"];
    let mut strategies = Vec::new();

    for platform in platforms {
        if let Some(config) = get_publish_config(platform) {
            strategies.push(check_can_publish(&conn, platform, &config));
        }
    }

    Ok(strategies)
}

#[tauri::command]
async fn generate_content(state: State<'_, AppState>, product_id: String, platforms: Vec<String>, languages: Vec<String>) -> Result<Vec<Content>, String> {
    log::info!("=== generate_content called ===");
    log::info!("Product ID: {}, Platforms: {:?}, Languages: {:?}", product_id, platforms, languages);

    // 获取产品信息
    let product = {
        let conn = state.db.lock().map_err(|e| {
            log::error!("Failed to lock db for product: {}", e);
            e.to_string()
        })?;
        conn.query_row(
            "SELECT id, name, url, tagline, description, product_type FROM products WHERE id = ?",
            params![product_id],
            |row| Ok(Product {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                tagline: row.get(3)?,
                description: row.get(4)?,
                product_type: row.get(5)?,
                priority: 5,
                weight: 5,
                created_at: String::new(),
            })
        ).map_err(|e| {
            log::error!("Product not found: {}", e);
            format!("Product not found: {}", e)
        })?
    };
    log::info!("Product loaded: {} ({})", product.name, product.url);

    // 获取 AI 配置
    let (provider, api_key) = {
        let conn = state.db.lock().map_err(|e| {
            log::error!("Failed to lock db for config: {}", e);
            e.to_string()
        })?;
        let get_value = |key: &str| -> Option<String> {
            conn.query_row("SELECT value FROM config WHERE key = ?1", params![key], |row| row.get(0)).ok()
        };
        let provider = get_value("ai.provider").unwrap_or_else(|| "gemini".to_string());
        let key = match provider.as_str() {
            "gemini" => get_value("ai.key.gemini"),
            "openai" => get_value("ai.key.openai"),
            "deepseek" => get_value("ai.key.deepseek"),
            "qwen" => get_value("ai.key.qwen"),
            _ => get_value("ai.key.gemini"),
        };
        (provider, key.unwrap_or_default())
    };
    log::info!("AI provider: {}, API key len: {}", provider, api_key.len());

    let client = reqwest::Client::new();
    let start_time = std::time::Instant::now();

    // 预先获取产品描述（只抓取一次）
    let tagline = product.tagline.as_deref().unwrap_or("").trim().to_string();
    let mut description = product.description.as_deref().unwrap_or("").trim().to_string();

    // 如果描述太短，从网站抓取（只做一次，最多等10秒）
    if description.len() < 50 && !product.url.is_empty() {
        log::info!("[TIMING] Starting website fetch...");
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            fetch_website_description(&product.url)
        ).await {
            Ok(Some(fetched)) => {
                description = fetched;
                log::info!("[TIMING] Website fetch succeeded: {} chars", description.len());
            },
            Ok(None) => {
                log::warn!("[TIMING] Website fetch returned no content");
            },
            Err(_) => {
                log::warn!("[TIMING] Website fetch timed out, proceeding without it");
            }
        }
        log::info!("[TIMING] Website fetch phase done: {:?}", start_time.elapsed());
    }

    // 如果没有 API key，使用模板（同步快速返回）
    if api_key.is_empty() {
        let mut contents = Vec::new();
        for platform in &platforms {
            for language in &languages {
                let config = get_publish_config(platform);
                let max_len = config.as_ref().map(|c| c.max_content_length).unwrap_or(2000);
                let body = generate_template_content(&product, platform, language);
                let hashtags = match platform.as_str() {
                    "twitter" | "x" | "linkedin" | "weibo" => vec!["tech".to_string(), "product".to_string()],
                    _ => vec![],
                };
                contents.push(Content {
                    platform: platform.clone(),
                    language: language.clone(),
                    product_id: product_id.clone(),
                    product_name: product.name.clone(),
                    body: body.chars().take(max_len as usize).collect(),
                    hashtags,
                    images: vec![],
                    account_id: None,
                });
            }
        }
        return Ok(contents);
    }

    // 串行生成内容（先确保功能正常）
    log::info!("[TIMING] Starting content generation for {} tasks (sequential)", platforms.len() * languages.len());
    let ai_start = std::time::Instant::now();

    let mut contents = Vec::new();

    for platform in &platforms {
        for language in &languages {
            let task_start = std::time::Instant::now();
            log::info!("[GEN] Generating for {} / {}...", platform, language);

            let config = get_publish_config(platform);
            let max_len = config.as_ref().map(|c| c.max_content_length).unwrap_or(2000);

            let lang_name = match language.as_str() {
                "en" => "English",
                "zh" => "Chinese (Simplified)",
                "ja" => "Japanese",
                "ko" => "Korean",
                "de" => "German",
                "fr" => "French",
                "es" => "Spanish",
                "ru" => "Russian",
                _ => "English",
            };

            // 获取平台安全限制
            let safety_limits = get_platform_safety_limits(platform);
            let include_link = safety_limits.link_allowed;

            // 构建产品上下文
            let product_context = if description.len() > 50 {
                if include_link {
                    format!("Product Name: {}\nTagline: {}\n{}\nWebsite: {}",
                        product.name, tagline, description, product.url)
                } else {
                    format!("Product Name: {}\nTagline: {}\n{}",
                        product.name, tagline, description)
                }
            } else if !tagline.is_empty() {
                if include_link {
                    format!("Product Name: {}\nTagline: {}\nWebsite: {}",
                        product.name, tagline, product.url)
                } else {
                    format!("Product Name: {}\nTagline: {}",
                        product.name, tagline)
                }
            } else {
                format!("Product Name: {}", product.name)
            };

            let url_instruction = if include_link {
                format!("4. Include the URL: {}", product.url)
            } else {
                "4. DO NOT include any URLs or links - this platform restricts external links".to_string()
            };

            let prompt = format!(
                r#"You are a marketing expert writing a post for {platform}.

=== PRODUCT INFORMATION ===
{product_info}

=== YOUR TASK ===
Write a {lang} post promoting this product for {platform}.

=== CRITICAL REQUIREMENTS ===
1. The post MUST be specifically about "{product_name}" and what it does
2. Mention the product name "{product_name}" at least once
3. Explain the value or benefit of using this product
{url_instruction}
5. Follow {platform} conventions and style
6. Maximum {max_len} characters
7. Be authentic and conversational, not promotional
8. No hashtags unless the platform commonly uses them

=== OUTPUT ===
Write ONLY the post content, nothing else:"#,
                platform = platform,
                product_info = product_context,
                lang = lang_name,
                product_name = product.name,
                url_instruction = url_instruction,
                max_len = max_len
            );

            let body = match provider.as_str() {
                "gemini" => {
                    match call_gemini_api(&client, &api_key, &prompt).await {
                        Ok(text) => text,
                        Err(e) => {
                            log::error!("[GEN] Gemini failed: {}", e);
                            generate_template_content(&product, platform, language)
                        }
                    }
                },
                "openai" => call_openai_api(&client, &api_key, &prompt).await.unwrap_or_else(|_| generate_template_content(&product, platform, language)),
                "deepseek" => call_deepseek_api(&client, &api_key, &prompt).await.unwrap_or_else(|_| generate_template_content(&product, platform, language)),
                "qwen" => call_qwen_api(&client, &api_key, &prompt).await.unwrap_or_else(|_| generate_template_content(&product, platform, language)),
                _ => generate_template_content(&product, platform, language),
            };

            log::info!("[TIMING] {} {} done in {:?}, body len: {}", platform, language, task_start.elapsed(), body.len());

            let hashtags = match platform.as_str() {
                "twitter" | "x" | "linkedin" | "weibo" => vec!["tech".to_string(), "product".to_string()],
                _ => vec![],
            };

            contents.push(Content {
                platform: platform.clone(),
                language: language.clone(),
                product_id: product_id.clone(),
                product_name: product.name.clone(),
                body: body.chars().take(max_len as usize).collect(),
                hashtags,
                images: vec![],
                account_id: None,
            });
        }
    }

    log::info!("[TIMING] All {} contents generated in {:?}", contents.len(), ai_start.elapsed());
    log::info!("[TIMING] Total time: {:?}", start_time.elapsed());

    Ok(contents)
}

// 模板内容生成
fn generate_template_content(product: &Product, platform: &str, language: &str) -> String {
    let name = &product.name;
    let tagline = product.tagline.as_deref().unwrap_or("");
    let url = &product.url;

    match (platform, language) {
        ("twitter" | "x", "zh") => format!("发现一个好工具：{}\n\n{}\n\n{}", name, tagline, url),
        ("twitter" | "x", _) => format!("Check out {}: {}\n\n{}", name, tagline, url),
        ("linkedin", "zh") => format!("很高兴分享一个我发现的工具：{}\n\n{}\n\n大家觉得怎么样？欢迎评论交流！\n\n{}", name, tagline, url),
        ("linkedin", _) => format!("Excited to share {} with my network!\n\n{}\n\nWhat do you think? Let me know in the comments.\n\n{}", name, tagline, url),
        ("reddit", "zh") => format!("大家好！分享一个我觉得很有用的工具：{}\n\n{}\n\n链接：{}\n\n欢迎反馈！", name, tagline, url),
        ("reddit", _) => format!("Hey everyone! Wanted to share {}\n\n{}\n\nLink: {}\n\nWould love to hear your thoughts!", name, tagline, url),
        ("hackernews", _) => format!("{} - {}", name, tagline),
        ("producthunt", _) => format!("{}\n\n{}\n\n{}", name, tagline, product.description.as_deref().unwrap_or("")),
        ("weibo", _) => format!("推荐一个工具：{}\n{}\n链接：{}", name, tagline, url),
        ("zhihu", _) => format!("分享一个我在用的工具：{}\n\n{}\n\n官网：{}", name, tagline, url),
        (_, "zh") => format!("{} - {}\n\n{}", name, tagline, url),
        _ => format!("{} - {}\n\n{}", name, tagline, url),
    }
}

#[tauri::command]
async fn publish_content(state: State<'_, AppState>, content: Content) -> Result<PublishResult, String> {
    let config = match get_publish_config(&content.platform) {
        Some(c) => c,
        None => return Ok(PublishResult {
            success: false,
            platform: content.platform,
            post_url: None,
            error: Some("Platform not supported for publishing".to_string()),
            next_available_time: None,
        }),
    };

    // 如果有指定账号，检查是否有绑定的 Profile
    if let Some(ref account_id) = content.account_id {
        let account_profile = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.query_row(
                "SELECT profile_id FROM accounts WHERE id = ?1",
                params![account_id],
                |row| row.get::<_, Option<String>>(0),
            ).ok().flatten()
        };

        if let Some(profile_id) = account_profile {
            log::info!("[PUBLISH] Using account {} profile: {}", account_id, profile_id);
            // 启动账号绑定的 Profile
            match unzoo_launch_profile(profile_id.clone()).await {
                Ok(tab_id) => {
                    set_active_tab(Some(tab_id));
                }
                Err(e) => {
                    log::warn!("[PUBLISH] Failed to launch account profile {}: {}, falling back to default", profile_id, e);
                }
            }
        }
    }

    // Ensure browser is connected before proceeding (fallback to global profile)
    if let Err(e) = ensure_browser_connected().await {
        return Ok(PublishResult {
            success: false,
            platform: content.platform,
            post_url: None,
            error: Some(format!("Browser not ready: {}. Please ensure Unzoo Browser is running.", e)),
            next_available_time: None,
        });
    }

    // 检查是否可以发帖
    let strategy = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        check_can_publish(&conn, &content.platform, &config)
    };

    if !strategy.can_post_now {
        let reason = if strategy.wait_minutes < 0 {
            format!("Daily limit reached ({}/{}). Try again tomorrow.", strategy.posts_today, strategy.max_daily)
        } else {
            format!("Please wait {} minutes before posting again.", strategy.wait_minutes)
        };

        return Ok(PublishResult {
            success: false,
            platform: content.platform,
            post_url: None,
            error: Some(reason),
            next_available_time: Some(format!("{} minutes", strategy.wait_minutes)),
        });
    }

    if strategy.is_warming_up && strategy.max_daily == 0 {
        return Ok(PublishResult {
            success: false,
            platform: content.platform,
            post_url: None,
            error: Some(format!(
                "Account is in warmup period ({} days left). Only commenting is allowed.",
                strategy.warmup_days_left
            )),
            next_available_time: None,
        });
    }

    log::info!("=== Publishing to {} ===", config.name);

    // 更长的随机延迟，模拟人类行为（5-15秒）
    let delay = get_human_delay(5, 15);
    log::info!("[HUMAN] Initial wait {}s before starting", delay);
    std::thread::sleep(std::time::Duration::from_secs(delay));

    // 导航到发帖页面
    if let Err(e) = unzoo_navigate(config.post_url) {
        return Ok(PublishResult {
            success: false,
            platform: content.platform,
            post_url: None,
            error: Some(format!("Failed to navigate: {}", e)),
            next_available_time: None,
        });
    }

    // 页面加载后的浏览行为（3-8秒）
    let browse_time = get_human_delay(3, 8);
    log::info!("[HUMAN] Browsing page for {}s...", browse_time);
    std::thread::sleep(std::time::Duration::from_secs(browse_time));

    // 模拟浏览：滚动页面
    let _ = unzoo_scroll("down", 200);
    std::thread::sleep(std::time::Duration::from_millis(get_human_delay(500, 1500)));
    let _ = unzoo_scroll("up", 100);
    std::thread::sleep(std::time::Duration::from_millis(get_human_delay(300, 800)));

    // 随机鼠标移动
    random_mouse_movement();

    // 如果需要标题，先输入标题
    if config.needs_title {
        if let Some(title_selector) = config.title_selector {
            let title = format!("Check out: {}", &content.product_name[..std::cmp::min(50, content.product_name.len())]);
            // 人类式点击输入框
            let _ = human_click(title_selector);
            std::thread::sleep(std::time::Duration::from_millis(get_human_delay(300, 800)));
            let _ = unzoo_type(title_selector, &title);
            // 输入后停顿（检查输入）
            std::thread::sleep(std::time::Duration::from_secs(get_human_delay(1, 3)));
        }
    }

    // 输入内容前的准备动作
    random_mouse_movement();
    std::thread::sleep(std::time::Duration::from_millis(get_human_delay(500, 1500)));

    // 输入内容 - 使用平台特定的输入方法
    let type_result = match content.platform.to_lowercase().as_str() {
        "twitter" | "x" => unzoo_twitter_type(&content.body),
        "medium" => unzoo_prosemirror_type(".ProseMirror", &content.body),
        "zhihu" => unzoo_prosemirror_type(".public-DraftEditor-content", &content.body),
        _ => {
            // 先点击输入框
            let _ = human_click(config.content_selector);
            std::thread::sleep(std::time::Duration::from_millis(get_human_delay(200, 500)));
            unzoo_type(config.content_selector, &content.body)
        }
    };

    if let Err(e) = type_result {
        return Ok(PublishResult {
            success: false,
            platform: content.platform,
            post_url: None,
            error: Some(format!("Failed to type content: {}", e)),
            next_available_time: None,
        });
    }

    // 输入后检查内容（3-8秒）
    let review_time = get_human_delay(3, 8);
    log::info!("[HUMAN] Reviewing content for {}s...", review_time);
    std::thread::sleep(std::time::Duration::from_secs(review_time));

    // 滚动查看内容
    let _ = unzoo_scroll("up", 150);
    std::thread::sleep(std::time::Duration::from_millis(get_human_delay(500, 1000)));

    // 随机鼠标移动到提交按钮附近
    random_mouse_movement();

    // 点击发布按钮（使用人类式点击）
    log::info!("[HUMAN] Clicking submit button...");
    if let Err(e) = human_click(config.submit_selector) {
        return Ok(PublishResult {
            success: false,
            platform: content.platform,
            post_url: None,
            error: Some(format!("Failed to submit: {}", e)),
            next_available_time: None,
        });
    }

    // 提交后等待（观察结果）
    let wait_result = get_human_delay(3, 6);
    log::info!("[HUMAN] Waiting {}s for result...", wait_result);
    std::thread::sleep(std::time::Duration::from_secs(wait_result));

    // 记录发布历史
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let id = Uuid::new_v4().to_string();
        let _ = conn.execute(
            "INSERT INTO publish_history (id, product_id, platform, language, content, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'published', datetime('now'))",
            params![id, content.product_id, content.platform, content.language, content.body],
        );
    }

    log::info!("✓ Published to {}", config.name);

    Ok(PublishResult {
        success: true,
        platform: content.platform,
        post_url: Some(config.post_url.to_string()),
        error: None,
        next_available_time: Some(format!("{} minutes", config.min_interval_minutes)),
    })
}

// Type text into element - uses HTTP API
/// human 模式输入：真人节奏 + typo+backspace 自我纠正 + 读回校验（v1.9.0）。
fn unzoo_human_type(selector: &str, text: &str) -> Result<(), String> {
    log::info!("[HUMAN] Typing {} chars into: {}", text.len(), selector);
    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab".to_string());
    }
    ensure_human_profile();
    unzoo_mcp("human_type", serde_json::json!({
        "tab_id": tab_id,
        "selector": selector,
        "text": text
    })).map(|_| ())
}

fn unzoo_type(selector: &str, text: &str) -> Result<(), String> {
    if unzoo_human_mode_enabled() {
        return unzoo_human_type(selector, text);
    }
    log::info!("[UNZOO] Typing (real keyboard) into: {}", selector);
    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab".to_string());
    }
    // browser_type：每字符真实 WebKeyboardEvent（isTrusted=true）+ 拟人间隔，CJK 走 ImeCommitText
    unzoo_mcp("browser_type", serde_json::json!({
        "tab_id": tab_id,
        "selector": selector,
        "text": text,
        "delay_ms": human_type_delay_ms(),
        "instant": false,
        "timeout": 8000
    })).map(|_| ())
}

// Execute JavaScript in browser - uses HTTP API
fn unzoo_evaluate(expression: &str) -> Result<String, String> {
    log::info!("[UNZOO] Evaluating JS expression");

    let client = get_blocking_client();
    let api_url = format!("{}/evaluate", UNZOO_API_BASE);

    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab".to_string());
    }

    let body = serde_json::json!({
        "tab_id": tab_id,
        "expression": expression
    });

    let resp = client.post(&api_url)
        .json(&body)
        .send()
        .map_err(|e| format!("Evaluate failed: {}", e))?;

    if resp.status().is_success() {
        // 响应结构为 { data: { result: <值> }, success: true }，取 data.result
        let v: serde_json::Value = resp.json().unwrap_or_default();
        let r = v.get("data").and_then(|d| d.get("result"));
        Ok(match r {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        })
    } else {
        Err(format!("Evaluate failed: {}", resp.status()))
    }
}

// Special type function for Twitter's Lexical editor (uses execCommand insertText)
fn unzoo_twitter_type(text: &str) -> Result<(), String> {
    // Escape the text for JavaScript string
    let escaped_text = text
        .replace("\\", "\\\\")
        .replace("'", "\\'")
        .replace("\n", "\\n")
        .replace("\r", "\\r");

    let js = format!(r#"
        (function() {{
            const editor = document.querySelector('[data-testid="tweetTextarea_0"]');
            if (!editor) return 'Editor not found';

            const editable = editor.querySelector('[contenteditable="true"]') || editor;
            editable.focus();

            // Use execCommand for Lexical editor compatibility
            document.execCommand('selectAll', false, null);
            document.execCommand('insertText', false, '{}');

            // Dispatch input event to trigger React state update
            editable.dispatchEvent(new Event('input', {{ bubbles: true }}));

            return 'success';
        }})()
    "#, escaped_text);

    let result = unzoo_evaluate(&js)?;
    if result.contains("success") {
        Ok(())
    } else {
        Err(format!("Twitter type failed: {}", result))
    }
}

// Special type function for Medium/Notion style ProseMirror editors
fn unzoo_prosemirror_type(selector: &str, text: &str) -> Result<(), String> {
    let escaped_text = text
        .replace("\\", "\\\\")
        .replace("'", "\\'")
        .replace("\n", "\\n")
        .replace("\r", "\\r");

    let js = format!(r#"
        (function() {{
            const editor = document.querySelector('{}');
            if (!editor) return 'Editor not found';

            editor.focus();
            document.execCommand('selectAll', false, null);
            document.execCommand('insertText', false, '{}');
            editor.dispatchEvent(new Event('input', {{ bubbles: true }}));

            return 'success';
        }})()
    "#, selector, escaped_text);

    let result = unzoo_evaluate(&js)?;
    if result.contains("success") {
        Ok(())
    } else {
        Err(format!("ProseMirror type failed: {}", result))
    }
}

// ============================================
// 智能多标签页批量发布系统
// - 同时打开多个标签页
// - 模拟人类浏览行为
// - 随机延迟和交错执行
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TabTask {
    tab_id: String,
    content: Content,
    config: serde_json::Value,
    status: String, // "pending", "browsing", "typing", "submitted", "done", "failed"
}

// Human-like random delay (longer and more variable)
fn get_human_delay(min_secs: u64, max_secs: u64) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
    let range = max_secs - min_secs;
    min_secs + (seed as u64 % (range + 1))
}

// Simulate human browsing behavior on a page
fn simulate_browsing(tab_id: &str) -> Result<(), String> {
    log::info!("[HUMAN] Simulating browsing on tab {}", tab_id);

    // Random scroll down
    let scroll_amount = get_human_delay(200, 500) as i32;
    let _ = unzoo_scroll("down", scroll_amount);
    std::thread::sleep(std::time::Duration::from_millis(get_human_delay(500, 1500)));

    // Scroll back up
    let _ = unzoo_scroll("up", scroll_amount / 2);
    std::thread::sleep(std::time::Duration::from_millis(get_human_delay(300, 800)));

    // Random mouse movement (simulated by hovering over elements)
    let hover_selectors = ["body", "header", "nav", "main", "article"];
    for selector in hover_selectors.iter().take(2) {
        let _ = unzoo_hover(selector);
        std::thread::sleep(std::time::Duration::from_millis(get_human_delay(200, 500)));
    }

    Ok(())
}

// Type text slowly like a human (with random pauses)
fn human_type(selector: &str, text: &str, platform: &str) -> Result<(), String> {
    log::info!("[HUMAN] Typing {} chars slowly for {}", text.len(), platform);

    // Use platform-specific typing
    match platform.to_lowercase().as_str() {
        "twitter" | "x" => {
            // For Twitter, use the special Lexical editor method
            unzoo_twitter_type(text)
        }
        "medium" => {
            unzoo_prosemirror_type(".ProseMirror", text)
        }
        "zhihu" => {
            unzoo_prosemirror_type(".public-DraftEditor-content", text)
        }
        _ => {
            // For normal inputs, we could type character by character
            // but the CLI doesn't support that well, so use normal type with delays
            unzoo_type(selector, text)
        }
    }
}

// Scroll helper - uses HTTP API
fn unzoo_scroll(direction: &str, amount: i32) -> Result<(), String> {
    log::info!("[UNZOO] Scrolling (real wheel) {} by {}", direction, amount);
    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab".to_string());
    }
    // browser_scroll：delta_y → 5 步真实 WebMouseWheelEvent（isTrusted=true）
    let dy = match direction { "up" => -amount, _ => amount };
    unzoo_mcp("browser_scroll", serde_json::json!({ "tab_id": tab_id, "delta_y": dy })).map(|_| ())
}

// Hover helper - uses HTTP API with random coordinates
// Note: The API doesn't support selector-based hover, so we hover at random viewport coordinates
fn unzoo_hover(_selector: &str) -> Result<(), String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    log::info!("[UNZOO] Hovering (trusted mouse-move)");

    let tab_id = get_active_tab().unwrap_or_default();
    if tab_id.is_empty() {
        return Err("No active tab".to_string());
    }

    // 随机视口坐标 → 真实 mouse-move（isTrusted=true，触发 :hover/mouseenter）
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    let x = (seed % 800 + 100) as i32; // 100-900 px
    let y = ((seed / 1000) % 400 + 200) as i32; // 200-600 px
    unzoo_mcp("browser_hover", serde_json::json!({ "tab_id": tab_id, "x": x, "y": y })).map(|_| ())
}

// Human-like click: hover first, wait, then click
fn human_click(selector: &str) -> Result<(), String> {
    log::info!("[HUMAN] Preparing to click: {}", selector);

    // Step 1: Move mouse near the element (hover)
    let _ = unzoo_hover(selector);

    // Step 2: Random "thinking" pause (200-800ms)
    let think_time = get_human_delay(200, 800);
    std::thread::sleep(std::time::Duration::from_millis(think_time));

    // Step 3: Small random mouse jitter (simulated by hovering nearby elements)
    // This makes the movement look less robotic

    // Step 4: Actually click
    let result = unzoo_click(selector);

    // Step 5: Post-click pause (as if watching the result)
    let watch_time = get_human_delay(500, 1500);
    std::thread::sleep(std::time::Duration::from_millis(watch_time));

    result
}

// Human-like typing with pauses between words
fn human_type_slow(selector: &str, text: &str) -> Result<(), String> {
    log::info!("[HUMAN] Typing slowly: {} chars", text.len());

    // Click the input first
    let _ = human_click(selector);

    // For long text, we could split by words/sentences
    // but CLI doesn't support character-by-character well
    // So we add realistic pauses before and after typing

    // Pause before starting to type (reading what to type)
    let pre_pause = get_human_delay(500, 1500);
    std::thread::sleep(std::time::Duration::from_millis(pre_pause));

    // Type the content
    let result = unzoo_type(selector, text);

    // Pause after typing (reviewing what was typed)
    let post_pause = get_human_delay(300, 1000);
    std::thread::sleep(std::time::Duration::from_millis(post_pause));

    result
}

// Random mouse movement to simulate human behavior
fn random_mouse_movement() {
    let movements = get_human_delay(1, 3) as usize;
    let hover_targets = ["body", "header", "nav", "main", "footer", "article", "section"];

    for _ in 0..movements {
        let idx = get_human_delay(0, hover_targets.len() as u64 - 1) as usize;
        let _ = unzoo_hover(hover_targets[idx]);
        std::thread::sleep(std::time::Duration::from_millis(get_human_delay(100, 400)));
    }
}

// Create a new tab for a specific URL
fn create_tab_for_url(url: &str) -> Result<String, String> {
    let unzoo = get_unzoo_path();
    let mut cmd = Command::new(&unzoo);
    cmd.args(["tab", "new", "--url", url, "--raw"]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().map_err(|e| format!("Failed to create tab: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse tab_id from response
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if let Some(tab_id) = json.get("data").and_then(|d| d.get("tab_id")).and_then(|t| t.as_i64()) {
            return Ok(tab_id.to_string());
        }
        if let Some(tab_id) = json.get("tab_id").and_then(|t| t.as_i64()) {
            return Ok(tab_id.to_string());
        }
    }

    Err(format!("Failed to parse tab_id: {}", stdout))
}

// Switch to a specific tab
fn switch_to_tab(tab_id: &str) -> Result<(), String> {
    let unzoo = get_unzoo_path();
    let mut cmd = Command::new(&unzoo);
    cmd.args(["tab", "switch", "--id", tab_id]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().map_err(|e| e.to_string())?;
    if output.status.success() { Ok(()) } else { Err("switch failed".to_string()) }
}

#[tauri::command]
async fn publish_contents_smart(
    state: State<'_, AppState>,
    contents: Vec<Content>,
) -> Result<Vec<PublishResult>, String> {
    if contents.is_empty() {
        return Ok(vec![]);
    }

    log::info!("[BATCH] Starting smart batch publish for {} items", contents.len());

    // Ensure browser is connected
    if let Err(e) = ensure_browser_connected().await {
        return Err(format!("Browser not ready: {}", e));
    }

    let mut results: Vec<PublishResult> = Vec::new();
    let mut tasks: Vec<TabTask> = Vec::new();

    // Phase 1: Open all tabs (up to 5 at a time to avoid overwhelming)
    let max_concurrent_tabs = std::cmp::min(contents.len(), 5);
    log::info!("[BATCH] Phase 1: Opening {} tabs...", max_concurrent_tabs);

    for (i, content) in contents.iter().take(max_concurrent_tabs).enumerate() {
        let config = match get_publish_config(&content.platform) {
            Some(c) => c,
            None => {
                results.push(PublishResult {
                    success: false,
                    platform: content.platform.clone(),
                    post_url: None,
                    error: Some("Platform not supported".to_string()),
                    next_available_time: None,
                });
                continue;
            }
        };

        // Random delay between opening tabs (looks more natural)
        if i > 0 {
            let delay = get_human_delay(2, 5);
            log::info!("[BATCH] Waiting {}s before opening next tab...", delay);
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        }

        // Create tab
        match create_tab_for_url(config.post_url) {
            Ok(tab_id) => {
                log::info!("[BATCH] Opened tab {} for {}", tab_id, content.platform);
                tasks.push(TabTask {
                    tab_id,
                    content: content.clone(),
                    config: serde_json::to_value(&config).unwrap_or_default(),
                    status: "pending".to_string(),
                });
            }
            Err(e) => {
                log::error!("[BATCH] Failed to open tab for {}: {}", content.platform, e);
                results.push(PublishResult {
                    success: false,
                    platform: content.platform.clone(),
                    post_url: None,
                    error: Some(format!("Failed to open tab: {}", e)),
                    next_available_time: None,
                });
            }
        }
    }

    // Phase 2: Wait and "browse" - simulate reading the pages
    log::info!("[BATCH] Phase 2: Simulating browsing behavior...");
    let browse_time = get_human_delay(10, 20);
    log::info!("[BATCH] Browsing for {}s...", browse_time);

    for task in &mut tasks {
        if task.status == "pending" {
            let _ = switch_to_tab(&task.tab_id);
            std::thread::sleep(std::time::Duration::from_secs(2));
            let _ = simulate_browsing(&task.tab_id);
            task.status = "browsing".to_string();

            // Random wait between tabs
            let wait = get_human_delay(3, 8);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        }
    }

    // Phase 3: Type content in random order
    log::info!("[BATCH] Phase 3: Typing content (staggered)...");

    // Shuffle tasks for random execution order
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as usize;
    let mut indices: Vec<usize> = (0..tasks.len()).collect();
    for i in 0..indices.len() {
        let j = (seed + i * 7) % indices.len();
        indices.swap(i, j);
    }

    for idx in indices.iter() {
        let task = &mut tasks[*idx];
        if task.status != "browsing" {
            continue;
        }

        log::info!("[BATCH] Typing content for {} in tab {}", task.content.platform, task.tab_id);

        // Switch to this tab
        let _ = switch_to_tab(&task.tab_id);
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Get config values
        let needs_title = task.config.get("needs_title").and_then(|v| v.as_bool()).unwrap_or(false);
        let title_selector = task.config.get("title_selector").and_then(|v| v.as_str());
        let content_selector = task.config.get("content_selector").and_then(|v| v.as_str()).unwrap_or("textarea");

        // Type title if needed
        if needs_title {
            if let Some(selector) = title_selector {
                let title = format!("Check out: {}", &task.content.product_name[..std::cmp::min(50, task.content.product_name.len())]);
                let _ = unzoo_type(selector, &title);
                std::thread::sleep(std::time::Duration::from_millis(get_human_delay(500, 1500)));
            }
        }

        // Type content
        if let Err(e) = human_type(content_selector, &task.content.body, &task.content.platform) {
            log::error!("[BATCH] Failed to type for {}: {}", task.content.platform, e);
            task.status = "failed".to_string();
        } else {
            task.status = "typing".to_string();
        }

        // Random delay before next tab
        let delay = get_human_delay(5, 15);
        log::info!("[BATCH] Waiting {}s before next...", delay);
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
    }

    // Phase 4: Submit in random order with long delays
    log::info!("[BATCH] Phase 4: Submitting (staggered)...");

    // Re-shuffle for submit order
    let seed2 = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as usize;
    for i in 0..indices.len() {
        let j = (seed2 + i * 11) % indices.len();
        indices.swap(i, j);
    }

    for idx in indices.iter() {
        let task = &mut tasks[*idx];
        if task.status != "typing" {
            continue;
        }

        log::info!("[BATCH] Submitting for {} in tab {}", task.content.platform, task.tab_id);

        // Switch to this tab
        let _ = switch_to_tab(&task.tab_id);
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Random mouse movement before clicking submit (looks more natural)
        random_mouse_movement();

        // One more scroll to look natural
        let _ = unzoo_scroll("up", 100);
        std::thread::sleep(std::time::Duration::from_millis(get_human_delay(300, 800)));

        // Maybe scroll down a bit to see the submit button
        let _ = unzoo_scroll("down", 50);
        std::thread::sleep(std::time::Duration::from_millis(get_human_delay(200, 500)));

        // Get submit selector
        let submit_selector = task.config.get("submit_selector").and_then(|v| v.as_str()).unwrap_or("button[type=\"submit\"]");

        // Human-like click submit (hover first, then click)
        if let Err(e) = human_click(submit_selector) {
            log::error!("[BATCH] Failed to submit for {}: {}", task.content.platform, e);
            task.status = "failed".to_string();
            results.push(PublishResult {
                success: false,
                platform: task.content.platform.clone(),
                post_url: None,
                error: Some(format!("Failed to submit: {}", e)),
                next_available_time: None,
            });
        } else {
            task.status = "submitted".to_string();
            log::info!("[BATCH] ✓ Submitted to {}", task.content.platform);

            // Record in history
            {
                if let Ok(conn) = state.db.lock() {
                    let id = Uuid::new_v4().to_string();
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = conn.execute(
                        "INSERT INTO publish_history (id, platform, content, status, created_at) VALUES (?1, ?2, ?3, 'published', ?4)",
                        rusqlite::params![id, task.content.platform, task.content.body, now],
                    );
                }
            }

            results.push(PublishResult {
                success: true,
                platform: task.content.platform.clone(),
                post_url: None,
                error: None,
                next_available_time: None,
            });
        }

        // Long delay between submissions (most important for avoiding detection)
        let delay = get_human_delay(30, 90);
        log::info!("[BATCH] Waiting {}s before next submission...", delay);
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
    }

    // Add remaining items to results
    for content in contents.iter().skip(max_concurrent_tabs) {
        results.push(PublishResult {
            success: false,
            platform: content.platform.clone(),
            post_url: None,
            error: Some("Queued for next batch".to_string()),
            next_available_time: None,
        });
    }

    log::info!("[BATCH] Batch publish complete: {} results", results.len());
    Ok(results)
}

#[tauri::command]
fn publish_contents(_contents: Vec<Content>) -> Result<Vec<bool>, String> {
    // 旧函数保留兼容性，建议使用 publish_contents_smart
    Err("请使用 publish_contents_smart 进行批量发布".to_string())
}

// ============================================
// 改进的发布系统 - 预览+确认模式
// 参考 baoyu-skills 的安全发布流程
// ============================================

// 截图功能
fn unzoo_screenshot() -> Result<String, String> {
    let unzoo = get_unzoo_path();
    log::info!("Taking screenshot");

    let mut cmd = Command::new(&unzoo);
    cmd.args(["screenshot", "--format", "base64", "--raw"]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()
        .map_err(|e| format!("Failed to run unzoo: {}", e))?;

    if output.status.success() {
        // 解析 JSON 响应获取 base64 图片
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(data) = json.get("data").and_then(|d| d.as_str()) {
                return Ok(data.to_string());
            }
        }
        Ok(stdout.to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// 复制图片到剪贴板（用于粘贴上传）
fn unzoo_copy_image(image_path: &str) -> Result<(), String> {
    let unzoo = get_unzoo_path();
    log::info!("Copying image to clipboard: {}", image_path);

    // 使用 JavaScript 读取图片并复制到剪贴板
    let js = format!(r#"
        (async function() {{
            try {{
                const response = await fetch('file://{}');
                const blob = await response.blob();
                await navigator.clipboard.write([
                    new ClipboardItem({{ [blob.type]: blob }})
                ]);
                return 'success';
            }} catch (e) {{
                return 'error: ' + e.message;
            }}
        }})()
    "#, image_path.replace("\\", "/"));

    let mut cmd = Command::new(&unzoo);
    cmd.args(["evaluate", "--expression", &js, "--await-promise", "--raw"]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()
        .map_err(|e| format!("Failed to run unzoo: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("success") {
        Ok(())
    } else {
        Err(format!("Failed to copy image: {}", stdout))
    }
}

// 粘贴操作（Ctrl+V / Cmd+V）
fn unzoo_paste() -> Result<(), String> {
    let unzoo = get_unzoo_path();
    log::info!("Pasting from clipboard");

    let mut cmd = Command::new(&unzoo);
    // 使用键盘快捷键粘贴
    #[cfg(target_os = "macos")]
    cmd.args(["key", "--key", "v", "--modifiers", "Meta", "--raw"]);
    #[cfg(not(target_os = "macos"))]
    cmd.args(["key", "--key", "v", "--modifiers", "Control", "--raw"]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()
        .map_err(|e| format!("Failed to run unzoo: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// 等待图片上传完成（检测 blob URL）
fn unzoo_wait_for_image_upload() -> Result<bool, String> {
    let js = r#"
        (function() {
            const images = document.querySelectorAll('img[src^="blob:"]');
            return images.length > 0 ? 'uploaded' : 'waiting';
        })()
    "#;

    for _ in 0..10 {
        let result = unzoo_evaluate(js)?;
        if result.contains("uploaded") {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(false)
}

// 预览发布 - 填充内容但不点击发布按钮
#[tauri::command]
async fn prepare_publish(state: State<'_, AppState>, content: Content) -> Result<PublishPreview, String> {
    let config = match get_publish_config(&content.platform) {
        Some(c) => c,
        None => return Ok(PublishPreview {
            platform: content.platform,
            content: content.body,
            images: content.images,
            screenshot: None,
            ready: false,
            error: Some("Platform not supported".to_string()),
        }),
    };

    // Ensure browser is connected before proceeding
    if let Err(e) = ensure_browser_connected().await {
        return Ok(PublishPreview {
            platform: content.platform,
            content: content.body,
            images: content.images,
            screenshot: None,
            ready: false,
            error: Some(format!("Browser not ready: {}. Please ensure Unzoo Browser is running.", e)),
        });
    }

    // 检查是否可以发帖
    let strategy = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        check_can_publish(&conn, &content.platform, &config)
    };

    if !strategy.can_post_now {
        let reason = if strategy.wait_minutes < 0 {
            format!("每日限制已达 ({}/{})", strategy.posts_today, strategy.max_daily)
        } else {
            format!("请等待 {} 分钟", strategy.wait_minutes)
        };
        return Ok(PublishPreview {
            platform: content.platform,
            content: content.body,
            images: content.images,
            screenshot: None,
            ready: false,
            error: Some(reason),
        });
    }

    log::info!("=== Preparing publish to {} (preview mode) ===", config.name);

    // 导航到发帖页面
    if let Err(e) = unzoo_navigate(config.post_url) {
        return Ok(PublishPreview {
            platform: content.platform,
            content: content.body,
            images: content.images,
            screenshot: None,
            ready: false,
            error: Some(format!("导航失败: {}", e)),
        });
    }

    std::thread::sleep(std::time::Duration::from_secs(2));

    // 如果需要标题，先输入标题
    if config.needs_title {
        if let Some(title_selector) = config.title_selector {
            let title = format!("Check out: {}", &content.product_name[..std::cmp::min(50, content.product_name.len())]);
            let _ = unzoo_type(title_selector, &title);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    // 输入内容 - 使用平台特定的输入方法
    let type_result = match content.platform.to_lowercase().as_str() {
        "twitter" | "x" => unzoo_twitter_type(&content.body),
        "medium" => unzoo_prosemirror_type(".ProseMirror", &content.body),
        "zhihu" => unzoo_prosemirror_type(".public-DraftEditor-content", &content.body),
        _ => unzoo_type(config.content_selector, &content.body),
    };

    if let Err(e) = type_result {
        return Ok(PublishPreview {
            platform: content.platform,
            content: content.body,
            images: content.images,
            screenshot: None,
            ready: false,
            error: Some(format!("输入内容失败: {}", e)),
        });
    }

    std::thread::sleep(std::time::Duration::from_millis(500));

    // 上传图片（如果有）
    for image_path in &content.images {
        log::info!("Uploading image: {}", image_path);

        // 复制图片到剪贴板
        if let Err(e) = unzoo_copy_image(image_path) {
            log::warn!("Failed to copy image: {}", e);
            continue;
        }

        std::thread::sleep(std::time::Duration::from_millis(300));

        // 粘贴图片
        if let Err(e) = unzoo_paste() {
            log::warn!("Failed to paste image: {}", e);
            continue;
        }

        // 等待图片上传
        let _ = unzoo_wait_for_image_upload();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // 截图预览
    let screenshot = unzoo_screenshot().ok();

    log::info!("✓ Content prepared for {} - waiting for confirmation", config.name);

    Ok(PublishPreview {
        platform: content.platform,
        content: content.body,
        images: content.images,
        screenshot,
        ready: true,
        error: None,
    })
}

// 确认发布 - 点击发布按钮
#[tauri::command]
async fn confirm_publish(state: State<'_, AppState>, platform: String, product_id: String, content_body: String) -> Result<PublishResult, String> {
    let config = match get_publish_config(&platform) {
        Some(c) => c,
        None => return Ok(PublishResult {
            success: false,
            platform,
            post_url: None,
            error: Some("Platform not supported".to_string()),
            next_available_time: None,
        }),
    };

    log::info!("=== Confirming publish to {} ===", config.name);

    // 点击发布按钮
    if let Err(e) = unzoo_click(config.submit_selector) {
        return Ok(PublishResult {
            success: false,
            platform,
            post_url: None,
            error: Some(format!("点击发布按钮失败: {}", e)),
            next_available_time: None,
        });
    }

    std::thread::sleep(std::time::Duration::from_secs(3));

    // 记录发布历史
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let id = Uuid::new_v4().to_string();
        let _ = conn.execute(
            "INSERT INTO publish_history (id, product_id, platform, language, content, status, created_at) VALUES (?1, ?2, ?3, 'en', ?4, 'published', datetime('now'))",
            params![id, product_id, platform, content_body],
        );
    }

    log::info!("✓ Published to {}", config.name);

    Ok(PublishResult {
        success: true,
        platform,
        post_url: Some(config.post_url.to_string()),
        error: None,
        next_available_time: Some(format!("{} minutes", config.min_interval_minutes)),
    })
}

// 取消发布 - 关闭当前页面或清空内容
#[tauri::command]
fn cancel_publish(platform: String) -> Result<(), String> {
    log::info!("Cancelling publish to {}", platform);

    // 按 Escape 关闭对话框
    let unzoo = get_unzoo_path();
    let mut cmd = Command::new(&unzoo);
    cmd.args(["key", "--key", "Escape", "--raw"]);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let _ = cmd.output();
    Ok(())
}

// ============================================
// 回复系统 - 更自然的推广方式
// ============================================

// 添加关键词
#[tauri::command]
fn add_keyword(state: State<AppState>, keyword: String, product_id: Option<String>, platforms: Vec<String>) -> Result<Keyword, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    let platforms_str = platforms.join(",");

    conn.execute(
        "INSERT INTO keywords (id, keyword, product_id, platforms, enabled) VALUES (?1, ?2, ?3, ?4, 1)",
        params![id, keyword, product_id, platforms_str],
    ).map_err(|e| e.to_string())?;

    Ok(Keyword {
        id,
        product_id,
        keyword,
        platforms,
        enabled: true,
    })
}

// 获取所有关键词
#[tauri::command]
fn list_keywords(state: State<AppState>) -> Result<Vec<Keyword>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, keyword, product_id, platforms, enabled FROM keywords ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;

    let keywords = stmt.query_map([], |row| {
        let platforms_str: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
        Ok(Keyword {
            id: row.get(0)?,
            keyword: row.get(1)?,
            product_id: row.get(2)?,
            platforms: platforms_str.split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
            enabled: row.get::<_, i32>(4)? == 1,
        })
    }).map_err(|e| e.to_string())?;

    keywords.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

// 删除关键词
#[tauri::command]
fn delete_keyword(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM keywords WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// 搜索相关帖子
#[tauri::command]
// 同步：内部全是阻塞 HTTP（unzoo_*），必须在非异步线程上调用（见引擎 spawn_blocking）。
fn discover_posts(state: State<'_, AppState>, platform: String, keyword: String) -> Result<Vec<DiscoveredPost>, String> {
    let config = match get_reply_config(&platform) {
        Some(c) => c,
        None => return Err(format!("Platform {} not supported for replies", platform)),
    };

    log::info!("=== Discovering posts on {} for keyword: {} ===", platform, keyword);

    // 构建搜索URL
    let search_url = config.search_url.replace("{query}", &urlencoding::encode(&keyword));

    // 导航到搜索页面
    if let Err(e) = unzoo_navigate(&search_url) {
        return Err(format!("Failed to navigate to search: {}", e));
    }

    // 等待页面加载：SPA 平台（Twitter/Reddit）首屏渲染慢，固定 sleep 常常
    // 在内容出现前就抓取，导致首次发现 0 条、要等重试才成功。改为轮询选择器，
    // 链接出现即停，最多等约 14 秒。
    std::thread::sleep(std::time::Duration::from_secs(2));
    let mut links = unzoo_get_links(config.post_link_selector).unwrap_or_default();
    let mut waited = 2;
    while links.is_empty() && waited < 14 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        waited += 2;
        links = unzoo_get_links(config.post_link_selector).unwrap_or_default();
    }
    log::info!(
        "[DISCOVER] {} 选择器命中 {} 条链接（等待 {}s）",
        platform, links.len(), waited
    );

    // 获取页面内容（用于相关性评分）
    let page_text = unzoo_get_text().unwrap_or_default();

    let mut posts = Vec::new();
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    for link in links.iter().take(10) {
        // 跳过已经发现的帖子
        let exists: bool = conn.query_row(
            "SELECT 1 FROM discovered_posts WHERE post_url = ?",
            params![link],
            |_| Ok(true)
        ).unwrap_or(false);

        if exists {
            continue;
        }

        let id = Uuid::new_v4().to_string();
        let title = extract_title_from_link(link);

        // 计算相关性分数（简单版本）
        let relevance = calculate_relevance(&keyword, &title, &page_text);

        // 保存到数据库
        let _ = conn.execute(
            "INSERT INTO discovered_posts (id, platform, post_url, post_title, keyword_matched, relevance_score, status, discovered_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'new', datetime('now'))",
            params![id, platform, link, title, keyword, relevance],
        );

        posts.push(DiscoveredPost {
            id: id.clone(),
            platform: platform.clone(),
            post_url: link.clone(),
            post_title: Some(title),
            post_content: None,
            keyword_matched: Some(keyword.clone()),
            relevance_score: relevance,
            status: "new".to_string(),
            discovered_at: Utc::now().to_rfc3339(),
        });
    }

    log::info!("Discovered {} new posts on {}", posts.len(), platform);
    Ok(posts)
}

// 获取链接列表。
// 注意：必须走 REST /evaluate（作用于当前活动标签），不要用 unzoo CLI 的
// `evaluate` 子命令——后者会新开一个 http://evaluate/ 标签页，反复调用会刷爆标签。
fn unzoo_get_links(selector: &str) -> Result<Vec<String>, String> {
    let expr = format!(
        "Array.from(document.querySelectorAll('{}')).map(a => a.href).filter(h => h && h.startsWith('http')).slice(0, 20).join('\\n')",
        selector
    );
    let raw = unzoo_evaluate(&expr)?;
    // unzoo_evaluate 返回 data 字段的 JSON 串；先尝试解码成普通字符串
    let joined: String = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    Ok(joined
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && l.starts_with("http"))
        .map(String::from)
        .collect())
}

// 从链接提取标题
fn extract_title_from_link(link: &str) -> String {
    link.split('/')
        .last()
        .unwrap_or("untitled")
        .replace('-', " ")
        .replace('_', " ")
        .chars()
        .take(100)
        .collect()
}

// 计算相关性分数
fn calculate_relevance(keyword: &str, title: &str, content: &str) -> f64 {
    let keyword_lower = keyword.to_lowercase();
    let title_lower = title.to_lowercase();
    let content_lower = content.to_lowercase();

    let mut score: f64 = 0.0;

    // 标题包含关键词 +0.5
    if title_lower.contains(&keyword_lower) {
        score += 0.5;
    }

    // 内容包含关键词 +0.3
    if content_lower.contains(&keyword_lower) {
        score += 0.3;
    }

    // 包含"推荐"、"求助"等词 +0.2
    let trigger_words = ["recommend", "looking for", "need", "help", "suggest", "best", "tool",
                         "推荐", "求推荐", "有没有", "求助", "怎么", "哪个好"];
    for word in trigger_words {
        if content_lower.contains(word) {
            score += 0.1;
        }
    }

    score.min(1.0)
}

// 获取待回复的帖子
#[tauri::command]
fn list_discovered_posts(state: State<AppState>, platform: Option<String>, status: Option<String>) -> Result<Vec<DiscoveredPost>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let mut sql = "SELECT id, platform, post_url, post_title, post_content, keyword_matched, relevance_score, status, discovered_at FROM discovered_posts".to_string();
    let mut conditions = Vec::new();

    if let Some(ref p) = platform {
        conditions.push(format!("platform = '{}'", p));
    }
    if let Some(ref s) = status {
        conditions.push(format!("status = '{}'", s));
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql.push_str(" ORDER BY relevance_score DESC, discovered_at DESC LIMIT 50");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let posts = stmt.query_map([], |row| {
        Ok(DiscoveredPost {
            id: row.get(0)?,
            platform: row.get(1)?,
            post_url: row.get(2)?,
            post_title: row.get(3)?,
            post_content: row.get(4)?,
            keyword_matched: row.get(5)?,
            relevance_score: row.get(6)?,
            status: row.get(7)?,
            discovered_at: row.get(8)?,
        })
    }).map_err(|e| e.to_string())?;

    posts.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

// 检查是否可以回复
fn check_can_reply(conn: &Connection, platform: &str, config: &ReplyConfig) -> ReplyStrategy {
    let now = Utc::now();

    // 获取今日回复数
    let replies_today: i32 = conn.query_row(
        "SELECT COUNT(*) FROM reply_history WHERE platform = ? AND date(created_at) = date('now')",
        params![platform],
        |row| row.get(0)
    ).unwrap_or(0);

    // 获取最后回复时间
    let last_reply: Option<String> = conn.query_row(
        "SELECT created_at FROM reply_history WHERE platform = ? ORDER BY created_at DESC LIMIT 1",
        params![platform],
        |row| row.get(0)
    ).ok();

    // 获取待回复帖子数
    let discovered: i32 = conn.query_row(
        "SELECT COUNT(*) FROM discovered_posts WHERE platform = ? AND status = 'new'",
        params![platform],
        |row| row.get(0)
    ).unwrap_or(0);

    // 检查是否达到每日限制
    if replies_today >= config.max_daily_replies {
        return ReplyStrategy {
            platform: platform.to_string(),
            max_daily: config.max_daily_replies,
            min_interval: config.min_interval_minutes,
            replies_today,
            last_reply_time: last_reply,
            can_reply_now: false,
            wait_minutes: -1,
            discovered_posts: discovered,
        };
    }

    // 检查时间间隔
    let mut wait_minutes = 0;
    if let Some(ref last) = last_reply {
        if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last) {
            let minutes_since_last = (now - last_time.with_timezone(&Utc)).num_minutes() as i32;
            if minutes_since_last < config.min_interval_minutes {
                wait_minutes = config.min_interval_minutes - minutes_since_last;
            }
        }
    }

    ReplyStrategy {
        platform: platform.to_string(),
        max_daily: config.max_daily_replies,
        min_interval: config.min_interval_minutes,
        replies_today,
        last_reply_time: last_reply,
        can_reply_now: wait_minutes == 0,
        wait_minutes,
        discovered_posts: discovered,
    }
}

// 获取所有平台的回复策略
#[tauri::command]
fn get_reply_strategies(state: State<AppState>) -> Result<Vec<ReplyStrategy>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let platforms = ["reddit", "twitter", "hackernews", "linkedin", "zhihu"];
    let mut strategies = Vec::new();

    for platform in platforms {
        if let Some(config) = get_reply_config(platform) {
            strategies.push(check_can_reply(&conn, platform, &config));
        }
    }

    Ok(strategies)
}

// 生成自然的回复内容（模板版本）
fn generate_natural_reply(product_name: &str, product_tagline: &str, post_title: &str, keyword: &str) -> String {
    let templates = [
        format!("I've been using {} for this exact use case. {}. Might be worth checking out!", product_name, product_tagline),
        format!("Have you tried {}? I found it really helpful for {}. Just sharing in case it helps!", product_name, keyword),
        format!("Not sure if it fits your specific needs, but {} has been great for me. {}", product_name, product_tagline),
        format!("Came across {} recently and it's been solid for {}. Worth a look maybe?", product_name, keyword),
    ];

    let index = post_title.len() % templates.len();
    templates[index].clone()
}

// AI 生成回复
#[tauri::command]
async fn generate_ai_reply(
    state: State<'_, AppState>,
    post_title: String,
    post_content: String,
    keyword: String,
    product_name: Option<String>,
    product_tagline: Option<String>,
) -> Result<String, String> {
    // 获取 AI 配置
    let (provider, api_key) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;

        let get_value = |key: &str| -> Option<String> {
            conn.query_row(
                "SELECT value FROM config WHERE key = ?1",
                params![key],
                |row| row.get(0)
            ).ok()
        };

        let provider = get_value("ai.provider").unwrap_or_else(|| "gemini".to_string());

        // 根据 provider 获取对应的 key
        let key = match provider.as_str() {
            "gemini" => get_value("ai.key.gemini"),
            "openai" => get_value("ai.key.openai"),
            "deepseek" => get_value("ai.key.deepseek"),
            "qwen" => get_value("ai.key.qwen"),
            _ => get_value("ai.key.gemini"),
        };

        (provider, key.unwrap_or_default())
    };

    if api_key.is_empty() {
        // 没有配置 API key，使用模板
        let prod_name = product_name.unwrap_or_else(|| "this tool".to_string());
        let prod_tag = product_tagline.unwrap_or_default();
        return Ok(generate_natural_reply(&prod_name, &prod_tag, &post_title, &keyword));
    }

    // 构建 prompt
    let product_context = match (&product_name, &product_tagline) {
        (Some(name), Some(tagline)) => format!("Product to mention: {} - {}", name, tagline),
        (Some(name), None) => format!("Product to mention: {}", name),
        _ => "No specific product to mention".to_string(),
    };

    let prompt = format!(
        r#"You are helping write a natural, helpful reply to a forum post.

Post title: {}
Post content: {}
Keyword matched: {}
{}

Write a helpful, genuine reply that:
1. Directly addresses the user's question or topic
2. Naturally mentions the product if relevant (don't force it if not appropriate)
3. Sounds like a real person, not an advertisement
4. Is concise (2-3 sentences max)
5. Doesn't use excessive exclamation marks or marketing language

Reply:"#,
        post_title, post_content, keyword, product_context
    );

    // 调用 AI API
    let client = reqwest::Client::new();
    let reply = match provider.as_str() {
        "gemini" => call_gemini_api(&client, &api_key, &prompt).await?,
        "openai" => call_openai_api(&client, &api_key, &prompt).await?,
        "deepseek" => call_deepseek_api(&client, &api_key, &prompt).await?,
        "qwen" => call_qwen_api(&client, &api_key, &prompt).await?,
        _ => call_gemini_api(&client, &api_key, &prompt).await?,
    };

    Ok(reply)
}

// ============ 智能回复管线：读真帖 → 判定相关性 → 生成针对性回复 ============

#[derive(Debug, Clone)]
struct SmartReply {
    relevant: bool,         // 是否值得回复（不相关则跳过，不刷屏）
    mention_product: bool,  // 本条是否软提产品
    reply: String,          // 回复正文
    reason: String,         // LLM 给出的判定理由（审核时展示）
    intent: i64,            // 买家意向分 0-100（此人是否在找解决方案/有购买信号）
}

/// 打开帖子读取真实标题+正文（截断到 ~2000 字）。
/// 同步：内部走阻塞 unzoo HTTP，必须在 spawn_blocking 中调用。
fn fetch_post_detail(url: &str) -> (String, String) {
    if unzoo_navigate(url).is_err() {
        return (String::new(), String::new());
    }
    std::thread::sleep(std::time::Duration::from_secs(3));
    // 优先取主帖容器文本，退回 body 全文；统一截断避免超长 prompt。
    let expr = "JSON.stringify({t:document.title||'',b:((document.querySelector('article,[role=\\'article\\'],[data-testid=\\'tweetText\\'],.Post,.post,.expando,main')||document.body)?.innerText||'').replace(/\\s+/g,' ').slice(0,2000)})";
    let raw = match unzoo_evaluate(expr) { Ok(r) => r, Err(_) => return (String::new(), String::new()) };
    // unzoo_evaluate 把 result 当字符串返回，可能是被 JSON 编码过的串。
    let decoded: String = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&decoded) {
        let t = v.get("t").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let b = v.get("b").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        return (t, b);
    }
    (String::new(), decoded)
}

/// 从 LLM 原始输出中解析 SmartReply（容忍 ```json 围栏、额外文本）。
fn parse_smart_reply(
    raw: &str, product_name: &str, product_tagline: &str,
    post_title: &str, keyword: &str, reply_type: &str,
) -> SmartReply {
    let cleaned = raw.trim()
        .trim_start_matches("```json").trim_start_matches("```")
        .trim_end_matches("```").trim();
    let slice = match (cleaned.find('{'), cleaned.rfind('}')) {
        (Some(a), Some(b)) if b > a => &cleaned[a..=b],
        _ => cleaned,
    };
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(slice) {
        let reply = v.get("reply").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let relevant = v.get("relevant").and_then(|x| x.as_bool()).unwrap_or(!reply.is_empty());
        let mention = v.get("mention_product").and_then(|x| x.as_bool()).unwrap_or(reply_type == "keyword");
        let reason = v.get("reason").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let intent = v.get("intent").and_then(|x| x.as_i64()).unwrap_or(50).clamp(0, 100);
        if !relevant {
            return SmartReply { relevant: false, mention_product: false, reply: String::new(), reason, intent };
        }
        if !reply.is_empty() {
            return SmartReply { relevant: true, mention_product: mention, reply, reason, intent };
        }
    }
    // 不是 JSON：把整段当作回复正文；为空则退回模板。
    let txt = slice.trim().to_string();
    if txt.is_empty() {
        SmartReply {
            relevant: true,
            mention_product: reply_type == "keyword",
            reply: generate_natural_reply(product_name, product_tagline, post_title, keyword),
            reason: "parse_fallback".into(),
            intent: 50,
        }
    } else {
        SmartReply { relevant: true, mention_product: reply_type == "keyword", reply: txt, reason: "plain_text".into(), intent: 50 }
    }
}

/// 读完真实帖子后，由 LLM 一次性判定「是否相关 / 是否提产品」并生成回复。
/// 无 API key 或调用失败 → 退回模板（relevant=true）。
async fn generate_smart_reply(
    provider: &str, api_key: &str,
    platform_name: &str, max_len: i32, reply_type: &str,
    post_title: &str, post_body: &str, keyword: &str,
    product_name: &str, product_tagline: &str,
) -> SmartReply {
    if api_key.is_empty() {
        return SmartReply {
            relevant: true,
            mention_product: reply_type == "keyword",
            reply: generate_natural_reply(product_name, product_tagline, post_title, keyword),
            reason: "no_api_key_fallback".into(),
            intent: 50,
        };
    }
    let body_trunc: String = post_body.chars().take(1500).collect();
    let role = if reply_type == "mention" {
        "You are the ORIGINAL AUTHOR replying to a comment on YOUR OWN post. Be warm and conversational. Do NOT advertise — you already posted."
    } else {
        "You are a knowledgeable community member replying to SOMEONE ELSE'S post. Be genuinely helpful first; only mention the product if it truly fits the question."
    };
    let prompt = format!(
        r#"{role}

Platform: {platform} (hard limit {max} characters, match this platform's tone)
Post title: {title}
Post content: {body}
Tracked keyword: {kw}
Our product: {pname} — {ptag}

Respond with ONLY a compact JSON object (no markdown, no prose):
{{"relevant": true|false, "intent": <0-100>, "mention_product": true|false, "reply": "<text>", "reason": "<one short sentence>"}}

Rules:
- intent = buying-intent score 0-100: how likely this person is actively looking for / evaluating / asking for a solution like ours RIGHT NOW. 80-100 = explicitly asking for recommendations or expressing a pain our product solves; 40-70 = on-topic discussion; 0-30 = off-topic, just venting, or already settled.
- relevant=false when the post is off-topic, low quality, pure spam, or where replying would look forced/promotional. When false, "reply" may be empty.
- The reply MUST engage with THIS specific post (reference its actual content), read like a real human, avoid marketing clichés and excessive exclamation marks.
- Mention the product ONLY when mention_product=true, and do it softly and contextually — never a hard sell.
- Stay within {max} characters."#,
        role = role, platform = platform_name, max = max_len,
        title = post_title, body = body_trunc, kw = keyword,
        pname = product_name, ptag = product_tagline
    );

    let client = reqwest::Client::new();
    let raw = match provider {
        "openai" => call_openai_api(&client, api_key, &prompt).await,
        "deepseek" => call_deepseek_api(&client, api_key, &prompt).await,
        "qwen" => call_qwen_api(&client, api_key, &prompt).await,
        _ => call_gemini_api(&client, api_key, &prompt).await,
    };
    match raw {
        Ok(s) => parse_smart_reply(&s, product_name, product_tagline, post_title, keyword, reply_type),
        Err(e) => {
            log::warn!("[REPLY] LLM 调用失败，回退模板：{}", e);
            SmartReply {
                relevant: true,
                mention_product: reply_type == "keyword",
                reply: generate_natural_reply(product_name, product_tagline, post_title, keyword),
                reason: format!("llm_error_fallback: {}", e),
                intent: 50,
            }
        }
    }
}

/// 读取 AI provider 配置（provider, api_key）。引擎与命令共用。
fn read_ai_config(conn: &Connection) -> (String, String) {
    let get_value = |key: &str| -> Option<String> {
        conn.query_row("SELECT value FROM config WHERE key = ?1", params![key], |r| r.get(0)).ok()
    };
    let provider = get_value("ai.provider").unwrap_or_else(|| "gemini".to_string());
    let key = match provider.as_str() {
        "openai" => get_value("ai.key.openai"),
        "deepseek" => get_value("ai.key.deepseek"),
        "qwen" => get_value("ai.key.qwen"),
        _ => get_value("ai.key.gemini"),
    };
    (provider, key.unwrap_or_default())
}

// ============ LinkedIn 信息流就地回复 ============
// LinkedIn 内容搜索页不暴露帖子永久链接，"发现链接→跳转→回复"模型不成立。
// 改为：在搜索结果信息流里直接对某条帖子的评论框输入并发布。

#[derive(Debug, Clone, Deserialize)]
struct FeedPost { idx: i64, author: String, text: String }

/// 稳定短哈希，用于给无永久链接的帖子生成去重键。
fn short_hash(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:x}", h.finish())
}

/// 滚动信息流并抽取可评论帖子；给每条帖容器打 data-um-idx、评论按钮打 data-um-cbtn。
/// 评论按钮按多语言 aria-label 匹配（该账号 UI 为中文"评论"）。同步，需在 spawn_blocking 中调用。
fn linkedin_extract_posts() -> Result<Vec<FeedPost>, String> {
    // LinkedIn 搜索结果懒加载且很慢：分段滚动 + 等待，确保帖子渲染。
    for y in [600, 1400, 2200, 1000] {
        let _ = unzoo_evaluate(&format!("(function(){{window.scrollTo(0,{});return 'ok'}})()", y));
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    // 评论按钮在搜索结果里可能无 aria-label，仅有文本"评论"；故同时匹配 aria-label 与 innerText。
    let js = r#"(function(){
        var rx=/评论|comment|comentar|kommentar|commentaire|commento/i;
        var btns=Array.prototype.slice.call(document.querySelectorAll('button'))
            .filter(function(b){return rx.test((b.getAttribute('aria-label')||'')+' '+(b.innerText||''))});
        var out=[];var idx=0;var seen=[];
        btns.forEach(function(b){
            var p=b;var hops=0;
            while(p&&p!==document.body&&hops<14){
                if((p.innerText||'').length>140){break;}
                p=p.parentElement;hops++;
            }
            if(!p||seen.indexOf(p)>=0)return;seen.push(p);
            p.setAttribute('data-um-idx',idx);
            b.setAttribute('data-um-cbtn',idx);
            var alink=p.querySelector('a[href*="/in/"],a[href*="/company/"]');
            var author=alink?(alink.innerText||'').trim().split('\n')[0]:'';
            var text=(p.innerText||'').replace(/\s+/g,' ').trim().slice(0,600);
            out.push({idx:idx,author:author,text:text});
            idx++;
        });
        return JSON.stringify(out.slice(0,10));
    })()"#;
    let raw = unzoo_evaluate(js)?;
    let decoded: String = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    serde_json::from_str::<Vec<FeedPost>>(&decoded)
        .map_err(|e| format!("解析信息流失败: {} | {}", e, decoded.chars().take(120).collect::<String>()))
}

/// 对信息流里第 idx 条帖子就地评论。submit=false 时只打开评论框验证机制（演练，不输入不提交）。
/// 实测要点（2026-05 LinkedIn）：评论框是 TipTap/ProseMirror contenteditable，且渲染在帖子容器之外；
/// 发布按钮（文本"评论"）在输入文本后才出现 → 必须 点开→定位编辑框→输入→再找发布按钮。
fn linkedin_infeed_reply(idx: i64, text: &str, submit: bool) -> Result<String, String> {
    // 滚动目标进可视区，点开评论按钮
    let _ = unzoo_evaluate(&format!(
        "(function(){{var p=document.querySelector('[data-um-idx=\"{}\"]');if(p)p.scrollIntoView({{block:'center'}});return 'ok'}})()", idx));
    std::thread::sleep(std::time::Duration::from_millis(900));
    unzoo_click(&format!("[data-um-cbtn=\"{}\"]", idx))?;
    std::thread::sleep(std::time::Duration::from_secs(3));

    // 评论框在帖子容器之外，全局取最后一个可见 contenteditable
    let mark_ed = r#"(function(){
        var eds=Array.prototype.slice.call(document.querySelectorAll('[contenteditable="true"],.tiptap,.ql-editor'))
            .filter(function(e){return e.offsetParent!==null});
        if(!eds.length)return 'noEditor';
        eds[eds.length-1].setAttribute('data-um-editor','1');
        return 'ok';
    })()"#;
    let r = unzoo_evaluate(mark_ed)?;
    let r: String = serde_json::from_str::<String>(&r).unwrap_or(r);
    if !r.contains("ok") {
        return Err("点开评论后未找到编辑框（LinkedIn DOM 可能改版）".into());
    }
    if !submit {
        return Ok("comment_box_ready".into());
    }

    // 聚焦并输入（real_keyboard，ProseMirror 接受真实键入）
    let _ = unzoo_evaluate("(function(){var e=document.querySelector('[data-um-editor]');if(e)e.focus();return 'ok'})()");
    std::thread::sleep(std::time::Duration::from_millis(400));
    unzoo_type("[data-um-editor=\"1\"]", text)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 4)));

    // 输入后发布按钮才出现：从编辑框上溯找首个含可点击「发布/评论」按钮的容器（排除帖子自身的评论按钮）
    let mark_sb = r#"(function(){
        var ed=document.querySelector('[data-um-editor]');if(!ed)return 'noEditor';
        var rx=/发布|post|comment|评论|comentar|publicar|senden/i;
        var c=ed;var h=0;
        while(c&&h<9){
            var b=Array.prototype.slice.call(c.querySelectorAll('button')).filter(function(x){
                return !x.disabled && !x.hasAttribute('data-um-cbtn') && rx.test((x.getAttribute('aria-label')||'')+' '+(x.innerText||''));
            });
            if(b.length){b[b.length-1].setAttribute('data-um-submit','1');return 'ok';}
            c=c.parentElement;h++;
        }
        return 'noSubmit';
    })()"#;
    let r = unzoo_evaluate(mark_sb)?;
    let r: String = serde_json::from_str::<String>(&r).unwrap_or(r);
    if !r.contains("ok") {
        return Err("输入后未找到发布按钮".into());
    }
    unzoo_click("[data-um-submit=\"1\"]")?;
    std::thread::sleep(std::time::Duration::from_secs(2));
    Ok("submitted".into())
}


// 回复帖子
#[tauri::command]
// 同步：内部全是阻塞 HTTP（unzoo_*），必须在非异步线程上调用（见引擎 spawn_blocking）。
fn reply_to_post(
    state: State<'_, AppState>,
    post_id: String,
    product_id: Option<String>,
    custom_reply: Option<String>,
) -> Result<ReplyResult, String> {
    // 获取帖子信息
    let (post, config) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;

        let post: DiscoveredPost = conn.query_row(
            "SELECT id, platform, post_url, post_title, post_content, keyword_matched, relevance_score, status, discovered_at FROM discovered_posts WHERE id = ?",
            params![post_id],
            |row| Ok(DiscoveredPost {
                id: row.get(0)?,
                platform: row.get(1)?,
                post_url: row.get(2)?,
                post_title: row.get(3)?,
                post_content: row.get(4)?,
                keyword_matched: row.get(5)?,
                relevance_score: row.get(6)?,
                status: row.get(7)?,
                discovered_at: row.get(8)?,
            })
        ).map_err(|e| format!("Post not found: {}", e))?;

        let config = get_reply_config(&post.platform)
            .ok_or_else(|| format!("Platform {} not supported", post.platform))?;

        // 检查是否可以回复
        let strategy = check_can_reply(&conn, &post.platform, &config);
        if !strategy.can_reply_now {
            return Ok(ReplyResult {
                success: false,
                platform: post.platform,
                post_url: post.post_url,
                reply_content: None,
                error: Some(if strategy.wait_minutes < 0 {
                    format!("Daily limit reached ({}/{})", strategy.replies_today, strategy.max_daily)
                } else {
                    format!("Please wait {} minutes", strategy.wait_minutes)
                }),
            });
        }

        (post, config)
    };

    log::info!("=== Replying to post on {} ===", config.name);
    log::info!("URL: {}", post.post_url);

    // 生成回复内容
    let reply_content = if let Some(custom) = custom_reply {
        custom
    } else {
        // 获取产品信息生成回复
        let (product_name, product_tagline) = if let Some(ref pid) = product_id {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.query_row(
                "SELECT name, tagline FROM products WHERE id = ?",
                params![pid],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?.unwrap_or_default()))
            ).unwrap_or(("our tool".to_string(), "it's really helpful".to_string()))
        } else {
            ("the tool".to_string(), "it might help".to_string())
        };

        generate_natural_reply(
            &product_name,
            &product_tagline,
            &post.post_title.unwrap_or_default(),
            &post.keyword_matched.unwrap_or_default()
        )
    };

    // 随机延迟（拟人）
    let delay = get_random_delay(2, 8);
    log::info!("Waiting {}ms before navigating", delay);
    std::thread::sleep(std::time::Duration::from_millis(delay));

    // 导航 + 输入 + 提交：统一走 post_reply_to_url（X 走 Draft.js 专用流程，其余走通用流程）
    let _ = &config; // config 仍用于上面的限流校验
    if let Err(e) = post_reply_to_url(&post.platform, &post.post_url, &reply_content) {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute("UPDATE discovered_posts SET status = 'failed' WHERE id = ?", params![post_id]);
        return Ok(ReplyResult {
            success: false,
            platform: post.platform,
            post_url: post.post_url,
            reply_content: Some(reply_content),
            error: Some(e),
        });
    }

    // 记录回复历史 + 线索（每条真实发出的回复 = 一条线索）
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let id = Uuid::new_v4().to_string();
        // 取该帖意向分（关键词流程发布前已写入）
        let intent: i64 = conn.query_row(
            "SELECT COALESCE(intent_score,0) FROM discovered_posts WHERE id=?1", params![post_id], |r| r.get(0)).unwrap_or(0);
        let _ = conn.execute(
            "INSERT INTO reply_history (id, post_id, platform, post_url, reply_content, product_mentioned, status, intent_score, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'sent', ?7, datetime('now'))",
            params![id, post_id, post.platform, post.post_url, reply_content, product_id, intent],
        );
        // 更新帖子状态
        let _ = conn.execute("UPDATE discovered_posts SET status = 'replied' WHERE id = ?", params![post_id]);
        // 线索：作者/关键词从帖子记录兜底取（前面字段可能已被消费，这里重新查）
        let (author, kw): (String, String) = conn.query_row(
            "SELECT COALESCE(post_title,''), COALESCE(keyword_matched,'') FROM discovered_posts WHERE id=?1",
            params![post_id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((String::new(), String::new()));
        record_lead(&conn, &id, &post.platform, &author, &post.post_url, &reply_content, intent, &kw, &None);
    }

    log::info!("✓ Replied to post on {}", config.name);

    Ok(ReplyResult {
        success: true,
        platform: post.platform,
        post_url: post.post_url,
        reply_content: Some(reply_content),
        error: None,
    })
}

// 标记帖子状态
#[tauri::command]
fn update_post_status(state: State<AppState>, post_id: String, status: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE discovered_posts SET status = ? WHERE id = ?", params![status, post_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// Dashboard stats
#[derive(Debug, Serialize, Deserialize)]
struct DashboardStats {
    active_tasks: i32,
    today_posts: i32,
    account_health: i32,
    success_rate: i32,
    campaigns: Vec<DashboardCampaign>,
    recent_activity: Vec<DashboardActivity>,
    platform_health: HashMap<String, i32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DashboardCampaign {
    id: String,
    name: String,
    platforms: Vec<String>,
    status: String,
    progress: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct DashboardActivity {
    time: String,
    status: String,
    message: String,
    platform: Option<String>,
}

#[tauri::command]
fn get_dashboard_stats(state: State<AppState>) -> Result<DashboardStats, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();

    // Count active tasks (pending or running)
    let active_tasks: i32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE status IN ('pending', 'running', 'scheduled')",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    // Count today's posts
    let today_posts: i32 = conn.query_row(
        "SELECT COUNT(*) FROM publish_history WHERE date(published_at) = date('now')",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    // Calculate account health (average across all accounts)
    let total_accounts: i32 = conn.query_row(
        "SELECT COUNT(*) FROM accounts WHERE is_active = 1",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let healthy_accounts: i32 = conn.query_row(
        "SELECT COUNT(*) FROM accounts WHERE is_active = 1 AND status = 'active'",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let account_health = if total_accounts > 0 {
        (healthy_accounts * 100) / total_accounts
    } else {
        0
    };

    // Calculate success rate from recent publish history
    let total_recent: i32 = conn.query_row(
        "SELECT COUNT(*) FROM publish_history WHERE published_at > datetime('now', '-7 days')",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let success_recent: i32 = conn.query_row(
        "SELECT COUNT(*) FROM publish_history WHERE status = 'success' AND published_at > datetime('now', '-7 days')",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let success_rate = if total_recent > 0 {
        (success_recent * 100) / total_recent
    } else {
        100 // Default to 100% if no data
    };

    // Get active campaigns
    let mut campaigns = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, platforms, status FROM campaigns WHERE status IN ('running', 'scheduled') ORDER BY created_at DESC LIMIT 5"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |row| {
            Ok(DashboardCampaign {
                id: row.get(0)?,
                name: row.get(1)?,
                platforms: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                status: row.get(3)?,
                progress: 0, // Will be calculated
            })
        }).map_err(|e| e.to_string())?;

        for row in rows {
            if let Ok(campaign) = row {
                campaigns.push(campaign);
            }
        }
    }

    // Get recent activity
    let mut recent_activity = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT published_at, status, platform, content FROM publish_history ORDER BY published_at DESC LIMIT 10"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |row| {
            let platform: String = row.get(2)?;
            let content: String = row.get::<_, String>(3).unwrap_or_default();
            let status_str: String = row.get(1)?;
            let status = match status_str.as_str() {
                "success" => "completed",
                "failed" => "failed",
                _ => "running",
            };

            let message = format!("{} - {}", platform, if content.len() > 50 { &content[..50] } else { &content });

            Ok(DashboardActivity {
                time: row.get(0)?,
                status: status.to_string(),
                message,
                platform: Some(platform),
            })
        }).map_err(|e| e.to_string())?;

        for row in rows {
            if let Ok(activity) = row {
                recent_activity.push(activity);
            }
        }
    }

    // Calculate platform health
    let mut platform_health = HashMap::new();
    let platforms = vec!["twitter", "reddit", "linkedin", "zhihu", "weibo"];

    for platform in platforms {
        let total: i32 = conn.query_row(
            "SELECT COUNT(*) FROM accounts WHERE platform = ? AND is_active = 1",
            params![platform],
            |row| row.get(0)
        ).unwrap_or(0);

        let healthy: i32 = conn.query_row(
            "SELECT COUNT(*) FROM accounts WHERE platform = ? AND is_active = 1 AND status = 'active'",
            params![platform],
            |row| row.get(0)
        ).unwrap_or(0);

        let health = if total > 0 { (healthy * 100) / total } else { 0 };
        platform_health.insert(platform.to_string(), health);
    }

    Ok(DashboardStats {
        active_tasks,
        today_posts,
        account_health,
        success_rate,
        campaigns,
        recent_activity,
        platform_health,
    })
}

// ===== Proxy Management =====

#[derive(Debug, Serialize, Deserialize)]
struct ProxyInfo {
    id: String,
    name: String,
    protocol: String,
    host: String,
    port: i32,
    username: Option<String>,
    password: Option<String>,
    tags: Vec<String>,
    status: String,
    in_use: bool,
    last_tested: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProxyListResponse {
    proxies: Vec<ProxyInfo>,
    stats: ProxyStats,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProxyStats {
    total: i32,
    active: i32,
    in_use: i32,
    failed: i32,
}

#[tauri::command]
fn list_proxies(state: State<AppState>) -> Result<ProxyListResponse, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let mut proxies = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, protocol, host, port, username, password, tags, status, in_use, last_tested
             FROM proxies ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |row| {
            let tags_str: String = row.get::<_, Option<String>>(7)?.unwrap_or_default();
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

            Ok(ProxyInfo {
                id: row.get(0)?,
                name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                protocol: row.get(2)?,
                host: row.get(3)?,
                port: row.get(4)?,
                username: row.get(5)?,
                password: row.get(6)?,
                tags,
                status: row.get::<_, String>(8)?,
                in_use: row.get::<_, i32>(9)? != 0,
                last_tested: row.get(10)?,
            })
        }).map_err(|e| e.to_string())?;

        for row in rows {
            if let Ok(proxy) = row {
                proxies.push(proxy);
            }
        }
    }

    // Calculate stats
    let total = proxies.len() as i32;
    let active = proxies.iter().filter(|p| p.status == "active").count() as i32;
    let in_use = proxies.iter().filter(|p| p.in_use).count() as i32;
    let failed = proxies.iter().filter(|p| p.status == "failed").count() as i32;

    Ok(ProxyListResponse {
        proxies,
        stats: ProxyStats { total, active, in_use, failed },
    })
}

#[tauri::command]
fn add_proxy(
    state: State<AppState>,
    name: Option<String>,
    protocol: String,
    host: String,
    port: i32,
    username: Option<String>,
    password: Option<String>,
    tags: Vec<String>,
) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();

    let name = name.unwrap_or_else(|| format!("{}:{}", host, port));
    let tags_json = serde_json::to_string(&tags).unwrap_or_default();

    conn.execute(
        "INSERT INTO proxies (id, name, protocol, host, port, username, password, tags, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', datetime('now'))",
        params![id, name, protocol, host, port, username, password, tags_json],
    ).map_err(|e| e.to_string())?;

    log::info!("Added proxy: {} ({}:{})", name, host, port);
    Ok(id)
}

#[tauri::command]
async fn test_proxy(state: State<'_, AppState>, id: String) -> Result<serde_json::Value, String> {
    // Get proxy details
    let (protocol, host, port, username, password): (String, String, i32, Option<String>, Option<String>) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT protocol, host, port, username, password FROM proxies WHERE id = ?",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        ).map_err(|e| e.to_string())?
    };

    // Build proxy URL
    let proxy_url = if let (Some(user), Some(pass)) = (&username, &password) {
        format!("{}://{}:{}@{}:{}", protocol, user, pass, host, port)
    } else {
        format!("{}://{}:{}", protocol, host, port)
    };

    // Test proxy by making a request
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(&proxy_url).map_err(|e| e.to_string())?)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    match client.get("https://httpbin.org/ip").send().await {
        Ok(resp) if resp.status().is_success() => {
            let latency = start.elapsed().as_millis() as i32;

            // Update proxy status
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE proxies SET status = 'active', last_tested = datetime('now'), latency_ms = ? WHERE id = ?",
                params![latency, id],
            ).map_err(|e| e.to_string())?;

            Ok(serde_json::json!({ "success": true, "latency_ms": latency }))
        }
        Ok(resp) => {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE proxies SET status = 'failed', last_tested = datetime('now') WHERE id = ?",
                params![id],
            ).map_err(|e| e.to_string())?;

            Ok(serde_json::json!({ "success": false, "error": format!("HTTP {}", resp.status()) }))
        }
        Err(e) => {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE proxies SET status = 'failed', last_tested = datetime('now') WHERE id = ?",
                params![id],
            ).map_err(|e| e.to_string())?;

            Ok(serde_json::json!({ "success": false, "error": e.to_string() }))
        }
    }
}

#[tauri::command]
fn delete_proxy(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM proxies WHERE id = ?", params![id])
        .map_err(|e| e.to_string())?;

    log::info!("Deleted proxy: {}", id);
    Ok(())
}

#[tauri::command]
fn get_stats(state: State<AppState>, _days: i32) -> Result<Stats, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let total_posts: i32 = conn.query_row(
        "SELECT COUNT(*) FROM publish_history",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let total_views: i32 = conn.query_row(
        "SELECT COALESCE(SUM(views), 0) FROM publish_history",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let total_engagements: i32 = conn.query_row(
        "SELECT COALESCE(SUM(engagements), 0) FROM publish_history",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let avg_engagement_rate = if total_views > 0 {
        total_engagements as f64 / total_views as f64
    } else {
        0.0
    };

    Ok(Stats {
        total_posts,
        total_views,
        total_engagements,
        avg_engagement_rate,
    })
}

#[tauri::command]
fn get_detailed_stats(state: State<AppState>, days: i32) -> Result<DetailedStats, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let days_clause = if days > 0 {
        format!("WHERE published_at >= datetime('now', '-{} days')", days)
    } else {
        String::new()
    };

    // Total stats for current period
    let total_posts: i32 = conn.query_row(
        &format!("SELECT COUNT(*) FROM publish_history {}", days_clause),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let total_views: i32 = conn.query_row(
        &format!("SELECT COALESCE(SUM(views), 0) FROM publish_history {}", days_clause),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let total_engagements: i32 = conn.query_row(
        &format!("SELECT COALESCE(SUM(engagements), 0) FROM publish_history {}", days_clause),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let avg_engagement_rate = if total_views > 0 {
        total_engagements as f64 / total_views as f64
    } else {
        0.0
    };

    // Previous period for comparison
    let prev_clause = if days > 0 {
        format!("WHERE published_at >= datetime('now', '-{} days') AND published_at < datetime('now', '-{} days')", days * 2, days)
    } else {
        String::from("WHERE 1=0")
    };

    let prev_posts: i32 = conn.query_row(
        &format!("SELECT COUNT(*) FROM publish_history {}", prev_clause),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let prev_views: i32 = conn.query_row(
        &format!("SELECT COALESCE(SUM(views), 0) FROM publish_history {}", prev_clause),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let prev_engagements: i32 = conn.query_row(
        &format!("SELECT COALESCE(SUM(engagements), 0) FROM publish_history {}", prev_clause),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let prev_rate = if prev_views > 0 {
        prev_engagements as f64 / prev_views as f64
    } else {
        0.0
    };

    // Daily data
    let mut daily_data = Vec::new();
    let mut stmt = conn.prepare(
        &format!(
            "SELECT date(published_at) as day, COUNT(*) as posts,
             COALESCE(SUM(views), 0) as views, COALESCE(SUM(engagements), 0) as engagements
             FROM publish_history {}
             GROUP BY day ORDER BY day",
            days_clause
        )
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok(DailyStats {
            date: row.get::<_, String>(0)?,
            posts: row.get(1)?,
            views: row.get(2)?,
            engagements: row.get(3)?,
        })
    }).map_err(|e| e.to_string())?;

    for row in rows {
        if let Ok(data) = row {
            daily_data.push(data);
        }
    }

    // Platform stats
    let mut platform_stats = HashMap::new();
    let mut stmt = conn.prepare(
        &format!(
            "SELECT platform, COUNT(*) as count FROM publish_history {} GROUP BY platform",
            days_clause
        )
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
    }).map_err(|e| e.to_string())?;

    for row in rows {
        if let Ok((platform, count)) = row {
            platform_stats.insert(platform, count);
        }
    }

    // Content breakdown
    let articles: i32 = conn.query_row(
        &format!("SELECT COUNT(*) FROM publish_history {} AND content_type = 'article'",
            if days > 0 { &days_clause } else { "WHERE 1=1" }),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let replies: i32 = conn.query_row(
        &format!("SELECT COUNT(*) FROM publish_history {} AND content_type = 'reply'",
            if days > 0 { &days_clause } else { "WHERE 1=1" }),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let reposts: i32 = conn.query_row(
        &format!("SELECT COUNT(*) FROM publish_history {} AND content_type = 'repost'",
            if days > 0 { &days_clause } else { "WHERE 1=1" }),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let comments: i32 = conn.query_row(
        &format!("SELECT COUNT(*) FROM publish_history {} AND content_type = 'comment'",
            if days > 0 { &days_clause } else { "WHERE 1=1" }),
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    // Best content
    let mut best_content = Vec::new();
    let mut stmt = conn.prepare(
        &format!(
            "SELECT id, COALESCE(title, '') as title, COALESCE(content, '') as content,
             platform, COALESCE(views, 0) as views, COALESCE(engagements, 0) as engagements,
             published_at
             FROM publish_history {}
             ORDER BY engagements DESC, views DESC LIMIT 5",
            days_clause
        )
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok(BestContent {
            id: row.get(0)?,
            title: row.get(1)?,
            content: row.get(2)?,
            platform: row.get(3)?,
            views: row.get(4)?,
            engagements: row.get(5)?,
            published_at: row.get(6)?,
        })
    }).map_err(|e| e.to_string())?;

    for row in rows {
        if let Ok(content) = row {
            best_content.push(content);
        }
    }

    // Heatmap data (last 90 days)
    let mut heatmap_data = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT date(published_at) as day, COUNT(*) as count
         FROM publish_history
         WHERE published_at >= datetime('now', '-90 days')
         GROUP BY day ORDER BY day"
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok(HeatmapData {
            date: row.get(0)?,
            count: row.get(1)?,
        })
    }).map_err(|e| e.to_string())?;

    for row in rows {
        if let Ok(data) = row {
            heatmap_data.push(data);
        }
    }

    Ok(DetailedStats {
        total_posts,
        total_views,
        total_engagements,
        avg_engagement_rate,
        posts_change: total_posts - prev_posts,
        views_change: total_views - prev_views,
        engagements_change: total_engagements - prev_engagements,
        rate_change: (avg_engagement_rate - prev_rate) * 100.0,
        daily_data,
        platform_stats,
        content_breakdown: ContentBreakdown {
            articles,
            replies,
            reposts,
            comments,
        },
        best_content,
        heatmap_data,
    })
}

#[tauri::command]
fn get_config(state: State<AppState>) -> Result<Config, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let get_value = |key: &str| -> Option<String> {
        conn.query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![key],
            |row| row.get(0)
        ).ok()
    };

    Ok(Config {
        ai: Some(AiConfig {
            provider: get_value("ai.provider"),
            model: get_value("ai.model"),
            api_key: get_value("ai.api_key"),
        }),
        browser: Some(BrowserConfig {
            unzoo_path: get_value("browser.unzoo_path"),
        }),
        scheduler: Some(SchedulerConfig {
            mode: get_value("scheduler.mode"),
            interval_minutes: get_value("scheduler.interval_minutes").and_then(|v| v.parse().ok()),
            max_daily_posts: get_value("scheduler.max_daily_posts").and_then(|v| v.parse().ok()),
        }),
    })
}

#[tauri::command]
fn set_config(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![key, value],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// 读取「平台是否需要手工登录」配置（config 表内 `platform_manual_login.<platform>` = "true"/"false"）。
/// 默认：segmentfault = true（其 Google OAuth 服务端回调失效，自动/手动 OAuth 都走不通，只能手动登录）。
/// 写入复用 set_config(key="platform_manual_login.<platform>", value="true"|"false")。
#[tauri::command]
fn get_platform_manual_login(state: State<AppState>) -> Result<HashMap<String, bool>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut map: HashMap<String, bool> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT key, value FROM config WHERE key LIKE 'platform_manual_login.%'")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let k: String = row.get(0)?;
                let v: String = row.get(1)?;
                Ok((k, v))
            })
            .map_err(|e| e.to_string())?;
        for r in rows.flatten() {
            if let Some(platform) = r.0.strip_prefix("platform_manual_login.") {
                map.insert(platform.to_string(), r.1 == "true" || r.1 == "1");
            }
        }
    }
    // 未显式配置时的默认：segmentfault 需手工登录
    map.entry("segmentfault".to_string()).or_insert(true);
    Ok(map)
}

#[derive(Serialize)]
struct AIConfig {
    provider: String,
    model: String,
    gemini_key: String,
    openai_key: String,
    deepseek_key: String,
    qwen_key: String,
}

#[tauri::command]
fn configure_ai(
    state: State<AppState>,
    provider: String,
    model: String,
    gemini_key: Option<String>,
    openai_key: Option<String>,
    deepseek_key: Option<String>,
    qwen_key: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("INSERT OR REPLACE INTO config (key, value) VALUES ('ai.provider', ?1)", params![provider]).map_err(|e| e.to_string())?;
    conn.execute("INSERT OR REPLACE INTO config (key, value) VALUES ('ai.model', ?1)", params![model]).map_err(|e| e.to_string())?;

    // Save each provider key if provided
    if let Some(key) = gemini_key {
        if !key.is_empty() {
            conn.execute("INSERT OR REPLACE INTO config (key, value) VALUES ('ai.key.gemini', ?1)", params![key]).map_err(|e| e.to_string())?;
        }
    }
    if let Some(key) = openai_key {
        if !key.is_empty() {
            conn.execute("INSERT OR REPLACE INTO config (key, value) VALUES ('ai.key.openai', ?1)", params![key]).map_err(|e| e.to_string())?;
        }
    }
    if let Some(key) = deepseek_key {
        if !key.is_empty() {
            conn.execute("INSERT OR REPLACE INTO config (key, value) VALUES ('ai.key.deepseek', ?1)", params![key]).map_err(|e| e.to_string())?;
        }
    }
    if let Some(key) = qwen_key {
        if !key.is_empty() {
            conn.execute("INSERT OR REPLACE INTO config (key, value) VALUES ('ai.key.qwen', ?1)", params![key]).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
fn get_ai_config(state: State<AppState>) -> Result<AIConfig, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let get_value = |key: &str| -> String {
        conn.query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![key],
            |row| row.get(0)
        ).unwrap_or_default()
    };

    Ok(AIConfig {
        provider: get_value("ai.provider"),
        model: get_value("ai.model"),
        gemini_key: get_value("ai.key.gemini"),
        openai_key: get_value("ai.key.openai"),
        deepseek_key: get_value("ai.key.deepseek"),
        qwen_key: get_value("ai.key.qwen"),
    })
}

/// 测试 AI 连接：用最小请求验证「Key + 模型」是否可用。
/// `key` 优先用前端传入的（用户刚输入、未必已保存）；为空则回退已保存的 Key。
/// `model` 为前端选中的模型；留空则用该供应商的默认模型。
/// 返回成功提示串，失败返回可读错误（HTTP 状态 + 供应商错误信息）。
#[tauri::command]
async fn test_ai_connection(
    state: State<'_, AppState>,
    provider: String,
    key: Option<String>,
    model: Option<String>,
) -> Result<String, String> {
    // 解析 Key：前端传入优先，否则回退数据库已保存的
    let api_key = match key {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            let key_name = format!("ai.key.{}", provider);
            conn.query_row(
                "SELECT value FROM config WHERE key = ?1",
                params![key_name],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .unwrap_or_default()
        }
    };
    if api_key.is_empty() {
        return Err("未配置 API Key —— 请先在上面填入该供应商的 Key（或先点保存）".to_string());
    }

    let model = model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let used_model = match provider.as_str() {
        "gemini" => test_gemini_connection(&client, &api_key, model.as_deref()).await?,
        "openai" => {
            test_openai_compatible(&client, "https://api.openai.com", &api_key,
                model.as_deref().unwrap_or("gpt-4o-mini")).await?
        }
        "deepseek" => {
            test_openai_compatible(&client, "https://api.deepseek.com", &api_key,
                model.as_deref().unwrap_or("deepseek-chat")).await?
        }
        "qwen" => test_qwen_connection(&client, &api_key, model.as_deref()).await?,
        other => return Err(format!("未知的 AI 供应商：{}", other)),
    };

    Ok(format!("✓ 连接成功（{} / {}）", provider, used_model))
}

// 提取供应商 JSON 错误体里的 error.message（OpenAI 风格）或顶层 message（DashScope 风格）
fn extract_api_error_message(json: &serde_json::Value) -> String {
    json.get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .or_else(|| json.get("message").and_then(|m| m.as_str()))
        .unwrap_or("未知错误")
        .to_string()
}

async fn test_gemini_connection(
    client: &reqwest::Client,
    api_key: &str,
    model: Option<&str>,
) -> Result<String, String> {
    let model = model.unwrap_or("gemini-2.0-flash");
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );
    let body = serde_json::json!({
        "contents": [{"parts": [{"text": "ping"}]}],
        "generationConfig": {"maxOutputTokens": 1}
    });
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败：{}", e))?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败：{}", e))?;
    if !status.is_success() {
        return Err(format!("HTTP {}：{}", status.as_u16(), extract_api_error_message(&json)));
    }
    Ok(model.to_string())
}

// OpenAI 兼容接口（OpenAI / DeepSeek 等）：POST {base}/v1/chat/completions
async fn test_openai_compatible(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1
    });
    let resp = client
        .post(format!("{}/v1/chat/completions", base))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败：{}", e))?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败：{}", e))?;
    if !status.is_success() {
        return Err(format!("HTTP {}：{}", status.as_u16(), extract_api_error_message(&json)));
    }
    Ok(model.to_string())
}

async fn test_qwen_connection(
    client: &reqwest::Client,
    api_key: &str,
    model: Option<&str>,
) -> Result<String, String> {
    let model = model.unwrap_or("qwen-turbo");
    let body = serde_json::json!({
        "model": model,
        "input": {"messages": [{"role": "user", "content": "ping"}]},
        "parameters": {"max_tokens": 1}
    });
    let resp = client
        .post("https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败：{}", e))?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败：{}", e))?;
    if !status.is_success() {
        return Err(format!("HTTP {}：{}", status.as_u16(), extract_api_error_message(&json)));
    }
    Ok(model.to_string())
}

#[tauri::command]
fn check_browser_status() -> bool {
    // Check if Unzoo CLI is available
    std::process::Command::new("unzoo")
        .arg("--version")
        .output()
        .is_ok()
}

#[derive(Debug, Clone, Serialize)]
struct Article {
    id: String,
    product_id: String,
    product_name: String,
    article_type: String,
    platform: String,
    language: String,
    title: String,
    body: String,
    keywords: Vec<String>,
    word_count: i32,
    created_at: String,
}

#[tauri::command]
async fn generate_article(
    state: State<'_, AppState>,
    product_id: String,
    article_type: String,
    platforms: Vec<String>,
    languages: Vec<String>,
    keywords: Vec<String>,
    tone: String,
) -> Result<Vec<Article>, String> {
    // Get product info
    let product = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, name, url, tagline, description, product_type FROM products WHERE id = ?",
            params![product_id],
            |row| Ok(Product {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                tagline: row.get(3)?,
                description: row.get(4)?,
                product_type: row.get(5)?,
                priority: 5,
                weight: 5,
                created_at: String::new(),
            })
        ).map_err(|e| format!("Product not found: {}", e))?
    };

    // Get AI config
    let (provider, api_key) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let get_value = |key: &str| -> Option<String> {
            conn.query_row("SELECT value FROM config WHERE key = ?1", params![key], |row| row.get(0)).ok()
        };
        let provider = get_value("ai.provider").unwrap_or_else(|| "gemini".to_string());
        let key = match provider.as_str() {
            "gemini" => get_value("ai.key.gemini"),
            "openai" => get_value("ai.key.openai"),
            "deepseek" => get_value("ai.key.deepseek"),
            "qwen" => get_value("ai.key.qwen"),
            _ => get_value("ai.key.gemini"),
        };
        (provider, key.unwrap_or_default())
    };

    let mut articles = Vec::new();
    let client = reqwest::Client::new();
    let now = chrono::Local::now();

    // Platform-specific word counts
    let get_target_words = |platform: &str| -> (i32, i32) {
        match platform {
            "zhihu" => (1000, 2000),
            "wechat" => (800, 1500),
            "medium" | "devto" | "blog" => (1200, 2500),
            "toutiao" => (600, 1200),
            "reddit" => (300, 800),
            _ => (500, 1000),
        }
    };

    // Article type descriptions
    let type_desc = match article_type.as_str() {
        "tutorial" => "a step-by-step tutorial/guide",
        "comparison" => "a comparison article with alternatives",
        "problem" => "a problem-solving article (pain point → solution)",
        "story" => "a user story or case study",
        "listicle" => "a listicle (top 10, best X tools)",
        _ => "an informative article",
    };

    for platform in &platforms {
        for language in &languages {
            let (min_words, max_words) = get_target_words(platform);
            let lang_name = match language.as_str() {
                "en" => "English",
                "zh" => "Chinese (Simplified)",
                _ => "English",
            };

            let keywords_str = keywords.join(", ");

            // 构建详细的产品上下文
            let tagline = product.tagline.as_deref().unwrap_or("").trim();
            let description = product.description.as_deref().unwrap_or("").trim();

            let product_context = if description.len() > 50 {
                format!("- Name: {}\n- Tagline: {}\n- Description: {}\n- Website: {}",
                    product.name, tagline, description, product.url)
            } else if !tagline.is_empty() {
                format!("- Name: {}\n- Tagline: {}\n- Website: {}\n(Note: Infer product features from the name and tagline)",
                    product.name, tagline, product.url)
            } else {
                format!("- Name: {}\n- Website: {}\n(Note: The product name suggests its function - write about what '{}' likely does)",
                    product.name, product.url, product.name)
            };

            let prompt = format!(
                r#"You are an expert content writer. Write {type_desc} for {platform}.

=== PRODUCT TO WRITE ABOUT ===
{product_context}

=== ARTICLE REQUIREMENTS ===
1. This article MUST be specifically about "{product_name}"
2. Explain what "{product_name}" does and why it's useful
3. The product name "{product_name}" must appear multiple times naturally
4. Include the URL {url} at least once
5. Language: {lang}
6. Word count: {min}-{max} words
7. Tone: {tone}
8. SEO keywords to include: {keywords}
9. Use proper formatting for {platform} (headers, lists, etc.)
10. Be informative and valuable, not overly promotional

=== OUTPUT FORMAT ===
TITLE: [Write a compelling title that includes "{product_name}"]
BODY:
[Write the full article here]"#,
                type_desc = type_desc,
                platform = platform,
                product_context = product_context,
                product_name = product.name,
                url = product.url,
                lang = lang_name,
                min = min_words,
                max = max_words,
                tone = tone,
                keywords = keywords_str
            );

            let response = if !api_key.is_empty() {
                match provider.as_str() {
                    "gemini" => call_gemini_api(&client, &api_key, &prompt).await.ok(),
                    "openai" => call_openai_api(&client, &api_key, &prompt).await.ok(),
                    "deepseek" => call_deepseek_api(&client, &api_key, &prompt).await.ok(),
                    "qwen" => call_qwen_api(&client, &api_key, &prompt).await.ok(),
                    _ => None,
                }
            } else {
                None
            };

            let (title, body) = if let Some(content) = response {
                // Parse title and body from response
                let mut title = String::new();
                let mut body = String::new();
                let mut in_body = false;

                for line in content.lines() {
                    if line.starts_with("TITLE:") {
                        title = line.trim_start_matches("TITLE:").trim().to_string();
                    } else if line.starts_with("BODY:") {
                        in_body = true;
                    } else if in_body {
                        if !body.is_empty() {
                            body.push('\n');
                        }
                        body.push_str(line);
                    }
                }

                if title.is_empty() {
                    title = format!("{}: {}", product.name, type_desc);
                }
                if body.is_empty() {
                    body = content;
                }

                (title, body)
            } else {
                // Generate template article if AI unavailable
                generate_template_article(&product, &article_type, platform, language, &keywords)
            };

            let word_count = body.split_whitespace().count() as i32;

            articles.push(Article {
                id: Uuid::new_v4().to_string(),
                product_id: product_id.clone(),
                product_name: product.name.clone(),
                article_type: article_type.clone(),
                platform: platform.clone(),
                language: language.clone(),
                title,
                body,
                keywords: keywords.clone(),
                word_count,
                created_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            });
        }
    }

    Ok(articles)
}

fn generate_template_article(product: &Product, article_type: &str, platform: &str, language: &str, keywords: &[String]) -> (String, String) {
    let name = &product.name;
    let tagline = product.tagline.as_deref().unwrap_or("");
    let description = product.description.as_deref().unwrap_or("");
    let url = &product.url;
    let keywords_str = keywords.join(", ");

    match (article_type, language) {
        ("tutorial", "zh") => (
            format!("{} 完整使用指南", name),
            format!(
                r#"# {} 完整使用指南

## 引言

在当今快节奏的工作环境中，{}成为了提高效率的重要工具。本文将详细介绍如何使用 {} 来优化你的工作流程。

## 什么是 {}？

{} - {}

{}

## 快速开始

### 第一步：访问产品

前往官网：{}

### 第二步：了解核心功能

{} 的核心功能包括：
- 高效的工作流程管理
- 直观的用户界面
- 强大的集成能力

### 第三步：开始使用

按照界面引导，即可快速上手使用。

## 进阶技巧

掌握以下技巧可以让你的使用更加得心应手：

1. **自定义设置** - 根据个人需求调整配置
2. **快捷操作** - 使用快捷键提高效率
3. **数据管理** - 合理组织和备份数据

## 总结

{} 是一个值得尝试的工具，能够有效提升工作效率。

相关关键词：{}"#,
                name, keywords_str, name, name, name, tagline, description, url, name, name, keywords_str
            )
        ),
        ("tutorial", "en") | ("tutorial", _) => (
            format!("Complete Guide to {}", name),
            format!(
                r#"# Complete Guide to {}

## Introduction

In today's fast-paced work environment, having the right tools is essential for productivity. This guide will walk you through everything you need to know about {}.

## What is {}?

{} - {}

{}

## Getting Started

### Step 1: Access the Product

Visit the official website: {}

### Step 2: Explore Core Features

{} offers several key features:
- Efficient workflow management
- Intuitive user interface
- Powerful integration capabilities

### Step 3: Start Using

Follow the on-screen guidance to get started quickly.

## Advanced Tips

Master these techniques to maximize your productivity:

1. **Customize Settings** - Adjust configurations to your needs
2. **Use Shortcuts** - Speed up your workflow with keyboard shortcuts
3. **Data Management** - Organize and backup your data effectively

## Conclusion

{} is a valuable tool that can significantly improve your workflow efficiency.

Keywords: {}"#,
                name, name, name, name, tagline, description, url, name, name, keywords_str
            )
        ),
        ("comparison", "zh") => (
            format!("{} vs 竞品对比：哪个更适合你？", name),
            format!(
                r#"# {} vs 竞品对比

## 概述

市场上有很多类似的工具，本文将对比分析 {} 与其他竞品的优劣。

## {} 简介

{} - {}

{}

官网：{}

## 对比分析

| 特性 | {} | 其他工具 |
|------|------|----------|
| 易用性 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ |
| 功能完整度 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ |
| 性价比 | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ |

## 选择建议

如果你需要：{}，{} 是理想选择。

关键词：{}"#,
                name, name, name, name, tagline, description, url, name, keywords_str, name, keywords_str
            )
        ),
        ("comparison", "en") | ("comparison", _) => (
            format!("{} vs Alternatives: Which One is Right for You?", name),
            format!(
                r#"# {} vs Alternatives

## Overview

There are many similar tools on the market. This article compares {} with other alternatives.

## About {}

{} - {}

{}

Website: {}

## Comparison

| Feature | {} | Others |
|---------|------|--------|
| Ease of Use | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ |
| Feature Set | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ |
| Value | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ |

## Recommendation

If you need: {}, {} is an excellent choice.

Keywords: {}"#,
                name, name, name, name, tagline, description, url, name, keywords_str, name, keywords_str
            )
        ),
        _ => (
            format!("Discover {}", name),
            format!(
                r#"# Discover {}

{}

{}

Learn more: {}

Keywords: {}"#,
                name, tagline, description, url, keywords_str
            )
        ),
    }
}

// ============================================================================
// Unzoo REST API Functions
// ============================================================================

pub(crate) fn get_http_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

/// Shared blocking HTTP client.
///
/// reqwest::blocking::Client owns an internal Tokio runtime; building one per
/// call and dropping it inside an async context (e.g. the task engine running
/// on the Tauri/Tokio runtime) panics with "Cannot drop a runtime ...".
/// A single shared client (held forever in a OnceLock) is never dropped, so
/// per-call clones can be created/dropped freely from any context. Clones share
/// the same connection pool + runtime.
pub(crate) fn get_blocking_client() -> reqwest::blocking::Client {
    static BLOCKING_CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    BLOCKING_CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build shared blocking client")
        })
        .clone()
}

/// 快速探测 Unzoo REST 是否在线（短超时），用于发布前给出清晰提示。
fn unzoo_rest_up() -> bool {
    get_blocking_client()
        .get(format!("{}/tabs", UNZOO_API_BASE))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

// Profile Management
#[tauri::command]
async fn unzoo_list_profiles() -> Result<Vec<BrowserProfile>, String> {
    let client = get_http_client();
    let url = format!("{}/profiles", UNZOO_API_BASE);

    let resp = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to Unzoo: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Unzoo API error: {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // 实际 /profiles 返回 {data:{profiles:[{name, path}]}}，无 id 字段；
    // 引擎用的 profile_id = path 末段文件夹名（如 Profile_TestProfile1）。
    let profiles = data.get("data").and_then(|d| d.get("profiles"))
        .or_else(|| data.get("profiles"))
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter().filter_map(|p| {
                let path = p.get("path").and_then(|x| x.as_str()).unwrap_or("");
                let folder = path.rsplit(|c| c == '\\' || c == '/').next().filter(|s| !s.is_empty());
                let id = p.get("id").and_then(|x| x.as_str()).map(|s| s.to_string())
                    .or_else(|| folder.map(|s| s.to_string()))?;
                let name = p.get("name").and_then(|x| x.as_str())
                    .filter(|s| !s.is_empty()).unwrap_or(&id).to_string();
                Some(BrowserProfile {
                    id,
                    name,
                    platform: p.get("group").and_then(|g| g.as_str()).unwrap_or("default").to_string(),
                    account_id: None,
                    fingerprint_id: None,
                    proxy: p.get("proxy").and_then(|pr| pr.as_str()).map(|s| s.to_string()),
                    stealth_enabled: true,
                    created_at: String::new(),
                })
            }).collect()
        })
        .unwrap_or_default();

    Ok(profiles)
}

#[tauri::command]
async fn unzoo_create_profile(request: ProfileCreateRequest) -> Result<BrowserProfile, String> {
    let client = get_http_client();
    let url = format!("{}/profiles/create", UNZOO_API_BASE);

    let mut tags = vec![];
    if let Some(ref account_id) = request.account_id {
        tags.push(format!("account:{}", account_id));
    }
    tags.push(format!("platform:{}", request.platform));

    let body = serde_json::json!({
        "name": request.name,
        "group": request.platform,
        "tags": tags
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to create profile: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to create profile: {}", error_text));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let profile_id = data.get("id")
        .or_else(|| data.get("profile_id"))
        .and_then(|id| id.as_str())
        .unwrap_or(&request.name)
        .to_string();

    // Set proxy if provided
    if let Some(ref proxy) = request.proxy {
        let _ = unzoo_set_profile_proxy(&profile_id, proxy).await;
    }

    // Randomize fingerprint for new profile
    let _ = unzoo_randomize_fingerprint(&profile_id).await;

    Ok(BrowserProfile {
        id: profile_id,
        name: request.name,
        platform: request.platform,
        account_id: request.account_id,
        fingerprint_id: None,
        proxy: request.proxy,
        stealth_enabled: true,
        created_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

#[tauri::command]
async fn unzoo_delete_profile(profile_id: String) -> Result<(), String> {
    let client = get_http_client();
    let url = format!("{}/profiles/delete", UNZOO_API_BASE);

    let body = serde_json::json!({
        "profile_id": profile_id
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to delete profile: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to delete profile: {}", resp.status()));
    }

    Ok(())
}

#[tauri::command]
async fn unzoo_launch_profile(profile_id: String) -> Result<String, String> {
    let client = get_http_client();
    // Use /tabs/create endpoint instead of /profiles/launch
    let url = format!("{}/tabs/create", UNZOO_API_BASE);

    let body = serde_json::json!({
        "profile_id": profile_id
    });

    log::info!("[BROWSER] Launching profile via tabs/create: {}", profile_id);

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to launch profile: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        log::error!("[BROWSER] Launch failed: {} - {}", status, body);
        return Err(format!("Failed to launch profile: {} - {}", status, body));
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    log::info!("[BROWSER] Launch response: {}", data);

    // Try to get tab_id from response - it may be a number or string
    let tab_id = data.get("data")
        .and_then(|d| d.get("tab_id"))
        .map(|t| {
            if let Some(n) = t.as_i64() {
                n.to_string()
            } else if let Some(s) = t.as_str() {
                s.to_string()
            } else {
                String::new()
            }
        })
        .unwrap_or_default();

    Ok(tab_id)
}

// Fingerprint Management
#[tauri::command]
async fn unzoo_randomize_fingerprint(profile_id: &str) -> Result<UnzooFingerprint, String> {
    let client = get_http_client();
    let url = format!("{}/profiles/fingerprint/randomize", UNZOO_API_BASE);

    let body = serde_json::json!({
        "profile_id": profile_id,
        "components": ["gpu", "canvas", "audio", "webgl"]
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to randomize fingerprint: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to randomize fingerprint: {}", resp.status()));
    }

    let fingerprint: UnzooFingerprint = resp.json().await
        .unwrap_or(UnzooFingerprint {
            gpu: Some("randomized".to_string()),
            canvas: Some("randomized".to_string()),
            audio: Some("randomized".to_string()),
            webgl: Some("randomized".to_string()),
        });

    log::info!("Fingerprint randomized for profile {}", profile_id);
    Ok(fingerprint)
}

#[tauri::command]
async fn unzoo_get_fingerprint(profile_id: String) -> Result<UnzooFingerprint, String> {
    let client = get_http_client();
    let url = format!("{}/profiles/fingerprint?profile_id={}", UNZOO_API_BASE, profile_id);

    let resp = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to get fingerprint: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to get fingerprint: {}", resp.status()));
    }

    let fingerprint: UnzooFingerprint = resp.json().await
        .map_err(|e| format!("Failed to parse fingerprint: {}", e))?;

    Ok(fingerprint)
}

// Performance/Stealth Mode
#[tauri::command]
async fn unzoo_set_stealth_mode(mode: String) -> Result<(), String> {
    let client = get_http_client();
    let url = format!("{}/performance/mode", UNZOO_API_BASE);

    // Valid modes: "full", "light", "stealth"
    let body = serde_json::json!({
        "mode": mode
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to set stealth mode: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to set stealth mode: {}", resp.status()));
    }

    log::info!("Stealth mode set to: {}", mode);
    Ok(())
}

#[tauri::command]
async fn unzoo_get_performance_mode() -> Result<String, String> {
    let client = get_http_client();
    let url = format!("{}/performance/mode", UNZOO_API_BASE);

    let resp = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to get performance mode: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to get performance mode: {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    let mode = data.get("mode")
        .and_then(|m| m.as_str())
        .unwrap_or("full")
        .to_string();

    Ok(mode)
}

// Proxy Management
async fn unzoo_set_profile_proxy(profile_id: &str, proxy: &str) -> Result<(), String> {
    let client = get_http_client();
    let url = format!("{}/profiles/proxy", UNZOO_API_BASE);

    let body = serde_json::json!({
        "profile_id": profile_id,
        "proxy": proxy
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to set proxy: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to set proxy: {}", resp.status()));
    }

    log::info!("Proxy set for profile {}: {}", profile_id, proxy);
    Ok(())
}

#[tauri::command]
async fn unzoo_update_profile_proxy(profile_id: String, proxy: Option<String>) -> Result<(), String> {
    let client = get_http_client();
    let url = format!("{}/profiles/proxy", UNZOO_API_BASE);

    let body = if let Some(ref p) = proxy {
        serde_json::json!({
            "profile_id": profile_id,
            "proxy": p
        })
    } else {
        serde_json::json!({
            "profile_id": profile_id,
            "proxy": null
        })
    };

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to update proxy: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to update proxy: {}", resp.status()));
    }

    Ok(())
}

// Scheduler Management
#[tauri::command]
async fn unzoo_list_scheduled_jobs() -> Result<Vec<UnzooScheduledJob>, String> {
    let client = get_http_client();
    let url = format!("{}/scheduler", UNZOO_API_BASE);

    let resp = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to list scheduled jobs: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to list scheduled jobs: {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    let jobs = data.get("jobs")
        .and_then(|j| j.as_array())
        .map(|arr| {
            arr.iter().filter_map(|j| {
                Some(UnzooScheduledJob {
                    id: j.get("id")?.as_str()?.to_string(),
                    name: j.get("name")?.as_str()?.to_string(),
                    cron: j.get("cron").and_then(|c| c.as_str()).unwrap_or("").to_string(),
                    enabled: j.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                    last_run: j.get("last_run").and_then(|l| l.as_str()).map(|s| s.to_string()),
                    next_run: j.get("next_run").and_then(|n| n.as_str()).map(|s| s.to_string()),
                })
            }).collect()
        })
        .unwrap_or_default();

    Ok(jobs)
}

#[tauri::command]
async fn unzoo_create_scheduled_job(name: String, cron: String, task_id: String) -> Result<String, String> {
    let client = get_http_client();
    let url = format!("{}/scheduler", UNZOO_API_BASE);

    // Create a job that will trigger a webhook or MCP tool
    let body = serde_json::json!({
        "name": name,
        "cron": cron,
        "action": {
            "type": "webhook",
            "url": format!("http://127.0.0.1:1420/api/execute-task/{}", task_id)
        }
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to create scheduled job: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to create scheduled job: {}", error_text));
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    let job_id = data.get("id")
        .or_else(|| data.get("job_id"))
        .and_then(|id| id.as_str())
        .unwrap_or("")
        .to_string();

    log::info!("Created scheduled job: {} ({})", name, job_id);
    Ok(job_id)
}

#[tauri::command]
async fn unzoo_delete_scheduled_job(job_id: String) -> Result<(), String> {
    let client = get_http_client();
    let url = format!("{}/scheduler/{}", UNZOO_API_BASE, job_id);

    let resp = client.delete(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to delete scheduled job: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to delete scheduled job: {}", resp.status()));
    }

    Ok(())
}

#[tauri::command]
async fn unzoo_pause_scheduled_job(job_id: String) -> Result<(), String> {
    let client = get_http_client();
    let url = format!("{}/scheduler/{}/pause", UNZOO_API_BASE, job_id);

    let resp = client.post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to pause scheduled job: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to pause scheduled job: {}", resp.status()));
    }

    Ok(())
}

#[tauri::command]
async fn unzoo_resume_scheduled_job(job_id: String) -> Result<(), String> {
    let client = get_http_client();
    let url = format!("{}/scheduler/{}/resume", UNZOO_API_BASE, job_id);

    let resp = client.post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to resume scheduled job: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to resume scheduled job: {}", resp.status()));
    }

    Ok(())
}

// Cookie Management
#[tauri::command]
async fn unzoo_get_cookies(url: String) -> Result<Vec<UnzooCookie>, String> {
    let client = get_http_client();
    let api_url = format!("{}/cookies?url={}", UNZOO_API_BASE, urlencoding::encode(&url));

    let resp = client.get(&api_url)
        .send()
        .await
        .map_err(|e| format!("Failed to get cookies: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to get cookies: {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    let cookies = data.get("cookies")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter().filter_map(|c| {
                Some(UnzooCookie {
                    name: c.get("name")?.as_str()?.to_string(),
                    value: c.get("value")?.as_str()?.to_string(),
                    domain: c.get("domain")?.as_str()?.to_string(),
                    path: c.get("path").and_then(|p| p.as_str()).map(|s| s.to_string()),
                    secure: c.get("secure").and_then(|s| s.as_bool()).unwrap_or(false),
                    http_only: c.get("httpOnly").and_then(|h| h.as_bool()).unwrap_or(false),
                    expires: c.get("expires").and_then(|e| e.as_f64()),
                })
            }).collect()
        })
        .unwrap_or_default();

    Ok(cookies)
}

#[tauri::command]
async fn unzoo_set_cookie(cookie: UnzooCookie) -> Result<(), String> {
    let client = get_http_client();
    let url = format!("{}/cookies", UNZOO_API_BASE);

    let body = serde_json::json!({
        "name": cookie.name,
        "value": cookie.value,
        "domain": cookie.domain,
        "path": cookie.path.unwrap_or("/".to_string()),
        "secure": cookie.secure,
        "httpOnly": cookie.http_only,
        "expires": cookie.expires
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to set cookie: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to set cookie: {}", resp.status()));
    }

    Ok(())
}

#[tauri::command]
async fn unzoo_import_cookies(cookies_json: String) -> Result<i32, String> {
    let client = get_http_client();
    let url = format!("{}/cookies/import", UNZOO_API_BASE);

    let body = serde_json::json!({
        "cookies": cookies_json
    });

    let resp = client.post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to import cookies: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to import cookies: {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();
    let count = data.get("imported")
        .and_then(|i| i.as_i64())
        .unwrap_or(0) as i32;

    Ok(count)
}

// Enhanced publish with profile support
#[tauri::command]
async fn publish_with_profile(
    profile_id: String,
    content: Content,
    state: State<'_, AppState>,
) -> Result<PublishResult, String> {
    // 1. Launch the profile
    let tab_id = unzoo_launch_profile(profile_id.clone()).await?;

    // 2. Set stealth mode
    let _ = unzoo_set_stealth_mode("stealth".to_string()).await;

    // 3. Perform the publish using existing logic
    let result = publish_content(state, content).await;

    // The profile stays open for subsequent operations
    result
}

// Get or create profile for an account
#[tauri::command]
async fn ensure_account_profile(
    account_id: String,
    platform: String,
) -> Result<BrowserProfile, String> {
    // Check if profile already exists
    let profiles = unzoo_list_profiles().await?;

    for profile in &profiles {
        if profile.account_id.as_ref() == Some(&account_id) {
            return Ok(profile.clone());
        }
    }

    // Create new profile for this account
    let profile_name = format!("{}_{}", platform, account_id);
    let request = ProfileCreateRequest {
        name: profile_name,
        platform: platform.clone(),
        account_id: Some(account_id),
        proxy: None,
    };

    unzoo_create_profile(request).await
}

// ============================================================================
// Browser Profile Selection
// ============================================================================

/// Get available Unzoo Browser profiles for selection
#[tauri::command]
async fn get_available_browser_profiles() -> Result<Vec<serde_json::Value>, String> {
    log::info!("[PROFILES] Fetching available browser profiles...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| {
            log::error!("[PROFILES] Failed to create client: {}", e);
            format!("Failed to create client: {}", e)
        })?;

    let url = "http://127.0.0.1:9399/api/v1/profiles";
    log::info!("[PROFILES] Requesting: {}", url);

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| {
            log::error!("[PROFILES] Failed to connect to Unzoo: {}", e);
            format!("Failed to connect to Unzoo: {}", e)
        })?;

    if !resp.status().is_success() {
        log::error!("[PROFILES] Unzoo API error: {}", resp.status());
        return Err(format!("Unzoo API error: {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        log::error!("[PROFILES] Parse error: {}", e);
        format!("Parse error: {}", e)
    })?;

    log::info!("[PROFILES] Response received: {}", data);

    // Extract profiles with user-friendly info
    if let Some(profiles) = data.get("data").and_then(|d| d.get("profiles")).and_then(|p| p.as_array()) {
        log::info!("[PROFILES] Found {} profiles", profiles.len());
        let result: Vec<serde_json::Value> = profiles
            .iter()
            .map(|p| {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("Unknown");
                let path = p.get("path").and_then(|n| n.as_str()).unwrap_or("");
                // Extract profile ID = path 末段文件夹名（兼容 / 与 \ 两种分隔符，
                // 与 personas.profile_id / accounts.profile_id 保持一致，#6 才能映射出可读名）
                let profile_id = path
                    .rsplit(|c| c == '/' || c == '\\')
                    .find(|s| !s.is_empty())
                    .unwrap_or(path);

                log::info!("[PROFILES] Profile: {} -> {}", name, profile_id);
                serde_json::json!({
                    "id": profile_id,
                    "name": name,
                    "path": path
                })
            })
            .collect();
        Ok(result)
    } else {
        log::error!("[PROFILES] No profiles found in response structure");
        Err("No profiles found in response".to_string())
    }
}

/// Save selected browser profile to config
#[tauri::command]
fn save_selected_browser_profile(state: State<AppState>, profile_id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES ('selected_browser_profile', ?1)",
        params![profile_id],
    )
    .map_err(|e| e.to_string())?;
    log::info!("[PROFILE] Saved selected browser profile: {}", profile_id);
    Ok(())
}

/// Get selected browser profile from config
#[tauri::command]
fn get_selected_browser_profile(state: State<AppState>) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let profile_id: String = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'selected_browser_profile'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    Ok(profile_id)
}

/// Connect to a browser profile - saves the profile and launches it
#[tauri::command]
async fn connect_browser_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<serde_json::Value, String> {
    log::info!("[PROFILE] Connecting to browser profile: {}", profile_id);

    // Save the profile to config
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES ('selected_browser_profile', ?1)",
            params![profile_id.clone()],
        )
        .map_err(|e| e.to_string())?;
    }

    // Launch the profile
    let tab_id = unzoo_launch_profile(profile_id.clone()).await?;

    // Save the active tab
    set_active_tab(Some(tab_id.clone()));

    log::info!("[PROFILE] Connected successfully, tab_id: {}", tab_id);

    Ok(serde_json::json!({
        "success": true,
        "profile_id": profile_id,
        "tab_id": tab_id,
        "message": format!("Connected to profile: {}", profile_id)
    }))
}

/// Get browser connection status
#[tauri::command]
async fn get_browser_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let selected_profile = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT value FROM config WHERE key = 'selected_browser_profile'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default()
    };

    let active_tab = get_active_tab();

    // Check if browser is actually connected
    let client = get_http_client();
    let tabs_url = format!("{}/tabs", UNZOO_API_BASE);

    let is_connected = match client.get(&tabs_url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                !text.contains("Not connected")
            } else {
                false
            }
        }
        Err(_) => false
    };

    Ok(serde_json::json!({
        "connected": is_connected,
        "selected_profile": selected_profile,
        "active_tab": active_tab,
        "has_profile_selected": !selected_profile.is_empty()
    }))
}

/// Bind a browser profile to an account
#[tauri::command]
fn bind_account_profile(
    state: State<AppState>,
    account_id: String,
    profile_id: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE accounts SET profile_id = ?1 WHERE id = ?2",
        params![profile_id, account_id],
    )
    .map_err(|e| e.to_string())?;
    log::info!("[ACCOUNT] Bound profile {} to account {}", profile_id, account_id);
    Ok(())
}

/// Unbind profile from an account (use global profile)
#[tauri::command]
fn unbind_account_profile(state: State<AppState>, account_id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE accounts SET profile_id = NULL WHERE id = ?1",
        params![account_id],
    )
    .map_err(|e| e.to_string())?;
    log::info!("[ACCOUNT] Unbound profile from account {}", account_id);
    Ok(())
}

/// Get account with its profile info
#[tauri::command]
fn get_account_with_profile(state: State<AppState>, account_id: String) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // First get required fields
    let result: Result<(String, String, Option<String>), _> = conn.query_row(
        "SELECT id, platform, username FROM accounts WHERE id = ?1",
        params![account_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );

    match result {
        Ok((id, platform, username)) => {
            // Then try to get profile_id (may not exist in old databases)
            let profile_id: Option<String> = conn.query_row(
                "SELECT profile_id FROM accounts WHERE id = ?1",
                params![account_id],
                |row| row.get(0),
            ).ok().flatten();

            Ok(serde_json::json!({
                "id": id,
                "platform": platform,
                "username": username,
                "profile_id": profile_id,
                "has_profile": profile_id.is_some()
            }))
        },
        Err(e) => Err(format!("Account not found: {}", e)),
    }
}

// ============================================================================
// Account Nurturing (养号功能)
// ============================================================================

/// Check if user is logged in to a platform by analyzing page content
fn check_platform_login_status(platform: &str) -> Result<bool, String> {
    // Wait a moment for page to load
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 用 /evaluate 取 innerText（实测比 /get-text 可靠，后者对 SPA 常返回空）。
    let page_text = {
        let raw = unzoo_evaluate("(document.body.innerText||'').slice(0,3000)").unwrap_or_default();
        let decoded = serde_json::from_str::<String>(&raw).unwrap_or(raw);
        if decoded.trim().is_empty() {
            unzoo_get_text().unwrap_or_default() // 退回 /get-text
        } else {
            decoded
        }
    }.to_lowercase();

    // Check for login indicators based on platform
    let (logged_in_indicators, logged_out_indicators) = match platform.to_lowercase().as_str() {
        "twitter" | "x" => (
            vec!["home", "post", "messages", "notifications", "profile"],
            vec!["log in", "sign up", "create account", "join twitter", "sign in to x"]
        ),
        "reddit" => (
            vec!["create post", "user settings", "karma", "get coins"],
            vec!["log in", "sign up", "join reddit"]
        ),
        "linkedin" => (
            vec!["messaging", "notifications", "start a post", "my network"],
            vec!["sign in", "join now", "join linkedin"]
        ),
        "zhihu" => (
            vec!["写回答", "发想法", "创作中心", "私信", "消息"],
            vec!["登录", "注册", "加入知乎"]
        ),
        "xiaohongshu" | "redbook" => (
            vec!["发布笔记", "消息", "我", "创作"],
            vec!["登录", "注册", "立即登录"]
        ),
        "weibo" => (
            vec!["首页", "私信", "发微博", "粉丝"],
            vec!["登录", "注册", "立即登录"]
        ),
        "v2ex" => (
            vec!["节点", "创建新主题", "未读提醒", "设置"],
            vec!["登录", "注册", "sign in", "sign up"]
        ),
        "producthunt" => (
            vec!["submit", "notifications", "profile", "my products"],
            vec!["log in", "sign up", "get started"]
        ),
        "hackernews" | "hn" => (
            vec!["submit", "threads", "logout"],
            vec!["login"]
        ),
        "indiehackers" => (
            vec!["new post", "notifications", "profile"],
            vec!["log in", "sign up", "join"]
        ),
        "github" => (
            vec!["pull requests", "issues", "your repositories", "your profile"],
            vec!["sign in", "sign up", "create account"]
        ),
        "hashnode" => (
            vec!["write", "dashboard", "notifications", "draft"],
            vec!["log in", "sign up", "create account", "get started"]
        ),
        "dev.to" | "devto" => (
            vec!["write post", "dashboard", "notifications"],
            vec!["log in", "create account", "sign up"]
        ),
        "medium" => (
            vec!["write", "stories", "notifications", "new story"],
            vec!["sign in", "get started", "sign up"]
        ),
        _ => (
            vec!["profile", "settings", "logout", "notifications"],
            vec!["log in", "sign in", "sign up", "register"]
        )
    };

    // Count matches for logged in vs logged out indicators
    let logged_in_count = logged_in_indicators.iter()
        .filter(|indicator| page_text.contains(*indicator))
        .count();
    let logged_out_count = logged_out_indicators.iter()
        .filter(|indicator| page_text.contains(*indicator))
        .count();

    log::info!("[LOGIN CHECK] Platform: {}, Logged in indicators: {}, Logged out indicators: {}",
               platform, logged_in_count, logged_out_count);

    // If more logged out indicators than logged in, user is not logged in
    if logged_out_count > logged_in_count && logged_in_count < 2 {
        Ok(false)
    } else if logged_in_count >= 2 {
        Ok(true)
    } else if logged_out_count == 0 && logged_in_count >= 1 {
        // 没有任何「请登录/注册」提示 + 至少一个登录后标志 → 判为已登录。
        // 修复：现代站点（如 GitHub 新版导航）前 3000 字常只露出 1 个登录后关键词，
        // 旧阈值「≥2 才算登录」会把已登录账号误判为未登录。
        Ok(true)
    } else {
        // Ambiguous - assume logged out for safety
        log::warn!("[LOGIN CHECK] Ambiguous login status for {}, assuming not logged in", platform);
        Ok(false)
    }
}

/// 导航到平台首页并轮询登录态（SPA 首屏渲染慢，单次检测易误判）。同步，需在 spawn_blocking 中调用。
fn verify_login_blocking(platform: &str) -> bool {
    let _ = unzoo_navigate(get_platform_home_url(platform));
    std::thread::sleep(std::time::Duration::from_secs(3));
    let mut logged = check_platform_login_status(platform).unwrap_or(false);
    let mut waited = 0;
    while !logged && waited < 12 {
        std::thread::sleep(std::time::Duration::from_secs(3));
        waited += 3;
        logged = check_platform_login_status(platform).unwrap_or(false);
    }
    logged
}

/// 阻塞版浏览模拟（用 std::thread::sleep，不碰 tokio）。引擎走 spawn_blocking 调用，
/// 避免在 async 上下文里穿插阻塞 reqwest 触发 "Cannot drop a runtime" panic。
fn simulate_browsing_blocking(platform: &str, duration_seconds: i64) -> Result<i64, String> {
    use std::time::{Duration, Instant};
    log::info!("[NURTURE] (blocking) browsing on {} for ~{}s", platform, duration_seconds);
    let start = Instant::now();
    let target = Duration::from_secs(duration_seconds as u64);

    if !verify_login_blocking(platform) {
        return Err(format!("未登录 {} 平台！请先在该 profile 登录后再养号。", platform));
    }

    while start.elapsed() < target {
        let scroll_amount = get_human_delay(200, 500) as i32;
        let direction = if pseudo_random_bool(0.8) { "down" } else { "up" };
        let _ = unzoo_scroll(direction, scroll_amount);
        std::thread::sleep(Duration::from_millis(get_human_delay(2000, 5000)));
        random_mouse_movement();
        if pseudo_random_bool(0.3) {
            let targets = ["article", "div.post", "div.feed-item", "main", "section"];
            let idx = (get_human_delay(0, (targets.len() - 1) as u64)) as usize;
            let _ = unzoo_hover(targets[idx]);
        }
        std::thread::sleep(Duration::from_millis(get_human_delay(1000, 3000)));
    }
    let elapsed = start.elapsed().as_secs() as i64;
    log::info!("[NURTURE] (blocking) completed: {}s", elapsed);
    Ok(elapsed)
}


/// Get nurturing status for an account
#[tauri::command]
fn get_account_nurture_status(state: State<AppState>, account_id: String) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // First verify the account exists
    let exists: bool = conn.query_row(
        "SELECT 1 FROM accounts WHERE id = ?1",
        params![account_id],
        |_| Ok(true),
    ).unwrap_or(false);

    if !exists {
        return Err(format!("Account not found: {}", account_id));
    }

    // Try to get nurture columns (may not exist in old databases)
    let started_at: Option<String> = conn.query_row(
        "SELECT nurture_started_at FROM accounts WHERE id = ?1",
        params![account_id],
        |row| row.get(0),
    ).ok().flatten();

    let total_seconds: i64 = conn.query_row(
        "SELECT total_nurture_seconds FROM accounts WHERE id = ?1",
        params![account_id],
        |row| row.get(0),
    ).unwrap_or(0);

    let last_at: Option<String> = conn.query_row(
        "SELECT last_nurture_at FROM accounts WHERE id = ?1",
        params![account_id],
        |row| row.get(0),
    ).ok().flatten();

    let total_hours = total_seconds as f64 / 3600.0;
    let total_days = total_seconds as f64 / 86400.0;

    Ok(serde_json::json!({
        "account_id": account_id,
        "nurture_started_at": started_at,
        "total_nurture_seconds": total_seconds,
        "total_nurture_hours": format!("{:.1}", total_hours),
        "total_nurture_days": format!("{:.2}", total_days),
        "last_nurture_at": last_at,
        "is_nurturing": started_at.is_some()
    }))
}

/// Get all accounts with their nurture status
#[tauri::command]
fn list_accounts_with_nurture_status(state: State<AppState>) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Check if nurture columns exist
    let has_nurture_columns = conn
        .prepare("SELECT nurture_started_at FROM accounts LIMIT 1")
        .is_ok();

    let mut results = Vec::new();

    if has_nurture_columns {
        // Query with nurture columns
        let mut stmt = conn
            .prepare(
                "SELECT id, platform, username, email, status, health_score,
                        nurture_started_at, total_nurture_seconds, last_nurture_at, created_at
                 FROM accounts ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let accounts = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let platform: String = row.get(1)?;
                let username: Option<String> = row.get(2)?;
                let email: Option<String> = row.get(3)?;
                let status: String = row.get(4)?;
                let health_score: i32 = row.get(5)?;
                let nurture_started_at: Option<String> = row.get(6)?;
                let total_nurture_seconds: i64 = row.get::<_, Option<i64>>(7)?.unwrap_or(0);
                let last_nurture_at: Option<String> = row.get(8)?;
                let created_at: String = row.get(9)?;

                let total_hours = total_nurture_seconds as f64 / 3600.0;

                Ok(serde_json::json!({
                    "id": id,
                    "platform": platform,
                    "username": username,
                    "email": email,
                    "status": status,
                    "health_score": health_score,
                    "nurture_started_at": nurture_started_at,
                    "total_nurture_seconds": total_nurture_seconds,
                    "total_nurture_hours": format!("{:.1}", total_hours),
                    "last_nurture_at": last_nurture_at,
                    "is_being_nurtured": nurture_started_at.is_some(),
                    "created_at": created_at
                }))
            })
            .map_err(|e| e.to_string())?;

        results = accounts.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    } else {
        // Query without nurture columns (old database)
        let mut stmt = conn
            .prepare(
                "SELECT id, platform, username, email, status, health_score, created_at
                 FROM accounts ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let accounts = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let platform: String = row.get(1)?;
                let username: Option<String> = row.get(2)?;
                let email: Option<String> = row.get(3)?;
                let status: String = row.get(4)?;
                let health_score: i32 = row.get(5)?;
                let created_at: String = row.get(6)?;

                Ok(serde_json::json!({
                    "id": id,
                    "platform": platform,
                    "username": username,
                    "email": email,
                    "status": status,
                    "health_score": health_score,
                    "nurture_started_at": null,
                    "total_nurture_seconds": 0,
                    "total_nurture_hours": "0.0",
                    "last_nurture_at": null,
                    "is_being_nurtured": false,
                    "created_at": created_at
                }))
            })
            .map_err(|e| e.to_string())?;

        results = accounts.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    }

    Ok(results)
}

/// Quick nurture - run a short browsing session
#[tauri::command]
async fn quick_nurture(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    seconds: i64,
) -> Result<String, String> {
    // Get account info - query platform first, then try profile_id
    let (platform, profile_id) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;

        // Ensure nurture columns exist (inline migration for robustness)
        let has_nurture_columns = conn
            .prepare("SELECT nurture_started_at FROM accounts LIMIT 1")
            .is_ok();
        if !has_nurture_columns {
            log::info!("[DB] Adding missing nurture columns to accounts table");
            let _ = conn.execute("ALTER TABLE accounts ADD COLUMN nurture_started_at TEXT", []);
            let _ = conn.execute("ALTER TABLE accounts ADD COLUMN total_nurture_seconds INTEGER DEFAULT 0", []);
            let _ = conn.execute("ALTER TABLE accounts ADD COLUMN last_nurture_at TEXT", []);
        }

        // First get platform (required)
        let platform: String = conn.query_row(
            "SELECT platform FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        ).map_err(|e| format!("Account not found: {}", e))?;
        // Then try to get profile_id (may not exist in old databases)
        let profile_id: Option<String> = conn.query_row(
            "SELECT profile_id FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get(0),
        ).ok().flatten();
        (platform, profile_id)
    };

    // Update nurture_started_at if not set
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE accounts SET nurture_started_at = COALESCE(nurture_started_at, ?1), last_nurture_at = ?1 WHERE id = ?2",
            params![now, account_id],
        )
        .map_err(|e| e.to_string())?;
    }

    log::info!("[NURTURE] Quick nurture for {} on {} for {}s", account_id, platform, seconds);

    // Launch browser
    let tab_id = if let Some(pid) = profile_id {
        unzoo_launch_profile(pid).await?
    } else {
        let selected = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            conn.query_row(
                "SELECT value FROM config WHERE key = 'selected_browser_profile'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_default()
        };
        if selected.is_empty() {
            return Err("No browser profile selected.".to_string());
        }
        unzoo_launch_profile(selected).await?
    };

    set_active_tab(Some(tab_id));

    // GitHub 走专属领域社交养号（按领域 star/follow/watch + L2），不走通用滚动。
    if platform.eq_ignore_ascii_case("github") {
        return nurture::github_nurture_run(&app, &account_id, seconds).await;
    }
    if platform.eq_ignore_ascii_case("twitter") || platform.eq_ignore_ascii_case("x") {
        return nurture::x_nurture_run(&app, &account_id, seconds).await;
    }
    // SegmentFault 走专属搜索驱动养号（按领域搜索→浏览→读文章/问题），不走纯滚动。
    if platform.eq_ignore_ascii_case("segmentfault") {
        return nurture::segmentfault_nurture_run(&app, &account_id, seconds).await;
    }

    // Simulate browsing for specified duration.
    // 走阻塞版 + spawn_blocking（阻塞 reqwest 不能在 async 里直接调，否则导航 panic、tab 空白不操作）。
    let pf = platform.clone();
    let duration = tauri::async_runtime::spawn_blocking(move || simulate_browsing_blocking(&pf, seconds))
        .await
        .map_err(|e| format!("养号任务异常: {}", e))??;

    // Update total nurture seconds and record session
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE accounts SET total_nurture_seconds = total_nurture_seconds + ?1, last_nurture_at = ?2 WHERE id = ?3",
            params![duration, now, account_id],
        )
        .map_err(|e| e.to_string())?;

        // Record in daily logs
        record_nurture_session(&conn, &account_id, duration)?;
    }

    Ok(format!("Quick nurture completed: {} seconds", duration))
}

// ============================================================================
// Nurture Strategy Management (养号策略管理)
// ============================================================================

/// Get all nurture strategies
#[tauri::command]
fn list_nurture_strategies(state: State<AppState>) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT platform, warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max,
                    session_duration_min, session_duration_max, active_hours_start,
                    active_hours_end, enabled, updated_at
             FROM nurture_strategies ORDER BY platform",
        )
        .map_err(|e| e.to_string())?;

    let strategies = stmt
        .query_map([], |row| {
            Ok(serde_json::json!({
                "platform": row.get::<_, String>(0)?,
                "warmup_days": row.get::<_, i32>(1)?,
                "growth_days": row.get::<_, i32>(2)?,
                "daily_sessions_min": row.get::<_, i32>(3)?,
                "daily_sessions_max": row.get::<_, i32>(4)?,
                "session_duration_min": row.get::<_, i32>(5)?,
                "session_duration_max": row.get::<_, i32>(6)?,
                "active_hours_start": row.get::<_, i32>(7)?,
                "active_hours_end": row.get::<_, i32>(8)?,
                "enabled": row.get::<_, i32>(9)? == 1,
                "updated_at": row.get::<_, String>(10)?
            }))
        })
        .map_err(|e| e.to_string())?;

    strategies.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Update a nurture strategy for a platform
#[tauri::command]
fn update_nurture_strategy(
    state: State<AppState>,
    platform: String,
    warmup_days: i32,
    growth_days: i32,
    daily_sessions_min: i32,
    daily_sessions_max: i32,
    session_duration_min: i32,
    session_duration_max: i32,
    active_hours_start: i32,
    active_hours_end: i32,
    enabled: bool,
) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO nurture_strategies (platform, warmup_days, growth_days, daily_sessions_min, daily_sessions_max, session_duration_min, session_duration_max, active_hours_start, active_hours_end, enabled, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(platform) DO UPDATE SET
             warmup_days = ?2, growth_days = ?3, daily_sessions_min = ?4, daily_sessions_max = ?5,
             session_duration_min = ?6, session_duration_max = ?7,
             active_hours_start = ?8, active_hours_end = ?9, enabled = ?10, updated_at = ?11",
        params![platform, warmup_days, growth_days, daily_sessions_min, daily_sessions_max,
                session_duration_min, session_duration_max, active_hours_start,
                active_hours_end, if enabled { 1 } else { 0 }, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(format!("Strategy updated for {}", platform))
}

/// Get daily nurture logs for an account
#[tauri::command]
fn get_account_nurture_logs(
    state: State<AppState>,
    account_id: String,
    days: i32,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT date, sessions_completed, total_seconds, session_details
             FROM nurture_daily_logs
             WHERE account_id = ?1
             ORDER BY date DESC
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;

    let logs = stmt
        .query_map(params![account_id, days], |row| {
            Ok(serde_json::json!({
                "date": row.get::<_, String>(0)?,
                "sessions_completed": row.get::<_, i32>(1)?,
                "total_seconds": row.get::<_, i32>(2)?,
                "session_details": row.get::<_, Option<String>>(3)?
            }))
        })
        .map_err(|e| e.to_string())?;

    logs.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// #19 一键结束养号：把账号标记为「跳过养号」→ 阶段直接显示「正常」，且不再被自动养号。
/// 适用于本来就成熟的老账号（非新注册），无需走预热期。
#[tauri::command]
fn finish_account_nurture(state: State<AppState>, account_id: String) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if conn.prepare("SELECT nurture_skip FROM accounts LIMIT 1").is_err() {
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN nurture_skip INTEGER DEFAULT 0", []);
    }
    let now = Utc::now().to_rfc3339();
    let n = conn.execute(
        "UPDATE accounts SET nurture_skip=1, nurture_started_at=COALESCE(nurture_started_at, ?1) WHERE id=?2",
        params![now, account_id]).map_err(|e| e.to_string())?;
    if n == 0 { return Err("账号不存在".into()); }
    Ok("已结束养号，账号标记为正常".into())
}

/// Get account lifecycle status
#[tauri::command]
fn get_account_lifecycle(
    state: State<AppState>,
    account_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Get basic account info first (required fields)
    let (platform, created_at): (String, String) = conn
        .query_row(
            "SELECT platform, created_at FROM accounts WHERE id = ?1",
            params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("Account not found: {}", e))?;

    // Try to get nurture columns (may not exist in old databases)
    let nurture_started_at: Option<String> = conn.query_row(
        "SELECT nurture_started_at FROM accounts WHERE id = ?1",
        params![account_id],
        |row| row.get(0),
    ).ok().flatten();

    let total_seconds: i64 = conn.query_row(
        "SELECT total_nurture_seconds FROM accounts WHERE id = ?1",
        params![account_id],
        |row| row.get(0),
    ).unwrap_or(0);

    // Get platform strategy（养号周期：预热 + 成长两段；成长 NULL 时兜底=预热）
    let (warmup_days, growth_days, daily_min, daily_max): (i32, i32, i32, i32) = conn
        .query_row(
            "SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max FROM nurture_strategies WHERE platform = ?1",
            params![platform],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap_or((14, 14, 2, 5)); // Default values
    // 完整养号周期 = 预热 + 成长（到达「成熟」所需天数）
    let total_cycle_days = (warmup_days + growth_days).max(1);

    // Calculate lifecycle status
    let now = Utc::now();

    // #19 跳过养号标记：成熟账号一键结束养号 → 直接当作「正常 / 养满」
    let skip: i64 = conn.query_row(
        "SELECT COALESCE(nurture_skip,0) FROM accounts WHERE id = ?1",
        params![account_id], |row| row.get(0)).unwrap_or(0);

    // 养号天数 = 实际「养过号的天数」（nurture_daily_logs 去重日期数）：今天养了就算今天，
    // 跳过的日子不计——比按日历差更贴合「养了几天」的真实含义。
    let nurtured_days: i32 = conn.query_row(
        "SELECT COUNT(DISTINCT date) FROM nurture_daily_logs WHERE account_id = ?1",
        params![account_id], |r| r.get(0)).unwrap_or(0);
    let days_since_start = if skip == 1 { total_cycle_days } else { nurtured_days };
    let days_remaining = (total_cycle_days - days_since_start).max(0);   // 距「成熟」还剩几天
    let progress_percent = ((days_since_start as f64 / total_cycle_days as f64) * 100.0).min(100.0);

    // Determine lifecycle stage：预热 → 成长 → 成熟（active），按「实际养过号的天数」分段
    let stage = if skip == 1 {
        "active"
    } else if nurture_started_at.is_none() {
        "new"
    } else if days_since_start < warmup_days {
        "warming"
    } else if days_since_start < total_cycle_days {
        "growing"
    } else {
        "active"
    };

    // Get today's nurture count
    let today = now.format("%Y-%m-%d").to_string();
    let today_sessions: i32 = conn
        .query_row(
            "SELECT sessions_completed FROM nurture_daily_logs WHERE account_id = ?1 AND date = ?2",
            params![account_id, today],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let today_target = (daily_min + daily_max) / 2;
    let today_completed = today_sessions >= daily_min;

    Ok(serde_json::json!({
        "account_id": account_id,
        "platform": platform,
        "stage": stage,
        "warmup_days": warmup_days,
        "growth_days": growth_days,
        "total_cycle_days": total_cycle_days,
        "days_since_start": days_since_start,
        "days_remaining": days_remaining,
        "progress_percent": format!("{:.0}", progress_percent),
        "total_nurture_seconds": total_seconds,
        "total_nurture_hours": format!("{:.1}", total_seconds as f64 / 3600.0),
        "today": {
            "date": today,
            "sessions_completed": today_sessions,
            "sessions_target": today_target,
            "sessions_min": daily_min,
            "sessions_max": daily_max,
            "completed": today_completed
        }
    }))
}

/// Record a nurture session (called after quick_nurture or scheduled nurture)
fn record_nurture_session(
    conn: &Connection,
    account_id: &str,
    duration_seconds: i64,
) -> Result<(), String> {
    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let session_id = format!("session-{}", now.timestamp_millis());

    // Insert session record
    conn.execute(
        "INSERT INTO nurture_sessions (id, account_id, started_at, ended_at, duration_seconds, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'completed')",
        params![
            session_id,
            account_id,
            (now - chrono::Duration::seconds(duration_seconds)).to_rfc3339(),
            now.to_rfc3339(),
            duration_seconds
        ],
    )
    .map_err(|e| e.to_string())?;

    // Update or insert daily log
    conn.execute(
        "INSERT INTO nurture_daily_logs (id, account_id, date, sessions_completed, total_seconds)
         VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(account_id, date) DO UPDATE SET
             sessions_completed = sessions_completed + 1,
             total_seconds = total_seconds + ?4",
        params![format!("log-{}-{}", account_id, today), account_id, today, duration_seconds],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get accounts that need nurturing today
#[tauri::command]
fn get_accounts_needing_nurture(state: State<AppState>) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let today = Utc::now().format("%Y-%m-%d").to_string();

    // Check if nurture columns exist
    let has_nurture_columns = conn
        .prepare("SELECT nurture_started_at FROM accounts LIMIT 1")
        .is_ok();

    if !has_nurture_columns {
        // No nurture columns - return empty list
        return Ok(Vec::new());
    }

    // Get all warming accounts with their today's progress
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.platform, a.username, a.email, a.nurture_started_at,
                    COALESCE(l.sessions_completed, 0) as today_sessions,
                    s.daily_sessions_min, s.daily_sessions_max, s.warmup_days
             FROM accounts a
             LEFT JOIN nurture_daily_logs l ON a.id = l.account_id AND l.date = ?1
             LEFT JOIN nurture_strategies s ON a.platform = s.platform
             WHERE a.nurture_started_at IS NOT NULL
               AND a.is_active = 1
               AND (s.enabled = 1 OR s.enabled IS NULL)
             ORDER BY today_sessions ASC, a.last_nurture_at ASC",
        )
        .map_err(|e| e.to_string())?;

    let accounts = stmt
        .query_map(params![today], |row| {
            let id: String = row.get(0)?;
            let platform: String = row.get(1)?;
            let username: Option<String> = row.get(2)?;
            let email: Option<String> = row.get(3)?;
            let nurture_started_at: String = row.get(4)?;
            let today_sessions: i32 = row.get(5)?;
            let daily_min: i32 = row.get::<_, Option<i32>>(6)?.unwrap_or(2);
            let daily_max: i32 = row.get::<_, Option<i32>>(7)?.unwrap_or(5);
            let warmup_days: i32 = row.get::<_, Option<i32>>(8)?.unwrap_or(14);

            let needs_nurture = today_sessions < daily_min;

            // Calculate days since nurture started
            let days_elapsed = chrono::DateTime::parse_from_rfc3339(&nurture_started_at)
                .map(|d| (Utc::now() - d.with_timezone(&Utc)).num_days() as i32)
                .unwrap_or(0);

            let still_warming = days_elapsed < warmup_days;

            Ok(serde_json::json!({
                "id": id,
                "platform": platform,
                "username": username.or(email).unwrap_or_else(|| "N/A".to_string()),
                "today_sessions": today_sessions,
                "daily_min": daily_min,
                "daily_max": daily_max,
                "needs_nurture": needs_nurture && still_warming,
                "days_elapsed": days_elapsed,
                "days_remaining": (warmup_days - days_elapsed).max(0),
                "still_warming": still_warming
            }))
        })
        .map_err(|e| e.to_string())?;

    accounts.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

// ============================================================================
// 任务执行引擎 (Task Execution Engine)
//
// 应用此前能"创建"任务但无人"执行"，导致任务永远卡在 pending。本引擎补齐
// 缺失的执行循环：认领到期任务 → 按类型派发到现有动作(复用 publish_content)
// → 落库(成功/重试退避/阻塞→人工)。单后台 tokio 任务，复用应用已有的浏览器/
// 限流逻辑；不持有 db 锁跨 await，future 满足 Send。
// ============================================================================

static ENGINE_RUNNING: AtomicBool = AtomicBool::new(false);
static ENGINE_STOP: AtomicBool = AtomicBool::new(false);
static ENGINE_PROCESSED: AtomicI64 = AtomicI64::new(0);
/// 当前已启动的 profile（用于复用标签页，避免每个任务都新开标签把浏览器刷爆）
static ENGINE_CURRENT_PROFILE: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();

const ENGINE_MAX_RETRIES: i32 = 3;
const ENGINE_POLL_SECS: u64 = 5;
const ENGINE_SPACING_SECS: u64 = 2;
/// 对外发帖类任务（安静时段会被抑制）
const OUTBOUND_TYPES: &[&str] = &["article", "post", "publish", "tweet", "reply", "engage", "content_publish"];

#[derive(Debug, Clone)]
struct ClaimedTask {
    id: String,
    campaign_id: Option<String>,
    task_type: String,
    platform: Option<String>,
    account_id: Option<String>,
    content: Option<String>,
    target_url: Option<String>,
    retry_count: i32,
}

enum TaskOutcome {
    Success(Option<String>),  // optional post url
    Retry(String),            // transient/rate-limited -> backoff & requeue
    Blocked(String),          // needs human (under-specified, captcha, ...)
    Failed(String),           // hard failure
}

#[derive(Debug, Serialize, Clone)]
pub struct TaskDto {
    pub id: String,
    pub task_type: String,
    pub platform: Option<String>,
    pub account_id: Option<String>,
    pub content: Option<String>,
    pub target_url: Option<String>,
    pub status: String,
    pub retry_count: i32,
    pub error_message: Option<String>,
    pub scheduled_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EngineStatus {
    pub running: bool,
    pub processed: i64,
    pub last_heartbeat: Option<String>,
    pub pending: i64,
    pub running_count: i64,
    pub blocked: i64,
    pub completed: i64,
    pub failed: i64,
}

pub(crate) fn engine_cfg_set(conn: &Connection, key: &str, value: &str) {
    let _ = conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![key, value],
    );
}

pub(crate) fn engine_cfg_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM config WHERE key = ?1", params![key], |r| r.get(0)).ok()
}

/// 当前是否处于安静时段（仅抑制对外发帖任务）。
/// 读取 config 的 engine_quiet_start / engine_quiet_end（0-23 整点）。
fn engine_in_quiet_hours(conn: &Connection) -> bool {
    let start = engine_cfg_get(conn, "engine_quiet_start").and_then(|v| v.parse::<u32>().ok());
    let end = engine_cfg_get(conn, "engine_quiet_end").and_then(|v| v.parse::<u32>().ok());
    match (start, end) {
        (Some(s), Some(e)) if s != e => {
            let h = Local::now().hour();
            if s < e { h >= s && h < e } else { h >= s || h < e }
        }
        _ => false,
    }
}

/// 认领下一个可执行任务并原子置为 running。单消费者循环，SELECT+带条件 UPDATE 即可。
fn engine_claim_next(conn: &Connection, exclude_outbound: bool) -> Option<ClaimedTask> {
    let sql = if exclude_outbound {
        "SELECT id, campaign_id, task_type, platform, account_id, content, target_url, retry_count \
         FROM tasks WHERE status = 'pending' \
           AND (scheduled_at IS NULL OR scheduled_at <= datetime('now')) \
           AND task_type NOT IN ('article','post','publish','tweet','reply','engage','content_publish') \
         ORDER BY created_at ASC LIMIT 1"
    } else {
        "SELECT id, campaign_id, task_type, platform, account_id, content, target_url, retry_count \
         FROM tasks WHERE status = 'pending' \
           AND (scheduled_at IS NULL OR scheduled_at <= datetime('now')) \
         ORDER BY created_at ASC LIMIT 1"
    };

    let task = conn.query_row(sql, [], |row| {
        Ok(ClaimedTask {
            id: row.get(0)?,
            campaign_id: row.get(1)?,
            task_type: row.get(2)?,
            platform: row.get(3)?,
            account_id: row.get(4)?,
            content: row.get(5)?,
            target_url: row.get(6)?,
            retry_count: row.get(7).unwrap_or(0),
        })
    }).ok()?;

    let changed = conn.execute(
        "UPDATE tasks SET status = 'running', started_at = datetime('now') WHERE id = ?1 AND status = 'pending'",
        params![task.id],
    ).unwrap_or(0);

    if changed == 1 { Some(task) } else { None }
}

/// 把任务结果落库：成功/重试退避/阻塞/失败。
fn engine_apply_outcome(conn: &Connection, task: &ClaimedTask, outcome: TaskOutcome) {
    match outcome {
        TaskOutcome::Success(_url) => {
            let _ = conn.execute(
                "UPDATE tasks SET status = 'completed', completed_at = datetime('now'), error_message = NULL WHERE id = ?1",
                params![task.id],
            );
            log::info!("[ENGINE] task {} completed", task.id);
        }
        TaskOutcome::Blocked(reason) => {
            let _ = conn.execute(
                "UPDATE tasks SET status = 'blocked', error_message = ?2 WHERE id = ?1",
                params![task.id, reason],
            );
            log::warn!("[ENGINE] task {} blocked: {}", task.id, reason);
        }
        TaskOutcome::Retry(err) | TaskOutcome::Failed(err) => {
            let next = task.retry_count + 1;
            if next <= ENGINE_MAX_RETRIES {
                let backoff_min = std::cmp::min(2_i64.pow(next as u32), 60);
                let _ = conn.execute(
                    "UPDATE tasks SET status = 'pending', retry_count = ?2, error_message = ?3, \
                     started_at = NULL, scheduled_at = datetime('now', '+' || ?4 || ' minutes') WHERE id = ?1",
                    params![task.id, next, err, backoff_min],
                );
                log::warn!("[ENGINE] task {} failed, retry {} in {}min: {}", task.id, next, backoff_min, err);
            } else {
                let _ = conn.execute(
                    "UPDATE tasks SET status = 'failed', error_message = ?2, completed_at = datetime('now') WHERE id = ?1",
                    params![task.id, err],
                );
                log::error!("[ENGINE] task {} failed permanently: {}", task.id, err);
            }
        }
    }
}

/// 我们帖子下的一条评论（类型 A 的输入）。
#[derive(Debug, Clone)]
struct FetchedComment {
    id: String,
    author: String,
    text: String,
    permalink: String,
}

/// 解析 Reddit 帖子 .json 的评论列表（listing[1].data.children 里 kind=t1 的项）。
fn parse_reddit_comment_listing(val: &serde_json::Value, post_url: &str) -> Vec<FetchedComment> {
    let mut out = Vec::new();
    if let Some(children) = val.get(1)
        .and_then(|l| l.get("data")).and_then(|d| d.get("children")).and_then(|c| c.as_array())
    {
        for child in children {
            if child.get("kind").and_then(|k| k.as_str()) != Some("t1") { continue; }
            let d = match child.get("data") { Some(d) => d, None => continue };
            let id = d.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if id.is_empty() { continue; }
            out.push(FetchedComment {
                id,
                author: d.get("author").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                text: d.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                permalink: d.get("permalink").and_then(|v| v.as_str())
                    .map(|p| format!("https://www.reddit.com{}", p))
                    .unwrap_or_else(|| post_url.to_string()),
            });
        }
    }
    out
}

fn reddit_json_url(post_url: &str) -> String {
    let base = post_url.trim_end_matches('/');
    if base.ends_with(".json") { base.to_string() } else { format!("{}.json?limit=50&sort=new", base) }
}

/// 经 Unzoo 浏览器（真实指纹+residential IP+cookie）取 Reddit 评论 JSON。
/// 实测(2026-06)：裸 HTTP 直连 reddit .json 已被 403；走浏览器返回 200。
fn reddit_comments_via_browser(post_url: &str) -> Result<Vec<FetchedComment>, String> {
    unzoo_navigate(&reddit_json_url(post_url))?;
    std::thread::sleep(std::time::Duration::from_secs(2));
    // Chrome 把纯 JSON 渲染进 <pre>；优先取它，取不到再退整页 body
    let txt = unzoo_get_text_sel("pre").ok().filter(|s| s.trim_start().starts_with('['))
        .or_else(|| unzoo_get_text().ok())
        .ok_or_else(|| "读取 reddit json 文本失败".to_string())?;
    let val: serde_json::Value = serde_json::from_str(txt.trim())
        .map_err(|e| format!("reddit json 解析失败: {}", e))?;
    Ok(parse_reddit_comment_listing(&val, post_url))
}

/// 抓取我们某帖下的评论。Reddit：优先走浏览器（绕 403），失败再退裸 HTTP；
/// 其它平台暂未接入评论抓取，返回「暂未支持」由派发层转为 blocked。
async fn fetch_post_comments(platform: &str, post_url: &str) -> Result<Vec<FetchedComment>, String> {
    match platform.to_lowercase().as_str() {
        "reddit" => {
            // 1) 浏览器路径（首选）
            let pu = post_url.to_string();
            if let Ok(Ok(v)) = tauri::async_runtime::spawn_blocking(move || reddit_comments_via_browser(&pu)).await {
                return Ok(v);
            }
            // 2) 兜底：裸 HTTP（多数环境已 403，但留作降级）
            let resp = Client::new()
                .get(&reddit_json_url(post_url))
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
                .send().await.map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("reddit 评论抓取失败：浏览器路径未取到且 HTTP {}（多为反爬 403，需账号 profile 在线）", resp.status()));
            }
            let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(parse_reddit_comment_listing(&val, post_url))
        }
        other => Err(format!("评论抓取暂未支持平台: {}", other)),
    }
}

/// 针对"我们帖子下的评论"生成上下文回复（致谢/答疑/互动，非硬广）。
fn generate_comment_reply(comment_text: &str) -> String {
    let templates = [
        "Thanks for the comment — really appreciate you taking the time!",
        "Great point, thanks for sharing your thoughts! Happy to answer any questions.",
        "Appreciate the feedback! Let me know if there's anything you'd like me to expand on.",
        "Thanks for engaging — glad it resonated! Feel free to ask if anything's unclear.",
    ];
    let idx = comment_text.len() % templates.len();
    templates[idx].to_string()
}

/// 发布一条回复到给定 URL（复用 reply_to_post 的导航→输入→提交序列）。
/// 元素是否存在（经 /evaluate）。
fn unzoo_element_exists(selector: &str) -> bool {
    let expr = format!("(document.querySelector('{}')!==null)", selector.replace('\'', "\\'"));
    unzoo_evaluate(&expr).map(|r| r.contains("true")).unwrap_or(false)
}

/// 把 X 帖子链接归一为基础 status 链接（去掉 /analytics、/photo/N、查询串等）。
fn clean_x_status_url(url: &str) -> String {
    if let Some(idx) = url.find("/status/") {
        let id: String = url[idx + 8..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if !id.is_empty() {
            return format!("{}{}", &url[..idx + 8], id);
        }
    }
    url.to_string()
}

/// X/Twitter 专用回复：实测要点（2026-05）——回复框是 Draft.js contenteditable，
/// 合成键入无效，必须 focus + execCommand('insertText')；内联发布按钮是 tweetButtonInline，
/// 且输入后才启用。目标链接必须归一为基础 status 链接。
fn twitter_reply(url: &str, text: &str) -> Result<(), String> {
    let clean = clean_x_status_url(url);
    unzoo_navigate(&clean).map_err(|e| format!("导航失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_secs(4));

    let box_sel = "[data-testid=\"tweetTextarea_0\"]";
    let mut waited = 0;
    while !unzoo_element_exists(box_sel) && waited < 12 {
        std::thread::sleep(std::time::Duration::from_secs(3));
        waited += 3;
    }
    if !unzoo_element_exists(box_sel) {
        return Err("未找到 X 回复框（可能未登录、推文不可回复或页面改版）".into());
    }

    // focus → 清空 → execCommand 插入（Draft.js 友好）；text 用 JSON 编码成安全的 JS 字符串字面量
    let js_text = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
    let inject = format!(
        "(function(){{var e=document.querySelector('[data-testid=\"tweetTextarea_0\"]');if(!e)return 'no';e.focus();document.execCommand('selectAll',false,null);document.execCommand('delete',false,null);var ok=document.execCommand('insertText',false,{});return 'exec='+ok;}})()",
        js_text);
    let r = unzoo_evaluate(&inject)?;
    if !r.contains("exec=true") {
        return Err(format!("X 回复框输入失败：{}", r));
    }
    std::thread::sleep(std::time::Duration::from_millis(1200));

    let submit_sel = "[data-testid=\"tweetButtonInline\"]";
    let mut w2 = 0;
    while !unzoo_element_exists(submit_sel) && w2 < 6 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        w2 += 2;
    }
    if !unzoo_element_exists(submit_sel) {
        return Err("X 已输入回复，但未出现可用的发布按钮".into());
    }
    unzoo_click(submit_sel).map_err(|e| format!("点击发布失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_secs(2));
    Ok(())
}

/// 导航到帖子并发布回复。步骤级精确报错 + 轮询等待编辑框/发布按钮出现
/// （SPA 渲染慢，且部分平台需先点开评论框）。
/// Reddit 专用回复：新版 shreddit 评论框是 Lexical（合成事件/ execCommand 均无法落字），
/// 改走 old.reddit.com——评论框是普通 textarea，browser_type 真键盘可靠落字，发布按钮是 button.save。
fn reddit_reply(url: &str, text: &str) -> Result<(), String> {
    let old = url
        .replace("www.reddit.com", "old.reddit.com")
        .replace("://reddit.com", "://old.reddit.com");
    let old = if old.contains("old.reddit.com") { old } else { old.replacen("reddit.com", "old.reddit.com", 1) };
    unzoo_navigate(&old).map_err(|e| format!("导航失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_secs(4));

    let ta = ".commentarea textarea[name=\"text\"]";
    let mut waited = 0;
    while !unzoo_element_exists(ta) && waited < 12 {
        std::thread::sleep(std::time::Duration::from_secs(3));
        waited += 3;
    }
    if !unzoo_element_exists(ta) {
        return Err("未找到 Reddit 评论框（old.reddit 可能未登录该账号）".into());
    }
    unzoo_type(ta, text).map_err(|e| format!("输入回复失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(900));

    let save = ".commentarea button.save";
    if !unzoo_element_exists(save) {
        return Err("Reddit 已输入回复，但未找到 save 发布按钮".into());
    }
    unzoo_click(save).map_err(|e| format!("点击发布失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_secs(2));
    Ok(())
}

/// 打开市场提交表单 → 按字段语义填好（含下拉选第一项有效值）→ 点击提交 → 校验结果。
/// 返回："submitted: <url>" | "filled_no_submit: ..." | "submitted_unverified: ..."；
/// 校验明确失败（需登录/必填/验证码）返回 Err。同步，需 spawn_blocking。
fn marketplace_prefill(url: &str, name: &str, tagline: &str, desc: &str, website: &str, repo: &str, install: &str, email: &str) -> Result<String, String> {
    unzoo_navigate(url).map_err(|e| format!("导航失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_secs(4));
    let j = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());

    // 1) 填字段（input/textarea 按语义；email 用提交者邮箱；select 选第一项；勾选必选框）
    let fill_js = format!(r#"(function(){{
      var v={{name:{n},website:{w},repo:{r},desc:{d},install:{i},tagline:{t},email:{e}}};
      function set(el,val){{if(!val)return 0;try{{el.focus();el.value=val;el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));return 1;}}catch(e){{return 0;}}}}
      var filled=0;
      document.querySelectorAll('input:not([type=hidden]):not([type=submit]):not([type=button]):not([type=checkbox]):not([type=radio]):not([type=file]), textarea').forEach(function(f){{
        if(f.value&&f.value.trim())return;
        var h=((f.name||'')+' '+(f.id||'')+' '+(f.placeholder||'')+' '+(f.getAttribute('aria-label')||'')).toLowerCase();
        if((f.type||'')==='email'||/e-?mail|邮箱|邮件/.test(h)){{filled+=set(f,v.email);}}
        else if(/repo|github|source|\bgit\b/.test(h)){{filled+=set(f,v.repo);}}
        else if(/url|website|link|homepage|site/.test(h)){{filled+=set(f,v.website);}}
        else if(/install|command|npx|config|usage/.test(h)){{filled+=set(f,v.install);}}
        else if(/desc|about|summary|detail|readme|content|abstract|intro/.test(h)||f.tagName==='TEXTAREA'){{filled+=set(f,v.desc);}}
        else if(/tagline|slogan|short|headline/.test(h)){{filled+=set(f,v.tagline);}}
        else if(/name|title|project|server|tool/.test(h)){{filled+=set(f,v.name);}}
      }});
      document.querySelectorAll('select').forEach(function(s){{
        if(s.selectedIndex<=0){{for(var k=0;k<s.options.length;k++){{if(s.options[k].value){{s.selectedIndex=k;s.dispatchEvent(new Event('change',{{bubbles:true}}));break;}}}}}}
      }});
      // 勾选未勾的复选框（同意条款/确认类），跳过明显的"订阅/newsletter"
      document.querySelectorAll('input[type=checkbox]').forEach(function(c){{
        var hh=((c.name||'')+' '+(c.id||'')+' '+(c.getAttribute('aria-label')||'')+' '+((c.closest('label')||{{}}).innerText||'')).toLowerCase();
        if(!c.checked && !/newsletter|subscribe|订阅|marketing/.test(hh)){{try{{c.click();}}catch(e){{}}}}
      }});
      return 'filled='+filled;
    }})()"#, n=j(name), w=j(website), r=j(repo), d=j(desc), i=j(install), t=j(tagline), e=j(email));
    let filled = unzoo_evaluate(&fill_js).unwrap_or_default();
    std::thread::sleep(std::time::Duration::from_millis(1200));

    // 2) 找并点击提交按钮（排除取消/导航；优先 type=submit，否则最后一个文案匹配的）
    let before_url = unzoo_evaluate("location.href").ok()
        .map(|s| serde_json::from_str::<String>(&s).unwrap_or(s)).unwrap_or_default();
    let click_js = r#"(function(){
      var loose=/submit|提交|publish|发布|create|添加|add server|register|登记/i;
      var cancel=/cancel|reset|取消|back|返回|dismiss|关闭|clear|清除/i;
      var btns=[].slice.call(document.querySelectorAll('button, input[type=submit]')).filter(function(b){
        var t=((b.innerText||'')+' '+(b.value||'')+' '+(b.getAttribute('aria-label')||'')).trim();
        return !b.disabled && t && loose.test(t) && !cancel.test(t);
      });
      var b=btns.filter(function(x){return (x.type||'')==='submit';})[0] || btns[btns.length-1];
      if(!b) return 'no_submit';
      b.scrollIntoView({block:'center'}); b.click(); return 'clicked';
    })()"#;
    // 提交按钮可能随多步表单延迟渲染：轮询最多 ~10s 再判定
    let mut clicked = unzoo_evaluate(click_js).map(|s| serde_json::from_str::<String>(&s).unwrap_or(s)).unwrap_or_default();
    let mut tries = 0;
    while clicked.contains("no_submit") && tries < 4 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        tries += 1;
        clicked = unzoo_evaluate(click_js).map(|s| serde_json::from_str::<String>(&s).unwrap_or(s)).unwrap_or_default();
    }
    if clicked.contains("no_submit") {
        return Ok(format!("filled_no_submit: 已填好但未找到提交按钮（{}）", filled));
    }
    std::thread::sleep(std::time::Duration::from_secs(4));

    // 3) 校验结果：URL 变化 / 成功提示 / 失败提示
    let verify_js = r#"(function(){var b=(document.body.innerText||'').slice(0,2500);
      return JSON.stringify({url:location.href,
        ok:/success|thank|submitted|received|提交成功|created|已创建|added|your server|listing/i.test(b),
        err:/required|必填|invalid|please sign in|please log in|请登录|verify you are human|captcha/i.test(b)});})()"#;
    let vraw = unzoo_evaluate(verify_js).map(|s| serde_json::from_str::<String>(&s).unwrap_or(s)).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&vraw).unwrap_or_default();
    let now_url = v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
    let err = v.get("err").and_then(|x| x.as_bool()).unwrap_or(false);
    // 仍停在提交页（含 /submit、/new）不算成功——避免 /submit→/zh/submit 这类语言重定向误判
    let still_on_form = now_url.contains("submit") || now_url.contains("/new");
    let navigated_away = !now_url.is_empty() && now_url != before_url && !still_on_form;
    if ok || navigated_away {
        Ok(format!("submitted: {}", now_url))
    } else if err {
        Err(format!("提交校验未通过（需登录/必填/验证码），表单已填好可人工补交：{}", now_url))
    } else {
        Ok(format!("submitted_unverified: 已点击提交但未检出成功提示（{}）", now_url))
    }
}

fn post_reply_to_url(platform: &str, url: &str, text: &str) -> Result<(), String> {
    // X/Twitter 走专用 Draft.js 流程（通用 click+type 在 X 无效）
    if platform.eq_ignore_ascii_case("twitter") || platform.eq_ignore_ascii_case("x") {
        return twitter_reply(url, text);
    }
    // Reddit 走 old.reddit.com 的普通 textarea 流程
    if platform.eq_ignore_ascii_case("reddit") {
        return reddit_reply(url, text);
    }
    let config = get_reply_config(platform)
        .ok_or_else(|| format!("平台 {} 不支持回复", platform))?;
    unzoo_navigate(url).map_err(|e| format!("导航失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_secs(3));

    // 轮询等待回复框；不存在则尝试点开（部分平台评论框需先点击激活），最多 ~12s
    let box_sel = config.reply_box_selector;
    let mut have_box = unzoo_element_exists(box_sel);
    let mut waited = 0;
    while !have_box && waited < 12 {
        let _ = unzoo_click(box_sel); // 尝试激活评论框（无害）
        std::thread::sleep(std::time::Duration::from_secs(3));
        waited += 3;
        have_box = unzoo_element_exists(box_sel);
    }
    if !have_box {
        return Err(format!("未找到回复框（选择器 {}）——可能未登录、平台改版或该帖不可评论", box_sel));
    }

    let _ = unzoo_click(box_sel);
    std::thread::sleep(std::time::Duration::from_millis(500));
    unzoo_type(box_sel, text).map_err(|e| format!("输入回复失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // 发布按钮（很多平台输入后才出现/启用）
    let submit_sel = config.reply_submit_selector;
    let mut have_submit = unzoo_element_exists(submit_sel);
    let mut w2 = 0;
    while !have_submit && w2 < 6 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        w2 += 2;
        have_submit = unzoo_element_exists(submit_sel);
    }
    if !have_submit {
        return Err(format!("已输入回复，但未找到发布按钮（选择器 {}）", submit_sel));
    }
    unzoo_click(submit_sel).map_err(|e| format!("点击发布失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_secs(2));
    Ok(())
}

/// 演练模式：只发现/抓评论，不真实发布。通过 config 的 engine_dry_run 控制。
fn engine_is_dry_run(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    state.db.lock().ok()
        .and_then(|conn| engine_cfg_get(&conn, "engine_dry_run"))
        .as_deref() == Some("1")
}

/// 回复模式：review（半自动，入审核队列）| auto（全自动，直接发）。默认 review。
pub(crate) fn engine_reply_mode(conn: &Connection) -> String {
    engine_cfg_get(conn, "engine_reply_mode").unwrap_or_else(|| "review".to_string())
}

/// 买家意向阈值（0-100）：低于此分不回复，省下额度给高意向。默认 40。
fn engine_intent_min(conn: &Connection) -> i64 {
    engine_cfg_get(conn, "engine_intent_min").and_then(|v| v.parse().ok()).unwrap_or(40)
}

/// 记录一条线索（每条真实发出的回复 = 一条线索）。reply_id 唯一，重复忽略。
fn record_lead(conn: &Connection, reply_id: &str, platform: &str, author: &str, post_url: &str,
               reply: &str, intent: i64, keyword: &str, account_id: &Option<String>) {
    let _ = conn.execute(
        "INSERT OR IGNORE INTO leads (id, reply_id, platform, author, post_url, our_reply, intent_score, status, keyword, account_id, created_at) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,'engaged',?8,?9,datetime('now'))",
        params![Uuid::new_v4().to_string(), reply_id, platform, author, post_url, reply, intent, keyword, account_id]);
}

/// 发帖前防封闸门（P0-2）：把养号期/健康度接进"发不发"的决策。
/// kind: "keyword"（关键词带货，高风险）| "publish"（对外发帖，高风险）| "mention"（回自己帖评论，低风险）
/// 返回 Err(原因) 表示应阻塞该任务。
fn account_outbound_guard(conn: &Connection, platform: &str, account_id: &Option<String>, kind: &str) -> Result<(), String> {
    let row = if let Some(aid) = account_id.as_ref().filter(|s| !s.is_empty()) {
        conn.query_row(
            "SELECT created_at, COALESCE(health_status,'unknown') FROM accounts WHERE id=?1",
            params![aid], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))).ok()
    } else {
        conn.query_row(
            "SELECT created_at, COALESCE(health_status,'unknown') FROM accounts \
             WHERE platform=?1 AND profile_id IS NOT NULL AND profile_id<>'' ORDER BY last_nurture_at DESC LIMIT 1",
            params![platform], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))).ok()
    };
    let (created_at, health) = match row { Some(v) => v, None => return Ok(()) };

    // 健康闸门：任何对外动作都拦（包括 mention）
    if matches!(health.as_str(), "logged_out" | "banned" | "shadowbanned" | "locked" | "restricted") {
        return Err(format!("账号健康异常({})，已暂停对外动作；请重新登录或更换账号", health));
    }

    // 养号期闸门：仅拦高风险对外动作（带货/发帖），回自己帖评论放行；可由 engine_warmup_gate 关闭
    let warmup_gate_on = engine_cfg_get(conn, "engine_warmup_gate").as_deref() != Some("0");
    if warmup_gate_on && (kind == "keyword" || kind == "publish") {
        let warmup = conn.query_row(
            "SELECT warmup_days FROM nurture_strategies WHERE platform=?1",
            params![platform.to_lowercase()], |r| r.get::<_, i64>(0)).unwrap_or(14);
        let age = created_at.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(999);
        if age < warmup {
            return Err(format!(
                "账号养号期未满（{}/{}天），暂不对外{}；引擎会先把号养熟（可在养号策略调小 warmup_days）",
                age, warmup, if kind == "publish" { "发帖" } else { "带货回复" }));
        }
    }
    Ok(())
}

/// Reddit 发帖前查 subreddit 规则：禁止自我推广/广告的版块直接跳过，避免被封/删帖。
/// 同步（阻塞 HTTP），需在 spawn_blocking 中调用。返回 true=该版块禁止推广（应跳过）。
fn reddit_sub_blocks_promo(post_url: &str) -> bool {
    // 从 .../r/<sub>/comments/... 提取版块名
    let sub = match post_url.split("/r/").nth(1).and_then(|s| s.split('/').next()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return false,
    };
    let url = format!("https://www.reddit.com/r/{}/about/rules.json", sub);
    let client = get_blocking_client();
    let resp = match client.get(&url).header("User-Agent", "unmarket/0.1").send() { Ok(r) => r, Err(_) => return false };
    if !resp.status().is_success() { return false; }
    let v: serde_json::Value = resp.json().unwrap_or_default();
    let markers = ["self-promo", "self promo", "no promotion", "no advertis", "advertising is not",
                   "promotion is not", "no self-advertis", "spam", "9:1", "self-advertising"];
    if let Some(rules) = v.get("rules").and_then(|r| r.as_array()) {
        for rule in rules {
            let text = format!(
                "{} {}",
                rule.get("short_name").and_then(|s| s.as_str()).unwrap_or(""),
                rule.get("description").and_then(|s| s.as_str()).unwrap_or("")
            ).to_lowercase();
            if markers.iter().any(|m| text.contains(m)) {
                return true;
            }
        }
    }
    false
}

/// 宽松解析时间戳（兼容 rfc3339 与 "YYYY-MM-DD HH:MM:SS"）。
pub(crate) fn parse_dt(s: &str) -> Option<chrono::DateTime<Utc>> {
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Utc));
    }
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|n| chrono::DateTime::<Utc>::from_naive_utc_and_offset(n, Utc))
}

/// 平台首页（养号/体检导航用）。
/// 按号龄分期定当日养号目标场次：新号轻、成长期重、成熟期维持。
/// 按「预热时长 + 成长时长」两段独立配置判定阶段：
///   预热 [0, warmup) → 成长 [warmup, warmup+growth) → 成熟 [warmup+growth, ∞)。
/// 成熟是终态、无时长。growth<=0 视为成长期长度为 0（预热完直接进成熟）。
fn nurture_phase_and_target(age_days: i64, warmup: i64, growth: i64, s_min: i64, s_max: i64) -> (&'static str, i64) {
    if age_days < warmup { ("warmup", s_min.max(1)) }
    else if age_days < warmup + growth.max(0) { ("growth", s_max.max(s_min)) }
    else { ("mature", ((s_min + s_max + 1) / 2).max(1)) }
}

/// AI 生成平台定制文案（复用 generate_content）。返回标题/正文/话题。
#[tauri::command]
async fn generate_post_content(state: State<'_, AppState>, product_id: String, platform: String, language: String) -> Result<GeneratedPost, String> {
    let product_name: String = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT name FROM products WHERE id=?1", params![product_id],
            |r| r.get::<_, String>(0)).unwrap_or_default()
    };
    let contents = generate_content(state, product_id, vec![platform.clone()], vec![language.clone()]).await?;
    let c = contents.into_iter().next().ok_or_else(|| "生成失败".to_string())?;
    // 标题：中文平台/需要标题的平台用产品名做兜底（后续可单独让 AI 生成标题）
    let title = product_name;
    let topics = if c.hashtags.is_empty() {
        match platform.to_lowercase().as_str() {
            "xiaohongshu" | "xhs" => vec!["好物分享".into(), "效率工具".into()],
            "douyin" => vec!["实用工具".into()],
            _ => vec![],
        }
    } else { c.hashtags };
    Ok(GeneratedPost { title, body: c.body, topics })
}

/// 养号调度（7×24）：按策略+号龄分期+活跃时段，给到期账号入队一条 nurture 任务。
/// 节流到每 ~5 分钟一次；同账号已有 pending/running 养号任务则跳过；按活跃窗口均摊间隔。
fn nurture_schedule_tick(conn: &Connection) {
    let now = Utc::now();
    if let Some(last) = engine_cfg_get(conn, "nurture_last_tick").and_then(|s| parse_dt(&s)) {
        if (now - last).num_seconds() < 300 { return; }
    }
    engine_cfg_set(conn, "nurture_last_tick", &now.to_rfc3339());

    let hour = Local::now().hour() as i64;
    let today = Local::now().format("%Y-%m-%d").to_string();

    // 有全局默认 profile 时，未单独绑定的账号也参与（继承全局）；否则只取已绑定的。
    let has_global = engine_cfg_get(conn, "selected_browser_profile").map(|s| !s.is_empty()).unwrap_or(false);
    // #19 跳过养号(nurture_skip=1)的账号不参与自动养号
    let where_bound = if has_global { "status='active' AND COALESCE(nurture_skip,0)=0" } else { "status='active' AND COALESCE(nurture_skip,0)=0 AND profile_id IS NOT NULL AND profile_id<>''" };
    let accounts: Vec<(String, String, Option<String>, String, Option<String>)> = {
        let sql = format!(
            "SELECT id, platform, created_at, COALESCE(health_status,'unknown'), last_nurture_at FROM accounts WHERE {}",
            where_bound);
        let mut stmt = match conn.prepare(&sql) { Ok(s) => s, Err(_) => return };
        let it = stmt.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?,
        )));
        match it { Ok(rows) => rows.flatten().collect(), Err(_) => return }
    };

    for (aid, platform, created_at, health, last_nurture) in accounts {
        if matches!(health.as_str(), "banned" | "logged_out" | "shadowbanned" | "locked" | "restricted") { continue; }
        let strat = conn.query_row(
            "SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max, session_duration_min, \
                    session_duration_max, active_hours_start, active_hours_end, enabled \
             FROM nurture_strategies WHERE platform=?1",
            params![platform.to_lowercase()],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?, r.get::<_, i64>(5)?, r.get::<_, i64>(6)?, r.get::<_, i64>(7)?, r.get::<_, i64>(8)?)),
        ).ok();
        let (warmup, growth, smin, smax, dmin, dmax, ah_s, ah_e, enabled) = match strat { Some(v) => v, None => continue };
        if enabled == 0 { continue; }

        let in_hours = if ah_s < ah_e { hour >= ah_s && hour < ah_e } else { hour >= ah_s || hour < ah_e };
        if !in_hours { continue; }

        let age_days = created_at.as_deref().and_then(parse_dt).map(|c| (now - c).num_days()).unwrap_or(0);
        let (phase, target) = nurture_phase_and_target(age_days, warmup, growth, smin, smax);

        let done: i64 = conn.query_row(
            "SELECT sessions_completed FROM nurture_daily_logs WHERE account_id=?1 AND date=?2",
            params![aid, today], |r| r.get(0)).unwrap_or(0);
        if done >= target { continue; }

        let pending: bool = conn.query_row(
            "SELECT 1 FROM tasks WHERE task_type='nurture' AND account_id=?1 AND status IN ('pending','running') LIMIT 1",
            params![aid], |_| Ok(true)).unwrap_or(false);
        if pending { continue; }

        // 把目标场次均摊到活跃窗口，控制最小间隔（>=20 分钟），避免扎堆。
        let window_min = if ah_e > ah_s { (ah_e - ah_s) * 60 } else { (24 - (ah_s - ah_e)) * 60 };
        let gap_min = (window_min / target.max(1)).max(20);
        if let Some(last) = last_nurture.as_deref().and_then(parse_dt) {
            if (now - last).num_minutes() < gap_min { continue; }
        }

        let dur = (get_random_delay(dmin as i32, dmax as i32) / 1000) as i64;
        let _ = conn.execute(
            "INSERT INTO tasks (id, task_type, platform, account_id, content, status, retry_count, created_at) \
             VALUES (?1,'nurture',?2,?3,?4,'pending',0,datetime('now'))",
            params![Uuid::new_v4().to_string(), platform, aid, dur.to_string()]);
        log::info!("[NURTURE-SCHED] 入队养号 {} {} {}s | {}期 今日 {}/{}", aid, platform, dur, phase, done + 1, target);
    }
}

/// 账号健康体检调度：每账号每 ~20 小时入队一条 health_check 任务（无 pending 则入）。
fn health_schedule_tick(conn: &Connection) {
    let now = Utc::now();
    if let Some(last) = engine_cfg_get(conn, "health_last_tick").and_then(|s| parse_dt(&s)) {
        if (now - last).num_seconds() < 600 { return; } // 调度本身 10 分钟跑一次
    }
    engine_cfg_set(conn, "health_last_tick", &now.to_rfc3339());

    // 只体检"有自己 profile 或 persona"的账号；没配置的体检无意义（会在全局 profile 里假报掉登录）
    let where_bound = "status='active' AND ((profile_id IS NOT NULL AND profile_id<>'') OR persona_id IS NOT NULL)";
    let accounts: Vec<(String, String, Option<String>)> = {
        let sql = format!("SELECT id, platform, last_health_check FROM accounts WHERE {}", where_bound);
        let mut stmt = match conn.prepare(&sql) { Ok(s) => s, Err(_) => return };
        let it = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)));
        match it { Ok(rows) => rows.flatten().collect(), Err(_) => return }
    };

    for (aid, platform, last_check) in accounts {
        let stale = last_check.as_deref().and_then(parse_dt)
            .map(|c| (now - c).num_hours() >= 20).unwrap_or(true);
        if !stale { continue; }
        let pending: bool = conn.query_row(
            "SELECT 1 FROM tasks WHERE task_type='health_check' AND account_id=?1 AND status IN ('pending','running') LIMIT 1",
            params![aid], |_| Ok(true)).unwrap_or(false);
        if pending { continue; }
        let _ = conn.execute(
            "INSERT INTO tasks (id, task_type, platform, account_id, status, retry_count, created_at) \
             VALUES (?1,'health_check',?2,?3,'pending',0,datetime('now'))",
            params![Uuid::new_v4().to_string(), platform, aid]);
        log::info!("[HEALTH-SCHED] 入队体检 {} {}", aid, platform);
    }
}

/// 为某平台/账号选择并启动其绑定的浏览器 profile（登录态所在）。
/// 优先用任务指定账号；否则取该平台一个已绑定 profile 的账号。
/// 找不到可用 profile 时返回 Err（由调用方转为 blocked，提示去绑定+登录）。
/// 解析账号实际使用的 profile：账号自身绑定 → 该平台任一已绑定账号 → 全局默认 profile。
/// 这让"未单独绑定的账号继承全局默认 profile"成立。
fn resolve_account_profile(conn: &Connection, account_id: &Option<String>, platform: &str) -> Option<String> {
    // 0) 账号所属身份(persona)的 profile —— 同一 Gmail 下所有账号共用这一个 profile/IP/指纹（最高优先级）
    if let Some(aid) = account_id.as_ref().filter(|s| !s.is_empty()) {
        if let Ok(p) = conn.query_row(
            "SELECT pe.profile_id FROM accounts a JOIN personas pe ON pe.id = a.persona_id WHERE a.id=?1",
            params![aid], |r| r.get::<_, Option<String>>(0)) {
            if let Some(p) = p { if !p.is_empty() { return Some(p); } }
        }
    }
    // 1) 账号自身绑定（旧模型，未归属身份时）
    if let Some(aid) = account_id.as_ref().filter(|s| !s.is_empty()) {
        if let Ok(Some(p)) = conn.query_row(
            "SELECT profile_id FROM accounts WHERE id=?1", params![aid],
            |r| r.get::<_, Option<String>>(0)) {
            if !p.is_empty() { return Some(p); }
        }
    }
    // 2) 该平台任一已绑定账号
    if let Ok(p) = conn.query_row(
        "SELECT profile_id FROM accounts WHERE platform=?1 AND profile_id IS NOT NULL AND profile_id<>'' ORDER BY last_nurture_at DESC LIMIT 1",
        params![platform], |r| r.get::<_, String>(0)) {
        if !p.is_empty() { return Some(p); }
    }
    // 3) 全局默认 profile
    engine_cfg_get(conn, "selected_browser_profile").filter(|s| !s.is_empty())
}

async fn engine_select_profile(app: &AppHandle, platform: &str, account_id: &Option<String>) -> Result<(), String> {
    let profile_id: Option<String> = {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        match guard { Ok(conn) => resolve_account_profile(&conn, account_id, platform), Err(_) => None }
    };

    let profile_id = profile_id
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{} 无可用 profile：请给账号绑定 profile，或在设置里设一个全局默认 profile", platform))?;

    // 复用：若该 profile 已是当前活动 profile 且仍有有效标签，则不再新开标签。
    let same_profile = {
        let guard = ENGINE_CURRENT_PROFILE
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .map_err(|e| e.to_string())?;
        guard.as_deref() == Some(profile_id.as_str())
    };
    if same_profile && get_active_tab().is_some() {
        log::info!("[ENGINE] reuse profile {} for {} (tab kept)", profile_id, platform);
        return Ok(());
    }

    let tab_id = unzoo_launch_profile(profile_id.clone()).await?;
    set_active_tab(Some(tab_id));
    if let Ok(mut guard) = ENGINE_CURRENT_PROFILE.get_or_init(|| std::sync::Mutex::new(None)).lock() {
        *guard = Some(profile_id.clone());
    }
    log::info!("[ENGINE] launched profile {} for {}", profile_id, platform);
    Ok(())
}

/// LinkedIn 关键词回复：走信息流就地评论路径（无永久链接）。
/// 调用前已确保浏览器就绪 + 已切到账号 profile。
async fn engine_linkedin_keyword(app: &AppHandle, task: &ClaimedTask, keyword: &str) -> TaskOutcome {
    if keyword.trim().is_empty() {
        return TaskOutcome::Blocked("无关键词，无法在 LinkedIn 信息流发现帖子".into());
    }
    let config = match get_reply_config("linkedin") { Some(c) => c, None => return TaskOutcome::Failed("无 linkedin 回复配置".into()) };
    let search_url = config.search_url.replace("{query}", &urlencoding::encode(keyword));

    // 产品 + AI 配置
    let (product_id, product_name, product_tagline): (Option<String>, String, String) = {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        match guard {
            Ok(c) => {
                let pid = task.campaign_id.as_ref().and_then(|cid| {
                    c.query_row("SELECT product_id FROM campaigns WHERE id = ?1", params![cid], |r| r.get::<_, Option<String>>(0)).ok().flatten()
                });
                let (name, tag) = pid.as_ref().and_then(|p| {
                    c.query_row("SELECT name, tagline FROM products WHERE id = ?1", params![p],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default()))).ok()
                }).unwrap_or(("our tool".to_string(), String::new()));
                (pid, name, tag)
            }
            Err(_) => (None, "our tool".to_string(), String::new()),
        }
    };
    let (provider, api_key) = {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        match guard { Ok(c) => read_ai_config(&c), Err(_) => (String::new(), String::new()) }
    };

    // 导航 + 抽取信息流帖子（阻塞 HTTP → spawn_blocking）
    let su = search_url.clone();
    let posts = tauri::async_runtime::spawn_blocking(move || {
        unzoo_navigate(&su)?;
        std::thread::sleep(std::time::Duration::from_secs(4));
        linkedin_extract_posts()
    }).await;
    let posts = match posts {
        Ok(Ok(p)) if !p.is_empty() => p,
        Ok(Ok(_)) => return TaskOutcome::Retry("LinkedIn 信息流未抽到帖子（可能未登录/未加载），稍后重试".into()),
        Ok(Err(e)) => return TaskOutcome::Retry(format!("抽取信息流失败: {}", e)),
        Err(e) => return TaskOutcome::Retry(format!("抽取任务异常: {}", e)),
    };

    // 去重：跳过已处理过的帖子（按 作者+正文片段 哈希）
    let chosen = {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        let conn = match guard { Ok(c) => c, Err(_) => return TaskOutcome::Retry("DB 锁失败".into()) };
        let mut pick: Option<(FeedPost, String)> = None;
        for p in &posts {
            let key = format!("linkedin-infeed:{}", short_hash(&format!("{}|{}", p.author, p.text.chars().take(80).collect::<String>())));
            let used: bool = conn.query_row(
                "SELECT 1 FROM discovered_posts WHERE post_url=?1 AND status IN ('replied','pending_review','skipped')",
                params![key], |_| Ok(true)).unwrap_or(false);
            if !used { pick = Some((p.clone(), key)); break; }
        }
        pick
    };
    let (post, synth_id) = match chosen {
        Some(v) => v,
        None => return TaskOutcome::Success(None), // 本屏帖子都处理过
    };

    // AI 判定相关性 + 生成回复
    let sr = generate_smart_reply(&provider, &api_key, config.name, config.max_reply_length, "keyword",
        &post.author, &post.text, keyword, &product_name, &product_tagline).await;
    if sr.reason.starts_with("llm_error_fallback") {
        return TaskOutcome::Retry(sr.reason);
    }

    // 落一条 discovered_posts 记录（用于去重 + 审核关联），含意向分
    {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        if let Ok(conn) = guard {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO discovered_posts (id, platform, post_url, post_title, post_content, keyword_matched, relevance_score, intent_score, status, discovered_at) \
                 VALUES (?1,'linkedin',?2,?3,?4,?5,?6,?7,'new',datetime('now'))",
                params![Uuid::new_v4().to_string(), synth_id, post.author, post.text, keyword, sr.intent as f64 / 100.0, sr.intent]);
            let _ = conn.execute("UPDATE discovered_posts SET intent_score=?2 WHERE post_url=?1", params![synth_id, sr.intent]);
        }
    }

    if !sr.relevant {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        if let Ok(conn) = guard { let _ = conn.execute("UPDATE discovered_posts SET status='skipped' WHERE post_url=?1", params![synth_id]); }
        log::info!("[ENGINE][LinkedIn] 跳过不相关帖子（{}）：{}", post.author, sr.reason);
        return TaskOutcome::Success(None);
    }

    // 意向闸门：低于阈值不回复，把额度留给高意向
    let intent_min = { let state = app.state::<AppState>(); let g = state.db.lock(); match g { Ok(c) => engine_intent_min(&c), Err(_) => 40 } };
    if sr.intent < intent_min {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        if let Ok(conn) = guard { let _ = conn.execute("UPDATE discovered_posts SET status='skipped' WHERE post_url=?1", params![synth_id]); }
        log::info!("[ENGINE][LinkedIn] 跳过低意向帖子（{}，意向 {}<{}）", post.author, sr.intent, intent_min);
        return TaskOutcome::Success(None);
    }

    // 演练：打开评论框验证机制，不输入不提交
    if engine_is_dry_run(app) {
        let idx = post.idx;
        let res = tauri::async_runtime::spawn_blocking(move || linkedin_infeed_reply(idx, "", false)).await;
        let mech = match res { Ok(Ok(s)) => s, Ok(Err(e)) => format!("评论框定位失败: {}", e), Err(e) => format!("异常: {}", e) };
        log::info!("[ENGINE][DRY][LinkedIn] 帖子({}) | 提产品={} | 机制={} | 理由={}\n  → 回复: {}",
            post.author, sr.mention_product, mech, sr.reason, sr.reply);
        return TaskOutcome::Success(None);
    }

    let snippet: String = post.text.chars().take(80).collect();

    // 半自动：入审核队列（locator=正文片段，批准时据此在实时信息流重新定位）
    let mode = {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        match guard { Ok(c) => engine_reply_mode(&c), Err(_) => "review".to_string() }
    };
    if mode == "review" {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        if let Ok(conn) = guard {
            let prod = if sr.mention_product { product_id.clone() } else { None };
            let _ = conn.execute(
                "INSERT INTO reply_history (id, post_id, platform, post_url, reply_content, product_mentioned, status, reason, account_id, reply_type, locator, intent_score, created_at) \
                 VALUES (?1,?2,'linkedin',?3,?4,?5,'pending_review',?6,?7,'keyword',?8,?9,datetime('now'))",
                params![Uuid::new_v4().to_string(), synth_id, search_url, sr.reply, prod, sr.reason, task.account_id, snippet, sr.intent]);
            let _ = conn.execute("UPDATE discovered_posts SET status='pending_review' WHERE post_url=?1", params![synth_id]);
        }
        log::info!("[ENGINE][LinkedIn] 已入审核队列（半自动，意向 {}）：{}", sr.intent, post.author);
        return TaskOutcome::Success(None);
    }

    // 全自动：信息流就地发布
    let idx = post.idx;
    let reply = sr.reply.clone();
    let res = tauri::async_runtime::spawn_blocking(move || linkedin_infeed_reply(idx, &reply, true)).await;
    match res {
        Ok(Ok(_)) => {
            let state = app.state::<AppState>();
            let guard = state.db.lock();
            if let Ok(conn) = guard {
                let _ = conn.execute("UPDATE discovered_posts SET status='replied' WHERE post_url=?1", params![synth_id]);
                let prod = if sr.mention_product { product_id.clone() } else { None };
                let rid = Uuid::new_v4().to_string();
                let _ = conn.execute(
                    "INSERT INTO reply_history (id, post_id, platform, post_url, reply_content, product_mentioned, status, reason, account_id, reply_type, locator, intent_score, created_at) \
                     VALUES (?1,?2,'linkedin',?3,?4,?5,'sent',?6,?7,'keyword',?8,?9,datetime('now'))",
                    params![rid, synth_id, search_url, sr.reply, prod, sr.reason, task.account_id, snippet, sr.intent]);
                record_lead(&conn, &rid, "linkedin", &post.author, &search_url, &sr.reply, sr.intent, keyword, &task.account_id);
            }
            TaskOutcome::Success(Some("linkedin-infeed".into()))
        }
        Ok(Err(e)) => TaskOutcome::Retry(format!("LinkedIn 就地回复失败: {}", e)),
        Err(e) => TaskOutcome::Retry(format!("LinkedIn 回复任务异常: {}", e)),
    }
}

/// 执行单个任务：按类型派发。复用现有 publish_content；信息不全则阻塞待人工。
async fn engine_execute(app: &AppHandle, task: &ClaimedTask) -> TaskOutcome {
    let ttype = task.task_type.to_lowercase();
    match ttype.as_str() {
        "article" | "post" | "publish" | "tweet" => {
            let body = task.content.clone().unwrap_or_default();
            if body.trim().is_empty() {
                return TaskOutcome::Blocked("缺少发布内容（content 为空）".into());
            }
            let platform = match &task.platform {
                Some(p) if !p.is_empty() => p.clone(),
                _ => return TaskOutcome::Blocked("缺少平台".into()),
            };
            let account_id = match &task.account_id {
                Some(a) if !a.is_empty() => Some(a.clone()),
                _ => return TaskOutcome::Blocked("未绑定账号，无法自动发布".into()),
            };

            // 发帖前防封闸门：养号期/健康度（对外发帖=高风险）
            {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                if let Ok(conn) = guard {
                    if let Err(e) = account_outbound_guard(&conn, &platform, &account_id, "publish") {
                        return TaskOutcome::Blocked(e);
                    }
                }
            }

            // 从 campaign 关联产品（仅用于记录，发布以 body 为准）
            let (product_id, product_name) = {
                let state = app.state::<AppState>();
                let conn = state.db.lock().ok();
                conn.and_then(|c| {
                    task.campaign_id.as_ref().and_then(|cid| {
                        c.query_row(
                            "SELECT p.id, p.name FROM campaigns ca JOIN products p ON p.id = ca.product_id WHERE ca.id = ?1",
                            params![cid],
                            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                        ).ok()
                    })
                }).unwrap_or_else(|| (String::new(), String::new()))
            };

            let content = Content {
                platform: platform.clone(),
                language: "en".to_string(),
                product_id,
                product_name,
                body,
                hashtags: Vec::new(),
                images: Vec::new(),
                account_id,
            };

            // 复用现有发布逻辑（内部处理 profile/限流/浏览器）
            let state = app.state::<AppState>();
            match publish_content(state, content).await {
                Ok(r) if r.success => TaskOutcome::Success(r.post_url),
                Ok(r) => {
                    let err = r.error.unwrap_or_else(|| "publish failed".into());
                    // 限流/需等待 -> 退避重试；其它 -> 失败重试
                    if err.contains("wait") || err.contains("limit") || err.contains("Daily") {
                        TaskOutcome::Retry(err)
                    } else {
                        TaskOutcome::Failed(err)
                    }
                }
                Err(e) => TaskOutcome::Retry(e),
            }
        }
        // 内容发布：原创内容（含图文/视频）发到指定平台账号，支持定时
        "content_publish" => {
            let post_id = task.content.clone().unwrap_or_default();
            if post_id.trim().is_empty() {
                return TaskOutcome::Blocked("缺少 post id".into());
            }
            let platform = task.platform.clone().unwrap_or_default();
            // 发帖前防封闸门（养号期/健康度）
            {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                if let Ok(conn) = guard {
                    if let Err(e) = account_outbound_guard(&conn, &platform, &task.account_id, "publish") {
                        let _ = conn.execute(
                            "UPDATE posts SET status='scheduled' WHERE id=?1 AND status='publishing'",
                            params![post_id]);
                        return TaskOutcome::Blocked(e);
                    }
                }
            }
            // 载入 post
            let post = {
                let state = app.state::<AppState>();
                state.db.lock().ok().and_then(|c| load_post(&c, &post_id))
            };
            let post = match post {
                Some(p) => p,
                None => return TaskOutcome::Failed("post 不存在".into()),
            };
            match publish_post(app, &post).await {
                Ok(url) => {
                    let state = app.state::<AppState>();
                    let guard = state.db.lock();
                    if let Ok(conn) = guard {
                        let _ = conn.execute(
                            "UPDATE posts SET status='published', published_at=datetime('now'), result_url=?1, error=NULL WHERE id=?2",
                            params![url, post_id]);
                    }
                    TaskOutcome::Success(Some(url))
                }
                Err(e) => {
                    let retry = e.contains("wait") || e.contains("limit")
                        || e.contains("Daily") || e.contains("未就绪") || e.contains("未完成");
                    let state = app.state::<AppState>();
                    let guard = state.db.lock();
                    if let Ok(conn) = guard {
                        if retry {
                            // 重试：保留 publishing，仅记录错误
                            let _ = conn.execute("UPDATE posts SET error=?1 WHERE id=?2", params![e, post_id]);
                        } else {
                            let _ = conn.execute("UPDATE posts SET status='failed', error=?1 WHERE id=?2", params![e, post_id]);
                        }
                    }
                    if retry { TaskOutcome::Retry(e) } else { TaskOutcome::Failed(e) }
                }
            }
        }
        // 成效采集：搜索排名 + 品牌提及（走专用 Default profile，只读、不算 outbound）
        "metrics_collect" => {
            if let Err(e) = ensure_browser_connected().await {
                return TaskOutcome::Retry(format!("浏览器未就绪: {}", e));
            }
            let app2 = app.clone();
            match tauri::async_runtime::spawn_blocking(move || metrics::metrics_collect_all(&app2)).await {
                Ok(Ok((s, m))) => TaskOutcome::Success(Some(format!("采集完成：排名 {} 词，提及 {} 词", s, m))),
                // 被 Google 限流：不要重试（会继续撞墙、加重限流），标 blocked，等明天的 tick 再采
                Ok(Err(e)) if e.contains("限流") || e.contains("CAPTCHA") || e.contains("BLOCKED") => TaskOutcome::Blocked(e),
                Ok(Err(e)) => TaskOutcome::Retry(e),
                Err(e) => TaskOutcome::Failed(format!("采集线程异常: {}", e)),
            }
        }
        // 类型 B：关键词检索 → 回复陌生帖（严格限流，主动获客）
        "reply" | "reply_keyword" | "engage" => {
            let platform = match &task.platform {
                Some(p) if !p.is_empty() => p.clone(),
                _ => return TaskOutcome::Blocked("缺少平台".into()),
            };
            let keyword = task.content.clone().unwrap_or_default();

            // 浏览器就绪检查（发现/回复都依赖）
            if let Err(e) = ensure_browser_connected().await {
                return TaskOutcome::Retry(format!("浏览器未就绪: {}", e));
            }
            // 切到账号绑定的 profile（登录态）——搜索/发现需要已登录，否则只会卡在登录墙
            if let Err(e) = engine_select_profile(app, &platform, &task.account_id).await {
                return TaskOutcome::Blocked(e);
            }

            // 发帖前防封闸门：养号期/健康度（关键词带货=高风险）
            {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                if let Ok(conn) = guard {
                    if let Err(e) = account_outbound_guard(&conn, &platform, &task.account_id, "keyword") {
                        return TaskOutcome::Blocked(e);
                    }
                }
            }

            // LinkedIn 无帖子永久链接 → 走信息流就地回复专用路径
            if platform.eq_ignore_ascii_case("linkedin") {
                return engine_linkedin_keyword(app, task, &keyword).await;
            }

            // campaign 关联产品（生成回复需要产品名/标语）
            let (product_id, product_name, product_tagline): (Option<String>, String, String) = {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                match guard {
                    Ok(c) => {
                        let pid = task.campaign_id.as_ref().and_then(|cid| {
                            c.query_row("SELECT product_id FROM campaigns WHERE id = ?1",
                                params![cid], |r| r.get::<_, Option<String>>(0)).ok().flatten()
                        });
                        // campaign 无产品时，回退用关键词自身绑定的产品（自动获客 tick 入队的任务走这条）
                        let pid = pid.or_else(|| c.query_row(
                            "SELECT product_id FROM keywords WHERE keyword=?1 AND enabled=1 AND product_id IS NOT NULL LIMIT 1",
                            params![keyword], |r| r.get::<_, Option<String>>(0)).ok().flatten());
                        let (name, tag) = pid.as_ref().and_then(|p| {
                            c.query_row("SELECT name, tagline FROM products WHERE id = ?1", params![p],
                                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default()))
                            ).ok()
                        }).unwrap_or(("our tool".to_string(), String::new()));
                        (pid, name, tag)
                    }
                    Err(_) => (None, "our tool".to_string(), String::new()),
                }
            };

            // 取一个待回复的帖子（仅 new 状态，跳过已审核/已跳过）；没有就按关键词先发现
            let existing: Option<String> = {
                let state = app.state::<AppState>();
                let conn = state.db.lock().ok();
                conn.and_then(|c| {
                    c.query_row(
                        "SELECT id FROM discovered_posts WHERE platform = ?1 AND status = 'new' \
                         ORDER BY relevance_score DESC, discovered_at ASC LIMIT 1",
                        params![platform],
                        |r| r.get::<_, String>(0),
                    ).ok()
                })
            };

            let post_id = match existing {
                Some(id) => id,
                None => {
                    if keyword.trim().is_empty() {
                        return TaskOutcome::Blocked("无关键词，无法自动发现帖子".into());
                    }
                    // 阻塞 HTTP 必须在非异步线程执行，否则 drop runtime panic
                    let app2 = app.clone();
                    let (pf, kw) = (platform.clone(), keyword.clone());
                    let discovered = tauri::async_runtime::spawn_blocking(move || {
                        let state = app2.state::<AppState>();
                        discover_posts(state, pf, kw)
                    }).await;
                    match discovered {
                        Ok(Ok(posts)) if !posts.is_empty() => posts[0].id.clone(),
                        Ok(Ok(_)) => return TaskOutcome::Retry("未发现匹配帖子，稍后重试".into()),
                        Ok(Err(e)) => return TaskOutcome::Retry(format!("发现帖子失败: {}", e)),
                        Err(e) => return TaskOutcome::Retry(format!("发现任务异常: {}", e)),
                    }
                }
            };

            // 取帖子 URL + 发现时标题
            let (post_url, fallback_title): (String, String) = {
                let state = app.state::<AppState>();
                let conn = state.db.lock().ok();
                conn.and_then(|c| c.query_row(
                    "SELECT post_url, COALESCE(post_title,'') FROM discovered_posts WHERE id = ?1",
                    params![post_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).ok()
                ).unwrap_or_default()
            };
            if post_url.is_empty() {
                return TaskOutcome::Failed("帖子无 URL".into());
            }

            // Reddit：发帖前查 subreddit 规则，禁止推广的版块直接跳过（防封/防删帖）
            if platform.eq_ignore_ascii_case("reddit") {
                let purl = post_url.clone();
                let blocks = tauri::async_runtime::spawn_blocking(move || reddit_sub_blocks_promo(&purl))
                    .await.unwrap_or(false);
                if blocks {
                    let state = app.state::<AppState>();
                    if let Ok(conn) = state.db.lock() {
                        let _ = conn.execute("UPDATE discovered_posts SET status='skipped' WHERE id=?1", params![post_id]);
                    }
                    log::info!("[GUARD] 跳过禁推广版块的帖子 {}", post_url);
                    return TaskOutcome::Success(None);
                }
            }

            // 1) 打开帖子读真实标题+正文，回填 post_content（这是"正确回复"的前提）
            let purl = post_url.clone();
            let (real_title, real_body) = tauri::async_runtime::spawn_blocking(move || fetch_post_detail(&purl))
                .await.unwrap_or((String::new(), String::new()));
            if !real_body.is_empty() {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                if let Ok(conn) = guard {
                    let _ = conn.execute("UPDATE discovered_posts SET post_content = ?1 WHERE id = ?2",
                        params![real_body, post_id]);
                }
            }
            let title_for_llm = if !real_title.is_empty() { real_title } else { fallback_title };

            // 2) AI 判定相关性 + 生成针对本帖的回复
            let (provider, api_key) = {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                match guard { Ok(c) => read_ai_config(&c), Err(_) => (String::new(), String::new()) }
            };
            let cfg = get_reply_config(&platform);
            let max_len = cfg.as_ref().map(|c| c.max_reply_length).unwrap_or(500);
            let pname: &str = cfg.as_ref().map(|c| c.name).unwrap_or(platform.as_str());
            let sr = generate_smart_reply(&provider, &api_key, pname, max_len, "keyword",
                &title_for_llm, &real_body, &keyword, &product_name, &product_tagline).await;

            // LLM 瞬时报错 → 重试，避免把模板兜底内容塞进队列
            if sr.reason.starts_with("llm_error_fallback") {
                return TaskOutcome::Retry(sr.reason);
            }

            // 存意向分（用于排序/分析）
            {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                if let Ok(conn) = guard {
                    let _ = conn.execute("UPDATE discovered_posts SET intent_score=?2 WHERE id=?1", params![post_id, sr.intent]);
                }
            }

            // 不相关 → 标记跳过，绝不刷屏
            if !sr.relevant {
                let state = app.state::<AppState>();
                if let Ok(conn) = state.db.lock() {
                    let _ = conn.execute("UPDATE discovered_posts SET status='skipped' WHERE id=?1", params![post_id]);
                }
                log::info!("[ENGINE] 跳过不相关帖子 {}：{}", post_id, sr.reason);
                return TaskOutcome::Success(None);
            }

            // 意向闸门：低于阈值不回复，把额度留给高意向
            let intent_min = { let state = app.state::<AppState>(); let g = state.db.lock(); match g { Ok(c) => engine_intent_min(&c), Err(_) => 40 } };
            if sr.intent < intent_min {
                let state = app.state::<AppState>();
                if let Ok(conn) = state.db.lock() {
                    let _ = conn.execute("UPDATE discovered_posts SET status='skipped' WHERE id=?1", params![post_id]);
                }
                log::info!("[ENGINE] 跳过低意向帖子 {}（意向 {}<{}）", post_id, sr.intent, intent_min);
                return TaskOutcome::Success(None);
            }

            // 演练模式：打印真实生成的回复内容供人工评估，不发布
            if engine_is_dry_run(app) {
                log::info!("[ENGINE][DRY] 关键词回复 | 帖子 {} | 意向={} | 提产品={} | 理由={}\n  → 回复: {}",
                    post_id, sr.intent, sr.mention_product, sr.reason, sr.reply);
                return TaskOutcome::Success(None);
            }

            // 半自动：入审核队列，等人工批准后再发
            let mode = {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                match guard { Ok(c) => engine_reply_mode(&c), Err(_) => "review".to_string() }
            };
            if mode == "review" {
                let state = app.state::<AppState>();
                if let Ok(conn) = state.db.lock() {
                    let prod = if sr.mention_product { product_id.clone() } else { None };
                    let _ = conn.execute(
                        "INSERT INTO reply_history (id, post_id, platform, post_url, reply_content, product_mentioned, status, reason, account_id, reply_type, intent_score, created_at) \
                         VALUES (?1,?2,?3,?4,?5,?6,'pending_review',?7,?8,'keyword',?9,datetime('now'))",
                        params![Uuid::new_v4().to_string(), post_id, platform, post_url, sr.reply, prod, sr.reason, task.account_id, sr.intent]);
                    let _ = conn.execute("UPDATE discovered_posts SET status='pending_review' WHERE id=?1", params![post_id]);
                }
                log::info!("[ENGINE] 已入审核队列（半自动，意向 {}）: {}", sr.intent, post_id);
                return TaskOutcome::Success(None);
            }

            // 全自动：带上 LLM 生成内容直接发布（复用限流/导航/提交序列）
            let app2 = app.clone();
            let prod = if sr.mention_product { product_id.clone() } else { None };
            let (pid, content) = (post_id.clone(), sr.reply.clone());
            let replied = tauri::async_runtime::spawn_blocking(move || {
                let state = app2.state::<AppState>();
                reply_to_post(state, pid, prod, Some(content))
            }).await;
            match replied {
                Ok(Ok(r)) if r.success => TaskOutcome::Success(Some(r.post_url)),
                Ok(Ok(r)) => {
                    let err = r.error.unwrap_or_else(|| "reply failed".into());
                    if err.contains("wait") || err.contains("limit") || err.contains("Daily") || err.contains("分钟") {
                        TaskOutcome::Retry(err)
                    } else {
                        TaskOutcome::Failed(err)
                    }
                }
                Ok(Err(e)) => TaskOutcome::Retry(e),
                Err(e) => TaskOutcome::Retry(format!("回复任务异常: {}", e)),
            }
        }
        // 类型 A：回复我们自己帖子下的评论（宽松限流，社区运营）
        "reply_mention" => {
            let platform = match &task.platform {
                Some(p) if !p.is_empty() => p.clone(),
                _ => return TaskOutcome::Blocked("缺少平台".into()),
            };
            let our_post_url = match &task.target_url {
                Some(u) if !u.is_empty() => u.clone(),
                _ => return TaskOutcome::Blocked("reply_mention 需要 target_url（我们帖子的链接）".into()),
            };

            if let Err(e) = ensure_browser_connected().await {
                return TaskOutcome::Retry(format!("浏览器未就绪: {}", e));
            }

            // 发帖前防封闸门：仅健康度（回自己帖评论=低风险，不受养号期限制）
            {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                if let Ok(conn) = guard {
                    if let Err(e) = account_outbound_guard(&conn, &platform, &task.account_id, "mention") {
                        return TaskOutcome::Blocked(e);
                    }
                }
            }

            // 1) 抓取该帖的评论
            let comments = match fetch_post_comments(&platform, &our_post_url).await {
                Ok(c) => c,
                Err(e) => {
                    if e.contains("暂未支持") { return TaskOutcome::Blocked(e); }
                    return TaskOutcome::Retry(e);
                }
            };
            if comments.is_empty() {
                return TaskOutcome::Success(None); // 暂无评论，无需处理
            }

            // 2) 去重入库，挑一个新评论本轮回复
            let target: Option<FetchedComment> = {
                let state = app.state::<AppState>();
                let guard = state.db.lock().ok();
                guard.and_then(|conn| {
                    let mut chosen: Option<FetchedComment> = None;
                    for c in &comments {
                        let inserted = conn.execute(
                            "INSERT OR IGNORE INTO inbound_comments \
                             (id, our_post_url, platform, comment_id, comment_author, comment_text, comment_permalink, account_id, status) \
                             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'new')",
                            params![
                                Uuid::new_v4().to_string(), our_post_url, platform,
                                c.id, c.author, c.text, c.permalink, task.account_id
                            ],
                        ).unwrap_or(0);
                        if inserted == 1 && chosen.is_none() {
                            chosen = Some(c.clone());
                        }
                    }
                    chosen
                })
            };

            let comment = match target {
                Some(c) => c,
                None => return TaskOutcome::Success(None), // 评论都已处理过
            };

            // 3) 生成针对评论的上下文回复（作者口吻，不广告）
            let reply_text = {
                let (provider, api_key) = {
                    let state = app.state::<AppState>();
                    let guard = state.db.lock();
                    match guard { Ok(c) => read_ai_config(&c), Err(_) => (String::new(), String::new()) }
                };
                let cfg = get_reply_config(&platform);
                let max_len = cfg.as_ref().map(|c| c.max_reply_length).unwrap_or(500);
                let pname: &str = cfg.as_ref().map(|c| c.name).unwrap_or(platform.as_str());
                let sr = generate_smart_reply(&provider, &api_key, pname, max_len, "mention",
                    "(reader's comment on our own post)", &comment.text, "", "our product", "").await;
                // mention 场景几乎总相关；不相关时退回礼貌致谢
                if sr.relevant && !sr.reply.is_empty() { sr.reply }
                else { generate_comment_reply(&comment.text) }
            };
            let reply_url = if comment.permalink.is_empty() {
                our_post_url.clone()
            } else {
                comment.permalink.clone()
            };

            // 演练模式：打印生成的回复供评估，不真实发布
            if engine_is_dry_run(app) {
                log::info!("[ENGINE][DRY] 评论回复 | 评论 {} @ {} | → 回复: {}", comment.id, reply_url, reply_text);
                return TaskOutcome::Success(Some(reply_url));
            }

            // 切到账号绑定的 profile 再发布回复
            if let Err(e) = engine_select_profile(app, &platform, &task.account_id).await {
                return TaskOutcome::Blocked(e);
            }
            // 阻塞 HTTP 必须在非异步线程执行
            let (pf, ru, rt) = (platform.clone(), reply_url.clone(), reply_text.clone());
            let posted = tauri::async_runtime::spawn_blocking(move || {
                post_reply_to_url(&pf, &ru, &rt)
            }).await;
            match posted {
                Ok(Ok(())) => {
                    let state = app.state::<AppState>();
                    if let Ok(conn) = state.db.lock() {
                        let _ = conn.execute(
                            "UPDATE inbound_comments SET status='replied', replied_at=datetime('now') \
                             WHERE our_post_url=?1 AND comment_id=?2",
                            params![our_post_url, comment.id],
                        );
                    }
                    TaskOutcome::Success(Some(reply_url))
                }
                Ok(Err(e)) => TaskOutcome::Retry(format!("回复评论失败: {}", e)),
                Err(e) => TaskOutcome::Retry(format!("回复任务异常: {}", e)),
            }
        }
        // 养号：模拟真人浏览，提升账号权重（地基，决定其它流程能不能不被封）
        "nurture" => {
            let platform = match &task.platform {
                Some(p) if !p.is_empty() => p.clone(),
                _ => return TaskOutcome::Blocked("缺少平台".into()),
            };
            let account_id = match &task.account_id {
                Some(a) if !a.is_empty() => a.clone(),
                _ => return TaskOutcome::Blocked("养号任务缺少账号".into()),
            };
            let duration: i64 = task.content.as_deref().and_then(|s| s.parse().ok()).unwrap_or(120);

            if let Err(e) = ensure_browser_connected().await {
                return TaskOutcome::Retry(format!("浏览器未就绪: {}", e));
            }
            if let Err(e) = engine_select_profile(app, &platform, &task.account_id).await {
                return TaskOutcome::Blocked(e);
            }
            if engine_is_dry_run(app) {
                log::info!("[ENGINE][DRY] 养号演练 {} {} {}s（不真实浏览）", account_id, platform, duration);
                return TaskOutcome::Success(None);
            }

            // GitHub 走专属领域社交养号（star/follow/watch + L2 评论），不走通用滚动。
            if platform.eq_ignore_ascii_case("github") {
                return match nurture::github_nurture_run(app, &account_id, duration).await {
                    Ok(msg) => { log::info!("[GH-NURTURE] {}", msg); TaskOutcome::Success(None) }
                    Err(e) if e.contains("未登录") => {
                        let st = app.state::<AppState>();
                        if let Ok(c) = st.db.lock() {
                            let _ = c.execute("UPDATE accounts SET health_status='logged_out', last_health_check=datetime('now') WHERE id=?1", params![account_id]);
                        }
                        TaskOutcome::Blocked(e)
                    }
                    Err(e) => TaskOutcome::Retry(format!("GitHub 养号失败: {}", e)),
                };
            }
            if platform.eq_ignore_ascii_case("twitter") || platform.eq_ignore_ascii_case("x") {
                return match nurture::x_nurture_run(app, &account_id, duration).await {
                    Ok(msg) => { log::info!("[X-NURTURE] {}", msg); TaskOutcome::Success(None) }
                    Err(e) if e.contains("未登录") => {
                        let st = app.state::<AppState>();
                        if let Ok(c) = st.db.lock() {
                            let _ = c.execute("UPDATE accounts SET health_status='logged_out', last_health_check=datetime('now') WHERE id=?1", params![account_id]);
                        }
                        TaskOutcome::Blocked(e)
                    }
                    Err(e) => TaskOutcome::Retry(format!("X 养号失败: {}", e)),
                };
            }

            let started = Utc::now().to_rfc3339();
            let pf = platform.clone();
            let browse = tauri::async_runtime::spawn_blocking(move || simulate_browsing_blocking(&pf, duration)).await;
            let browse = match browse { Ok(r) => r, Err(e) => return TaskOutcome::Retry(format!("养号任务异常: {}", e)) };
            match browse {
                Ok(secs) => {
                    let state = app.state::<AppState>();
                    if let Ok(conn) = state.db.lock() {
                        let now = Utc::now().to_rfc3339();
                        let today = Local::now().format("%Y-%m-%d").to_string();
                        // 成功浏览即证明已登录且可用 → 标记健康（同时清掉历史误判的 logged_out）
                        let _ = conn.execute(
                            "UPDATE accounts SET nurture_started_at=COALESCE(nurture_started_at,?1), last_nurture_at=?1, \
                             total_nurture_seconds=COALESCE(total_nurture_seconds,0)+?2, health_status='healthy', last_health_check=?1 WHERE id=?3",
                            params![now, secs, account_id]);
                        let _ = conn.execute(
                            "INSERT INTO nurture_sessions (id, account_id, started_at, ended_at, duration_seconds, status) \
                             VALUES (?1,?2,?3,?4,?5,'completed')",
                            params![Uuid::new_v4().to_string(), account_id, started, now, secs]);
                        let _ = conn.execute(
                            "INSERT INTO nurture_daily_logs (id, account_id, date, sessions_completed, total_seconds) \
                             VALUES (?1,?2,?3,1,?4) \
                             ON CONFLICT(account_id,date) DO UPDATE SET sessions_completed=sessions_completed+1, total_seconds=total_seconds+?4",
                            params![Uuid::new_v4().to_string(), account_id, today, secs]);
                    }
                    log::info!("[ENGINE] 养号完成 {} {} {}s", account_id, platform, secs);
                    TaskOutcome::Success(None)
                }
                Err(e) if e.contains("未登录") => {
                    let state = app.state::<AppState>();
                    if let Ok(conn) = state.db.lock() {
                        let _ = conn.execute("UPDATE accounts SET health_status='logged_out', last_health_check=datetime('now') WHERE id=?1", params![account_id]);
                    }
                    TaskOutcome::Blocked(e)
                }
                Err(e) => TaskOutcome::Retry(format!("养号失败: {}", e)),
            }
        }
        // 账号体检：检测登录态/健康度，写回 accounts.health_status
        "health_check" => {
            let platform = match &task.platform {
                Some(p) if !p.is_empty() => p.clone(),
                _ => return TaskOutcome::Blocked("缺少平台".into()),
            };
            let account_id = match &task.account_id {
                Some(a) if !a.is_empty() => a.clone(),
                _ => return TaskOutcome::Blocked("体检缺少账号".into()),
            };
            if let Err(e) = ensure_browser_connected().await {
                return TaskOutcome::Retry(format!("浏览器未就绪: {}", e));
            }
            if let Err(e) = engine_select_profile(app, &platform, &task.account_id).await {
                return TaskOutcome::Blocked(e);
            }
            if engine_is_dry_run(app) {
                log::info!("[ENGINE][DRY] 体检演练 {} {}", account_id, platform);
                return TaskOutcome::Success(None);
            }
            let pf = platform.clone();
            let logged = tauri::async_runtime::spawn_blocking(move || verify_login_blocking(&pf))
                .await.unwrap_or(false);
            let status = if logged { "healthy" } else { "logged_out" };
            let state = app.state::<AppState>();
            if let Ok(conn) = state.db.lock() {
                let _ = conn.execute(
                    "UPDATE accounts SET health_status=?1, last_health_check=datetime('now') WHERE id=?2",
                    params![status, account_id]);
            }
            log::info!("[HEALTH] {} {} → {}", account_id, platform, status);
            TaskOutcome::Success(None)
        }
        // 市场提交：打开提交表单并按生成的资料尽力预填（不自动点提交，留人工核对）
        "marketplace_submit" => {
            let marketplace_id = task.platform.clone().unwrap_or_default();
            let product_id = task.content.clone().unwrap_or_default();
            let url = task.target_url.clone().unwrap_or_default();
            if url.is_empty() { return TaskOutcome::Blocked("缺少表单地址".into()); }

            if let Err(e) = ensure_browser_connected().await {
                return TaskOutcome::Retry(format!("浏览器未就绪: {}", e));
            }
            // 用全局/任一 profile 打开（提交目录通常无需特定账号）
            if let Err(e) = engine_select_profile(app, "marketplace", &None).await {
                return TaskOutcome::Blocked(e);
            }
            let prod = {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                match guard {
                    Ok(c) => c.query_row(
                        "SELECT name, COALESCE(tagline,''), COALESCE(description,''), COALESCE(url,''), COALESCE(repo_url,''), COALESCE(install_cmd,'') FROM products WHERE id=?1",
                        params![product_id],
                        |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?, r.get::<_,String>(4)?, r.get::<_,String>(5)?))
                    ).ok(),
                    Err(_) => None,
                }
            };
            let (pname, ptag, pdesc, purl, repo, install) = match prod {
                Some(v) => v,
                None => return TaskOutcome::Failed("产品信息缺失".into()),
            };
            if engine_is_dry_run(app) {
                log::info!("[ENGINE][DRY] 市场提交预填演练 {} → {}", product_id, url);
                return TaskOutcome::Success(None);
            }
            // 提交者邮箱（很多表单必填）：config.submitter_email
            let email = {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                match guard { Ok(c) => engine_cfg_get(&c, "submitter_email").unwrap_or_default(), Err(_) => String::new() }
            };
            let u = url.clone();
            let res = tauri::async_runtime::spawn_blocking(move || {
                marketplace_prefill(&u, &pname, &ptag, &pdesc, &purl, &repo, &install, &email)
            }).await;
            // 解析自动提交结果 → 状态/结果链接
            let (status, note, result_url): (&str, String, String) = match res {
                Ok(Ok(s)) => {
                    if let Some(u) = s.strip_prefix("submitted:") {
                        ("submitted", s.clone(), u.trim().to_string())
                    } else {
                        // filled_no_submit / submitted_unverified → 待人工核对
                        ("prefilled", s, String::new())
                    }
                }
                Ok(Err(e)) => ("needs_review", e, String::new()),
                Err(e) => ("needs_review", format!("异常: {}", e), String::new()),
            };
            {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                if let Ok(conn) = guard {
                    let _ = conn.execute(
                        "UPDATE marketplace_submissions SET status=?3, error=?4, result_url=CASE WHEN ?5<>'' THEN ?5 ELSE result_url END, updated_at=datetime('now') \
                         WHERE product_id=?1 AND marketplace_id=?2",
                        params![product_id, marketplace_id, status, note, result_url]);
                }
            }
            log::info!("[ENGINE] 市场自动提交 {} → {} ({})", marketplace_id, status, note);
            TaskOutcome::Success(Some(url))
        }
        other => TaskOutcome::Blocked(format!("未支持的任务类型: {}", other)),
    }
}

/// 引擎主循环（7×24）。在独立 tokio 任务中运行；每次迭代锁库为同步块，await 前释放。
async fn engine_loop(app: AppHandle) {
    ENGINE_RUNNING.store(true, Ordering::SeqCst);
    ENGINE_STOP.store(false, Ordering::SeqCst);
    ENGINE_PROCESSED.store(0, Ordering::SeqCst);

    // 绑定一次 State（State 为 Copy，&AppState: Send，可安全跨 await）；
    // 每个锁 guard 都在更短的块作用域内，await 前必然释放。
    let state = app.state::<AppState>();

    // 崩溃恢复：把上次遗留的 running 任务重置为 pending
    if let Ok(conn) = state.db.lock() {
        let _ = conn.execute("UPDATE tasks SET status = 'pending', started_at = NULL WHERE status = 'running'", []);
        engine_cfg_set(&conn, "engine_running", "1");
        engine_cfg_set(&conn, "engine_started_at", &Utc::now().to_rfc3339());
    }
    log::info!("[ENGINE] started");

    loop {
        if ENGINE_STOP.load(Ordering::SeqCst) { break; }

        // 心跳 + 安静时段判断 + 认领（同步块，结束即释放锁）
        let claimed = {
            if let Ok(conn) = state.db.lock() {
                engine_cfg_set(&conn, "engine_heartbeat", &Utc::now().to_rfc3339());
                // 养号 + 体检调度（内部各自节流），再认领下一个任务
                nurture_schedule_tick(&conn);
                health_schedule_tick(&conn);
                post_schedule_tick(&conn);
                metrics::metrics_collect_tick(&conn);
                engage::engage_monitor_tick(&conn);   // 真·Engage 闭环：自动派发关键词获客 + 自有帖评论监控
                let quiet = engine_in_quiet_hours(&conn);
                engine_claim_next(&conn, quiet)
            } else {
                None
            }
        };

        match claimed {
            None => {
                tokio::time::sleep(std::time::Duration::from_secs(ENGINE_POLL_SECS)).await;
            }
            Some(task) => {
                let outcome = engine_execute(&app, &task).await;
                if let Ok(conn) = state.db.lock() {
                    engine_apply_outcome(&conn, &task, outcome);
                    let n = ENGINE_PROCESSED.fetch_add(1, Ordering::SeqCst) + 1;
                    engine_cfg_set(&conn, "engine_processed", &n.to_string());
                }
                tokio::time::sleep(std::time::Duration::from_secs(ENGINE_SPACING_SECS)).await;
            }
        }
    }

    ENGINE_RUNNING.store(false, Ordering::SeqCst);
    if let Ok(conn) = state.db.lock() {
        engine_cfg_set(&conn, "engine_running", "0");
    }
    log::info!("[ENGINE] stopped");
}

#[tauri::command]
fn start_engine(app: AppHandle) -> Result<(), String> {
    if ENGINE_RUNNING.load(Ordering::SeqCst) {
        return Err("引擎已在运行".into());
    }
    tauri::async_runtime::spawn(engine_loop(app));
    Ok(())
}

#[tauri::command]
fn stop_engine() -> Result<(), String> {
    if !ENGINE_RUNNING.load(Ordering::SeqCst) {
        return Err("引擎未运行".into());
    }
    ENGINE_STOP.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn get_engine_status(state: State<'_, AppState>) -> Result<EngineStatus, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let count = |s: &str| -> i64 {
        conn.query_row("SELECT COUNT(*) FROM tasks WHERE status = ?1", params![s], |r| r.get(0)).unwrap_or(0)
    };
    Ok(EngineStatus {
        running: ENGINE_RUNNING.load(Ordering::SeqCst),
        processed: engine_cfg_get(&conn, "engine_processed").and_then(|v| v.parse().ok()).unwrap_or(0),
        last_heartbeat: engine_cfg_get(&conn, "engine_heartbeat"),
        pending: count("pending"),
        running_count: count("running"),
        blocked: count("blocked"),
        completed: count("completed"),
        failed: count("failed"),
    })
}

#[tauri::command]
fn list_tasks(state: State<'_, AppState>, status: Option<String>, limit: Option<i64>) -> Result<Vec<TaskDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let lim = limit.unwrap_or(200);
    let map_row = |row: &rusqlite::Row| -> rusqlite::Result<TaskDto> {
        Ok(TaskDto {
            id: row.get(0)?,
            task_type: row.get(1)?,
            platform: row.get(2)?,
            account_id: row.get(3)?,
            content: row.get(4)?,
            target_url: row.get(5)?,
            status: row.get(6)?,
            retry_count: row.get(7).unwrap_or(0),
            error_message: row.get(8)?,
            scheduled_at: row.get(9)?,
            created_at: row.get(10)?,
        })
    };
    let cols = "id, task_type, platform, account_id, content, target_url, status, retry_count, error_message, scheduled_at, created_at";
    let rows: Vec<TaskDto> = if let Some(s) = status {
        let sql = format!("SELECT {} FROM tasks WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2", cols);
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let iter = stmt.query_map(params![s, lim], map_row).map_err(|e| e.to_string())?;
        iter.filter_map(|r| r.ok()).collect()
    } else {
        let sql = format!("SELECT {} FROM tasks ORDER BY created_at DESC LIMIT ?1", cols);
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let iter = stmt.query_map(params![lim], map_row).map_err(|e| e.to_string())?;
        iter.filter_map(|r| r.ok()).collect()
    };
    Ok(rows)
}

#[tauri::command]
fn unblock_task(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let n = conn.execute(
        "UPDATE tasks SET status = 'pending', error_message = NULL, started_at = NULL, scheduled_at = NULL WHERE id = ?1 AND status = 'blocked'",
        params![id],
    ).map_err(|e| e.to_string())?;
    Ok(n > 0)
}

#[tauri::command]
fn cancel_task(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let n = conn.execute(
        "UPDATE tasks SET status = 'cancelled' WHERE id = ?1 AND status IN ('pending','blocked')",
        params![id],
    ).map_err(|e| e.to_string())?;
    Ok(n > 0)
}

#[tauri::command]
fn retry_task(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let n = conn.execute(
        "UPDATE tasks SET status = 'pending', retry_count = 0, error_message = NULL, started_at = NULL, scheduled_at = NULL WHERE id = ?1 AND status IN ('failed','blocked','cancelled')",
        params![id],
    ).map_err(|e| e.to_string())?;
    Ok(n > 0)
}

#[tauri::command]
fn set_engine_dry_run(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    engine_cfg_set(&conn, "engine_dry_run", if enabled { "1" } else { "0" });
    log::info!("[ENGINE] dry_run = {}", enabled);
    Ok(())
}

#[tauri::command]
fn get_engine_dry_run(state: State<'_, AppState>) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(engine_cfg_get(&conn, "engine_dry_run").as_deref() == Some("1"))
}

// ============ 回复模式 + 半自动审核队列 ============

#[tauri::command]
fn set_engine_reply_mode(state: State<'_, AppState>, mode: String) -> Result<(), String> {
    let m = if mode == "auto" { "auto" } else { "review" };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    engine_cfg_set(&conn, "engine_reply_mode", m);
    log::info!("[ENGINE] reply_mode = {}", m);
    Ok(())
}

#[tauri::command]
fn get_engine_reply_mode(state: State<'_, AppState>) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(engine_reply_mode(&conn))
}

// ============ 自动驾驶（控制台用）：一处看全运行状态 + 安全旋钮 ============

#[derive(Debug, Serialize)]
pub struct RunState {
    running: bool,           // 引擎是否在跑
    autopilot: bool,         // 真·全自动（运行中 + auto + 非演练）
    reply_mode: String,      // auto | review
    dry_run: bool,           // 演练（不真实发布）
    intent_min: i64,         // 意向阈值
    warmup_gate: bool,       // 养号期保护
    quiet_start: Option<i64>,
    quiet_end: Option<i64>,
    processed: i64,
    pending_review: i64,     // 待人工审核回复数（仅 review 模式才有）
    blocked_tasks: i64,      // 阻塞任务数（真正需人工的）
    unhealthy_accounts: i64, // 真·掉登录/封禁账号数（仅统计已配置 profile/persona 的）
    unconfigured_accounts: i64, // 待配置账号数（没绑 profile/persona，不是"问题"，只是没设置）
}

#[tauri::command]
fn get_run_state(state: State<'_, AppState>) -> Result<RunState, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let running = ENGINE_RUNNING.load(Ordering::SeqCst);
    let reply_mode = engine_reply_mode(&conn);
    let dry_run = engine_cfg_get(&conn, "engine_dry_run").as_deref() == Some("1");
    let warmup_gate = engine_cfg_get(&conn, "engine_warmup_gate").as_deref() != Some("0");
    let intent_min = engine_intent_min(&conn);
    let qi = |k: &str| engine_cfg_get(&conn, k).and_then(|v| v.parse::<i64>().ok());
    let cnt = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    Ok(RunState {
        running,
        autopilot: running && reply_mode == "auto" && !dry_run,
        reply_mode,
        dry_run,
        intent_min,
        warmup_gate,
        quiet_start: qi("engine_quiet_start"),
        quiet_end: qi("engine_quiet_end"),
        processed: engine_cfg_get(&conn, "engine_processed").and_then(|v| v.parse().ok()).unwrap_or(0),
        pending_review: cnt("SELECT COUNT(*) FROM reply_history WHERE status='pending_review'"),
        // 排除 metrics_collect：它被 Google 限流时标 blocked 是正常退避，不是"卡住需人工"
        blocked_tasks: cnt("SELECT COUNT(*) FROM tasks WHERE status='blocked' AND task_type<>'metrics_collect'"),
        // 只有"已配置 profile 或 persona"的账号掉登录/被封才算真问题；没配置的不报警
        unhealthy_accounts: cnt(
            "SELECT COUNT(*) FROM accounts WHERE health_status IN ('logged_out','banned','shadowbanned') \
             AND ((profile_id IS NOT NULL AND profile_id<>'') OR persona_id IS NOT NULL)"),
        unconfigured_accounts: cnt(
            "SELECT COUNT(*) FROM accounts WHERE status='active' \
             AND (profile_id IS NULL OR profile_id='') AND persona_id IS NULL"),
    })
}

/// 设置一个安全旋钮（白名单）。控制台一处可调，之后引擎自动遵守。
#[tauri::command]
fn set_run_option(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    let cfg_key = match key.as_str() {
        "reply_mode" => { let v = if value == "auto" { "auto" } else { "review" };
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            engine_cfg_set(&conn, "engine_reply_mode", v); return Ok(()); }
        "dry_run" => "engine_dry_run",
        "warmup_gate" => "engine_warmup_gate",
        "intent_min" => "engine_intent_min",
        "quiet_start" => "engine_quiet_start",
        "quiet_end" => "engine_quiet_end",
        _ => return Err("未知选项".into()),
    };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    engine_cfg_set(&conn, cfg_key, &value);
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct PendingReply {
    id: String,
    post_id: Option<String>,
    platform: String,
    post_url: String,
    reply_content: String,
    reason: Option<String>,
    reply_type: Option<String>,
    product_mentioned: Option<String>,
    intent_score: i64,
    created_at: String,
    post_title: Option<String>,
    post_content: Option<String>,
}

#[tauri::command]
fn list_pending_replies(state: State<'_, AppState>) -> Result<Vec<PendingReply>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT r.id, r.post_id, r.platform, r.post_url, r.reply_content, r.reason, r.reply_type, \
                r.product_mentioned, COALESCE(r.intent_score,0), r.created_at, d.post_title, d.post_content \
         FROM reply_history r LEFT JOIN discovered_posts d ON (d.id = r.post_id OR d.post_url = r.post_id) \
         WHERE r.status = 'pending_review' ORDER BY r.intent_score DESC, r.created_at DESC LIMIT 100",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| Ok(PendingReply {
        id: row.get(0)?,
        post_id: row.get(1)?,
        platform: row.get(2)?,
        post_url: row.get(3)?,
        reply_content: row.get(4)?,
        reason: row.get(5)?,
        reply_type: row.get(6)?,
        product_mentioned: row.get(7)?,
        intent_score: row.get(8)?,
        created_at: row.get(9)?,
        post_title: row.get(10)?,
        post_content: row.get(11)?,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// 解析某平台/账号应使用的浏览器 profile：账号绑定 → 平台任一绑定。
fn resolve_profile_for(conn: &Connection, platform: &str, account_id: &Option<String>) -> Option<String> {
    resolve_account_profile(conn, account_id, platform)
}

/// 在指定 profile 下打开一个标签，使其成为活动 profile（同步，供审核发布用）。
fn ensure_profile_tab(profile_id: &str) -> Result<(), String> {
    let client = get_blocking_client();
    let url = format!("{}/mcp/tools/call", UNZOO_API_BASE);
    let body = serde_json::json!({
        "name": "tab_create",
        "arguments": { "profile_id": profile_id, "url": "about:blank" }
    });
    let resp = client.post(&url).json(&body).send().map_err(|e| e.to_string())?;
    // 必须解析新标签 id 并设为活动标签，否则后续 navigate/click 报"无激活 tab"
    let v: serde_json::Value = resp.json().unwrap_or_default();
    let tab_id = v.get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|inner| inner.get("tab_id").map(|t| match t {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        }));
    match tab_id {
        Some(id) if !id.is_empty() => {
            set_active_tab(Some(id.clone()));
            log::info!("[REVIEW] 切到 profile {} 的新标签 {}", profile_id, id);
        }
        _ => return Err("创建标签成功但未取到 tab_id（无法设为活动标签）".into()),
    }
    std::thread::sleep(std::time::Duration::from_millis(1000));
    Ok(())
}

/// 批准一条待审回复并真实发布（可附带人工编辑后的内容）。
#[tauri::command]
fn approve_reply(state: State<'_, AppState>, id: String, edited_content: Option<String>) -> Result<ReplyResult, String> {
    let (platform, post_url, mut content, post_id, account_id, locator) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT platform, post_url, reply_content, post_id, account_id, locator FROM reply_history WHERE id = ?1 AND status = 'pending_review'",
            params![id],
            |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?,
            )),
        ).map_err(|e| format!("待审回复未找到: {}", e))?
    };
    if let Some(ed) = edited_content {
        if !ed.trim().is_empty() { content = ed; }
    }
    // 发布前先确认 Unzoo 浏览器在线，否则给出清晰提示而非晦涩报错
    if !unzoo_rest_up() {
        return Ok(ReplyResult {
            success: false, platform, post_url, reply_content: Some(content),
            error: Some("Unzoo 浏览器未运行（127.0.0.1:9399 无响应）。请先启动 Unzoo 浏览器，再点「批准发布」。".into()),
        });
    }
    // 切到账号 profile（登录态）
    let profile_id = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        resolve_profile_for(&conn, &platform, &account_id)
    };
    if let Some(pf) = &profile_id {
        let _ = ensure_profile_tab(pf);
    }

    // 发布：LinkedIn 无永久链接 → 回到信息流按 locator 重新定位帖子就地评论；其余走 URL 导航。
    let posted: Result<(), String> = if platform.eq_ignore_ascii_case("linkedin") {
        let snippet = locator.unwrap_or_default();
        if snippet.is_empty() {
            Err("LinkedIn 缺少 locator，无法在信息流重新定位帖子".into())
        } else {
            (|| {
                unzoo_navigate(&post_url)?;
                std::thread::sleep(std::time::Duration::from_secs(4));
                let posts = linkedin_extract_posts()?;
                let hit = posts.into_iter().find(|p| p.text.contains(&snippet) || snippet.contains(&p.text.chars().take(40).collect::<String>()));
                match hit {
                    Some(p) => { linkedin_infeed_reply(p.idx, &content, true)?; Ok(()) }
                    None => Err("信息流中未再找到该帖（可能已被新内容挤下/删除），可稍后重试".into()),
                }
            })()
        }
    } else {
        post_reply_to_url(&platform, &post_url, &content)
    };

    match posted {
        Ok(()) => {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            let _ = conn.execute(
                "UPDATE reply_history SET status='sent', reply_content=?2 WHERE id=?1",
                params![id, content]);
            if let Some(p) = &post_id {
                let _ = conn.execute("UPDATE discovered_posts SET status='replied' WHERE id=?1 OR post_url=?1", params![p]);
            }
            // 批准发布 → 记一条线索（意向分/作者/关键词从关联表兜底取）
            let (intent, author, kw): (i64, String, String) = conn.query_row(
                "SELECT COALESCE(r.intent_score,0), COALESCE(d.post_title,''), COALESCE(d.keyword_matched,'') \
                 FROM reply_history r LEFT JOIN discovered_posts d ON (d.id=r.post_id OR d.post_url=r.post_id) WHERE r.id=?1",
                params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap_or((0, String::new(), String::new()));
            record_lead(&conn, &id, &platform, &author, &post_url, &content, intent, &kw, &account_id);
            log::info!("[REVIEW] 已批准并发布回复 {}（意向 {}）", id, intent);
            Ok(ReplyResult { success: true, platform, post_url, reply_content: Some(content), error: None })
        }
        Err(e) => Ok(ReplyResult { success: false, platform, post_url, reply_content: Some(content), error: Some(e) }),
    }
}

/// 驳回一条待审回复（不发布，对应帖子标记跳过）。
#[tauri::command]
fn reject_reply(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let post_id: Option<String> = conn.query_row(
        "SELECT post_id FROM reply_history WHERE id=?1", params![id],
        |r| r.get(0)).ok().flatten();
    let n = conn.execute(
        "UPDATE reply_history SET status='rejected' WHERE id=?1 AND status='pending_review'",
        params![id]).map_err(|e| e.to_string())?;
    if let Some(p) = post_id {
        let _ = conn.execute("UPDATE discovered_posts SET status='skipped' WHERE id=?1 OR post_url=?1", params![p]);
    }
    Ok(n > 0)
}

// ============ 养号总览 + 手动入队（P0-1/P0-3 的 UI 数据源） ============

#[derive(Debug, Serialize)]
pub struct NurtureOverview {
    account_id: String,
    platform: String,
    username: String,
    bound: bool,
    health_status: String,
    phase: String,
    today_done: i64,
    today_target: i64,
    total_seconds: i64,
    age_days: i64,
    last_nurture_at: Option<String>,
}

#[tauri::command]
fn get_nurture_overview(state: State<'_, AppState>) -> Result<Vec<NurtureOverview>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let today = Local::now().format("%Y-%m-%d").to_string();
    let now = Utc::now();
    let has_global = engine_cfg_get(&conn, "selected_browser_profile").map(|s| !s.is_empty()).unwrap_or(false);
    let mut stmt = conn.prepare(
        "SELECT id, platform, username, COALESCE(health_status,'unknown'), COALESCE(total_nurture_seconds,0), \
                last_nurture_at, created_at, profile_id FROM accounts ORDER BY platform"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok((
        r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
        r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<String>>(5)?,
        r.get::<_, Option<String>>(6)?, r.get::<_, Option<String>>(7)?,
    ))).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows.flatten() {
        let (id, platform, username, health, total, last_n, created, profile) = row;
        // 有自身绑定，或有全局默认 profile 可继承，都算"可用"
        let bound = profile.as_deref().map(|s| !s.is_empty()).unwrap_or(false) || has_global;
        let strat = conn.query_row(
            "SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max, enabled FROM nurture_strategies WHERE platform=?1",
            params![platform.to_lowercase()],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?)),
        ).ok();
        let age_days = created.as_deref().and_then(parse_dt).map(|c| (now - c).num_days()).unwrap_or(0);
        let (phase, target) = match strat {
            Some((w, g, smin, smax, en)) if en != 0 => {
                let (p, t) = nurture_phase_and_target(age_days, w, g, smin, smax);
                (p.to_string(), t)
            }
            _ => ("—".to_string(), 0),
        };
        let today_done: i64 = conn.query_row(
            "SELECT sessions_completed FROM nurture_daily_logs WHERE account_id=?1 AND date=?2",
            params![id, today], |r| r.get(0)).unwrap_or(0);
        out.push(NurtureOverview {
            account_id: id, platform, username, bound,
            health_status: health, phase, today_done, today_target: target,
            total_seconds: total, age_days, last_nurture_at: last_n,
        });
    }
    Ok(out)
}

/// 手动给选中账号入队养号任务（持久化到 tasks 表，由引擎执行——不再用旧的前端内存队列）。
#[tauri::command]
fn enqueue_nurture(state: State<'_, AppState>, account_ids: Vec<String>, duration: Option<i64>) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let dur = duration.unwrap_or(120).clamp(30, 600);
    let has_global = engine_cfg_get(&conn, "selected_browser_profile").map(|s| !s.is_empty()).unwrap_or(false);
    let mut n = 0i64;
    for aid in account_ids {
        let row = conn.query_row(
            "SELECT platform, profile_id FROM accounts WHERE id=?1", params![aid],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        ).ok();
        if let Some((platform, profile)) = row {
            if profile.as_deref().unwrap_or("").is_empty() && !has_global { continue; }
            if conn.execute(
                "INSERT INTO tasks (id, task_type, platform, account_id, content, status, retry_count, created_at) \
                 VALUES (?1,'nurture',?2,?3,?4,'pending',0,datetime('now'))",
                params![Uuid::new_v4().to_string(), platform, aid, dur.to_string()]).is_ok() {
                n += 1;
            }
        }
    }
    log::info!("[NURTURE] 手动入队 {} 条养号任务", n);
    Ok(n)
}

// ============ P1-5 转化闭环 / 轻 CRM ============

#[derive(Debug, Serialize)]
pub struct Lead {
    id: String,
    platform: String,
    author: Option<String>,
    post_url: Option<String>,
    our_reply: Option<String>,
    intent_score: i64,
    status: String,
    keyword: Option<String>,
    notes: Option<String>,
    created_at: String,
}

#[tauri::command]
fn list_leads(state: State<'_, AppState>, status: Option<String>) -> Result<Vec<Lead>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let filt: Option<String> = status.filter(|s| !s.is_empty() && s != "all");
    let mut stmt = conn.prepare(
        "SELECT id, platform, author, post_url, our_reply, COALESCE(intent_score,0), status, keyword, notes, created_at \
         FROM leads WHERE (?1 IS NULL OR status=?1) ORDER BY intent_score DESC, created_at DESC LIMIT 200"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![filt], |row| {
        Ok(Lead {
            id: row.get(0)?, platform: row.get(1)?, author: row.get(2)?, post_url: row.get(3)?,
            our_reply: row.get(4)?, intent_score: row.get(5)?, status: row.get(6)?,
            keyword: row.get(7)?, notes: row.get(8)?, created_at: row.get(9)?,
        })
    }).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn update_lead_status(state: State<'_, AppState>, id: String, status: String, notes: Option<String>) -> Result<bool, String> {
    let st = match status.as_str() {
        "engaged" | "replied_back" | "converted" | "dismissed" => status.as_str(),
        _ => return Err("非法状态".into()),
    };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let n = conn.execute(
        "UPDATE leads SET status=?2, notes=COALESCE(?3, notes), last_checked_at=datetime('now') WHERE id=?1",
        params![id, st, notes]).map_err(|e| e.to_string())?;
    Ok(n > 0)
}

// ============ P1-6 效果分析 ============

#[derive(Debug, Serialize)]
pub struct PlatformStat {
    platform: String,
    discovered: i64,
    skipped: i64,
    replied: i64,
    pending_review: i64,
    avg_intent: i64,
    leads: i64,
    converted: i64,
}

#[derive(Debug, Serialize)]
pub struct MarketingStats {
    totals: PlatformStat,           // platform 字段为 "ALL"
    by_platform: Vec<PlatformStat>,
    top_keywords: Vec<(String, i64, i64)>,  // (keyword, replied, avg_intent)
}

#[tauri::command]
fn get_marketing_stats(state: State<'_, AppState>) -> Result<MarketingStats, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let platforms: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT platform FROM discovered_posts UNION SELECT DISTINCT platform FROM leads"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
        rows.flatten().collect()
    };

    let stat_for = |pf: Option<&str>| -> PlatformStat {
        let cnt = |sql: &str| -> i64 {
            match pf {
                Some(p) => conn.query_row(sql, params![p], |r| r.get(0)).unwrap_or(0),
                None => conn.query_row(&sql.replace("WHERE platform=?1", "").replace("AND platform=?1", ""), [], |r| r.get(0)).unwrap_or(0),
            }
        };
        PlatformStat {
            platform: pf.unwrap_or("ALL").to_string(),
            discovered: cnt("SELECT COUNT(*) FROM discovered_posts WHERE platform=?1"),
            skipped: cnt("SELECT COUNT(*) FROM discovered_posts WHERE status='skipped' AND platform=?1"),
            replied: cnt("SELECT COUNT(*) FROM reply_history WHERE status='sent' AND platform=?1"),
            pending_review: cnt("SELECT COUNT(*) FROM reply_history WHERE status='pending_review' AND platform=?1"),
            avg_intent: cnt("SELECT CAST(COALESCE(AVG(intent_score),0) AS INT) FROM discovered_posts WHERE intent_score>0 AND platform=?1"),
            leads: cnt("SELECT COUNT(*) FROM leads WHERE platform=?1"),
            converted: cnt("SELECT COUNT(*) FROM leads WHERE status='converted' AND platform=?1"),
        }
    };

    let by_platform: Vec<PlatformStat> = platforms.iter().map(|p| stat_for(Some(p))).collect();
    let totals = stat_for(None);

    let top_keywords: Vec<(String, i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT keyword_matched, COUNT(*) c, CAST(COALESCE(AVG(intent_score),0) AS INT) \
             FROM discovered_posts WHERE keyword_matched IS NOT NULL AND keyword_matched<>'' \
             GROUP BY keyword_matched ORDER BY c DESC LIMIT 8"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))
            .map_err(|e| e.to_string())?;
        rows.flatten().collect()
    };

    Ok(MarketingStats { totals, by_platform, top_keywords })
}

// ============ 市场提交（MCP/Skill 上架） ============

#[derive(Debug, Serialize)]
pub struct Marketplace {
    id: String, name: String, kind: String, submit_method: String,
    submit_url: Option<String>, notes: Option<String>,
}

#[tauri::command]
fn list_marketplaces(state: State<'_, AppState>) -> Result<Vec<Marketplace>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, submit_method, submit_url, notes FROM marketplaces WHERE enabled=1 ORDER BY kind, name"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok(Marketplace {
        id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, submit_method: r.get(3)?,
        submit_url: r.get(4)?, notes: r.get(5)?,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct SubmissionRow {
    marketplace_id: String, marketplace_name: String, kind: String, submit_method: String,
    submit_url: Option<String>, notes: Option<String>,
    status: String, listing: Option<String>, result_url: Option<String>, error: Option<String>,
}

/// 列出某产品对所有市场的提交状态（左连接：未提交的也列出，status=pending）。
#[tauri::command]
fn list_marketplace_submissions(state: State<'_, AppState>, product_id: String) -> Result<Vec<SubmissionRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT m.id, m.name, m.kind, m.submit_method, m.submit_url, m.notes, \
                COALESCE(s.status,'pending'), s.listing, s.result_url, s.error \
         FROM marketplaces m \
         LEFT JOIN marketplace_submissions s ON s.marketplace_id=m.id AND s.product_id=?1 \
         WHERE m.enabled=1 ORDER BY m.kind, m.name"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![product_id], |r| Ok(SubmissionRow {
        marketplace_id: r.get(0)?, marketplace_name: r.get(1)?, kind: r.get(2)?, submit_method: r.get(3)?,
        submit_url: r.get(4)?, notes: r.get(5)?,
        status: r.get(6)?, listing: r.get(7)?, result_url: r.get(8)?, error: r.get(9)?,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_product_repo(state: State<'_, AppState>, product_id: String, repo_url: String, install_cmd: Option<String>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE products SET repo_url=?2, install_cmd=?3 WHERE id=?1",
        params![product_id, repo_url, install_cmd.unwrap_or_default()]).map_err(|e| e.to_string())?;
    Ok(())
}

/// 生成某市场所需的上架资料（AI）。模板兜底。
async fn gen_listing(provider: &str, key: &str, m_name: &str, m_method: &str, kind: &str,
    p_name: &str, p_tag: &str, p_desc: &str, p_url: &str, repo: &str, install: &str) -> String {
    let tmpl = || format!(
        "# {p} — {m} 上架资料（{k}）\n\n一句话：{p} {t}\n\n简介：{d}\n\n仓库：{r}\n主页：{u}\n安装：{i}\n\nAwesome 列表条目：\n- [{p}]({r}) - {t}\n",
        p=p_name, m=m_name, k=kind, t=p_tag, d=p_desc, r=repo, u=p_url, i=install);
    if key.is_empty() { return tmpl(); }
    // CLI 渠道（Official MCP Registry / Smithery）：直接产出可落仓库的配置文件 + 发布命令
    let prompt = if m_method == "cli" {
        format!(
            r#"You are preparing CLI-publish artifacts to list a {kind} on "{m_name}" (a CLI/registry-based channel).

Product: {p_name} — {p_tag}
Description: {p_desc}
Website: {p_url}
GitHub repo: {repo}
Install command: {install}

Output EXACTLY the config file(s) + commands this channel needs:
- If "{m_name}" is the Official MCP Registry: output a valid **server.json** in a ```json code block with: "$schema", "name" in reverse-DNS from the repo (e.g. io.github.<owner>/<repo>), "description", "version" (use 0.1.0 if unknown), "repository" {{"url","source":"github"}}, and a "packages" array inferred from the install command (npm→registryType npm, pip→pypi, etc.). Then a **Commands** section: `mcp-publisher login github` then `mcp-publisher publish`.
- If "{m_name}" is Smithery: output a valid **smithery.yaml** in a ```yaml code block (runtime/startCommand appropriate to the install), then a **Commands** section with the connect step (add the repo on smithery.ai, or `npx @smithery/cli ...`).

IMPORTANT: if the product is a desktop app / not an installable package (no npm/pypi/oci package), put a one-line WARNING at the very top that this channel requires a packaged or hostable server and may not accept it. Output only code blocks + the Commands section."#,
            kind=kind, m_name=m_name, p_name=p_name, p_tag=p_tag,
            p_desc=p_desc, p_url=p_url, repo=repo, install=install)
    } else {
        format!(
            r#"You are preparing a marketplace listing to submit a {kind} to "{m_name}" (submission method: {m_method}).

Product: {p_name} — {p_tag}
Description: {p_desc}
Website: {p_url}
GitHub repo: {repo}
Install command: {install}

Produce ready-to-paste listing materials in Markdown with these sections:
- **One-liner** (≤90 chars, punchy, no hype)
- **Short description** (2-3 sentences, what it does + who it's for)
- **Tags/Categories** (5-8, comma-separated, fit a dev tool directory)
- **Install** (the command/config snippet; for MCP include a minimal mcp.json/server.json example using the repo)
- **Awesome-list entry** (single markdown line: `- [{p_name}]({repo}) - <short desc>`)

Be accurate to the product; do not invent features. Output only the Markdown."#,
            kind=kind, m_name=m_name, m_method=m_method, p_name=p_name, p_tag=p_tag,
            p_desc=p_desc, p_url=p_url, repo=repo, install=install)
    };
    let client = reqwest::Client::new();
    let raw = match provider {
        "openai" => call_openai_api(&client, key, &prompt).await,
        "deepseek" => call_deepseek_api(&client, key, &prompt).await,
        "qwen" => call_qwen_api(&client, key, &prompt).await,
        _ => call_gemini_api(&client, key, &prompt).await,
    };
    match raw { Ok(s) if !s.trim().is_empty() => s, _ => tmpl() }
}

#[tauri::command]
async fn generate_marketplace_listing(state: State<'_, AppState>, product_id: String, marketplace_id: String) -> Result<String, String> {
    let (pname, ptag, pdesc, purl, repo, install, provider, key, mkind, mmethod, msub) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let p = conn.query_row(
            "SELECT name, COALESCE(tagline,''), COALESCE(description,''), COALESCE(url,''), COALESCE(repo_url,''), COALESCE(install_cmd,'') FROM products WHERE id=?1",
            params![product_id], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?, r.get::<_,String>(4)?, r.get::<_,String>(5)?))
        ).map_err(|e| format!("产品未找到: {}", e))?;
        let m = conn.query_row(
            "SELECT name, kind, submit_method, COALESCE(submit_url,'') FROM marketplaces WHERE id=?1",
            params![marketplace_id], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?))
        ).map_err(|e| format!("市场未找到: {}", e))?;
        let (provider, key) = read_ai_config(&conn);
        let _ = &m.0;
        (p.0, p.1, p.2, p.3, p.4, p.5, provider, key, m.1, m.2, m.3)
    };
    if repo.trim().is_empty() {
        return Err("该产品还没填 GitHub 仓库地址，请先在产品里设置 repo_url".into());
    }
    let kind = if mkind == "skill" { "skill" } else { "mcp" };
    let mname = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT name FROM marketplaces WHERE id=?1", params![marketplace_id], |r| r.get::<_, String>(0)).unwrap_or_default()
    };
    let listing = gen_listing(&provider, &key, &mname, &mmethod, kind, &pname, &ptag, &pdesc, &purl, &repo, &install).await;
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT INTO marketplace_submissions (id,product_id,marketplace_id,kind,status,listing,submit_url,created_at,updated_at) \
             VALUES (?1,?2,?3,?4,'materials_ready',?5,?6,datetime('now'),datetime('now')) \
             ON CONFLICT(product_id,marketplace_id,kind) DO UPDATE SET listing=?5, \
               status=CASE WHEN status IN ('submitted','listed') THEN status ELSE 'materials_ready' END, updated_at=datetime('now')",
            params![Uuid::new_v4().to_string(), product_id, marketplace_id, kind, listing, msub]);
    }
    Ok(listing)
}

/// 提交：form 类 → 入队浏览器预填任务；其余（pr/cli/auto_index）→ 备好资料，返回提交入口由人工完成。
#[tauri::command]
fn submit_marketplace(state: State<'_, AppState>, product_id: String, marketplace_id: String) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (kind, method, sub_url) = conn.query_row(
        "SELECT kind, submit_method, COALESCE(submit_url,'') FROM marketplaces WHERE id=?1",
        params![marketplace_id], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?))
    ).map_err(|e| format!("市场未找到: {}", e))?;
    let kind = if kind == "skill" { "skill" } else { "mcp" };
    // 必须先有生成的资料
    let has_listing: bool = conn.query_row(
        "SELECT 1 FROM marketplace_submissions WHERE product_id=?1 AND marketplace_id=?2 AND kind=?3 AND listing IS NOT NULL AND listing<>''",
        params![product_id, marketplace_id, kind], |_| Ok(true)).unwrap_or(false);
    if !has_listing {
        return Err("请先「生成资料」再提交".into());
    }
    if method == "form" {
        // 入队浏览器预填任务（platform=marketplace_id, content=product_id, target_url=表单）
        let _ = conn.execute(
            "INSERT INTO tasks (id, task_type, platform, account_id, content, target_url, status, retry_count, created_at) \
             VALUES (?1,'marketplace_submit',?2,NULL,?3,?4,'pending',0,datetime('now'))",
            params![Uuid::new_v4().to_string(), marketplace_id, product_id, sub_url]);
        let _ = conn.execute(
            "UPDATE marketplace_submissions SET status='submitting', submit_url=?4, updated_at=datetime('now') \
             WHERE product_id=?1 AND marketplace_id=?2 AND kind=?3",
            params![product_id, marketplace_id, kind, sub_url]);
        Ok(format!("已入队：引擎会打开 {} 自动填表并提交（用全局登录 profile）；完成后这里会显示 已提交/需人工 状态。", sub_url))
    } else {
        Ok(format!("{} 为 {} 方式：资料已备好，请打开 {} 按生成的资料提交。", marketplace_id, method, sub_url))
    }
}

/// 人工标记提交状态（submitted / listed / skipped / failed）。
#[tauri::command]
fn mark_submission(state: State<'_, AppState>, product_id: String, marketplace_id: String, kind: String, status: String, result_url: Option<String>) -> Result<(), String> {
    let st = match status.as_str() {
        "pending" | "materials_ready" | "submitted" | "listed" | "skipped" | "failed" => status.as_str(),
        _ => return Err("非法状态".into()),
    };
    let k = if kind == "skill" { "skill" } else { "mcp" };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO marketplace_submissions (id,product_id,marketplace_id,kind,status,result_url,created_at,updated_at) \
         VALUES (?1,?2,?3,?4,?5,?6,datetime('now'),datetime('now')) \
         ON CONFLICT(product_id,marketplace_id,kind) DO UPDATE SET status=?5, result_url=COALESCE(?6,result_url), updated_at=datetime('now')",
        params![Uuid::new_v4().to_string(), product_id, marketplace_id, k, st, result_url]).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct SelectorCheck {
    pub platform: String,
    pub search_url: String,
    pub selector: String,
    pub profile_used: String,
    pub page_title: String,
    pub total_links: i64,
    pub matched: i64,
    pub samples: Vec<String>,
    pub note: String,
}

/// 选择器自检：在某平台的搜索页上跑 post_link_selector，报告命中数 + 样本，
/// 便于校准各平台的帖子链接选择器。同步命令（Tauri 在非异步线程执行），
/// 内部全是阻塞 HTTP，可安全调用。
#[tauri::command]
fn check_selector(state: State<'_, AppState>, platform: String, keyword: Option<String>) -> Result<SelectorCheck, String> {
    let config = get_reply_config(&platform)
        .ok_or_else(|| format!("平台 {} 无回复配置（不支持检索回复）", platform))?;
    let kw = keyword.unwrap_or_else(|| "AI tools".to_string());
    let search_url = config.search_url.replace("{query}", &urlencoding::encode(&kw));

    // profile：账号绑定优先，否则全局已选 profile
    let bound: Option<String> = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT profile_id FROM accounts WHERE platform=?1 AND profile_id IS NOT NULL AND profile_id<>'' LIMIT 1",
            params![platform],
            |r| r.get::<_, Option<String>>(0),
        ).ok().flatten()
    };
    let profile_id = bound
        .or_else(|| { let g = get_saved_browser_profile(); if g.is_empty() { None } else { Some(g) } })
        .ok_or_else(|| format!("{} 无可用 profile（请先绑定账号或在设置选择默认 profile）", platform))?;

    // 启动 profile 标签并设为活动
    let client = get_blocking_client();
    let create = client.post(&format!("{}/tabs/create", UNZOO_API_BASE))
        .json(&serde_json::json!({ "profile_id": profile_id }))
        .send()
        .map_err(|e| format!("启动 profile 失败: {}", e))?;
    let cdata: serde_json::Value = create.json().unwrap_or_default();
    let tab_id = cdata.get("data").and_then(|d| d.get("tab_id"))
        .map(|t| if let Some(n) = t.as_i64() { n.to_string() } else { t.as_str().unwrap_or("").to_string() })
        .unwrap_or_default();
    if tab_id.is_empty() {
        return Err("未获取到 tab_id（profile 启动失败）".into());
    }
    set_active_tab(Some(tab_id));

    // 导航到搜索页并等待加载
    unzoo_navigate(&search_url)?;
    std::thread::sleep(std::time::Duration::from_secs(5));

    // 评估选择器命中情况（选择器内含双引号，用反引号包裹）
    let sel = config.post_link_selector;
    let expr = format!(
        "JSON.stringify({{title:document.title,totalLinks:document.querySelectorAll('a').length,matched:document.querySelectorAll(`{sel}`).length,samples:Array.from(document.querySelectorAll(`{sel}`)).slice(0,5).map(a=>a.href||a.getAttribute('href')||'')}})",
        sel = sel
    );
    let raw = unzoo_evaluate(&expr)?;
    // unzoo_evaluate 返回 data 字段的字符串（外层可能再包一层引号），尝试双重解析
    let inner: String = serde_json::from_str::<String>(&raw).unwrap_or_else(|_| raw.clone());
    let parsed: serde_json::Value = serde_json::from_str(&inner)
        .or_else(|_| serde_json::from_str(&raw))
        .unwrap_or_default();

    let matched = parsed.get("matched").and_then(|v| v.as_i64()).unwrap_or(0);
    Ok(SelectorCheck {
        platform,
        search_url,
        selector: sel.to_string(),
        profile_used: profile_id,
        page_title: parsed.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        total_links: parsed.get("totalLinks").and_then(|v| v.as_i64()).unwrap_or(0),
        matched,
        samples: parsed.get("samples").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        note: if matched == 0 {
            "命中 0：可能未登录 / 选择器过时 / 被登录墙或反爬拦截".to_string()
        } else {
            "OK".to_string()
        },
    })
}

// ============================================================================
// Roadmap P1-P3：① AI 配图 ② 内容日历 ③ 矩阵内容变体 ④ AI 视频 ⑤ Engage 升级
// ============================================================================

/// 媒体落盘目录 %APPDATA%/unmarket/media（AI 配图/视频存这里，再加入 post.media_paths）。
fn unmarket_media_dir() -> PathBuf {
    let d = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("unmarket").join("media");
    std::fs::create_dir_all(&d).ok();
    d
}

/// 取 Gemini key：优先 config(ai.key.gemini)，回退环境变量 GEMINI_API_KEY。
fn gemini_key_of(conn: &Connection) -> Option<String> {
    conn.query_row("SELECT value FROM config WHERE key='ai.key.gemini'", [], |r| r.get::<_, String>(0))
        .ok().filter(|s| !s.trim().is_empty())
        .or_else(|| std::env::var("GEMINI_API_KEY").ok().filter(|s| !s.trim().is_empty()))
}


// ---------- ③ 矩阵内容变体 ----------

/// 同主题生成 N 条**差异化**文案（防关联铺量用）。返回 Vec<GeneratedPost>。
#[tauri::command]
async fn generate_post_variations(state: State<'_, AppState>, product_id: String, platform: String, language: String, count: i64) -> Result<Vec<GeneratedPost>, String> {
    let n = count.clamp(1, 10);
    let (provider, key, product_name, tagline, desc) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let (p, k) = ai_provider_key(&conn);
        let (name, tag, d): (String, Option<String>, Option<String>) = conn.query_row(
            "SELECT name, tagline, description FROM products WHERE id=?1", params![product_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap_or_default();
        (p, k, name, tag, d)
    };
    let key = key.ok_or_else(|| "未配置 AI Key".to_string())?;
    let lang_label = if language.starts_with("zh") { "简体中文" } else { "English" };
    let prompt = format!(
        "你是社媒矩阵运营。为产品在 {platform} 上生成 {n} 条**彼此明显不同**的原创推广文案（{lang}）。\
         要求：每条角度/开头/语气/例子都不同，像不同真人写的（防账号关联），自然软植入产品、不要硬广。\
         产品：{name}｜卖点：{tag}｜简介：{desc}。\
         只输出 JSON 数组，每个元素 {{\"title\":\"标题(无标题平台可空)\",\"body\":\"正文\",\"topics\":[\"话题1\",\"话题2\"]}}。不要任何解释。",
        platform = platform, n = n, lang = lang_label,
        name = product_name,
        tag = tagline.unwrap_or_default(),
        desc = desc.unwrap_or_default());
    let client = reqwest::Client::new();
    let raw = ai_complete(&client, &provider, &key, &prompt).await?;
    let cleaned = strip_code_fence(&raw);
    let arr: serde_json::Value = serde_json::from_str(&cleaned)
        .map_err(|e| format!("变体解析失败({}): {}", e, cleaned.chars().take(120).collect::<String>()))?;
    let items = arr.as_array().ok_or_else(|| "AI 未返回数组".to_string())?;
    let out: Vec<GeneratedPost> = items.iter().filter_map(|v| {
        let body = v.get("body").and_then(|b| b.as_str())?.trim().to_string();
        if body.is_empty() { return None; }
        Some(GeneratedPost {
            title: v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            body,
            topics: v.get("topics").and_then(|t| t.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        })
    }).collect();
    if out.is_empty() { return Err("AI 未生成有效变体".into()); }
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct MatrixVariant {
    #[serde(default)] pub title: String,
    pub body: String,
    #[serde(default)] pub topics: Vec<String>,
    #[serde(default)] pub media_paths: Vec<String>,
}

/// 矩阵铺量：把变体按邮箱(account_id)逐条建 post，可错峰排期（防关联）。返回创建数量。
#[tauri::command]
fn matrix_create_posts(state: State<AppState>, product_id: Option<String>, platform: String,
    account_ids: Vec<String>, variants: Vec<MatrixVariant>,
    start_at: Option<String>, interval_minutes: Option<i64>) -> Result<i64, String> {
    if account_ids.is_empty() { return Err("请至少选择一个邮箱/账号".into()); }
    if variants.is_empty() { return Err("没有可用的内容变体".into()); }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let step = interval_minutes.unwrap_or(0).max(0);
    let base = start_at.as_ref().filter(|s| !s.is_empty()).and_then(|s| parse_dt(s));
    let mut created = 0i64;
    for (i, aid) in account_ids.iter().enumerate() {
        let v = &variants[i % variants.len()];   // 变体不够则轮转复用
        let media_json = serde_json::to_string(&v.media_paths).unwrap_or_else(|_| "[]".into());
        let media_type = detect_media_type(&v.media_paths).to_string();
        let topics_json = serde_json::to_string(&v.topics).unwrap_or_else(|_| "[]".into());
        let (sched, status) = match base {
            Some(b) => (Some((b + chrono::Duration::minutes(step * i as i64)).to_rfc3339()), "scheduled"),
            None => (None, "draft"),
        };
        let id = Uuid::new_v4().to_string();
        let r = conn.execute(
            "INSERT INTO posts (id, product_id, platform, account_id, title, body, topics, \
                    media_paths, media_type, scheduled_at, status, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,datetime('now'))",
            params![id, product_id, platform, aid, v.title, v.body, topics_json,
                    media_json, media_type, sched, status]);
        if r.is_ok() { created += 1; }
    }
    Ok(created)
}

// ---------- ④ AI 视频生成（Veo, predictLongRunning）----------

/// 文字→视频：Gemini Veo。轮询长任务，下载 mp4 到本地，返回路径。约 1-3 分钟。
/// model 默认 veo-3.1-fast-generate-preview；aspect 默认 16:9（竖屏传 9:16）。

// ============================================================================
// 矩阵内容工厂：一个创意 → 逐平台格式适配 + 逐 persona 口吻差异化 + 配图 → 错峰铺量
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FactoryItem {
    #[serde(default)] pub account_id: Option<String>,
    #[serde(default)] pub persona_email: Option<String>,
    pub platform: String,
    #[serde(default)] pub angle: String,        // 人设/角度（差异化防关联）
    #[serde(default)] pub title: String,
    #[serde(default)] pub body: String,
    #[serde(default)] pub topics: Vec<String>,
    #[serde(default)] pub image_prompt: String, // 给 generate_ai_image 用
    #[serde(default)] pub aspect_ratio: String,
    #[serde(default)] pub media_paths: Vec<String>,
    #[serde(default)] pub product_id: Option<String>,
}

/// 每个平台的"成品形态"约束（这是矩阵工厂的核心：单一创意→各平台正确形态）。
fn platform_style(platform: &str) -> &'static str {
    match platform.to_lowercase().as_str() {
        "xiaohongshu" | "xhs" => "小红书种草笔记：title 带1-2个emoji且吸睛(≤20字)；body 口语化、分3-5短段、适度emoji、强体验感、结尾引导收藏关注；topics 给6-8个(不带#)；image_prompt 描述一张竖版精美生活化配图。",
        "douyin" => "抖音口播短视频脚本：title 是视频标题(≤16字带钩子)；body 写成可直接照读的口播脚本——开头3秒强钩子+中间分点+结尾引导点赞关注；topics 3-5个；image_prompt 描述竖版封面图。",
        "twitter" | "x" => "X/Twitter 推文：title 留空；body ≤270字符、1个犀利观点+自然1-2个hashtag+可选一句CTA；topics 1-2个；image_prompt 描述16:9横版配图。",
        "linkedin" => "LinkedIn 帖子：title 留空；body 专业第一人称、有行业洞见、3-5短段、先给同行价值再软提产品、不硬广；topics 1-2个；image_prompt 留空。",
        "reddit" => "Reddit 帖子：title 像真实用户的真诚标题(不党不硬广)；body 给足上下文、像分享经验/求讨论、产品只在相关处自然带一句；topics 0-2个(subreddit感)；image_prompt 留空。",
        _ => "通用社媒帖子：自然、有信息量、软植入产品；image_prompt 描述一张相关配图。",
    }
}
fn platform_aspect(platform: &str) -> &'static str {
    match platform.to_lowercase().as_str() {
        "xiaohongshu" | "xhs" => "4:5",
        "douyin" => "9:16",
        "twitter" | "x" => "16:9",
        _ => "1:1",
    }
}
const PERSONA_ANGLES: &[&str] = &[
    "资深实操玩家，分享亲测干货和具体步骤",
    "犀利吐槽派，先戳痛点再给解决方案",
    "理性分析党，摆事实讲逻辑做对比",
    "热心新手视角，记录从踩坑到上手的过程",
    "效率控，只讲怎么更快更省事",
    "讲故事的人，用一个真实场景自然带出产品",
];

/// 矩阵工厂·生成：为每个 (平台×persona) 槽位产出**平台适配+人设差异化**的成品文案。
/// 入参 items 只需带 platform/account_id/persona_email；返回填好 title/body/topics/image_prompt/aspect。
#[tauri::command]
async fn matrix_factory_generate(state: State<'_, AppState>, product_id: String, idea: String, items: Vec<FactoryItem>) -> Result<Vec<FactoryItem>, String> {
    if items.is_empty() { return Err("没有要生成的槽位（先选平台/邮箱）".into()); }
    let (provider, key, pname, ptag, pdesc) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let (p, k) = ai_provider_key(&conn);
        let (n, t, d): (String, Option<String>, Option<String>) = conn.query_row(
            "SELECT name, tagline, description FROM products WHERE id=?1", params![product_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap_or_default();
        (p, k, n, t, d)
    };
    let key = key.ok_or_else(|| "未配置 AI Key".to_string())?;
    let client = reqwest::Client::new();
    let mut out: Vec<FactoryItem> = Vec::with_capacity(items.len());
    for (i, mut it) in items.into_iter().enumerate() {
        let angle = if it.angle.is_empty() { PERSONA_ANGLES[i % PERSONA_ANGLES.len()].to_string() } else { it.angle.clone() };
        let prompt = format!(
            "你在做社媒矩阵投放。围绕同一个创意，为指定平台产出一条**成品**内容，并且写成指定人设的口吻（不同账号要像不同真人，防关联）。\n\
             创意主题：{idea}\n产品：{pname}｜卖点：{ptag}｜简介：{pdesc}\n\
             目标平台形态要求：{style}\n人设/角度：{angle}\n\
             只输出 JSON：{{\"title\":\"\",\"body\":\"\",\"topics\":[],\"image_prompt\":\"\"}}。不要解释，不要markdown围栏。",
            idea = if idea.trim().is_empty() { format!("围绕「{}」的价值做一条自然种草", pname) } else { idea.clone() },
            pname = pname, ptag = ptag.clone().unwrap_or_default(), pdesc = pdesc.clone().unwrap_or_default(),
            style = platform_style(&it.platform), angle = angle);
        let raw = ai_complete(&client, &provider, &key, &prompt).await
            .map_err(|e| format!("第{}槽生成失败: {}", i + 1, e))?;
        let v: serde_json::Value = serde_json::from_str(&strip_code_fence(&raw))
            .unwrap_or_else(|_| serde_json::json!({"body": raw}));
        it.angle = angle;
        it.title = v.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        it.body = v.get("body").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        it.topics = v.get("topics").and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        it.image_prompt = v.get("image_prompt").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        it.aspect_ratio = platform_aspect(&it.platform).to_string();
        it.product_id = Some(product_id.clone());
        out.push(it);
    }
    Ok(out)
}

/// 矩阵工厂·铺量：把每个成品槽位按各自账号建 post，可错峰排期（防关联）。返回创建数。
#[tauri::command]
fn factory_commit(state: State<AppState>, items: Vec<FactoryItem>, start_at: Option<String>, interval_minutes: Option<i64>) -> Result<i64, String> {
    if items.is_empty() { return Err("没有可铺量的内容".into()); }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let step = interval_minutes.unwrap_or(0).max(0);
    let base = start_at.as_ref().filter(|s| !s.is_empty()).and_then(|s| parse_dt(s));
    let mut created = 0i64;
    for (i, it) in items.iter().enumerate() {
        if it.body.trim().is_empty() && it.media_paths.is_empty() { continue; }
        let media_json = serde_json::to_string(&it.media_paths).unwrap_or_else(|_| "[]".into());
        let media_type = detect_media_type(&it.media_paths).to_string();
        let topics_json = serde_json::to_string(&it.topics).unwrap_or_else(|_| "[]".into());
        let (sched, status) = match base {
            Some(b) => (Some((b + chrono::Duration::minutes(step * i as i64)).to_rfc3339()), "scheduled"),
            None => (None, "draft"),
        };
        let r = conn.execute(
            "INSERT INTO posts (id, product_id, platform, account_id, title, body, topics, \
                    media_paths, media_type, scheduled_at, status, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,datetime('now'))",
            params![Uuid::new_v4().to_string(), it.product_id, it.platform, it.account_id,
                    it.title, it.body, topics_json, media_json, media_type, sched, status]);
        if r.is_ok() { created += 1; }
    }
    Ok(created)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_path = get_db_path();
    let conn = Connection::open(&db_path).expect("Failed to open database");
    init_db(&conn).expect("Failed to initialize database");
    run_migrations(&conn).expect("Failed to run database migrations");

    // 输入模式开关：config.unzoo_input_mode（默认 human）。控制 unzoo_click/type 走 human_* 还是 browser_*。
    let input_mode = engine_cfg_get(&conn, "unzoo_input_mode").unwrap_or_else(|| "human".to_string());
    unzoo_set_human_mode(input_mode != "browser");
    log::info!("[UNZOO] 输入模式 = {}", if input_mode != "browser" { "human" } else { "browser" });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState {
            db: Mutex::new(conn),
        })
        .invoke_handler(tauri::generate_handler![
            // Dashboard
            get_dashboard_stats,
            // Campaigns
            campaign::list_campaigns,
            campaign::create_campaign,
            campaign::start_campaign,
            campaign::pause_campaign,
            campaign::delete_campaign,
            // Proxies
            list_proxies,
            add_proxy,
            test_proxy,
            delete_proxy,
            // Products
            list_products,
            create_product,
            delete_product,
            analyze_url,
            list_accounts,
            add_account,
            delete_account,
            get_gmail_status,
            setup_gmail,
            confirm_gmail_connected,
            get_register_platforms,
            register_platform,
            sync_all_platforms,
            get_ai_providers,
            generate_content,
            generate_article,
            publish_content,
            publish_contents,
            publish_contents_smart,
            prepare_publish,
            confirm_publish,
            cancel_publish,
            get_publish_strategies,
            get_stats,
            get_detailed_stats,
            get_config,
            set_config,
            get_platform_manual_login,
            configure_ai,
            get_ai_config,
            test_ai_connection,
            fetch_available_models,
            check_browser_status,
            detect_unzoo_path,
            // 回复系统
            add_keyword,
            list_keywords,
            delete_keyword,
            discover_posts,
            list_discovered_posts,
            get_reply_strategies,
            reply_to_post,
            update_post_status,
            generate_ai_reply,
            // Unzoo Profile Management
            unzoo_list_profiles,
            unzoo_create_profile,
            unzoo_delete_profile,
            unzoo_launch_profile,
            ensure_account_profile,
            // Unzoo Fingerprint
            unzoo_randomize_fingerprint,
            unzoo_get_fingerprint,
            // Unzoo Performance/Stealth
            unzoo_set_stealth_mode,
            unzoo_get_performance_mode,
            // Unzoo Proxy
            unzoo_update_profile_proxy,
            // Unzoo Scheduler
            unzoo_list_scheduled_jobs,
            unzoo_create_scheduled_job,
            unzoo_delete_scheduled_job,
            unzoo_pause_scheduled_job,
            unzoo_resume_scheduled_job,
            // Unzoo Cookies
            unzoo_get_cookies,
            unzoo_set_cookie,
            unzoo_import_cookies,
            // Enhanced publishing
            publish_with_profile,
            // Browser Profile Selection
            get_available_browser_profiles,
            save_selected_browser_profile,
            get_selected_browser_profile,
            connect_browser_profile,
            get_browser_status,
            // Account Profile Binding
            bind_account_profile,
            unbind_account_profile,
            get_account_with_profile,
            // Account Nurturing
            get_account_nurture_status,
            list_accounts_with_nurture_status,
            quick_nurture,
            // Nurture Strategy Management
            list_nurture_strategies,
            update_nurture_strategy,
            get_account_nurture_logs,
            get_account_lifecycle,
            finish_account_nurture,
            get_accounts_needing_nurture,
            // Task Execution Engine (执行引擎)
            start_engine,
            stop_engine,
            get_engine_status,
            list_tasks,
            unblock_task,
            cancel_task,
            retry_task,
            set_engine_dry_run,
            get_engine_dry_run,
            check_selector,
            set_engine_reply_mode,
            get_engine_reply_mode,
            get_run_state,
            set_run_option,
            posts::list_posts,
            posts::save_post,
            posts::delete_post,
            posts::schedule_post,
            posts::publish_post_now,
            posts::cancel_post,
            generate_post_content,
            metrics::metrics_list_keywords,
            metrics::metrics_add_keyword,
            metrics::metrics_delete_keyword,
            metrics::metrics_toggle_keyword,
            metrics::metrics_overview,
            metrics::metrics_get_settings,
            metrics::metrics_set_trends,
            metrics::metrics_collect_now,
            set_account_persona,
            account_auto_login,
            persona_login_all,
            persona_provision_all,
            multi_account::persona_list,
            multi_account::persona_create,
            multi_account::persona_create_fixed,
            multi_account::persona_open_gmail_login,
            multi_account::persona_open_browser,
            multi_account::persona_delete,
            multi_account::persona_test_ip,
            persona_platform_catalog,
            persona_remove_platforms,
            airport::airport_set_subscription,
            airport::airport_get_subscription,
            airport::airport_refresh_subscription,
            airport::airport_status,
            list_pending_replies,
            approve_reply,
            reject_reply,
            get_nurture_overview,
            enqueue_nurture,
            gh_domains_catalog,
            get_account_gh_domains,
            set_account_gh_domains,
            x_niches_catalog,
            get_account_x_niches,
            set_account_x_niches,
            sf_domains_catalog,
            get_account_sf_domains,
            set_account_sf_domains,
            platform_scenes,
            account_niches,
            set_unzoo_input_mode,
            list_leads,
            update_lead_status,
            get_marketing_stats,
            list_marketplaces,
            list_marketplace_submissions,
            set_product_repo,
            generate_marketplace_listing,
            submit_marketplace,
            mark_submission,
            // Roadmap P1-P3
            ai::generate_ai_image,
            generate_post_variations,
            matrix_create_posts,
            ai::generate_ai_video,
            engage::engage_inbox,
            engage::engage_summary,
            engage::engage_get_settings,
            engage::engage_set_auto,
            matrix_factory_generate,
            factory_commit,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Auto-start Unzoo Browser on app launch
            std::thread::spawn(|| {
                if let Err(e) = auto_start_unzoo_browser() {
                    log::warn!("Failed to auto-start Unzoo Browser: {}", e);
                }
            });

            // 若已配置机场订阅，启动时拉起自带 mihomo 内核并重建 listener 配置
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || { multi_account::mihomo_boot(&handle); });
            }

            // #11 后台定时（10 分钟）刷新机场订阅，自动替换失效节点
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(airport::airport_refresh_loop(handle));
            }

            // Optional: auto-start the task engine (headless 7x24 / verification).
            // Normal launches start the engine manually from the Tasks page.
            if std::env::var("UNMARKET_ENGINE_AUTOSTART").ok().as_deref() == Some("1") {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(engine_loop(handle));
                log::info!("[ENGINE] auto-start enabled via UNMARKET_ENGINE_AUTOSTART");
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                multi_account::mihomo_stop(); // 退出时关掉自带 mihomo 内核，避免残留进程
            }
        });
}

#[cfg(test)]
mod platform_meta_tests {
    use super::*;

    #[test]
    fn all_29_keys_have_meta() {
        assert_eq!(PLATFORM_KEYS.len(), 29);
        for k in PLATFORM_KEYS {
            assert!(platform_meta(k).is_some(), "missing meta for {}", k);
        }
    }

    #[test]
    fn mode_counts_15_auto_14_manual() {
        let auto = PLATFORM_KEYS.iter().filter(|k| platform_meta(k).unwrap().mode == "auto").count();
        let manual = PLATFORM_KEYS.iter().filter(|k| platform_meta(k).unwrap().mode == "manual").count();
        assert_eq!(auto, 15, "auto count");
        assert_eq!(manual, 14, "manual count");
    }

    #[test]
    fn spot_checks() {
        let g = platform_meta("github").unwrap();
        assert_eq!((g.scene, g.region, g.mode), ("research", "us", "auto"));
        let x = platform_meta("xiaohongshu").unwrap();
        assert_eq!((x.scene, x.region, x.mode), ("lifestyle", "cn", "manual"));
        // 敌意平台：虽支持 google_oauth 但仍为 manual
        assert_eq!(platform_meta("twitter").unwrap().mode, "manual");
        assert_eq!(platform_meta("reddit").unwrap().mode, "manual");
        // 别名解析
        assert!(platform_meta("redbook").is_some());
        assert!(platform_meta("okjike").is_some());
        // 未知平台
        assert!(platform_meta("unknown_xyz").is_none());
    }

    #[test]
    fn all_six_scenes_present() {
        use std::collections::HashSet;
        let scenes: HashSet<_> = PLATFORM_KEYS.iter().map(|k| platform_meta(k).unwrap().scene).collect();
        for s in ["research", "product", "social", "content", "career", "lifestyle"] {
            assert!(scenes.contains(s), "missing scene {}", s);
        }
    }

    #[test]
    fn catalog_items_cover_all_keys_with_name() {
        // 不依赖 DB：仅验证 name 解析 + meta 覆盖
        for k in PLATFORM_KEYS {
            let m = platform_meta(k).unwrap();
            let name = get_platform_config(k).map(|c| c.name.to_string());
            assert!(name.is_some(), "platform {} 在 get_platform_config 里没有配置", k);
            assert!(!m.scene.is_empty() && !m.region.is_empty() && !m.mode.is_empty());
        }
    }

    #[test]
    fn nurture_homepage_resolves_to_platform_not_google() {
        // 回归：养号导航曾用残缺的 platform_home()，csdn 落到 google.com 兜底 →
        // 在 google 页上检测 csdn 登录必然失败、养号空转。现已统一走 get_platform_home_url()。
        assert_eq!(get_platform_home_url("csdn"), "https://www.csdn.net");
        for p in ["csdn", "zhihu", "weibo", "twitter", "reddit", "linkedin", "github", "v2ex", "devto", "medium"] {
            assert_ne!(
                get_platform_home_url(p),
                "https://www.google.com",
                "{} 养号导航落到 google 兜底，会导致登录检测失败、不真正操作",
                p
            );
        }
    }

    // ===== GitHub 养号：纯逻辑单元测试 =====

    #[test]
    fn gh_domains_keys_unique_and_topics_nonempty() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for d in GH_DOMAINS {
            assert!(seen.insert(d.key), "duplicate domain key {}", d.key);
            assert!(!d.label.is_empty(), "empty label for {}", d.key);
            assert!(!d.topics.is_empty(), "no topics for {}", d.key);
        }
        for k in ["frontend", "backend", "ml", "ai_coding", "devops"] {
            assert!(GH_DOMAINS.iter().any(|d| d.key == k), "missing domain {}", k);
        }
    }

    #[test]
    fn gh_domain_topics_collects_and_dedups() {
        let t = gh_domain_topics(&["frontend", "ai_coding"]);
        assert!(t.contains(&"react"));
        assert!(t.contains(&"claude"));
        let t2 = gh_domain_topics(&["frontend", "unknown_xyz"]);
        assert!(t2.contains(&"react"));
        let t3 = gh_domain_topics(&["frontend", "frontend"]);
        let uniq: std::collections::HashSet<_> = t3.iter().collect();
        assert_eq!(uniq.len(), t3.len());
    }

    #[test]
    fn gh_daily_quota_by_phase() {
        assert_eq!(gh_daily_quota("warmup"), (1, 0, 0));
        let (s, f, w) = gh_daily_quota("growth");
        assert!(s >= 1 && s <= 3 && f <= 2 && w <= 1);
        let (s2, _, _) = gh_daily_quota("mature");
        assert!(s2 >= 1);
        assert_eq!(gh_daily_quota("unknown"), (1, 0, 0));
    }

    #[test]
    fn gh_l2_gate() {
        assert!(!gh_l2_allowed(30, "warmup", 50));
        assert!(!gh_l2_allowed(6, "growth", 50));
        assert!(!gh_l2_allowed(10, "growth", 0));
        assert!(gh_l2_allowed(7, "growth", 5));
        assert!(gh_l2_allowed(30, "mature", 100));
    }

    #[test]
    fn gh_pick_targets_filters_and_limits() {
        use std::collections::HashSet;
        let cands = vec![
            "https://github.com/a/x".to_string(),
            "https://github.com/b/y".to_string(),
            "https://github.com/c/z".to_string(),
        ];
        let mut already = HashSet::new();
        already.insert("https://github.com/a/x".to_string());
        let picked = gh_pick_targets(&cands, &already, 2, 12345);
        assert!(picked.len() <= 2);
        assert!(!picked.contains(&"https://github.com/a/x".to_string()));
        for p in &picked { assert!(cands.contains(p)); }
    }

    #[test]
    fn gh_pick_targets_empty_when_all_acted() {
        use std::collections::HashSet;
        let cands = vec!["https://github.com/a/x".to_string()];
        let already: HashSet<String> = cands.iter().cloned().collect();
        assert!(gh_pick_targets(&cands, &already, 3, 1).is_empty());
    }

    #[test]
    fn gh_benign_comment_is_nonpromotional() {
        for i in 0..6 {
            let c = gh_benign_comment(i);
            assert!(!c.is_empty());
            assert!(!c.contains("http")); // 不带链接/推广
        }
        assert_ne!(gh_benign_comment(0), gh_benign_comment(1));
    }

    // ===== X 养号：纯逻辑单元测试 =====

    #[test]
    fn x_niches_keys_unique_and_16_dirs() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for n in X_NICHES {
            assert!(seen.insert(n.key), "duplicate niche key {}", n.key);
            assert!(!n.label.is_empty() && !n.keywords.is_empty(), "bad niche {}", n.key);
        }
        assert_eq!(X_NICHES.len(), 16, "X 官方顶层方向应为 16 个");
        for k in ["technology", "business_finance", "science", "gaming", "music"] {
            assert!(X_NICHES.iter().any(|n| n.key == k), "missing niche {}", k);
        }
    }

    #[test]
    fn x_niche_keywords_collects_and_dedups() {
        let kw = x_niche_keywords(&["technology", "science"]);
        assert!(kw.contains(&"tech"));
        assert!(kw.contains(&"science"));
        let kw2 = x_niche_keywords(&["technology", "unknown_xyz"]);
        assert!(kw2.contains(&"tech"));
        let kw3 = x_niche_keywords(&["technology", "technology"]);
        let uniq: std::collections::HashSet<_> = kw3.iter().collect();
        assert_eq!(uniq.len(), kw3.len());
    }

    #[test]
    fn x_daily_quota_by_phase() {
        assert_eq!(x_daily_quota("warmup"), (2, 0, 0));
        let (l, f, e) = x_daily_quota("growth");
        assert!(l >= 3 && f >= 1 && e >= 1);
        let (l2, _, _) = x_daily_quota("mature");
        assert!(l2 >= 1);
        assert_eq!(x_daily_quota("unknown"), (1, 0, 0));
    }

    #[test]
    fn nurture_phase_two_durations() {
        // 预热10 + 成长10：成熟从第 20 天起（= 旧的 2×warmup 行为）
        assert_eq!(nurture_phase_and_target(0, 10, 10, 2, 4).0, "warmup");
        assert_eq!(nurture_phase_and_target(9, 10, 10, 2, 4).0, "warmup");
        assert_eq!(nurture_phase_and_target(10, 10, 10, 2, 4).0, "growth");
        assert_eq!(nurture_phase_and_target(19, 10, 10, 2, 4).0, "growth");
        assert_eq!(nurture_phase_and_target(20, 10, 10, 2, 4).0, "mature");
        // 两段独立：预热7 + 成长30 → 成熟从第 37 天起
        assert_eq!(nurture_phase_and_target(6, 7, 30, 2, 4).0, "warmup");
        assert_eq!(nurture_phase_and_target(7, 7, 30, 2, 4).0, "growth");
        assert_eq!(nurture_phase_and_target(36, 7, 30, 2, 4).0, "growth");
        assert_eq!(nurture_phase_and_target(37, 7, 30, 2, 4).0, "mature");
        // 成长时长为 0：预热完直接进成熟
        assert_eq!(nurture_phase_and_target(5, 5, 0, 2, 4).0, "mature");
    }

    #[test]
    fn x_l3_gate() {
        // 闸门跟随 warmup：号龄未到预热期末或无 L1 历史都不解锁
        assert!(!x_l3_allowed(6, 10, 50));    // 号龄 < warmup
        assert!(!x_l3_allowed(20, 10, 0));    // 无 L1 历史
        assert!(x_l3_allowed(10, 10, 1));     // 恰好走完预热期 + 有 L1
        assert!(x_l3_allowed(40, 10, 100));
        // 周期调长后门槛同步后移
        assert!(!x_l3_allowed(14, 21, 1));
        assert!(x_l3_allowed(21, 21, 1));
    }

    #[test]
    fn x_benign_tweet_is_nonpromotional() {
        for i in 0..6 {
            let t = x_benign_tweet(i);
            assert!(!t.is_empty());
            assert!(!t.contains("http"));
        }
        assert_ne!(x_benign_tweet(0), x_benign_tweet(1));
    }

    #[test]
    fn x_classify_health_maps() {
        assert_eq!(x_classify_health("Your account is suspended"), Some("banned"));
        assert_eq!(x_classify_health("Your account has been locked"), Some("locked"));
        assert_eq!(x_classify_health("You are unable to perform this action"), Some("restricted"));
        assert_eq!(x_classify_health("Rate limit exceeded, try again later"), Some("restricted"));
        // 中文界面（简/繁）
        assert_eq!(x_classify_health("你的账号已被冻结"), Some("banned"));
        assert_eq!(x_classify_health("验证你的身份以继续"), Some("locked"));
        assert_eq!(x_classify_health("无法执行此操作，请稍后再试"), Some("restricted"));
        // 顺带测粉丝数解析
        assert_eq!(x_parse_count("1,234 Followers"), Some(1234));
        assert_eq!(x_parse_count("1.2K Followers"), Some(1200));
        assert_eq!(x_parse_count("3.4M"), Some(3_400_000));
        assert_eq!(x_parse_count("Followers 5,678"), Some(5678));
        assert_eq!(x_parse_count("no digits"), None);
        // 正常页面（含登录后导航词）不误报
        assert_eq!(x_classify_health("Home timeline, Post, Notifications, Messages"), None);
        assert_eq!(x_classify_health("首页 发推 通知 私信"), None);
    }
}

#[cfg(test)]
mod gh_db_tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE accounts (id TEXT PRIMARY KEY, gh_domains TEXT);
            CREATE TABLE gh_actions_log (id TEXT PRIMARY KEY, account_id TEXT, action_type TEXT, target TEXT, date TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP);
        ").unwrap();
        c
    }

    #[test]
    fn record_and_already_acted() {
        let c = setup();
        let tgt = "https://github.com/a/x";
        assert!(!gh_already_acted(&c, "acc1", tgt));
        gh_record_action(&c, "acc1", "star", tgt).unwrap();
        assert!(gh_already_acted(&c, "acc1", tgt));
        assert!(!gh_already_acted(&c, "acc2", tgt));
    }

    #[test]
    fn target_persona_count_cross_account() {
        let c = setup();
        let tgt = "https://github.com/a/x";
        gh_record_action(&c, "acc1", "star", tgt).unwrap();
        gh_record_action(&c, "acc2", "star", tgt).unwrap();
        gh_record_action(&c, "acc2", "star", tgt).unwrap();
        assert_eq!(gh_target_persona_count(&c, tgt), 2);
    }

    #[test]
    fn read_account_domains() {
        let c = setup();
        c.execute("INSERT INTO accounts (id, gh_domains) VALUES ('acc1', '[\"frontend\",\"ai_coding\"]')", []).unwrap();
        let d = account_gh_domains(&c, "acc1");
        assert_eq!(d, vec!["frontend".to_string(), "ai_coding".to_string()]);
        assert!(account_gh_domains(&c, "nope").is_empty());
    }
}

#[cfg(test)]
mod x_db_tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE accounts (id TEXT PRIMARY KEY, x_niches TEXT);
            CREATE TABLE x_actions_log (id TEXT PRIMARY KEY, account_id TEXT, action_type TEXT, target TEXT, date TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP);
        ").unwrap();
        c
    }

    #[test]
    fn record_and_already_acted() {
        let c = setup();
        let tgt = "https://x.com/u/status/1";
        assert!(!x_already_acted(&c, "acc1", tgt));
        x_record_action(&c, "acc1", "like", tgt).unwrap();
        assert!(x_already_acted(&c, "acc1", tgt));
        assert!(!x_already_acted(&c, "acc2", tgt));
    }

    #[test]
    fn target_persona_count_cross_account() {
        let c = setup();
        let tgt = "https://x.com/u/status/1";
        x_record_action(&c, "acc1", "like", tgt).unwrap();
        x_record_action(&c, "acc2", "like", tgt).unwrap();
        x_record_action(&c, "acc2", "like", tgt).unwrap();
        assert_eq!(x_target_persona_count(&c, tgt), 2);
    }

    #[test]
    fn read_account_niches() {
        let c = setup();
        c.execute("INSERT INTO accounts (id, x_niches) VALUES ('acc1', '[\"technology\",\"science\"]')", []).unwrap();
        assert_eq!(account_x_niches(&c, "acc1"), vec!["technology".to_string(), "science".to_string()]);
        assert!(account_x_niches(&c, "nope").is_empty());
    }
}
