//! 多账号隔离域：自带 Mihomo 内核 + persona/节点管理。
//! 设计见 docs/multi-account-architecture.md。自带独立 mihomo（API 19090 / listeners 30000+），
//! 与用户的 Clash Verge 完全隔离。每 persona = 1 真实Gmail = 1 Unzoo profile = 1 机场节点(本地 listener 端口) = 1 指纹。

use serde::Serialize;
use tauri::{AppHandle, State, Manager};
use rusqlite::{Connection, params};
use uuid::Uuid;
use std::path::PathBuf;

use crate::{AppState, get_http_client, get_blocking_client, engine_cfg_get, UNZOO_API_BASE};
use crate::metrics;

pub(crate) const MIHOMO_API_PORT: u16 = 19090;
const MIHOMO_SECRET: &str = "unmarket-local-mihomo";
const MIHOMO_LISTENER_BASE: u16 = 30000;

static MIHOMO_CHILD: std::sync::OnceLock<std::sync::Mutex<Option<std::process::Child>>> = std::sync::OnceLock::new();

fn mihomo_home_dir() -> PathBuf {
    let d = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("unmarket").join("mihomo");
    std::fs::create_dir_all(&d).ok();
    d
}
fn mihomo_config_path() -> PathBuf { mihomo_home_dir().join("config.yaml") }
pub(crate) fn mihomo_sub_path() -> PathBuf { mihomo_home_dir().join("subscription.yaml") }

/// 内置 mihomo 二进制文件名（Windows 用 .exe，其它系统用无扩展名）。
#[cfg(windows)]
const MIHOMO_BIN: &str = "mihomo.exe";
#[cfg(not(windows))]
const MIHOMO_BIN: &str = "mihomo";

/// 解析内置 mihomo 路径（资源目录优先，回退到 exe 同级）。按系统选对应二进制。
fn mihomo_exe_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(dir) = app.path().resource_dir() {
        for cand in [dir.join("resources").join(MIHOMO_BIN), dir.join(MIHOMO_BIN)] {
            if cand.exists() { return Ok(cand); }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for cand in [dir.join("resources").join(MIHOMO_BIN), dir.join(MIHOMO_BIN)] {
                if cand.exists() { return Ok(cand); }
            }
        }
    }
    Err(format!("找不到内置 {}（请重新安装）", MIHOMO_BIN))
}

/// 判断是不是机场塞进 proxies 里的"信息展示项"（剩余流量/套餐到期/官网导航等），这些不是真节点。
pub(crate) fn is_junk_node_name(name: &str, typ: &str) -> bool {
    // 只接受真实代理协议；info 项有时也写成 vless，所以还要看名字
    const REAL_TYPES: &[&str] = &["ss","ssr","vmess","vless","trojan","hysteria","hysteria2","hy2","tuic","wireguard","wg","snell","anytls","mieru","socks5","http"];
    if !REAL_TYPES.contains(&typ.to_ascii_lowercase().as_str()) { return true; }
    let junk = ["剩余","套餐","到期","流量","重置","距离","导航","官网","网址","订阅","过期","续费","客服",
        "公告","通知","更新","邮箱","群","频道","telegram","whatsapp","expire","reset","traffic","http://","https://",".com",".net",".org","：",":GB"];
    junk.iter().any(|k| name.contains(k))
}

/// 节点名 → 粗略地区标签（用于 UI 展示）。
pub(crate) fn node_region(name: &str) -> String {
    let pairs = [("香港","🇭🇰 香港"),("HK","🇭🇰 香港"),("台湾","🇹🇼 台湾"),("台","🇹🇼 台湾"),("TW","🇹🇼 台湾"),
        ("日本","🇯🇵 日本"),("JP","🇯🇵 日本"),("新加坡","🇸🇬 新加坡"),("狮城","🇸🇬 新加坡"),("SG","🇸🇬 新加坡"),
        ("美国","🇺🇸 美国"),("US","🇺🇸 美国"),("韩国","🇰🇷 韩国"),("KR","🇰🇷 韩国"),("英国","🇬🇧 英国"),("UK","🇬🇧 英国"),
        ("德国","🇩🇪 德国"),("DE","🇩🇪 德国"),("土耳其","🇹🇷 土耳其"),("阿根廷","🇦🇷 阿根廷"),("马来","🇲🇾 马来"),
        ("越南","🇻🇳 越南"),("印度","🇮🇳 印度"),("法国","🇫🇷 法国"),("荷兰","🇳🇱 荷兰"),("俄","🇷🇺 俄罗斯")];
    for (kw, label) in pairs { if name.contains(kw) { return label.to_string(); } }
    "🌐 其它".to_string()
}

