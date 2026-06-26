# 身份统一为 Gmail + 账号级 SOCKS5 覆盖 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把自定义固定代理能力从「身份级」下放到「账号级」——身份统一为 Gmail(机场)，每个账号可配 SOCKS5 覆盖出口（没配走身份机场节点）。

**Architecture:** 后端在 `accounts` 加 `custom_proxy` 列；新增 `apply_account_proxy` 在启动账号 profile 后动态设 profile 出口代理（有自定义用它、否则回机场端口），接入养号/预检/任务引擎三处；启动时一次性删除已有 fixed 身份。前端删两个固定身份分类、放开加账号平台、账号卡片加「🧦 SOCKS5」按钮+批量弹框。

**Tech Stack:** Rust(Tauri v2, rusqlite), TypeScript(esbuild bundle), unzoo MCP HTTP, mihomo。

参考设计：`docs/superpowers/specs/2026-06-26-identity-unify-account-socks5-design.md`

---

## 文件结构

- `src-tauri/src/lib.rs` — `normalize_proxy` 纯函数、`Account` 结构体加字段、账号列表 SELECT、`custom_proxy` 列迁移、`set_account_proxy`/`set_accounts_proxy`/`test_account_proxy` 命令注册、三处接入点、fixed 身份清理启动钩子、单测。
- `src-tauri/src/multi_account.rs` — `apply_account_proxy`、`test_account_proxy_inner`、`drop_fixed_personas_once`（复用 `persona_delete`/`resolve_profile_path`/`unzoo_set_profile_proxy2`/`metrics`）。
- `src/tauri-frontend/app.ts` — 删 fixed 分类、放开加账号、账号卡片 🧦 按钮、单账号弹框、批量弹框、i18n 清理。
- `dist/tauri/index.html` — 批量 SOCKS5 弹框 DOM。
- 打包：`npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js --format=iife --platform=browser`

每个 Rust 任务结束跑 `cd src-tauri && cargo test --lib <name>`；前端任务结束跑 `npx tsc --noEmit` + esbuild。

---

## Task 1: `normalize_proxy` 代理规范化纯函数

**Files:**
- Modify: `src-tauri/src/lib.rs`（紧挨 `stop_nurture` 命令之后，约 9596 行附近插入）
- Test: `src-tauri/src/lib.rs`（新 `#[cfg(test)] mod proxy_normalize_tests`，置于文件末尾测试区，紧跟 `login_precheck_tests` 之后）

- [ ] **Step 1: 写实现（先放实现，再写测试驱动边界）**

在 `src-tauri/src/lib.rs` 的 `fn stop_nurture()` 结束花括号之后插入：

```rust
/// 规范化账号自定义代理：trim；空→None；无协议前缀补 socks5://；仅允许 socks5/http/https。
/// 账号级 SOCKS5 配置入库前统一走这里。纯逻辑，可单测。
pub(crate) fn normalize_proxy(input: &str) -> Result<Option<String>, String> {
    let s = input.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let s = if s.contains("://") { s.to_string() } else { format!("socks5://{}", s) };
    if s.starts_with("socks5://") || s.starts_with("http://") || s.starts_with("https://") {
        Ok(Some(s))
    } else {
        Err("代理需为 socks5:// / http:// / https:// 或 host:port".to_string())
    }
}
```

- [ ] **Step 2: 写失败测试**

在 `src-tauri/src/lib.rs` 末尾 `mod login_precheck_tests { ... }` 之后插入：

```rust
#[cfg(test)]
mod proxy_normalize_tests {
    use crate::normalize_proxy;

    #[test]
    fn normalize_proxy_rules() {
        // 空 / 纯空白 → None
        assert_eq!(normalize_proxy("").unwrap(), None);
        assert_eq!(normalize_proxy("   ").unwrap(), None);
        // host:port 无协议 → 补 socks5://
        assert_eq!(normalize_proxy("1.2.3.4:18080").unwrap(), Some("socks5://1.2.3.4:18080".to_string()));
        // 带账号密码
        assert_eq!(normalize_proxy("u:p@host:1080").unwrap(), Some("socks5://u:p@host:1080".to_string()));
        // 已带协议保持原样
        assert_eq!(normalize_proxy("http://h:8080").unwrap(), Some("http://h:8080".to_string()));
        assert_eq!(normalize_proxy("socks5://h:1080").unwrap(), Some("socks5://h:1080".to_string()));
        // 非法协议 → Err
        assert!(normalize_proxy("ftp://h:21").is_err());
    }
}
```

- [ ] **Step 3: 跑测试**

