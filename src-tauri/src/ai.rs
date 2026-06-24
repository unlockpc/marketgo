//! AI provider 抽象层：四家大模型文本补全(Gemini/OpenAI/DeepSeek/Qwen) + 统一入口
//! ai_complete + 养号文案 gen_nurture_text + 图片/视频生成。
//! 内容域的 generate_content/article/post_* 仍在各自位置,经 `use crate::ai::{...}` 复用本层。

use tauri::State;
use rusqlite::Connection;
use uuid::Uuid;
use crate::*;

// ===== Block A: 四家大模型文本补全 API =====
// Gemini API
pub(crate) async fn call_gemini_api(client: &reqwest::Client, api_key: &str, prompt: &str) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key={}",
        api_key
    );

    let body = serde_json::json!({
        "contents": [{
            "parts": [{"text": prompt}]
        }]
    });

    log::info!("[AI] Calling Gemini API...");
    let start = std::time::Instant::now();

    let resp = client.post(&url)
        .timeout(std::time::Duration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log::error!("[AI] Gemini API request failed: {}", e);
            format!("Gemini API request failed: {}", e)
        })?;

    log::info!("[AI] Gemini response received in {:?}", start.elapsed());

    let json: serde_json::Value = resp.json().await
        .map_err(|e| {
            log::error!("[AI] Failed to parse Gemini response: {}", e);
            format!("Failed to parse Gemini response: {}", e)
        })?;

    let result = json.get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.get(0))
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| {
            log::error!("[AI] Failed to extract text from Gemini response: {:?}", json);
            "Failed to extract text from Gemini response".to_string()
        })?;

    log::info!("[AI] Gemini content generated: {} chars", result.len());
    Ok(result)
}

// OpenAI API
pub(crate) async fn call_openai_api(client: &reqwest::Client, api_key: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": 200
    });

    let resp = client.post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI API request failed: {}", e))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "Failed to extract text from OpenAI response".to_string())
}

// DeepSeek API (OpenAI compatible)
pub(crate) async fn call_deepseek_api(client: &reqwest::Client, api_key: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "deepseek-chat",
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": 200
    });

    let resp = client.post("https://api.deepseek.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("DeepSeek API request failed: {}", e))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse DeepSeek response: {}", e))?;

    json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "Failed to extract text from DeepSeek response".to_string())
}

// Qwen/DashScope API
pub(crate) async fn call_qwen_api(client: &reqwest::Client, api_key: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "qwen-turbo",
        "input": {
            "messages": [{"role": "user", "content": prompt}]
        }
    });

    let resp = client.post("https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Qwen API request failed: {}", e))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse Qwen response: {}", e))?;

    json.get("output")
        .and_then(|o| o.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "Failed to extract text from Qwen response".to_string())
}

// ===== Block B: 统一入口 + 养号文案 + 图片生成 =====
pub(crate) fn ai_provider_key(conn: &Connection) -> (String, Option<String>) {
    let provider = conn.query_row("SELECT value FROM config WHERE key='ai.provider'", [], |r| r.get::<_, String>(0))
        .unwrap_or_else(|_| "gemini".to_string());
    let key = conn.query_row("SELECT value FROM config WHERE key=?1",
        params![format!("ai.key.{}", provider)], |r| r.get::<_, String>(0)).ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| if provider == "gemini" { std::env::var("GEMINI_API_KEY").ok().filter(|s| !s.trim().is_empty()) } else { None });
    (provider, key)
}

/// 统一文本补全分发（复用各 provider 实现）。
pub(crate) async fn ai_complete(client: &reqwest::Client, provider: &str, key: &str, prompt: &str) -> Result<String, String> {
    match provider {
        "openai" => call_openai_api(client, key, prompt).await,
        "deepseek" => call_deepseek_api(client, key, prompt).await,
        "qwen" => call_qwen_api(client, key, prompt).await,
        _ => call_gemini_api(client, key, prompt).await,
    }
}

/// 读取当前 AI 配置（provider + 对应 key）。未配置 key 返回 None。
fn ai_reply_config(app: &AppHandle) -> Option<(String, String)> {
    let st = app.state::<AppState>();
    let conn = st.db.lock().ok()?;
    let get_value = |k: &str| -> Option<String> {
        conn.query_row("SELECT value FROM config WHERE key = ?1", params![k], |row| row.get(0)).ok()
    };
    let provider = get_value("ai.provider").unwrap_or_else(|| "gemini".to_string());
    let key = match provider.as_str() {
        "openai" => get_value("ai.key.openai"),
        "deepseek" => get_value("ai.key.deepseek"),
        "qwen" => get_value("ai.key.qwen"),
        _ => get_value("ai.key.gemini"),
    }.unwrap_or_default();
    if key.trim().is_empty() { return None; }
    Some((provider, key))
}