/// 用订阅里的 proxies + 各 persona 的 listener 生成 mihomo 配置。personas: (node_name, local_port)。
fn build_mihomo_config(personas: &[(String, u16)]) -> Result<String, String> {
    use serde_yaml::{Value, Mapping};
    let mut root = Mapping::new();
    root.insert(Value::from("mixed-port"), Value::from(0));
    root.insert(Value::from("allow-lan"), Value::from(false));
    root.insert(Value::from("mode"), Value::from("rule"));
    root.insert(Value::from("log-level"), Value::from("warning"));
    root.insert(Value::from("external-controller"), Value::from(format!("127.0.0.1:{}", MIHOMO_API_PORT)));
    root.insert(Value::from("secret"), Value::from(MIHOMO_SECRET));

    // proxies + 关键顶层字段 来自缓存的订阅
    let mut proxies = Value::Sequence(vec![]);
    if mihomo_sub_path().exists() {
        if let Ok(txt) = std::fs::read_to_string(mihomo_sub_path()) {
            if let Ok(sub) = serde_yaml::from_str::<Value>(&txt) {
                if let Some(p) = sub.get("proxies").cloned() { proxies = p; }
                if let Some(f) = sub.get("global-client-fingerprint").cloned() {
                    root.insert(Value::from("global-client-fingerprint"), f);
                }
            }
        }
    }
    root.insert(Value::from("proxies"), proxies);

    let mut listeners = vec![];
    for (i, (node, port)) in personas.iter().enumerate() {
        let mut m = Mapping::new();
        m.insert(Value::from("name"), Value::from(format!("persona-{}", i)));
        m.insert(Value::from("type"), Value::from("socks"));
        m.insert(Value::from("listen"), Value::from("127.0.0.1"));
        m.insert(Value::from("port"), Value::from(*port as u64));
        m.insert(Value::from("proxy"), Value::from(node.clone()));
        listeners.push(Value::Mapping(m));
    }
    root.insert(Value::from("listeners"), Value::Sequence(listeners));
    root.insert(Value::from("rules"), Value::Sequence(vec![Value::from("MATCH,DIRECT")]));
    serde_yaml::to_string(&Value::Mapping(root)).map_err(|e| e.to_string())
}

/// 从 DB 里的 personas 重新生成 mihomo 配置文件。
pub(crate) fn regenerate_mihomo_config(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn.prepare(
        "SELECT node_name, local_port FROM personas WHERE node_name IS NOT NULL AND node_name<>'' AND local_port IS NOT NULL ORDER BY local_port"
    ).map_err(|e| e.to_string())?;
    let rows: Vec<(String, u16)> = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u16)))
        .map_err(|e| e.to_string())?.flatten().collect();
    let cfg = build_mihomo_config(&rows)?;
    std::fs::write(mihomo_config_path(), cfg).map_err(|e| e.to_string())?;
    Ok(())
}

async fn mihomo_api_up() -> bool {
    let client = get_http_client();
    client.get(format!("http://127.0.0.1:{}/version", MIHOMO_API_PORT))
        .header("Authorization", format!("Bearer {}", MIHOMO_SECRET))
        .send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

pub(crate) async fn mihomo_reload() -> Result<(), String> {
    let client = get_http_client();
    let path = mihomo_config_path().to_string_lossy().to_string();
    let resp = client.put(format!("http://127.0.0.1:{}/configs?force=true", MIHOMO_API_PORT))
        .header("Authorization", format!("Bearer {}", MIHOMO_SECRET))
        .json(&serde_json::json!({"path": path}))
        .send().await.map_err(|e| format!("mihomo 热重载失败: {}", e))?;
    if resp.status().is_success() { Ok(()) } else { Err(format!("mihomo 热重载 HTTP {}", resp.status())) }
}

fn mihomo_spawn(app: &AppHandle) -> Result<(), String> {
    let exe = mihomo_exe_path(app)?;
    // Unix：确保二进制可执行（打包/拷贝可能丢失 +x，导致 spawn 报 Permission denied）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&exe) {
            let mut perm = meta.permissions();
            if perm.mode() & 0o111 == 0 {
                perm.set_mode(perm.mode() | 0o755);
                let _ = std::fs::set_permissions(&exe, perm);
            }
        }
    }
    if !mihomo_config_path().exists() {
        let cfg = build_mihomo_config(&[]).unwrap_or_default();
        std::fs::write(mihomo_config_path(), cfg).ok();
    }
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("-d").arg(mihomo_home_dir()).arg("-f").arg(mihomo_config_path());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(crate::CREATE_NO_WINDOW); }
    let child = cmd.spawn().map_err(|e| format!("启动 mihomo 失败: {}", e))?;
    if let Ok(mut g) = MIHOMO_CHILD.get_or_init(|| std::sync::Mutex::new(None)).lock() { *g = Some(child); }
    log::info!("[MIHOMO] 已启动内核：{:?}", exe);
    Ok(())
}

