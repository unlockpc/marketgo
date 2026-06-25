//! 养号运行时域：GitHub / X / SegmentFault 三平台的专属养号 runner + 各平台 blocking 浏览器动作
//! + 逐动作进度推送。催化剂/配额/记录/账号配置等共享 helper 仍在 lib.rs(crate root),
//! 经 `use crate::*` 反向引用(子模块可见 crate root 私有项)。

use tauri::{AppHandle, Manager};
use rusqlite::params;
use chrono::{Utc, Local};
use uuid::Uuid;
use crate::*;
use crate::ai::gen_nurture_text;

/// 向前端推送一条养号逐动作进度。前端监听 `nurture-progress` 事件 → 实时显示当前账号在干嘛。
/// 养号动作之间有几十秒的拟人间隔/重页面加载，逐动作推送能让进度看着「在动」。
fn emit_nurture_step(app: &AppHandle, account_id: &str, note: &str) {
    let _ = app.emit("nurture-progress", serde_json::json!({
        "accountId": account_id,
        "note": note,
    }));
    log::info!("[NURTURE-STEP] {} {}", account_id, note);
}

/// GitHub L1 养号：按账号领域，从 topic 页采候选 → 去重选取 → star。
/// 浏览器调用走 spawn_blocking（阻塞 reqwest 不能在 async 直接调，见项目约定）。
/// follow/watch 见同文件 follow/watch 助手（Task 7b 接入）；L2 评论见末尾（Task 9）。
pub(crate) async fn github_nurture_run(app: &AppHandle, account_id: &str, _duration: i64) -> Result<String, String> {
    let session_start = std::time::Instant::now();
    // 1) 读领域 + 分期
    let (domains, topics, phase) = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let domains = account_topics(&conn, account_id);
        let topics = account_topic_keywords(&conn, account_id);
        let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
        let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
        let strat = conn.query_row("SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max FROM nurture_strategies WHERE platform='github'",
            [], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?, r.get::<_,i64>(2)?, r.get::<_,i64>(3)?))).ok();
        let (warmup, growth, smin, smax) = strat.unwrap_or((3, 3, 2, 5));
        let (phase, _t) = nurture_phase_and_target(age, warmup, growth, smin, smax);
        (domains, topics, phase.to_string())
    };
    if domains.is_empty() {
        return Ok("账号未选领域，跳过 GitHub 养号".to_string());
    }
    let (n_star, n_follow, n_watch) = gh_daily_quota(&phase);

    // 2) 选 topic（关键词已在首块按所选领域收集；时间派生种子）
    if topics.is_empty() { return Ok("领域无可用 topic".to_string()); }
    let seed = get_random_delay(1, 100_000);
    let topic = &topics[(seed as usize) % topics.len()];

    // 3) 浏览器：导航 topic 页（按 star 排序）采 repo 链接
    let topic_owned = topic.to_string();
    let repo_links: Vec<String> = tauri::async_runtime::spawn_blocking(move || {
        let url = format!("https://github.com/topics/{}?o=desc&s=stars", topic_owned);
        unzoo_navigate(&url)?;
        std::thread::sleep(std::time::Duration::from_secs(3));
        if !check_platform_login_status("github").unwrap_or(false) {
            return Err("未登录 github".to_string());
        }
        unzoo_get_links("article h3 a[href^=\"/\"], h3 a[href*=\"/\"]")
    }).await.map_err(|e| format!("采集异常: {}", e))??;

    // 规范化为 owner/repo 两段的绝对 URL
    let mut repos: Vec<String> = repo_links.into_iter()
        .filter_map(|h| {
            let path = h.trim_start_matches("https://github.com").trim_start_matches('/');
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if segs.len() == 2 { Some(format!("https://github.com/{}/{}", segs[0], segs[1])) } else { None }
        }).collect();
    repos.dedup();
    log::info!("[GH-NURTURE] topic={} 采到 {} 个候选 repo, phase={}, 配额(star/follow/watch)={}/{}/{}",
        topic, repos.len(), phase, n_star, n_follow, n_watch);

    // 4) DB 过滤：本账号已操作 + 跨账号触碰过多（≥3 个 persona）→ 排除
    let chosen: Vec<String> = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let mut already = std::collections::HashSet::new();
        for r in &repos {
            if gh_already_acted(&conn, account_id, r) || gh_target_persona_count(&conn, r) >= 3 {
                already.insert(r.clone());
            }
        }
        gh_pick_targets(&repos, &already, n_star.max(1) as usize, seed)
    };
    log::info!("[GH-NURTURE] 过滤后选中 {} 个 repo 准备 star: {:?}", chosen.len(), chosen);

    // 5) 浏览器：逐个 star（含拟人间隔）
    let mut done = 0i64;
    emit_nurture_step(app, account_id, &format!("开始 GitHub 养号 · 准备 Star {} 个仓库", chosen.len()));
    for (i, repo) in chosen.iter().enumerate() {
        let short = repo.trim_start_matches("https://github.com/");
        emit_nurture_step(app, account_id, &format!("⭐ Star {}/{}：{}", i + 1, chosen.len(), short));
        let repo_c = repo.clone();
        let res = tauri::async_runtime::spawn_blocking(move || gh_star_repo_blocking(&repo_c)).await
            .map_err(|e| format!("star 异常: {}", e))?;
        if res.is_ok() {
            let st = app.state::<AppState>();
            if let Ok(conn) = st.db.lock() { let _ = gh_record_action(&conn, account_id, "star", repo); }
            done += 1;
        }
        tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(15, 40))).await;
    }
    // follow：对已 star 的 repo follow 其 owner（owner profile = https://github.com/owner）
    if n_follow > 0 {
        for (i, repo) in chosen.iter().take(n_follow as usize).enumerate() {
            let owner_url = repo.rsplitn(2, '/').nth(1).unwrap_or(repo).to_string();
            if owner_url.matches('/').count() != 3 { continue; } // 仅 https://github.com/owner 形态
            emit_nurture_step(app, account_id, &format!("👤 Follow {}/{}：{}", i + 1, n_follow, owner_url.trim_start_matches("https://github.com/")));
            let acted = {
                let st = app.state::<AppState>();
                let locked = st.db.lock().map_err(|e| e.to_string())?;
                gh_already_acted(&locked, account_id, &owner_url)
            };
            if !acted {
                let u = owner_url.clone();
                let res = tauri::async_runtime::spawn_blocking(move || gh_follow_user_blocking(&u)).await
                    .map_err(|e| e.to_string())?;
                if res.is_ok() {
                    let st = app.state::<AppState>();
                    let locked = st.db.lock();
                    if let Ok(conn) = locked { let _ = gh_record_action(&conn, account_id, "follow", &owner_url); }
                }
                tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(15, 40))).await;
            }
        }
    }
    // watch：对第一个 star 的 repo watch（target 用 "<repo>#watch" 与 star 区分）
    if n_watch > 0 {
        if let Some(repo) = chosen.first() {
            emit_nurture_step(app, account_id, &format!("👁 Watch：{}", repo.trim_start_matches("https://github.com/")));
            let watch_key = format!("{}#watch", repo);
            let acted = {
                let st = app.state::<AppState>();
                let locked = st.db.lock().map_err(|e| e.to_string())?;
                gh_already_acted(&locked, account_id, &watch_key)
            };
            if !acted {
                let r = repo.clone();
                let res = tauri::async_runtime::spawn_blocking(move || gh_watch_repo_blocking(&r)).await
                    .map_err(|e| e.to_string())?;
                if res.is_ok() {
                    let st = app.state::<AppState>();
                    let locked = st.db.lock();
                    if let Ok(conn) = locked { let _ = gh_record_action(&conn, account_id, "watch", &watch_key); }
                }
            }
        }
    }

    // 6) 写养号统计（如实记录本次耗时 + 累加总时长，与通用养号一致）
    let elapsed_secs = session_start.elapsed().as_secs() as i64;
    {
        let st = app.state::<AppState>();
        let locked = st.db.lock(); // 绑定到 locked，确保其借用在 st 之前释放
        if let Ok(conn) = locked {
            let now = Utc::now().to_rfc3339();
            let today = Local::now().format("%Y-%m-%d").to_string();
            let _ = conn.execute(
                "UPDATE accounts SET nurture_started_at=COALESCE(nurture_started_at,?1), last_nurture_at=?1, \
                 total_nurture_seconds=COALESCE(total_nurture_seconds,0)+?2, health_status='healthy', last_health_check=?1 WHERE id=?3",
                params![now, elapsed_secs, account_id]);
            let _ = conn.execute(
                "INSERT INTO nurture_daily_logs (id, account_id, date, sessions_completed, total_seconds) VALUES (?1,?2,?3,1,?4) \
                 ON CONFLICT(account_id,date) DO UPDATE SET sessions_completed=sessions_completed+1, total_seconds=total_seconds+?4",
                params![Uuid::new_v4().to_string(), account_id, today, elapsed_secs]);
        }
    }
    log::info!("[GH-NURTURE] account={} topic={} star={} 耗时={}s", account_id, topic, done, elapsed_secs);
    Ok(format!("GitHub 养号完成：topic={} star={} 用时{}s", topic, done, elapsed_secs))
}

