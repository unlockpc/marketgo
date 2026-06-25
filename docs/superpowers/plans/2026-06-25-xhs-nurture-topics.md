> **已作废**：本计划为小红书专属版。范围已升级为「平台隔离统一主题系统 + 三套现有平台迁入」，将由新计划取代。新设计见 `docs/superpowers/specs/2026-06-25-nurture-topics-unified-design.md`。下文仅留作历史记录。

# 小红书养号主题选择 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给小红书账号引入「内置主题 + 用户自定义添加 + 多选勾选」的养号主题选择系统，主题持久化存储（runner 后续做）。

**Architecture:** 完全复用现有 X/SegmentFault 的「领域选择」模式：内置主题硬编码为 Rust 常量；用户自定义主题存新表 `xhs_custom_topics`；账号已选 keys 存 `accounts.xhs_topics` JSON 列。后端把有分支的逻辑拆成可测的 conn-level helper，命令是薄包装。前端新增一个带「添加/删除」的多选 modal，复用现有 `.modal.active` 与 chip 展示。

**Tech Stack:** Rust + Tauri v2 + rusqlite；TypeScript 前端（esbuild 打包）。

参考设计：`docs/superpowers/specs/2026-06-25-xhs-nurture-topics-design.md`

---

## File Structure

- `src-tauri/src/lib.rs`（修改）
  - DB 迁移块（~3656）：加 `xhs_topics` 列 + 建 `xhs_custom_topics` 表
  - niches 区（~1316 之后）：`XhsTopic` 常量、`XhsTopicItem`、conn-level helpers、5 个 `#[tauri::command]`
  - `account_niches`（1330-1352）：批量查询纳入 `xhs_topics`，platform 映射加 xiaohongshu
  - `invoke_handler`（~12359）：注册 5 个新命令
  - 测试模块（文件末尾，~12760 后）：新增 `mod xhs_topics_tests`
- `src/tauri-frontend/app.ts`（修改）
  - 新函数 `pickXhsTopics`（1635 `pickXNiches` 之后区域）
  - `xhsTopicLabels` 声明（~2475）+ `loadAccounts` 中每次刷新（~2492）
  - 账号卡片入口（3137 旁）+ chip label map（3112）

参考现有同构实现：`account_x_niches`(lib.rs:1204)、`x_niches_catalog`/`get_account_x_niches`/`set_account_x_niches`(1257-1283)、`account_niches`(1330)、`pickXNiches`(app.ts:1603)。

---

### Task 1: DB 迁移（列 + 表）

**Files:**
- Modify: `src-tauri/src/lib.rs:3656`

- [ ] **Step 1: 加迁移语句**

在 lib.rs:3656 的 `sf_domains` 迁移行之后追加两行：

```rust
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN sf_domains TEXT", []);
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN xhs_topics TEXT", []);
    let _ = conn.execute("CREATE TABLE IF NOT EXISTS xhs_custom_topics (key TEXT PRIMARY KEY, label TEXT NOT NULL)", []);
```

（第一行是已存在的，用于定位；新增后两行。`ALTER TABLE ADD COLUMN` 已存在时返回 Err 被 `let _ =` 吞掉，与现有迁移一致。）

- [ ] **Step 2: 编译确认无误**

