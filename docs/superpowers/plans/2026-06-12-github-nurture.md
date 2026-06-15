# GitHub 养号实现计划（L1/L2 · 浏览器 · 按领域）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 GitHub 账号养号从「通用滚动」升级为按 persona 领域的真实社交行为（L1：star/follow/watch；L2：良性评论），养成可信开发者身份。

**Architecture:** 全部代码加在 `src-tauri/src/lib.rs`（沿用本仓「单文件」现状，不新建模块以免大量 `pub(crate)` 改造）。纯逻辑单元（领域映射、目标去重选取、分期闸门）做成自由函数并用既有 `#[cfg(test)]` 模块做 TDD；浏览器编排走既有 `unzoo_*` 阻塞助手 + `spawn_blocking`；前端在 persona 开通流程加领域多选。L1 优先，L2 其次。

**Tech Stack:** Rust (Tauri, rusqlite, reqwest 经 socks5 代理), TypeScript (tauri-frontend), Unzoo 浏览器 MCP, vitest + cargo test。

设计来源：`docs/superpowers/specs/2026-06-12-github-nurture-design.md`

---

## 文件结构

- `src-tauri/src/lib.rs`
  - 新增静态常量 `GH_DOMAINS` + 纯逻辑函数 `gh_domain_topics` / `gh_pick_targets` / `gh_daily_quota` / `gh_l2_allowed`
  - 新增 DB 助手 `account_gh_domains` / `gh_already_acted` / `gh_record_action` / `gh_target_persona_count`
  - 新增建表（`gh_actions_log`）与迁移（`accounts.gh_domains` 列）
  - 新增命令 `gh_domains_catalog` / `set_account_gh_domains`
  - 新增编排 `github_nurture_run`（async）+ 在 nurture arm 分发
  - 新增 `#[cfg(test)]` 测试（并入既有 `mod platform_meta_tests` 同文件测试区）
- `src/tauri-frontend/app.ts`
  - persona 开通流程加「领域多选」UI + 调 `set_account_gh_domains`
- 测试：`src-tauri/src/lib.rs`（cargo test）、手动集成（需 Unzoo + 已登录 github 号）

---

## Phase 1 — L1（领域社交信号）

### Task 1: 领域分类常量 + topics 映射（纯逻辑，TDD）

**Files:**
- Modify: `src-tauri/src/lib.rs`（在 `platform_meta` 附近、即 ~`fn platform_meta` 之前加常量与函数）
- Test: `src-tauri/src/lib.rs` 的 `#[cfg(test)] mod platform_meta_tests`

- [ ] **Step 1: 写失败测试**

在 `mod platform_meta_tests` 末尾（`catalog_items_cover_all_keys_with_name` 之后）加：

```rust
    #[test]
    fn gh_domains_keys_unique_and_topics_nonempty() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for d in GH_DOMAINS {
            assert!(seen.insert(d.key), "duplicate domain key {}", d.key);
            assert!(!d.label.is_empty(), "empty label for {}", d.key);
            assert!(!d.topics.is_empty(), "no topics for {}", d.key);
        }
        // 关键领域存在
        for k in ["frontend", "backend", "ml", "ai_coding", "devops"] {
            assert!(GH_DOMAINS.iter().any(|d| d.key == k), "missing domain {}", k);
        }
    }

    #[test]
    fn gh_domain_topics_collects_and_dedups() {
        // ai_coding 含 claude，frontend 含 react；选两个领域应合并去重
        let t = gh_domain_topics(&["frontend", "ai_coding"]);
        assert!(t.contains(&"react"));
        assert!(t.contains(&"claude"));
        // 未知 key 跳过、不 panic
        let t2 = gh_domain_topics(&["frontend", "unknown_xyz"]);
        assert!(t2.contains(&"react"));
        // 去重：传同一领域两次不应有重复 topic
        let t3 = gh_domain_topics(&["frontend", "frontend"]);
        let uniq: std::collections::HashSet<_> = t3.iter().collect();
        assert_eq!(uniq.len(), t3.len());
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test platform_meta_tests::gh_ -- --nocapture`
Expected: 编译失败（`GH_DOMAINS` / `gh_domain_topics` 未定义）

- [ ] **Step 3: 实现常量与函数**

在 `src-tauri/src/lib.rs` 中 `fn platform_meta` 定义之前插入：

