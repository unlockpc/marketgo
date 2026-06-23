//! 代理池域：代理列表/新增/测试/删除（用户自填的出口代理，与机场 mihomo 分开）。

use serde::{Serialize, Deserialize};
use tauri::State;
use crate::*;

// ===== Proxy Management =====

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ProxyInfo {
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
pub(crate) struct ProxyListResponse {
    proxies: Vec<ProxyInfo>,
    stats: ProxyStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ProxyStats {
    total: i32,
    active: i32,
    in_use: i32,
    failed: i32,
}

#[tauri::command]
pub(crate) fn list_proxies(state: State<AppState>) -> Result<ProxyListResponse, String> {
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
pub(crate) fn add_proxy(
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
pub(crate) async fn test_proxy(state: State<'_, AppState>, id: String) -> Result<serde_json::Value, String> {
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
pub(crate) fn delete_proxy(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM proxies WHERE id = ?", params![id])
        .map_err(|e| e.to_string())?;

    log::info!("Deleted proxy: {}", id);
    Ok(())
}
