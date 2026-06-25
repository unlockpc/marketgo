# 小红书专属养号 runner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给小红书做搜索驱动的专属养号 runner（按所选主题关键词搜索→拟人浏览→点进笔记阅读，成长期对少量笔记点赞），默认预热 7 天/成长 5 天可编辑。

**Architecture:** 照搬 `segmentfault_nurture_run` + `sf_nurture_browse_blocking`（nurture.rs）的搜索驱动结构；点赞去重借用 X 的 `x_actions_log` 模式（新建 `xhs_actions_log`）；所有点击动作前"轮询关键元素出现确认加载 + 随机延迟"。消费已实现的 `account_topic_keywords`。

**Tech Stack:** Rust + Tauri v2 + rusqlite；unzoo 浏览器 helper（`unzoo_navigate/scroll/get_links/element_exists/click`）。

参考：SF runner `nurture.rs:203/265`、X 去重 `lib.rs:1340-1352`、X 表 `lib.rs:3620`、调度 `lib.rs:9512-9522`、搜索 URL `lib.rs:1694`、登录检测模板 `sf_logged_in_blocking nurture.rs:176`。
设计：`docs/superpowers/specs/2026-06-25-xhs-nurture-runner-design.md`

---

## File Structure

- `src-tauri/src/lib.rs`（修改）
  - 迁移块（~3656，nurture_topics 迁移旁）：建 `xhs_actions_log` 表
  - 策略默认（seed 区 ~3699 + 迁移块）：小红书 seed warmup 14→7 + growth=5；`apply_xhs_strategy_default` 守卫迁移
  - helper（X 去重旁 ~1352）：`xhs_already_acted` / `xhs_record_action`
  - `quick_nurture`（~9521，SF 分支后）：小红书调度分支
  - 测试模块（文件末尾）：`mod xhs_runner_tests`
- `src-tauri/src/nurture.rs`（修改，SF runner 旁）
  - `xhs_phase_intensity`、`xhs_wait_loaded_blocking`、`xhs_logged_in_blocking`、`xhs_like_blocking`、`xhs_nurture_browse_blocking`、`xiaohongshu_nurture_run`

---

### Task 1: DB 迁移（动作日志表 + 策略默认 7/5）

**Files:** Modify `src-tauri/src/lib.rs`；Test: `mod xhs_runner_tests`

- [ ] **Step 1: 写失败测试**

在文件末尾追加：

```rust
#[cfg(test)]
mod xhs_runner_tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_strategies() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE nurture_strategies (platform TEXT PRIMARY KEY, warmup_days INTEGER, growth_days INTEGER, daily_sessions_min INTEGER, daily_sessions_max INTEGER);
        ").unwrap();
        c
    }

    #[test]
    fn xhs_default_applies_only_to_untouched() {
        let c = setup_strategies();
        // 自动默认行(14/NULL) → 改成 7/5
        c.execute("INSERT INTO nurture_strategies (platform,warmup_days,growth_days,daily_sessions_min,daily_sessions_max) VALUES ('xiaohongshu',14,NULL,3,6)", []).unwrap();
        assert_eq!(apply_xhs_strategy_default(&c), 1);
        let (w, g): (i64, i64) = c.query_row("SELECT warmup_days, growth_days FROM nurture_strategies WHERE platform='xiaohongshu'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((w, g), (7, 5));
        // 再跑一次幂等：已是 7/5，不再命中
        assert_eq!(apply_xhs_strategy_default(&c), 0);
    }

    #[test]
    fn xhs_default_preserves_user_edits() {
        let c = setup_strategies();
        // 用户手改过(warmup=10) → 不动
        c.execute("INSERT INTO nurture_strategies (platform,warmup_days,growth_days,daily_sessions_min,daily_sessions_max) VALUES ('xiaohongshu',10,4,3,6)", []).unwrap();
        assert_eq!(apply_xhs_strategy_default(&c), 0);
        let w: i64 = c.query_row("SELECT warmup_days FROM nurture_strategies WHERE platform='xiaohongshu'", [], |r| r.get(0)).unwrap();
        assert_eq!(w, 10);
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test xhs_runner_tests 2>&1 | grep -iE "cannot find|test result" | head`
Expected: 编译失败 —— 找不到 `apply_xhs_strategy_default`。

