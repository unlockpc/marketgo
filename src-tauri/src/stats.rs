//! 效果分析域：仪表盘统计 / 概览统计 / 详细统计 / 营销漏斗统计。
//! 四个命令(get_dashboard_stats / get_stats / get_detailed_stats / get_marketing_stats)
//! 原散落在 lib.rs 三处,均只读 publish_history/discovered_posts/leads/reply_history 等表聚合。

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use tauri::State;
use rusqlite::params;
use chrono::Utc;
use crate::*;

// ===== 概览 / 详细统计的返回结构 =====

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

// ===== 仪表盘统计 =====

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DashboardStats {
    active_tasks: i32,
    today_posts: i32,
    account_health: i32,
    success_rate: i32,
    campaigns: Vec<DashboardCampaign>,
    recent_activity: Vec<DashboardActivity>,
    platform_health: HashMap<String, i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DashboardCampaign {
    id: String,
    name: String,
    platforms: Vec<String>,
    status: String,
    progress: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DashboardActivity {
    time: String,
    status: String,
    message: String,
    platform: Option<String>,
}

#[tauri::command]
pub(crate) fn get_dashboard_stats(state: State<AppState>) -> Result<DashboardStats, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let now = Utc::now();
    let _today = now.format("%Y-%m-%d").to_string();

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

#[tauri::command]
pub(crate) fn get_stats(state: State<AppState>, _days: i32) -> Result<Stats, String> {
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
pub(crate) fn get_detailed_stats(state: State<AppState>, days: i32) -> Result<DetailedStats, String> {
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

// ============ P1-6 效果分析(营销漏斗) ============

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
pub(crate) fn get_marketing_stats(state: State<'_, AppState>) -> Result<MarketingStats, String> {
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
