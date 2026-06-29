# X 养号自动回复 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** X 养号时读取搜索到的推文正文，交配置的大模型生成养号互动型回复，按配额直接发出（带开关，默认关）。

**Architecture:** 在 `x_nurture_run`(nurture.rs) 现有点赞/关注/转推之后插入「回复」阶段：读推文正文(`article [data-testid="tweetText"]`)→`gen_nurture_text("x_reply",正文)`大模型生成→`twitter_reply()`发→`x_actions_log`去重记录。开关存 config、配额按 phase。

**Tech Stack:** Rust(Tauri v2, rusqlite, reqwest)、unzoo MCP、已集成的 ai.rs(Gemini/OpenAI/DeepSeek/Qwen)、TypeScript(esbuild)。

参考设计：`docs/superpowers/specs/2026-06-29-x-nurture-auto-reply-design.md`

---

## 文件结构

- `src-tauri/src/nurture.rs` — `x_reply_quota`、`x_clean_tweet_text`(纯)、`x_read_tweet_text_blocking`、`x_nurture_run` 内回复阶段。
- `src-tauri/src/lib.rs` — `x_reply_enabled` 读取 + `get_x_reply_enabled`/`set_x_reply_enabled` 命令 + 注册；单测。
- `src/tauri-frontend/app.ts` + `dist/tauri/index.html` — AI 设置页「X 自动回复」开关。
- 复用（不改）：`gen_nurture_text`(ai.rs:197)、`twitter_reply`(lib.rs:10351)、`x_already_acted`/`x_record_action`(lib.rs:1340/1346)。

Rust 任务结束跑 `cd src-tauri && cargo test --lib <name>` + `cargo check --lib`；前端跑 `npx tsc --noEmit` + esbuild 打包。

---

## Task 1: `x_reply_quota` + `x_clean_tweet_text` 纯函数 + 单测

**Files:**
- Modify: `src-tauri/src/nurture.rs`（紧接 `x_expand_query` 等扩展函数之后，约 545 行附近）
- Test: `src-tauri/src/lib.rs`（`xhs_runner_tests` mod 内追加）

- [ ] **Step 1: 写两个纯函数**

在 `src-tauri/src/nurture.rs` 的 `x_expand_query` 函数之后插入：

```rust
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
```

- [ ] **Step 2: 写失败测试**

在 `src-tauri/src/lib.rs` 的 `mod xhs_runner_tests { ... }` 内（任一 `#[test]` 之后）追加：

```rust
    #[test]
    fn x_reply_quota_by_phase() {
        use crate::nurture::x_reply_quota;
        assert_eq!(x_reply_quota("warmup"), 1);
        assert_eq!(x_reply_quota("growth"), 1);
        assert_eq!(x_reply_quota("mature"), 2);
        assert_eq!(x_reply_quota("other"), 1);
    }

    #[test]
    fn x_clean_tweet_text_length_gate() {
        use crate::nurture::x_clean_tweet_text;
        // 太短 / 空 → None
        assert_eq!(x_clean_tweet_text(""), None);
        assert_eq!(x_clean_tweet_text("  short  "), None);      // trim 后 5 字符
        // 足够长 → Some(trimmed)
        let long = "  this is a long enough tweet body  ";
        assert_eq!(x_clean_tweet_text(long), Some("this is a long enough tweet body".to_string()));
    }
```

- [ ] **Step 3: 跑测试**

Run: `cd src-tauri && cargo test --lib x_reply_quota_by_phase x_clean_tweet_text_length_gate`
Expected: 2 passed

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/nurture.rs src-tauri/src/lib.rs
git commit -m "feat(nurture): X 回复配额 x_reply_quota + 正文长度门 x_clean_tweet_text + 单测"
```

---

## Task 2: 开关 config 读取 + 命令

**Files:**
- Modify: `src-tauri/src/lib.rs`（`x_record_action` 函数之后，约 1352 行附近加 helper + 命令；invoke_handler 12462 区注册）

- [ ] **Step 1: 写 helper + 两个命令**

在 `src-tauri/src/lib.rs` 的 `fn x_record_action(...)` 闭合大括号之后插入：

```rust
/// 读 X 养号自动回复开关（config `x_nurture_reply_enabled` == "1" 才开；缺省=关）。
pub(crate) fn x_reply_enabled(conn: &Connection) -> bool {
    conn.query_row("SELECT value FROM config WHERE key='x_nurture_reply_enabled'", [], |r| r.get::<_, String>(0))
        .map(|v| v == "1").unwrap_or(false)
}