Run: `cd src-tauri && cargo test --lib proxy_normalize`
Expected: PASS（1 passed）

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(proxy): normalize_proxy 账号代理规范化纯函数 + 单测"
```

---

## Task 2: `accounts.custom_proxy` 列 + Account 结构体 + 列表查询

**Files:**
- Modify: `src-tauri/src/lib.rs:3227`（Account 结构体）、`:3695` 区（列迁移）、`:4090` 区（SELECT + 映射）

- [ ] **Step 1: 加列迁移**

在 `src-tauri/src/lib.rs` 约 3695 行（`ALTER TABLE accounts ADD COLUMN nurture_topics TEXT` 这行之后）加一行：

```rust
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN custom_proxy TEXT", []);
```

- [ ] **Step 2: Account 结构体加字段**

在 `src-tauri/src/lib.rs` 的 `pub struct Account` 里、`login_method` 字段之后加：

```rust
    #[serde(default)]
    pub custom_proxy: Option<String>,    // 账号级自定义 SOCKS5/HTTP 代理（空=走身份机场节点）
```

- [ ] **Step 3: 列表 SELECT 与映射带上 custom_proxy**

在 `src-tauri/src/lib.rs` 约 4090 的账号列表 `stmt`，把 SELECT 改为（在 `p.email` 后加 `a.custom_proxy`）：

```rust
    let mut stmt = conn.prepare(
        "SELECT a.id, a.platform, a.username, a.email, a.status, a.created_at, a.profile_id, \
                COALESCE(a.health_status,'unknown'), COALESCE(a.total_nurture_seconds,0), a.last_nurture_at, \
                a.persona_id, p.email, a.custom_proxy \
         FROM accounts a LEFT JOIN personas p ON p.id = a.persona_id ORDER BY a.created_at DESC")
        .map_err(|e| e.to_string())?;
```

并在同一函数的 `Ok(Account { ... })` 映射里、`login_method,` 之后加：

```rust
            custom_proxy: row.get(12)?,
```

- [ ] **Step 4: 编译验证**

Run: `cd src-tauri && cargo check --lib 2>&1 | tail -3`
Expected: `Finished`（仅 warnings）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(accounts): 增加 custom_proxy 列并随账号列表返回"
```

---

## Task 3: `set_account_proxy` / `set_accounts_proxy` 命令

**Files:**
- Modify: `src-tauri/src/lib.rs`（`normalize_proxy` 之后加两个命令）、`:12372` 区（注册）

- [ ] **Step 1: 写命令**

在 `src-tauri/src/lib.rs` 的 `normalize_proxy` 函数之后插入：

```rust
/// 设置单个账号的自定义代理（None/空=清除，走身份机场节点）。
#[tauri::command]
fn set_account_proxy(state: State<AppState>, account_id: String, proxy: Option<String>) -> Result<(), String> {
    let normalized = normalize_proxy(proxy.as_deref().unwrap_or(""))?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE accounts SET custom_proxy = ?1 WHERE id = ?2",
        params![normalized, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// 批量设置多个账号的自定义代理（None/空=清除）。返回成功条数。
#[tauri::command]
fn set_accounts_proxy(state: State<AppState>, account_ids: Vec<String>, proxy: Option<String>) -> Result<usize, String> {
    let normalized = normalize_proxy(proxy.as_deref().unwrap_or(""))?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut n = 0usize;
    for id in &account_ids {
        if conn.execute("UPDATE accounts SET custom_proxy = ?1 WHERE id = ?2", params![normalized, id]).unwrap_or(0) > 0 {
            n += 1;
        }
    }
    Ok(n)
}
```

- [ ] **Step 2: 注册命令**

在 `src-tauri/src/lib.rs` 的 `invoke_handler` 里，`check_account_login,` 之后加：

```rust
            set_account_proxy,
            set_accounts_proxy,
```

- [ ] **Step 3: 编译验证**

