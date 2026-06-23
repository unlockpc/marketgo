//! 产品域：产品列表/新增/删除 + URL 抓取分析（reqwest + Unzoo 渲染 + AI 提炼卖点）。

use tauri::State;
use crate::*;
use crate::ai::{call_gemini_api, call_openai_api, call_deepseek_api, call_qwen_api};

#[tauri::command]
pub(crate) fn list_products(state: State<AppState>) -> Result<Vec<Product>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, name, url, tagline, description, product_type, priority, weight, created_at FROM products ORDER BY priority DESC")
        .map_err(|e| e.to_string())?;

    let products = stmt.query_map([], |row| {
        Ok(Product {
            id: row.get(0)?,
            name: row.get(1)?,
            url: row.get(2)?,
            tagline: row.get(3)?,
            description: row.get(4)?,
            product_type: row.get::<_, Option<String>>(5)?.unwrap_or_else(|| "tool".to_string()),
            priority: row.get(6)?,
            weight: row.get(7)?,
            created_at: row.get(8)?,
        })
    }).map_err(|e| e.to_string())?;

    products.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// 从网站抓取产品描述
pub(crate) async fn fetch_website_description(url: &str) -> Option<String> {
    // 确保 URL 有协议前缀
    let fetch_url = if !url.starts_with("http://") && !url.starts_with("https://") {
        format!("https://{}", url)
    } else {
        url.to_string()
    };

    log::info!("[FETCH] Fetching website: {}", fetch_url);

    // 使用较短的超时，避免阻塞太久
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("[FETCH] Failed to create client: {}", e);
            return None;
        }
    };

    match client.get(&fetch_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
    {
        Ok(resp) => {
            match resp.text().await {
                Ok(html) => {
                    let mut website_info = Vec::new();

                    // 提取 title
                    if let Some(start) = html.find("<title>") {
                        if let Some(end) = html[start..].find("</title>") {
                            let title = html[start + 7..start + end].trim();
                            if !title.is_empty() {
                                website_info.push(format!("Title: {}", title));
                            }
                        }
                    }

                    // 提取 meta description
                    let desc_patterns = [
                        r#"meta name="description" content=""#,
                        r#"meta property="og:description" content=""#,
                    ];
                    for pattern in desc_patterns {
                        if let Some(start) = html.find(pattern) {
                            let content_start = start + pattern.len();
                            if let Some(end) = html[content_start..].find('"') {
                                let meta_desc = html[content_start..content_start + end].trim();
                                if !meta_desc.is_empty() && meta_desc.len() > 20 {
                                    website_info.push(format!("Description: {}", meta_desc));
                                    break;
                                }
                            }
                        }
                    }

                    // 提取 h1
                    if let Some(start) = html.find("<h1") {
                        if let Some(tag_end) = html[start..].find('>') {
                            let h1_start = start + tag_end + 1;
                            if let Some(end) = html[h1_start..].find("</h1>") {
                                let h1_text = html[h1_start..h1_start + end].to_string();
                                // 清理 HTML 标签
                                let mut clean = String::new();
                                let mut in_tag = false;
                                for c in h1_text.chars() {
                                    if c == '<' { in_tag = true; }
                                    else if c == '>' { in_tag = false; }
                                    else if !in_tag { clean.push(c); }
                                }
                                let h1_clean = clean.trim().to_string();
                                if !h1_clean.is_empty() && h1_clean.len() < 200 {
                                    website_info.push(format!("Headline: {}", h1_clean));
                                }
                            }
                        }
                    }

                    // 提取页面文本摘要
                    let mut text_content = String::new();
                    let mut in_tag = false;
                    let mut in_script = false;
                    let mut in_style = false;
                    let html_lower = html.to_lowercase();

                    for (i, c) in html.chars().enumerate() {
                        if i < html_lower.len() {
                            if html_lower[i..].starts_with("<script") { in_script = true; }
                            if html_lower[i..].starts_with("</script") { in_script = false; }
                            if html_lower[i..].starts_with("<style") { in_style = true; }
                            if html_lower[i..].starts_with("</style") { in_style = false; }
                        }

                        if c == '<' { in_tag = true; }
                        else if c == '>' { in_tag = false; text_content.push(' '); }
                        else if !in_tag && !in_script && !in_style {
                            text_content.push(c);
                        }

                        if text_content.len() > 1500 { break; }
                    }

                    // 清理并取前100个词
                    let text_content: String = text_content
                        .split_whitespace()
                        .take(150)
                        .collect::<Vec<_>>()
                        .join(" ");

                    if !text_content.is_empty() {
                        website_info.push(format!("Content: {}", text_content));
                    }

                    if !website_info.is_empty() {
                        let result = website_info.join("\n");
                        log::info!("Fetched website description: {} chars", result.len());
                        Some(result)
                    } else {
                        log::warn!("No useful content extracted from {}", fetch_url);
                        None
                    }
                }
                Err(e) => {
                    log::warn!("Failed to read response body from {}: {}", fetch_url, e);
                    None
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch website {}: {}", fetch_url, e);
            None
        }
    }
}

#[tauri::command]
pub(crate) async fn create_product(
    state: State<'_, AppState>,
    name: String,
    url: String,
    tagline: Option<String>,
    description: Option<String>,
    product_type: String,
    priority: i32,
    weight: i32,
) -> Result<Product, String> {
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();

    // 如果没有提供描述，自动从网站抓取
    let final_description = if description.as_ref().map(|d| d.trim().len()).unwrap_or(0) < 50 {
        log::info!("Auto-fetching website content for new product: {}", url);
        fetch_website_description(&url).await
    } else {
        description
    };

    // 保存到数据库
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO products (id, name, url, tagline, description, product_type, priority, weight, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, name, url, tagline, final_description, product_type, priority, weight, created_at],
        ).map_err(|e| e.to_string())?;
    }

    Ok(Product {
        id,
        name,
        url,
        tagline,
        description: final_description,
        product_type,
        priority,
        weight,
        created_at,
    })
}