/// 读 X 自动回复开关（前端设置页用）。
#[tauri::command]
fn get_x_reply_enabled(state: State<AppState>) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(x_reply_enabled(&conn))
}

/// 设 X 自动回复开关。
#[tauri::command]
fn set_x_reply_enabled(state: State<AppState>, enabled: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES ('x_nurture_reply_enabled', ?1)",
        params![if enabled { "1" } else { "0" }],
    ).map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 2: 注册命令**

在 `src-tauri/src/lib.rs` invoke_handler 里 `get_ai_config,` 之后加：

```rust
            get_x_reply_enabled,
            set_x_reply_enabled,
```

- [ ] **Step 3: 编译验证**

Run: `cd src-tauri && cargo check --lib 2>&1 | grep -E "^error|Finished" | tail -3`
Expected: `Finished`（仅 warnings）

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(nurture): X 自动回复开关 config + get/set 命令"
```

---

## Task 3: 读推文正文 `x_read_tweet_text_blocking`

**Files:**
- Modify: `src-tauri/src/nurture.rs`（`x_clean_tweet_text` 之后插入）

- [ ] **Step 1: 写 blocking helper**

在 `src-tauri/src/nurture.rs` 的 `x_clean_tweet_text` 函数之后插入：

```rust
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
```

- [ ] **Step 2: 编译验证**

Run: `cd src-tauri && cargo check --lib 2>&1 | grep -E "^error|Finished" | tail -3`
Expected: `Finished`

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/nurture.rs
git commit -m "feat(nurture): x_read_tweet_text_blocking 读推文正文(详情页 tweetText)"
```

---

## Task 4: 在 x_nurture_run 接入回复阶段

**Files:**
- Modify: `src-tauri/src/nurture.rs`（`x_nurture_run` 内，转推段之后 `let _ = engages;`（约 915 行）与 L3 原创段（约 917 行）之间插入）

- [ ] **Step 1: 插入回复阶段**

在 `src-tauri/src/nurture.rs` 的 `let _ = engages;` 这一行之后插入：

```rust
    // 7a-bis) 自动回复（开关开 + 配额>0）：读推文正文 → 大模型生成切题回复 → 直接发。
    // 风险动作：默认关；读不到正文/生成不合格都跳过；回复间隔 30~90s；跨 session 去重(#reply)。
    let reply_quota = x_reply_quota(&phase);
    let reply_on = {
        let st = app.state::<AppState>();
        st.db.lock().ok().map(|c| crate::x_reply_enabled(&c)).unwrap_or(false)
    };
    let mut replies = 0i64;
    if aborted_health.is_none() && reply_on && reply_quota > 0 {
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
            let reply = match gen_nurture_text(app, "x_reply", &body).await {
                Some(r) => r,
                None => { emit_nurture_step(app, account_id, "未配置 AI 或回复不合格，跳过回复"); continue }
            };
            emit_nurture_step(app, account_id, &format!("💬 回复 {}/{}", replies + 1, reply_quota));
            // 发回复（twitter_reply 自带导航 + Draft.js 注入 + 提交）
            let url = t.clone(); let rep = reply.clone();
            let r = tauri::async_runtime::spawn_blocking(move || twitter_reply(&url, &rep)).await.map_err(|e| e.to_string())?;
            if r.is_ok() {
                let st = app.state::<AppState>(); let l = st.db.lock();
                if let Ok(conn) = l { let _ = x_record_action(&conn, account_id, "reply", &key); }
                replies += 1;
            }
            // 回复间隔 30~90s（拟人）
            tokio::time::sleep(std::time::Duration::from_millis(get_random_delay(30, 90))).await;
        }
    }
    let _ = replies;
```

- [ ] **Step 2: 编译验证**

Run: `cd src-tauri && cargo check --lib 2>&1 | grep -E "^error|Finished" | tail -5`
Expected: `Finished`。若报 `twitter_reply` 未找到，改用 `crate::twitter_reply`（它是 crate 根私有函数，子模块可见）。

- [ ] **Step 3: 跑全量 lib 测试确认无回归**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -4`
Expected: 新增测试全过（既有 `platform_meta_tests` 2 个失败为历史问题，与本次无关）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/nurture.rs
git commit -m "feat(nurture): X 养号接入自动回复(读正文→大模型生成→发→去重, 开关+配额控制)"
```

