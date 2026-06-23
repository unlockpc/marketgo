//! 成效追踪域（搜索排名 + 品牌提及 + 可选 Trends）。
//! 全程走 Unzoo 的专用 auto/Default profile（与发帖 profile 隔离 → 排名最干净、最可比）。
//! 实测验证过：SERP 解析 JS 可定位域名排名；品牌词 Trends 对小品牌返回"无足够数据"。

use serde::Serialize;
use tauri::{AppHandle, State, Manager};
use rusqlite::{Connection, params};
use uuid::Uuid;
use chrono::Utc;

use crate::{
    AppState, get_blocking_client, UNZOO_API_BASE, engine_cfg_get, engine_cfg_set,
    human_type_delay_ms, parse_dt, ensure_browser_connected,
};

const METRICS_PROFILE_NAME: &str = "um-metrics"; // 专用采集 profile（干净/不登录/不绑代理，与身份隔离的 auto 分开）
const METRICS_GL: &str = "us";
const METRICS_HL: &str = "zh-CN";
const METRICS_REGION: &str = "us/zh-CN";

struct KwRow { keyword: String, kind: String, domain: String }

/// 解析专用采集 profile 的完整路径：按 name=="um-metrics" 找；找不到就建一个**干净**的（不登录/不绑代理）。
/// 关键：绝不回退到 auto/Default——那个现在可能是某个登录态身份，会污染排名采集。
/// 也：/tabs/create 的 profile_id 不会真正切 profile，必须 /profiles/launch + profile_path。
fn metrics_resolve_profile_path() -> Result<String, String> {
    let client = get_blocking_client();
    let resp = client.get(&format!("{}/profiles", UNZOO_API_BASE))
        .send().map_err(|e| format!("列出 profiles 失败: {}", e))?;
    let v: serde_json::Value = resp.json().unwrap_or_default();
    let arr = v.get("data").and_then(|d| d.get("profiles"))
        .or_else(|| v.get("profiles"))
        .and_then(|x| x.as_array()).cloned().unwrap_or_default();
    // 按文件夹 Profile_um-metrics 匹配（Unzoo 给程序建的 profile 显示名是默认"用户N"，不能按显示名匹配）
    let want_folder = format!("Profile_{}", METRICS_PROFILE_NAME);
    for p in &arr {
        let name = p.get("name").and_then(|n| n.as_str());
        let path = p.get("path").and_then(|x| x.as_str()).unwrap_or("");
        let norm = path.replace('/', "\\");
        let folder = norm.rsplit('\\').next().unwrap_or("");
        if name == Some(METRICS_PROFILE_NAME) || folder == want_folder {
            return Ok(path.to_string());
        }
    }
    // 不存在 → 现建一个干净的专用采集 profile（不登录、不绑代理）
    let resp = client.post(&format!("{}/profiles/create", UNZOO_API_BASE))
        .json(&serde_json::json!({"name": METRICS_PROFILE_NAME, "group": "metrics", "tags": ["unmarket-metrics"]}))
        .send().map_err(|e| format!("建采集 profile 失败: {}", e))?;
    if !resp.status().is_success() { return Err(format!("建采集 profile 失败: HTTP {}", resp.status())); }
    let data: serde_json::Value = resp.json().unwrap_or_default();
    let path = data.get("data").and_then(|d| d.get("path")).and_then(|p| p.as_str())
        .or_else(|| data.get("path").and_then(|p| p.as_str()))
        .ok_or("建采集 profile 成功但无 path")?;
    Ok(path.to_string())
}