Run: `cd src-tauri && cargo check --lib 2>&1 | tail -3`
Expected: `Finished`（仅 warnings）

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(proxy): set_account_proxy / set_accounts_proxy 命令"
```

---

## Task 4: `apply_account_proxy` 动态生效 + `test_account_proxy`

**Files:**
- Modify: `src-tauri/src/multi_account.rs`（新增两个 `pub(crate)` 函数）、`src-tauri/src/lib.rs`（`test_account_proxy` 命令包装 + 注册）

- [ ] **Step 1: multi_account 加 apply_account_proxy**

在 `src-tauri/src/multi_account.rs` 的 `persona_test_ip` 函数之后插入：

```rust
/// 启动账号 profile 后、操作前调用：按账号决定 profile 出口代理。
/// 有 custom_proxy → 用它；否则身份是机场(local_port 非空) → 设回机场端口；都没有 → 不动。
/// 设代理失败仅记日志、不阻断操作（退回 profile 当前代理）。
pub(crate) async fn apply_account_proxy(app: &AppHandle, account_id: &str) -> Result<(), String> {
    let (custom, profile_id, local_port): (Option<String>, Option<String>, Option<i64>) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT a.custom_proxy, COALESCE(p.profile_id, a.profile_id), p.local_port \
             FROM accounts a LEFT JOIN personas p ON p.id = a.persona_id WHERE a.id = ?1",
            params![account_id],
            |r| Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
            )),
        ).map_err(|e| e.to_string())?
    };
    let pid = match profile_id { Some(p) if !p.is_empty() => p, _ => return Ok(()) }; // 无 profile 不处理
    let path = match resolve_profile_path(&pid).await { Some(p) => p, None => return Ok(()) };
    let proxy = if let Some(cp) = custom.filter(|s| !s.trim().is_empty()) {
        cp
    } else if let Some(port) = local_port {
        format!("socks5://127.0.0.1:{}", port)
    } else {
        return Ok(()); // 未归属且无自定义 → 不动
    };
    if let Err(e) = unzoo_set_profile_proxy2(&path, &proxy).await {
        log::warn!("[PROXY] 账号 {} 设代理失败: {}", account_id, e);
    } else {
        log::info!("[PROXY] 账号 {} 出口 → {}", account_id, proxy);
    }
    Ok(())
}

/// 测试账号当前出口 IP：先按账号 apply 代理，再开 profile 导航 IP 服务。供前端「测试出口IP」按钮用。
pub(crate) async fn test_account_proxy(app: AppHandle, account_id: String) -> Result<String, String> {
    apply_account_proxy(&app, &account_id).await?;
    let profile_id: String = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        conn.query_row(
            "SELECT COALESCE(p.profile_id, a.profile_id) FROM accounts a \
             LEFT JOIN personas p ON p.id = a.persona_id WHERE a.id = ?1",
            params![account_id], |r| r.get::<_, Option<String>>(0))
            .map_err(|_| "账号不存在".to_string())?
            .ok_or("账号无可用 profile".to_string())?
    };
    let path = resolve_profile_path(&profile_id).await.ok_or("找不到 profile 路径".to_string())?;
    let tab_id = {
        let client = get_http_client();
        let resp = client.post(format!("{}/profiles/launch", UNZOO_API_BASE))
            .json(&serde_json::json!({"profile_path": path})).send().await.map_err(|e| e.to_string())?;
        let v: serde_json::Value = resp.json().await.unwrap_or_default();
        v.get("data").and_then(|d| d.get("tab_id")).map(|t| if let Some(n)=t.as_i64(){n.to_string()}else if let Some(s)=t.as_str(){s.to_string()}else{String::new()}).unwrap_or_default()
    };
    if tab_id.is_empty() { return Err("启动 profile 失败".into()); }
    let tid = tab_id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        metrics::metrics_navigate(&tid, "https://api.ip.sb/geoip")?;
        std::thread::sleep(std::time::Duration::from_millis(2500));
        metrics::metrics_evaluate(&tid, "(document.body&&document.body.innerText)||''")
    }).await.map_err(|e| e.to_string())??;
    let txt = serde_json::from_str::<String>(&res).unwrap_or(res);
    let v: serde_json::Value = serde_json::from_str(&txt).unwrap_or(serde_json::json!({}));
    let ip = v.get("ip").and_then(|x| x.as_str()).unwrap_or("?");
    let country = v.get("country").and_then(|x| x.as_str()).unwrap_or("");
    let city = v.get("city").and_then(|x| x.as_str()).unwrap_or("");
    Ok(format!("出口 IP：{}  ({} {})", ip, country, city))
}
```

- [ ] **Step 2: lib.rs 加 test_account_proxy 命令包装 + 注册**

在 `src-tauri/src/lib.rs` 的 `set_accounts_proxy` 命令之后加：

```rust
/// 测试账号当前出口 IP（按账号 apply 代理后查 IP）。
#[tauri::command]
async fn test_account_proxy(app: AppHandle, account_id: String) -> Result<String, String> {
    multi_account::test_account_proxy(app, account_id).await
}
```

在 `invoke_handler` 里 `set_accounts_proxy,` 之后加：

```rust
            test_account_proxy,
```

- [ ] **Step 3: 编译验证**

Run: `cd src-tauri && cargo check --lib 2>&1 | tail -3`
Expected: `Finished`（仅 warnings）

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/src/multi_account.rs
git commit -m "feat(proxy): apply_account_proxy 动态切代理 + test_account_proxy 测出口IP"
```

---

## Task 5: 接入三处入口（养号 / 登录预检 / 任务引擎）

**Files:**
- Modify: `src-tauri/src/lib.rs` — `quick_nurture`(:9547 区)、`check_account_login`、`engine_select_profile`(:10785 区)

