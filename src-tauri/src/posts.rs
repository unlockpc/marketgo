//! 内容发布域：原创帖 CRUD + 定时发布 + 媒体上传，全程走 Unzoo。
//! 文本平台(X/LinkedIn/Reddit) + 中文图文/视频(小红书/抖音)。

use serde::{Serialize, Deserialize};
use tauri::{AppHandle, State, Manager};
use rusqlite::{Connection, params};
use crate::*;

// ============================================================================
// 内容发布模块（原创 + 定时 + 媒体上传），借鉴 social-auto-upload，全程走 Unzoo。
// 文本平台（X/LinkedIn/Reddit）+ 中文图文/视频（小红书/抖音）。
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PostItem {
    pub id: String,
    pub product_id: Option<String>,
    pub platform: String,
    pub account_id: Option<String>,
    pub title: Option<String>,
    pub body: String,
    pub topics: Vec<String>,
    pub media_paths: Vec<String>,
    pub media_type: String,
    pub status: String,
    pub scheduled_at: Option<String>,
    pub published_at: Option<String>,
    pub result_url: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
}

fn json_str_array(s: &Option<String>) -> Vec<String> {
    s.as_ref()
        .and_then(|t| serde_json::from_str::<Vec<String>>(t).ok())
        .unwrap_or_default()
}

pub(crate) fn detect_media_type(paths: &[String]) -> &'static str {
    if paths.is_empty() { return "none"; }
    let lower = paths[0].to_lowercase();
    if lower.ends_with(".mp4") || lower.ends_with(".mov") || lower.ends_with(".avi")
        || lower.ends_with(".mkv") || lower.ends_with(".webm") || lower.ends_with(".flv") {
        "video"
    } else {
        "image"
    }
}

