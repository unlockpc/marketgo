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
        if nurture_should_stop() { break; }
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
            if nurture_should_stop() { break; }
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
        if nurture_should_stop() { break; }
        if start.elapsed().as_secs() as i64 >= duration_secs { break; }
        // xorshift 推进选词
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        let kw = &keywords[(seed as usize) % keywords.len()];
        // 再推进一次给后缀，让选词与后缀不同源；主题扩展：领域词后总是拼技术意图后缀。
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        let query = sf_expand_query(kw, seed);
        let q_enc = query.replace(' ', "%20");
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
            if nurture_should_stop() { break; }
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

/// 小红书养号分期强度 → (搜索次数, 是否点赞)。各阶段均点赞；成长期搜索更多。
/// 每轮阅读篇数(3-8)与点赞次数(2-4)在 runner 里随机，不在此固定。
pub(crate) fn xhs_phase_intensity(phase: &str) -> (i64, bool) {
    match phase {
        "growth" => (3, true),
        _ => (2, true), // warmup / mature
    }
}

/// 小红书自动评论配额（按养号分期）：每轮最多 1 条，且只在成长/成熟期评论；预热期只读不评。
/// 评论比点赞风控敏感得多，故全程一轮顶多 1 条。纯逻辑，可单测。
pub(crate) fn xhs_reply_quota(phase: &str) -> i64 {
    match phase {
        "growth" | "mature" => 1,
        _ => 0, // warmup 及兜底：不评论
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

/// X/Twitter 登录检测（DOM 法）：X 是窄窗重 SPA，通用文本法常误判未登录，故走登录标记元素。
/// 导航首页 → 轮询「账号切换 / Home / 发推」标记(=已登录) vs 登录按钮(=未登录)。在 spawn_blocking 中调用。
fn x_logged_in_blocking() -> bool {
    use std::time::Duration;
    let _ = unzoo_navigate("https://x.com/home");
    let mut waited = 0;
    while waited < 18 {
        std::thread::sleep(Duration::from_secs(3));
        waited += 3;
        let logged_in = unzoo_element_exists("[data-testid=\"SideNav_AccountSwitcher_Button\"]")
            || unzoo_element_exists("[data-testid=\"AppTabBar_Home_Link\"]")
            || unzoo_element_exists("[data-testid=\"SideNav_NewTweet_Button\"]");
        if logged_in {
            return true;
        }
        let logged_out = unzoo_element_exists("[data-testid=\"loginButton\"]")
            || unzoo_element_exists("a[href=\"/login\"]");
        if logged_out {
            return false;
        }
    }
    false
}

/// GitHub 登录检测：导航首页后轮询（文本信号可靠，但首屏渲染慢，单次检测会把已登录误判为未登录）。
/// 命中登录后文本即 true；出现「Sign in」入口即 false；最多等约 18s。在 spawn_blocking 中调用。
fn gh_logged_in_blocking() -> bool {
    use std::time::Duration;
    let _ = unzoo_navigate("https://github.com/");
    let mut waited = 0;
    while waited < 18 {
        // check_platform_login_status 内部已 sleep 2s 再读文本，确保读的是渲染后的页面
        if check_platform_login_status("github").unwrap_or(false) {
            return true;
        }
        // 明确未登录：首页存在「Sign in」入口（已登录页无 /login 链接）→ 提前返回，不必等满
        if unzoo_element_exists("a[href=\"/login\"]") {
            return false;
        }
        std::thread::sleep(Duration::from_secs(1));
        waited += 3; // 约 2s(内部读取) + 1s
    }
    false
}

/// 一键养号「未登录预检」覆盖的平台：这些平台的专属 runner 未登录会直接报错失败，
/// 值得开跑前先检测并提示。其它平台走通用滚动、不强依赖登录，不预检。纯逻辑，可单测。
pub(crate) fn nurture_requires_login(platform: &str) -> bool {
    matches!(
        platform.to_lowercase().as_str(),
        "github" | "twitter" | "x" | "segmentfault" | "xiaohongshu" | "redbook" | "weibo"
    )
}

/// 一键养号预检入口：检测账号是否已登录其平台。复用各 runner 自己的登录判定（比通用文本法准），
/// 无专属检测的平台走通用文本法兜底。内部导航站点 + 轮询，需在 spawn_blocking 中调用。
pub(crate) fn platform_logged_in_blocking(platform: &str) -> bool {
    match platform.to_lowercase().as_str() {
        "segmentfault" => sf_logged_in_blocking(),
        "xiaohongshu" | "redbook" => xhs_logged_in_blocking(),
        "twitter" | "x" => x_logged_in_blocking(),
        "github" => gh_logged_in_blocking(),
        "weibo" => weibo_logged_in_blocking(),
        other => verify_login_blocking(other),
    }
}

/// 强制 human 点击：小红书很多元素(封面 a.cover 被自身 mask 遮挡、搜索图标含 svg、点赞 wrapper、
/// 轮播箭头)会被遮挡检测拦下，需 force 才能点中。成功返回 true。
fn xhs_force_click(selector: &str) -> bool {
    let tab_id = match get_active_tab() { Some(t) if !t.is_empty() => t, _ => return false };
    ensure_human_profile();
    unzoo_mcp("human_click", serde_json::json!({ "tab_id": tab_id, "selector": selector, "force": true })).is_ok()
}

/// 在当前笔记弹框点赞。注意：弹框内有大量评论的 `.like-wrapper`(实测一篇 79 个)，
/// 笔记主点赞精确选择器是 `.engage-bar .like-wrapper`(唯一)，必须优先，否则会误点评论赞。
fn xhs_like_blocking() -> bool {
    let selectors = [".engage-bar .like-wrapper", ".note-detail-mask .engage-bar .like-wrapper", ".interaction-container > .left .like-wrapper"];
    for s in selectors {
        if unzoo_element_exists(s) {
            return xhs_force_click(s);
        }
    }
    false
}

/// 多图笔记：随机点几下「下一张」翻图，更像真人。单图/视频笔记没有箭头(.arrow-controller.right)→直接跳过。
/// 到末张箭头变 .arrow-controller.right.forbidden，命中即停。
fn xhs_browse_images_blocking() {
    use std::time::Duration;
    if !unzoo_element_exists(".arrow-controller.right") { return; } // 非多图(单图/视频)
    let times = get_human_delay(1, 3); // 随机翻 1~3 张
    for _ in 0..times {
        if !unzoo_element_exists(".arrow-controller.right") { break; }
        if unzoo_element_exists(".arrow-controller.right.forbidden") { break; } // 已到末张
        if !xhs_force_click(".arrow-controller.right") { break; }
        std::thread::sleep(Duration::from_millis(get_human_delay(1500, 3500))); // 看一张图停一会
        random_mouse_movement();
    }
}

/// 在搜索结果页上「点击」第 idx 张笔记卡片（idx 从 1 起）打开弹框——human force 点击
/// (封面 a.cover 常被自身 mask 遮挡，必须 force)，触发小红书弹框逻辑。点击成功返回 true。
///
/// 只认含 `a.cover` 的「真笔记」：瀑布流里混着 `.query-note-wrapper`(「大家都在搜/相关搜索」卡片，
/// 同为 section.note-item 但无 a.cover)，绝不能点——点中会触发搜索而非开帖。命中这种索引直接返回 false 跳过。
fn xhs_open_note_blocking(idx: i64) -> bool {
    let cands = [
        format!("section.note-item:nth-of-type({}) a.cover", idx),
        format!(".feeds-container section:nth-of-type({}) a.cover", idx),
    ];
    for c in &cands {
        if unzoo_element_exists(c) && xhs_force_click(c) {
            return true;
        }
    }
    false
}

/// 关闭当前笔记弹框：优先 force 点关闭按钮，兜底按 Esc。关后回到搜索结果页（不丢上下文）。
fn xhs_close_note_blocking() {
    let close_sels = [".close-circle", ".note-detail-mask .close", ".close-box .close"];
    for s in &close_sels {
        if unzoo_element_exists(s) && xhs_force_click(s) {
            return;
        }
    }
    // 兜底：按 Esc 关弹框
    let tab_id = get_active_tab().unwrap_or_default();
    if !tab_id.is_empty() {
        let _ = unzoo_mcp("browser_press_key", serde_json::json!({ "tab_id": tab_id, "key": "Escape" }));
    }
}

/// 读当前打开的笔记正文（标题+描述），给大模型生成切题评论用。`.note-content` 唯一，含标题(#detail-title)+
/// 描述(#detail-desc)。trim 后字符数 ≥10 才返回 Some——太短(纯图无文字)的笔记不评论，由此兜底。
fn xhs_read_note_text_blocking() -> Option<String> {
    let raw = unzoo_evaluate("(function(){var e=document.querySelector('.note-content');return e?(e.innerText||''):'';})()").ok()?;
    let text = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    let t = text.trim();
    if t.chars().count() >= 10 { Some(t.chars().take(600).collect()) } else { None }
}

/// 评论框发送核心（真站验证流程 2026-06-29，已纠正为真实键盘）：
/// 调用前提：评论框**已被真实点击激活**（占位浮层「说点什么…」已隐藏、#content-textarea 已聚焦无遮挡）。
/// 1) **真实键盘输入**(browser_type，每字符真实 WebKeyboardEvent，CJK 走 IME)。小红书提交逻辑只认真实键盘事件——
///    `execCommand('insertText')` 虽能点亮按钮但提交不认(和 X 同坑，时好时坏)，故弃用；
/// 2) 校验按钮已激活(仍 gray/disabled → 落字未被识别 → 失败)；
/// 3) force 点 `.btn.submit` 发送；
/// 4) 校验 #content-textarea 已清空/消失(发出后会复位/收起)，否则疑似未发出 → 失败。
/// 笔记评论与楼中回复共用此核心。任一步失败返回 Err（调用方据此跳过、不记库，避免假成功）。
fn xhs_submit_comment_box_blocking(text: &str) -> Result<(), String> {
    use std::time::Duration;
    let tab_id = get_active_tab().filter(|t| !t.is_empty()).ok_or_else(|| "无活动标签页".to_string())?;
    std::thread::sleep(Duration::from_millis(get_human_delay(400, 900)));
    // 1) 真实键盘输入（box 已激活、占位浮层已隐藏，browser_type 聚焦点击可命中输入框；instant=false → 逐字真实键盘+IME）
    unzoo_mcp("browser_type", serde_json::json!({
        "tab_id": tab_id, "selector": "#content-textarea", "text": text,
        "instant": false, "timeout": 8000
    })).map_err(|e| format!("评论输入失败: {}", e))?;
    std::thread::sleep(Duration::from_millis(get_human_delay(800, 1600)));
    // 2) 校验发送按钮已激活(仍 gray/disabled → 落字未被识别 → 失败，绝不假成功)
    let enabled_js = "(function(){var b=document.querySelector('.btn.submit');if(!b)return 'nobtn';return (b.disabled||/\\bgray\\b/.test(b.className))?'disabled':'enabled';})()";
    let st = unzoo_evaluate(enabled_js)?;
    if !st.contains("enabled") {
        return Err(format!("小红书评论未发出：发送按钮不可用(落字未被识别，state={})", st.trim()));
    }
    // 3) 发送(force 点，按钮含 svg/被样式包裹易判遮挡)
    if !xhs_force_click(".btn.submit") {
        return Err("点击发送失败".into());
    }
    std::thread::sleep(Duration::from_secs(3));
    // 4) 校验已发出：评论框应清空或消失(发出后会复位/收起)
    let posted_js = "(function(){var el=document.querySelector('#content-textarea');if(!el)return 'posted';var t=(el.innerText||'').replace(/\\s/g,'');return t===''?'posted':'stuck';})()";
    let pv = unzoo_evaluate(posted_js)?;
    if pv.contains("stuck") {
        return Err("小红书已点发送但评论框未清空，疑似未发出".into());
    }
    Ok(())
}

/// 给笔记本身发评论：先**真实点击占位浮层「说点什么…」(`.inner-when-not-active`)激活评论框**——
/// 这才能让小红书进入真正编辑态、隐藏占位浮层(否则文字会和占位符叠在一起、提交也不认；这是实测踩坑点)。
/// `.click-area`/`.content-edit` 作兜底(不同笔记结构不一)。激活后共用发送核心(真实键盘输入)。
fn xhs_comment_blocking(text: &str) -> Result<(), String> {
    if !unzoo_element_exists("#content-textarea") {
        return Err("未找到评论框(#content-textarea)".into());
    }
    if !xhs_force_click(".inner-when-not-active") && !xhs_force_click(".click-area") && !xhs_force_click(".content-edit") {
        return Err("激活评论框失败".into());
    }
    xhs_submit_comment_box_blocking(text)
}

/// 回复楼里【别人的某条评论】：force 点该评论的「回复」按钮(`#<comment_id> .reply.icon-container`)——
/// 实测点后焦点会落到底部那个【唯一】的 #content-textarea，XHS 用 Vue state 内部记住回复目标 → 共用发送核心。
/// comment_id 形如 "comment-6a42..."（已含 comment- 前缀）。
fn xhs_reply_comment_blocking(comment_id: &str, text: &str) -> Result<(), String> {
    use std::time::Duration;
    let reply_btn = format!("#{} .reply.icon-container", comment_id);
    if !unzoo_element_exists(&reply_btn) {
        return Err(format!("未找到该评论的回复按钮({})", reply_btn));
    }
    if !xhs_force_click(&reply_btn) {
        return Err("点击评论回复按钮失败".into());
    }
    std::thread::sleep(Duration::from_millis(get_human_delay(800, 1500)));
    if !unzoo_element_exists("#content-textarea") {
        return Err("回复框未出现(#content-textarea)".into());
    }
    xhs_submit_comment_box_blocking(text)
}

/// 判断一条评论是否为「灌水」（无价值、不值得回复）。纯逻辑、可单测，作为调 AI 前的廉价预筛。
/// 命中任一即视为灌水：太短(<5字)、去掉@提及后实义不足、纯表情/标点(无中文且无字母数字)、命中空泛套话黑名单。
pub(crate) fn xhs_is_filler_comment(text: &str) -> bool {
    let t = text.trim();
    let chars = t.chars().count();
    if chars < 5 { return true; }
    // 去掉所有 "@昵称" 段后看剩余实义内容（纯 @某人 的评论实义为空）
    let mut without_at = String::new();
    let mut skipping = false;
    for c in t.chars() {
        if c == '@' { skipping = true; continue; }
        if skipping { if c.is_whitespace() { skipping = false; without_at.push(c); } continue; }
        without_at.push(c);
    }
    if without_at.trim().chars().count() < 3 { return true; }
    // 纯表情/标点：既无中文也无字母数字 → 无实义
    let has_meaning = t.chars().any(|c| c.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&c));
    if !has_meaning { return true; }
    // 套话黑名单：去空白/标点后整条等于套话，或很短(≤8字)且含套话
    let compact: String = t.chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation() && !"，。！？、；：…~～·「」".contains(*c))
        .collect();
    let low = compact.to_lowercase();
    const FILLER: &[&str] = &[
        "学到了", "感谢分享", "谢谢分享", "谢谢", "支持", "支持一下", "码住", "马住", "收藏", "已收藏",
        "沙发", "打卡", "顶", "mark", "马克", "路过", "好的", "不错", "厉害", "厉害了", "太棒了",
        "赞", "已赞", "期待", "催更", "蹲", "蹲一个", "蹲后续", "666", "牛", "牛逼", "yyds", "哈哈", "哈哈哈", "可以",
    ];
    for f in FILLER {
        let fl = f.to_lowercase();
        if low == fl { return true; }
        if chars <= 8 && low.contains(&fl) { return true; }
    }
    false
}