---

## Task 5: 前端「X 自动回复」开关

**Files:**
- Modify: `dist/tauri/index.html`（AI 设置卡片内加勾选框）
- Modify: `src/tauri-frontend/app.ts`（加载时读状态、切换时写）

- [ ] **Step 1: index.html 加勾选框**

在 `dist/tauri/index.html` 的 AI 设置区（含 `id="aiProvider"` 的那张卡片内、保存按钮附近）插入：

```html
        <div style="margin-top:12px;display:flex;align-items:center;gap:8px;">
          <input type="checkbox" id="xReplyEnabled">
          <label for="xReplyEnabled" style="font-size:13px;">
            X 养号自动回复（开启后养号会读推文→大模型生成回复→自动发；高风险，默认关）
          </label>
        </div>
```

- [ ] **Step 2: app.ts 读状态 + 绑定切换**

在 `src/tauri-frontend/app.ts` 加载 AI 设置的函数里（搜索 `get_ai_config` 的 invoke 调用处），其后追加读取开关：

```ts
  try {
    const xReply = await invoke<boolean>('get_x_reply_enabled');
    const cb = document.getElementById('xReplyEnabled') as HTMLInputElement | null;
    if (cb) cb.checked = !!xReply;
  } catch {}
```

在事件绑定区（搜索 `document.getElementById('aiProvider')?.addEventListener` 那一行）其后追加：

```ts
  document.getElementById('xReplyEnabled')?.addEventListener('change', async (ev) => {
    const on = (ev.target as HTMLInputElement).checked;
    try {
      await invoke('set_x_reply_enabled', { enabled: on });
      showToast(on ? 'X 自动回复已开启（高风险）' : 'X 自动回复已关闭', on ? 'warning' : 'success');
    } catch (e) { showToast('设置失败：' + e, 'error'); }
  });
```

- [ ] **Step 3: 类型检查 + 打包**

Run: `npx tsc --noEmit 2>&1 | grep "app.ts" | head` → 无输出
Run: `npx esbuild src/tauri-frontend/app.ts --bundle --outfile=dist/tauri/scripts/app.js --format=iife --platform=browser` → Done

- [ ] **Step 4: 提交**

```bash
git add src/tauri-frontend/app.ts dist/tauri/index.html dist/tauri/scripts/app.js
git commit -m "feat(nurture): AI 设置页加 X 自动回复开关(默认关)"
```

---

## Task 6: 端到端验证（手动）

**Files:** 无

- [ ] **Step 1: 全量测试 + 编译**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -4`
Expected: 新增 4 个测试通过（quota/clean_text 2 个 + 既有）。

- [ ] **Step 2: 启动**

Run: `pkill -f "tauri dev"; pkill -f "serve dist/tauri"; sleep 1; npm run tauri:dev`（后台）
等日志出现 `Finished` + `GET /scripts/app.js Returned 200`。

- [ ] **Step 3: 人工核对（需已登录 X 的账号）**

- AI 设置页有「X 养号自动回复」勾选框，默认未勾；勾上 toast 提示高风险；刷新后状态保留。
- 勾上后跑该 X 账号养号：日志/进度出现 `💬 回复 N/M`；X 上该推文下出现一条**切题、同语言**的回复。
- 同一条推文再次养号不重复回复（`x_actions_log` 有 `<url>#reply`）。
- 未配置大模型 key 时：进度提示「未配置 AI 或回复不合格，跳过回复」，不发。
- 关掉开关后养号不再回复。

- [ ] **Step 4: 收尾（如有微调）提交**

```bash
git add -A && git commit -m "test: X 自动回复端到端验证微调" && git push
```

---

## 自检对照（spec 覆盖）

- 集成进 x_nurture_run、直接发 → Task 4 ✅
- 读正文(`article [data-testid="tweetText"]` + 长度门) → Task 1(纯) + Task 3(blocking) ✅
- 大模型基于正文生成(gen_nurture_text x_reply) → Task 4 ✅
- 配额 预热1/成长1/成熟2 → Task 1 ✅
- 开关默认关 → Task 2 + Task 5 ✅
- 去重(#reply)/限速(30~90s)/停止开关/生成不合格跳过 → Task 4 ✅
- 测试 → Task 1 单测 + Task 6 手动 ✅
- 非目标(不带产品/不审核队列/不回时间线) → 设计即不实现 ✅