/// 启动专用采集 profile（auto，独立窗口，与发帖 profile 完全隔离）并返回其窗口里的一个标签页 id。
fn metrics_ensure_tab() -> Result<String, String> {
    let path = metrics_resolve_profile_path()?;
    let client = get_blocking_client();
    let resp = client.post(&format!("{}/profiles/launch", UNZOO_API_BASE))
        .json(&serde_json::json!({"profile_path": path}))
        .send().map_err(|e| format!("启动采集 profile 失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("启动采集 profile 失败: HTTP {}", resp.status()));
    }
    let data: serde_json::Value = resp.json().unwrap_or_default();
    let tid = data.get("data").and_then(|d| d.get("tab_id")).map(|t| {
        if let Some(n) = t.as_i64() { n.to_string() }
        else if let Some(s) = t.as_str() { s.to_string() }
        else { String::new() }
    }).unwrap_or_default();
    if tid.is_empty() { return Err("采集 profile 启动后无 tab_id".into()); }
    Ok(tid)
}

pub(crate) fn metrics_navigate(tab_id: &str, url: &str) -> Result<(), String> {
    let client = get_blocking_client();
    let resp = client.post(&format!("{}/navigate", UNZOO_API_BASE))
        .json(&serde_json::json!({"tab_id": tab_id, "url": url}))
        .send().map_err(|e| format!("采集导航失败: {}", e))?;
    if resp.status().is_success() { Ok(()) } else { Err(format!("采集导航失败: HTTP {}", resp.status())) }
}

pub(crate) fn metrics_evaluate(tab_id: &str, expr: &str) -> Result<String, String> {
    let client = get_blocking_client();
    let resp = client.post(&format!("{}/evaluate", UNZOO_API_BASE))
        .json(&serde_json::json!({"tab_id": tab_id, "expression": expr}))
        .send().map_err(|e| format!("采集求值失败: {}", e))?;
    if !resp.status().is_success() { return Err(format!("采集求值失败: HTTP {}", resp.status())); }
    let v: serde_json::Value = resp.json().unwrap_or_default();
    let r = v.get("data").and_then(|d| d.get("result"));
    Ok(match r {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    })
}

fn metrics_close_tab(tab_id: &str) {
    let client = get_blocking_client();
    let _ = client.post(&format!("{}/tabs/close", UNZOO_API_BASE))
        .json(&serde_json::json!({"tab_id": tab_id})).send();
}

/// 把 /evaluate 的返回稳健地解析为 JSON（可能是裸 JSON 串，也可能被再包一层 String）。
fn metrics_parse(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(raw)
        .or_else(|_| serde_json::from_str::<String>(raw).and_then(|s| serde_json::from_str::<serde_json::Value>(&s)))
        .unwrap_or_else(|_| serde_json::json!({}))
}

/// SERP 解析 JS：返回目标域名在自然结果里的排名（未进前 N 则 rank=null）+ 前 5 名。
fn metrics_serp_js(domain: &str) -> String {
    const TPL: &str = r#"(function(){
  var anchors=Array.prototype.slice.call(document.querySelectorAll('a')).filter(function(a){return a.querySelector('h3');});
  var seen={},out=[],pos=0;
  anchors.forEach(function(a){
    var href=a.href; if(!href||href.indexOf('https://www.google.')===0||href.indexOf('https://webcache')===0) return;
    var host; try{host=new URL(href).hostname.replace(/^www\./,'');}catch(e){return;}
    if(seen[href])return; seen[href]=1; pos++;
    out.push({pos:pos,host:host,title:(a.querySelector('h3').innerText||'').slice(0,60)});
  });
  var hit=null; for(var i=0;i<out.length;i++){ if(out[i].host.indexOf('__DOMAIN__')>=0){hit=out[i];break;} }
  return JSON.stringify({rank:hit?hit.pos:null,total:out.length,top:out.slice(0,5)});
})()"#;
    TPL.replace("__DOMAIN__", domain)
}

/// 品牌提及 JS：统计排除自家域名后的第三方独立域名数（精确匹配查询时噪声最低）。
fn metrics_mention_js(domain: &str) -> String {
    const TPL: &str = r#"(function(){
  var anchors=Array.prototype.slice.call(document.querySelectorAll('a')).filter(function(a){return a.querySelector('h3');});
  var seen={},hosts={};
  anchors.forEach(function(a){
    var href=a.href; if(!href||href.indexOf('https://www.google.')===0||href.indexOf('https://webcache')===0) return;
    var host; try{host=new URL(href).hostname.replace(/^www\./,'');}catch(e){return;}
    if(host.indexOf('__DOMAIN__')>=0)return; if(seen[href])return; seen[href]=1;
    hosts[host]=(hosts[host]||0)+1;
  });
  var keys=Object.keys(hosts);
  return JSON.stringify({domains:keys.length,total:keys.reduce(function(s,k){return s+hosts[k];},0),hosts:hosts});
})()"#;
    TPL.replace("__DOMAIN__", domain)
}

fn metrics_search_url(query: &str) -> Result<String, String> {
    reqwest::Url::parse_with_params(
        "https://www.google.com/search",
        &[("q", query), ("num", "30"), ("hl", METRICS_HL), ("gl", METRICS_GL), ("pws", "0")],
    ).map(|u| u.to_string()).map_err(|e| e.to_string())
}

/// 是否被 Google 拦截（/sorry/ 异常流量 CAPTCHA）。被拦时不能把结果当"未进前30"记，否则是假数据。
fn metrics_blocked(tab_id: &str) -> bool {
    let raw = metrics_evaluate(tab_id, "location.href").unwrap_or_default();
    let href = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    href.contains("/sorry")
}