```rust
/// GitHub 养号领域分类：key 唯一，topics 为真实高 population 的 GitHub topic。
/// 单一来源，前端经 gh_domains_catalog 命令拉取渲染多选框。
pub struct GhDomain {
    pub key: &'static str,
    pub label: &'static str,
    pub topics: &'static [&'static str],
}

const GH_DOMAINS: &[GhDomain] = &[
    GhDomain { key: "frontend",  label: "前端",            topics: &["frontend","react","vue","angular","typescript","nextjs","tailwindcss"] },
    GhDomain { key: "backend",   label: "后端",            topics: &["backend","api","nodejs","golang","spring-boot","microservices","graphql"] },
    GhDomain { key: "ml",        label: "AI/机器学习",     topics: &["machine-learning","deep-learning","pytorch","tensorflow","nlp","computer-vision","generative-ai"] },
    GhDomain { key: "ai_coding", label: "AI 编程/Agent/Skills", topics: &["claude","claude-code","mcp","model-context-protocol","ai-agents","langchain","rag","prompt-engineering","github-copilot","cursor","coding-assistant"] },
    GhDomain { key: "data",      label: "数据工程",        topics: &["data-science","data-engineering","data-analysis","apache-spark","etl","pandas"] },
    GhDomain { key: "devops",    label: "DevOps/云原生",   topics: &["devops","kubernetes","docker","terraform","ansible","cloud-native","observability"] },
    GhDomain { key: "mobile",    label: "移动开发",        topics: &["android","ios","flutter","react-native","swift","kotlin","jetpack-compose"] },
    GhDomain { key: "security",  label: "安全",            topics: &["security","cybersecurity","penetration-testing","cryptography","ethical-hacking","infosec"] },
    GhDomain { key: "web3",      label: "区块链/Web3",     topics: &["blockchain","ethereum","solidity","web3","smart-contracts","defi"] },
    GhDomain { key: "gamedev",   label: "游戏开发",        topics: &["gamedev","unity","godot","unreal-engine","game-engine"] },
    GhDomain { key: "database",  label: "数据库",          topics: &["database","postgresql","mysql","redis","mongodb","sqlite"] },
    GhDomain { key: "embedded",  label: "嵌入式/IoT",      topics: &["embedded","iot","arduino","raspberry-pi","esp32","microcontroller"] },
    GhDomain { key: "devtools",  label: "开发工具/效率",   topics: &["cli","developer-tools","vscode","neovim","terminal","automation"] },
];

/// 收集所选领域 key 对应的全部 topic（按出现顺序去重，未知 key 跳过）。
fn gh_domain_topics(keys: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for k in keys {
        if let Some(d) = GH_DOMAINS.iter().find(|d| d.key == *k) {
            for t in d.topics {
                if !out.contains(t) { out.push(t); }
            }
        }
    }
    out
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test platform_meta_tests::gh_ -- --nocapture`
Expected: PASS（2 个新测试）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): 领域分类常量 GH_DOMAINS + topics 映射"
```

---

### Task 2: 每日配额 + L2 闸门（纯逻辑，TDD）

**Files:**
- Modify: `src-tauri/src/lib.rs`（紧接 Task 1 的函数之后）
- Test: `src-tauri/src/lib.rs` 同测试模块

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn gh_daily_quota_by_phase() {
        // warmup 极轻：至多 1 star、不 follow/watch
        assert_eq!(gh_daily_quota("warmup"), (1, 0, 0));
        // growth/mature 放开
        let (s, f, w) = gh_daily_quota("growth");
        assert!(s >= 1 && s <= 3 && f <= 2 && w <= 1);
        let (s2, _, _) = gh_daily_quota("mature");
        assert!(s2 >= 1);
        // 未知 phase 走保守默认
        assert_eq!(gh_daily_quota("unknown"), (1, 0, 0));
    }

    #[test]
    fn gh_l2_gate() {
        // warmup 永不评论
        assert!(!gh_l2_allowed(30, "warmup", 50));
        // 号龄不足 7 天不评论
        assert!(!gh_l2_allowed(6, "growth", 50));
        // 无 L1 历史不评论
        assert!(!gh_l2_allowed(10, "growth", 0));
        // 满足：号龄>=7 且 growth/mature 且有 L1 历史
        assert!(gh_l2_allowed(7, "growth", 5));
        assert!(gh_l2_allowed(30, "mature", 100));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test platform_meta_tests::gh_daily_quota platform_meta_tests::gh_l2_gate`
Expected: 编译失败（函数未定义）

- [ ] **Step 3: 实现**