/// 确保自带 mihomo 在跑（已在跑则复用）。
pub(crate) async fn mihomo_ensure_running(app: &AppHandle) -> Result<(), String> {
    if mihomo_api_up().await { return Ok(()); }
    mihomo_spawn(app)?;
    for _ in 0..30 {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        if mihomo_api_up().await { return Ok(()); }
    }
    Err("mihomo 启动后 API 未就绪".into())
}

pub(crate) fn mihomo_stop() {
    if let Some(m) = MIHOMO_CHILD.get() {
        if let Ok(mut g) = m.lock() {
            if let Some(mut c) = g.take() { let _ = c.kill(); }
        }
    }
}

/// 启动时：若已配置订阅则拉起内核并按现有 personas 重建配置。
pub(crate) fn mihomo_boot(app: &AppHandle) {
    let has_sub = {
        let state = app.state::<AppState>();
        let g = state.db.lock();
        g.ok().and_then(|c| engine_cfg_get(&c, "airport_sub_url")).map(|s| !s.trim().is_empty()).unwrap_or(false)
    };
    if !has_sub { log::info!("[MIHOMO] 未配置机场订阅，跳过内核启动"); return; }
    {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        if let Ok(c) = guard { let _ = regenerate_mihomo_config(&c); }
    }
    tauri::async_runtime::block_on(async {
        if let Err(e) = mihomo_ensure_running(app).await { log::warn!("[MIHOMO] 启动失败: {}", e); return; }
        let _ = mihomo_reload().await;
        log::info!("[MIHOMO] 内核就绪");
    });
}

fn sanitize_profile_name(email: &str) -> String {
    // 按邮箱本地部分（@前）命名，folder-safe，干净易认（如 lixd220@gmail.com → lixd220）
    let local = email.split('@').next().unwrap_or(email);
    let s: String = local.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if s.is_empty() { format!("acct_{}", &Uuid::new_v4().to_string()[..6]) } else { s }
}

/// 直接调 /profiles/create，返回 (profile_id=path末段, 完整 path)。
async fn create_profile_raw(name: &str) -> Result<(String, String), String> {
    let client = get_http_client();
    // 先用干净名；失败再加唯一后缀重试——Unzoo 删过的名字会残留注册项，导致同名"failed to create"。
    let mut last_err = String::new();
    for attempt in 0..3 {
        let try_name = if attempt == 0 { name.to_string() }
            else { format!("{}_{}", name, &Uuid::new_v4().to_string()[..6]) };
        let resp = match client.post(format!("{}/profiles/create", UNZOO_API_BASE))
            .json(&serde_json::json!({"name": try_name, "group": "persona", "tags": ["unmarket-persona"]}))
            .send().await {
            Ok(r) => r, Err(e) => { last_err = format!("请求失败: {}", e); continue; }
        };
        let ok = resp.status().is_success();
        let data: serde_json::Value = resp.json().await.unwrap_or_default();
        let path = data.get("data").and_then(|d| d.get("path")).and_then(|p| p.as_str())
            .or_else(|| data.get("path").and_then(|p| p.as_str())).map(|s| s.to_string());
        if ok {
            if let Some(path) = path {
                let id = path.replace('/', "\\").rsplit('\\').next().filter(|s| !s.is_empty())
                    .unwrap_or(&try_name).to_string();
                // Unzoo 2.0.6+ 建完即设干净显示名（去掉 Profile_/um_ 噪声），不再卡成"用户N"；
                // 失败不阻塞建号流程（老版本 1.8.13 会静默无效）。
                let display = name.trim_start_matches("Profile_").trim_start_matches("um_");
                if let Err(e) = unzoo_set_profile_name(&id, display).await {
                    log::warn!("[PERSONA] 设 profile 显示名失败（旧版 Unzoo?）: {}", e);
                }
                return Ok((id, path));
            }
        }
        last_err = data.get("error").and_then(|e| e.as_str()).unwrap_or("failed to create profile").to_string();
        log::warn!("[PERSONA] 建 profile '{}' 失败({})，换名重试", try_name, last_err);
    }
    Err(format!("建 profile 失败: {}", last_err))
}

