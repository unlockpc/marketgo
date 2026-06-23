//! Engage 获客域：统一互动收件箱（待审回复 + 高意向线索 + 品牌提及）+ 自主驱动器
//! （引擎自动派发关键词获客 + 自有帖评论监控）。

use serde::Serialize;
use tauri::State;
use rusqlite::{Connection, params};
use uuid::Uuid;
use chrono::Utc;

use crate::{AppState, engine_cfg_get, engine_cfg_set, parse_dt, engine_reply_mode};

/// 买点意向加权：命中强购买信号则提分并标 hot。
fn buy_intent_score(text: &str) -> (i64, bool) {
    let t = text.to_lowercase();
    let strong = ["求链接", "怎么买", "哪里买", "如何购买", "多少钱", "下单", "购买链接", "求购",
                  "where to buy", "how to buy", "price", "pricing", "link please", "dm me", "send link", "sign up", "purchase"];
    let medium = ["推荐", "有没有", "求推荐", "想试试", "怎么用", "教程", "对比", "值得吗",
                  "recommend", "alternative", "vs ", "worth it", "how do i", "looking for", "any tool"];
    let mut score = 0i64; let mut hot = false;
    if strong.iter().any(|k| t.contains(k)) { score += 45; hot = true; }
    if medium.iter().any(|k| t.contains(k)) { score += 20; }
    (score.min(60), hot)
}

#[derive(Debug, Serialize)]
pub struct InboxItem {
    kind: String,        // lead | pending_reply | mention
    ref_id: String,
    platform: String,
    author: Option<String>,
    text: String,        // 对方说的话 / 我们的回复 / 提及域名
    url: Option<String>,
    intent: i64,
    hot: bool,           // 强购买信号
    status: String,
    created_at: String,
}

/// 统一互动收件箱：合并 待审回复 + 高意向线索 + 品牌提及，按 hot/意向/时间排序。
/// filter: all | hot | pending_reply | lead | mention
#[tauri::command]
pub(crate) fn engage_inbox(state: State<AppState>, filter: Option<String>) -> Result<Vec<InboxItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let f = filter.unwrap_or_else(|| "all".into());
    let mut items: Vec<InboxItem> = Vec::new();

    // 1) 待审回复（reply_history.pending_review）—— 对方原文用于买点识别
    if f == "all" || f == "hot" || f == "pending_reply" {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT r.id, r.platform, r.post_url, r.reply_content, COALESCE(r.intent_score,0), r.created_at, d.post_content \
             FROM reply_history r LEFT JOIN discovered_posts d ON (d.id = r.post_id OR d.post_url = r.post_id) \
             WHERE r.status='pending_review' ORDER BY r.created_at DESC LIMIT 100") {
            let rows = stmt.query_map([], |row| Ok((
                row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?, row.get::<_, i64>(4)?, row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            )));
            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    let (id, platform, url, reply, mut intent, created, ctx) = r;
                    let (boost, hot) = buy_intent_score(&format!("{} {}", ctx.clone().unwrap_or_default(), reply));
                    intent = (intent + boost).min(100);
                    items.push(InboxItem {
                        kind: "pending_reply".into(), ref_id: id, platform,
                        author: None, text: ctx.unwrap_or(reply), url, intent, hot,
                        status: "待审核".into(), created_at: created,
                    });
                }
            }
        }
    }

    // 2) 线索（leads）
    if f == "all" || f == "hot" || f == "lead" {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, platform, author, post_url, our_reply, COALESCE(intent_score,0), status, created_at \
             FROM leads WHERE status<>'dismissed' ORDER BY created_at DESC LIMIT 100") {
            let rows = stmt.query_map([], |row| Ok((
                row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, String>(7)?,
            )));
            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    let (id, platform, author, url, reply, mut intent, status, created) = r;
                    let (boost, hot) = buy_intent_score(&reply.clone().unwrap_or_default());
                    intent = (intent + boost).min(100);
                    items.push(InboxItem {
                        kind: "lead".into(), ref_id: id, platform, author,
                        text: reply.unwrap_or_default(), url, intent, hot,
                        status, created_at: created,
                    });
                }
            }
        }
    }

    // 3) 品牌提及（metrics: source=mention 的最近采样，detail 里是命中域名）
    if f == "all" || f == "mention" {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, keyword, COALESCE(value,0), detail, captured_at FROM metrics \
             WHERE source='mention' AND COALESCE(value,0)>0 ORDER BY captured_at DESC LIMIT 30") {
            let rows = stmt.query_map([], |row| Ok((
                row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?, row.get::<_, String>(4)?,
            )));
            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    let (id, keyword, value, detail, created) = r;
                    items.push(InboxItem {
                        kind: "mention".into(), ref_id: id.to_string(), platform: "web".into(),
                        author: Some(keyword.clone()),
                        text: format!("品牌提及「{}」命中 {} 个来源 {}", keyword, value, detail.unwrap_or_default()),
                        url: None, intent: 30, hot: false, status: "提及".into(), created_at: created,
                    });
                }
            }
        }
    }

    // hot 过滤
    if f == "hot" { items.retain(|i| i.hot || i.intent >= 70); }
    // 排序：hot 优先 → 意向 → 时间
    items.sort_by(|a, b| b.hot.cmp(&a.hot)
        .then(b.intent.cmp(&a.intent))
        .then(b.created_at.cmp(&a.created_at)));
    items.truncate(150);
    Ok(items)
}