/// 读当前登录用户自己的 user id（从侧栏「我」入口的 /user/profile/<id> 取），用于跳过回复自己的评论。
/// 读不到返回 None（此时不做自跳过，靠去重 + AI 判定兜底）。
fn xhs_my_user_id_blocking() -> Option<String> {
    let js = "(function(){var ls=document.querySelectorAll('a[href*=\"/user/profile/\"]');\
        for(var i=0;i<ls.length;i++){if(/我/.test(ls[i].innerText||'')){var m=(ls[i].getAttribute('href')||'').match(/\\/user\\/profile\\/([0-9a-f]+)/);if(m)return m[1];}}return '';})()";
    let raw = unzoo_evaluate(js).ok()?;
    let id = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    let id = id.trim().to_string();
    if id.is_empty() { None } else { Some(id) }
}

/// 读当前打开笔记的主楼评论（`.parent-comment`），返回每条 (comment_id, author_user_id, is_author, text)。
/// comment_id 含 "comment-" 前缀，可直接拼回复按钮选择器；author_user_id 用于跳过自己；is_author 标识笔记作者。
fn xhs_read_parent_comments_blocking() -> Vec<(String, String, bool, String)> {
    let js = "(function(){var ps=document.querySelectorAll('.parent-comment');var out=[];\
        for(var i=0;i<ps.length;i++){var p=ps[i];var item=p.querySelector('.comment-item')||p;\
        var a=p.querySelector('.author');var c=p.querySelector('.content');var uidEl=p.querySelector('[data-user-id]');\
        out.push({id:item.id||'',uid:uidEl?uidEl.getAttribute('data-user-id'):'',isAuthor:/作者/.test(a?a.innerText:''),text:(c?(c.innerText||''):'').trim()});}\
        return JSON.stringify(out);})()";
    let raw = match unzoo_evaluate(js) { Ok(r) => r, Err(_) => return Vec::new() };
    let inner = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    let arr: Vec<serde_json::Value> = serde_json::from_str(&inner).unwrap_or_default();
    arr.into_iter().filter_map(|v| {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if id.is_empty() { return None; }
        let uid = v.get("uid").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let is_author = v.get("isAuthor").and_then(|x| x.as_bool()).unwrap_or(false);
        let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
        Some((id, uid, is_author, text))
    }).collect()
}