/// 设置 profile 显示名。Unzoo 2.0.6+ 实测可用：POST /profiles/update {profile_id, name}
/// （1.8.13 是设了不生效的 bug；2.0.6 修复，profile_id=path 末段文件夹名）。
async fn unzoo_set_profile_name(profile_id: &str, name: &str) -> Result<(), String> {
    let client = get_http_client();
    let resp = client.post(format!("{}/profiles/update", UNZOO_API_BASE))
        .json(&serde_json::json!({"profile_id": profile_id, "name": name}))
        .send().await.map_err(|e| format!("设置 profile 名失败: {}", e))?;
    if resp.status().is_success() { Ok(()) } else { Err(format!("设置 profile 名 HTTP {}", resp.status())) }
}

/// 设置 profile 代理（当前 Unzoo 要求 profile_path + proxy_server，实测验证）。
async fn unzoo_set_profile_proxy2(profile_path: &str, proxy_server: &str) -> Result<(), String> {
    let client = get_http_client();
    let resp = client.post(format!("{}/profiles/proxy", UNZOO_API_BASE))
        .json(&serde_json::json!({"profile_path": profile_path, "proxy_server": proxy_server}))
        .send().await.map_err(|e| format!("设置代理失败: {}", e))?;
    if !resp.status().is_success() { return Err(format!("设置代理失败: HTTP {}", resp.status())); }
    Ok(())
}

/// 由 profile_id（path 末段）解析完整 path（/profiles 列表里查）。
async fn resolve_profile_path(profile_id: &str) -> Option<String> {
    let client = get_http_client();
    let resp = client.get(format!("{}/profiles", UNZOO_API_BASE)).send().await.ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    let arr = v.get("data").and_then(|d| d.get("profiles")).or_else(|| v.get("profiles"))
        .and_then(|x| x.as_array())?.clone();
    arr.iter().find_map(|p| {
        let path = p.get("path").and_then(|x| x.as_str())?;
        let folder = path.replace('/', "\\").rsplit('\\').next().map(|s| s.to_string());
        if folder.as_deref() == Some(profile_id) { Some(path.to_string()) } else { None }
    })
}

/// 删除 profile（当前 Unzoo 要求 path 参数，实测验证）。
async fn unzoo_delete_profile_by_path(path: &str) -> Result<(), String> {
    let client = get_http_client();
    let resp = client.post(format!("{}/profiles/delete", UNZOO_API_BASE))
        .json(&serde_json::json!({"path": path})).send().await.map_err(|e| e.to_string())?;
    if resp.status().is_success() { Ok(()) } else { Err(format!("删 profile HTTP {}", resp.status())) }
}

/// 随机化指纹（当前 Unzoo 要求 profile_path，实测验证）。
async fn unzoo_randomize_fingerprint2(profile_path: &str) -> Result<(), String> {
    let client = get_http_client();
    let resp = client.post(format!("{}/profiles/fingerprint/randomize", UNZOO_API_BASE))
        .json(&serde_json::json!({"profile_path": profile_path, "components": ["gpu","canvas","audio","webgl"]}))
        .send().await.map_err(|e| format!("随机指纹失败: {}", e))?;
    if resp.status().is_success() { Ok(()) } else { Err(format!("随机指纹 HTTP {}", resp.status())) }
}