#[tauri::command]
pub(crate) fn delete_product(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM products WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 用 Unzoo 真实浏览器打开网页，返回渲染后的 HTML（SPA/反爬站点 reqwest 拿不到内容）。
/// 失败返回空串，由调用方回退 reqwest。
async fn fetch_rendered_html(url: &str) -> String {
    let _ = ensure_browser_connected().await;
    if get_active_tab().is_none() {
        let prof = get_saved_browser_profile();
        if !prof.is_empty() {
            if let Ok(tid) = unzoo_launch_profile(prof).await { set_active_tab(Some(tid)); }
        }
    }
    if get_active_tab().is_none() { return String::new(); }
    let u = url.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        if unzoo_navigate(&u).is_err() { return String::new(); }
        std::thread::sleep(std::time::Duration::from_secs(4));
        let raw = unzoo_evaluate("document.documentElement.outerHTML.slice(0,200000)").unwrap_or_default();
        serde_json::from_str::<String>(&raw).unwrap_or(raw)
    }).await.unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn analyze_url(state: State<'_, AppState>, url: String) -> Result<AnalyzeResult, String> {
    // 获取 AI 配置用于分析
    let (provider, api_key) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let get_value = |key: &str| -> Option<String> {
            conn.query_row("SELECT value FROM config WHERE key = ?1", params![key], |row| row.get(0)).ok()
        };
        let provider = get_value("ai.provider").unwrap_or_else(|| "gemini".to_string());
        let key = match provider.as_str() {
            "gemini" => get_value("ai.key.gemini"),
            "openai" => get_value("ai.key.openai"),
            "deepseek" => get_value("ai.key.deepseek"),
            "qwen" => get_value("ai.key.qwen"),
            _ => get_value("ai.key.gemini"),
        };
        (provider, key.unwrap_or_default())
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| e.to_string())?;

    // 优先用 Unzoo 真实浏览器拿"渲染后"的 HTML（reqwest 对 SPA/反爬常拿到空白页）
    log::info!("[ANALYZE] Fetching via Unzoo: {}", url);
    let mut html = fetch_rendered_html(&url).await;
    if html.trim().len() < 200 {
        log::info!("[ANALYZE] Unzoo content thin, falling back to reqwest");
        html = match client.get(&url).send().await {
            Ok(resp) => resp.text().await.unwrap_or_default(),
            Err(e) => { log::warn!("[ANALYZE] reqwest fallback failed: {}", e); html }
        };
    }

    // 提取基本信息
    let mut name = String::new();
    let mut tagline = String::new();
    let mut description = String::new();

    // 提取 title
    if let Some(start) = html.find("<title>") {
        if let Some(end) = html[start..].find("</title>") {
            name = html[start + 7..start + end].trim().to_string();
            // 移除常见后缀
            for suffix in [" - ", " | ", " – ", " — "] {
                if let Some(pos) = name.find(suffix) {
                    name = name[..pos].trim().to_string();
                    break;
                }
            }
        }
    }

    // 提取 meta description
    let desc_patterns = [
        r#"meta name="description" content=""#,
        r#"meta property="og:description" content=""#,
        r#"meta name="twitter:description" content=""#,
    ];
    for pattern in desc_patterns {
        if let Some(start) = html.find(pattern) {
            let content_start = start + pattern.len();
            if let Some(end) = html[content_start..].find('"') {
                description = html[content_start..content_start + end].trim().to_string();
                if !description.is_empty() {
                    break;
                }
            }
        }
    }

    // 提取 og:title 或 h1 作为 tagline
    if let Some(start) = html.find(r#"meta property="og:title" content=""#) {
        let content_start = start + 35;
        if let Some(end) = html[content_start..].find('"') {
            tagline = html[content_start..content_start + end].trim().to_string();
        }
    }
    if tagline.is_empty() {
        if let Some(start) = html.find("<h1") {
            if let Some(tag_end) = html[start..].find('>') {
                let h1_start = start + tag_end + 1;
                if let Some(end) = html[h1_start..].find("</h1>") {
                    tagline = html[h1_start..h1_start + end]
                        .replace("<br>", " ")
                        .replace("<br/>", " ")
                        .replace("&nbsp;", " ")
                        .trim()
                        .to_string();
                    // 移除 HTML 标签
                    let mut clean_tagline = String::new();
                    let mut in_tag = false;
                    for c in tagline.chars() {
                        if c == '<' { in_tag = true; }
                        else if c == '>' { in_tag = false; }
                        else if !in_tag { clean_tagline.push(c); }
                    }
                    tagline = clean_tagline.trim().to_string();
                }
            }
        }
    }

    // 如果有 AI API，使用 AI 来分析网页内容
    if !api_key.is_empty() && !html.is_empty() {
        // 提取纯文本内容（限制长度）
        // 安全去标签：按字符流处理，不做按字节切片（避免多字节 UTF-8 panic）
        let mut text_content = String::new();
        let mut in_tag = false;
        let mut in_script = false;
        let mut in_style = false;
        let mut tagbuf = String::new();
        for c in html.chars() {
            if c == '<' {
                in_tag = true;
                tagbuf.clear();
            } else if c == '>' {
                in_tag = false;
                let tl = tagbuf.to_lowercase();
                if tl.starts_with("script") { in_script = true; }
                else if tl.starts_with("/script") { in_script = false; }
                else if tl.starts_with("style") { in_style = true; }
                else if tl.starts_with("/style") { in_style = false; }
                text_content.push(' ');
            } else if in_tag {
                if tagbuf.chars().count() < 16 { tagbuf.push(c); }
            } else if !in_script && !in_style {
                text_content.push(c);
            }
            if text_content.len() > 5000 { break; }
        }

        // 清理文本
        let text_content: String = text_content
            .split_whitespace()
            .take(800)
            .collect::<Vec<_>>()
            .join(" ");

        let prompt = format!(
            r#"Analyze this website and extract product information.

URL: {}
Page Content:
{}

Extract and return in this exact format:
NAME: [Product/Company name, 1-5 words]
TAGLINE: [Short catchy description, max 15 words]
DESCRIPTION: [What the product does and its key features, 2-3 sentences]
TYPE: [One of: tool, saas, app, service, platform, other]

Be concise and accurate. Extract real information from the content."#,
            url, text_content
        );

        let ai_result = match provider.as_str() {
            "gemini" => call_gemini_api(&client, &api_key, &prompt).await,
            "openai" => call_openai_api(&client, &api_key, &prompt).await,
            "deepseek" => call_deepseek_api(&client, &api_key, &prompt).await,
            "qwen" => call_qwen_api(&client, &api_key, &prompt).await,
            _ => Err("No AI provider".to_string()),
        };

        if let Ok(ai_response) = ai_result {
            for line in ai_response.lines() {
                if line.starts_with("NAME:") {
                    name = line.trim_start_matches("NAME:").trim().to_string();
                } else if line.starts_with("TAGLINE:") {
                    tagline = line.trim_start_matches("TAGLINE:").trim().to_string();
                } else if line.starts_with("DESCRIPTION:") {
                    description = line.trim_start_matches("DESCRIPTION:").trim().to_string();
                }
            }
        }
    }

    // 如果仍然没有名称，从 URL 提取
    if name.is_empty() {
        name = url.split('/').nth(2).unwrap_or("Unknown")
            .replace("www.", "")
            .split('.')
            .next()
            .unwrap_or("Unknown")
            .to_string();
    }

    log::info!("Analyzed URL: name={}, tagline={}, desc_len={}", name, tagline, description.len());

    Ok(AnalyzeResult {
        name: name.chars().take(100).collect(),
        url,
        tagline: if tagline.is_empty() { None } else { Some(tagline.chars().take(200).collect()) },
        description: if description.is_empty() { None } else { Some(description.chars().take(1000).collect()) },
        product_type: Some("tool".to_string()),
    })
}