/// 在当前页搜索框输入主题并提交——比导航 search_result URL 更像真人，且不开新 tab、URL 不带 type 字段。
/// 实测流程(2026-06-26 真站验证)：聚焦 #search-input → JS 全选 + Backspace 清空(React 受控框 Meta+A 选不中，
/// setSelectionRange 才可靠) → human 输入 → 强点 .input-box .search-icon 提交(图标含 svg 会判定遮挡，需 force)。
/// 提交成功返回 true。
fn xhs_search_box_blocking(kw: &str) -> bool {
    use std::time::Duration;
    let tab_id = match get_active_tab() { Some(t) if !t.is_empty() => t, _ => return false };
    if !unzoo_element_exists("#search-input") { return false; }
    // 聚焦搜索框
    let _ = unzoo_human_click("#search-input");
    std::thread::sleep(Duration::from_millis(get_human_delay(400, 900)));
    // 清空已有关键词：JS 选中全部 + 退格删除
    let _ = unzoo_evaluate("(function(){var el=document.querySelector('#search-input');if(!el)return 0;el.focus();el.setSelectionRange(0,(el.value||'').length);return (el.value||'').length;})()");
    let _ = unzoo_mcp("browser_press_key", serde_json::json!({ "tab_id": tab_id, "key": "Backspace" }));
    std::thread::sleep(Duration::from_millis(get_human_delay(300, 700)));
    // human 输入主题词
    if unzoo_human_type("#search-input", kw).is_err() { return false; }
    std::thread::sleep(Duration::from_millis(get_human_delay(500, 1200)));
    // 提交：搜索图标含 svg 会被遮挡检测拦下，force 强点
    ensure_human_profile();
    unzoo_mcp("human_click", serde_json::json!({ "tab_id": tab_id, "selector": ".input-box .search-icon", "force": true })).is_ok()
}

/// 主题扩展：在主题词后**总是**随机叠加一个意图后缀(推荐/测评/教程…)，不再直接搜光秃秃的主题词。
/// 让每次搜索更自然多样；对内置和自定义主题都通用。
pub(crate) fn xhs_expand_query(topic: &str, seed: u64) -> String {
    const MODS: &[&str] = &["推荐", "测评", "教程", "分享", "好物", "攻略", "干货", "盘点", "种草", "怎么样", "最新", "实测"];
    let m = MODS[(seed as usize) % MODS.len()];
    format!("{} {}", topic, m)
}

/// 思否主题扩展：中文技术社区，给领域词总是拼一个技术意图后缀(教程/实战/原理…)，不直接搜光秃秃的领域词。
pub(crate) fn sf_expand_query(topic: &str, seed: u64) -> String {
    const MODS: &[&str] = &["教程", "实战", "原理", "入门", "最佳实践", "源码", "报错", "面试", "踩坑", "进阶"];
    let m = MODS[(seed as usize) % MODS.len()];
    format!("{} {}", topic, m)
}

/// X 主题扩展：英文社区。hashtag(以 # 开头)保持原样不拼(否则破坏话题流)；
/// 普通词总是拼一个英文意图后缀(tutorial/tips/explained…)，让浏览更聚焦该方向。
pub(crate) fn x_expand_query(kw: &str, seed: u64) -> String {
    let kw = kw.trim();
    if kw.starts_with('#') { return kw.to_string(); }
    const MODS: &[&str] = &["tutorial", "tips", "guide", "explained", "review", "examples", "basics", "news", "trends", "best practices"];
    let m = MODS[(seed as usize) % MODS.len()];
    format!("{} {}", kw, m)
}

/// X 自动回复配额（按养号分期）：预热 1 / 成长 1 / 成熟 2；兜底=1。纯逻辑，可单测。
pub(crate) fn x_reply_quota(phase: &str) -> i64 {
    match phase {
        "growth" => 1,
        "mature" => 2,
        _ => 1, // warmup 及兜底
    }
}

/// 清洗推文正文并过长度门：trim 后非空且字符数 ≥15 才返回 Some，否则 None。
/// 读不到/太短(纯图/视频/转发无文字)的推文不回复，由此兜底。纯逻辑，可单测。
pub(crate) fn x_clean_tweet_text(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.chars().count() >= 15 { Some(t.to_string()) } else { None }
}

/// 打开推文详情页，读主推文正文。读到非空且够长(见 x_clean_tweet_text)才返回 Some。
/// 在 spawn_blocking 中调用。选择器 `article [data-testid="tweetText"]` 已实测稳定拿主推文、不混评论。
fn x_read_tweet_text_blocking(tweet_url: &str) -> Option<String> {
    use std::time::Duration;
    if unzoo_navigate(tweet_url).is_err() { return None; }
    // 轮询正文元素出现，最多 ~8s
    let mut waited = 0;
    while !unzoo_element_exists("article [data-testid=\"tweetText\"]") && waited < 8 {
        std::thread::sleep(Duration::from_secs(2));
        waited += 2;
    }
    let raw = unzoo_evaluate(
        "(function(){var e=document.querySelector('article [data-testid=\"tweetText\"]');return e?e.innerText:'';})()"
    ).unwrap_or_default();
    let text = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    x_clean_tweet_text(&text)
}