#[derive(Serialize)]
pub struct PersonaDto {
    pub id: String, pub email: String, pub profile_id: Option<String>,
    pub node_name: Option<String>, pub region: Option<String>,
    pub local_port: Option<i64>, pub status: String, pub created_at: Option<String>,
    pub account_count: i64,
    /// #13 IP 来源类型：airport(机场轮换) | fixed(固定IP)
    pub ip_mode: String,
    pub fixed_proxy: Option<String>,
    /// 身份显示名称（用户可填/改；空则前端回退显示邮箱）
    pub name: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn persona_row_to_dto(conn: &Connection, id: &str, email: &str, profile_id: Option<String>,
        node_name: Option<String>, local_port: Option<i64>, status: String, created_at: Option<String>,
        ip_mode: String, fixed_proxy: Option<String>, stored_region: Option<String>) -> PersonaDto {
    // 固定IP身份用存库的 region；机场身份从节点名推导
    let region = if ip_mode == "fixed" { stored_region } else { node_name.as_ref().map(|n| node_region(n)) };
    let account_count: i64 = conn.query_row("SELECT COUNT(*) FROM accounts WHERE persona_id=?1", params![id], |r| r.get(0)).unwrap_or(0);
    let name: Option<String> = conn.query_row("SELECT name FROM personas WHERE id=?1", params![id], |r| r.get(0)).ok().flatten();
    PersonaDto { id: id.to_string(), email: email.to_string(), profile_id, node_name, region, local_port, status, created_at, account_count, ip_mode, fixed_proxy, name }
}

#[tauri::command]
pub(crate) fn persona_list(state: State<AppState>) -> Result<Vec<PersonaDto>, String> {
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, email, profile_id, node_name, local_port, status, created_at, \
                COALESCE(ip_mode,'airport'), fixed_proxy, region FROM personas ORDER BY created_at"
    ).map_err(|e| e.to_string())?;
    let rows: Vec<(String,String,Option<String>,Option<String>,Option<i64>,String,Option<String>,String,Option<String>,Option<String>)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)))
        .map_err(|e| e.to_string())?.flatten().collect();
    Ok(rows.into_iter().map(|(id,email,pid,node,port,status,ca,ip_mode,fp,region)|
        persona_row_to_dto(&conn,&id,&email,pid,node,port,status,ca,ip_mode,fp,region)).collect())
}

/// 创建 persona：分配节点+端口 → 建 profile → 随机指纹 → 加 listener+reload → profile 绑代理。
#[tauri::command]
pub(crate) async fn persona_create(app: AppHandle, email: String, name: Option<String>) -> Result<PersonaDto, String> {
    let email = email.trim().to_string();
    if !email.contains('@') || email.len() < 5 { return Err("请输入有效的 Gmail 地址".into()); }
    let name: Option<String> = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    // 唯一性 + 分配节点/端口（锁内快进快出）
    let (id, node, port): (String, String, u16) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        let exists: bool = conn.query_row("SELECT 1 FROM personas WHERE email=?1", params![email], |_| Ok(true)).unwrap_or(false);
        if exists { return Err("这个 Gmail 已经创建过身份了".into()); }
        let node: String = conn.query_row("SELECT name FROM nodes WHERE in_use=0 ORDER BY name LIMIT 1", [], |r| r.get(0))
            .map_err(|_| "节点池没有空闲节点了。请到设置里填机场订阅、或删掉不用的身份释放节点。".to_string())?;
        conn.execute("UPDATE nodes SET in_use=1 WHERE name=?1", params![node]).map_err(|e| e.to_string())?;
        let port: i64 = conn.query_row("SELECT COALESCE(MAX(local_port), ?1)+1 FROM personas", params![MIHOMO_LISTENER_BASE as i64 - 1], |r| r.get(0)).unwrap_or(MIHOMO_LISTENER_BASE as i64);
        let id = Uuid::new_v4().to_string();
        (id, node, port as u16)
    };

    // mihomo 必须在跑（listener 才能生效）
    if let Err(e) = mihomo_ensure_running(&app).await {
        // 回滚节点占用
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        if let Ok(c) = guard { let _ = c.execute("UPDATE nodes SET in_use=0 WHERE name=?1", params![node]); }
        return Err(format!("代理内核未就绪: {}", e));
    }

    // 建 profile + 随机指纹
    let pname = sanitize_profile_name(&email);
    let (profile_id, profile_path) = match create_profile_raw(&pname).await {
        Ok(p) => p,
        Err(e) => {
            let state = app.state::<AppState>();
            let guard = state.db.lock();
            if let Ok(c) = guard { let _ = c.execute("UPDATE nodes SET in_use=0 WHERE name=?1", params![node]); }
            return Err(e);
        }
    };
    let _ = unzoo_randomize_fingerprint2(&profile_path).await;

    // 写 persona → 重建配置 → 热重载
    {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        conn.execute(
            "INSERT INTO personas (id, email, profile_id, node_name, local_port, status, created_at, name) \
             VALUES (?1,?2,?3,?4,?5,'active',datetime('now'),?6)",
            params![id, email, profile_id, node, port as i64, name]).map_err(|e| e.to_string())?;
        regenerate_mihomo_config(&conn)?;
    }
    mihomo_reload().await?;

    // profile 绑这个 persona 专属的本地 socks5 端口（= 专属节点 = 专属出口 IP）
    unzoo_set_profile_proxy2(&profile_path, &format!("socks5://127.0.0.1:{}", port)).await?;

    // 关键 UX：建好身份后，立刻打开这套浏览器并定位到 Google 登录页，引导用户把这个 Gmail 登进去
    // （这是基础登录，登一次之后，名下平台账号才能自动 Google 注册/登录）
    let pp = profile_path.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || open_profile_window(&pp, "https://accounts.google.com/signin/v2/identifier?flowName=GlifWebSignIn")).await;

    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    Ok(persona_row_to_dto(&conn, &id, &email, Some(profile_id), Some(node), Some(port as i64), "active".into(), None, "airport".into(), None, None))
}