- [ ] **Step 1: 接入 quick_nurture**

在 `src-tauri/src/lib.rs` 的 `quick_nurture` 里，`set_active_tab(Some(tab_id));`（约 9547）之后插入：

```rust
    // 账号级代理覆盖：有 custom_proxy 走它，否则走身份机场节点。
    let _ = multi_account::apply_account_proxy(&app, &account_id).await;
```

- [ ] **Step 2: 接入 check_account_login**

在 `src-tauri/src/lib.rs` 的 `check_account_login` 里，`set_active_tab(Some(tab_id));` 之后插入同样一行：

```rust
    let _ = multi_account::apply_account_proxy(&app, &account_id).await;
```

注意：`check_account_login` 现签名是 `(state, account_id)`，需要 `app: AppHandle`。在该命令参数表最前面加 `app: AppHandle,`（Tauri 会自动注入；前端 invoke 不需改）。

- [ ] **Step 3: 接入 engine_select_profile**

在 `src-tauri/src/lib.rs` 的 `engine_select_profile`（约 10761）里，确保**复用与新开两条路径都 apply**。把函数末尾的两处 `return Ok(())`/结尾改造：在 `if same_profile && get_active_tab().is_some() { ... return Ok(()); }` 的 return 之前、以及函数最后 `Ok(())` 之前，都插入：

```rust
    if let Some(aid) = account_id {
        let _ = apply_account_proxy_via(app, aid).await;
    }
```

并在 `engine_select_profile` 之前加一个轻量包装（避免 import 歧义）：

```rust
async fn apply_account_proxy_via(app: &AppHandle, account_id: &str) -> Result<(), String> {
    multi_account::apply_account_proxy(app, account_id).await
}
```

具体地，复用路径改为：

```rust
    if same_profile && get_active_tab().is_some() {
        if let Some(aid) = account_id { let _ = apply_account_proxy_via(app, aid).await; }
        log::info!("[ENGINE] reuse profile {} for {} (tab kept)", profile_id, platform);
        return Ok(());
    }
```

新开路径在函数结尾 `Ok(())` 之前：

```rust
    if let Some(aid) = account_id { let _ = apply_account_proxy_via(app, aid).await; }
    Ok(())
}
```

（`account_id: &Option<String>`，`if let Some(aid) = account_id` 得到 `&String`，`aid` 传 `&str` 用 `aid` 即可，因 `apply_account_proxy_via` 收 `&str` → 传 `aid.as_str()`；若类型不符改为 `aid.as_str()`。）

- [ ] **Step 4: 编译验证**

Run: `cd src-tauri && cargo check --lib 2>&1 | tail -3`
Expected: `Finished`（仅 warnings）。若报 `account_id` 借用/类型错，按提示把 `aid` 改为 `aid.as_str()`。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(proxy): 养号/登录预检/任务引擎启动 profile 后接入 apply_account_proxy"
```

---

## Task 6: 启动一次性删除已有 fixed 身份

**Files:**
- Modify: `src-tauri/src/multi_account.rs`（`drop_fixed_personas_once`）、`src-tauri/src/lib.rs:12543` 区（启动调用）
- Test: `src-tauri/src/lib.rs`（SQL 迁移核心逻辑单测）

- [ ] **Step 1: multi_account 加清理函数**

在 `src-tauri/src/multi_account.rs` 的 `apply_account_proxy` 之后插入：

```rust
/// 一次性：删除所有「固定 IP 身份」(ip_mode='fixed')。删其 unzoo profile、解除账号关联(账号保留为未归属)、
/// 删 persona。受 config flag `migrated_drop_fixed_personas` 守护，只跑一次。
pub(crate) async fn drop_fixed_personas_once(app: &AppHandle) {
    let ids: Vec<String> = {
        let state = app.state::<AppState>();
        let conn = match state.db.lock() { Ok(c) => c, Err(_) => return };
        if crate::engine_cfg_get(&conn, "migrated_drop_fixed_personas").is_some() {
            return;
        }
        let mut stmt = match conn.prepare("SELECT id FROM personas WHERE ip_mode='fixed'") { Ok(s) => s, Err(_) => return };
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map(|it| it.filter_map(|x| x.ok()).collect()).unwrap_or_default();
        rows
    };
    for id in &ids {
        // 复用 persona_delete：删 profile + 解除账号关联 + 删 persona + 重建 mihomo。
        let _ = persona_delete(app.clone(), id.clone()).await;
    }
    {
        let state = app.state::<AppState>();
        if let Ok(conn) = state.db.lock() {
            crate::engine_cfg_set(&conn, "migrated_drop_fixed_personas", "1");
        }
    }
    if !ids.is_empty() {
        log::info!("[MIGRATE] 已删除 {} 个固定 IP 身份（账号转为未归属）", ids.len());
    }
}
```

- [ ] **Step 2: 启动调用**

在 `src-tauri/src/lib.rs` 约 12543 行 `std::thread::spawn(move || { multi_account::mihomo_boot(&handle); });` 之后插入：

```rust
                {
                    let handle2 = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        multi_account::drop_fixed_personas_once(&handle2).await;
                    });
                }