/// 小红书养号（搜索驱动）：按主题关键词搜索→拟人浏览→点进笔记阅读；成长期对少量笔记点赞；
/// 开关开时对**一篇**读过的笔记自动评论(读正文→大模型生成切题评论→execCommand 注入并发送)，
/// 并对楼里**一条**别人的非灌水评论自动回复(启发式预筛灌水 + AI 判定值不值得回)。
/// 全程"等加载+随机延迟"再操作。返回 (searched, read, liked, replied, creplied)。
/// app/account_id 用于点赞/评论去重(xhs_actions_log)与进度推送。reply_on/reply_quota/creply_quota 控制自动评论与楼中回复。
fn xhs_nurture_browse_blocking(app: AppHandle, account_id: &str, keywords: Vec<String>, n_like: i64, reply_on: bool, reply_quota: i64, creply_quota: i64, reply_style: String, duration_secs: i64, seed0: u64) -> Result<(i64, i64, i64, i64, i64), String> {
    use std::time::{Duration, Instant};
    let start = Instant::now();
    if keywords.is_empty() { return Ok((0, 0, 0, 0, 0)); }
    if !xhs_logged_in_blocking() {
        return Err("未登录小红书！请先点卡片上「✋ 手工登录」在浏览器里登一次，再养号。".to_string());
    }
    let mut searched = 0i64; let mut read = 0i64; let mut liked = 0i64; let mut replied = 0i64; let mut creplied = 0i64;
    // 本轮(整 session)已点开读过的笔记 URL：避免反复点开同一帖——搜索结果常有重复卡片，且关闭弹框后 feed
    // 会重排/懒加载，导致按 nth-of-type 序号点会错位命中已读过的笔记。跨主题搜索也共用此集合去重。
    let mut opened_urls: std::collections::HashSet<String> = std::collections::HashSet::new();
    // 自己的 user id：用于楼中回复时跳过回复自己（读不到则靠去重 + AI 判定兜底）。开关关时不必读。
    let my_uid = if reply_on { xhs_my_user_id_blocking().unwrap_or_default() } else { String::new() };
    let mut seed = seed0 | 1;
    // 把所有选中主题打乱后逐个搜索，确保每个主题(含自定义)都轮到——
    // 之前随机取模 keywords[seed%len] 一个 session 只搜 n_search 次，排在后面的自定义主题常被漏掉。
    let mut kw_order = keywords.clone();
    for i in (1..kw_order.len()).rev() {
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        let j = (seed as usize) % (i + 1);
        kw_order.swap(i, j);
    }
    for (si, kw) in kw_order.iter().enumerate() {
        if nurture_should_stop() { break; }
        if start.elapsed().as_secs() as i64 >= duration_secs { break; }
        // 主题扩展：随机叠加意图后缀(推荐/测评/教程…)，让搜索词更自然多样，不千篇一律搜同一个词。
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        let query = xhs_expand_query(kw, seed);
        if si == 0 {
            // 首次搜索：从 explore 首页出发，其搜索框是「问点」AI 框、提交图标也不同，box 流程不适用。
            // 先用 URL 落到搜索结果页(建立一致的搜索栏 UI)，之后换主题都走搜索框。
            // 带 source=web_search_result_notes：与 box 搜索一致，避免被小红书重定向补 type=51。
            emit_nurture_step(&app, account_id, &format!("🔍 搜索「{}」", query));
            let url = format!("https://www.xiaohongshu.com/search_result?keyword={}&source=web_search_result_notes", query.replace(' ', "%20"));
            if unzoo_navigate(&url).is_err() { continue; }
        } else {
            // 换主题 → 在当前搜索结果页的搜索框输入并提交（同一 tab、不开新 tab、URL 不带 type）
            emit_nurture_step(&app, account_id, &format!("🔍 搜索框搜索「{}」", query));
            if !xhs_search_box_blocking(&query) {
                // 搜索框兜底：搜索框不可用时退回 URL 导航(同样带 source,不带 type)，保证不整体卡死
                let url = format!("https://www.xiaohongshu.com/search_result?keyword={}&source=web_search_result_notes", query.replace(' ', "%20"));
                if unzoo_navigate(&url).is_err() { continue; }
            }
        }
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
        // 本轮随机阅读 3-8 篇；各阶段配额内点赞。卡片数量用于限定上界，避免点到不存在的序号。
        let card_count = unzoo_get_links("a[href*=\"/explore/\"]").map(|v| v.len() as i64).unwrap_or(0);
        let total_cards = card_count.max(1);
        let read_per_search = get_human_delay(3, 8).min(total_cards as u64) as i64;
        // 打乱卡片顺序（Fisher-Yates，复用 xorshift seed），别每次都从第 1 张顺序点 —— 真人是随机翻看。
        let mut order: Vec<i64> = (1..=total_cards).collect();
        for i in (1..order.len()).rev() {
            seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
            let j = (seed as usize) % (i + 1);
            order.swap(i, j);
        }
        let mut opened = 0i64;
        for &idx in &order {
            if opened >= read_per_search { break; }
            if nurture_should_stop() { break; }
            if start.elapsed().as_secs() as i64 >= duration_secs { break; }
            // 在搜索页上「点击」第 idx 张卡片打开弹框（human 真实点击，触发小红书弹框逻辑）
            if !xhs_open_note_blocking(idx) { continue; }
            // 等弹框加载完再操作；加载不出就关掉跳过这篇
            if !xhs_wait_loaded_blocking(".note-detail-mask, #noteContainer, .note-content", 8) {
                xhs_close_note_blocking();
                std::thread::sleep(Duration::from_millis(get_human_delay(800, 1500)));
                continue;
            }
            std::thread::sleep(Duration::from_millis(get_human_delay(2500, 4000)));
            // 弹框打开后小红书会把 URL 更新为 /explore/<id>，取来做跨 session 点赞去重
            let note_url = unzoo_evaluate("location.href").unwrap_or_default();
            let dedup_key = if note_url.contains("/explore/") { note_url } else { String::new() };
            // 本轮已读过这篇 → 立刻关掉跳过，不重复读/赞/评（serps 有重复卡片、关闭后 feed 重排会让序号错位命中同一帖）
            if !dedup_key.is_empty() && !opened_urls.insert(dedup_key.clone()) {
                xhs_close_note_blocking();
                std::thread::sleep(Duration::from_millis(get_human_delay(800, 1500)));
                continue;
            }
            // 弹框内「停留阅读」——不滚动（弹窗里滚动不像真人），只随机停几秒 + 少量鼠标移动
            for _ in 0..get_human_delay(2, 4) {
                std::thread::sleep(Duration::from_millis(get_human_delay(1800, 4000)));
                random_mouse_movement();
            }
            // 多图笔记：随机翻几张图（单图/视频自动跳过）
            xhs_browse_images_blocking();
            read += 1; opened += 1;
            // 点赞：不是每篇都点（约 40% 概率），且配额内 + 未赞过；点赞要随机分散更像真人。
            seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
            let like_roll = (seed % 100) as i64;
            if liked < n_like && like_roll < 40 {
                let already = if dedup_key.is_empty() { false } else {
                    let st = app.state::<AppState>();
                    let locked = st.db.lock();
                    match locked { Ok(c) => xhs_already_acted(&c, account_id, &dedup_key), Err(_) => true }
                };
                if !already {
                    std::thread::sleep(Duration::from_millis(get_human_delay(2000, 5000))); // 加载后随机停几秒再点赞
                    if xhs_like_blocking() {
                        liked += 1;
                        if !dedup_key.is_empty() {
                            let st = app.state::<AppState>();
                            let locked = st.db.lock();
                            if let Ok(c) = locked { let _ = xhs_record_action(&c, account_id, "like", &dedup_key); }
                        }
                        emit_nurture_step(&app, account_id, &format!("👍 点赞 {}/{}", liked, n_like));
                        std::thread::sleep(Duration::from_millis(get_human_delay(1500, 3000))); // 点后 settle
                    }
                }
            }
            // 笔记正文：评论/楼中回复都要用，读一次复用（开关关时不读，省一次 JS 调用）。
            let note_body = if reply_on { xhs_read_note_text_blocking() } else { None };
            // 自动评论（开关开 + 配额内 + 有笔记 URL 可去重 + 未评过这篇）：基于正文→大模型生成切题评论→注入并发送。
            // 风险动作：默认关；读不到正文/生成不合格/发送失败都跳过不记库（避免假成功）；一轮顶多 1 条。
            if reply_on && replied < reply_quota && !dedup_key.is_empty() {
                let comment_key = format!("{}#comment", dedup_key);
                let already_cmt = {
                    let st = app.state::<AppState>();
                    let locked = st.db.lock();
                    match locked { Ok(c) => xhs_already_acted(&c, account_id, &comment_key), Err(_) => true }
                };
                if !already_cmt {
                    // 读不到正文/太短 → 跳过，不评论
                    if let Some(body) = note_body.as_ref() {
                        // 大模型基于正文生成中文切题评论（无 key/不合格 → None → 跳过）。
                        // 在 spawn_blocking 线程里用 block_on 驱动这个 async 调用（非 runtime worker 线程，安全）。
                        let reply = tauri::async_runtime::block_on(gen_nurture_text(&app, "xhs_reply", body, &reply_style));
                        match reply {
                            Some(r) => {
                                emit_nurture_step(&app, account_id, &format!("💬 评论 {}/{}", replied + 1, reply_quota));
                                std::thread::sleep(Duration::from_millis(get_human_delay(2000, 5000))); // 评论前随机停几秒
                                match xhs_comment_blocking(&r) {
                                    Ok(_) => {
                                        replied += 1;
                                        let st = app.state::<AppState>();
                                        if let Ok(c) = st.db.lock() { let _ = xhs_record_action(&c, account_id, "comment", &comment_key); }
                                        emit_nurture_step(&app, account_id, "💬 评论已发出");
                                        std::thread::sleep(Duration::from_millis(get_human_delay(2000, 4000))); // 发后 settle
                                    }
                                    Err(e) => { emit_nurture_step(&app, account_id, &format!("评论发送失败，跳过：{}", e)); }
                                }
                            }
                            None => { emit_nurture_step(&app, account_id, "未配置 AI 或评论不合格，跳过评论"); }
                        }
                    }
                }
            }
            // 楼中回复（开关开 + 配额内 + 有笔记 URL 可去重）：读主楼评论→启发式刷掉灌水→AI 判定值不值得回→回复。
            // 只回主楼、跳过笔记作者与自己；灌水(启发式或 AI 判 SKIP)不回；一轮顶多 1 条。
            if reply_on && creplied < creply_quota && !dedup_key.is_empty() {
                let note_ctx = note_body.as_deref().unwrap_or("");
                for (cid, uid, is_author, ctext) in xhs_read_parent_comments_blocking() {
                    if creplied >= creply_quota { break; }
                    if nurture_should_stop() { break; }
                    if is_author { continue; }                              // 跳过笔记作者的评论
                    if !my_uid.is_empty() && uid == my_uid { continue; }    // 跳过自己的评论
                    if xhs_is_filler_comment(&ctext) { continue; }          // 启发式预筛：明显灌水直接跳过，不调 AI
                    let creply_key = format!("{}#creply:{}", dedup_key, cid);
                    let already = {
                        let st = app.state::<AppState>();
                        let locked = st.db.lock();
                        match locked { Ok(c) => xhs_already_acted(&c, account_id, &creply_key), Err(_) => true }
                    };
                    if already { continue; }
                    // AI 判定+生成：灌水 → 返回 SKIP；有价值 → 一句切题回复
                    let ai_ctx = format!("笔记内容：\n{}\n\n这条评论：\n{}", note_ctx, ctext);
                    let gen = tauri::async_runtime::block_on(gen_nurture_text(&app, "xhs_creply", &ai_ctx, &reply_style));
                    let reply = match gen { Some(r) => r, None => { continue; } };
                    let rt = reply.trim();
                    if rt.is_empty() || rt.eq_ignore_ascii_case("skip") || rt.to_uppercase().starts_with("SKIP") {
                        emit_nurture_step(&app, account_id, "该评论判为灌水/无需回复，跳过");
                        continue;
                    }
                    let preview: String = ctext.chars().take(10).collect();
                    emit_nurture_step(&app, account_id, &format!("💬 回复评论「{}…」", preview));
                    std::thread::sleep(Duration::from_millis(get_human_delay(2000, 5000))); // 回复前随机停几秒
                    match xhs_reply_comment_blocking(&cid, rt) {
                        Ok(_) => {
                            creplied += 1;
                            let st = app.state::<AppState>();
                            if let Ok(c) = st.db.lock() { let _ = xhs_record_action(&c, account_id, "creply", &creply_key); }
                            emit_nurture_step(&app, account_id, "💬 楼中回复已发出");
                            std::thread::sleep(Duration::from_millis(get_human_delay(2000, 4000))); // 发后 settle
                        }
                        Err(e) => { emit_nurture_step(&app, account_id, &format!("楼中回复发送失败，跳过：{}", e)); }
                    }
                }
            }
            // 关弹框回到搜索结果页，再读下一张卡片
            xhs_close_note_blocking();
            std::thread::sleep(Duration::from_millis(get_human_delay(1500, 3000)));
        }
        std::thread::sleep(Duration::from_millis(get_human_delay(2000, 4000)));
    }
    Ok((searched, read, liked, replied, creplied))
}

