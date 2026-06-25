# 养号主题统一系统（平台隔离）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 GitHub/X/SegmentFault 各自的"方向选择"统一成一个平台隔离的主题系统，新增"用户自定义添加主题"能力，并让小红书作为新平台接入（runner 后续做）。

**Architecture:** 保留 `GH_DOMAINS/X_NICHES/SF_DOMAINS/XHS_TOPICS` 四常量当内置数据源（key 不变 → 旧数据无缝迁移），其上加统一层：`builtin_topics(platform)` + 自定义表 `custom_topics(key,platform,label,keywords)` + 账号通用列 `accounts.nurture_topics`。catalog/命令/前端全部按 platform 通用。三个 runner 的"读方向→收关键词"统一走 `account_topic_keywords`。

**Tech Stack:** Rust + Tauri v2 + rusqlite；TypeScript（esbuild）。

参考设计：`docs/superpowers/specs/2026-06-25-nurture-topics-unified-design.md`
现有同构参考：`GH_DOMAINS`(lib.rs:936) `X_NICHES`(972) `SF_DOMAINS`(1013)、三个 runner(nurture.rs:25/266/323)、`pickXNiches`(app.ts:1603)。

---

## File Structure

- `src-tauri/src/lib.rs`（修改）
  - 迁移块（~3656）：加 `nurture_topics` 列 + 建 `custom_topics` 表 + 一次性数据迁移
  - 主题统一层（紧接 `sf_domain_keywords` 之后，~1039）：`XHS_TOPICS` 常量、`TopicDef`/`TopicItem`、conn-level helpers、6 命令
  - `invoke_handler`（~12356）：注册 6 新命令、移除旧命令
  - 旧代码删除：旧 catalog/get/set 命令、旧 helper、旧测试
  - 测试模块（文件末尾）：新增 `mod topics_tests`
- `src-tauri/src/nurture.rs`（修改）：3 个 runner 的读取/关键词改造（25-50 / 266-286 / 323-345）
- `src/tauri-frontend/app.ts`（修改）：`pickTopics` 合并三函数、卡片入口、chip 改 label、删旧 label map

---

### Task 1: DB 迁移（列 + 表 + 数据迁移）

**Files:** Modify `src-tauri/src/lib.rs:3656`

- [ ] **Step 1: 加迁移语句**

在 lib.rs:3656 `sf_domains` 迁移行后追加：

```rust
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN sf_domains TEXT", []);
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN nurture_topics TEXT", []);
    let _ = conn.execute("CREATE TABLE IF NOT EXISTS custom_topics (key TEXT PRIMARY KEY, platform TEXT NOT NULL, label TEXT NOT NULL, keywords TEXT)", []);
    // 一次性把旧三列方向迁入统一列（key 不变，直接搬 JSON）
    let _ = conn.execute("UPDATE accounts SET nurture_topics = gh_domains WHERE platform='github' AND nurture_topics IS NULL AND gh_domains IS NOT NULL", []);
    let _ = conn.execute("UPDATE accounts SET nurture_topics = x_niches WHERE platform IN ('twitter','x') AND nurture_topics IS NULL AND x_niches IS NOT NULL", []);
    let _ = conn.execute("UPDATE accounts SET nurture_topics = sf_domains WHERE platform='segmentfault' AND nurture_topics IS NULL AND sf_domains IS NOT NULL", []);
```

（首行已存在，用于定位。）

- [ ] **Step 2: 编译确认**