/// 采一个词的搜索排名。返回 (排名位次或 None, detail JSON)。被 Google 限流时返回 Err("BLOCKED")。
fn metrics_collect_serp(tab_id: &str, keyword: &str, domain: &str) -> Result<(Option<i64>, String), String> {
    let url = metrics_search_url(keyword)?;
    metrics_navigate(tab_id, &url)?;
    std::thread::sleep(std::time::Duration::from_millis(2500));
    if metrics_blocked(tab_id) { return Err("BLOCKED: Google 限流(CAPTCHA)".into()); }
    let raw = metrics_evaluate(tab_id, &metrics_serp_js(domain))?;
    let v = metrics_parse(&raw);
    // 零自然结果 = 软限流/异常页（正常搜索一定有结果）→ 当作被拦，不记假"未进前30"
    if v.get("total").and_then(|x| x.as_i64()).unwrap_or(0) == 0 {
        return Err("BLOCKED: 0 结果(疑似软限流)".into());
    }
    let rank = v.get("rank").and_then(|x| x.as_i64());
    Ok((rank, v.to_string()))
}

/// 采一个词的品牌提及（精确匹配 + 排除自家域名）。返回 (第三方域名数, detail JSON)。被限流时 Err("BLOCKED")。
fn metrics_collect_mention(tab_id: &str, keyword: &str, domain: &str) -> Result<(i64, String), String> {
    let q = format!("\"{}\" -site:{}", keyword, domain);
    let url = metrics_search_url(&q)?;
    metrics_navigate(tab_id, &url)?;
    std::thread::sleep(std::time::Duration::from_millis(2500));
    if metrics_blocked(tab_id) { return Err("BLOCKED: Google 限流(CAPTCHA)".into()); }
    let raw = metrics_evaluate(tab_id, &metrics_mention_js(domain))?;
    let v = metrics_parse(&raw);
    let domains = v.get("domains").and_then(|x| x.as_i64()).unwrap_or(0);
    Ok((domains, v.to_string()))
}

/// 采一个品牌词的 Trends 状态（默认关）。小品牌通常返回"无足够数据" → value=None。
fn metrics_collect_trends(tab_id: &str, term: &str) -> Result<(Option<i64>, String), String> {
    let url = reqwest::Url::parse_with_params(
        "https://trends.google.com/trends/explore",
        &[("date", "today 12-m"), ("q", term), ("hl", METRICS_HL)],
    ).map(|u| u.to_string()).map_err(|e| e.to_string())?;
    metrics_navigate(tab_id, &url)?;
    std::thread::sleep(std::time::Duration::from_millis(4500));
    let js = r#"(function(){var b=(document.body&&document.body.innerText)||'';var no=b.indexOf('没有足够')>=0||b.indexOf("enough data")>=0||b.indexOf('not enough')>=0;return JSON.stringify({no_data:no});})()"#;
    let raw = metrics_evaluate(tab_id, js)?;
    let v = metrics_parse(&raw);
    let no_data = v.get("no_data").and_then(|x| x.as_bool()).unwrap_or(true);
    Ok((if no_data { None } else { Some(1) }, v.to_string()))
}