/// 清洗 + 校验 AI 输出：去围栏/引号，挡链接 / @提及 / 超长。不合格返回 None。
fn validate_reply(raw: &str) -> Option<String> {
    let stripped = strip_code_fence(raw.trim());
    let t = stripped.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_string();
    if t.is_empty() { return None; }
    let low = t.to_lowercase();
    if low.contains("http://") || low.contains("https://") || low.contains("www.") { return None; }
    if t.contains('@') { return None; }
    if t.chars().count() > 280 { return None; }
    Some(t)
}

/// 养号发文统一入口：读 AI 配置 → 拼 prompt → 调 AI → 校验。
/// 无 key / 调用失败 / 不合格 均返回 None（调用方据此跳过本次发文，不影响点赞等动作）。
/// kind: "x_reply"（回复推文）| "gh_comment"（Issue 评论）| "x_tweet"（原创，context 传领域/话题）。
pub(crate) async fn gen_nurture_text(app: &AppHandle, kind: &str, context: &str) -> Option<String> {
    let (provider, key) = ai_reply_config(app)?;
    let prompt = match kind {
        "x_tweet" => format!(
            "你是一个活跃在该领域的真实用户。请就以下领域/话题，写一条自然、口语化的原创短推文，建立真人感。\
             要求：与领域常用语言一致；不超过 200 字符；绝不包含链接、产品名/推广、@提及、话题标签(#)；\
             只输出推文正文，不要解释、不要加引号。\n\n领域/话题：{}",
            context),
        "gh_comment" => format!(
            "你是一个该领域的普通开发者。请针对下面这个 GitHub Issue 的内容，写一句友善、有同理心的简短评论，建立真人感。\
             要求：与原文语言一致；不超过 200 字符；绝不包含链接、产品/推广、@提及；\
             只输出评论正文，不要解释、不要加引号。\n\nIssue 内容：\n{}",
            context),
        _ => format!(
            "你是一个活跃在该领域的真实用户，正在刷推。请针对下面这条推文【具体说了什么】写一句切题的回复：\
             要么认同并补充一个相关的具体看法，要么提一个真诚的小问题，让人一眼看出你确实读懂了这条推文。\
             硬性要求：\
             1) 必须用与推文【完全相同的语言】回复——推文是英文就用英文，日文就用日文，只有推文是中文才用中文；\
             2) 紧扣推文的实际内容，绝不能是「太真实了」「说到心坎里」「深有同感」「说得太对了」这类不沾内容的空泛套话；\
             3) 不超过 200 字符；不含链接、产品/推广、@提及、话题标签(#)；\
             4) 只输出回复正文，不要解释、不要加引号。\n\n推文内容：\n{}",
            context),
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build().ok()?;
    let raw = ai_complete(&client, &provider, &key, &prompt).await.ok()?;
    validate_reply(&raw)
}

/// 在任意嵌套 JSON 中递归找第一个匹配 key 的字符串值（用于解析 Veo 多版本响应）。
fn json_find_str(v: &serde_json::Value, want: &[&str]) -> Option<String> {
    match v {
        serde_json::Value::Object(m) => {
            for (k, val) in m {
                if want.iter().any(|w| k.eq_ignore_ascii_case(w)) {
                    if let Some(s) = val.as_str() { if !s.is_empty() { return Some(s.to_string()); } }
                }
                if let Some(found) = json_find_str(val, want) { return Some(found); }
            }
            None
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| json_find_str(x, want)),
        _ => None,
    }
}

/// 去掉 ```json ... ``` 围栏，便于解析模型返回的 JSON。
pub(crate) fn strip_code_fence(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    t.trim_end_matches("```").trim().to_string()
}

// ---------- ① AI 配图 ----------

/// 文字→图：Gemini gemini-3-pro-image-preview，存本地 jpg，返回绝对路径。
/// aspect_ratio 可选：1:1 / 16:9 / 9:16 / 4:5 ...（默认随平台，未传则 1:1）
#[tauri::command]
pub(crate) async fn generate_ai_image(state: State<'_, AppState>, prompt: String, aspect_ratio: Option<String>) -> Result<String, String> {
    let key = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        gemini_key_of(&conn).ok_or_else(|| "未配置 Gemini Key（设置→AI 或环境变量 GEMINI_API_KEY）".to_string())?
    };
    if prompt.trim().is_empty() { return Err("配图描述不能为空".into()); }
    let ar = aspect_ratio.filter(|s| !s.is_empty()).unwrap_or_else(|| "1:1".into());
    let client = reqwest::Client::new();
    let url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro-image-preview:generateContent".to_string();
    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": {
            "responseModalities": ["IMAGE"],
            "imageConfig": { "aspectRatio": ar }
        }
    });
    let resp = client.post(&url)
        .header("x-goog-api-key", &key)
        .timeout(std::time::Duration::from_secs(120))
        .json(&body).send().await
        .map_err(|e| format!("配图请求失败: {}", e))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| format!("配图响应解析失败: {}", e))?;
    if let Some(msg) = json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
        return Err(format!("Gemini 配图错误: {}", msg));
    }
    let b64 = json_find_str(&json, &["data"]).ok_or_else(|| "未返回图片数据".to_string())?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim())
        .map_err(|e| format!("图片解码失败: {}", e))?;
    let fname = format!("img_{}.jpg", Uuid::new_v4());
    let path = unmarket_media_dir().join(&fname);
    std::fs::write(&path, &bytes).map_err(|e| format!("图片保存失败: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

// ===== Block C: 视频生成 =====
#[tauri::command]
pub(crate) async fn generate_ai_video(state: State<'_, AppState>, prompt: String, model: Option<String>, aspect_ratio: Option<String>) -> Result<String, String> {
    let key = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        gemini_key_of(&conn).ok_or_else(|| "未配置 Gemini Key".to_string())?
    };
    if prompt.trim().is_empty() { return Err("视频描述不能为空".into()); }
    let model = model.filter(|s| !s.is_empty()).unwrap_or_else(|| "veo-3.1-fast-generate-preview".into());
    let ar = aspect_ratio.filter(|s| !s.is_empty()).unwrap_or_else(|| "16:9".into());
    let client = reqwest::Client::new();
    // 1) 发起长任务
    let start_url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:predictLongRunning", model);
    let body = serde_json::json!({
        "instances": [{ "prompt": prompt }],
        "parameters": { "aspectRatio": ar, "sampleCount": 1 }
    });
    let resp = client.post(&start_url)
        .header("x-goog-api-key", &key)
        .timeout(std::time::Duration::from_secs(60))
        .json(&body).send().await
        .map_err(|e| format!("视频任务发起失败: {}", e))?;
    let started: serde_json::Value = resp.json().await.map_err(|e| format!("视频任务响应解析失败: {}", e))?;
    if let Some(msg) = started.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
        return Err(format!("Veo 错误: {}（可能未开通视频权限/计费）", msg));
    }
    let op_name = started.get("name").and_then(|n| n.as_str())
        .ok_or_else(|| "未返回任务名(operation)".to_string())?.to_string();
    // 2) 轮询（最多 ~5 分钟）
    let poll_url = format!("https://generativelanguage.googleapis.com/v1beta/{}", op_name);
    let mut done_json: Option<serde_json::Value> = None;
    for _ in 0..60 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let r = client.get(&poll_url).header("x-goog-api-key", &key)
            .timeout(std::time::Duration::from_secs(30)).send().await
            .map_err(|e| format!("轮询失败: {}", e))?;
        let j: serde_json::Value = r.json().await.map_err(|e| format!("轮询解析失败: {}", e))?;
        if let Some(msg) = j.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
            return Err(format!("Veo 生成错误: {}", msg));
        }
        if j.get("done").and_then(|d| d.as_bool()).unwrap_or(false) { done_json = Some(j); break; }
    }
    let done = done_json.ok_or_else(|| "视频生成超时（>5分钟）".to_string())?;
    // 3) 取视频：优先内联 bytes，否则下载 uri
    use base64::Engine;
    let bytes: Vec<u8> = if let Some(b64) = json_find_str(&done, &["videoBytes", "bytesBase64Encoded"]) {
        base64::engine::general_purpose::STANDARD.decode(b64.trim())
            .map_err(|e| format!("视频解码失败: {}", e))?
    } else if let Some(uri) = json_find_str(&done, &["uri", "fileUri", "videoUri"]) {
        let dl = client.get(&uri).header("x-goog-api-key", &key)
            .timeout(std::time::Duration::from_secs(180)).send().await
            .map_err(|e| format!("视频下载失败: {}", e))?;
        dl.bytes().await.map_err(|e| format!("视频读取失败: {}", e))?.to_vec()
    } else {
        return Err("响应中未找到视频数据".into());
    };
    let path = unmarket_media_dir().join(format!("vid_{}.mp4", Uuid::new_v4()));
    std::fs::write(&path, &bytes).map_err(|e| format!("视频保存失败: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}