/// SegmentFault 登录检测（DOM 法，思否专用）。通用文本检测对思否失效（中文站 + 首页常停 loading）。
/// 可靠信号（见 site-patterns/segmentfault.com.md）：有「登录」入口 = 未登录；导航已渲染且无登录入口 = 已登录。
/// 在 spawn_blocking 中同步调用。
fn sf_logged_in_blocking() -> bool {
    use std::time::Duration;
    let _ = unzoo_navigate("https://segmentfault.com/");
    let mut waited = 0;
    while waited < 18 {
        std::thread::sleep(Duration::from_secs(3));
        waited += 3;
        // 已登录确证：写文章 / 草稿 / 通知 等登录后入口出现
        if unzoo_element_exists("a[href^=\"/write\"]")
            || unzoo_element_exists("a[href*=\"/user/draft\"]")
            || unzoo_element_exists("a[href*=\"/user/notifications\"]")
            || unzoo_element_exists("a[href^=\"/u/\"]") {
            return true;
        }
        // 未登录确证：登录入口存在
        if unzoo_element_exists("a[href*=\"/user/login\"]") {
            return false;
        }
        // 都没命中 → 首屏还没渲染好，继续等
    }
    false
}

/// SegmentFault 养号（搜索驱动，全程只读）：按所选领域取关键词 → 站内搜索 → 拟人浏览结果
/// → 随机点进文章/问题阅读。预热/成长期都安全（不点赞/不发帖，只读不触发风控计数）。
/// 成长期搜索次数与阅读篇数更多；阅读到的问题正好供后续人工/回复流去回答。
/// 一次 spawn_blocking 跑完整轮（阻塞 reqwest 不能在 async 直接调，见项目约定）。
fn sf_nurture_browse_blocking(keywords: Vec<String>, n_search: i64, read_per_search: i64, duration_secs: i64, seed0: u64) -> Result<(i64, i64), String> {
    use std::time::{Duration, Instant};
    let start = Instant::now();
    if keywords.is_empty() { return Ok((0, 0)); }
    // 先确认登录（思否需手工登录，未登录直接报错让用户先登一次）。
    // 通用 verify_login_blocking 走英文文本匹配，对思否（中文站 + 首页常停在 loading）失效，
    // 故用 DOM 检测：见 sf_logged_in_blocking。
    if !sf_logged_in_blocking() {
        return Err("未登录 SegmentFault！请先点卡片上「✋ 手工登录」在浏览器里登一次，再养号。".to_string());
    }
    let mut searched = 0i64;
    let mut read = 0i64;
    let mut seed = seed0 | 1;
    for _ in 0..n_search.max(1) {
        if start.elapsed().as_secs() as i64 >= duration_secs { break; }
        // xorshift 推进选词
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        let kw = &keywords[(seed as usize) % keywords.len()];
        let q_enc = kw.replace(' ', "%20");
        let url = format!("https://segmentfault.com/search?q={}", q_enc);
        if unzoo_navigate(&url).is_err() { continue; }
        std::thread::sleep(Duration::from_millis(get_human_delay(2500, 4500)));
        searched += 1;
        // 拟人滚动结果页
        for _ in 0..get_human_delay(2, 4) {
            let _ = unzoo_scroll("down", get_human_delay(200, 500) as i32);
            std::thread::sleep(Duration::from_millis(get_human_delay(1500, 3500)));
            random_mouse_movement();
        }
        // 采结果链接（思否文章 /a/、问题 /q/），规范化为绝对 URL 去重
        let links = unzoo_get_links("a[href*=\"/a/\"], a[href*=\"/q/\"]").unwrap_or_default();
        let mut posts: Vec<String> = links.into_iter().filter_map(|h| {
            let h = h.trim();
            let abs = if h.starts_with("http") { h.to_string() }
                      else if h.starts_with('/') { format!("https://segmentfault.com{}", h) }
                      else { return None; };
            if abs.contains("/a/") || abs.contains("/q/") { Some(abs) } else { None }
        }).collect();
        posts.dedup();
        // 点进 read_per_search 篇阅读（拟人滚动到底，stay 一会）
        let mut opened = 0i64;
        for p in posts {
            if opened >= read_per_search { break; }
            if start.elapsed().as_secs() as i64 >= duration_secs { break; }
            if unzoo_navigate(&p).is_err() { continue; }
            std::thread::sleep(Duration::from_millis(get_human_delay(2500, 4000)));
            for _ in 0..get_human_delay(3, 6) {
                let _ = unzoo_scroll("down", get_human_delay(250, 600) as i32);
                std::thread::sleep(Duration::from_millis(get_human_delay(1800, 4000)));
                random_mouse_movement();
            }
            read += 1;
            opened += 1;
        }
        // 下一次搜索前停顿
        std::thread::sleep(Duration::from_millis(get_human_delay(2000, 4000)));
    }
    Ok((searched, read))
}