- [ ] **Step 3: 实现 helper**

在 lib.rs `x_record_action` 函数结束（~1352 行 `}`）之后插入：

```rust
/// 小红书养号动作去重（与 x_already_acted 同构，作用于 xhs_actions_log）。
fn xhs_already_acted(conn: &Connection, account_id: &str, target: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM xhs_actions_log WHERE account_id=?1 AND target=?2 LIMIT 1",
        params![account_id, target], |_| Ok(true)).unwrap_or(false)
}

fn xhs_record_action(conn: &Connection, account_id: &str, action_type: &str, target: &str) -> Result<(), String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    conn.execute(
        "INSERT INTO xhs_actions_log (id, account_id, action_type, target, date) VALUES (?1,?2,?3,?4,?5)",
        params![Uuid::new_v4().to_string(), account_id, action_type, target, today])
        .map(|_| ()).map_err(|e| e.to_string())
}

/// 小红书养号策略默认值：预热7/成长5——仅当该行仍是自动默认(warmup=14且growth为空)时，避免覆盖用户手改。
fn apply_xhs_strategy_default(conn: &Connection) -> usize {
    conn.execute(
        "UPDATE nurture_strategies SET warmup_days=7, growth_days=5 WHERE platform='xiaohongshu' AND warmup_days=14 AND growth_days IS NULL",
        []).unwrap_or(0)
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test xhs_runner_tests 2>&1 | grep -iE "test result" | head -1`
Expected: `test result: ok. 2 passed`。

- [ ] **Step 5: 建表 + 调用迁移 + 改 seed**

在 lib.rs:3656 的 `custom_topics` 建表行之后插入建表：

```rust
    let _ = conn.execute("CREATE TABLE IF NOT EXISTS xhs_actions_log (id TEXT PRIMARY KEY, account_id TEXT NOT NULL, action_type TEXT NOT NULL, target TEXT NOT NULL, date TEXT NOT NULL, created_at TEXT DEFAULT CURRENT_TIMESTAMP)", []);
    let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_xhs_actions_acct ON xhs_actions_log(account_id, target)", []);
    apply_xhs_strategy_default(&conn);  // 老库：小红书自动默认 14/NULL → 7/5（不覆盖手改）
```

把 lib.rs:3699 的 seed 元组小红书 warmup 14 改为 7：

```rust
                ("xiaohongshu", 7, 3, 6, 60, 180, 10, 23),
```

在 seed 循环结束（`for (platform, ...) in strategies { ... }` 之后、`if count==0` 块内）补一句给小红书设 growth_days：

```rust
            let _ = conn.execute("UPDATE nurture_strategies SET growth_days=5 WHERE platform IN ('xiaohongshu','redbook')", []);
```

- [ ] **Step 6: 编译 + 测试**