/// Engage 概览数字（hot 线索数 / 待审 / 今日提及）。
#[tauri::command]
pub(crate) fn engage_summary(state: State<AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let pending: i64 = conn.query_row("SELECT COUNT(*) FROM reply_history WHERE status='pending_review'", [], |r| r.get(0)).unwrap_or(0);
    let leads_open: i64 = conn.query_row("SELECT COUNT(*) FROM leads WHERE status NOT IN ('dismissed','converted')", [], |r| r.get(0)).unwrap_or(0);
    let converted: i64 = conn.query_row("SELECT COUNT(*) FROM leads WHERE status='converted'", [], |r| r.get(0)).unwrap_or(0);
    let mentions: i64 = conn.query_row("SELECT COUNT(*) FROM metrics WHERE source='mention' AND COALESCE(value,0)>0", [], |r| r.get(0)).unwrap_or(0);
    Ok(serde_json::json!({
        "pending_review": pending,
        "leads_open": leads_open,
        "converted": converted,
        "mentions": mentions,
    }))
}

// ============================================================================
// 真·Engage 获客闭环：自主驱动器（让引擎自己去监控+回复，而不是等人手点）
// ============================================================================

/// 引擎每拍调用（内部节流）。自动派发两类监控任务：
/// A) 关键词获客：每个启用关键词 × 每平台，挑一个该平台的活跃账号(persona)，入队 engage 任务（受 reply_mode 闸门：review→进收件箱，auto→真回复）。
/// B) 自有帖评论监控：对我们已发布的帖子定期入队 reply_mention，自动读评论并就地回复（社区运营，低风险）。
pub(crate) fn engage_monitor_tick(conn: &Connection) {
    if engine_cfg_get(conn, "engage_auto").as_deref() == Some("0") { return; }   // 默认开
    let now = Utc::now();
    let interval = engine_cfg_get(conn, "engage_interval_secs").and_then(|s| s.parse::<i64>().ok()).unwrap_or(1800);
    if let Some(last) = engine_cfg_get(conn, "engage_last_tick").and_then(|s| parse_dt(&s)) {
        if (now - last).num_seconds() < interval { return; }
    }
    engine_cfg_set(conn, "engage_last_tick", &now.to_rfc3339());

    // ---- A) 关键词获客（跨 persona 矩阵铺开）----
    let cap = engine_cfg_get(conn, "engage_max_inflight").and_then(|s| s.parse::<i64>().ok()).unwrap_or(6);
    let inflight: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE task_type IN ('engage','reply','reply_keyword') AND status IN ('pending','running')",
        [], |r| r.get(0)).unwrap_or(0);
    let mut budget = (cap - inflight).max(0);
    if budget > 0 {
        let kws: Vec<(String, Vec<String>)> = {
            let mut stmt = match conn.prepare(
                "SELECT keyword, COALESCE(platforms,'[]') FROM keywords WHERE enabled=1") { Ok(s) => s, Err(_) => return };
            let it = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
            match it {
                Ok(rows) => rows.flatten().map(|(k, pj)| {
                    let plats: Vec<String> = serde_json::from_str(&pj).unwrap_or_default();
                    (k, plats)
                }).collect(),
                Err(_) => return,
            }
        };
        'outer: for (keyword, platforms) in kws {
            let plats = if platforms.is_empty() { vec!["twitter".to_string(), "reddit".to_string()] } else { platforms };
            for platform in plats {
                if budget <= 0 { break 'outer; }
                // 该平台挑一个活跃账号：尚未在跑同一关键词、优先最久没动的（轮转，防扎堆）
                let acct: Option<String> = conn.query_row(
                    "SELECT a.id FROM accounts a \
                     WHERE lower(a.platform)=lower(?1) AND a.status='active' \
                       AND COALESCE(a.health_status,'unknown') NOT IN ('banned','logged_out','shadowbanned') \
                       AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.task_type IN ('engage','reply','reply_keyword') \
                            AND t.status IN ('pending','running') AND lower(t.platform)=lower(?1) AND t.content=?2) \
                     ORDER BY COALESCE(a.last_nurture_at,'') ASC LIMIT 1",
                    params![platform, keyword], |r| r.get::<_, String>(0)).ok();
                if let Some(aid) = acct {
                    let r = conn.execute(
                        "INSERT INTO tasks (id, task_type, platform, account_id, content, status, retry_count, created_at) \
                         VALUES (?1,'engage',?2,?3,?4,'pending',0,datetime('now'))",
                        params![Uuid::new_v4().to_string(), platform, aid, keyword]);
                    if r.is_ok() { budget -= 1; }
                }
            }
        }
    }

    // ---- B) 自有帖评论监控（社区运营，复用 reply_mention 任务臂）----
    let mention_secs = engine_cfg_get(conn, "engage_mention_secs").and_then(|s| s.parse::<i64>().ok()).unwrap_or(21600); // 6h
    let mut mbudget = engine_cfg_get(conn, "engage_mention_max").and_then(|s| s.parse::<i64>().ok()).unwrap_or(3);
    let posts: Vec<(String, String, String)> = {
        let mut stmt = match conn.prepare(
            "SELECT platform, account_id, result_url FROM posts \
             WHERE status='published' AND result_url IS NOT NULL AND result_url<>'' \
               AND account_id IS NOT NULL AND account_id<>'' \
               AND COALESCE(published_at, created_at) > datetime('now','-14 days') \
             ORDER BY published_at DESC LIMIT 50") { Ok(s) => s, Err(_) => return };
        let it = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)));
        match it { Ok(rows) => rows.flatten().collect(), Err(_) => return }
    };
    for (platform, account_id, url) in posts {
        if mbudget <= 0 { break; }
        let busy: bool = conn.query_row(
            "SELECT 1 FROM tasks WHERE task_type='reply_mention' AND status IN ('pending','running') AND target_url=?1 LIMIT 1",
            params![url], |_| Ok(true)).unwrap_or(false);
        if busy { continue; }
        let last: Option<String> = conn.query_row(
            "SELECT MAX(created_at) FROM tasks WHERE task_type='reply_mention' AND target_url=?1",
            params![url], |r| r.get::<_, Option<String>>(0)).ok().flatten();
        if let Some(l) = last.as_ref().and_then(|s| parse_dt(s)) {
            if (now - l).num_seconds() < mention_secs { continue; }
        }
        let r = conn.execute(
            "INSERT INTO tasks (id, task_type, platform, account_id, target_url, status, retry_count, created_at) \
             VALUES (?1,'reply_mention',?2,?3,?4,'pending',0,datetime('now'))",
            params![Uuid::new_v4().to_string(), platform, account_id, url]);
        if r.is_ok() { mbudget -= 1; }
    }
}