/// 给身份改名（空=清除名称，前端回退显示邮箱）。
#[tauri::command]
pub(crate) async fn persona_rename(app: AppHandle, id: String, name: Option<String>) -> Result<(), String> {
    let name: Option<String> = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    conn.execute("UPDATE personas SET name=?1 WHERE id=?2", params![name, id]).map_err(|e| e.to_string())?;
    Ok(())
}

/// #13 创建「固定 IP 身份」：不分配机场节点、不走 mihomo，直接给独立 profile 绑用户填的固定代理。
/// label = 身份标识（手机号/任意名，唯一）；region = cn(国内) / 其他(海外)；proxy = socks5://.. 或 http://.. 或 host:port。
/// 天然不进 #11 自动轮换（node_name 为空）。一身份 = 一固定 IP（建议一身份一号，由前端约束）。
#[tauri::command]
pub(crate) async fn persona_create_fixed(app: AppHandle, label: String, region: String, proxy: String) -> Result<PersonaDto, String> {
    let label = label.trim().to_string();
    let region = region.trim().to_string();
    let mut proxy = proxy.trim().to_string();
    if label.is_empty() { return Err("请填写身份标识（名称/手机号）".into()); }
    if proxy.is_empty() { return Err("请填写固定代理地址".into()); }
    // 规范化代理：没带协议前缀的按 socks5 处理
    if !proxy.contains("://") { proxy = format!("socks5://{}", proxy); }
    if !(proxy.starts_with("socks5://") || proxy.starts_with("http://") || proxy.starts_with("https://")) {
        return Err("代理需为 socks5:// / http:// / https:// 或 host:port".into());
    }

    let id = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        let exists: bool = conn.query_row("SELECT 1 FROM personas WHERE email=?1", params![label], |_| Ok(true)).unwrap_or(false);
        if exists { return Err("这个标识已经创建过身份了".into()); }
        Uuid::new_v4().to_string()
    };

    // 建独立 profile + 随机指纹
    let pname = sanitize_profile_name(&label);
    let (profile_id, profile_path) = create_profile_raw(&pname).await?;
    // 固定身份：把 Unzoo profile 显示名设成与身份标识一致（覆盖 create_profile_raw 里的 sanitize 名）
    let _ = unzoo_set_profile_name(&profile_id, &label).await;
    let _ = unzoo_randomize_fingerprint2(&profile_path).await;

    // 直接给 profile 绑用户的固定代理（不经机场/mihomo）
    unzoo_set_profile_proxy2(&profile_path, &proxy).await?;

    {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        conn.execute(
            "INSERT INTO personas (id, email, profile_id, node_name, local_port, status, created_at, ip_mode, fixed_proxy, region) \
             VALUES (?1,?2,?3,NULL,NULL,'active',datetime('now'),'fixed',?4,?5)",
            params![id, label, profile_id, proxy, region]).map_err(|e| e.to_string())?;
    }

    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|_| "db".to_string())?;
    Ok(persona_row_to_dto(&conn, &id, &label, Some(profile_id), None, None, "active".into(), None, "fixed".into(), Some(proxy), Some(region)))
}

/// 打开某 profile 的窗口并导航到 url（用于引导用户在新身份里登录 Gmail）。阻塞，spawn_blocking 调用。
fn open_profile_window(profile_path: &str, url: &str) -> Result<(), String> {
    let client = get_blocking_client();
    let resp = client.post(format!("{}/profiles/launch", UNZOO_API_BASE))
        .json(&serde_json::json!({"profile_path": profile_path})).send()
        .map_err(|e| format!("打开 profile 失败: {}", e))?;
    let v: serde_json::Value = resp.json().unwrap_or_default();
    let tab = v.get("data").and_then(|d| d.get("tab_id"))
        .map(|t| if let Some(n)=t.as_i64(){n.to_string()} else if let Some(s)=t.as_str(){s.to_string()} else {String::new()})
        .unwrap_or_default();
    if tab.is_empty() { return Err("打开 profile 后无 tab".into()); }
    std::thread::sleep(std::time::Duration::from_millis(600));
    let _ = client.post(format!("{}/navigate", UNZOO_API_BASE))
        .json(&serde_json::json!({"tab_id": tab, "url": url})).send();
    Ok(())
}