/// SegmentFault 养号入口：读领域 + 分期 → 搜索驱动浏览 → 写养号统计（与通用养号一致）。
/// 未选领域 → 跳过并提示（不退回纯滚动，避免无领域指纹）。
pub(crate) async fn segmentfault_nurture_run(app: &AppHandle, account_id: &str, duration: i64) -> Result<String, String> {
    let session_start = std::time::Instant::now();
    // 1) 读领域 + 分期
    let (domains, kws, phase) = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let domains = account_topics(&conn, account_id);
        let kws = account_topic_keywords(&conn, account_id);
        let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
        let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
        let strat = conn.query_row("SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max FROM nurture_strategies WHERE platform='segmentfault'",
            [], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?, r.get::<_,i64>(2)?, r.get::<_,i64>(3)?))).ok();
        let (warmup, growth, smin, smax) = strat.unwrap_or((3, 7, 1, 2));
        let (phase, _t) = nurture_phase_and_target(age, warmup, growth, smin, smax);
        (domains, kws, phase.to_string())
    };
    if domains.is_empty() {
        return Ok("账号未选领域，跳过 SegmentFault 养号（点卡片上「🎯 主题」选一下方向）".to_string());
    }
    if kws.is_empty() { return Ok("领域无可用关键词".to_string()); }
    // 分期定强度：预热轻、成长重、成熟维持
    let (n_search, read_per_search) = match phase.as_str() {
        "growth" => (3i64, 2i64),
        "mature" => (2, 1),
        _ => (2, 1), // warmup
    };
    let dur = duration.max(30);
    let seed0 = get_random_delay(1, 100_000);
    emit_nurture_step(app, account_id, &format!("开始 SegmentFault 养号 · 按领域搜索 {} 次并阅读（约 {}s）", n_search, dur));
    let (searched, read) = tauri::async_runtime::spawn_blocking(move || sf_nurture_browse_blocking(kws, n_search, read_per_search, dur, seed0))
        .await.map_err(|e| format!("养号任务异常: {}", e))??;

    // 写养号统计（如实记录耗时 + 累加，与通用/GH 养号一致）
    let elapsed_secs = session_start.elapsed().as_secs() as i64;
    {
        let st = app.state::<AppState>();
        let locked = st.db.lock();
        if let Ok(conn) = locked {
            let now = Utc::now().to_rfc3339();
            let today = Local::now().format("%Y-%m-%d").to_string();
            let _ = conn.execute(
                "UPDATE accounts SET nurture_started_at=COALESCE(nurture_started_at,?1), last_nurture_at=?1, \
                 total_nurture_seconds=COALESCE(total_nurture_seconds,0)+?2, health_status='healthy', last_health_check=?1 WHERE id=?3",
                params![now, elapsed_secs, account_id]);
            let _ = conn.execute(
                "INSERT INTO nurture_daily_logs (id, account_id, date, sessions_completed, total_seconds) VALUES (?1,?2,?3,1,?4) \
                 ON CONFLICT(account_id,date) DO UPDATE SET sessions_completed=sessions_completed+1, total_seconds=total_seconds+?4",
                params![Uuid::new_v4().to_string(), account_id, today, elapsed_secs]);
        }
    }
    log::info!("[SF-NURTURE] account={} phase={} 搜索={} 阅读={} 耗时={}s", account_id, phase, searched, read, elapsed_secs);
    Ok(format!("SegmentFault 养号完成（{}）：搜索 {} 次 · 阅读 {} 篇 · 用时 {}s", phase, searched, read, elapsed_secs))
}