/// Engage 自动获客的开关 + 节奏。
#[tauri::command]
pub(crate) fn engage_get_settings(state: State<AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let g = |k: &str| engine_cfg_get(&conn, k);
    let inflight: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE task_type IN ('engage','reply','reply_keyword') AND status IN ('pending','running')",
        [], |r| r.get(0)).unwrap_or(0);
    let kw_enabled: i64 = conn.query_row("SELECT COUNT(*) FROM keywords WHERE enabled=1", [], |r| r.get(0)).unwrap_or(0);
    Ok(serde_json::json!({
        "auto": g("engage_auto").as_deref() != Some("0"),
        "interval_minutes": g("engage_interval_secs").and_then(|s| s.parse::<i64>().ok()).unwrap_or(1800) / 60,
        "max_inflight": g("engage_max_inflight").and_then(|s| s.parse::<i64>().ok()).unwrap_or(6),
        "reply_mode": engine_reply_mode(&conn),
        "inflight": inflight,
        "keywords_enabled": kw_enabled,
        "last_tick": g("engage_last_tick"),
    }))
}

#[tauri::command]
pub(crate) fn engage_set_auto(state: State<AppState>, on: bool, interval_minutes: Option<i64>, max_inflight: Option<i64>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    engine_cfg_set(&conn, "engage_auto", if on { "1" } else { "0" });
    if let Some(m) = interval_minutes { engine_cfg_set(&conn, "engage_interval_secs", &(m.max(5) * 60).to_string()); }
    if let Some(c) = max_inflight { engine_cfg_set(&conn, "engage_max_inflight", &c.clamp(1, 30).to_string()); }
    // 改了开关 → 清掉节流时间戳，让引擎下一拍立刻评估
    engine_cfg_set(&conn, "engage_last_tick", "");
    Ok(())
}