/// 若该 profile 的浏览器【已经打开】，复用它的一个标签页导航到 url（避免重复开窗）。
/// 找不到该 profile 的已开标签则返回 Err，调用方据此回退到新开窗口逻辑。
/// 复用优先级：about:blank 空白页 > 当前激活页 > 第一个标签——尽量不覆盖用户正在用的登录页。
fn navigate_existing_profile_tab(profile_path: &str, url: &str) -> Result<(), String> {
    let client = get_blocking_client();
    let resp = client.get(format!("{}/tabs", UNZOO_API_BASE)).send()
        .map_err(|e| format!("查标签失败: {}", e))?;
    let v: serde_json::Value = resp.json().map_err(|e| format!("解析标签失败: {}", e))?;
    let tabs = v.get("data").and_then(|d| d.get("tabs")).or_else(|| v.get("tabs"))
        .and_then(|x| x.as_array()).ok_or("无标签数据")?;
    // 只看属于这个 profile 的标签（profile_path 精确匹配）
    let mine: Vec<&serde_json::Value> = tabs.iter()
        .filter(|t| t.get("profile_path").and_then(|p| p.as_str()) == Some(profile_path))
        .collect();
    if mine.is_empty() { return Err("该 profile 浏览器未打开".into()); }
    let pick = mine.iter().copied().find(|t| t.get("url").and_then(|u| u.as_str()) == Some("about:blank"))
        .or_else(|| mine.iter().copied().find(|t| t.get("active").and_then(|a| a.as_bool()) == Some(true)))
        .or_else(|| mine.iter().copied().next())
        .ok_or("无可复用标签")?;
    let tab_id = pick.get("tab_id").cloned().ok_or("标签无 id")?;
    let r = client.post(format!("{}/navigate", UNZOO_API_BASE))
        .json(&serde_json::json!({"tab_id": tab_id, "url": url})).send()
        .map_err(|e| format!("导航失败: {}", e))?;
    if r.status().is_success() { Ok(()) } else { Err(format!("导航 HTTP {}", r.status())) }
}

const GMAIL_LOGIN_URL: &str = "https://accounts.google.com/signin/v2/identifier?flowName=GlifWebSignIn";

/// 打开某个身份的浏览器到 Google 登录页（让用户补登/重登该身份的 Gmail）。
/// 优先复用该 profile 已打开的浏览器窗口；没开过才新开一套窗口。
#[tauri::command]
pub(crate) async fn persona_open_gmail_login(app: AppHandle, persona_id: String) -> Result<String, String> {
    let (email, profile_id): (String, Option<String>) = {
        let st = app.state::<AppState>();
        let guard = st.db.lock();
        let conn = guard.map_err(|e| e.to_string())?;
        conn.query_row("SELECT email, profile_id FROM personas WHERE id=?1", params![persona_id],
            |r| Ok((r.get::<_,String>(0)?, r.get::<_,Option<String>>(1)?)))
            .map_err(|_| "身份不存在".to_string())?
    };
    let pid = profile_id.ok_or("该身份没有 profile")?;
    let path = resolve_profile_path(&pid).await.ok_or("找不到该身份的 profile 路径")?;

    // ① 先尝试复用已打开的浏览器（导航现有标签）
    let reuse_path = path.clone();
    let reused = tauri::async_runtime::spawn_blocking(move || navigate_existing_profile_tab(&reuse_path, GMAIL_LOGIN_URL))
        .await.map_err(|e| e.to_string())?;
    if reused.is_ok() {
        return Ok(format!("已在 {} 已打开的浏览器里跳到 Google 登录页（复用现有窗口）", email));
    }

    // ② 没有已打开的窗口 → 走原逻辑新开一套
    tauri::async_runtime::spawn_blocking(move || open_profile_window(&path, GMAIL_LOGIN_URL))
        .await.map_err(|e| e.to_string())??;
    Ok(format!("已打开 {} 的浏览器到 Google 登录页，请在窗口里登录这个 Gmail", email))
}

