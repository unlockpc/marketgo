//! 市场提交域：把 MCP/Skill 上架到各市场。
//! 列出市场 / 列出某产品的提交状态 / AI 生成上架资料 / 提交（form 入队浏览器, 其余备料人工）/ 人工标记状态。
//! read_ai_config 等共享 helper 仍在 lib.rs,经 `use crate::*` 反向引用。

use serde::Serialize;
use tauri::State;
use rusqlite::params;
use uuid::Uuid;
use crate::*;
use crate::ai::{call_gemini_api, call_openai_api, call_deepseek_api, call_qwen_api};

// ============ 市场提交（MCP/Skill 上架） ============

#[derive(Debug, Serialize)]
pub struct Marketplace {
    id: String, name: String, kind: String, submit_method: String,
    submit_url: Option<String>, notes: Option<String>,
}

#[tauri::command]
pub(crate) fn list_marketplaces(state: State<'_, AppState>) -> Result<Vec<Marketplace>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, submit_method, submit_url, notes FROM marketplaces WHERE enabled=1 ORDER BY kind, name"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok(Marketplace {
        id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, submit_method: r.get(3)?,
        submit_url: r.get(4)?, notes: r.get(5)?,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct SubmissionRow {
    marketplace_id: String, marketplace_name: String, kind: String, submit_method: String,
    submit_url: Option<String>, notes: Option<String>,
    status: String, listing: Option<String>, result_url: Option<String>, error: Option<String>,
}

/// 列出某产品对所有市场的提交状态（左连接：未提交的也列出，status=pending）。
#[tauri::command]
pub(crate) fn list_marketplace_submissions(state: State<'_, AppState>, product_id: String) -> Result<Vec<SubmissionRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT m.id, m.name, m.kind, m.submit_method, m.submit_url, m.notes, \
                COALESCE(s.status,'pending'), s.listing, s.result_url, s.error \
         FROM marketplaces m \
         LEFT JOIN marketplace_submissions s ON s.marketplace_id=m.id AND s.product_id=?1 \
         WHERE m.enabled=1 ORDER BY m.kind, m.name"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![product_id], |r| Ok(SubmissionRow {
        marketplace_id: r.get(0)?, marketplace_name: r.get(1)?, kind: r.get(2)?, submit_method: r.get(3)?,
        submit_url: r.get(4)?, notes: r.get(5)?,
        status: r.get(6)?, listing: r.get(7)?, result_url: r.get(8)?, error: r.get(9)?,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn set_product_repo(state: State<'_, AppState>, product_id: String, repo_url: String, install_cmd: Option<String>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE products SET repo_url=?2, install_cmd=?3 WHERE id=?1",
        params![product_id, repo_url, install_cmd.unwrap_or_default()]).map_err(|e| e.to_string())?;
    Ok(())
}

/// 生成某市场所需的上架资料（AI）。模板兜底。
async fn gen_listing(provider: &str, key: &str, m_name: &str, m_method: &str, kind: &str,
    p_name: &str, p_tag: &str, p_desc: &str, p_url: &str, repo: &str, install: &str) -> String {
    let tmpl = || format!(
        "# {p} — {m} 上架资料（{k}）\n\n一句话：{p} {t}\n\n简介：{d}\n\n仓库：{r}\n主页：{u}\n安装：{i}\n\nAwesome 列表条目：\n- [{p}]({r}) - {t}\n",
        p=p_name, m=m_name, k=kind, t=p_tag, d=p_desc, r=repo, u=p_url, i=install);
    if key.is_empty() { return tmpl(); }
    // CLI 渠道（Official MCP Registry / Smithery）：直接产出可落仓库的配置文件 + 发布命令
    let prompt = if m_method == "cli" {
        format!(
            r#"You are preparing CLI-publish artifacts to list a {kind} on "{m_name}" (a CLI/registry-based channel).

Product: {p_name} — {p_tag}
Description: {p_desc}
Website: {p_url}
GitHub repo: {repo}
Install command: {install}

Output EXACTLY the config file(s) + commands this channel needs:
- If "{m_name}" is the Official MCP Registry: output a valid **server.json** in a ```json code block with: "$schema", "name" in reverse-DNS from the repo (e.g. io.github.<owner>/<repo>), "description", "version" (use 0.1.0 if unknown), "repository" {{"url","source":"github"}}, and a "packages" array inferred from the install command (npm→registryType npm, pip→pypi, etc.). Then a **Commands** section: `mcp-publisher login github` then `mcp-publisher publish`.
- If "{m_name}" is Smithery: output a valid **smithery.yaml** in a ```yaml code block (runtime/startCommand appropriate to the install), then a **Commands** section with the connect step (add the repo on smithery.ai, or `npx @smithery/cli ...`).

IMPORTANT: if the product is a desktop app / not an installable package (no npm/pypi/oci package), put a one-line WARNING at the very top that this channel requires a packaged or hostable server and may not accept it. Output only code blocks + the Commands section."#,
            kind=kind, m_name=m_name, p_name=p_name, p_tag=p_tag,
            p_desc=p_desc, p_url=p_url, repo=repo, install=install)
    } else {
        format!(
            r#"You are preparing a marketplace listing to submit a {kind} to "{m_name}" (submission method: {m_method}).

Product: {p_name} — {p_tag}
Description: {p_desc}
Website: {p_url}
GitHub repo: {repo}
Install command: {install}

Produce ready-to-paste listing materials in Markdown with these sections:
- **One-liner** (≤90 chars, punchy, no hype)
- **Short description** (2-3 sentences, what it does + who it's for)
- **Tags/Categories** (5-8, comma-separated, fit a dev tool directory)
- **Install** (the command/config snippet; for MCP include a minimal mcp.json/server.json example using the repo)
- **Awesome-list entry** (single markdown line: `- [{p_name}]({repo}) - <short desc>`)

Be accurate to the product; do not invent features. Output only the Markdown."#,
            kind=kind, m_name=m_name, m_method=m_method, p_name=p_name, p_tag=p_tag,
            p_desc=p_desc, p_url=p_url, repo=repo, install=install)
    };
    let client = reqwest::Client::new();
    let raw = match provider {
        "openai" => call_openai_api(&client, key, &prompt).await,
        "deepseek" => call_deepseek_api(&client, key, &prompt).await,
        "qwen" => call_qwen_api(&client, key, &prompt).await,
        _ => call_gemini_api(&client, key, &prompt).await,
    };
    match raw { Ok(s) if !s.trim().is_empty() => s, _ => tmpl() }
}

#[tauri::command]
pub(crate) async fn generate_marketplace_listing(state: State<'_, AppState>, product_id: String, marketplace_id: String) -> Result<String, String> {
    let (pname, ptag, pdesc, purl, repo, install, provider, key, mkind, mmethod, msub) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let p = conn.query_row(
            "SELECT name, COALESCE(tagline,''), COALESCE(description,''), COALESCE(url,''), COALESCE(repo_url,''), COALESCE(install_cmd,'') FROM products WHERE id=?1",
            params![product_id], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?, r.get::<_,String>(4)?, r.get::<_,String>(5)?))
        ).map_err(|e| format!("产品未找到: {}", e))?;
        let m = conn.query_row(
            "SELECT name, kind, submit_method, COALESCE(submit_url,'') FROM marketplaces WHERE id=?1",
            params![marketplace_id], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?))
        ).map_err(|e| format!("市场未找到: {}", e))?;
        let (provider, key) = read_ai_config(&conn);
        let _ = &m.0;
        (p.0, p.1, p.2, p.3, p.4, p.5, provider, key, m.1, m.2, m.3)
    };
    if repo.trim().is_empty() {
        return Err("该产品还没填 GitHub 仓库地址，请先在产品里设置 repo_url".into());
    }
    let kind = if mkind == "skill" { "skill" } else { "mcp" };
    let mname = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT name FROM marketplaces WHERE id=?1", params![marketplace_id], |r| r.get::<_, String>(0)).unwrap_or_default()
    };
    let listing = gen_listing(&provider, &key, &mname, &mmethod, kind, &pname, &ptag, &pdesc, &purl, &repo, &install).await;
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT INTO marketplace_submissions (id,product_id,marketplace_id,kind,status,listing,submit_url,created_at,updated_at) \
             VALUES (?1,?2,?3,?4,'materials_ready',?5,?6,datetime('now'),datetime('now')) \
             ON CONFLICT(product_id,marketplace_id,kind) DO UPDATE SET listing=?5, \
               status=CASE WHEN status IN ('submitted','listed') THEN status ELSE 'materials_ready' END, updated_at=datetime('now')",
            params![Uuid::new_v4().to_string(), product_id, marketplace_id, kind, listing, msub]);
    }
    Ok(listing)
}