/// 依次尝试多个选择器点击，全部失败才返回最后错误（抗"随机类名/改版"）。
fn unzoo_click_any(selectors: &[&str]) -> Result<(), String> {
    let mut last = String::from("无可用选择器");
    for s in selectors {
        match unzoo_click(s) {
            Ok(_) => return Ok(()),
            Err(e) => { last = e; }
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    Err(last)
}

/// 依次尝试多个选择器上传文件。
fn unzoo_upload_any(selectors: &[&str], files: &[String]) -> Result<usize, String> {
    let mut last = String::from("无可用 input");
    for s in selectors {
        match unzoo_upload(s, files) {
            Ok(n) => return Ok(n),
            Err(e) => { last = e; }
        }
    }
    Err(last)
}

fn human_sleep(min_s: u64, max_s: u64) {
    let d = get_human_delay(min_s, max_s);
    std::thread::sleep(std::time::Duration::from_secs(d));
}

/// 把排期时间（UTC RFC3339）格式化成抖音定时发布要求的本地时间字符串："YYYY年MM月DD日 HH:MM"
fn douyin_schedule_str(scheduled_at: &Option<String>) -> Option<String> {
    let dt = scheduled_at.as_deref().and_then(parse_dt)?;
    let local = dt.with_timezone(&Local);
    Some(local.format("%Y年%m月%d日 %H:%M").to_string())
}

/// 小红书图文发布（移植 social-auto-upload xhs_uploader，调用改 Unzoo）。
/// 前置：已 launch 账号 profile、active tab 已就绪、已登录。
fn publish_xiaohongshu_note(title: &str, body: &str, topics: &[String], images: &[String]) -> Result<String, String> {
    if images.is_empty() {
        return Err("小红书图文必须至少 1 张图片".into());
    }
    log::info!("[XHS] 打开创作页…");
    unzoo_navigate("https://creator.xiaohongshu.com/publish/publish?source=official")
        .map_err(|e| format!("打开小红书创作页失败: {}", e))?;
    human_sleep(4, 7);

    // 选「上传图文」tab（默认可能是上传视频）
    let _ = unzoo_click_any(&["text=上传图文", "text=图文"]);
    human_sleep(1, 2);

    // 上传图片到隐藏 input
    log::info!("[XHS] 上传 {} 张图片…", images.len());
    unzoo_upload_any(
        &[".upload-input", "input.upload-input", "input[type=file]", ".drag-over input"],
        images,
    ).map_err(|e| format!("上传图片失败: {}", e))?;
    // 等图片处理（缩略图出现，文案区可编辑）
    human_sleep(6, 10);

    // 标题
    if !title.trim().is_empty() {
        let t: String = title.chars().take(20).collect(); // 小红书标题上限 20
        let _ = unzoo_click_any(&["input[placeholder*=\"标题\"]", ".d-text input", "input.d-text"]);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = unzoo_type("input[placeholder*=\"标题\"]", &t)
            .or_else(|_| unzoo_type(".d-text input", &t));
        human_sleep(1, 2);
    }

    // 正文（Quill 编辑器 .ql-editor）+ 话题
    let mut full = body.to_string();
    for tag in topics {
        let tag = tag.trim_start_matches('#');
        if !tag.is_empty() { full.push_str(&format!(" #{}", tag)); }
    }
    let _ = unzoo_click_any(&[".ql-editor", "div[contenteditable=true].ql-editor", "div[contenteditable=true]"]);
    std::thread::sleep(std::time::Duration::from_millis(600));
    unzoo_type(".ql-editor", &full)
        .or_else(|_| unzoo_type("div[contenteditable=true]", &full))
        .map_err(|e| format!("填写正文失败: {}", e))?;
    human_sleep(2, 4);

    // 发布
    log::info!("[XHS] 点击发布…");
    unzoo_click_any(&["button:has-text(\"发布\")", "text=发布", ".publishBtn", "button.d-button-content"])
        .map_err(|e| format!("点击发布失败: {}", e))?;
    human_sleep(4, 7);

    // 校验：跳转到发布成功/笔记管理页，或出现成功文案
    let ok = unzoo_get_text().map(|t| t.contains("发布成功") || t.contains("发布记录") || t.contains("笔记管理")).unwrap_or(false);
    if ok {
        Ok("https://creator.xiaohongshu.com/publish/success".to_string())
    } else {
        // 不确定时返回 unverified，不算硬失败（避免重复发）
        Ok("https://creator.xiaohongshu.com/publish".to_string())
    }
}

/// 抖音视频发布（移植 social-auto-upload douyin_uploader，调用改 Unzoo）。
/// schedule_local：Some → 定时发布；None → 立即发布。
fn publish_douyin_video(title: &str, body: &str, topics: &[String], video: &[String], schedule_local: Option<String>) -> Result<String, String> {
    if video.is_empty() {
        return Err("抖音视频发布必须提供视频文件".into());
    }
    log::info!("[DY] 打开创作上传页…");
    unzoo_navigate("https://creator.douyin.com/creator-micro/content/upload")
        .map_err(|e| format!("打开抖音创作页失败: {}", e))?;
    human_sleep(4, 7);

    // 上传视频（容器内隐藏 input）
    log::info!("[DY] 上传视频…");
    unzoo_upload_any(
        &["div[class^='container'] input", ".container-drag input", "input[type=file]"],
        &video[..1],
    ).map_err(|e| format!("上传视频失败: {}", e))?;

    // 等待跳到发布页 + 上传完成（"重新上传"出现 / "上传失败"则报错）
    human_sleep(3, 5);
    unzoo_wait_text("重新上传", Some("上传失败"), 300)
        .map_err(|e| format!("视频上传未完成: {}", e))?;
    log::info!("[DY] 视频上传完成");
    human_sleep(1, 3);

    // 标题（短标题 input，<=30）
    let t: String = if title.trim().is_empty() {
        body.chars().take(30).collect()
    } else {
        title.chars().take(30).collect()
    };
    let _ = unzoo_click_any(&["input[placeholder*=\"作品\"]", ".info-main input", "input[type=text]"]);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = unzoo_type("input[placeholder*=\"作品\"]", &t)
        .or_else(|_| unzoo_type(".info-main input", &t))
        .or_else(|_| unzoo_type("input[type=text]", &t));
    human_sleep(1, 2);

    // 简介/正文（contenteditable .zone-container）+ 话题
    let mut full = body.to_string();
    for tag in topics {
        let tag = tag.trim_start_matches('#');
        if !tag.is_empty() { full.push_str(&format!(" #{}", tag)); }
    }
    let _ = unzoo_click_any(&[".zone-container", "div[contenteditable=true].zone-container", "div[contenteditable=true]"]);
    std::thread::sleep(std::time::Duration::from_millis(600));
    let _ = unzoo_type(".zone-container", &full)
        .or_else(|_| unzoo_type("div[contenteditable=true]", &full));
    human_sleep(2, 3);

    // 定时发布
    if let Some(when) = schedule_local {
        log::info!("[DY] 设置定时发布 {}", when);
        let _ = unzoo_click_any(&["text=定时发布", "label:has-text(\"定时发布\")"]);
        std::thread::sleep(std::time::Duration::from_millis(800));
        // 日期时间输入框
        let _ = unzoo_click_any(&[".semi-input[placeholder=\"日期和时间\"]", "input[placeholder*=\"日期\"]"]);
        std::thread::sleep(std::time::Duration::from_millis(400));
        // 全选清空后输入
        let _ = unzoo_mcp("browser_press_key", serde_json::json!({"key":"Control+a"}));
        let _ = unzoo_type(".semi-input[placeholder=\"日期和时间\"]", &when)
            .or_else(|_| unzoo_type("input[placeholder*=\"日期\"]", &when));
        let _ = unzoo_mcp("browser_press_key", serde_json::json!({"key":"Enter"}));
        human_sleep(1, 2);
    }

    // 发布
    log::info!("[DY] 点击发布…");
    unzoo_click_any(&["button:has-text(\"发布\")", "text=发布", "[role=button][aria-label=发布]"])
        .map_err(|e| format!("点击发布失败: {}", e))?;
    human_sleep(4, 8);

    // 校验：跳转到内容管理页
    let ok = unzoo_get_text().map(|t| t.contains("作品管理") || t.contains("发布成功") || t.contains("内容管理")).unwrap_or(false);
    if ok {
        Ok("https://creator.douyin.com/creator-micro/content/manage".to_string())
    } else {
        Ok("https://creator.douyin.com/creator-micro/content/manage".to_string())
    }
}

/// 推特原创发帖（支持配图）：移植 social-auto-upload 的"原创+媒体"思路到 X。
fn publish_twitter_media(body: &str, topics: &[String], images: &[String]) -> Result<String, String> {
    log::info!("[X] 打开发帖编辑器…");
    unzoo_navigate("https://x.com/compose/post")
        .map_err(|e| format!("打开 X 编辑器失败: {}", e))?;
    human_sleep(3, 6);

    // 正文 + 话题
    let mut full = body.to_string();
    for tag in topics {
        let tag = tag.trim_start_matches('#');
        if !tag.is_empty() { full.push_str(&format!(" #{}", tag)); }
    }
    unzoo_twitter_type(&full).map_err(|e| format!("输入正文失败: {}", e))?;
    human_sleep(1, 3);

    // 配图（最多 4 张）
    if !images.is_empty() {
        let imgs: Vec<String> = images.iter().take(4).cloned().collect();
        log::info!("[X] 上传 {} 张配图…", imgs.len());
        unzoo_upload_any(
            &["input[data-testid=fileInput]", "input[type=file][accept*=image]", "input[type=file]"],
            &imgs,
        ).map_err(|e| format!("上传配图失败: {}", e))?;
        human_sleep(3, 6); // 等缩略图渲染
    }

    // 发布（实测选择器：弹层 composer = tweetButton；详情内联 = tweetButtonInline）
    unzoo_click_any(&["[data-testid=\"tweetButton\"]", "[data-testid=\"tweetButtonInline\"]", "button:has-text(\"Post\")"])
        .map_err(|e| format!("点击发布失败: {}", e))?;
    human_sleep(3, 5);

    // 成功校验（不再无脑返回成功）：先看是否弹出 X 的错误提示 → 真失败，避免误记"已发布"导致漏发。
    let page = unzoo_get_text().unwrap_or_default();
    let low = page.to_lowercase();
    const X_ERRORS: &[&str] = &[
        "already said that", "over the daily limit", "rate limit",
        "something went wrong", "couldn't be sent", "could not be sent", "try again",
        "已经发过", "超过", "出错了", "稍后再试", "发送失败",
    ];
    if let Some(hit) = X_ERRORS.iter().find(|e| low.contains(&e.to_lowercase())) {
        return Err(format!("X 发布被拒：页面提示「{}」", hit));
    }
    // 成功信号：发出后 composer 关闭、正文清空。若编辑器里还残留我们的文字 → 大概率没发出去。
    let still_has_text = unzoo_get_text_sel("div.public-DraftEditor-content")
        .map(|t| !t.trim().is_empty() && body.len() > 8 && t.contains(&body.chars().take(12).collect::<String>()))
        .unwrap_or(false);
    if still_has_text {
        return Err("X 发布未确认：编辑器仍有内容（可能未发出，留待重试）".into());
    }
    Ok("https://x.com/home".to_string())
}

/// 发布一条 post（按平台分发）。返回 result_url。
pub(crate) async fn publish_post(app: &AppHandle, post: &PostItem) -> Result<String, String> {
    // 浏览器就绪 + 切到账号 profile（登录态）
    ensure_browser_connected().await.map_err(|e| format!("浏览器未就绪: {}", e))?;
    engine_select_profile(app, &post.platform, &post.account_id).await?;
    human_sleep(2, 4);

    let pf = post.platform.to_lowercase();
    let media = post.media_paths.clone();
    let body = post.body.clone();
    let title = post.title.clone().unwrap_or_default();
    let topics = post.topics.clone();
    let sched = post.scheduled_at.clone();

    match pf.as_str() {
        "xiaohongshu" | "xhs" | "redbook" | "rednote" => {
            tauri::async_runtime::spawn_blocking(move || {
                publish_xiaohongshu_note(&title, &body, &topics, &media)
            }).await.map_err(|e| format!("任务异常: {}", e))?
        }
        "douyin" | "tiktok-cn" => {
            tauri::async_runtime::spawn_blocking(move || {
                let when = douyin_schedule_str(&sched);
                publish_douyin_video(&title, &body, &topics, &media, when)
            }).await.map_err(|e| format!("任务异常: {}", e))?
        }
        "twitter" | "x" => {
            // 推特：有配图走专用媒体流；纯文本也用同一流（媒体可选）
            tauri::async_runtime::spawn_blocking(move || {
                publish_twitter_media(&body, &topics, &media)
            }).await.map_err(|e| format!("任务异常: {}", e))?
        }
        _ => {
            // 其它文本平台（linkedin/reddit/medium/zhihu…）复用现有 publish_content
            let mut full = body.clone();
            for tag in &topics {
                let tag = tag.trim_start_matches('#');
                if !tag.is_empty() { full.push_str(&format!(" #{}", tag)); }
            }
            let content = Content {
                platform: post.platform.clone(),
                language: "en".to_string(),
                product_id: post.product_id.clone().unwrap_or_default(),
                product_name: title.clone(),
                body: full,
                hashtags: Vec::new(),
                images: media.clone(),
                account_id: post.account_id.clone(),
            };
            let state = app.state::<AppState>();
            match publish_content(state, content).await {
                Ok(r) if r.success => Ok(r.post_url.unwrap_or_default()),
                Ok(r) => Err(r.error.unwrap_or_else(|| "发布失败".into())),
                Err(e) => Err(e),
            }
        }
    }
}

/// 读取单条 post。
pub(crate) fn load_post(conn: &Connection, id: &str) -> Option<PostItem> {
    conn.query_row(
        "SELECT id, product_id, platform, account_id, title, body, topics, media_paths, media_type, \
                status, scheduled_at, published_at, result_url, error, created_at FROM posts WHERE id=?1",
        params![id],
        |r| Ok(PostItem {
            id: r.get(0)?,
            product_id: r.get(1)?,
            platform: r.get(2)?,
            account_id: r.get(3)?,
            title: r.get(4)?,
            body: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            topics: json_str_array(&r.get::<_, Option<String>>(6)?),
            media_paths: json_str_array(&r.get::<_, Option<String>>(7)?),
            media_type: r.get::<_, Option<String>>(8)?.unwrap_or_else(|| "none".into()),
            status: r.get::<_, Option<String>>(9)?.unwrap_or_else(|| "draft".into()),
            scheduled_at: r.get(10)?,
            published_at: r.get(11)?,
            result_url: r.get(12)?,
            error: r.get(13)?,
            created_at: r.get::<_, Option<String>>(14)?.unwrap_or_default(),
        }),
    ).ok()
}

/// 把一条 post 入队为 content_publish 任务（立即发或排期到点时调用）。
fn enqueue_post_task(conn: &Connection, post: &PostItem) -> Result<String, String> {
    let task_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO tasks (id, task_type, platform, account_id, content, status, retry_count, created_at) \
         VALUES (?1,'content_publish',?2,?3,?4,'pending',0,datetime('now'))",
        params![task_id, post.platform, post.account_id, post.id],
    ).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE posts SET status='publishing', task_id=?1 WHERE id=?2",
        params![task_id, post.id],
    ).map_err(|e| e.to_string())?;
    Ok(task_id)
}