```rust
/// 按号龄分期返回当日 GitHub L1 配额：(stars, follows, watches)。
fn gh_daily_quota(phase: &str) -> (i64, i64, i64) {
    match phase {
        "growth" => (3, 2, 1),
        "mature" => (2, 1, 1),
        _ => (1, 0, 0), // warmup / 未知：极轻
    }
}

/// L2（评论）是否解锁：非 warmup、号龄≥7 天、且已有 L1 历史。
fn gh_l2_allowed(age_days: i64, phase: &str, l1_action_count: i64) -> bool {
    phase != "warmup" && age_days >= 7 && l1_action_count > 0
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test platform_meta_tests::gh_daily_quota platform_meta_tests::gh_l2_gate`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): 分期每日配额 + L2 解锁闸门"
```

---

### Task 3: 目标选取（去重 + 随机），纯逻辑 TDD

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs` 同测试模块

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn gh_pick_targets_filters_and_limits() {
        use std::collections::HashSet;
        let cands = vec![
            "https://github.com/a/x".to_string(),
            "https://github.com/b/y".to_string(),
            "https://github.com/c/z".to_string(),
        ];
        let mut already = HashSet::new();
        already.insert("https://github.com/a/x".to_string());
        // seed 固定 → 结果可预测、不含已操作项、数量受限
        let picked = gh_pick_targets(&cands, &already, 2, 12345);
        assert!(picked.len() <= 2);
        assert!(!picked.contains(&"https://github.com/a/x".to_string()));
        for p in &picked { assert!(cands.contains(p)); }
    }

    #[test]
    fn gh_pick_targets_empty_when_all_acted() {
        use std::collections::HashSet;
        let cands = vec!["https://github.com/a/x".to_string()];
        let already: HashSet<String> = cands.iter().cloned().collect();
        assert!(gh_pick_targets(&cands, &already, 3, 1).is_empty());
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test platform_meta_tests::gh_pick_targets`
Expected: 编译失败

- [ ] **Step 3: 实现**

```rust
/// 从候选里去掉已操作项，用确定性洗牌（seed）挑至多 n 个。
/// seed 由调用方用时间派生（脚本不可用 rand，引擎里用 get_random_delay 同源时间种子）。
fn gh_pick_targets(candidates: &[String], already: &std::collections::HashSet<String>, n: usize, seed: u64) -> Vec<String> {
    let mut pool: Vec<String> = candidates.iter().filter(|c| !already.contains(*c)).cloned().collect();
    // 简单确定性洗牌（xorshift）：避免引入 rand 依赖，且 seed 相同结果稳定可测。
    let mut s = seed | 1;
    for i in (1..pool.len()).rev() {
        s ^= s << 13; s ^= s >> 7; s ^= s << 17;
        let j = (s as usize) % (i + 1);
        pool.swap(i, j);
    }
    pool.truncate(n);
    pool
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test platform_meta_tests::gh_pick_targets`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): 目标去重+确定性随机选取"
```

---

### Task 4: 数据库——`accounts.gh_domains` 列 + `gh_actions_log` 表

**Files:**
- Modify: `src-tauri/src/lib.rs`（建表区 ~`CREATE TABLE IF NOT EXISTS reply_history` 附近加新表；迁移区 ~`ALTER TABLE accounts ADD COLUMN ...` 处加列）

- [ ] **Step 1: 加建表语句**

在建表 SQL 串里（与 `nurture_sessions` 同一批 `CREATE TABLE` 区域，例如紧随其后）加：

```sql
        CREATE TABLE IF NOT EXISTS gh_actions_log (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            action_type TEXT NOT NULL,   -- star | follow | watch | comment
            target TEXT NOT NULL,        -- repo/user/thread 的 URL
            date TEXT NOT NULL,          -- YYYY-MM-DD
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_gh_actions_acct ON gh_actions_log(account_id, action_type);
        CREATE INDEX IF NOT EXISTS idx_gh_actions_target ON gh_actions_log(target);
```

- [ ] **Step 2: 加列迁移**

在迁移区（与 `ALTER TABLE accounts ADD COLUMN nurture_started_at TEXT` 同处）加（用既有「忽略已存在错误」的 `let _ = conn.execute(...)` 范式）：

```rust
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN gh_domains TEXT", []);
```

- [ ] **Step 3: 编译确认无误**

Run: `cd src-tauri && cargo check`
Expected: 无 error

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): accounts.gh_domains 列 + gh_actions_log 表"
```

---

### Task 5: DB 助手（去重/记录/跨账号计数/读领域）+ 测试

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`，新增 `#[cfg(test)] mod gh_db_tests`（用内存库 `rusqlite::Connection::open_in_memory`）

- [ ] **Step 1: 写失败测试**

新增测试模块（放在文件测试区，紧随 `mod platform_meta_tests` 之后）：

```rust
#[cfg(test)]
mod gh_db_tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE accounts (id TEXT PRIMARY KEY, gh_domains TEXT);
            CREATE TABLE gh_actions_log (id TEXT PRIMARY KEY, account_id TEXT, action_type TEXT, target TEXT, date TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP);
        ").unwrap();
        c
    }

    #[test]
    fn record_and_already_acted() {
        let c = setup();
        let tgt = "https://github.com/a/x";
        assert!(!gh_already_acted(&c, "acc1", tgt));
        gh_record_action(&c, "acc1", "star", tgt).unwrap();
        assert!(gh_already_acted(&c, "acc1", tgt));
        // 另一账号不受影响
        assert!(!gh_already_acted(&c, "acc2", tgt));
    }

    #[test]
    fn target_persona_count_cross_account() {
        let c = setup();
        let tgt = "https://github.com/a/x";
        gh_record_action(&c, "acc1", "star", tgt).unwrap();
        gh_record_action(&c, "acc2", "star", tgt).unwrap();
        gh_record_action(&c, "acc2", "star", tgt).unwrap(); // 同账号重复不增计数
        assert_eq!(gh_target_persona_count(&c, tgt), 2);
    }

    #[test]
    fn read_account_domains() {
        let c = setup();
        c.execute("INSERT INTO accounts (id, gh_domains) VALUES ('acc1', '[\"frontend\",\"ai_coding\"]')", []).unwrap();
        let d = account_gh_domains(&c, "acc1");
        assert_eq!(d, vec!["frontend".to_string(), "ai_coding".to_string()]);
        // 无该账号 / 空值 → 空 vec，不 panic
        assert!(account_gh_domains(&c, "nope").is_empty());
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test gh_db_tests`
Expected: 编译失败（助手未定义）

- [ ] **Step 3: 实现助手**

```rust
/// 该账号是否已对某 target 执行过任何 GitHub 动作。
fn gh_already_acted(conn: &Connection, account_id: &str, target: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM gh_actions_log WHERE account_id=?1 AND target=?2 LIMIT 1",
        params![account_id, target], |_| Ok(true)).unwrap_or(false)
}

/// 记一条 GitHub 动作。
fn gh_record_action(conn: &Connection, account_id: &str, action_type: &str, target: &str) -> Result<(), String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    conn.execute(
        "INSERT INTO gh_actions_log (id, account_id, action_type, target, date) VALUES (?1,?2,?3,?4,?5)",
        params![Uuid::new_v4().to_string(), account_id, action_type, target, today])
        .map(|_| ()).map_err(|e| e.to_string())
}

/// 有多少个不同账号触碰过该 target（跨账号去同质化用）。
fn gh_target_persona_count(conn: &Connection, target: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(DISTINCT account_id) FROM gh_actions_log WHERE target=?1",
        params![target], |r| r.get(0)).unwrap_or(0)
}

/// 读账号所选领域（gh_domains JSON 数组），解析失败/空 → 空 vec。
fn account_gh_domains(conn: &Connection, account_id: &str) -> Vec<String> {
    let raw: Option<String> = conn.query_row(
        "SELECT gh_domains FROM accounts WHERE id=?1",
        params![account_id], |r| r.get(0)).ok().flatten();
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default()
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test gh_db_tests`
Expected: PASS（3 测试）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): gh_actions_log DB 助手 + 账号领域读取"
```

---

### Task 6: 命令——`gh_domains_catalog` + `set_account_gh_domains`

**Files:**
- Modify: `src-tauri/src/lib.rs`（命令定义 + 注册到 `generate_handler!`）

- [ ] **Step 1: 实现两个命令**

```rust
#[derive(serde::Serialize)]
pub struct GhDomainItem { pub key: String, pub label: String, pub topics: Vec<String> }

