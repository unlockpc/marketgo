//! 机场订阅域：拉取 Clash 订阅 → 节点入池 → 为各身份配对/改派节点 → 重建并热重载 mihomo 内核。
//! 依赖 lib.rs 中的 mihomo 内核子系统与共享原语（见 `use crate::{...}`）。

use tauri::{AppHandle, State, Manager, Emitter};
use rusqlite::params;

use crate::{AppState, get_http_client, engine_cfg_get, engine_cfg_set};
use crate::multi_account::{
    mihomo_sub_path, is_junk_node_name, node_region, regenerate_mihomo_config,
    mihomo_ensure_running, mihomo_reload, MIHOMO_API_PORT,
};

/// 设置/刷新机场订阅：拉取 → 解析节点 → 入池 → 重建配置 → 启动并热重载内核。
#[tauri::command]
pub(crate) async fn airport_set_subscription(app: AppHandle, url: String) -> Result<String, String> {
    let url = url.trim().to_string();
    if !url.starts_with("http") { return Err("请输入有效的订阅链接（http/https 开头）".into()); }
    // 手动设置：force_reload=true（无论是否有改动都重建内核配置并重载）
    let (count, repaired) = airport_refresh(&app, url, true).await?;
    let tail = if repaired > 0 { format!("，已为 {} 个身份重新配对节点", repaired) } else { String::new() };
    Ok(format!("订阅已更新：{} 个有效节点入池，内核已就绪{}", count, tail))
}

/// 拉取/解析机场订阅 → 节点入池 → 把「节点已失效」的身份改派同地区相似节点 → 按需重建配置并热重载。
/// 返回 (有效节点数, 改派身份数)。
/// - force_reload=true：手动设置订阅时用，无论是否有改动都重建配置+重载。
/// - force_reload=false：定时刷新用，只有「换了订阅 / 有身份的节点失效被改派」时才重载，避免无谓中断连接。
/// 「相似节点」= 同 region 优先（保持出口地区不变），没有再退而取任意空闲节点。
async fn airport_refresh(app: &AppHandle, url: String, force_reload: bool) -> Result<(usize, usize), String> {
    let url = url.trim().to_string();
    if !url.starts_with("http") { return Err("无效订阅链接".into()); }
    // 拉订阅（Clash YAML）
    let client = get_http_client();
    let resp = client.get(&url).header("User-Agent", "clash-verge/v1.7.7").send().await
        .map_err(|e| format!("拉订阅失败: {}", e))?;
    if !resp.status().is_success() { return Err(format!("拉订阅 HTTP {}", resp.status())); }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&text)
        .map_err(|_| "订阅不是 Clash 配置格式（需要 Clash 订阅链接，不是 ss/vmess 那种）".to_string())?;
    let proxies = doc.get("proxies").and_then(|p| p.as_sequence())
        .ok_or("订阅里没有 proxies 节点")?.clone();
    if proxies.is_empty() { return Err("订阅里节点为空".into()); }
    std::fs::write(mihomo_sub_path(), &text).map_err(|e| e.to_string())?;

    // 入池：只收真实节点，过滤掉机场的信息展示项（剩余流量/套餐到期/官网等）
    let names: Vec<(String,String)> = proxies.iter().filter_map(|p| {
        let n = p.get("name").and_then(|x| x.as_str())?.to_string();
        let t = p.get("type").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if is_junk_node_name(&n, &t) { return None; }
        Some((n, t))
    }).collect();
    if names.is_empty() { return Err("订阅里没有可用节点（全是信息展示项？请确认是 Clash 订阅）".into()); }
    let count = names.len();
    let valid: std::collections::HashSet<String> = names.iter().map(|(n,_)| n.clone()).collect();
    let mut repaired = 0usize;
    let changed;
    {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;

        // 是否换了订阅：和上次保存的订阅链接对比
        let prev_url = engine_cfg_get(&conn, "airport_sub_url").unwrap_or_default();
        let sub_changed = prev_url.trim() != url;
        engine_cfg_set(&conn, "airport_sub_url", &url);

        // 入池：upsert 本次订阅的有效节点
        for (name, typ) in &names {
            let region = node_region(name);
            let _ = conn.execute(
                "INSERT INTO nodes (name, region, type, in_use, last_seen) VALUES (?1,?2,?3,0,datetime('now')) \
                 ON CONFLICT(name) DO UPDATE SET region=?2, type=?3, last_seen=datetime('now')",
                params![name, region, typ]);
        }

        // 决定要重配的身份：
        //   换了订阅 → 全部身份重配一遍（旧节点名多半已失效）
        //   同一家订阅 → 只兜底处理节点恰好消失的身份（#11 定时刷新的核心：哪个身份的节点没了就替）
        let personas: Vec<(String, String)> = {
            let mut s = conn.prepare("SELECT id, node_name FROM personas WHERE node_name IS NOT NULL AND node_name<>''").map_err(|e| e.to_string())?;
            let rows: Vec<(String,String)> = s.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?))).map_err(|e| e.to_string())?
                .flatten().collect();
            rows
        };
        let targets: Vec<(String, String)> = personas.into_iter()
            .filter(|(_, n)| sub_changed || !valid.contains(n))
            .collect();

        for (pid, old_node) in &targets {
            // 同地区优先：尽量让身份的出口地区保持不变（美国身份仍派美国节点）
            let want_region = node_region(old_node);
            let pick = conn.query_row(
                "SELECT name FROM nodes WHERE in_use=0 AND region=?1 ORDER BY name LIMIT 1",
                params![want_region], |r| r.get::<_,String>(0))
                .or_else(|_| conn.query_row(
                    "SELECT name FROM nodes WHERE in_use=0 ORDER BY name LIMIT 1", [], |r| r.get::<_,String>(0)));
            if let Ok(new_node) = pick {
                if new_node == *old_node { continue; } // 同一节点仍有效，无需替换
                let _ = conn.execute("UPDATE nodes SET in_use=1 WHERE name=?1", params![new_node]);
                let _ = conn.execute("UPDATE personas SET node_name=?1 WHERE id=?2", params![new_node, pid]);
                // 释放旧节点占用；若已不在新订阅里则一并清掉（幽灵节点）
                if valid.contains(old_node) {
                    let _ = conn.execute("UPDATE nodes SET in_use=0 WHERE name=?1", params![old_node]);
                } else {
                    let _ = conn.execute("DELETE FROM nodes WHERE name=?1", params![old_node]);
                }
                repaired += 1;
            }
        }

        // 清理：删掉本次订阅里已不存在、且没被身份占用的旧节点
        if let Ok(mut stmt) = conn.prepare("SELECT name FROM nodes WHERE in_use=0") {
            let stale: Vec<String> = stmt.query_map([], |r| r.get::<_,String>(0)).ok()
                .map(|it| it.flatten().filter(|n| !valid.contains(n)).collect()).unwrap_or_default();
            for n in stale { let _ = conn.execute("DELETE FROM nodes WHERE name=?1", params![n]); }
        }

        changed = sub_changed || repaired > 0;
        // 只有真的改了配对，或强制（手动设置）时才重建配置，避免定时刷新无谓地热重载
        if force_reload || changed {
            regenerate_mihomo_config(&conn)?;
        }
    }
    if force_reload || changed {
        mihomo_ensure_running(app).await?;
        mihomo_reload().await?;
    }
    Ok((count, repaired))
}