/// #13 打开某身份的浏览器（通用，固定 IP 身份用：没有 Gmail 登录步骤，直接开窗让用户操作平台）。
/// 优先复用已打开的窗口；没开过才新开一套。
#[tauri::command]
pub(crate) async fn persona_open_browser(app: AppHandle, persona_id: String) -> Result<String, String> {
    let (label, profile_id): (String, Option<String>) = {
        let st = app.state::<AppState>();
        let conn = st.db.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT email, profile_id FROM personas WHERE id=?1", params![persona_id],
            |r| Ok((r.get::<_,String>(0)?, r.get::<_,Option<String>>(1)?)))
            .map_err(|_| "身份不存在".to_string())?
    };
    let pid = profile_id.ok_or("该身份没有 profile")?;
    let path = resolve_profile_path(&pid).await.ok_or("找不到该身份的 profile 路径")?;
    let reuse_path = path.clone();
    let reused = tauri::async_runtime::spawn_blocking(move || navigate_existing_profile_tab(&reuse_path, "about:blank"))
        .await.map_err(|e| e.to_string())?;
    if reused.is_ok() {
        return Ok(format!("已聚焦 {} 已打开的浏览器", label));
    }
    tauri::async_runtime::spawn_blocking(move || open_profile_window(&path, "about:blank"))
        .await.map_err(|e| e.to_string())??;
    Ok(format!("已打开 {} 的浏览器", label))
}

#[tauri::command]
pub(crate) async fn persona_delete(app: AppHandle, id: String) -> Result<(), String> {
    let (profile_id, node): (Option<String>, Option<String>) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        conn.query_row("SELECT profile_id, node_name FROM personas WHERE id=?1", params![id],
            |r| Ok((r.get::<_,Option<String>>(0)?, r.get::<_,Option<String>>(1)?)))
            .map_err(|_| "身份不存在".to_string())?
    };
    if let Some(pid) = &profile_id {
        if let Some(path) = resolve_profile_path(pid).await {
            let _ = unzoo_delete_profile_by_path(&path).await;
        }
    }
    {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        if let Some(n) = &node { let _ = conn.execute("UPDATE nodes SET in_use=0 WHERE name=?1", params![n]); }
        let _ = conn.execute("UPDATE accounts SET persona_id=NULL WHERE persona_id=?1", params![id]);
        conn.execute("DELETE FROM personas WHERE id=?1", params![id]).map_err(|e| e.to_string())?;
        let _ = regenerate_mihomo_config(&conn);
    }
    let _ = mihomo_reload().await;
    Ok(())
}

/// 测试某个 persona 的出口 IP（开它的 profile → 查 IP 服务）。
#[tauri::command]
pub(crate) async fn persona_test_ip(app: AppHandle, id: String) -> Result<String, String> {
    let profile_id: String = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|_| "db".to_string())?;
        conn.query_row("SELECT profile_id FROM personas WHERE id=?1", params![id], |r| r.get(0))
            .map_err(|_| "身份不存在或未配置 profile".to_string())?
    };
    // 找 profile 路径并启动它（独立窗口）
    let path = {
        let client = get_http_client();
        let resp = client.get(format!("{}/profiles", UNZOO_API_BASE)).send().await.map_err(|e| e.to_string())?;
        let v: serde_json::Value = resp.json().await.unwrap_or_default();
        let arr = v.get("data").and_then(|d| d.get("profiles")).or_else(|| v.get("profiles"))
            .and_then(|x| x.as_array()).cloned().unwrap_or_default();
        arr.iter().find_map(|p| {
            let path = p.get("path").and_then(|x| x.as_str())?;
            let folder = path.replace('/', "\\").rsplit('\\').next().map(|s| s.to_string());
            if folder.as_deref() == Some(profile_id.as_str()) { Some(path.to_string()) } else { None }
        }).ok_or("找不到该 profile 的路径".to_string())?
    };
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
    // 解析 ip + country
    let v: serde_json::Value = serde_json::from_str(&txt).unwrap_or(serde_json::json!({}));
    let ip = v.get("ip").and_then(|x| x.as_str()).unwrap_or("?");
    let country = v.get("country").and_then(|x| x.as_str()).unwrap_or("");
    let city = v.get("city").and_then(|x| x.as_str()).unwrap_or("");
    Ok(format!("出口 IP：{}  ({} {})", ip, country, city))
}

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
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    };
    for id in &ids {
        // 复用 persona_delete：删 profile + 解除账号关联 + 删 persona + 重建 mihomo。
        let _ = persona_delete(app.clone(), id.clone()).await;
    }
    {
        let state = app.state::<AppState>();
        let guard = state.db.lock();
        if let Ok(conn) = guard {
            crate::engine_cfg_set(&conn, "migrated_drop_fixed_personas", "1");
        }
    }
    if !ids.is_empty() {
        log::info!("[MIGRATE] 已删除 {} 个固定 IP 身份（账号转为未归属）", ids.len());
    }
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