/// 小红书养号入口：读主题 + 分期 → 搜索驱动浏览(+成长期点赞) → 写养号统计。未选主题 → 跳过提示。
pub(crate) async fn xiaohongshu_nurture_run(app: &AppHandle, account_id: &str, duration: i64) -> Result<String, String> {
    let session_start = std::time::Instant::now();
    let (topics, kws, phase) = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let topics = account_topics(&conn, account_id);
        let kws = account_topic_keywords(&conn, account_id);
        let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
        let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
        let strat = conn.query_row("SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max FROM nurture_strategies WHERE platform='xiaohongshu'",
            [], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?, r.get::<_,i64>(2)?, r.get::<_,i64>(3)?))).ok();
        let (warmup, growth, smin, smax) = strat.unwrap_or((7, 5, 1, 2));
        let (phase, _t) = nurture_phase_and_target(age, warmup, growth, smin, smax);
        (topics, kws, phase.to_string())
    };
    if topics.is_empty() {
        return Ok("账号未选主题，跳过小红书养号（点卡片上「🎯 主题」选一下方向）".to_string());
    }
    if kws.is_empty() { return Ok("主题无可用关键词".to_string()); }
    let (_n_search, allow_like) = xhs_phase_intensity(&phase);
    // 成长/成熟期点赞随机 1-2 次（预热只读 → 0）。刻意压低：新号一轮点太多赞易触发风控。
    // 注意用 get_human_delay（返回原值 1-2）而非 get_random_delay（返回毫秒 1000-2000）——
    // 后者会让点赞配额≈数千、形同不限量（曾让 4 天新号一早上点 25 个赞），是个老 bug。
    let n_like = if allow_like { get_human_delay(1, 2) as i64 } else { 0 };
    // 自动评论 + 楼中回复：开关开 + 分期允许(成长/成熟=1，预热=0)。
    // 一轮养号只随机做其中【一个】——要么评论笔记、要么回复楼里某条评论，不必两样都做(更像真人、也更克制)。
    let base_q = xhs_reply_quota(&phase);
    let (reply_quota, creply_quota) = if base_q > 0 {
        if get_human_delay(0, 1) == 0 { (base_q, 0) } else { (0, base_q) }
    } else {
        (0, 0)
    };
    let (reply_on, reply_style) = {
        let st = app.state::<AppState>();
        let locked = st.db.lock();
        match locked {
            Ok(c) => (crate::xhs_reply_enabled(&c), crate::account_reply_style(&c, account_id)),
            Err(_) => (false, "sincere".to_string()),
        }
    };
    let dur = duration.max(30);
    let seed0 = get_random_delay(1, 100_000);
    let topic_n = kws.len();
    emit_nurture_step(app, account_id, &format!("开始小红书养号 · 逐个搜索 {} 个主题并阅读（约 {}s）", topic_n, dur));
    let app_cl = app.clone();
    let acct = account_id.to_string();
    let (searched, read, liked, replied, creplied) = tauri::async_runtime::spawn_blocking(move || xhs_nurture_browse_blocking(app_cl, &acct, kws, n_like, reply_on, reply_quota, creply_quota, reply_style, dur, seed0))
        .await.map_err(|e| format!("养号任务异常: {}", e))??;

    // 写养号统计（与 SF 一致）
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
    log::info!("[XHS-NURTURE] account={} phase={} 搜索={} 阅读={} 点赞={} 评论={} 楼中回复={} 耗时={}s", account_id, phase, searched, read, liked, replied, creplied, elapsed_secs);
    let cmt_note = if reply_on { format!(" · 评论 {} · 楼中回复 {}", replied, creplied) } else { String::new() };
    Ok(format!("小红书养号完成（{}）：搜索 {} 次 · 阅读 {} 篇 · 点赞 {}{} · 用时 {}s", phase, searched, read, liked, cmt_note, elapsed_secs))
}