/// 给前端渲染领域多选框。
#[tauri::command]
fn gh_domains_catalog() -> Vec<GhDomainItem> {
    GH_DOMAINS.iter().map(|d| GhDomainItem {
        key: d.key.to_string(),
        label: d.label.to_string(),
        topics: d.topics.iter().map(|t| t.to_string()).collect(),
    }).collect()
}

/// 保存某账号所选领域（仅保留合法 key）。
#[tauri::command]
fn set_account_gh_domains(state: State<AppState>, account_id: String, domains: Vec<String>) -> Result<(), String> {
    let valid: Vec<String> = domains.into_iter()
        .filter(|k| GH_DOMAINS.iter().any(|d| d.key == k))
        .collect();
    let json = serde_json::to_string(&valid).map_err(|e| e.to_string())?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE accounts SET gh_domains=?1 WHERE id=?2", params![json, account_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 2: 注册命令**

在 `tauri::generate_handler![` 列表里加入：

```rust
            gh_domains_catalog,
            set_account_gh_domains,
```

- [ ] **Step 3: 编译确认**

Run: `cd src-tauri && cargo check`
Expected: 无 error

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): gh_domains_catalog + set_account_gh_domains 命令"
```

---

### Task 7: GitHub L1 编排 `github_nurture_run` + nurture arm 分发

**Files:**
- Modify: `src-tauri/src/lib.rs`（nurture arm dispatch 点 ~`engine_select_profile` 之后；新增 `github_nurture_run` async）

说明：浏览器动作不能在测试里跑，本任务用「编译通过 + 手动集成」验证。选择器为 best-effort，需在 Step 4 对照实时 GitHub 校准。

- [ ] **Step 1: 加分发**

在 nurture arm 的 dry-run 检查之后、`let started = Utc::now()...` 之前插入：

```rust
            if platform.eq_ignore_ascii_case("github") {
                return match github_nurture_run(app, &account_id, duration).await {
                    Ok(msg) => { log::info!("[GH-NURTURE] {}", msg); TaskOutcome::Success(None) }
                    Err(e) if e.contains("未登录") => {
                        let st = app.state::<AppState>();
                        if let Ok(c) = st.db.lock() {
                            let _ = c.execute("UPDATE accounts SET health_status='logged_out', last_health_check=datetime('now') WHERE id=?1", params![account_id]);
                        }
                        TaskOutcome::Blocked(e)
                    }
                    Err(e) => TaskOutcome::Retry(format!("GitHub 养号失败: {}", e)),
                };
            }
