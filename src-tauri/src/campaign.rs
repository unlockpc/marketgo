//! Campaign 营销活动域：活动列表/创建/启停/删除 + 按平台×类型×关键词铺排发帖任务。

use serde::{Serialize, Deserialize};
use tauri::State;
use rusqlite::{Connection, params};
use crate::*;

// ===== Campaign Management =====

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CampaignListResponse {
    campaigns: Vec<CampaignInfo>,
    stats: CampaignStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CampaignInfo {
    id: String,
    name: String,
    product_id: String,
    product_name: String,
    platforms: Vec<String>,
    status: String,
    schedule_type: String,
    total_tasks: i32,
    completed_tasks: i32,
    started_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CampaignStats {
    active: i32,
    scheduled: i32,
    completed: i32,
    total_tasks: i32,
}

#[tauri::command]
pub(crate) fn list_campaigns(state: State<AppState>) -> Result<CampaignListResponse, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Get campaigns with product names
    let mut campaigns = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.name, c.product_id, COALESCE(p.name, 'Unknown Product') as product_name,
                    c.platforms, c.status, c.schedule_type, c.total_tasks, c.completed_tasks,
                    c.started_at, c.created_at
             FROM campaigns c
             LEFT JOIN products p ON c.product_id = p.id
             ORDER BY c.created_at DESC"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |row| {
            let platforms_str: String = row.get(4)?;
            let platforms: Vec<String> = serde_json::from_str(&platforms_str).unwrap_or_default();

            Ok(CampaignInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                product_id: row.get(2)?,
                product_name: row.get(3)?,
                platforms,
                status: row.get(5)?,
                schedule_type: row.get(6)?,
                total_tasks: row.get(7)?,
                completed_tasks: row.get(8)?,
                started_at: row.get(9)?,
                created_at: row.get(10)?,
            })
        }).map_err(|e| e.to_string())?;

        for row in rows {
            if let Ok(campaign) = row {
                campaigns.push(campaign);
            }
        }
    }

    // Calculate stats
    let active: i32 = conn.query_row(
        "SELECT COUNT(*) FROM campaigns WHERE status = 'running'",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let scheduled: i32 = conn.query_row(
        "SELECT COUNT(*) FROM campaigns WHERE status = 'scheduled'",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let completed: i32 = conn.query_row(
        "SELECT COUNT(*) FROM campaigns WHERE status = 'completed'",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    let total_tasks: i32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks",
        [],
        |row| row.get(0)
    ).unwrap_or(0);

    Ok(CampaignListResponse {
        campaigns,
        stats: CampaignStats {
            active,
            scheduled,
            completed,
            total_tasks,
        },
    })
}

#[tauri::command]
pub(crate) fn create_campaign(
    state: State<AppState>,
    name: String,
    product_id: String,
    description: Option<String>,
    platforms: Vec<String>,
    post_types: Vec<String>,
    languages: Vec<String>,
    keywords: Vec<String>,
    schedule_type: String,
    start_time: Option<String>,
    posts_per_day: i32,
    duration: i32,
    start_immediately: bool,
) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();

    // Create schedule config JSON
    let schedule_config = serde_json::json!({
        "post_types": post_types,
        "languages": languages,
        "keywords": keywords,
        "posts_per_day": posts_per_day,
        "duration_days": duration,
        "start_time": start_time,
        "description": description,
    });

    let status = if start_immediately { "running" } else { "draft" };
    let started_at = if start_immediately {
        Some(Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
    } else {
        None
    };

    let total_tasks = posts_per_day * duration * platforms.len() as i32;

    conn.execute(
        "INSERT INTO campaigns (id, name, product_id, platforms, status, schedule_type, schedule_config, total_tasks, started_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))",
        params![
            id,
            name,
            product_id,
            serde_json::to_string(&platforms).unwrap_or_default(),
            status,
            schedule_type,
            schedule_config.to_string(),
            total_tasks,
            started_at,
        ],
    ).map_err(|e| e.to_string())?;

    // If starting immediately, create initial tasks
    if start_immediately {
        create_campaign_tasks(&conn, &id, &platforms, &post_types, &keywords, posts_per_day)?;
    }

    log::info!("Created campaign: {} ({})", name, id);
    Ok(id)
}

fn create_campaign_tasks(
    conn: &Connection,
    campaign_id: &str,
    platforms: &[String],
    post_types: &[String],
    keywords: &[String],
    posts_per_day: i32,
) -> Result<(), String> {
    // Create tasks for today
    for platform in platforms {
        for _ in 0..posts_per_day {
            let task_id = uuid::Uuid::new_v4().to_string();
            let task_type = post_types.first().map(|s| s.as_str()).unwrap_or("article");
            let keyword = keywords.first().map(|s| s.as_str()).unwrap_or("");

            conn.execute(
                "INSERT INTO tasks (id, campaign_id, task_type, platform, content, status, scheduled_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pending', datetime('now', '+' || (ABS(RANDOM()) % 60) || ' minutes'), datetime('now'))",
                params![task_id, campaign_id, task_type, platform, keyword],
            ).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[tauri::command]
pub(crate) fn start_campaign(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Get campaign details
    let (platforms_str, schedule_config_str): (String, String) = conn.query_row(
        "SELECT platforms, COALESCE(schedule_config, '{}') FROM campaigns WHERE id = ?",
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?))
    ).map_err(|e| e.to_string())?;

    let platforms: Vec<String> = serde_json::from_str(&platforms_str).unwrap_or_default();
    let config: serde_json::Value = serde_json::from_str(&schedule_config_str).unwrap_or_default();

    let post_types: Vec<String> = config["post_types"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["article".to_string()]);
    let keywords: Vec<String> = config["keywords"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let posts_per_day = config["posts_per_day"].as_i64().unwrap_or(3) as i32;

    // Update status
    conn.execute(
        "UPDATE campaigns SET status = 'running', started_at = datetime('now') WHERE id = ?",
        params![id],
    ).map_err(|e| e.to_string())?;

    // Create tasks if none exist
    let task_count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE campaign_id = ?",
        params![id],
        |row| row.get(0)
    ).unwrap_or(0);

    if task_count == 0 {
        create_campaign_tasks(&conn, &id, &platforms, &post_types, &keywords, posts_per_day)?;
    }

    log::info!("Started campaign: {}", id);
    Ok(())
}

#[tauri::command]
pub(crate) fn pause_campaign(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE campaigns SET status = 'paused' WHERE id = ?",
        params![id],
    ).map_err(|e| e.to_string())?;

    log::info!("Paused campaign: {}", id);
    Ok(())
}

#[tauri::command]
pub(crate) fn delete_campaign(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Delete tasks first
    conn.execute("DELETE FROM tasks WHERE campaign_id = ?", params![id])
        .map_err(|e| e.to_string())?;

    // Delete campaign
    conn.execute("DELETE FROM campaigns WHERE id = ?", params![id])
        .map_err(|e| e.to_string())?;

    log::info!("Deleted campaign: {}", id);
    Ok(())
}