/// 小红书养号分期强度 → (搜索次数, 每次阅读, 点赞数)。预热只读，成长点赞，成熟维持。
pub(crate) fn xhs_phase_intensity(phase: &str) -> (i64, i64, i64) {
    match phase {
        "growth" => (3, 3, 2),
        "mature" => (2, 2, 1),
        _ => (2, 2, 0), // warmup
    }
}

/// 轮询关键元素出现确认页面加载完（每秒一次，最多 max_secs 秒）。供"等加载再操作"。
fn xhs_wait_loaded_blocking(selector: &str, max_secs: u64) -> bool {
    use std::time::Duration;
    let mut waited = 0;
    while waited < max_secs {
        if unzoo_element_exists(selector) { return true; }
        std::thread::sleep(Duration::from_secs(1));
        waited += 1;
    }
    false
}

/// 小红书登录检测：导航首页，轮询登录后入口出现 vs 登录入口。选择器为最佳猜测，实测可能需微调。
fn xhs_logged_in_blocking() -> bool {
    use std::time::Duration;
    let _ = unzoo_navigate("https://www.xiaohongshu.com/");
    let mut waited = 0;
    while waited < 18 {
        std::thread::sleep(Duration::from_secs(3));
        waited += 3;
        // 已登录确证：用户头像 / 侧栏「我」入口（实测可能需调整）
        if unzoo_element_exists(".reds-avatar")
            || unzoo_element_exists("a[href*=\"/user/profile/\"]")
            || unzoo_element_exists(".side-bar .user") {
            return true;
        }
        // 未登录确证：登录弹窗/按钮（实测可能需调整）
        if unzoo_element_exists(".login-container") || unzoo_element_exists(".login-btn") {
            return false;
        }
    }
    false
}

/// 在当前笔记页点赞（最佳猜测选择器，实测可能需微调）。成功点击返回 true。
fn xhs_like_blocking() -> bool {
    let selectors = ["span.like-wrapper", ".interact-container .like-wrapper", "[class*=\"like-active\"]", ".like-wrapper"];
    for s in selectors {
        if unzoo_element_exists(s) {
            return unzoo_click(s).is_ok();
        }
    }
    false
}