```

- [ ] **Step 2: 实现编排**

```rust
/// GitHub L1 养号：按账号领域，从 topic 页采候选 → 去重选取 → star/follow/watch。
/// 浏览器调用走 spawn_blocking（阻塞 reqwest 不能在 async 直接调，见项目约定）。
async fn github_nurture_run(app: &AppHandle, account_id: &str, _duration: i64) -> Result<String, String> {
    // 1) 读领域 + 当前分期/配额（DB，快速持锁）
    let (domains, phase) = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        let domains = account_gh_domains(&conn, account_id);
        // 号龄→分期：复用 nurture_strategies.warmup_days + nurture_phase_and_target
        let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
        let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
        let strat = conn.query_row("SELECT warmup_days, daily_sessions_min, daily_sessions_max FROM nurture_strategies WHERE platform='github'",
            [], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?, r.get::<_,i64>(2)?))).ok();
        let (warmup, smin, smax) = strat.unwrap_or((3, 2, 5));
        let (phase, _t) = nurture_phase_and_target(age, warmup, smin, smax);
        (domains, phase.to_string())
    };
    if domains.is_empty() {
        return Ok("账号未选领域，跳过 GitHub 养号".to_string());
    }
    let (n_star, n_follow, n_watch) = gh_daily_quota(&phase);

    // 2) 选一个领域 → 取 topics → 选一个 topic（用时间种子）
    let dom_keys: Vec<&str> = domains.iter().map(|s| s.as_str()).collect();
    let topics = gh_domain_topics(&dom_keys);
    if topics.is_empty() { return Ok("领域无可用 topic".to_string()); }
    let seed = get_random_delay(1, 1_000_000) as u64; // 复用既有时间派生随机
    let topic = topics[(seed as usize) % topics.len()];

    // 3) 浏览器：导航 topic 页（按 star 排序）并采 repo 链接
    let topic_owned = topic.to_string();
    let repo_links: Vec<String> = tauri::async_runtime::spawn_blocking(move || {
        let url = format!("https://github.com/topics/{}?o=desc&s=stars", topic_owned);
        unzoo_navigate(&url)?;
        std::thread::sleep(std::time::Duration::from_secs(3));
        // 登录校验：topic 页右上若出现登录入口说明未登录
        if !check_platform_login_status("github").unwrap_or(false) {
            return Err("未登录 github".to_string());
        }
        // 仓库卡片标题链接： <h3> 下的 a，形如 /owner/repo
        unzoo_get_links("article h3 a[href^='/'], h3 a[href*='/']")
    }).await.map_err(|e| format!("采集异常: {}", e))??;

    // 规范化为绝对 repo URL（owner/repo 两段）
    let mut repos: Vec<String> = repo_links.into_iter()
        .filter_map(|h| {
            let path = h.trim_start_matches("https://github.com").trim_start_matches('/');
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if segs.len() == 2 { Some(format!("https://github.com/{}/{}", segs[0], segs[1])) } else { None }
        }).collect();
    repos.dedup();

    // 4) DB 过滤：本账号已操作 + 跨账号触碰过多（>3 个 persona 碰过的就跳过）
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

    // 5) 浏览器：对选中 repo star（含拟人停留），并 follow/watch 少量
    let mut done = 0i64;
    for repo in &chosen {
        let repo_c = repo.clone();
        let ok = tauri::async_runtime::spawn_blocking(move || gh_star_repo_blocking(&repo_c)).await
            .map_err(|e| format!("star 异常: {}", e))?;
        if ok.is_ok() {
            let st = app.state::<AppState>();
            if let Ok(conn) = st.db.lock() { let _ = gh_record_action(&conn, account_id, "star", repo); }
            done += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(get_random_delay(60, 180) * 1000));
    }
    let _ = (n_follow, n_watch); // follow/watch 见 Task 7b（可与 star 同构实现）

    // 6) 写养号统计（与通用养号一致）
    {
        let st = app.state::<AppState>();
        if let Ok(conn) = st.db.lock() {
            let now = Utc::now().to_rfc3339();
            let today = Local::now().format("%Y-%m-%d").to_string();
            let _ = conn.execute("UPDATE accounts SET last_nurture_at=?1, health_status='healthy', last_health_check=?1 WHERE id=?2", params![now, account_id]);
            let _ = conn.execute(
                "INSERT INTO nurture_daily_logs (id, account_id, date, sessions_completed, total_seconds) VALUES (?1,?2,?3,1,0) \
                 ON CONFLICT(account_id,date) DO UPDATE SET sessions_completed=sessions_completed+1",
                params![Uuid::new_v4().to_string(), account_id, today]);
        }
    }
    Ok(format!("GitHub 养号完成：topic={} star={}", topic, done))
}