// ===== 微博养号（搜索驱动：s.weibo.com 搜索领域词 → 采集卡片 mid → 就地点赞 + 滚动浏览）=====
// 端选型说明：微博全站搜索结果只有 s.weibo.com 这一个落地页（首页搜索框点回车也是跳到这里）。
// s.weibo.com 保留了旧版带 action-type/mid 的稳定语义 DOM，比首页 woo-* 动态混淆类、比 H5 containerid 路由
// 都好抓，且点赞按钮自带 mid → 天然去重 key。故养号直接导航该页就地点赞，与 X 的 x.com/search 思路一致。

/// 微博登录检测：导航 weibo.com，轮询已登录标记(搜索框/信息流卡片) vs 未登录(跳转登录页)。最多约 18s。
fn weibo_logged_in_blocking() -> bool {
    use std::time::Duration;
    let _ = unzoo_navigate("https://weibo.com/");
    let mut waited = 0;
    while waited < 18 {
        std::thread::sleep(Duration::from_secs(3));
        waited += 3;
        // 已登录：首页信息流文章卡片 / 顶栏搜索框（登录后才渲染）
        if unzoo_element_exists("article") || unzoo_element_exists("input.woo-input-main") {
            return true;
        }
        // 未登录：跳到登录页 / passport
        let raw = unzoo_evaluate("location.href").unwrap_or_default();
        let href = serde_json::from_str::<String>(&raw).unwrap_or(raw);
        if href.contains("/login") || href.contains("passport.weibo") || href.contains("signin") {
            return false;
        }
    }
    false
}

/// 采集当前 s.weibo.com 搜索结果页的卡片 mid 列表（稳定属性 div.card-wrap[mid]）。读不到返回空。
fn weibo_collect_mids_blocking() -> Vec<String> {
    let raw = unzoo_evaluate(
        "(function(){var cs=document.querySelectorAll('div.card-wrap[action-type=\"feed_list_item\"]');var o=[];for(var i=0;i<cs.length;i++){var m=cs[i].getAttribute('mid');if(m)o.push(m);}return JSON.stringify(o);})()"
    ).unwrap_or_default();
    // JS 返回 JSON.stringify(array)，unzoo 直接回传该字符串；个别情况下可能被再包一层字符串，故两路兜底。
    if let Ok(v) = serde_json::from_str::<Vec<String>>(&raw) { return v; }
    let inner = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    serde_json::from_str::<Vec<String>>(&inner).unwrap_or_default()
}

/// 在搜索结果页随机滚动几下，模拟真人浏览（看几屏再换关键词）。
fn weibo_browse_scroll_blocking(seed: u64) {
    use std::time::Duration;
    let times = (seed % 3) + 1; // 滚 1~3 屏
    for _ in 0..times {
        let _ = unzoo_evaluate("window.scrollBy(0, 380+Math.floor(Math.random()*520))");
        std::thread::sleep(Duration::from_millis(get_human_delay(1500, 3500)));
        random_mouse_movement();
    }
}

/// 就地点赞搜索结果页中 mid 对应的微博：滚动到卡片→读几秒→校验未赞→force 点赞→校验已赞。
/// 返回 Ok(true)=本次点赞成功；Ok(false)=该卡片已赞(跳过，不记库)；Err=未找到/点击失败/校验未生效。
/// 已赞态识别：点赞按钮内 svg use 的 xlink:href 含 "liked"(未赞为 "#def_woo_svg_like")。绝不点已赞的，以免取消赞。
fn weibo_like_by_mid_blocking(mid: &str) -> Result<bool, String> {
    use std::time::Duration;
    let card_sel = format!("div.card-wrap[mid=\"{}\"]", mid);
    if !unzoo_element_exists(&card_sel) { return Err("卡片不存在(已翻走/改版)".into()); }
    // 滚动到卡片中部，模拟真人定位
    let scroll_js = format!("(function(){{var c=document.querySelector('div.card-wrap[mid=\"{}\"]');if(!c)return 'no';c.scrollIntoView({{block:'center'}});return 'ok';}})()", mid);
    let _ = unzoo_evaluate(&scroll_js);
    std::thread::sleep(Duration::from_millis(get_random_delay(3, 7))); // 阅读几秒再赞
    // 校验已赞态：返回 "1"=已赞 / "0"=未赞 / 其它=按钮缺失
    let state_js = format!("(function(){{var c=document.querySelector('div.card-wrap[mid=\"{}\"]');if(!c)return 'no';var b=c.querySelector('button.woo-like-main');if(!b)return 'nobtn';var u=b.querySelector('svg use');var h=u?(u.getAttribute('xlink:href')||u.getAttribute('href')||''):'';return /liked/.test(h)?'1':'0';}})()", mid);
    let state = unzoo_evaluate(&state_js).unwrap_or_default();
    let state = state.trim();
    if state == "1" { return Ok(false); }           // 已赞 → 跳过
    if state != "0" { return Err(format!("点赞按钮缺失(state={})", state)); }
    // force 点赞（woo-like 按钮常被遮挡检测拦下，需 force）
    let like_sel = format!("div.card-wrap[mid=\"{}\"] button.woo-like-main", mid);
    if !xhs_force_click(&like_sel) { return Err("点赞点击失败".into()); }
    std::thread::sleep(Duration::from_millis(900));
    // 校验已变已赞
    let after = unzoo_evaluate(&state_js).unwrap_or_default();
    if after.trim() == "1" { Ok(true) } else { Err(format!("点击后未变已赞(state={})，疑似未生效", after.trim())) }
}