/// 小红书养号（搜索驱动）：按主题关键词搜索→拟人浏览→点进笔记阅读；成长期对少量笔记点赞。
/// 全程"等加载+随机延迟"再操作。返回 (searched, read, liked)。
/// app/account_id 用于点赞去重(xhs_actions_log)与进度推送。
fn xhs_nurture_browse_blocking(app: AppHandle, account_id: &str, keywords: Vec<String>, n_search: i64, read_per_search: i64, n_like: i64, duration_secs: i64, seed0: u64) -> Result<(i64, i64, i64), String> {
    use std::time::{Duration, Instant};
    let start = Instant::now();
    if keywords.is_empty() { return Ok((0, 0, 0)); }
    if !xhs_logged_in_blocking() {
        return Err("未登录小红书！请先点卡片上「✋ 手工登录」在浏览器里登一次，再养号。".to_string());
    }
    let mut searched = 0i64; let mut read = 0i64; let mut liked = 0i64;
    let mut seed = seed0 | 1;
    for _ in 0..n_search.max(1) {
        if start.elapsed().as_secs() as i64 >= duration_secs { break; }
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        let kw = &keywords[(seed as usize) % keywords.len()];
        let q_enc = kw.replace(' ', "%20");
        let url = format!("https://www.xiaohongshu.com/search_result?keyword={}", q_enc);
        if unzoo_navigate(&url).is_err() { continue; }
        // 等搜索结果加载完（笔记卡片出现）再操作
        if !xhs_wait_loaded_blocking("a[href*=\"/explore/\"]", 8) { continue; }
        std::thread::sleep(Duration::from_millis(get_human_delay(2000, 4000)));
        searched += 1;
        // 拟人滚动结果页
        for _ in 0..get_human_delay(2, 4) {
            let _ = unzoo_scroll("down", get_human_delay(200, 500) as i32);
            std::thread::sleep(Duration::from_millis(get_human_delay(1500, 3500)));
            random_mouse_movement();
        }
        // 采笔记链接，规范化绝对 URL 去重
        let links = unzoo_get_links("a[href*=\"/explore/\"]").unwrap_or_default();
        let mut notes: Vec<String> = links.into_iter().filter_map(|h| {
            let h = h.trim();
            let abs = if h.starts_with("http") { h.to_string() }
                      else if h.starts_with('/') { format!("https://www.xiaohongshu.com{}", h) }
                      else { return None; };
            if abs.contains("/explore/") { Some(abs) } else { None }
        }).collect();
        notes.dedup();
        // 点进 read_per_search 篇阅读，成长期配额内点赞
        let mut opened = 0i64;
        for p in notes {
            if opened >= read_per_search { break; }
            if start.elapsed().as_secs() as i64 >= duration_secs { break; }
            if unzoo_navigate(&p).is_err() { continue; }
            // 等笔记页加载完再操作；加载不出就跳过这篇
            if !xhs_wait_loaded_blocking(".note-content, #noteContainer, .interaction-container", 8) { continue; }
            std::thread::sleep(Duration::from_millis(get_human_delay(2500, 4000)));
            // 拟人滚动阅读
            for _ in 0..get_human_delay(3, 6) {
                let _ = unzoo_scroll("down", get_human_delay(250, 600) as i32);
                std::thread::sleep(Duration::from_millis(get_human_delay(1800, 4000)));
                random_mouse_movement();
            }
            read += 1; opened += 1;
            // 成长期点赞：配额内 + 未赞过（去重避免重复点赞导致取消赞）+ 等加载后随机停 2~5s 才点
            if liked < n_like {
                let already = {
                    let st = app.state::<AppState>();
                    let locked = st.db.lock();
                    match locked { Ok(c) => xhs_already_acted(&c, account_id, &p), Err(_) => true }
                };
                if !already {
                    std::thread::sleep(Duration::from_millis(get_human_delay(2000, 5000))); // 加载后随机停几秒再点赞
                    if xhs_like_blocking() {
                        liked += 1;
                        {
                            let st = app.state::<AppState>();
                            let locked = st.db.lock();
                            if let Ok(c) = locked { let _ = xhs_record_action(&c, account_id, "like", &p); }
                        }
                        emit_nurture_step(&app, account_id, &format!("👍 点赞 {}/{}", liked, n_like));
                        std::thread::sleep(Duration::from_millis(get_human_delay(1500, 3000))); // 点后 settle
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(get_human_delay(2000, 4000)));
    }
    Ok((searched, read, liked))
}

/// X 养号：按方向取关键词→搜索采推文/用户→去重选取→点赞/关注/转推/回复 + 极少原创。
pub(crate) async fn x_nurture_run(app: &AppHandle, account_id: &str, _duration: i64) -> Result<String, String> {
    let session_start = std::time::Instant::now();
    // 1) 读方向 + 分期
    let (niches, kws, phase, warmup) = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let niches = account_topics(&conn, account_id);
        let kws = account_topic_keywords(&conn, account_id);
        let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
        let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
        let strat = conn.query_row("SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max FROM nurture_strategies WHERE platform='twitter'",
            [], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?, r.get::<_,i64>(2)?, r.get::<_,i64>(3)?))).ok();
        let (warmup, growth, smin, smax) = strat.unwrap_or((5, 5, 2, 4));
        let (phase, _t) = nurture_phase_and_target(age, warmup, growth, smin, smax);
        (niches, kws, phase.to_string(), warmup)
    };
    if niches.is_empty() {
        return Ok("账号未选方向，跳过 X 养号".to_string());
    }
    let (n_like, n_follow, n_engage) = x_daily_quota(&phase);

    // 2) 选方向 → 关键词（已在首块收集）
    if kws.is_empty() { return Ok("方向无可用关键词".to_string()); }
    let seed = get_random_delay(1, 100_000);
    let kw = &kws[(seed as usize) % kws.len()];

    // 3) 浏览器：搜领域词，用「热门(Top)」标签——X 按互动热度排序，直接给该领域当下热门推。
    //    注意：高级运算符 min_faves 在 X 网页端已失效（会被当字面文本→0 结果），故不用运算符；
    //    不带 f=live → 默认 Top(热门)；Top 本身偏向近期高互动，兼顾热度+新鲜。
    // 关键词常是 hashtag(带 #)，必须整体 URL 编码：# 不编码会被浏览器当作 fragment 分隔符，
    // 导致 q= 变成空查询 → 采到 0 条推文。.into_owned() 让其满足 spawn_blocking 的 'static 约束。
    let q_enc = urlencoding::encode(kw).into_owned();
    let probe: (Option<String>, Vec<String>) = tauri::async_runtime::spawn_blocking(move || {
        let url = format!("https://x.com/search?q={}", q_enc);
        unzoo_navigate(&url)?;
        std::thread::sleep(std::time::Duration::from_secs(4));
        // A：体检——抓页面文本判封禁/锁定（suspended/locked 会重定向到对应页）
        let raw = unzoo_evaluate("(document.body.innerText||'').slice(0,4000)").unwrap_or_default();
        let text = serde_json::from_str::<String>(&raw).unwrap_or(raw);
        if let Some(state) = x_classify_health(&text) {
            if state == "banned" || state == "locked" {
                return Ok((Some(state.to_string()), Vec::new()));
            }
        }
        // 登录判定走 DOM 元素（X 是重 SPA、窄窗导航只有图标无文字，文本法会误判未登录）：
        // 有「账号切换/Home/发推」标记=登录；仅当出现登录按钮且无登录标记时才判未登录。
        let logged_in_marker = unzoo_element_exists("[data-testid=\"SideNav_AccountSwitcher_Button\"]")
            || unzoo_element_exists("[data-testid=\"AppTabBar_Home_Link\"]")
            || unzoo_element_exists("[data-testid=\"SideNav_NewTweet_Button\"]");
        let logged_out_marker = unzoo_element_exists("[data-testid=\"loginButton\"]")
            || unzoo_element_exists("a[href=\"/login\"]");
        if logged_out_marker && !logged_in_marker {
            return Err("未登录 twitter".to_string());
        }
        // 等推文渲染（X SPA 懒加载，Latest 较慢；最多再等 ~10s）
        let mut waited = 0;
        while !unzoo_element_exists("[data-testid=\"tweet\"]") && waited < 10 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            waited += 2;
        }
        Ok((None, unzoo_get_links("a[href*=\"/status/\"]")?))
    }).await.map_err(|e| format!("采集异常: {}", e))??;
    // A：检测到已封/锁定 → 写 health_status，停止本次养号
    if let Some(state) = probe.0 {
        let st = app.state::<AppState>();
        if let Ok(conn) = st.db.lock() {
            let _ = conn.execute("UPDATE accounts SET health_status=?1, last_health_check=datetime('now') WHERE id=?2", params![state, account_id]);
        }
        log::warn!("[X-NURTURE] account={} 检测到账号状态 {} → 停止养号", account_id, state);
        return Ok(format!("X 账号状态异常({})，已停止养号", state));
    }
    let tweet_links: Vec<String> = probe.1;

    // 规范化为 https://x.com/<user>/status/<id>
    let mut tweets: Vec<String> = tweet_links.into_iter().filter_map(|h| {
        let idx = h.find("/status/")?;
        let after = &h[idx + "/status/".len()..];
        let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if id.is_empty() { return None; }
        let head = &h[..idx];
        Some(format!("{}/status/{}", head.trim_end_matches('/'), id))
    }).collect();
    tweets.sort(); tweets.dedup();
    log::info!("[X-NURTURE] kw={} 采到 {} 条推文, phase={}, 配额(like/follow/engage)={}/{}/{}",
        kw, tweets.len(), phase, n_like, n_follow, n_engage);

    // 4) DB 过滤（本账号已操作 + 跨账号≥3）
    let chosen: Vec<String> = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let mut already = std::collections::HashSet::new();
        for t in &tweets {
            if x_already_acted(&conn, account_id, t) || x_target_persona_count(&conn, t) >= 3 {
                already.insert(t.clone());
            }
        }
        gh_pick_targets(&tweets, &already, n_like.max(1) as usize, seed)
    };

    // 5) L1 点赞（B：动作命中限流/受限 → 退避，停止本轮剩余动作）
    let mut likes = 0i64;
    let mut aborted_health: Option<String> = None;
    emit_nurture_step(app, account_id, &format!("开始 X 养号 · 准备点赞 {} 条推文", chosen.len()));
    for (i, t) in chosen.iter().enumerate() {
        emit_nurture_step(app, account_id, &format!("❤️ 点赞中 {}/{}", i + 1, chosen.len()));
        let tc = t.clone();
        let r = tauri::async_runtime::spawn_blocking(move || x_like_blocking(&tc)).await.map_err(|e| e.to_string())?;
        match r {
            Ok(_) => {
                let st = app.state::<AppState>();
                let locked = st.db.lock();
                if let Ok(conn) = locked { let _ = x_record_action(&conn, account_id, "like", t); }
                likes += 1;
            }
            Err(e) if e.starts_with("HEALTH:") => { aborted_health = Some(e[7..].to_string()); break; }
            Err(_) => {}
        }
        // X 动作间隔：随机 15-40 秒（拟人 + 不过度）
        tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(15, 40))).await;
    }

    // 6) L1 关注：People 搜索找该领域好用户 → 质量门(有简介+粉丝≥500)达标才关注
    let mut follows = 0i64;
    if aborted_health.is_none() && n_follow > 0 {
        let ukw = kws[((seed >> 3) as usize) % kws.len()].to_string(); // 换一个子话题搜人
        emit_nurture_step(app, account_id, &format!("👤 搜索领域优质用户中…（{}）", ukw));
        let candidates: Vec<String> = match tauri::async_runtime::spawn_blocking(move || x_search_users_blocking(&ukw)).await {
            Ok(Ok(v)) => v,
            _ => Vec::new(), // people 搜索失败就不关注，不影响其它动作
        };
        // 过滤已关注 + 跨账号去重；多取些候选（质量门会刷掉一部分）
        let pool: Vec<String> = {
            let st = app.state::<AppState>();
            let conn = st.db.lock().map_err(|e| e.to_string())?;
            let mut already = std::collections::HashSet::new();
            for p in &candidates {
                if x_already_acted(&conn, account_id, p) || x_target_persona_count(&conn, p) >= 3 {
                    already.insert(p.clone());
                }
            }
            gh_pick_targets(&candidates, &already, (n_follow * 3).max(3) as usize, seed)
        };
        emit_nurture_step(app, account_id, &format!("👤 找领域优质用户关注（目标 {} 个）", n_follow));
        for prof in &pool {
            if follows >= n_follow { break; }
            emit_nurture_step(app, account_id, &format!("👤 关注评估中（已 {}/{}）：{}", follows, n_follow, prof.trim_start_matches("https://x.com/")));
            let p = prof.clone();
            let res = tauri::async_runtime::spawn_blocking(move || x_follow_quality_blocking(&p)).await
                .map_err(|e| e.to_string())?;
            match res {
                Ok(true) => {
                    let st = app.state::<AppState>(); let l = st.db.lock();
                    if let Ok(conn) = l { let _ = x_record_action(&conn, account_id, "follow", prof); }
                    follows += 1;
                }
                Ok(false) => {} // 不达标/已关注，下一个
                Err(e) if e.starts_with("HEALTH:") => { aborted_health = Some(e[7..].to_string()); break; }
                Err(_) => {}
            }
            tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(15, 40))).await; // X 动作间隔 15-40s
        }
    }

    // 7a) L2：engage 预算内，对部分已点赞推文转推（回复动作已移除——详情页正文抓取在养号场景不可靠，
    //     且自动回复质量难保证；点赞/关注/转推/极少原创已足够养号）
    let mut engages = 0i64;
    if aborted_health.is_none() && n_engage > 0 {
        for (i, t) in chosen.iter().take(n_engage as usize).enumerate() {
            let key = format!("{}#engage", t);
            let acted = { let st = app.state::<AppState>(); let l = st.db.lock().map_err(|e| e.to_string())?; x_already_acted(&l, account_id, &key) };
            if acted { continue; }
            emit_nurture_step(app, account_id, &format!("🔁 转推 {}/{}", i + 1, n_engage));
            let tc = t.clone();
            let r = tauri::async_runtime::spawn_blocking(move || x_retweet_blocking(&tc)).await.map_err(|e| e.to_string())?;
            if r.is_ok() {
                let st = app.state::<AppState>(); let l = st.db.lock();
                if let Ok(conn) = l {
                    let _ = x_record_action(&conn, account_id, "retweet", &key);
                }
                engages += 1;
            }
            tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(15, 40))).await; // X 动作间隔 15-40s
        }
    }
    let _ = engages;

    // 7b) L3：满足闸门时发 1 条极少原创（每周 ≤1 条）
    if aborted_health.is_none() {
        let st = app.state::<AppState>();
        let (age, l1, weekly) = {
            let conn = st.db.lock().map_err(|e| e.to_string())?;
            let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
            let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
            let l1: i64 = conn.query_row("SELECT COUNT(*) FROM x_actions_log WHERE account_id=?1 AND action_type IN ('like','follow')", params![account_id], |r| r.get(0)).unwrap_or(0);
            let weekly: i64 = conn.query_row("SELECT COUNT(*) FROM x_actions_log WHERE account_id=?1 AND action_type='tweet' AND date >= date('now','-7 day')", params![account_id], |r| r.get(0)).unwrap_or(0);
            (age, l1, weekly)
        };
        if x_l3_allowed(age, warmup, l1) && weekly < 1 {
            // AI 基于账号领域生成一条原创（L3 无原文，按领域/话题；无 AI / 不合格则跳过）
            let domains_str = niches.join("、");
            let ctx = format!("领域: {} / 话题: {}", domains_str, kw);
            let r = match gen_nurture_text(app, "x_tweet", &ctx).await {
                Some(txt) => tauri::async_runtime::spawn_blocking(move || x_post_tweet_blocking(&txt)).await.map_err(|e| e.to_string())?,
                None => { emit_nurture_step(app, account_id, "未配置 AI 或生成失败，跳过原创"); Err(String::new()) }
            };
            if r.is_ok() {
                let st2 = app.state::<AppState>(); let l = st2.db.lock();
                if let Ok(conn) = l {
                    let tag = format!("tweet:{}", Local::now().format("%Y-%m-%d"));
                    let _ = x_record_action(&conn, account_id, "tweet", &tag);
                }
                log::info!("[X-NURTURE] 发了 1 条原创");
            }
        }
    }

    // 8) 记录耗时 + 动作（B：命中受限则写 restricted，否则 healthy）
    let elapsed = session_start.elapsed().as_secs() as i64;
    let final_health = aborted_health.as_deref().unwrap_or("healthy");
    {
        let st = app.state::<AppState>();
        let locked = st.db.lock();
        if let Ok(conn) = locked {
            let now = Utc::now().to_rfc3339();
            let today = Local::now().format("%Y-%m-%d").to_string();
            let _ = conn.execute(
                "UPDATE accounts SET nurture_started_at=COALESCE(nurture_started_at,?1), last_nurture_at=?1, \
                 total_nurture_seconds=COALESCE(total_nurture_seconds,0)+?2, health_status=?4, last_health_check=?1 WHERE id=?3",
                params![now, elapsed, account_id, final_health]);
            let _ = conn.execute(
                "INSERT INTO nurture_daily_logs (id, account_id, date, sessions_completed, total_seconds) VALUES (?1,?2,?3,1,?4) \
                 ON CONFLICT(account_id,date) DO UPDATE SET sessions_completed=sessions_completed+1, total_seconds=total_seconds+?4",
                params![Uuid::new_v4().to_string(), account_id, today, elapsed]);
        }
    }
    if let Some(s) = &aborted_health {
        log::warn!("[X-NURTURE] account={} 养号中检测到 {} → 已退避", account_id, s);
    }
    log::info!("[X-NURTURE] account={} kw={} like={} follow={} 耗时={}s health={}", account_id, kw, likes, follows, elapsed, final_health);
    let note = aborted_health.as_deref().map(|s| format!("（{}退避）", s)).unwrap_or_default();
    Ok(format!("X 养号完成：kw={} like={} follow={}{} 用时{}s", kw, likes, follows, note, elapsed))
}