/// 跑一整轮采集：读启用关键词 → 逐词走 Unzoo 采 → 每条独立写库。网络 IO 在 spawn_blocking 里跑，绝不持 db 锁。
pub(crate) fn metrics_collect_all(app: &AppHandle) -> Result<(usize, usize), String> {
    let rows: Vec<KwRow> = {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        let conn = match guard { Ok(c) => c, Err(_) => return Err("db lock".into()) };
        let mut stmt = conn.prepare(
            "SELECT keyword, kind, COALESCE(target_domain,'doaipm.com') FROM metric_keywords WHERE enabled=1 ORDER BY kind, keyword"
        ).map_err(|e| e.to_string())?;
        let it = stmt.query_map([], |r| Ok(KwRow{keyword:r.get(0)?, kind:r.get(1)?, domain:r.get(2)?}))
            .map_err(|e| e.to_string())?;
        it.flatten().collect()
    };
    if rows.is_empty() { return Ok((0, 0)); }

    let trends_on = {
        let state = app.state::<AppState>();
        let g = state.db.lock();
        g.ok().and_then(|c| engine_cfg_get(&c, "metrics_trends_on")) .as_deref() == Some("1")
    };

    let tab_id = metrics_ensure_tab()?;
    let mut serp_done = 0usize;
    let mut mention_done = 0usize;

    let mut blocked = false;
    for kw in &rows {
        let collected: Result<(&str, Option<i64>, String), String> = match kw.kind.as_str() {
            "mention" => metrics_collect_mention(&tab_id, &kw.keyword, &kw.domain).map(|(d, det)| ("mention", Some(d), det)),
            _ => metrics_collect_serp(&tab_id, &kw.keyword, &kw.domain).map(|(rank, det)| ("serp", rank, det)),
        };
        let (source, value, detail) = match collected {
            Ok(v) => v,
            // 被 Google 限流：立刻停，绝不把后续记成"未进前30"假数据
            Err(e) if e.starts_with("BLOCKED") => {
                log::warn!("[METRICS] {}，本轮提前结束（已采 serp {} / mention {}）", e, serp_done, mention_done);
                blocked = true; break;
            }
            Err(e) => { log::warn!("[METRICS] {} 采集失败: {}", kw.keyword, e); continue; }
        };
        if source == "serp" { serp_done += 1; } else { mention_done += 1; }
        {
            let state = app.state::<AppState>();
            let guard = state.db.lock();
            if let Ok(conn) = guard {
                let _ = conn.execute(
                    "INSERT INTO metrics (keyword, kind, source, region, value, detail) VALUES (?1,?2,?3,?4,?5,?6)",
                    params![kw.keyword, kw.kind, source, METRICS_REGION, value, detail]);
            }
        }
        // 拟人限速，别把 Google 打急了
        std::thread::sleep(std::time::Duration::from_millis(1500 + (human_type_delay_ms() as u64) * 8));
    }

    if trends_on && !blocked {
        let brand_terms: Vec<String> = rows.iter().filter(|k| k.kind == "brand").map(|k| k.keyword.clone()).collect();
        for term in brand_terms {
            if let Ok((val, det)) = metrics_collect_trends(&tab_id, &term) {
                let state = app.state::<AppState>();
                let guard = state.db.lock();
                if let Ok(conn) = guard {
                    let _ = conn.execute(
                        "INSERT INTO metrics (keyword, kind, source, region, value, detail) VALUES (?1,'brand','trends',?2,?3,?4)",
                        params![term, METRICS_REGION, val, det]);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2000));
        }
    }

    metrics_close_tab(&tab_id);
    // 整轮一开头就被限流、一条没采到 → 明确报错（引擎会择机重试），不要假装"采集完成"
    if blocked && serp_done == 0 && mention_done == 0 {
        return Err("Google 临时限流(CAPTCHA)，本轮未采到数据；通常是短时间查询过多，稍后自动重试即可".into());
    }
    log::info!("[METRICS] 采集{}：排名 {} 词，提及 {} 词", if blocked {"中断(限流)"} else {"完成"}, serp_done, mention_done);
    Ok((serp_done, mention_done))
}

/// 成效采集调度：每天一次入队 metrics_collect 任务（实际采集在 engine_execute 里跑，避免持 db 锁做网络 IO）。
pub(crate) fn metrics_collect_tick(conn: &Connection) {
    if engine_cfg_get(conn, "metrics_enabled").as_deref() == Some("0") { return; }
    let now = Utc::now();
    let interval = engine_cfg_get(conn, "metrics_interval_secs").and_then(|s| s.parse::<i64>().ok()).unwrap_or(86400);
    if let Some(last) = engine_cfg_get(conn, "metrics_last_tick").and_then(|s| parse_dt(&s)) {
        if (now - last).num_seconds() < interval { return; }
    }
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE task_type='metrics_collect' AND status IN ('pending','running')",
        [], |r| r.get(0)).unwrap_or(0);
    engine_cfg_set(conn, "metrics_last_tick", &now.to_rfc3339());
    if pending > 0 { return; }
    let task_id = Uuid::new_v4().to_string();
    let _ = conn.execute(
        "INSERT INTO tasks (id, task_type, status, retry_count, created_at) VALUES (?1,'metrics_collect','pending',0,datetime('now'))",
        params![task_id]);
    log::info!("[METRICS] 入队采集任务 {}", task_id);
}

#[derive(Serialize)]
pub struct KeywordDto { pub id: String, pub keyword: String, pub kind: String, pub target_domain: String, pub enabled: bool }

#[derive(Serialize)]
pub struct MetricOverview {
    pub keyword: String,
    pub kind: String,
    pub source: String,              // serp | mention | trends
    pub latest: Option<i64>,
    pub previous: Option<i64>,
    pub captured_at: Option<String>,
    pub samples: usize,
    pub series: Vec<Option<i64>>,    // 时间升序的值（折线/迷你图）
    pub detail: Option<String>,      // 最新一条 detail（JSON 串）
}