/// 微博养号阻塞主流程：轮转关键词搜索 → 体检 → 采集 mid → DB 去重 → 就地点赞 → 滚动浏览。
/// 返回 (searched, liked, health_abort)：命中验证码/封号/限流时写 health_abort 并提前结束（保留已完成计数）。
fn weibo_nurture_browse_blocking(app: AppHandle, account_id: &str, keywords: Vec<String>, n_like: i64, duration_secs: i64, seed0: u64) -> Result<(i64, i64, Option<String>), String> {
    use std::time::{Duration, Instant};
    let start = Instant::now();
    let mut searched = 0i64;
    let mut liked = 0i64;
    let mut seed = seed0;
    let mut acted_mids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ki = (seed0 as usize) % keywords.len().max(1);
    while liked < n_like {
        if nurture_should_stop() { break; }
        if start.elapsed().as_secs() as i64 >= duration_secs { break; }
        let kw = keywords[ki % keywords.len()].clone();
        ki += 1;
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        // 1) 导航搜索页（关键词整体 URL 编码：含 # 的话题不编码会被当 fragment → q 变空）
        let q_enc = urlencoding::encode(&kw).into_owned();
        let url = format!("https://s.weibo.com/weibo?q={}&Refer=weibo_weibo", q_enc);
        if unzoo_navigate(&url).is_err() { continue; }
        searched += 1;
        std::thread::sleep(Duration::from_millis(get_random_delay(3, 6)));
        // 2) 体检：抓页面文本判验证码/封号/限流 → 命中即写 health 并提前结束
        let body = unzoo_evaluate("(document.body.innerText||'').slice(0,4000)").unwrap_or_default();
        if let Some(state) = weibo_classify_health(&body) {
            let st = app.state::<AppState>();
            if let Ok(conn) = st.db.lock() {
                let _ = conn.execute("UPDATE accounts SET health_status=?1, last_health_check=datetime('now') WHERE id=?2", params![state, account_id]);
            }
            log::warn!("[WEIBO-NURTURE] account={} kw={} 检测到 {} → 停止养号", account_id, kw, state);
            return Ok((searched, liked, Some(state.to_string())));
        }
        // 3) 采集卡片 mid
        let mids = weibo_collect_mids_blocking();
        if mids.is_empty() {
            std::thread::sleep(Duration::from_millis(get_random_delay(2, 4)));
            continue;
        }
        // 4) DB 过滤：本账号已赞 + 跨账号≥3 + 本轮已点
        let candidates: Vec<String> = {
            let st = app.state::<AppState>();
            let conn = st.db.lock().map_err(|e| e.to_string())?;
            mids.into_iter().filter(|m| {
                !acted_mids.contains(m)
                    && !weibo_already_acted(&conn, account_id, m)
                    && weibo_target_persona_count(&conn, m) < 3
            }).collect()
        };
        // 5) 就地点赞：每个关键词页至多点 1~2 条（避免单页狂点），点够总额即止
        let per_kw = get_human_delay(1, 2) as i64;
        let mut kw_liked = 0i64;
        for mid in &candidates {
            if liked >= n_like || kw_liked >= per_kw { break; }
            if nurture_should_stop() { break; }
            acted_mids.insert(mid.clone());
            match weibo_like_by_mid_blocking(mid) {
                Ok(true) => {
                    let st = app.state::<AppState>();
                    if let Ok(conn) = st.db.lock() { let _ = weibo_record_action(&conn, account_id, "like", mid); }
                    liked += 1;
                    kw_liked += 1;
                    emit_nurture_step(&app, account_id, &format!("❤️ 微博点赞 {}/{}", liked, n_like));
                }
                Ok(false) => {} // 已赞，跳过不记库
                Err(e) => { log::info!("[WEIBO-NURTURE] like 跳过 mid={}: {}", mid, e); }
            }
            // 点赞间隔 18~45s（拟人 + 不过度）
            std::thread::sleep(Duration::from_millis(get_random_delay(18, 45)));
        }
        // 6) 读完这页随机滚动浏览，再换词
        weibo_browse_scroll_blocking(seed);
        std::thread::sleep(Duration::from_millis(get_random_delay(3, 8)));
    }
    Ok((searched, liked, None))
}

/// 微博养号：按方向取关键词 → s.weibo.com 搜索采卡片 → 去重选取 → 就地点赞 + 滚动浏览。
/// 首版只做「点赞 + 浏览」；评论/楼中回复留二期（可套小红书的 creply + 灌水识别）。
pub(crate) async fn weibo_nurture_run(app: &AppHandle, account_id: &str, duration: i64) -> Result<String, String> {
    let session_start = std::time::Instant::now();
    // 1) 读方向 + 关键词 + 分期
    let (topics, kws, phase) = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let topics = account_topics(&conn, account_id);
        let kws = account_topic_keywords(&conn, account_id);
        let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
        let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
        let strat = conn.query_row("SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max FROM nurture_strategies WHERE platform='weibo'",
            [], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?, r.get::<_,i64>(2)?, r.get::<_,i64>(3)?))).ok();
        let (warmup, growth, smin, smax) = strat.unwrap_or((3, 5, 2, 4));
        let (phase, _t) = nurture_phase_and_target(age, warmup, growth, smin, smax);
        (topics, kws, phase.to_string())
    };
    if topics.is_empty() {
        return Ok("账号未选方向，跳过微博养号（点卡片上「🎯 主题」选一下方向）".to_string());
    }
    if kws.is_empty() { return Ok("方向无可用关键词".to_string()); }
    // 2) 点赞配额：按分期取基数再加抖动（微博较严，最高一轮 4 赞）
    let seed = get_random_delay(1, 100_000);
    let n_like = crate::weibo_jitter_likes(crate::weibo_like_quota(&phase), seed);
    let dur = duration.max(30);
    emit_nurture_step(app, account_id, &format!("开始微博养号 · 搜索领域词并就地点赞（目标 {} 赞，约 {}s）", n_like, dur));
    // 3) 阻塞驱动：搜索→采集→点赞→浏览
    let app_cl = app.clone();
    let acct = account_id.to_string();
    let (searched, liked, aborted) = tauri::async_runtime::spawn_blocking(move || weibo_nurture_browse_blocking(app_cl, &acct, kws, n_like, dur, seed))
        .await.map_err(|e| format!("养号任务异常: {}", e))??;

    // 4) 记录耗时 + 健康态 + 当日 session
    let elapsed_secs = session_start.elapsed().as_secs() as i64;
    let final_health = aborted.as_deref().unwrap_or("healthy");
    {
        let st = app.state::<AppState>();
        let locked = st.db.lock();
        if let Ok(conn) = locked {
            let now = Utc::now().to_rfc3339();
            let today = Local::now().format("%Y-%m-%d").to_string();
            let _ = conn.execute(
                "UPDATE accounts SET nurture_started_at=COALESCE(nurture_started_at,?1), last_nurture_at=?1, \
                 total_nurture_seconds=COALESCE(total_nurture_seconds,0)+?2, health_status=?4, last_health_check=?1 WHERE id=?3",
                params![now, elapsed_secs, account_id, final_health]);
            let _ = conn.execute(
                "INSERT INTO nurture_daily_logs (id, account_id, date, sessions_completed, total_seconds) VALUES (?1,?2,?3,1,?4) \
                 ON CONFLICT(account_id,date) DO UPDATE SET sessions_completed=sessions_completed+1, total_seconds=total_seconds+?4",
                params![Uuid::new_v4().to_string(), account_id, today, elapsed_secs]);
        }
    }
    log::info!("[WEIBO-NURTURE] account={} phase={} 搜索={} 点赞={} 耗时={}s health={}", account_id, phase, searched, liked, elapsed_secs, final_health);
    if let Some(s) = &aborted {
        return Ok(format!("微博养号中止（{}）：已搜索 {} 次 · 点赞 {} · 用时 {}s", s, searched, liked, elapsed_secs));
    }
    Ok(format!("微博养号完成（{}）：搜索 {} 次 · 点赞 {} · 用时 {}s", phase, searched, liked, elapsed_secs))
}