/// 提交：form 类 → 入队浏览器预填任务；其余（pr/cli/auto_index）→ 备好资料，返回提交入口由人工完成。
#[tauri::command]
pub(crate) fn submit_marketplace(state: State<'_, AppState>, product_id: String, marketplace_id: String) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (kind, method, sub_url) = conn.query_row(
        "SELECT kind, submit_method, COALESCE(submit_url,'') FROM marketplaces WHERE id=?1",
        params![marketplace_id], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?))
    ).map_err(|e| format!("市场未找到: {}", e))?;
    let kind = if kind == "skill" { "skill" } else { "mcp" };
    // 必须先有生成的资料
    let has_listing: bool = conn.query_row(
        "SELECT 1 FROM marketplace_submissions WHERE product_id=?1 AND marketplace_id=?2 AND kind=?3 AND listing IS NOT NULL AND listing<>''",
        params![product_id, marketplace_id, kind], |_| Ok(true)).unwrap_or(false);
    if !has_listing {
        return Err("请先「生成资料」再提交".into());
    }
    if method == "form" {
        // 入队浏览器预填任务（platform=marketplace_id, content=product_id, target_url=表单）
        let _ = conn.execute(
            "INSERT INTO tasks (id, task_type, platform, account_id, content, target_url, status, retry_count, created_at) \
             VALUES (?1,'marketplace_submit',?2,NULL,?3,?4,'pending',0,datetime('now'))",
            params![Uuid::new_v4().to_string(), marketplace_id, product_id, sub_url]);
        let _ = conn.execute(
            "UPDATE marketplace_submissions SET status='submitting', submit_url=?4, updated_at=datetime('now') \
             WHERE product_id=?1 AND marketplace_id=?2 AND kind=?3",
            params![product_id, marketplace_id, kind, sub_url]);
        Ok(format!("已入队：引擎会打开 {} 自动填表并提交（用全局登录 profile）；完成后这里会显示 已提交/需人工 状态。", sub_url))
    } else {
        Ok(format!("{} 为 {} 方式：资料已备好，请打开 {} 按生成的资料提交。", marketplace_id, method, sub_url))
    }
}

/// 人工标记提交状态（submitted / listed / skipped / failed）。
#[tauri::command]
pub(crate) fn mark_submission(state: State<'_, AppState>, product_id: String, marketplace_id: String, kind: String, status: String, result_url: Option<String>) -> Result<(), String> {
    let st = match status.as_str() {
        "pending" | "materials_ready" | "submitted" | "listed" | "skipped" | "failed" => status.as_str(),
        _ => return Err("非法状态".into()),
    };
    let k = if kind == "skill" { "skill" } else { "mcp" };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO marketplace_submissions (id,product_id,marketplace_id,kind,status,result_url,created_at,updated_at) \
         VALUES (?1,?2,?3,?4,?5,?6,datetime('now'),datetime('now')) \
         ON CONFLICT(product_id,marketplace_id,kind) DO UPDATE SET status=?5, result_url=COALESCE(?6,result_url), updated_at=datetime('now')",
        params![Uuid::new_v4().to_string(), product_id, marketplace_id, k, st, result_url]).map_err(|e| e.to_string())?;
    Ok(())
}