/// 在 repo 页点 Star（best-effort 选择器，需对照实时 GitHub 校准）。
fn gh_star_repo_blocking(repo_url: &str) -> Result<(), String> {
    unzoo_navigate(repo_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5) * 1000));
    // 已 star 时按钮文案为 Unstar/Starred，跳过避免取消 star
    let star_selectors = [
        "button[aria-label^='Star this']",
        "button[aria-label^='Star ']",
        "form[action$='/star'] button",
    ];
    for sel in star_selectors {
        if unzoo_element_exists(sel) {
            unzoo_click(sel).map_err(|e| format!("点击 star 失败: {}", e))?;
            std::thread::sleep(std::time::Duration::from_millis(800));
            return Ok(());
        }
    }
    Err("未找到 star 按钮（可能已 star / 改版 / 未登录）".to_string())
}
```

- [ ] **Step 3: 编译确认**

Run: `cd src-tauri && cargo check`
Expected: 无 error（warning 可接受）

- [ ] **Step 4: 手动集成验证（需 Unzoo + 已登录 github 号）**

1. 给一个 github 账号设置领域（前端或直接 `set_account_gh_domains`）。
2. 启动 app（`npm run build` 后 `cargo tauri dev`），手动入队该账号 github 养号任务。
3. 观察：浏览器导航到 `github.com/topics/<topic>`，进入某 repo 页并成功点亮 Star。
4. 校准：若 star 按钮没点中，用浏览器 DevTools 查实际选择器，更新 `star_selectors` 后重试。
5. 确认 `gh_actions_log` 出现对应 star 记录。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): L1 编排 github_nurture_run（star）+ nurture 分发"
```

---

### Task 7b: 补 follow / watch 动作（与 star 同构）

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 实现 follow / watch 阻塞助手**

```rust
/// 在用户 profile 页点 Follow（best-effort）。
fn gh_follow_user_blocking(user_url: &str) -> Result<(), String> {
    unzoo_navigate(user_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5) * 1000));
    for sel in ["form[action$='/follow'] button", "button[aria-label^='Follow']"] {
        if unzoo_element_exists(sel) {
            unzoo_click(sel).map_err(|e| format!("点击 follow 失败: {}", e))?;
            std::thread::sleep(std::time::Duration::from_millis(800));
            return Ok(());
        }
    }
    Err("未找到 follow 按钮".to_string())
}

/// 在 repo 页点 Watch → 选 “All Activity / Participating”（best-effort）。
fn gh_watch_repo_blocking(repo_url: &str) -> Result<(), String> {
    unzoo_navigate(repo_url)?;
    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(2, 5) * 1000));
    for sel in ["button[aria-label*='watch' i]", "summary[aria-label*='Notifications']"] {
        if unzoo_element_exists(sel) {
            unzoo_click(sel).map_err(|e| format!("点击 watch 失败: {}", e))?;
            std::thread::sleep(std::time::Duration::from_millis(800));
            return Ok(());
        }
    }
    Err("未找到 watch 入口".to_string())
}
```

- [ ] **Step 2: 在 `github_nurture_run` 第 5 步后接入 follow（取 chosen repo 的 owner profile）与 watch**

把 `let _ = (n_follow, n_watch);` 替换为：