/// X 养号：按方向取关键词→搜索采推文/用户→去重选取→点赞/关注/转推/回复 + 极少原创。
pub(crate) async fn x_nurture_run(app: &AppHandle, account_id: &str, _duration: i64) -> Result<String, String> {
    let session_start = std::time::Instant::now();
    // 1) 读方向 + 分期 + 回复风格
    let (niches, kws, phase, warmup, reply_style) = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let niches = account_topics(&conn, account_id);
        let kws = account_topic_keywords(&conn, account_id);
        let reply_style = crate::account_reply_style(&conn, account_id);
        let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
        let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
        let strat = conn.query_row("SELECT warmup_days, COALESCE(growth_days, warmup_days), daily_sessions_min, daily_sessions_max FROM nurture_strategies WHERE platform='twitter'",
            [], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?, r.get::<_,i64>(2)?, r.get::<_,i64>(3)?))).ok();
        let (warmup, growth, smin, smax) = strat.unwrap_or((5, 5, 2, 4));
        let (phase, _t) = nurture_phase_and_target(age, warmup, growth, smin, smax);
        (niches, kws, phase.to_string(), warmup, reply_style)
    };
    if niches.is_empty() {
        return Ok("账号未选方向，跳过 X 养号".to_string());
    }
    let quota_base = x_daily_quota(&phase);

    // 2) 选方向 → 关键词（已在首块收集）
    if kws.is_empty() { return Ok("方向无可用关键词".to_string()); }
    let seed = get_random_delay(1, 100_000);
    // 配额加随机抖动：点赞次数浮动、转推 0~3、关注 0~base，避免每轮次数固定像脚本
    let (n_like, n_follow, n_engage) = x_jitter_quota(quota_base, seed);
    let kw = &kws[(seed as usize) % kws.len()];
    // 主题扩展：普通词拼英文意图后缀(tutorial/tips…)，hashtag 保持原样。
    let kw = &x_expand_query(kw, seed);

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
        gh_pick_targets(&tweets, &already, n_like.max(n_engage).max(3) as usize, seed)
    };

    // 5) 动作执行：点赞 / 关注 / 转推 / 回复——本轮顺序随机打乱（避免每次都「点赞完→转推→回复」像脚本）。
    //    各动作命中限流/受限(HEALTH:) → 置 aborted_health，后续动作整体跳过。
    let mut likes = 0i64;
    let mut follows = 0i64;
    let mut engages = 0i64;
    let mut replies = 0i64;
    let mut aborted_health: Option<String> = None;
    let reply_quota = x_reply_quota(&phase);
    let reply_on = {
        let st = app.state::<AppState>();
        st.db.lock().ok().map(|c| crate::x_reply_enabled(&c)).unwrap_or(false)
    };

    // 本轮动作顺序随机：0=点赞 1=关注 2=转推 3=回复（Fisher–Yates，用 seed 派生伪随机）
    let mut order = [0u8, 1, 2, 3];
    {
        let mut s = seed ^ 0x9E3779B97F4A7C15;
        for i in (1..order.len()).rev() {
            s ^= s << 13; s ^= s >> 7; s ^= s << 17;
            let j = (s as usize) % (i + 1);
            order.swap(i, j);
        }
    }

    for step in order {
        if nurture_should_stop() || aborted_health.is_some() { break; }
        match step {
            // 点赞（B：动作命中限流/受限 → 退避，停止本轮剩余动作）
            0 => {
                emit_nurture_step(app, account_id, &format!("开始点赞 · {} 条推文", chosen.len()));
                for (i, t) in chosen.iter().enumerate() {
                    if nurture_should_stop() { break; }
                    emit_nurture_step(app, account_id, &format!("❤️ 点赞中 {}/{}", i + 1, chosen.len()));
                    let tc = t.clone();
                    let r = tauri::async_runtime::spawn_blocking(move || x_like_blocking(&tc)).await.map_err(|e| e.to_string())?;
                    match r {
                        Ok(_) => {
                            let st = app.state::<AppState>();
                            if let Ok(conn) = st.db.lock() { let _ = x_record_action(&conn, account_id, "like", t); }
                            likes += 1;
                        }
                        Err(e) if e.starts_with("HEALTH:") => { aborted_health = Some(e[7..].to_string()); break; }
                        Err(_) => {}
                    }
                    // X 动作间隔：随机 15-40 秒（拟人 + 不过度）
                    tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(15, 40))).await;
                }
            }
            // 关注：People 搜索找该领域好用户 → 质量门(有简介+粉丝达标)才关注
            1 => {
                if n_follow > 0 {
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
                        if nurture_should_stop() { break; }
                        if follows >= n_follow { break; }
                        emit_nurture_step(app, account_id, &format!("👤 关注评估中（已 {}/{}）：{}", follows, n_follow, prof.trim_start_matches("https://x.com/")));
                        let p = prof.clone();
                        let res = tauri::async_runtime::spawn_blocking(move || x_follow_quality_blocking(&p)).await
                            .map_err(|e| e.to_string())?;
                        match res {
                            Ok(true) => {
                                let st = app.state::<AppState>();
                                if let Ok(conn) = st.db.lock() { let _ = x_record_action(&conn, account_id, "follow", prof); }
                                follows += 1;
                            }
                            Ok(false) => {} // 不达标/已关注，下一个
                            Err(e) if e.starts_with("HEALTH:") => { aborted_health = Some(e[7..].to_string()); break; }
                            Err(_) => {}
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(15, 40))).await; // X 动作间隔 15-40s
                    }
                }
            }
            // 转推：engage 预算内，对部分推文转推
            2 => {
                if n_engage > 0 {
                    for (i, t) in chosen.iter().take(n_engage as usize).enumerate() {
                        if nurture_should_stop() { break; }
                        let key = format!("{}#engage", t);
                        let acted = { let st = app.state::<AppState>(); let l = st.db.lock().map_err(|e| e.to_string())?; x_already_acted(&l, account_id, &key) };
                        if acted { continue; }
                        emit_nurture_step(app, account_id, &format!("🔁 转推 {}/{}", i + 1, n_engage));
                        let tc = t.clone();
                        let r = tauri::async_runtime::spawn_blocking(move || x_retweet_blocking(&tc)).await.map_err(|e| e.to_string())?;
                        if r.is_ok() {
                            let st = app.state::<AppState>();
                            if let Ok(conn) = st.db.lock() { let _ = x_record_action(&conn, account_id, "retweet", &key); }
                            engages += 1;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(15, 40))).await; // X 动作间隔 15-40s
                    }
                }
            }
            // 自动回复（开关开 + 配额>0）：读正文 → 大模型生成切题回复 → 真实键盘输入并发送。
            // 风险动作：默认关；读不到正文/生成不合格/发送失败都跳过；回复间隔 30~90s；跨 session 去重(#reply)。
            3 => {
                if reply_on && reply_quota > 0 {
                    for t in chosen.iter() {
                        if nurture_should_stop() { break; }
                        if replies >= reply_quota { break; }
                        let key = format!("{}#reply", t);
                        // 去重：本账号已回过这条 → 跳过
                        let acted = { let st = app.state::<AppState>(); let l = st.db.lock().map_err(|e| e.to_string())?; x_already_acted(&l, account_id, &key) };
                        if acted { continue; }
                        // 读正文（读不到/太短 → 跳过，不回复）
                        let tc = t.clone();
                        let body = tauri::async_runtime::spawn_blocking(move || x_read_tweet_text_blocking(&tc)).await.map_err(|e| e.to_string())?;
                        let body = match body { Some(b) => b, None => continue };
                        // 大模型基于正文生成回复（无 key/不合格 → None → 跳过）
                        let reply = match gen_nurture_text(app, "x_reply", &body, &reply_style).await {
                            Some(r) => r,
                            None => { emit_nurture_step(app, account_id, "未配置 AI 或回复不合格，跳过回复"); continue }
                        };
                        emit_nurture_step(app, account_id, &format!("💬 回复 {}/{}", replies + 1, reply_quota));
                        // 发回复（twitter_reply 自带导航 + 真实键盘输入 + 校验按钮可用 + 提交校验）
                        let url = t.clone(); let rep = reply.clone();
                        let r = tauri::async_runtime::spawn_blocking(move || twitter_reply(&url, &rep)).await.map_err(|e| e.to_string())?;
                        match r {
                            Ok(_) => {
                                let st = app.state::<AppState>();
                                if let Ok(conn) = st.db.lock() { let _ = x_record_action(&conn, account_id, "reply", &key); }
                                replies += 1;
                            }
                            // 发送失败（如按钮未激活/未登录）→ 不记库、出提示，避免假成功
                            Err(e) => { emit_nurture_step(app, account_id, &format!("回复发送失败，跳过：{}", e)); }
                        }
                        // 回复间隔 30~90s（拟人）
                        tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(30, 90))).await;
                    }
                }
            }
            _ => {}
        }
    }
    let _ = (likes, follows, engages, replies);

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
            let r = match gen_nurture_text(app, "x_tweet", &ctx, &reply_style).await {
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
    // 打开推文后先「阅读」几秒再点赞，避免秒赞像机器人
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(4, 9)));
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
    // 打开推文后先「阅读」几秒再转推，避免秒转像机器人
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(4, 9)));
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