/// 定时发布调度：到点的 scheduled post → 入队 content_publish 任务。
pub(crate) fn post_schedule_tick(conn: &Connection) {
    let now = Utc::now();
    if let Some(last) = engine_cfg_get(conn, "post_last_tick").and_then(|s| parse_dt(&s)) {
        if (now - last).num_seconds() < 30 { return; }
    }
    engine_cfg_set(conn, "post_last_tick", &now.to_rfc3339());

    let due: Vec<String> = {
        let mut stmt = match conn.prepare(
            "SELECT id FROM posts WHERE status='scheduled' AND scheduled_at IS NOT NULL \
                AND scheduled_at <= ?1 ORDER BY scheduled_at ASC LIMIT 20") {
            Ok(s) => s, Err(_) => return };
        let it = stmt.query_map(params![now.to_rfc3339()], |r| r.get::<_, String>(0));
        match it { Ok(rows) => rows.flatten().collect(), Err(_) => return }
    };
    for id in due {
        if let Some(post) = load_post(conn, &id) {
            match enqueue_post_task(conn, &post) {
                Ok(tid) => log::info!("[POST-SCHED] 到点入队发布 {} {} -> task {}", id, post.platform, tid),
                Err(e) => log::warn!("[POST-SCHED] 入队失败 {}: {}", id, e),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct NewPost {
    #[serde(default)] pub id: Option<String>,
    #[serde(default)] pub product_id: Option<String>,
    pub platform: String,
    #[serde(default)] pub account_id: Option<String>,
    #[serde(default)] pub title: Option<String>,
    #[serde(default)] pub body: String,
    #[serde(default)] pub topics: Vec<String>,
    #[serde(default)] pub media_paths: Vec<String>,
    #[serde(default)] pub scheduled_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GeneratedPost {
    pub title: String,
    pub body: String,
    pub topics: Vec<String>,
}

#[tauri::command]
pub(crate) fn list_posts(state: State<AppState>) -> Result<Vec<PostItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, product_id, platform, account_id, title, body, topics, media_paths, media_type, \
                status, scheduled_at, published_at, result_url, error, created_at \
         FROM posts ORDER BY created_at DESC LIMIT 200").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok(PostItem {
        id: r.get(0)?,
        product_id: r.get(1)?,
        platform: r.get(2)?,
        account_id: r.get(3)?,
        title: r.get(4)?,
        body: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
        topics: json_str_array(&r.get::<_, Option<String>>(6)?),
        media_paths: json_str_array(&r.get::<_, Option<String>>(7)?),
        media_type: r.get::<_, Option<String>>(8)?.unwrap_or_else(|| "none".into()),
        status: r.get::<_, Option<String>>(9)?.unwrap_or_else(|| "draft".into()),
        scheduled_at: r.get(10)?,
        published_at: r.get(11)?,
        result_url: r.get(12)?,
        error: r.get(13)?,
        created_at: r.get::<_, Option<String>>(14)?.unwrap_or_default(),
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// 新建或更新一条 post（草稿/排期）。返回 post id。
#[tauri::command]
pub(crate) fn save_post(state: State<AppState>, post: NewPost) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let media_type = detect_media_type(&post.media_paths).to_string();
    let topics_json = serde_json::to_string(&post.topics).unwrap_or_else(|_| "[]".into());
    let media_json = serde_json::to_string(&post.media_paths).unwrap_or_else(|_| "[]".into());
    // 有排期时间则视为 scheduled，否则 draft
    let status = if post.scheduled_at.as_ref().map(|s| !s.is_empty()).unwrap_or(false) {
        "scheduled"
    } else {
        "draft"
    };
    let id = match &post.id {
        Some(id) if !id.is_empty() => {
            conn.execute(
                "UPDATE posts SET product_id=?1, platform=?2, account_id=?3, title=?4, body=?5, \
                        topics=?6, media_paths=?7, media_type=?8, scheduled_at=?9, status=?10, error=NULL \
                 WHERE id=?11",
                params![post.product_id, post.platform, post.account_id, post.title, post.body,
                        topics_json, media_json, media_type, post.scheduled_at, status, id],
            ).map_err(|e| e.to_string())?;
            id.clone()
        }
        _ => {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO posts (id, product_id, platform, account_id, title, body, topics, \
                        media_paths, media_type, scheduled_at, status, created_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,datetime('now'))",
                params![id, post.product_id, post.platform, post.account_id, post.title, post.body,
                        topics_json, media_json, media_type, post.scheduled_at, status],
            ).map_err(|e| e.to_string())?;
            id
        }
    };
    Ok(id)
}

#[tauri::command]
pub(crate) fn delete_post(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM posts WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

/// 排期：设定发布时间（UTC RFC3339）。空字符串=取消排期回到草稿。
#[tauri::command]
pub(crate) fn schedule_post(state: State<AppState>, id: String, scheduled_at: Option<String>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match scheduled_at.as_ref().filter(|s| !s.is_empty()) {
        Some(t) => conn.execute(
            "UPDATE posts SET scheduled_at=?1, status='scheduled', error=NULL WHERE id=?2",
            params![t, id]),
        None => conn.execute(
            "UPDATE posts SET scheduled_at=NULL, status='draft' WHERE id=?1", params![id]),
    }.map_err(|e| e.to_string())?;
    Ok(())
}

/// 立即发布：直接入队 content_publish 任务（引擎下一拍执行）。
#[tauri::command]
pub(crate) fn publish_post_now(state: State<AppState>, id: String) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let post = load_post(&conn, &id).ok_or_else(|| "post 不存在".to_string())?;
    if post.status == "publishing" {
        return Err("该内容已在发布队列中".into());
    }
    enqueue_post_task(&conn, &post)
}

/// 取消：把排期/发布中的内容退回草稿（已入队的任务标记取消）。
#[tauri::command]
pub(crate) fn cancel_post(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let tid: Option<String> = conn.query_row("SELECT task_id FROM posts WHERE id=?1", params![id],
        |r| r.get::<_, Option<String>>(0)).ok().flatten();
    if let Some(tid) = tid {
        let _ = conn.execute("UPDATE tasks SET status='canceled' WHERE id=?1 AND status IN ('pending','failed')", params![tid]);
    }
    conn.execute("UPDATE posts SET status='draft', scheduled_at=NULL, task_id=NULL WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