Run: `cd src-tauri && cargo build 2>&1 | tail -3`
Expected: `Finished` 无 error。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 统一主题 DB 迁移(nurture_topics 列+custom_topics 表+旧数据迁入)"
```

---

### Task 2: 内置层 + 类型 + 读取 + catalog（TDD）

**Files:** Modify `src-tauri/src/lib.rs`（紧接 `sf_domain_keywords` 结束，~1039 行 `}` 之后）；Test：文件末尾新增 `mod topics_tests`

- [ ] **Step 1: 写失败测试**

在文件末尾追加：

```rust
#[cfg(test)]
mod topics_tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE accounts (id TEXT PRIMARY KEY, platform TEXT, nurture_topics TEXT);
            CREATE TABLE custom_topics (key TEXT PRIMARY KEY, platform TEXT NOT NULL, label TEXT NOT NULL, keywords TEXT);
        ").unwrap();
        c
    }

    #[test]
    fn builtin_maps_each_platform() {
        assert!(builtin_topics("github").iter().any(|t| t.key == "frontend" && !t.keywords.is_empty()));
        assert!(builtin_topics("twitter").iter().any(|t| t.key == "technology"));
        assert!(builtin_topics("segmentfault").iter().any(|t| t.key == "frontend"));
        let beauty = builtin_topics("xiaohongshu").into_iter().find(|t| t.key == "beauty").unwrap();
        assert_eq!(beauty.label, "美妆护肤");
        assert_eq!(beauty.keywords, vec!["美妆护肤".to_string()]);
        assert!(builtin_topics("unknown").is_empty());
    }

    #[test]
    fn catalog_platform_isolated_builtin_first() {
        let c = setup();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u1','xiaohongshu','露营装备',NULL)", []).unwrap();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u2','github','我的库',NULL)", []).unwrap();
        let xhs = topics_catalog_from(&c, "xiaohongshu");
        assert!(xhs[0].builtin);
        assert!(xhs.iter().any(|i| i.key == "u1" && !i.builtin));
        assert!(!xhs.iter().any(|i| i.key == "u2")); // 跨平台隔离
    }

    #[test]
    fn read_account_topics_roundtrip() {
        let c = setup();
        c.execute("INSERT INTO accounts (id,platform,nurture_topics) VALUES ('a1','xiaohongshu','[\"beauty\",\"food\"]')", []).unwrap();
        assert_eq!(account_topics(&c, "a1"), vec!["beauty".to_string(), "food".to_string()]);
        assert!(account_topics(&c, "nope").is_empty());
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test topics_tests 2>&1 | tail -15`
Expected: 编译失败 —— 找不到 `builtin_topics`/`topics_catalog_from`/`account_topics`。

- [ ] **Step 3: 实现内置层**

在 lib.rs `sf_domain_keywords` 函数结束（~1039 行）之后插入：

```rust
/// 小红书养号主题（内置赛道，代码事实源）。keywords 默认即主题名（主题名当搜索词）。
#[derive(Clone, Copy)]
struct XhsTopic { key: &'static str, label: &'static str }
const XHS_TOPICS: &[XhsTopic] = &[
    XhsTopic { key: "beauty",    label: "美妆护肤" },
    XhsTopic { key: "fashion",   label: "穿搭时尚" },
    XhsTopic { key: "food",      label: "美食探店" },
    XhsTopic { key: "travel",    label: "旅行出行" },
    XhsTopic { key: "home",      label: "家居家装" },
    XhsTopic { key: "parenting", label: "母婴育儿" },
    XhsTopic { key: "fitness",   label: "健身运动" },
    XhsTopic { key: "digital",   label: "数码科技" },
    XhsTopic { key: "career",    label: "职场成长" },
    XhsTopic { key: "emotion",   label: "情感生活" },
    XhsTopic { key: "pet",       label: "萌宠" },
    XhsTopic { key: "wellness",  label: "养生健康" },
];

pub struct TopicDef { pub key: String, pub label: String, pub keywords: Vec<String> }

#[derive(serde::Serialize, Clone)]
pub struct TopicItem { pub key: String, pub label: String, pub keywords: Vec<String>, pub builtin: bool }

/// 内置主题按平台映射（四常量归一为 key/label/keywords）。未知平台 → 空。
fn builtin_topics(platform: &str) -> Vec<TopicDef> {
    match platform.to_lowercase().as_str() {
        "github" => GH_DOMAINS.iter().map(|d| TopicDef {
            key: d.key.to_string(), label: d.label.to_string(),
            keywords: d.topics.iter().map(|s| s.to_string()).collect(),
        }).collect(),
        "twitter" | "x" => X_NICHES.iter().map(|n| TopicDef {
            key: n.key.to_string(), label: n.label.to_string(),
            keywords: n.keywords.iter().map(|s| s.to_string()).collect(),
        }).collect(),
        "segmentfault" => SF_DOMAINS.iter().map(|d| TopicDef {
            key: d.key.to_string(), label: d.label.to_string(),
            keywords: d.keywords.iter().map(|s| s.to_string()).collect(),
        }).collect(),
        "xiaohongshu" | "redbook" => XHS_TOPICS.iter().map(|t| TopicDef {
            key: t.key.to_string(), label: t.label.to_string(),
            keywords: vec![t.label.to_string()],
        }).collect(),
        _ => Vec::new(),
    }
}

/// 账号 platform（不存在 → None）。
fn account_platform(conn: &Connection, account_id: &str) -> Option<String> {
    conn.query_row("SELECT platform FROM accounts WHERE id=?1", params![account_id], |r| r.get(0)).ok()
}

/// 读账号所选主题 keys（nurture_topics JSON 数组），失败/空 → 空 vec。
fn account_topics(conn: &Connection, account_id: &str) -> Vec<String> {
    let raw: Option<String> = conn.query_row(
        "SELECT nurture_topics FROM accounts WHERE id=?1",
        params![account_id], |r| r.get(0)).ok().flatten();
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default()
}

/// catalog = 内置(platform) ∪ 自定义表 WHERE platform（内置在前）。
fn topics_catalog_from(conn: &Connection, platform: &str) -> Vec<TopicItem> {
    let mut out: Vec<TopicItem> = builtin_topics(platform).into_iter().map(|d| TopicItem {
        key: d.key, label: d.label, keywords: d.keywords, builtin: true,
    }).collect();
    if let Ok(mut stmt) = conn.prepare("SELECT key, label, keywords FROM custom_topics WHERE platform=?1 ORDER BY rowid") {
        if let Ok(rows) = stmt.query_map(params![platform], |r| {
            let kw_raw: Option<String> = r.get(2)?;
            let keywords = kw_raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default();
            Ok(TopicItem { key: r.get(0)?, label: r.get(1)?, keywords, builtin: false })
        }) {
            for it in rows.flatten() { out.push(it); }
        }
    }
    out
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test topics_tests 2>&1 | tail -8`
Expected: `test result: ok. 3 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 统一主题内置层+类型+catalog(平台隔离)"
```

---

### Task 3: 保存账号已选主题 helper（TDD）

**Files:** Modify `src-tauri/src/lib.rs`（接 Task 2 helper 之后）；Test：`mod topics_tests`

- [ ] **Step 1: 写失败测试**

在 `mod topics_tests` 内追加：

```rust
    #[test]
    fn set_filters_unknown_and_roundtrips() {
        let c = setup();
        c.execute("INSERT INTO accounts (id,platform) VALUES ('a1','xiaohongshu')", []).unwrap();
        set_account_topics_conn(&c, "a1", &["beauty".to_string(), "ghost".to_string()]).unwrap();
        assert_eq!(account_topics(&c, "a1"), vec!["beauty".to_string()]);
    }

    #[test]
    fn set_keeps_custom_of_same_platform() {
        let c = setup();
        c.execute("INSERT INTO accounts (id,platform) VALUES ('a1','xiaohongshu')", []).unwrap();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u1','xiaohongshu','露营装备',NULL)", []).unwrap();
        set_account_topics_conn(&c, "a1", &["u1".to_string()]).unwrap();
        assert_eq!(account_topics(&c, "a1"), vec!["u1".to_string()]);
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test topics_tests::set 2>&1 | tail -12`
Expected: 编译失败 —— 找不到 `set_account_topics_conn`。

- [ ] **Step 3: 实现 set helper**

紧接 `topics_catalog_from` 之后插入：

```rust
/// 保存账号所选主题，按账号 platform 的 catalog 过滤非法 key。
fn set_account_topics_conn(conn: &Connection, account_id: &str, keys: &[String]) -> Result<(), String> {
    let platform = account_platform(conn, account_id).ok_or_else(|| "账号不存在".to_string())?;
    let valid: std::collections::HashSet<String> =
        topics_catalog_from(conn, &platform).into_iter().map(|i| i.key).collect();
    let kept: Vec<String> = keys.iter().filter(|k| valid.contains(*k)).cloned().collect();
    let json = serde_json::to_string(&kept).map_err(|e| e.to_string())?;
    conn.execute("UPDATE accounts SET nurture_topics=?1 WHERE id=?2", params![json, account_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test topics_tests 2>&1 | tail -8`
Expected: `test result: ok. 5 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 统一主题账号已选保存 helper(按平台过滤)"
```

---

### Task 4: 添加自定义主题 helper（TDD）

**Files:** Modify `src-tauri/src/lib.rs`（接 Task 3 之后）；Test：`mod topics_tests`

- [ ] **Step 1: 写失败测试**

在 `mod topics_tests` 内追加：

```rust
    #[test]
    fn add_trims_and_rejects_dup_per_platform() {
        let c = setup();
        let it = add_custom_topic_conn(&c, "xiaohongshu", "  露营装备  ").unwrap();
        assert_eq!(it.label, "露营装备");
        assert!(!it.builtin);
        assert!(add_custom_topic_conn(&c, "xiaohongshu", "   ").is_err());      // 空
        assert!(add_custom_topic_conn(&c, "xiaohongshu", "美妆护肤").is_err()); // 内置重名
        assert!(add_custom_topic_conn(&c, "xiaohongshu", "露营装备").is_err()); // 自定义重名
        add_custom_topic_conn(&c, "github", "露营装备").unwrap();              // 不同平台同名 OK
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test topics_tests::add 2>&1 | tail -12`
Expected: 编译失败 —— 找不到 `add_custom_topic_conn`。

- [ ] **Step 3: 实现 add helper**

紧接 `set_account_topics_conn` 之后插入：

```rust
/// 加自定义主题：label trim 非空 + 与该 platform 现有(内置+自定义)label 不重名；uuid key。
fn add_custom_topic_conn(conn: &Connection, platform: &str, label: &str) -> Result<TopicItem, String> {
    let label = label.trim();
    if label.is_empty() { return Err("主题名不能为空".to_string()); }
    if topics_catalog_from(conn, platform).iter().any(|i| i.label == label) {
        return Err("主题已存在".to_string());
    }
    let key = Uuid::new_v4().to_string();
    conn.execute("INSERT INTO custom_topics (key, platform, label, keywords) VALUES (?1,?2,?3,NULL)",
        params![key, platform, label]).map_err(|e| e.to_string())?;
    Ok(TopicItem { key, label: label.to_string(), keywords: Vec::new(), builtin: false })
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test topics_tests 2>&1 | tail -8`
Expected: `test result: ok. 6 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 添加自定义主题 helper(去空格+同平台重名校验)"
```

---

### Task 5: 删除自定义主题 helper（TDD）

**Files:** Modify `src-tauri/src/lib.rs`（接 Task 4 之后）；Test：`mod topics_tests`

- [ ] **Step 1: 写失败测试**

在 `mod topics_tests` 内追加：

```rust
    #[test]
    fn delete_custom_strips_and_rejects_builtin() {
        let c = setup();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u1','xiaohongshu','露营装备',NULL)", []).unwrap();
        c.execute("INSERT INTO accounts (id,platform,nurture_topics) VALUES ('a1','xiaohongshu','[\"beauty\",\"u1\"]')", []).unwrap();
        delete_custom_topic_conn(&c, "u1").unwrap();
        assert!(!topics_catalog_from(&c, "xiaohongshu").iter().any(|i| i.key == "u1"));
        assert_eq!(account_topics(&c, "a1"), vec!["beauty".to_string()]);
        assert!(delete_custom_topic_conn(&c, "beauty").is_err()); // 内置不可删
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test topics_tests::delete 2>&1 | tail -12`
Expected: 编译失败 —— 找不到 `delete_custom_topic_conn`。

- [ ] **Step 3: 实现 delete helper**

紧接 `add_custom_topic_conn` 之后插入（用 DELETE 行数判定内置/不存在）：

```rust
/// 删自定义主题（内置/不存在 → 拒绝），并从所有账号 nurture_topics 移除该 key。
fn delete_custom_topic_conn(conn: &Connection, key: &str) -> Result<(), String> {
    let n = conn.execute("DELETE FROM custom_topics WHERE key=?1", params![key])
        .map_err(|e| e.to_string())?;
    if n == 0 { return Err("内置主题不可删除".to_string()); }
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT id, nurture_topics FROM accounts WHERE nurture_topics IS NOT NULL")
            .map_err(|e| e.to_string())?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?.flatten().collect()
    };
    for (id, raw) in rows {
        if let Ok(keys) = serde_json::from_str::<Vec<String>>(&raw) {
            if keys.iter().any(|k| k == key) {
                let kept: Vec<String> = keys.into_iter().filter(|k| k != key).collect();
                let json = serde_json::to_string(&kept).map_err(|e| e.to_string())?;
                conn.execute("UPDATE accounts SET nurture_topics=?1 WHERE id=?2", params![json, id])
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test topics_tests 2>&1 | tail -8`
Expected: `test result: ok. 7 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 删除自定义主题 helper(连带清理账号已选)"
```

---

### Task 6: runner 关键词收集 helper（TDD）

**Files:** Modify `src-tauri/src/lib.rs`（接 Task 5 之后）；Test：`mod topics_tests`

- [ ] **Step 1: 写失败测试**

在 `mod topics_tests` 内追加：

```rust
    #[test]
    fn topic_keywords_builtin_and_custom_fallback() {
        let c = setup();
        c.execute("INSERT INTO custom_topics (key,platform,label,keywords) VALUES ('u1','xiaohongshu','露营装备',NULL)", []).unwrap();
        c.execute("INSERT INTO accounts (id,platform,nurture_topics) VALUES ('a1','xiaohongshu','[\"beauty\",\"u1\"]')", []).unwrap();
        let kws = account_topic_keywords(&c, "a1");
        assert!(kws.contains(&"美妆护肤".to_string())); // 内置 xhs：keywords=label
        assert!(kws.contains(&"露营装备".to_string())); // 自定义无 keywords → 回退 label
        c.execute("INSERT INTO accounts (id,platform,nurture_topics) VALUES ('g1','github','[\"frontend\"]')", []).unwrap();
        assert!(account_topic_keywords(&c, "g1").contains(&"react".to_string())); // github：keywords 来自 topics
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test topics_tests::topic_keywords 2>&1 | tail -12`
Expected: 编译失败 —— 找不到 `account_topic_keywords`。

- [ ] **Step 3: 实现 keyword 收集**

紧接 `delete_custom_topic_conn` 之后插入：

```rust
/// 按账号 platform + 所选 keys，从 catalog 收集 keywords 去重；自定义无 keywords 时回退 label。供 runner 用。
fn account_topic_keywords(conn: &Connection, account_id: &str) -> Vec<String> {
    let platform = match account_platform(conn, account_id) { Some(p) => p, None => return Vec::new() };
    let keys = account_topics(conn, account_id);
    let catalog = topics_catalog_from(conn, &platform);
    let mut out: Vec<String> = Vec::new();
    for k in &keys {
        if let Some(item) = catalog.iter().find(|i| &i.key == k) {
            if item.keywords.is_empty() {
                if !out.contains(&item.label) { out.push(item.label.clone()); }
            } else {
                for w in &item.keywords {
                    if !out.contains(w) { out.push(w.clone()); }
                }
            }
        }
    }
    out
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test topics_tests 2>&1 | tail -8`
Expected: `test result: ok. 8 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): runner 关键词收集 helper(内置+自定义回退 label)"
```

---

### Task 7: 6 个命令 + 注册

**Files:** Modify `src-tauri/src/lib.rs`（命令插在 Task 6 之后；注册在 ~12356）

- [ ] **Step 1: 写 6 个命令**

紧接 `account_topic_keywords` 之后插入：

```rust
#[tauri::command]
fn topics_catalog(state: State<AppState>, platform: String) -> Result<Vec<TopicItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(topics_catalog_from(&conn, &platform))
}

#[tauri::command]
fn get_account_topics(state: State<AppState>, account_id: String) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(account_topics(&conn, &account_id))
}

#[tauri::command]
fn set_account_topics(state: State<AppState>, account_id: String, keys: Vec<String>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    set_account_topics_conn(&conn, &account_id, &keys)
}

#[tauri::command]
fn add_custom_topic(state: State<AppState>, platform: String, label: String) -> Result<TopicItem, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    add_custom_topic_conn(&conn, &platform, &label)
}

#[tauri::command]
fn delete_custom_topic(state: State<AppState>, key: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    delete_custom_topic_conn(&conn, &key)
}

/// account_id → 已选主题 label 列表（后端按 platform 解析 key→label），供卡片 chip 直接显示。
#[tauri::command]
fn account_topic_labels(state: State<AppState>) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let accts: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT id, platform FROM accounts WHERE nurture_topics IS NOT NULL")
            .map_err(|e| e.to_string())?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?.flatten().collect()
    };
    let mut out = std::collections::HashMap::new();
    for (id, platform) in accts {
        let keys = account_topics(&conn, &id);
        if keys.is_empty() { continue; }
        let catalog = topics_catalog_from(&conn, &platform);
        let labels: Vec<String> = keys.iter()
            .filter_map(|k| catalog.iter().find(|i| &i.key == k).map(|i| i.label.clone()))
            .collect();
        if !labels.is_empty() { out.insert(id, labels); }
    }
    Ok(out)
}
```

- [ ] **Step 2: 注册命令**

在 lib.rs:12356 的 `x_niches_catalog,` 行附近、`account_niches,` 之前插入 6 行：

```rust
            topics_catalog,
            get_account_topics,
            set_account_topics,
            add_custom_topic,
            delete_custom_topic,
            account_topic_labels,
```

- [ ] **Step 3: 编译 + 测试**

Run: `cd src-tauri && cargo build 2>&1 | tail -3 && cargo test topics_tests 2>&1 | tail -5`
Expected: `Finished` 无 error；`test result: ok. 8 passed`。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 统一主题 6 命令 + 注册"
```

---

### Task 8: 三个 runner 改造（读方向→收关键词统一走新 helper）

**Files:** Modify `src-tauri/src/nurture.rs`

- [ ] **Step 1: GitHub runner（nurture.rs:28-50）**

把第一块（28-39）+ 选 topic（45-50）改为：在首块同时取 keys 与 keywords，下游 `topic` 改取引用。

将 28-39 行替换为：

```rust
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
```

将 45-50 行（`// 2) ...` 到 `let topic = topics[...];`）替换为：

```rust
    // 2) 选 topic（关键词已在首块按所选领域收集；时间派生种子）
    if topics.is_empty() { return Ok("领域无可用 topic".to_string()); }
    let seed = get_random_delay(1, 100_000);
    let topic = &topics[(seed as usize) % topics.len()];
```

（`if domains.is_empty()` 跳过检查、`gh_daily_quota` 行保持不变。`topic` 现为 `&String`，下游 `topic.to_string()`/`format!` 均兼容。）

- [ ] **Step 2: SegmentFault runner（nurture.rs:269-286）**

将 269-280 行（首块）替换为：

```rust
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
```

删除原 284-285 行：

```rust
    let keys: Vec<&str> = domains.iter().map(|s| s.as_str()).collect();
    let kws: Vec<String> = sf_domain_keywords(&keys).iter().map(|s| s.to_string()).collect();
```

（`if domains.is_empty()` 跳过检查、`if kws.is_empty()` 检查、`sf_nurture_browse_blocking(kws, ...)` 调用保持不变；`kws` 现由首块提供，仍是 `Vec<String>`。）

- [ ] **Step 3: X runner（nurture.rs:325-347）**

将 325-336 行（首块）替换为：

```rust
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
```

将 342-347 行（`// 2) 选方向 → 关键词` 到 `let kw = kws[...];`）替换为：

```rust
    // 2) 选方向 → 关键词（已在首块收集）
    if kws.is_empty() { return Ok("方向无可用关键词".to_string()); }
    let seed = get_random_delay(1, 100_000);
    let kw = &kws[(seed as usize) % kws.len()];
```

（`if niches.is_empty()` 跳过检查、`x_daily_quota` 行保持不变。`kw` 现为 `&String`：`urlencoding::encode(kw)`、`format!`、`kws[..].to_string()`（447 行）均兼容。原 `let keys: Vec<&str> = niches...` 行随旧 343-344 一并删除。）

- [ ] **Step 4: 编译确认（热重载或手动 build）**

Run: `cd src-tauri && cargo build 2>&1 | tail -5`
Expected: `Finished` 无 error。若报 `kw`/`topic` 类型相关 error，按提示把对应下游用法改成对 `&String`/`String` 兼容（不改变逻辑）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/nurture.rs
git commit -m "refactor(nurture): 三 runner 读方向/关键词统一走 account_topic_keywords"
```

---

### Task 9: 删除旧命令 / helper / 测试 + 清理注册

**Files:** Modify `src-tauri/src/lib.rs`

- [ ] **Step 1: 删旧命令注册**

在 `invoke_handler` 列表中删除这些行（逐个 grep 定位）：`gh_domains_catalog,`、`get_account_gh_domains,`、`set_account_gh_domains,`、`x_niches_catalog,`、`get_account_x_niches,`、`set_account_x_niches,`、`sf_domains_catalog,`、`get_account_sf_domains,`、`set_account_sf_domains,`、`account_niches,`。

Run（定位）: `grep -nE "gh_domains_catalog,|account_x_niches,|sf_domains_catalog,|account_niches," src-tauri/src/lib.rs`

- [ ] **Step 2: 删旧命令与 helper 定义**

删除这些 `#[tauri::command] fn`/`fn` 定义（连同其上 doc 注释）：
- `gh_domains_catalog`、`get_account_gh_domains`、`set_account_gh_domains`（lib.rs ~1223-1250）
- `x_niches_catalog`、`get_account_x_niches`、`set_account_x_niches`（~1256-1283）
- `sf_domains_catalog`、`get_account_sf_domains`、`set_account_sf_domains`（~1289-1316）
- `account_niches`（~1329-1352）
- helper `account_gh_domains`(~1175)、`account_x_niches`(~1204)、`account_sf_domains`(~1212)
- helper `gh_domain_topics`(~953)、`x_niche_keywords`(~992)、`sf_domain_keywords`(~1029)

（保留 `GhDomain`/`XNiche`/`SfDomain` 结构 + `GH_DOMAINS`/`X_NICHES`/`SF_DOMAINS` 常量 —— 仍是 `builtin_topics` 的数据源。）

- [ ] **Step 3: 删旧测试**

删除这些测试函数：`read_account_domains`（~12710）、`read_account_niches`（~12754）、`gh_domain_topics_collects_and_dedups`（~12521）、`x_niche_keywords_collects_and_dedups`（~12593）。若删后某 `#[cfg(test)] mod` 变空，连空 mod 一并删。

- [ ] **Step 4: 编译 + 全量测试**

Run: `cd src-tauri && cargo build 2>&1 | tail -5 && cargo test 2>&1 | tail -15`
Expected: `Finished` 无 error（可有 unused 警告）；所有测试 `ok`，含 `topics_tests` 8 passed。若报某旧符号仍被引用，按编译提示删除残余引用。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "refactor(nurture): 删除旧 gh/x/sf 方向命令+helper+测试(统一到主题系统)"
```

---

### Task 10: 前端 —— pickTopics 合并 + 卡片入口 + chip

**Files:** Modify `src/tauri-frontend/app.ts`

- [ ] **Step 1: 新增 pickTopics（替代三个 pick 函数）**

在 `pickXNiches` 函数（app.ts:1603）之前插入统一函数：

```ts
interface TopicItem { key: string; label: string; keywords: string[]; builtin: boolean }

// 统一养号主题多选（按账号+平台）：内置 + 用户自定义；可现场添加/删除自定义主题。复用 .modal.active。
(window as any).pickTopics = async function(accountId: string, platform: string): Promise<void> {
  const load = async (): Promise<{ cat: TopicItem[]; current: string[] } | null> => {
    try {
      const cat = await invoke<TopicItem[]>('topics_catalog', { platform });
      const current = await invoke<string[]>('get_account_topics', { accountId });
      return { cat, current };
    } catch (e) { showToast('加载主题失败: ' + e, 'error'); return null; }
  };
  const first = await load();
  if (!first) return;
  const overlay = document.createElement('div');
  overlay.className = 'modal active';
  document.body.appendChild(overlay);

  const checkedKeys = (): string[] =>
    Array.from(overlay.querySelectorAll<HTMLInputElement>('input[type=checkbox]:checked')).map(i => i.value);

  const draw = (cat: TopicItem[], selected: string[]) => {
    overlay.innerHTML = `
      <div class="modal-content">
        <div class="modal-header"><h3>选择养号主题（可多选）</h3></div>
        <div class="modal-body">
          ${cat.map(d => `<label style="display:flex;align-items:center;gap:6px;margin:6px 0;">
            <input type="checkbox" value="${d.key}"${selected.includes(d.key) ? ' checked' : ''}> ${escapeHtml(d.label)}
            ${d.builtin ? '' : `<button class="btn btn-small btn-danger" style="margin-left:auto;padding:0 8px;" data-del="${d.key}" title="删除自定义主题">✕</button>`}
          </label>`).join('')}
        </div>
        <div style="display:flex;gap:6px;padding:0 16px 8px;">
          <input id="topicNew" type="text" placeholder="添加主题，如：露营装备" style="flex:1;" />
          <button class="btn btn-small btn-secondary" id="topicAdd">+ 添加</button>
        </div>
        <div class="modal-footer">
          <button class="btn" id="topicCancel">取消</button>
          <button class="btn btn-success" id="topicSave">保存</button>
        </div>
      </div>`;
    bind();
  };
  const reload = async (keep: string[]) => {
    const d = await load();
    if (d) draw(d.cat, Array.from(new Set([...d.current, ...keep])));
  };
  function bind() {
    overlay.querySelector('#topicCancel')!.addEventListener('click', () => overlay.remove());
    overlay.querySelector('#topicSave')!.addEventListener('click', async () => {
      try {
        await invoke('set_account_topics', { accountId, keys: checkedKeys() });
        showToast('养号主题已保存', 'success');
        overlay.remove();
        await loadAccounts();
      } catch (e) { showToast('保存失败: ' + e, 'error'); }
    });
    overlay.querySelector('#topicAdd')!.addEventListener('click', async () => {
      const input = overlay.querySelector<HTMLInputElement>('#topicNew')!;
      const label = input.value.trim();
      if (!label) return;
      const keep = checkedKeys();
      try {
        const item = await invoke<TopicItem>('add_custom_topic', { platform, label });
        keep.push(item.key);
        await reload(keep);
      } catch (e) { showToast('添加失败: ' + e, 'error'); }
    });
    overlay.querySelectorAll<HTMLElement>('[data-del]').forEach(btn => {
      btn.addEventListener('click', async (ev) => {
        ev.preventDefault();
        const key = btn.getAttribute('data-del')!;
        const keep = checkedKeys().filter(k => k !== key);
        try {
          await invoke('delete_custom_topic', { key });
          await reload(keep);
        } catch (e) { showToast('删除失败: ' + e, 'error'); }
      });
    });
  }
  draw(first.cat, first.current);
};
```

- [ ] **Step 2: 删除三个旧 pick 函数**

删除 `pickXNiches`（app.ts:1603-1635）、`pickGithubDomains`（GitHub 领域多选，~1554-1590）、`pickSegmentfaultDomains`（SegmentFault 领域多选，~1639 起）整个函数。保留 `toggleNicheRow`（chip 展开仍用）。

Run（定位）: `grep -nE "pickXNiches|pickGithubDomains|pickSegmentfaultDomains" src/tauri-frontend/app.ts`

- [ ] **Step 3: chip label 改用后端 + 删旧 label map**

把 app.ts:2473-2476 的 4 个 `let` 改为单个：

```ts
let accountTopicLabels: Record<string, string[]> = {};
```

把 `loadAccounts` 中 app.ts:2488-2493 的加载块（`if (Object.keys(ghDomainLabels)...` 到 `accountNichesMap = ...`）替换为：

```ts
    try { accountTopicLabels = (await invoke<Record<string, string[]>>('account_topic_labels')) || {}; } catch { /* */ }
```

把 renderAccountCard 的 chip 块（app.ts:3109-3119）替换为（直接用 label，不再查 key→label）：

```ts
        ${(() => {
          const labels = accountTopicLabels[account.id] || [];
          if (!labels.length) return '';
          const chip = (lb: string, hidden: boolean) => `<span class="stage-badge" style="background:var(--bg-secondary);color:var(--primary);${hidden ? 'display:none;' : ''}" data-extra="${hidden ? '1' : '0'}" title="养号主题">🎯 ${escapeHtml(lb)}</span>`;
          const chips = labels.map((lb, i) => chip(lb, i >= 2)).join('');
          const more = labels.length > 2
            ? `<button class="btn btn-small btn-secondary" style="padding:0 8px;font-size:11px;" onclick="toggleNicheRow(this)" data-more="${labels.length - 2}">展开 +${labels.length - 2}</button>`
            : '';
          return `<div style="display:flex;align-items:center;gap:6px;flex-wrap:wrap;margin-top:4px;" data-expanded="0">${chips}${more}</div>`;
        })()}
```

- [ ] **Step 4: 卡片入口统一**

把 app.ts:3135-3137 的三个分支按钮替换为一行（四平台统一）：

```ts
          ${['github','twitter','x','segmentfault','xiaohongshu'].includes(account.platform) ? `<button class="btn btn-small btn-secondary" onclick="pickTopics('${account.id}','${escapeHtml(account.platform)}')" title="选择养号主题">🎯 主题</button>` : ''}
```

- [ ] **Step 5: 打包 + 类型检查**

Run: `npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js 2>&1 | tail -3 && npx tsc --noEmit 2>&1 | grep -i "app.ts" | tail -10`
Expected: esbuild `Done`；tsc 对 app.ts 无新增 error（旧 label map/pick 删除后若有残余引用，按报错清掉）。

- [ ] **Step 6: 手动验证（app 在 tauri dev 跑）**

1. GitHub/X/SegmentFault/小红书 账号卡片均出现 `🎯 主题` 按钮。
2. 打开各平台 modal：看到对应内置主题（GitHub 13 领域 / X 16 方向 / SF 12 领域 / 小红书 12 主题），平台之间不串。
3. 在小红书加「露营装备」→ 自动勾上 → 保存 → 卡片 chip 出现；GitHub modal 里**不**出现「露营装备」（平台隔离）。
4. 删除「露营装备」→ 列表与 chip 同步移除。
5. 确认迁移：原先选过领域的 GitHub/X/SF 账号，打开 modal 仍回勾旧选择（数据已迁入 nurture_topics）。

- [ ] **Step 7: Commit**

```bash
git add src/tauri-frontend/app.ts dist/tauri/scripts/app.js
git commit -m "feat(nurture): 前端统一养号主题(pickTopics 合并三函数+四平台入口+chip)"
```

---

## 自我审查记录

- **Spec 覆盖**：迁移+数据搬运(Task 1) · builtin_topics/类型/catalog(Task 2) · set(Task 3) · add(Task 4) · delete(Task 5) · account_topic_keywords(Task 6) · 6 命令+account_topic_labels(Task 7) · 3 runner 改造(Task 8) · 删旧代码(Task 9) · 前端统一(Task 10)。spec 各节均有任务。
- **平台隔离**：custom_topics 带 platform、catalog/add 按 platform、key 跨平台可重名 —— Task 2/4 测试覆盖。
- **类型/命名一致**：`TopicDef`/`TopicItem{key,label,keywords,builtin}`；helper `builtin_topics`/`account_topics`/`account_platform`/`topics_catalog_from`/`set_account_topics_conn`/`add_custom_topic_conn`/`delete_custom_topic_conn`/`account_topic_keywords`；命令 `topics_catalog`/`get|set_account_topics`/`add|delete_custom_topic`/`account_topic_labels`；前端 invoke 参数 `platform`/`accountId`/`keys`/`label`/`key` 与 Rust 签名（camelCase↔snake_case）匹配，前后一致。
- **runner 类型**：`topic`/`kw` 改为 `&topics[i]`/`&kws[i]`（&String），下游 `to_string`/`format!`/`urlencoding::encode` 均兼容；Task 8 Step 4 build gate 兜底。
- **删除安全**：保留四常量当内置源；Task 9 全量 `cargo test` 兜底残余引用。