/// 依次尝试一组选择器，命中即点击；全程打日志，便于对照实时 GitHub 校准选择器。
fn gh_click_first(action: &str, url: &str, selectors: &[&str]) -> Result<String, String> {
    log::info!("[GH-ACTION] {} 目标={}", action, url);
    for sel in selectors {
        let exists = unzoo_element_exists(sel);
        log::info!("[GH-ACTION] {} 选择器 '{}' 存在={}", action, sel, exists);
        if exists {
            unzoo_click(sel).map_err(|e| format!("{} 点击失败({}): {}", action, sel, e))?;
            std::thread::sleep(std::time::Duration::from_millis(800));
            log::info!("[GH-ACTION] {} ✓ 已点击 '{}'", action, sel);
            return Ok(sel.to_string());
        }
    }
    Err(format!("{} 未命中任何选择器（已试 {} 个，可能已操作/改版/未登录）", action, selectors.len()))
}

/// 在 repo 页点 Star（已 star 时按钮文案为 Unstar，选择器只匹配未 star 态以免误取消）。
fn gh_star_repo_blocking(repo_url: &str) -> Result<(), String> {
    unzoo_navigate(repo_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5)));
    gh_click_first("star", repo_url, &[
        "button[aria-label^='Star this']",
        "button[aria-label^='Star ']",
        "form[action$='/star'] button",
        ".starring-container.unstarred button",
        "[data-testid='star-button']",
    ]).map(|_| ())
}