/// #11 后台定时刷新机场订阅（默认 10 分钟一次）：自动替换失效节点，保证各身份出口 IP 不中断。
pub(crate) async fn airport_refresh_loop(app: AppHandle) {
    // 启动后稍等，让 mihomo_boot 先就绪，避免和启动重建撞车
    tokio::time::sleep(std::time::Duration::from_secs(90)).await;
    loop {
        let url = {
            let state = app.state::<AppState>();
            state.db.lock().ok().and_then(|c| engine_cfg_get(&c, "airport_sub_url"))
        };
        if let Some(url) = url {
            if url.trim().starts_with("http") {
                match airport_refresh(&app, url, false).await {
                    Ok((count, repaired)) => {
                        if repaired > 0 {
                            log::info!("[AIRPORT] 定时刷新：{} 个有效节点，已为 {} 个身份替换失效节点", count, repaired);
                            // 通知前端：弹个 toast + 刷新账号页
                            let _ = app.emit("airport-nodes-replaced", serde_json::json!({"count": count, "repaired": repaired}));
                        } else {
                            log::info!("[AIRPORT] 定时刷新：{} 个有效节点，节点无变化", count);
                        }
                    }
                    Err(e) => log::warn!("[AIRPORT] 定时刷新失败: {}", e),
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(600)).await; // 10 分钟
    }
}

/// 返回当前已保存的机场订阅链接（供「设置订阅」弹框预填）。没有则返回空串。
#[tauri::command]
pub(crate) fn airport_get_subscription(state: State<AppState>) -> Result<String, String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    Ok(engine_cfg_get(&conn, "airport_sub_url").unwrap_or_default())
}

/// 「刷新订阅」：用已保存的订阅 URL 重新拉取，逻辑同定时刷新（只替换失效节点，不强制重载）。
#[tauri::command]
pub(crate) async fn airport_refresh_subscription(app: AppHandle) -> Result<String, String> {
    let url = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        engine_cfg_get(&conn, "airport_sub_url").unwrap_or_default()
    };
    if !url.trim().starts_with("http") { return Err("还没设置机场订阅，请先点「设置订阅」".into()); }
    let (count, repaired) = airport_refresh(&app, url, false).await?;
    if repaired > 0 {
        Ok(format!("已刷新：{} 个有效节点，替换了 {} 个身份的失效节点", count, repaired))
    } else {
        Ok(format!("已刷新：{} 个有效节点，节点无变化", count))
    }
}

#[tauri::command]
pub(crate) fn airport_status(state: State<AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0)).unwrap_or(0);
    let in_use: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE in_use=1", [], |r| r.get(0)).unwrap_or(0);
    let url = engine_cfg_get(&conn, "airport_sub_url").unwrap_or_default();
    let mut by_region: Vec<(String,i64)> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT region, COUNT(*) FROM nodes GROUP BY region ORDER BY COUNT(*) DESC") {
        if let Ok(it) = stmt.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get::<_,i64>(1)?))) {
            by_region = it.flatten().collect();
        }
    }
    Ok(serde_json::json!({
        "configured": !url.is_empty(),
        "total": total, "in_use": in_use, "free": total - in_use,
        "by_region": by_region,
        "kernel_port": MIHOMO_API_PORT,
    }))
}