```

（`app` 为 setup 闭包里的 `&mut App`；用 `app.handle().clone()` 取 AppHandle。若该作用域变量名不同，按上下文取到 AppHandle 即可。）

- [ ] **Step 3: 写 SQL 行为单测（验证迁移效果，不依赖浏览器）**

在 `src-tauri/src/lib.rs` 末尾 `proxy_normalize_tests` 之后插入：

```rust
#[cfg(test)]
mod drop_fixed_personas_tests {
    use rusqlite::{params, Connection};

    /// 模拟 drop_fixed_personas_once 的纯 DB 部分：fixed persona 删除 + 账号解除关联。
    fn drop_fixed_sql(conn: &Connection) {
        let ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM personas WHERE ip_mode='fixed'").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().filter_map(|x| x.ok()).collect()
        };
        for id in &ids {
            conn.execute("UPDATE accounts SET persona_id=NULL WHERE persona_id=?1", params![id]).unwrap();
            conn.execute("DELETE FROM personas WHERE id=?1", params![id]).unwrap();
        }
    }

    #[test]
    fn fixed_personas_dropped_accounts_unlinked_but_kept() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE personas (id TEXT PRIMARY KEY, ip_mode TEXT);
            CREATE TABLE accounts (id TEXT PRIMARY KEY, persona_id TEXT);
            INSERT INTO personas (id, ip_mode) VALUES ('gm1','airport'), ('fx1','fixed'), ('fx2','fixed');
            INSERT INTO accounts (id, persona_id) VALUES ('a1','gm1'), ('a2','fx1'), ('a3','fx2');
        ").unwrap();
        drop_fixed_sql(&c);
        // fixed 身份被删，airport 保留
        let persona_n: i64 = c.query_row("SELECT COUNT(*) FROM personas", [], |r| r.get(0)).unwrap();
        assert_eq!(persona_n, 1);
        // 账号全部保留
        let acct_n: i64 = c.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0)).unwrap();
        assert_eq!(acct_n, 3);
        // 原挂 fixed 的账号变未归属
        let unlinked: i64 = c.query_row("SELECT COUNT(*) FROM accounts WHERE persona_id IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(unlinked, 2);
        // 挂 airport 的账号关联不变
        let a1: Option<String> = c.query_row("SELECT persona_id FROM accounts WHERE id='a1'", [], |r| r.get(0)).unwrap();
        assert_eq!(a1, Some("gm1".to_string()));
    }
}
```

- [ ] **Step 4: 跑测试 + 编译**

Run: `cd src-tauri && cargo test --lib drop_fixed_personas && cargo check --lib 2>&1 | tail -3`
Expected: 测试 PASS；`Finished`

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/src/multi_account.rs
git commit -m "feat(migrate): 启动一次性删除固定IP身份(账号转未归属) + 单测"
```

---

## Task 7: 前端删除「固定IP」身份分类

**Files:**
- Modify: `src/tauri-frontend/app.ts` — `IdentityCategory`(:2574)、`ID_CATEGORIES`(:2576区)、tab/新建/选择器分支、i18n(:95-104,237-240,566-575)、引导(:1281,1292)
- Modify: `src-tauri/src/lib.rs:12479` — 摘 `persona_create_fixed` 注册

- [ ] **Step 1: 收窄 IdentityCategory 与 ID_CATEGORIES**

在 `src/tauri-frontend/app.ts:2574`：

```ts
type IdentityCategory = 'gmail' | '__none__';
```

`ID_CATEGORIES`（约 2576）删去 `fixed_cn`、`fixed_overseas` 两项，仅留：

```ts
const ID_CATEGORIES = [
  { key: 'gmail', labelKey: 'idcat.gmail', match: (p: any) => (p?.ip_mode || 'airport') === 'airport' },
];
```

- [ ] **Step 2: 删新建固定身份入口与类型选择器分支**

在 `src/tauri-frontend/app.ts` 约 2650 的分类操作按钮：删 `cat === 'fixed_cn' ? ...createFixedPersonaPrompt('cn')...` 与对应 overseas 分支，使新建按钮只剩 Gmail。
在约 2831 的「新建身份类型选择器」与 2865 的 `createFixedPersonaPrompt`：删除固定身份选项与该函数（含 2871 `region==='cn'?'fixed_cn':'fixed_overseas'` 归类、2945-2960 按 ip_policy 的 fixed 分支留到 Task 8 处理）。新建身份直接走 Gmail 流程。