/// 在用户 profile 页点 Follow。
fn gh_follow_user_blocking(user_url: &str) -> Result<(), String> {
    unzoo_navigate(user_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5)));
    gh_click_first("follow", user_url, &[
        "form[action$='/follow'] button",
        "button[aria-label^='Follow']",
        "[data-testid='follow-button']",
    ]).map(|_| ())
}

/// 在 repo 页点 Watch。
fn gh_watch_repo_blocking(repo_url: &str) -> Result<(), String> {
    unzoo_navigate(repo_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5)));
    gh_click_first("watch", repo_url, &[
        "button[aria-label*='watch' i]",
        "summary[aria-label*='Notifications']",
        "[data-testid='watch-button']",
    ]).map(|_| ())
}

/// 良性、不带推广意图的短评论（养号阶段建立真人感，绝不带链接/产品）。
pub(crate) fn gh_benign_comment(seed: u64) -> String {
    const POOL: &[&str] = &[
        "Ran into the same thing — thanks for documenting this.",
        "This worked for me, appreciate the write-up.",
        "Nice, the explanation here is really clear.",
        "Confirmed on my side too. Helpful, thanks!",
        "Subscribing — running into something similar.",
        "Great repo, learned a lot reading through this.",
    ];
    POOL[(seed as usize) % POOL.len()].to_string()
}

// ===== X 动作助手（human 模式语义定位；unzoo_click/type 已路由到 human）=====
// 选择器用 X 稳定的 data-testid（CSS 属性选择器），best-effort，需实测校准。