```rust
    // follow：对已 star 的 repo，follow 其 owner（owner profile = 去掉 /repo 段）
    if n_follow > 0 {
        for repo in chosen.iter().take(n_follow as usize) {
            let owner_url = repo.rsplitn(2, '/').nth(1).unwrap_or(repo).to_string(); // https://github.com/owner
            if owner_url.matches('/').count() == 3 {
                let st = app.state::<AppState>();
                let acted = { let conn = st.db.lock().map_err(|e| e.to_string())?; gh_already_acted(&conn, account_id, &owner_url) };
                if !acted {
                    let u = owner_url.clone();
                    if tauri::async_runtime::spawn_blocking(move || gh_follow_user_blocking(&u)).await.map_err(|e| e.to_string())?.is_ok() {
                        if let Ok(conn) = st.db.lock() { let _ = gh_record_action(&conn, account_id, "follow", &owner_url); }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(get_random_delay(60, 180) * 1000));
                }
            }
        }
    }
    // watch：对第一个 star 的 repo watch
    if n_watch > 0 {
        if let Some(repo) = chosen.first() {
            let st = app.state::<AppState>();
            let acted = { let conn = st.db.lock().map_err(|e| e.to_string())?; gh_already_acted(&conn, account_id, &format!("{}#watch", repo)) };
            if !acted {
                let r = repo.clone();
                if tauri::async_runtime::spawn_blocking(move || gh_watch_repo_blocking(&r)).await.map_err(|e| e.to_string())?.is_ok() {
                    if let Ok(conn) = st.db.lock() { let _ = gh_record_action(&conn, account_id, "watch", &format!("{}#watch", repo)); }
                }
            }
        }
    }
```

- [ ] **Step 3: 编译 + 手动校准选择器**（同 Task 7 Step 4 流程，验证 follow/watch 命中）

Run: `cd src-tauri && cargo check`
Expected: 无 error

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): 补 follow/watch L1 动作"
```

---

### Task 8: 前端——persona 开通时的领域多选

**Files:**
- Modify: `src/tauri-frontend/app.ts`（在开通 github 账号的流程加多选；复用 `pickProvisionPlatforms` 同款弹窗范式）

- [ ] **Step 1: 加领域多选弹窗 + 持久化**

在 `app.ts` 合适处（紧邻 `pickProvisionPlatforms` 暴露处）加：

```ts
interface GhDomainItem { key: string; label: string; topics: string[] }