Run: `cd src-tauri && cargo build 2>&1 | grep -iE "^error|Finished" | head && cargo test xhs_runner_tests 2>&1 | grep "test result" | head -1`
Expected: `Finished` 无 error；`test result: ok. 2 passed`。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 小红书养号动作日志表+去重 helper+默认策略7/5"
```

---

### Task 2: 分期强度（纯逻辑，TDD）

**Files:** Modify `src-tauri/src/nurture.rs`（SF runner 旁）；Test: `mod xhs_runner_tests`（lib.rs）

- [ ] **Step 1: 写失败测试**

在 lib.rs `mod xhs_runner_tests` 内追加：

```rust
    #[test]
    fn phase_intensity_by_stage() {
        assert_eq!(crate::nurture::xhs_phase_intensity("warmup"), (2, 2, 0)); // 预热不点赞
        assert_eq!(crate::nurture::xhs_phase_intensity("growth"), (3, 3, 2)); // 成长点赞
        assert_eq!(crate::nurture::xhs_phase_intensity("mature"), (2, 2, 1)); // 成熟维持
        assert_eq!(crate::nurture::xhs_phase_intensity("other"), (2, 2, 0)); // 兜底=预热
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test xhs_runner_tests::phase 2>&1 | grep -iE "cannot find|test result" | head`
Expected: 编译失败 —— 找不到 `xhs_phase_intensity`。

- [ ] **Step 3: 实现**

在 nurture.rs `segmentfault_nurture_run` 函数结束（~317 行 `}`）之后插入：

```rust
/// 小红书养号分期强度 → (搜索次数, 每次阅读, 点赞数)。预热只读，成长点赞，成熟维持。
pub(crate) fn xhs_phase_intensity(phase: &str) -> (i64, i64, i64) {
    match phase {
        "growth" => (3, 3, 2),
        "mature" => (2, 2, 1),
        _ => (2, 2, 0), // warmup
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test xhs_runner_tests 2>&1 | grep "test result" | head -1`
Expected: `test result: ok. 3 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/nurture.rs src-tauri/src/lib.rs
git commit -m "feat(nurture): 小红书养号分期强度(预热只读/成长点赞)"
```

---

### Task 3: 浏览器动作 helper（登录检测 / 等加载 / 点赞）

**Files:** Modify `src-tauri/src/nurture.rs`（接 Task 2 之后）。无单测（需实时浏览器）；以 `cargo build` 为准。

- [ ] **Step 1: 写三个 helper**

在 nurture.rs `xhs_phase_intensity` 之后插入：

```rust
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
```

- [ ] **Step 2: 编译确认**

Run: `cd src-tauri && cargo build 2>&1 | grep -iE "^error|Finished" | head`
Expected: `Finished` 无 error（未被调用的 fn 会有 warning，下一 Task 接入后消失）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/nurture.rs
git commit -m "feat(nurture): 小红书登录检测/等加载/点赞 浏览器 helper"
```

---

### Task 4: 搜索驱动浏览主循环

**Files:** Modify `src-tauri/src/nurture.rs`（接 Task 3 之后）。无单测；`cargo build` 为准。

- [ ] **Step 1: 写 xhs_nurture_browse_blocking**

在 nurture.rs `xhs_like_blocking` 之后插入。每个动作前"等加载 + 随机延迟"；点赞前额外随机停 2~5s；点赞去重防止重复点赞（小红书重复点赞会取消赞）：

```rust
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
                    match st.db.lock() { Ok(c) => xhs_already_acted(&c, account_id, &p), Err(_) => true }
                };
                if !already {
                    std::thread::sleep(Duration::from_millis(get_human_delay(2000, 5000))); // 加载后随机停几秒再点赞
                    if xhs_like_blocking() {
                        liked += 1;
                        { let st = app.state::<AppState>(); if let Ok(c) = st.db.lock() { let _ = xhs_record_action(&c, account_id, "like", &p); } }
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
```

- [ ] **Step 2: 编译确认**

Run: `cd src-tauri && cargo build 2>&1 | grep -iE "^error|Finished" | head`
Expected: `Finished` 无 error。若报 `AppHandle`/`Manager`/`state` 相关，确认 nurture.rs 顶部已 `use tauri::Manager;`（SF/X runner 已用 `app.state`，应已导入）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/nurture.rs
git commit -m "feat(nurture): 小红书搜索驱动浏览主循环(等加载+随机延迟+成长期点赞去重)"
```

---

### Task 5: runner 入口 + 调度接入

**Files:** Modify `src-tauri/src/nurture.rs`（接 Task 4 之后）+ `src-tauri/src/lib.rs`（quick_nurture）

- [ ] **Step 1: 写 xiaohongshu_nurture_run**

在 nurture.rs `xhs_nurture_browse_blocking` 之后插入（照搬 `segmentfault_nurture_run` 的读分期 + 写统计）：

```rust
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
    let (n_search, read_per_search, n_like) = xhs_phase_intensity(&phase);
    let dur = duration.max(30);
    let seed0 = get_random_delay(1, 100_000);
    emit_nurture_step(app, account_id, &format!("开始小红书养号 · 按主题搜索 {} 次并阅读（约 {}s）", n_search, dur));
    let app_cl = app.clone();
    let acct = account_id.to_string();
    let (searched, read, liked) = tauri::async_runtime::spawn_blocking(move || xhs_nurture_browse_blocking(app_cl, &acct, kws, n_search, read_per_search, n_like, dur, seed0))
        .await.map_err(|e| format!("养号任务异常: {}", e))??;

    // 写养号统计（与 SF 一致）
    let elapsed_secs = session_start.elapsed().as_secs() as i64;
    {
        let st = app.state::<AppState>();
        if let Ok(conn) = st.db.lock() {
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
    log::info!("[XHS-NURTURE] account={} phase={} 搜索={} 阅读={} 点赞={} 耗时={}s", account_id, phase, searched, read, liked, elapsed_secs);
    Ok(format!("小红书养号完成（{}）：搜索 {} 次 · 阅读 {} 篇 · 点赞 {} · 用时 {}s", phase, searched, read, liked, elapsed_secs))
}
```

- [ ] **Step 2: 调度接入**

在 lib.rs:9521 的 SegmentFault 分支（`return nurture::segmentfault_nurture_run(...)` 的 `}`）之后插入：

```rust
    // 小红书走专属搜索驱动养号（按主题搜索→浏览→读笔记，成长期点赞），不走纯滚动。
    if platform.eq_ignore_ascii_case("xiaohongshu") || platform.eq_ignore_ascii_case("redbook") {
        return nurture::xiaohongshu_nurture_run(&app, &account_id, seconds).await;
    }
```

- [ ] **Step 3: 编译 + 全量测试**

Run: `cd src-tauri && cargo build 2>&1 | grep -iE "^error|Finished" | head && cargo test 2>&1 | grep -E "test result|FAILED$" | head`
Expected: `Finished` 无 error；`xhs_runner_tests` 与 `topics_tests` 全过（既存的 2 个 `platform_meta_tests` 失败与本次无关，可忽略）。

- [ ] **Step 4: 手动验证（app 在 tauri dev 跑）**

1. 准备一个**已登录**的小红书账号，给它选 1~2 个主题（🎯 主题）。
2. 点该账号「🌱 快速养号」。观察：
   - 进度推送「开始小红书养号 · 按主题搜索 …」
   - 浏览器导航到 `search_result?keyword=<主题>`，结果页加载完才滚动
   - 点进笔记（`/explore/…`），加载完才滚动阅读
   - 若账号已过预热期（age≥7，growth）→ 看到「👍 点赞 x/2」，且点赞前有几秒停顿
   - 结束提示「小红书养号完成（…）：搜索 x 次 · 阅读 x 篇 · 点赞 x · 用时 xs」
3. 未登录小红书账号点养号 → 提示「未登录小红书！请先…手工登录」。
4. 若搜索/点赞选择器没命中（结果空 / 不点赞）→ 按 Task 3 注释微调 `a[href*="/explore/"]` / 登录 / 点赞选择器。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/nurture.rs src-tauri/src/lib.rs
git commit -m "feat(nurture): 小红书养号 runner 入口 + quick_nurture 调度接入"
```

---

## 自我审查记录

- **Spec 覆盖**：分期强度(Task 2) · 时间控制"等加载+随机延迟+点赞前停 2~5s"(Task 3 `xhs_wait_loaded_blocking` + Task 4 主循环) · 默认 7/5 可编辑(Task 1，编辑沿用现有 UI) · 点赞去重 `xhs_actions_log`(Task 1+4) · 登录检测/笔记链接/点赞选择器最佳猜测+实测微调(Task 3/4 注释) · 调度接入(Task 5) · v1 只点赞无收藏。spec 各节均有任务。
- **类型/命名一致**：`xhs_already_acted`/`xhs_record_action`/`apply_xhs_strategy_default`(lib.rs)；`xhs_phase_intensity`(pub(crate))/`xhs_wait_loaded_blocking`/`xhs_logged_in_blocking`/`xhs_like_blocking`/`xhs_nurture_browse_blocking`/`xiaohongshu_nurture_run`(nurture.rs)。`xhs_nurture_browse_blocking` 签名 `(AppHandle, &str, Vec<String>, i64,i64,i64,i64,u64) -> Result<(i64,i64,i64),String>` 与 Task 5 调用一致。
- **占位符扫描**：无 TBD/TODO；DOM 选择器为"最佳猜测+实测微调"是设计既定决策，非占位。
- **风险**：`AppHandle` 传入 spawn_blocking 闭包（Clone+Send）+ 闭包内 `app.state()/db.lock()/emit` —— Task 4 build gate 兜底；若不通则改为按动作 spawn_blocking + async 中操作 db（X runner 模式）。
