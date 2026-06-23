//! 互动结果处理域：待审回复审核(批准发布/驳回) + 转化闭环轻 CRM(线索列表/状态更新)。

use serde::Serialize;
use tauri::State;
use rusqlite::{Connection, params};
use crate::*;

// ===== 待审回复审核 =====

#[derive(Debug, Serialize)]
pub(crate) struct PendingReply {
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
pub(crate) fn list_pending_replies(state: State<'_, AppState>) -> Result<Vec<PendingReply>, String> {
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
pub(crate) fn approve_reply(state: State<'_, AppState>, id: String, edited_content: Option<String>) -> Result<ReplyResult, String> {
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
pub(crate) fn reject_reply(state: State<'_, AppState>, id: String) -> Result<bool, String> {
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

// ===== P1-5 转化闭环 / 轻 CRM =====
#[derive(Debug, Serialize)]
pub(crate) struct Lead {
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
pub(crate) fn list_leads(state: State<'_, AppState>, status: Option<String>) -> Result<Vec<Lead>, String> {
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
pub(crate) fn update_lead_status(state: State<'_, AppState>, id: String, status: String, notes: Option<String>) -> Result<bool, String> {
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