Run: `cd src-tauri && cargo build 2>&1 | tail -5`
Expected: `Finished` 无 error（warning 允许）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 小红书主题 DB 迁移(xhs_topics 列 + xhs_custom_topics 表)"
```

---

### Task 2: 内置主题常量 + 读取/catalog helper（TDD）

**Files:**
- Modify: `src-tauri/src/lib.rs`（niches 区，紧接 `set_account_sf_domains` 之后，约 1316 行）
- Test: `src-tauri/src/lib.rs` 文件末尾新增 `mod xhs_topics_tests`

- [ ] **Step 1: 写失败测试**

在文件末尾（现有 `mod x_db_tests` 之后，约 12760 行）追加：

```rust
#[cfg(test)]
mod xhs_topics_tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("
            CREATE TABLE accounts (id TEXT PRIMARY KEY, xhs_topics TEXT);
            CREATE TABLE xhs_custom_topics (key TEXT PRIMARY KEY, label TEXT NOT NULL);
        ").unwrap();
        c
    }

    #[test]
    fn catalog_merges_builtin_then_custom() {
        let c = setup();
        c.execute("INSERT INTO xhs_custom_topics (key, label) VALUES ('cust1', '露营装备')", []).unwrap();
        let cat = xhs_catalog_from(&c);
        // 内置 12 个在前，自定义在后
        assert_eq!(cat.len(), XHS_TOPICS.len() + 1);
        assert!(cat[0].builtin);
        assert_eq!(cat[0].key, "beauty");
        let last = cat.last().unwrap();
        assert!(!last.builtin);
        assert_eq!(last.label, "露营装备");
    }

    #[test]
    fn read_account_topics() {
        let c = setup();
        c.execute("INSERT INTO accounts (id, xhs_topics) VALUES ('acc1', '[\"beauty\",\"food\"]')", []).unwrap();
        assert_eq!(account_xhs_topics(&c, "acc1"), vec!["beauty".to_string(), "food".to_string()]);
        assert!(account_xhs_topics(&c, "nope").is_empty());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test xhs_topics_tests 2>&1 | tail -20`
Expected: 编译失败 —— `cannot find function xhs_catalog_from` / `account_xhs_topics` / `XHS_TOPICS`。

- [ ] **Step 3: 实现常量 + helper**

在 lib.rs `set_account_sf_domains` 命令结束后（约 1316 行 `}` 之后）插入：

```rust
#[derive(Clone, Copy)]
struct XhsTopic { key: &'static str, label: &'static str }

/// 小红书养号主题（内置赛道，代码事实源）。用户自定义主题另存 xhs_custom_topics 表。
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

/// 读账号所选小红书主题（xhs_topics JSON 数组），解析失败/空 → 空 vec。
fn account_xhs_topics(conn: &Connection, account_id: &str) -> Vec<String> {
    let raw: Option<String> = conn.query_row(
        "SELECT xhs_topics FROM accounts WHERE id=?1",
        params![account_id], |r| r.get(0)).ok().flatten();
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default()
}

#[derive(serde::Serialize, Clone)]
pub struct XhsTopicItem { pub key: String, pub label: String, pub builtin: bool }