- [ ] **Step 3: 清理 i18n 文案与引导**

删除/改写这些 i18n 键的值（`src/tauri-frontend/app.ts:95-104, 237-240, 566-575, 714-717` 区）：`idcat.fixedCn`、`idcat.fixedOverseas`、`accounts.newFixedCn`、`accounts.ipFixedCn`、`persona.newFixedCn`、`persona.newFixedCnDesc`、`idcat.emptyFixedCn`、`idcat.emptyFixedOverseas`。这些键若被删，确保无残留引用（搜索 `fixedCn`/`fixedOverseas`/`newFixedCn` 应为 0 命中）。
引导卡片 `GUIDE_SCENARIOS`（约 1281）「国内种草/生活」条 `descZh` 去掉「（需国内固定 IP）」；引导语（1292/1293）改为：「出口 IP：默认走 Gmail 身份的机场节点；个别账号需要专用 IP，在账号卡片上点「🧦 SOCKS5」配置即可。」

- [ ] **Step 4: 后端摘 persona_create_fixed 注册**

在 `src-tauri/src/lib.rs:12479` 删除 `multi_account::persona_create_fixed,` 这一行。函数体保留（死代码，加 `#[allow(dead_code)]` 于 `persona_create_fixed` 上方避免 warning 失败——本仓库 warning 不阻断，可选）。

- [ ] **Step 5: 构建 + 类型检查**

Run: `cd src-tauri && cargo check --lib 2>&1 | tail -3` → `Finished`
Run: `cd /Users/jinguichao/workspace/sec-zt/marketgo && npx tsc --noEmit 2>&1 | grep "app.ts" | head` → 无输出
Run: `npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js --format=iife --platform=browser` → Done

- [ ] **Step 6: 提交**

```bash
git add src/tauri-frontend/app.ts dist/tauri/scripts/app.js src-tauri/src/lib.rs
git commit -m "feat(identity): 删除国内/国外固定IP身份分类，统一为 Gmail 身份"
```

---

## Task 8: 前端放开「加账号」平台范围

**Files:**
- Modify: `src/tauri-frontend/app.ts:2957` 区（candidates 过滤）

- [ ] **Step 1: 去掉 ip_policy 分流**

在 `src/tauri-frontend/app.ts` 约 2957 的 candidates 过滤，改为不再按 `ip_policy`/`region` 分流，只按 `login_method` 区分一键开通 vs 手动加：

```ts
  const candidates = catalog.filter((c: any) => {
    if (c.provisioned) return false;
    // Gmail 身份：Google 登录平台走一键开通(personaProvisionAll)，其余手机/密码平台走手动加账号。
    // 此处「加账号」列出所有未开通、非 Google 登录的平台（不再按 IP 策略分流）。
    return c.login_method !== 'google';
  });
```

（若该函数同时服务「一键开通」选择器，保持其对 `login_method === 'google'` 的既有分支不动；本步只改「加账号」用的过滤。执行时按上下文确认两个用途的过滤是否同一处，必要时各自调整。）

- [ ] **Step 2: 构建 + 类型检查**

Run: `npx tsc --noEmit 2>&1 | grep "app.ts" | head` → 无输出
Run: `npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js --format=iife --platform=browser` → Done

- [ ] **Step 3: 提交**

```bash
git add src/tauri-frontend/app.ts dist/tauri/scripts/app.js
git commit -m "feat(identity): Gmail 身份加账号放开到所有平台"
```

---

## Task 9: 账号卡片「🧦 SOCKS5」按钮 + 单账号弹框 + 展示当前代理

**Files:**
- Modify: `src/tauri-frontend/app.ts` — 账号卡片 `.account-actions`(:3097区)、persona 行展示(:3088区)、新增 `openAccountProxyModal`

- [ ] **Step 1: 卡片展示当前代理 + 加按钮**

在 `src/tauri-frontend/app.ts` 账号卡片 persona 行（约 3088 `<div style="display:flex;...">${personaBadge}` 内），在 `${nurtureStats}` 之前插入当前代理 chip：