/// 动作失败时抓当前页面文本分类健康风险；命中则返回 "HEALTH:<state>" 供上层退避，否则返回 default。
fn x_fail_health(default: &str) -> String {
    let raw = unzoo_evaluate("(document.body.innerText||'').slice(0,4000)").unwrap_or_default();
    let text = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    match x_classify_health(&text) {
        Some(s) => format!("HEALTH:{}", s),
        None => default.to_string(),
    }
}

/// 点赞某推文（已 Like 的 testid 为 "unlike"，只点 "like" 不取消）。
fn x_like_blocking(tweet_url: &str) -> Result<(), String> {
    unzoo_navigate(tweet_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5)));
    log::info!("[X-ACTION] like 目标={}", tweet_url);
    if unzoo_element_exists("[data-testid=\"like\"]") {
        unzoo_click("[data-testid=\"like\"]").map_err(|e| format!("like 失败: {}", e))?;
        std::thread::sleep(std::time::Duration::from_millis(800));
        return Ok(());
    }
    Err(x_fail_health("未找到 like 按钮（可能已赞/改版/未登录）"))
}

/// People 搜索：按领域词找该领域的账号，返回候选 profile URL 列表。
fn x_search_users_blocking(kw: &str) -> Result<Vec<String>, String> {
    let q = urlencoding::encode(kw); // 同上：hashtag 的 # 必须编码成 %23，否则 q 变空 → 拿到的是无关推荐用户
    unzoo_navigate(&format!("https://x.com/search?q={}&f=user", q))?;
    std::thread::sleep(std::time::Duration::from_secs(4));
    let mut waited = 0;
    while !unzoo_element_exists("[data-testid=\"UserCell\"]") && waited < 10 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        waited += 2;
    }
    let links = unzoo_get_links("[data-testid=\"UserCell\"] a[href^=\"/\"]")?;
    // 规范化为 https://x.com/<handle>（单段路径，排除保留路径）
    let reserved = ["i","search","hashtag","explore","home","notifications","messages","settings","compose"];
    let mut profiles: Vec<String> = links.into_iter().filter_map(|h| {
        let path = h.trim_start_matches("https://x.com").trim_start_matches('/');
        if path.is_empty() || path.contains('/') || path.contains('?') { return None; }
        if reserved.contains(&path) { return None; }
        Some(format!("https://x.com/{}", path))
    }).collect();
    profiles.sort(); profiles.dedup();
    log::info!("[X-ACTION] people 搜索 kw={} 采到 {} 个候选账号", kw, profiles.len());
    Ok(profiles)
}

/// 关注一个「好用户」：在其主页读 简介 + 粉丝数 做质量门，达标才关注。
/// 返回 Ok(true)=已关注 / Ok(false)=跳过(不达标/已关注) / Err=异常(含 HEALTH:)。
fn x_follow_quality_blocking(profile_url: &str) -> Result<bool, String> {
    unzoo_navigate(profile_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5)));
    // 没有 follow 按钮（已关注 / 改版 / 自己）→ 跳过
    if !unzoo_element_exists("[data-testid$=\"-follow\"]") {
        return Ok(false);
    }
    // 读简介
    let bio_raw = unzoo_evaluate("(() => { const e = document.querySelector('[data-testid=\"UserDescription\"]'); return e ? e.innerText : ''; })()").unwrap_or_default();
    let bio = serde_json::from_str::<String>(&bio_raw).unwrap_or(bio_raw);
    // 读粉丝数
    let fol_raw = unzoo_evaluate("(() => { const a = document.querySelector('a[href$=\"/verified_followers\"], a[href$=\"/followers\"]'); return a ? a.innerText : ''; })()").unwrap_or_default();
    let fol = serde_json::from_str::<String>(&fol_raw).unwrap_or(fol_raw);
    let followers = x_parse_count(&fol);
    // 质量门：有简介 且 (粉丝≥300，或粉丝读不出时放行靠 People 排序兜底)
    let has_bio = !bio.trim().is_empty();
    let followers_ok = followers.map(|n| n >= 300).unwrap_or(true);
    if !(has_bio && followers_ok) {
        log::info!("[X-ACTION] follow 跳过(质量不达标) bio={} followers={:?}(原文「{}」) {}", has_bio, followers, fol.trim(), profile_url);
        return Ok(false);
    }
    unzoo_click("[data-testid$=\"-follow\"]").map_err(|e| x_fail_health(&format!("follow 点击失败: {}", e)))?;
    std::thread::sleep(std::time::Duration::from_millis(800));
    log::info!("[X-ACTION] follow ✓ followers={:?} {}", followers, profile_url);
    Ok(true)
}

/// 转推（Repost）某推文。
fn x_retweet_blocking(tweet_url: &str) -> Result<(), String> {
    unzoo_navigate(tweet_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5)));
    log::info!("[X-ACTION] retweet 目标={}", tweet_url);
    if !unzoo_element_exists("[data-testid=\"retweet\"]") {
        return Err("未找到 retweet 按钮".to_string());
    }
    unzoo_click("[data-testid=\"retweet\"]").map_err(|e| format!("retweet 失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(1, 2)));
    unzoo_click("[data-testid=\"retweetConfirm\"]").map_err(|e| format!("retweet 确认失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(800));
    Ok(())
}

/// 发一条原创推文（home compose）。
fn x_post_tweet_blocking(text: &str) -> Result<(), String> {
    unzoo_navigate("https://x.com/compose/post")?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5)));
    log::info!("[X-ACTION] tweet（原创）");
    if !unzoo_element_exists("[data-testid=\"tweetTextarea_0\"]") {
        return Err("未找到发推输入框".to_string());
    }
    unzoo_type("[data-testid=\"tweetTextarea_0\"]", text).map_err(|e| format!("发推输入失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(800));
    unzoo_click("[data-testid=\"tweetButton\"]").map_err(|e| format!("发推发布失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(1000));
    Ok(())
}