/// catalog = 内置 ∪ 自定义（内置在前，自定义按插入顺序）。
fn xhs_catalog_from(conn: &Connection) -> Vec<XhsTopicItem> {
    let mut out: Vec<XhsTopicItem> = XHS_TOPICS.iter().map(|t| XhsTopicItem {
        key: t.key.to_string(), label: t.label.to_string(), builtin: true,
    }).collect();
    if let Ok(mut stmt) = conn.prepare("SELECT key, label FROM xhs_custom_topics ORDER BY rowid") {
        if let Ok(rows) = stmt.query_map([], |r| Ok(XhsTopicItem {
            key: r.get::<_, String>(0)?, label: r.get::<_, String>(1)?, builtin: false,
        })) {
            for it in rows.flatten() { out.push(it); }
        }
    }
    out
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test xhs_topics_tests 2>&1 | tail -10`
Expected: `test result: ok. 2 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 小红书内置主题常量 + catalog/读取 helper"
```

---

### Task 3: 保存账号已选主题 helper（TDD）

**Files:**
- Modify: `src-tauri/src/lib.rs`（接 Task 2 的 helper 之后）
- Test: `mod xhs_topics_tests`

- [ ] **Step 1: 写失败测试**

在 `mod xhs_topics_tests` 内追加：

```rust
    #[test]
    fn set_filters_unknown_keys_and_roundtrips() {
        let c = setup();
        c.execute("INSERT INTO accounts (id) VALUES ('acc1')", []).unwrap();
        // beauty 合法、ghost 非法 → 只留 beauty
        set_account_xhs_topics_conn(&c, "acc1", &["beauty".to_string(), "ghost".to_string()]).unwrap();
        assert_eq!(account_xhs_topics(&c, "acc1"), vec!["beauty".to_string()]);
    }

    #[test]
    fn set_keeps_custom_key() {
        let c = setup();
        c.execute("INSERT INTO accounts (id) VALUES ('acc1')", []).unwrap();
        c.execute("INSERT INTO xhs_custom_topics (key, label) VALUES ('cust1', '露营装备')", []).unwrap();
        set_account_xhs_topics_conn(&c, "acc1", &["cust1".to_string()]).unwrap();
        assert_eq!(account_xhs_topics(&c, "acc1"), vec!["cust1".to_string()]);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test xhs_topics_tests::set 2>&1 | tail -15`
Expected: 编译失败 —— `cannot find function set_account_xhs_topics_conn`。

- [ ] **Step 3: 实现 set helper**

紧接 `xhs_catalog_from` 之后插入：

```rust
/// 保存账号所选主题，仅保留 catalog（内置+自定义）中存在的 key。
fn set_account_xhs_topics_conn(conn: &Connection, account_id: &str, keys: &[String]) -> Result<(), String> {
    let valid: std::collections::HashSet<String> =
        xhs_catalog_from(conn).into_iter().map(|i| i.key).collect();
    let kept: Vec<String> = keys.iter().filter(|k| valid.contains(*k)).cloned().collect();
    let json = serde_json::to_string(&kept).map_err(|e| e.to_string())?;
    conn.execute("UPDATE accounts SET xhs_topics=?1 WHERE id=?2", params![json, account_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test xhs_topics_tests 2>&1 | tail -10`
Expected: `test result: ok. 4 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 小红书账号已选主题保存 helper(过滤非法 key)"
```

---

### Task 4: 添加自定义主题 helper（TDD）

**Files:**
- Modify: `src-tauri/src/lib.rs`（接 Task 3 之后）
- Test: `mod xhs_topics_tests`

- [ ] **Step 1: 写失败测试**

在 `mod xhs_topics_tests` 内追加：

```rust
    #[test]
    fn add_custom_then_appears() {
        let c = setup();
        let item = add_xhs_custom_topic_conn(&c, "  露营装备  ").unwrap();
        assert!(!item.builtin);
        assert_eq!(item.label, "露营装备"); // trim 生效
        assert!(xhs_catalog_from(&c).iter().any(|i| i.key == item.key && i.label == "露营装备"));
    }

    #[test]
    fn add_rejects_empty_and_dup() {
        let c = setup();
        assert!(add_xhs_custom_topic_conn(&c, "   ").is_err());        // 空
        assert!(add_xhs_custom_topic_conn(&c, "美妆护肤").is_err());   // 与内置重名
        add_xhs_custom_topic_conn(&c, "露营装备").unwrap();
        assert!(add_xhs_custom_topic_conn(&c, "露营装备").is_err());   // 与已有自定义重名
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test xhs_topics_tests::add 2>&1 | tail -15`
Expected: 编译失败 —— `cannot find function add_xhs_custom_topic_conn`。

- [ ] **Step 3: 实现 add helper**

紧接 `set_account_xhs_topics_conn` 之后插入：

```rust
/// 加自定义主题：label trim 非空 + 与现有(内置+自定义)label 不重名；生成 uuid key。
fn add_xhs_custom_topic_conn(conn: &Connection, label: &str) -> Result<XhsTopicItem, String> {
    let label = label.trim();
    if label.is_empty() { return Err("主题名不能为空".to_string()); }
    if xhs_catalog_from(conn).iter().any(|i| i.label == label) {
        return Err("主题已存在".to_string());
    }
    let key = Uuid::new_v4().to_string();
    conn.execute("INSERT INTO xhs_custom_topics (key, label) VALUES (?1, ?2)",
        params![key, label]).map_err(|e| e.to_string())?;
    Ok(XhsTopicItem { key, label: label.to_string(), builtin: false })
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test xhs_topics_tests 2>&1 | tail -10`
Expected: `test result: ok. 6 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 添加小红书自定义主题 helper(去空格+重名校验)"
```

---

### Task 5: 删除自定义主题 helper（TDD）

**Files:**
- Modify: `src-tauri/src/lib.rs`（接 Task 4 之后）
- Test: `mod xhs_topics_tests`

- [ ] **Step 1: 写失败测试**

在 `mod xhs_topics_tests` 内追加：

```rust
    #[test]
    fn delete_custom_strips_from_accounts() {
        let c = setup();
        c.execute("INSERT INTO xhs_custom_topics (key, label) VALUES ('cust1', '露营装备')", []).unwrap();
        c.execute("INSERT INTO accounts (id, xhs_topics) VALUES ('acc1', '[\"beauty\",\"cust1\"]')", []).unwrap();
        delete_xhs_custom_topic_conn(&c, "cust1").unwrap();
        // 表里没了
        assert!(!xhs_catalog_from(&c).iter().any(|i| i.key == "cust1"));
        // 账号已选里也被移除，beauty 保留
        assert_eq!(account_xhs_topics(&c, "acc1"), vec!["beauty".to_string()]);
    }

    #[test]
    fn delete_rejects_builtin() {
        let c = setup();
        assert!(delete_xhs_custom_topic_conn(&c, "beauty").is_err());
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test xhs_topics_tests::delete 2>&1 | tail -15`
Expected: 编译失败 —— `cannot find function delete_xhs_custom_topic_conn`。

- [ ] **Step 3: 实现 delete helper**

紧接 `add_xhs_custom_topic_conn` 之后插入：

```rust
/// 删自定义主题（内置 key 拒绝），并从所有账号 xhs_topics 已选里移除该 key（防悬空引用）。
fn delete_xhs_custom_topic_conn(conn: &Connection, key: &str) -> Result<(), String> {
    if XHS_TOPICS.iter().any(|t| t.key == key) {
        return Err("内置主题不可删除".to_string());
    }
    conn.execute("DELETE FROM xhs_custom_topics WHERE key=?1", params![key])
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT id, xhs_topics FROM accounts WHERE xhs_topics IS NOT NULL")
            .map_err(|e| e.to_string())?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?.flatten().collect()
    };
    for (id, raw) in rows {
        if let Ok(keys) = serde_json::from_str::<Vec<String>>(&raw) {
            if keys.iter().any(|k| k == key) {
                let kept: Vec<String> = keys.into_iter().filter(|k| k != key).collect();
                let json = serde_json::to_string(&kept).map_err(|e| e.to_string())?;
                conn.execute("UPDATE accounts SET xhs_topics=?1 WHERE id=?2", params![json, id])
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test xhs_topics_tests 2>&1 | tail -10`
Expected: `test result: ok. 8 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 删除小红书自定义主题 helper(连带清理账号已选)"
```

---

### Task 6: 5 个命令 + 注册 + account_niches 接入

**Files:**
- Modify: `src-tauri/src/lib.rs`（命令插在 Task 5 helper 之后；注册在 ~12359；`account_niches` 在 1330-1352）

- [ ] **Step 1: 写 5 个命令包装**

紧接 `delete_xhs_custom_topic_conn` 之后插入：

```rust
#[tauri::command]
fn xhs_topics_catalog(state: State<AppState>) -> Result<Vec<XhsTopicItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(xhs_catalog_from(&conn))
}

#[tauri::command]
fn get_account_xhs_topics(state: State<AppState>, account_id: String) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(account_xhs_topics(&conn, &account_id))
}

#[tauri::command]
fn set_account_xhs_topics(state: State<AppState>, account_id: String, keys: Vec<String>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    set_account_xhs_topics_conn(&conn, &account_id, &keys)
}

#[tauri::command]
fn add_xhs_custom_topic(state: State<AppState>, label: String) -> Result<XhsTopicItem, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    add_xhs_custom_topic_conn(&conn, &label)
}

#[tauri::command]
fn delete_xhs_custom_topic(state: State<AppState>, key: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    delete_xhs_custom_topic_conn(&conn, &key)
}
```

- [ ] **Step 2: 注册命令**

在 lib.rs:12359 的 `sf_domains_catalog,` 行附近、`account_niches,` 之前插入：

```rust
            sf_domains_catalog,
            xhs_topics_catalog,
            get_account_xhs_topics,
            set_account_xhs_topics,
            add_xhs_custom_topic,
            delete_xhs_custom_topic,
```

（首行 `sf_domains_catalog,` 已存在，用于定位。）

- [ ] **Step 3: account_niches 纳入小红书**

把 lib.rs:1332 的查询、1333-1337 的 query_map、1340-1346 的 match 改为（新增 `xhs_topics` 列与映射）：

```rust
    let mut stmt = conn.prepare("SELECT id, platform, gh_domains, x_niches, sf_domains, xhs_topics FROM accounts").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok((
        r.get::<_, String>(0)?, r.get::<_, String>(1)?,
        r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?,
        r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?,
    ))).map_err(|e| e.to_string())?;
    let mut out = std::collections::HashMap::new();
    for row in rows.flatten() {
        let (id, platform, gh, x, sf, xhs) = row;
        let raw = match platform.to_lowercase().as_str() {
            "github" => gh,
            "twitter" | "x" => x,
            "segmentfault" => sf,
            "xiaohongshu" | "redbook" => xhs,
            _ => None,
        };
        if let Some(keys) = raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()) {
            if !keys.is_empty() { out.insert(id, keys); }
        }
    }
    Ok(out)
```

- [ ] **Step 4: 编译 + 全量后端测试**

Run: `cd src-tauri && cargo build 2>&1 | tail -3 && cargo test xhs_topics_tests 2>&1 | tail -5`
Expected: `Finished` 无 error；`test result: ok. 8 passed`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): 小红书主题 5 命令 + 注册 + account_niches 接入"
```

---

### Task 7: 前端 —— 多选 modal + 卡片入口 + chip label

**Files:**
- Modify: `src/tauri-frontend/app.ts`（~1635 / ~2475 / ~2492 / ~3112 / ~3137）

- [ ] **Step 1: 新增 pickXhsTopics 函数**

在 app.ts `pickXNiches` 函数结束后（约 1635 行 `};` 之后）插入：

```ts
interface XhsTopicItem { key: string; label: string; builtin: boolean }

// 小红书养号主题多选（按账号）：内置 + 用户自定义；可现场添加/删除自定义主题。复用 .modal.active。
(window as any).pickXhsTopics = async function(accountId: string): Promise<void> {
  const load = async (): Promise<{ cat: XhsTopicItem[]; current: string[] } | null> => {
    try {
      const cat = await invoke<XhsTopicItem[]>('xhs_topics_catalog');
      const current = await invoke<string[]>('get_account_xhs_topics', { accountId });
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

  const draw = (cat: XhsTopicItem[], selected: string[]) => {
    overlay.innerHTML = `
      <div class="modal-content">
        <div class="modal-header"><h3>选择小红书养号主题（可多选）</h3></div>
        <div class="modal-body">
          ${cat.map(d => `<label style="display:flex;align-items:center;gap:6px;margin:6px 0;">
            <input type="checkbox" value="${d.key}"${selected.includes(d.key) ? ' checked' : ''}> ${escapeHtml(d.label)}
            ${d.builtin ? '' : `<button class="btn btn-small btn-danger" style="margin-left:auto;padding:0 8px;" data-del="${d.key}" title="删除自定义主题">✕</button>`}
          </label>`).join('')}
        </div>
        <div style="display:flex;gap:6px;padding:0 16px 8px;">
          <input id="xhsNewTopic" type="text" placeholder="添加主题，如：露营装备" style="flex:1;" />
          <button class="btn btn-small btn-secondary" id="xhsAddTopic">+ 添加</button>
        </div>
        <div class="modal-footer">
          <button class="btn" id="xhsCancel">取消</button>
          <button class="btn btn-success" id="xhsSave">保存</button>
        </div>
      </div>`;
    bind();
  };

  // 重新拉取 catalog（add/delete 后），并把传入的 keep 与服务端 current 合并回勾
  const reload = async (keep: string[]) => {
    const d = await load();
    if (d) draw(d.cat, Array.from(new Set([...d.current, ...keep])));
  };

  function bind() {
    overlay.querySelector('#xhsCancel')!.addEventListener('click', () => overlay.remove());
    overlay.querySelector('#xhsSave')!.addEventListener('click', async () => {
      try {
        await invoke('set_account_xhs_topics', { accountId, keys: checkedKeys() });
        showToast('小红书主题已保存', 'success');
        overlay.remove();
        await loadAccounts();
      } catch (e) { showToast('保存失败: ' + e, 'error'); }
    });
    overlay.querySelector('#xhsAddTopic')!.addEventListener('click', async () => {
      const input = overlay.querySelector<HTMLInputElement>('#xhsNewTopic')!;
      const label = input.value.trim();
      if (!label) return;
      const keep = checkedKeys();
      try {
        const item = await invoke<XhsTopicItem>('add_xhs_custom_topic', { label });
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
          await invoke('delete_xhs_custom_topic', { key });
          await reload(keep);
        } catch (e) { showToast('删除失败: ' + e, 'error'); }
      });
    });
  }

  draw(first.cat, first.current);
};
```

- [ ] **Step 2: 声明 xhsTopicLabels + 每次刷新**

在 app.ts:2475 `let sfDomainLabels...` 行后加一行：

```ts
let sfDomainLabels: Record<string, string> = {};
let xhsTopicLabels: Record<string, string> = {};
```

在 app.ts:2492（`if (Object.keys(ghDomainLabels).length === 0) {...}` 块的 `}` 之后、`accountNichesMap` 那行之前）追加（每次刷新，因为自定义主题会动态增删）：

```ts
    }
    // 小红书主题含用户自定义，每次刷新以保证新增/删除后 chip label 同步
    try { (await invoke<any[]>('xhs_topics_catalog')).forEach((t: any) => { xhsTopicLabels[t.key] = t.label; }); } catch { /* */ }
    try { accountNichesMap = (await invoke<Record<string, string[]>>('account_niches')) || {}; } catch { /* */ }
```

（末两行的 `}` 与 `accountNichesMap` 行是已存在锚点，仅在其间插入中间那两行注释 + try。）

- [ ] **Step 3: chip label map 纳入小红书**

把 app.ts:3112 的 `lm` 定义改为：

```ts
          const lm = account.platform === 'github' ? ghDomainLabels : ((account.platform === 'twitter' || account.platform === 'x') ? xNicheLabels : (account.platform === 'segmentfault' ? sfDomainLabels : (account.platform === 'xiaohongshu' ? xhsTopicLabels : {})));
```

- [ ] **Step 4: 账号卡片加「主题」入口**

在 app.ts:3137 的 SegmentFault 入口行之后插入一行：

```ts
          ${account.platform === 'segmentfault' ? `<button class="btn btn-small btn-secondary" onclick="pickSegmentfaultDomains('${account.id}')" title="选择 SegmentFault 养号领域（养号时按领域搜索→浏览→读文章）">🎯 领域</button>` : ''}
          ${account.platform === 'xiaohongshu' ? `<button class="btn btn-small btn-secondary" onclick="pickXhsTopics('${account.id}')" title="选择小红书养号主题">🎯 主题</button>` : ''}
```

（首行是已存在锚点。）

- [ ] **Step 5: 打包前端 + 类型检查**

Run: `npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js 2>&1 | tail -3 && npx tsc --noEmit 2>&1 | tail -5`
Expected: esbuild 打印 `app.js ... Done`；tsc 无 error 输出（若 tsc 因既有无关错误报错，只确认本次新增代码无新报错）。

- [ ] **Step 6: 手动验证（应用已在 tauri dev 跑）**

1. 在身份管理页找到一个小红书账号卡片，确认出现 `🎯 主题` 按钮。
2. 点开 → 看到 12 个内置主题；输入「露营装备」点「+ 添加」→ 列表追加该项且自动勾上。
3. 勾几个内置 + 自定义 → 保存 → 卡片上出现对应 `🎯 chip`。
4. 重开 modal → 删除「露营装备」（✕）→ 列表移除；保存后卡片 chip 不再有它。

- [ ] **Step 7: Commit**

```bash
git add src/tauri-frontend/app.ts dist/tauri/scripts/app.js
git commit -m "feat(nurture): 小红书养号主题选择前端(多选+自定义增删+卡片入口)"
```

---

## 自我审查记录

- **Spec 覆盖**：数据存储(Task 1/2) · 5 命令(Task 6) · catalog 合并(Task 2) · set 过滤(Task 3) · add 去空格+重名(Task 4) · delete 内置拒绝+连带清理(Task 5) · 前端 modal/入口/chip(Task 7) · 内置 12 主题(Task 2) · account_niches 接入(Task 6) —— 全部有对应任务。
- **非目标**：runner、跨平台推广、主题关键词 —— 计划内均未触碰。
- **类型一致**：`XhsTopicItem{key,label,builtin}`、helper 名 `xhs_catalog_from`/`account_xhs_topics`/`set_account_xhs_topics_conn`/`add_xhs_custom_topic_conn`/`delete_xhs_custom_topic_conn`、命令名 `xhs_topics_catalog`/`get|set_account_xhs_topics`/`add|delete_xhs_custom_topic` 前后一致；前端 invoke 参数 `accountId`/`keys`/`label`/`key` 与 Rust 命令签名（tauri camelCase↔snake_case）匹配。