```ts
          ${account.custom_proxy ? `<span class="chip-btn" style="background:#fef3c7;color:#92400e;" title="该账号走自定义代理">🧦 ${escapeHtml(String(account.custom_proxy).replace(/^socks5:\/\//,'').replace(/^https?:\/\//,''))}</span>` : ''}
```

在 `.account-actions`（约 3097）内、`🌱 养号` 按钮之后插入：

```ts
          <button class="btn btn-small btn-secondary" onclick="openAccountProxyModal('${account.id}')" title="配置该账号的自定义 SOCKS5 代理（不配走身份机场节点）">🧦 SOCKS5</button>
```

- [ ] **Step 2: 新增单账号代理弹框**

在 `src/tauri-frontend/app.ts` 合适位置（如 `autoLoginAccount` 附近）新增：

```ts
(window as any).openAccountProxyModal = function(accountId: string) {
  const acc = accounts.find((a: any) => a.id === accountId);
  const cur = (acc?.custom_proxy as string) || '';
  const overlay = document.createElement('div');
  overlay.className = 'modal active';
  overlay.innerHTML = `
    <div class="modal-content" style="max-width:460px;">
      <div class="modal-header"><h3>🧦 配置 SOCKS5 代理</h3><button class="modal-close" data-cancel>&times;</button></div>
      <div class="modal-body">
        <p style="font-size:13px;color:var(--text-muted);margin-bottom:8px;">留空=清除，走身份统一的机场节点。格式 <code>socks5://user:pass@host:port</code> 或 <code>host:port</code>。</p>
        <input id="acctProxyInput" type="text" class="form-input" style="width:100%;" placeholder="host:port 或 socks5://..." value="${escapeHtml(cur)}">
        <p id="acctProxyTestResult" style="font-size:12px;margin-top:8px;color:var(--text-muted);"></p>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary" id="acctProxyTest">测试出口IP</button>
        <button class="btn btn-secondary" data-cancel>取消</button>
        <button class="btn btn-primary" id="acctProxySave">保存</button>
      </div>
    </div>`;
  const close = () => overlay.remove();
  overlay.querySelectorAll('[data-cancel]').forEach(el => el.addEventListener('click', close));
  overlay.addEventListener('click', (e) => { if (e.target === overlay) close(); });
  overlay.querySelector('#acctProxySave')?.addEventListener('click', async () => {
    const val = (overlay.querySelector('#acctProxyInput') as HTMLInputElement).value.trim();
    try {
      await invoke('set_account_proxy', { accountId, proxy: val || null });
      showToast(val ? '已保存自定义代理' : '已清除，走身份机场节点', 'success');
      close();
      await loadAccounts();
    } catch (e) { showToast('保存失败：' + e, 'error'); }
  });
  overlay.querySelector('#acctProxyTest')?.addEventListener('click', async () => {
    const r = overlay.querySelector('#acctProxyTestResult') as HTMLElement;
    const val = (overlay.querySelector('#acctProxyInput') as HTMLInputElement).value.trim();
    r.textContent = '测试中…（先保存当前输入再测）';
    try {
      await invoke('set_account_proxy', { accountId, proxy: val || null });
      const out = await invoke<string>('test_account_proxy', { accountId });
      r.textContent = out;
    } catch (e) { r.textContent = '测试失败：' + e; }
  });
  document.body.appendChild(overlay);
};
```

- [ ] **Step 3: 构建 + 类型检查**

Run: `npx tsc --noEmit 2>&1 | grep "app.ts" | head` → 无输出
Run: `npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js --format=iife --platform=browser` → Done

- [ ] **Step 4: 提交**

```bash
git add src/tauri-frontend/app.ts dist/tauri/scripts/app.js
git commit -m "feat(proxy): 账号卡片 SOCKS5 按钮 + 配置弹框 + 当前代理展示"
```

---

## Task 10: 批量配置 SOCKS5

**Files:**
- Modify: `dist/tauri/index.html`（批量弹框 DOM，仿批量养号弹框）
- Modify: `src/tauri-frontend/app.ts`（打开弹框、渲染账号 checkbox、应用/清除）

- [ ] **Step 1: 加批量弹框 DOM**

在 `dist/tauri/index.html` 的「Batch Nurture Task Modal」(约 2037) 之后，加：

```html
  <!-- Batch SOCKS5 Modal -->
  <div class="modal" id="modalBatchProxy">
    <div class="modal-content" style="max-width: 600px;">
      <div class="modal-header"><h3>🧦 批量配置 SOCKS5 / Batch proxy</h3><button class="modal-close" data-close>&times;</button></div>
      <div class="modal-body">
        <p style="font-size:13px;color:var(--text-muted);">勾选账号 → 填代理「应用到选中」，或「清除选中」走机场节点。</p>
        <input id="batchProxyInput" type="text" class="form-input" style="width:100%;margin:8px 0;" placeholder="host:port 或 socks5://...">
        <div id="batchProxyAccounts" style="max-height:320px;overflow:auto;"></div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary" data-close>取消</button>
        <button class="btn btn-danger" id="batchProxyClear">清除选中</button>
        <button class="btn btn-primary" id="batchProxyApply">应用到选中</button>
      </div>
    </div>
  </div>
```

- [ ] **Step 2: 加打开入口（账号页工具栏）**

在 `src/tauri-frontend/app.ts` 账号页头部工具栏（与「一键养号」/「批量养号」按钮同处，搜索 `startBatchNurtureTask` 的入口按钮附近）加一个按钮：

```ts
        <button class="btn btn-secondary" onclick="openBatchProxyModal()">🧦 批量 SOCKS5</button>
```

- [ ] **Step 3: 实现 openBatchProxyModal + 应用/清除**

在 `src/tauri-frontend/app.ts` 新增：

```ts
(window as any).openBatchProxyModal = function() {
  const box = document.getElementById('batchProxyAccounts');
  if (box) {
    box.innerHTML = accounts.map((a: any) => {
      const name = a.username || a.email || a.platform || a.id;
      const cur = a.custom_proxy ? `（当前 ${escapeHtml(String(a.custom_proxy).replace(/^socks5:\/\//,''))}）` : '';
      return `<label style="display:flex;align-items:center;gap:8px;padding:4px 0;">
        <input type="checkbox" name="batchProxyAcct" value="${a.id}">
        <span>${escapeHtml(name)} · ${escapeHtml(a.platform)} ${cur}</span></label>`;
    }).join('');
  }
  openModal('modalBatchProxy');
};

async function applyBatchProxy(clear: boolean) {
  const ids = Array.from(document.querySelectorAll('input[name="batchProxyAcct"]:checked'))
    .map((cb) => (cb as HTMLInputElement).value);
  if (!ids.length) { showToast('请至少勾选一个账号', 'warning'); return; }
  const val = clear ? null : ((document.getElementById('batchProxyInput') as HTMLInputElement)?.value.trim() || null);
  try {
    const n = await invoke<number>('set_accounts_proxy', { accountIds: ids, proxy: val });
    showToast(`已${clear ? '清除' : '设置'} ${n} 个账号的代理`, 'success');
    closeModal('modalBatchProxy');
    await loadAccounts();
  } catch (e) { showToast('批量设置失败：' + e, 'error'); }
}
```

在事件绑定初始化处（搜索 `getElementById('nurtureAllConcurrency')` 附近的事件注册区）加：

```ts
  document.getElementById('batchProxyApply')?.addEventListener('click', () => applyBatchProxy(false));
  document.getElementById('batchProxyClear')?.addEventListener('click', () => applyBatchProxy(true));
```

（`openModal`/`closeModal` 为本仓库现有弹框开关函数，按现有用法调用。）

- [ ] **Step 4: 构建 + 类型检查**

Run: `npx tsc --noEmit 2>&1 | grep "app.ts" | head` → 无输出
Run: `npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js --format=iife --platform=browser` → Done

- [ ] **Step 5: 提交**

```bash
git add src/tauri-frontend/app.ts dist/tauri/index.html dist/tauri/scripts/app.js
git commit -m "feat(proxy): 批量配置/清除账号 SOCKS5"
```

---

## Task 11: 端到端验证（手动）

**Files:** 无（运行验证）

- [ ] **Step 1: 全量测试**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -6`
Expected: 新增测试全 PASS（既有 `platform_meta_tests` 2 个失败为本仓历史问题，与本次无关）。

- [ ] **Step 2: 启动**

Run: `pkill -f "tauri dev"; pkill -f "serve dist/tauri"; sleep 1; npm run tauri:dev`（后台）
等待日志出现 `Finished` + `GET /scripts/app.js Returned 200`。

- [ ] **Step 3: 人工核对**

- 身份页只剩「📧 Gmail 身份」+「🧩 未归属」两个 tab；原固定身份账号变未归属。
- 账号卡片有「🧦 SOCKS5」按钮；给一个账号配 `host:port` → 卡片出现 `🧦 host:port` chip；点弹框「测试出口IP」返回代理 IP；清除后再测为机场 IP。
- 一键养号/养号时该账号走其自定义代理（日志 `[PROXY] 账号 x 出口 → ...`）。
- 账号页「🧦 批量 SOCKS5」可多选应用/清除。

- [ ] **Step 4: 收尾提交（如有微调）**

```bash
git add -A && git commit -m "test: 身份统一+账号SOCKS5 端到端验证微调" && git push
```

---

## 自检对照（spec 覆盖）

- A 删固定分类 → Task 7 ✅
- B 启动删 fixed 身份 → Task 6 ✅
- C 加账号放开 → Task 8 ✅
- D 账号级 SOCKS5（列/命令/卡片按钮/批量/展示）→ Task 2,3,9,10 ✅
- E 动态生效 + 三处接入 → Task 4,5 ✅
- F 测试 → Task 1,6 单测 + Task 11 手动 ✅