(window as any).pickGithubDomains = async function(accountId: string): Promise<void> {
  const cat = await invoke<GhDomainItem[]>('gh_domains_catalog');
  // 复用现有 modal 容器（.modal.active 显示）；这里用一个轻量动态弹窗
  const overlay = document.createElement('div');
  overlay.className = 'modal active';
  overlay.innerHTML = `
    <div class="modal-content">
      <div class="modal-header"><h3>选择 GitHub 养号领域（可多选）</h3></div>
      <div class="modal-body">
        ${cat.map(d => `<label style="display:block;margin:6px 0;">
          <input type="checkbox" value="${d.key}"> ${d.label}
          <span style="color:var(--text-muted);font-size:12px;">(${d.topics.slice(0,4).join(', ')}…)</span>
        </label>`).join('')}
      </div>
      <div class="modal-footer">
        <button class="btn" id="ghDomCancel">取消</button>
        <button class="btn btn-success" id="ghDomSave">保存</button>
      </div>
    </div>`;
  document.body.appendChild(overlay);
  overlay.querySelector('#ghDomCancel')!.addEventListener('click', () => overlay.remove());
  overlay.querySelector('#ghDomSave')!.addEventListener('click', async () => {
    const keys = Array.from(overlay.querySelectorAll<HTMLInputElement>('input:checked')).map(i => i.value);
    try {
      await invoke('set_account_gh_domains', { accountId, domains: keys });
      showToast('GitHub 领域已保存', 'success');
      overlay.remove();
    } catch (e) { showToast('保存失败: ' + e, 'error'); }
  });
};
```

- [ ] **Step 2: 在 github 账号卡片加入口按钮**

在渲染账号卡片处（`renderAccounts`，platform==='github' 时）加一个按钮：

```ts
${account.platform === 'github' ? `<button class="btn btn-small btn-secondary" onclick="pickGithubDomains('${account.id}')" title="选择养号领域">🎯 领域</button>` : ''}
```

- [ ] **Step 3: 构建前端**

Run: `cd /Users/jinguichao/workspace/sec-zt/marketgo && npm run build && npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js`
Expected: 构建成功

- [ ] **Step 4: 手动验证**：github 账号卡片出现「🎯 领域」按钮，点开能勾选并保存，`accounts.gh_domains` 写入。

- [ ] **Step 5: 提交**

```bash
git add src/tauri-frontend/app.ts dist/tauri/scripts/app.js
git commit -m "feat(github-nurture): 前端 persona 领域多选"
```

---

## Phase 2 — L2（良性评论）

### Task 9: L2 评论生成 + 入审核队列（复用现有回复管线）

**Files:**
- Modify: `src-tauri/src/lib.rs`（在 `github_nurture_run` 末尾、统计之前接入 L2；新增 `gh_benign_comment` 文案生成）
- Test: `src-tauri/src/lib.rs` 测试区

- [ ] **Step 1: 写失败测试（文案生成是纯函数）**

```rust
    #[test]
    fn gh_benign_comment_is_nonpromotional() {
        for i in 0..5 {
            let c = gh_benign_comment(i);
            assert!(!c.is_empty());
            assert!(!c.contains("http")); // 不带链接/推广
        }
        // 不同 seed 可取到不同文案
        assert_ne!(gh_benign_comment(0), gh_benign_comment(1));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test gh_benign_comment`
Expected: 编译失败

- [ ] **Step 3: 实现良性文案 + L2 接入**

```rust
/// 良性、不带推广意图的短评论（养号阶段建立真人感，绝不带链接/产品）。
fn gh_benign_comment(seed: u64) -> String {
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
```

在 `github_nurture_run` 第 6 步（统计）之前接入 L2（受闸门 + 审核队列约束）：

```rust
    // L2：满足闸门时，在所选领域 repo 的某 Issue/Discussion 下入一条待审核评论
    {
        let st = app.state::<AppState>();
        let (age, l1_count, mode) = {
            let conn = st.db.lock().map_err(|e| e.to_string())?;
            let created: Option<String> = conn.query_row("SELECT created_at FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok().flatten();
            let age = created.as_deref().and_then(parse_dt).map(|c| (Utc::now() - c).num_days()).unwrap_or(0);
            let l1: i64 = conn.query_row("SELECT COUNT(*) FROM gh_actions_log WHERE account_id=?1 AND action_type IN ('star','follow','watch')", params![account_id], |r| r.get(0)).unwrap_or(0);
            (age, l1, engine_reply_mode(&conn))
        };
        // 每周 2~4 条节流：本周已评论数 < 上限才继续
        let weekly: i64 = {
            let conn = st.db.lock().map_err(|e| e.to_string())?;
            conn.query_row("SELECT COUNT(*) FROM gh_actions_log WHERE account_id=?1 AND action_type='comment' AND date >= date('now','-7 day')", params![account_id], |r| r.get(0)).unwrap_or(0)
        };
        if gh_l2_allowed(age, &phase, l1_count) && weekly < 3 {
            // 取一个 star 过的 repo 的 issues，挑第一个开放 issue 线程
            if let Some(repo) = chosen.first() {
                let issues_url = format!("{}/issues?q=is%3Aissue+is%3Aopen", repo);
                let iu = issues_url.clone();
                let thread = tauri::async_runtime::spawn_blocking(move || {
                    unzoo_navigate(&iu)?;
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let links = unzoo_get_links("a[href*='/issues/']")?;
                    Ok::<Option<String>, String>(links.into_iter().find(|h| h.contains("/issues/") && h.chars().filter(|c| *c=='/').count() >= 5))
                }).await.map_err(|e| e.to_string())??;
                if let Some(thread_url) = thread {
                    let already = { let conn = st.db.lock().map_err(|e| e.to_string())?; gh_already_acted(&conn, account_id, &thread_url) };
                    if !already {
                        let text = gh_benign_comment(seed);
                        if mode == "auto" {
                            let tu = thread_url.clone(); let tx = text.clone();
                            let _ = tauri::async_runtime::spawn_blocking(move || post_reply_to_url("github", &tu, &tx)).await.map_err(|e| e.to_string())?;
                        } else {
                            // 半自动：入 reply_history 审核队列，由现有审核 UI 批准后再发
                            let conn = st.db.lock().map_err(|e| e.to_string())?;
                            let _ = conn.execute(
                                "INSERT INTO reply_history (id, platform, post_url, reply_content, status) VALUES (?1,'github',?2,?3,'pending_review')",
                                params![Uuid::new_v4().to_string(), thread_url, text]);
                        }
                        if let Ok(conn) = st.db.lock() { let _ = gh_record_action(&conn, account_id, "comment", &thread_url); }
                    }
                }
            }
        }
    }
```

- [ ] **Step 4: 运行单测 + 编译**

Run: `cd src-tauri && cargo test gh_benign_comment && cargo check`
Expected: 单测 PASS、无 error

- [ ] **Step 5: 手动集成验证**：号龄≥7天、有 L1 历史、reply 模式=review 时，跑一次养号 → `reply_history` 出现一条 `github / pending_review` 评论；现有审核 UI 能看到并批准。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(github-nurture): L2 良性评论（闸门+审核队列复用）"
```

---

## 验收清单

- [ ] `cargo test`（gh 相关 + 既有）全绿
- [ ] `cargo check` 无 error
- [ ] 前端构建成功，github 卡片有「🎯 领域」入口、可保存
- [ ] 手动：L1 能在所选领域 star/follow/watch，`gh_actions_log` 有记录、跨账号去重生效
- [ ] 手动：L2 在闸门满足时入审核队列（review 模式）
- [ ] 选择器已对照实时 GitHub 校准（Task 7/7b Step 4）

## 明确不做（YAGNI，承接 spec §9）

- 不做 L3 绿格 / commit；不引入 API/PAT/SSH；不自动补全 profile；养号阶段不做推广。