#[tauri::command]
pub(crate) fn metrics_list_keywords(state: State<AppState>) -> Result<Vec<KeywordDto>, String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, keyword, kind, COALESCE(target_domain,'doaipm.com'), enabled FROM metric_keywords ORDER BY kind, keyword"
    ).map_err(|e| e.to_string())?;
    let it = stmt.query_map([], |r| Ok(KeywordDto{
        id: r.get(0)?, keyword: r.get(1)?, kind: r.get(2)?, target_domain: r.get(3)?, enabled: r.get::<_, i64>(4)? != 0
    })).map_err(|e| e.to_string())?;
    Ok(it.flatten().collect())
}

#[tauri::command]
pub(crate) fn metrics_add_keyword(state: State<AppState>, keyword: String, kind: String, target_domain: Option<String>) -> Result<String, String> {
    let kw = keyword.trim().to_string();
    if kw.is_empty() { return Err("关键词为空".into()); }
    let kind = if ["brand","longtail","mention"].contains(&kind.as_str()) { kind } else { "longtail".to_string() };
    let id = Uuid::new_v4().to_string();
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    conn.execute(
        "INSERT INTO metric_keywords (id, keyword, kind, target_domain, enabled) VALUES (?1,?2,?3,?4,1)",
        params![id, kw, kind, target_domain.unwrap_or_else(|| "doaipm.com".into())]
    ).map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub(crate) fn metrics_delete_keyword(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    conn.execute("DELETE FROM metric_keywords WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub(crate) fn metrics_toggle_keyword(state: State<AppState>, id: String, enabled: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    conn.execute("UPDATE metric_keywords SET enabled=?1 WHERE id=?2",
        params![if enabled {1} else {0}, id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub(crate) fn metrics_overview(state: State<AppState>) -> Result<Vec<MetricOverview>, String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    let combos: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT keyword, source, COALESCE(kind,'') FROM metrics ORDER BY source, keyword"
        ).map_err(|e| e.to_string())?;
        let it = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|e| e.to_string())?;
        it.flatten().collect()
    };
    let mut out = Vec::new();
    for (kw, source, kind) in combos {
        let rows: Vec<(Option<i64>, String, Option<String>)> = {
            let mut s2 = conn.prepare(
                "SELECT value, captured_at, detail FROM metrics WHERE keyword=?1 AND source=?2 ORDER BY captured_at ASC"
            ).map_err(|e| e.to_string())?;
            let it = s2.query_map(params![kw, source], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|e| e.to_string())?;
            it.flatten().collect()
        };
        if rows.is_empty() { continue; }
        let full: Vec<Option<i64>> = rows.iter().map(|r| r.0).collect();
        let latest = rows.last().and_then(|r| r.0);
        let previous = if rows.len() >= 2 { rows[rows.len()-2].0 } else { None };
        let captured_at = rows.last().map(|r| r.1.clone());
        let detail = rows.last().and_then(|r| r.2.clone());
        // 仅保留最近 30 个点用于迷你图
        let series: Vec<Option<i64>> = if full.len() > 30 { full[full.len()-30..].to_vec() } else { full };
        out.push(MetricOverview{ keyword: kw, kind, source, latest, previous, captured_at, samples: rows.len(), series, detail });
    }
    Ok(out)
}

#[tauri::command]
pub(crate) fn metrics_get_settings(state: State<AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    Ok(serde_json::json!({
        "trends_on": engine_cfg_get(&conn, "metrics_trends_on").as_deref() == Some("1"),
        "enabled": engine_cfg_get(&conn, "metrics_enabled").as_deref() != Some("0"),
        "last_tick": engine_cfg_get(&conn, "metrics_last_tick"),
        "region": METRICS_REGION,
        "profile": METRICS_PROFILE_NAME,
    }))
}

#[tauri::command]
pub(crate) fn metrics_set_trends(state: State<AppState>, on: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    engine_cfg_set(&conn, "metrics_trends_on", if on {"1"} else {"0"});
    Ok(())
}

#[tauri::command]
pub(crate) async fn metrics_collect_now(app: AppHandle) -> Result<String, String> {
    ensure_browser_connected().await.map_err(|e| format!("浏览器未就绪: {}", e))?;
    let app2 = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || metrics_collect_all(&app2)).await
        .map_err(|e| format!("采集线程异常: {}", e))?;
    let (s, m) = res?;
    Ok(format!("采集完成：排名 {} 词，提及 {} 词", s, m))
}
