#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// 桌面 AI 宠物 · 桌面版 v0.2（Tauri v2）
// 架构遵循 SRS：模型请求与密钥管理收敛于主进程（CON-07 / FR-MDL-013），
// 宠物窗口 / 气泡窗口 / 设置窗口为三个独立窗口。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Position, Size,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

// ================= 默认状态（与 MVP 版一致的数据结构，密钥不入库） =================

const DEFAULT_STATE: &str = r##"{
  "v": 1,
  "providers": [
    {"id":"p-demo","name":"演示模式（内置，无需 Key）","type":"mock","baseUrl":"","model":"demo-pet-v1","headers":"","apiKey":"","isDefault":true,
     "params":{"temp":0.8,"topP":1,"maxTokens":1024,"ctxRounds":10,"timeout":30,"retry":2,"stream":true}},
    {"id":"p-deepseek","name":"DeepSeek","type":"openai","baseUrl":"https://api.deepseek.com/v1","model":"deepseek-chat","headers":"","apiKey":"","isDefault":false,
     "params":{"temp":0.8,"topP":1,"maxTokens":1024,"ctxRounds":10,"timeout":30,"retry":2,"stream":true}},
    {"id":"p-openai","name":"OpenAI","type":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o-mini","headers":"","apiKey":"","isDefault":false,
     "params":{"temp":0.8,"topP":1,"maxTokens":1024,"ctxRounds":10,"timeout":30,"retry":2,"stream":true}},
    {"id":"p-ollama","name":"Ollama（本地）","type":"openai","baseUrl":"http://localhost:11434/v1","model":"qwen2.5:7b","headers":"","apiKey":"","isDefault":false,
     "params":{"temp":0.8,"topP":1,"maxTokens":1024,"ctxRounds":10,"timeout":30,"retry":2,"stream":true}}
  ],
  "personas": [
    {"id":"ps-1","name":"贴心小助手","avatar":"🐱","skin":"blue","providerId":"p-demo",
     "systemPrompt":"你是桌面宠物「团子」，性格温柔贴心，说话简短可爱，回复控制在 120 字内。用户的消息可能包含工作问题或日常闲聊，请友好回应。",
     "greeting":"你好呀～我是团子 🐱 直接在下面输入框和我聊天吧。也可以试试：对我说「记住：我喜欢蓝色」，或者「25 分钟后提醒我喝水」。",
     "quickActions":[
       {"label":"今天做点什么","prompt":"给我 3 条今天可以做的轻松小事"},
       {"label":"写句鼓励","prompt":"给我一句简短的今日鼓励"},
       {"label":"提醒我喝水","prompt":"25 分钟后提醒我喝水"}]},
    {"id":"ps-2","name":"毒舌吐槽役","avatar":"😈","skin":"orange","providerId":"p-demo",
     "systemPrompt":"你是桌面宠物「辣辣」，毒舌但内心关心用户，喜欢吐槽用户的拖延和熬夜，嘴硬心软，回复不超过 80 字。",
     "greeting":"又来了？行吧，有话快说，我忙着睡觉呢 😈",
     "quickActions":[
       {"label":"吐槽我一下","prompt":"随便吐槽我两句"},
       {"label":"骂醒我","prompt":"我不想干活了，骂醒我"},
       {"label":"夸夸我","prompt":"虽然你嘴毒，但今天请认真夸夸我"}]},
    {"id":"ps-3","name":"专注工作搭子","avatar":"🧢","skin":"mint","providerId":"p-demo",
     "systemPrompt":"你是桌面宠物「工友」，专业简洁的工作助手，回答结构化、要点化，不寒暄不卖萌。",
     "greeting":"工友就位。直接说需求：写大纲 / 翻译 / 总结 / 定时提醒都可以。",
     "quickActions":[
       {"label":"番茄钟","prompt":"25 分钟后提醒我休息，进入专注"},
       {"label":"总结要点","prompt":"我会发送一段文字，请总结为 3 个要点"},
       {"label":"待办排序","prompt":"帮我把今天的待办按优先级排一下（我发给你）"}]}
  ],
  "sessions": [],
  "currentSessionId": null,
  "memories": [],
  "reminders": [],
  "settings": {
    "petScale": 1, "skin": "blue", "petPos": null, "clickThrough": false, "lowFps": true,
    "bubbleW": 340, "bubbleIdleClose": 60, "enterSend": true, "newSessionPerClick": false,
    "chatter": "off", "dnd": false, "lastPersonaId": "ps-1", "lastProviderId": null
  },
  "stats": {"requests": 0, "tokensIn": 0, "tokensOut": 0}
}"##;

// ================= 应用上下文 =================

struct AppCtx {
    data_path: std::path::PathBuf,
    state: Mutex<Value>,
    stop: AtomicBool,
    bubble_anchor_below: Mutex<bool>,
    fg_before: Mutex<usize>,
    pet_visible: Mutex<bool>,
    pending: Mutex<Vec<Value>>,
    dnd_queue: Mutex<Vec<Value>>,
    http: reqwest::Client,
}

fn load_or_default(path: &std::path::PathBuf) -> Value {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| default_state()),
        Err(_) => default_state(),
    }
}

fn default_state() -> Value {
    serde_json::from_str(DEFAULT_STATE).expect("默认状态 JSON 合法")
}

// 写盘专用：绝不接触 state 锁。
// 设计约束：v0.2.3 及之前本函数内部会 state.lock()，而 tick_reminders 在持锁状态下调用它，
// std::sync::Mutex 同线程重入自死锁 → state 锁被调度器线程永久持有 →
// 主线程执行 state_load 等命令时永久阻塞 → 整机冻结（点击设置按钮触发暴露）。
// 现在改为接收序列化好的快照，从 API 层面杜绝此类死锁。
fn write_state_file(ctx: &AppCtx, snapshot: &str) {
    let _ = std::fs::write(&ctx.data_path, snapshot);
}

// 快照并落盘：短临界区取锁序列化，写盘在锁外。调用方不得已持有 state 锁；
// 必须持锁写盘的场景（如 tick_reminders）请在锁内自行序列化，锁释放后调用 write_state_file。
fn persist_state(ctx: &AppCtx) {
    let s = {
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        serde_json::to_string_pretty(&*st).ok()
    };
    if let Some(s) = s {
        write_state_file(ctx, &s);
    }
}

fn app_log(app: &AppHandle, msg: &str) {
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let line = format!("[{}] {}\n", chrono_now(), msg);
        let p = dir.join("deskpet.log");
        if let Ok(mut len) = std::fs::metadata(&p).map(|m| m.len()) {
            if len > 200_000 {
                let _ = std::fs::remove_file(&p);
                len = 0;
            }
            let _ = len;
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 简易 UTC+8 时间戳（仅日志用）
    let secs = now + 8 * 3600;
    let d = secs / 86400;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("D{} {:02}:{:02}:{:02}", d, h, mi, s)
}

// ================= Win32 工具 =================

#[allow(non_snake_case)]
fn work_area_at(x: i32, y: i32) -> RECT {
    unsafe {
        let pt = POINT { x, y };
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut info) != 0 {
            info.rcWork
        } else {
            RECT { left: 0, top: 0, right: 1920, bottom: 1040 }
        }
    }
}

fn save_foreground(app: &AppHandle) {
    let hwnd = unsafe { GetForegroundWindow() };
    let ctx = app.state::<AppCtx>();
    *ctx.fg_before.lock().unwrap() = hwnd as usize;
}

fn restore_foreground(app: &AppHandle) {
    let ctx = app.state::<AppCtx>();
    let h = *ctx.fg_before.lock().unwrap();
    if h != 0 {
        unsafe { SetForegroundWindow(h as _); }
    }
    *ctx.fg_before.lock().unwrap() = 0;
}

// ================= 窗口管理 =================

fn sf(w: &WebviewWindow) -> f64 {
    w.scale_factor().unwrap_or(1.0)
}

fn show_bubble(app: &AppHandle) -> Result<(), String> {
    show_bubble_reason(app, "click")
}

fn show_bubble_reason(app: &AppHandle, reason: &str) -> Result<(), String> {
    let bubble = app.get_webview_window("bubble").ok_or("bubble 窗口不存在")?;
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    let p_pos = pet.outer_position().map_err(|e| e.to_string())?;
    let p_size = pet.outer_size().map_err(|e| e.to_string())?;
    let b_size = bubble.outer_size().map_err(|e| e.to_string())?;
    let scale = sf(&pet);
    let gap = (14.0 * scale).round() as i32;
    let center_x = p_pos.x + p_size.width as i32 / 2;
    let pet_top = p_pos.y;
    let pet_bottom = p_pos.y + p_size.height as i32;
    let b_w = b_size.width as i32;
    let b_h = b_size.height as i32;
    let work = work_area_at(center_x, pet_top + p_size.height as i32 / 2);
    let mut below = false;
    let mut y = pet_top - gap - b_h;
    if y < work.top {
        below = true;
        y = pet_bottom + gap;
    }
    if y + b_h > work.bottom {
        y = (work.bottom - b_h).max(work.top);
    }
    let mut x = center_x - b_w / 2;
    x = x.clamp(work.left, (work.right - b_w).max(work.left));
    {
        let ctx = app.state::<AppCtx>();
        *ctx.bubble_anchor_below.lock().unwrap() = below;
    }
    bubble
        .set_position(Position::Physical(PhysicalPosition::new(x, y)))
        .map_err(|e| e.to_string())?;
    save_foreground(app);
    bubble.show().map_err(|e| e.to_string())?;
    bubble.set_focus().map_err(|e| e.to_string())?;
    let _ = app.emit_to("bubble", "bubble-shown", json!({"below": below, "reason": reason}));
    Ok(())
}

fn hide_bubble(app: &AppHandle) -> Result<(), String> {
    let bubble = app.get_webview_window("bubble").ok_or("bubble 窗口不存在")?;
    bubble.hide().map_err(|e| e.to_string())?;
    restore_foreground(app);
    let _ = app.emit_to("pet", "chat-state", json!({"state": "idle"}));
    Ok(())
}

fn bubble_visible(app: &AppHandle) -> bool {
    app.get_webview_window("bubble")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

fn open_settings(app: &AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        // 预创建窗口直接显示（内容已加载，避免懒创建 WebView2 冷启动白屏）
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
        let _ = app.emit_to("settings", "settings-shown", ());
        Ok(())
    } else {
        // 兼容路径：窗口意外被销毁时重建
        WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
            .title("设置 — 桌面 AI 宠物")
            .inner_size(780.0, 560.0)
            .min_inner_size(600.0, 440.0)
            .decorations(false)
            .center()
            .build()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn pet_default_pos(app: &AppHandle) -> (i32, i32) {
    let scale = app
        .get_webview_window("pet")
        .map(|w| sf(&w))
        .unwrap_or(1.0);
    let w = (120.0 * scale) as i32;
    let h = (112.0 * scale) as i32;
    let work = work_area_at(1600, 900);
    (work.right - w - 180, work.bottom - h - 24)
}

fn position_pet(app: &AppHandle) {
    let pet = match app.get_webview_window("pet") {
        Some(p) => p,
        None => return,
    };
    let (saved, scale) = {
        let ctx = app.state::<AppCtx>();
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        (
            st["settings"]["petPos"].as_object().map(|p| {
                (p.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                 p.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
            }),
            st["settings"]["petScale"].as_f64().unwrap_or(1.0),
        )
    };
    let _ = pet.set_size(Size::Logical(LogicalSize::new(120.0 * scale, 112.0 * scale)));
    let (x, y) = saved.unwrap_or_else(|| pet_default_pos(app));
    // 越界回收（FR-PET-005）
    let work = work_area_at(x + 60, y + 60);
    let x = x.clamp(work.left, (work.right - 60).max(work.left));
    let y = y.clamp(work.top, (work.bottom - 60).max(work.top));
    let _ = pet.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

fn rebuild_tray(app: &AppHandle) {
    let (dnd, ct, pet_visible) = {
        let ctx = app.state::<AppCtx>();
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        let dnd = st["settings"]["dnd"].as_bool().unwrap_or(false);
        let ct = st["settings"]["clickThrough"].as_bool().unwrap_or(false);
        let pv = *ctx.pet_visible.lock().unwrap();
        (dnd, ct, pv)
    };
    let autostart = use_autostart(app, |m| m.is_enabled().unwrap_or(false));
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open_chat", "打开对话（Ctrl+Shift+Space）", true, None::<&str>).unwrap(),
            &MenuItem::with_id(app, "new_session", "新会话", true, None::<&str>).unwrap(),
            &PredefinedMenuItem::separator(app).unwrap(),
            &MenuItem::with_id(
                app,
                "toggle_pet",
                if pet_visible { "隐藏宠物" } else { "显示宠物" },
                true,
                None::<&str>,
            )
            .unwrap(),
            &MenuItem::with_id(app, "settings", "设置（Ctrl+,）", true, None::<&str>).unwrap(),
            &PredefinedMenuItem::separator(app).unwrap(),
            &MenuItem::with_id(
                app,
                "dnd",
                if dnd { "✓ 免打扰（睡眠）" } else { "免打扰（睡眠）" },
                true,
                None::<&str>,
            )
            .unwrap(),
            &MenuItem::with_id(
                app,
                "ct",
                if ct { "✓ 点击穿透" } else { "点击穿透" },
                true,
                None::<&str>,
            )
            .unwrap(),
            &MenuItem::with_id(
                app,
                "autostart",
                if autostart { "✓ 开机自启" } else { "开机自启" },
                true,
                None::<&str>,
            )
            .unwrap(),
            &PredefinedMenuItem::separator(app).unwrap(),
            &MenuItem::with_id(app, "reset_pos", "复位宠物位置", true, None::<&str>).unwrap(),
            &MenuItem::with_id(app, "reset_data", "重置全部数据…", true, None::<&str>).unwrap(),
            &PredefinedMenuItem::separator(app).unwrap(),
            &MenuItem::with_id(app, "quit", "退出", true, None::<&str>).unwrap(),
        ],
    )
    .unwrap();
    if let Some(tray) = app.tray_by_id("deskpet-tray") {
        let _ = tray.set_menu(Some(menu));
    }
}

fn use_autostart<T>(app: &AppHandle, f: impl FnOnce(&tauri_plugin_autostart::AutoLaunchManager) -> T) -> T {
    use tauri_plugin_autostart::ManagerExt;
    f(&app.autolaunch())
}

// ================= 密钥安全存储（Windows 凭据管理器） =================

const KEY_SERVICE: &str = "DeskPet";

fn key_get(id: &str) -> Option<String> {
    keyring::Entry::new(KEY_SERVICE, id)
        .ok()
        .and_then(|e| e.get_password().ok())
}

fn key_set_kv(id: &str, key: &str) -> Result<(), String> {
    keyring::Entry::new(KEY_SERVICE, id)
        .map_err(|e| e.to_string())?
        .set_password(key)
        .map_err(|e| e.to_string())
}

fn key_del(id: &str) {
    if let Ok(e) = keyring::Entry::new(KEY_SERVICE, id) {
        let _ = e.delete_credential();
    }
}

// ================= 提醒调度（主进程线程） =================

fn start_scheduler(app: AppHandle) {
    std::thread::spawn(move || {
        let mut chatter_at = std::time::Instant::now() + Duration::from_secs(120);
        loop {
            std::thread::sleep(Duration::from_secs(1));
            tick_reminders(&app);
            tick_chatter(&app, &mut chatter_at);
        }
    });
}

fn tick_reminders(app: &AppHandle) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let mut fired: Vec<Value> = vec![];
    let snapshot = {
        let ctx = app.state::<AppCtx>();
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(rems) = st["reminders"].as_array_mut() {
            let mut keep: Vec<Value> = vec![];
            for r in rems.iter() {
                let enabled = r["enabled"].as_bool().unwrap_or(true);
                let due = r["due"].as_i64().unwrap_or(0);
                if !enabled || due > now {
                    keep.push(r.clone());
                } else if r["rule"].as_str() == Some("daily") {
                    let mut nr = r.clone();
                    let mut nd = due;
                    while nd <= now {
                        nd += 86_400_000;
                    }
                    nr["due"] = json!(nd);
                    keep.push(nr);
                } else {
                    fired.push(r.clone());
                }
            }
            if fired.is_empty() {
                return;
            }
            *rems = keep;
        }
        // 持锁期间仅做序列化（快照），写盘必须在锁释放后进行（见函数顶部注释：曾因持锁调旧版 write_state_file 自死锁）
        serde_json::to_string_pretty(&*st).ok()
    };
    if let Some(s) = snapshot {
        let ctx = app.state::<AppCtx>();
        write_state_file(&ctx, &s);
    }
    let dnd = {
        let ctx = app.state::<AppCtx>();
        let g = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        g["settings"]["dnd"].as_bool().unwrap_or(false)
    };
    if dnd {
        let ctx = app.state::<AppCtx>();
        ctx.dnd_queue.lock().unwrap().extend(fired);
        let _ = app.emit("state-changed", json!({"origin": "backend"}));
        return;
    }
    for r in fired {
        push_notice(app, json!({"kind": "reminder", "reminder": r}));
        let _ = app.emit_to("pet", "reminder-fx", json!({"kind": "happy"}));
    }
    let _ = show_bubble_reason(app, "reminder");
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
}

fn push_notice(app: &AppHandle, notice: Value) {
    let ctx = app.state::<AppCtx>();
    ctx.pending.lock().unwrap().push(notice);
}

fn tick_chatter(app: &AppHandle, chatter_at: &mut std::time::Instant) {
    if std::time::Instant::now() < *chatter_at {
        return;
    }
    let interval = {
        let ctx = app.state::<AppCtx>();
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        if st["settings"]["dnd"].as_bool().unwrap_or(true) {
            return;
        }
        match st["settings"]["chatter"].as_str() {
            Some("low") => 600,
            Some("mid") => 300,
            Some("high") => 150,
            _ => return,
        }
    };
    *chatter_at = std::time::Instant::now() + Duration::from_secs(interval as u64);
    if bubble_visible(app) {
        return;
    }
    let nano = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    if nano % 100 < 35 {
        return; // 35% 概率跳过，避免打扰感过强
    }
    let lines = [
        "要不要喝口水休息一下？",
        "盯屏幕太久啦，眨眨眼睛吧～",
        "起来活动活动？我帮你看着桌面。",
        "有点无聊…点我聊聊天嘛。",
        "记得抬头看看远处，放松一下眼睛哦。",
    ];
    let text = lines[(nano as usize) % lines.len()];
    let _ = app.emit_to("pet", "chatter", json!({"text": text}));
    let _ = app.emit_to("pet", "reminder-fx", json!({"kind": "happy"}));
}

// ================= 聊天流式（主进程，FR-MDL-013） =================

#[derive(serde::Deserialize, Clone)]
struct ChatMsg {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct ChatArgs {
    #[serde(rename = "providerId")]
    provider_id: String,
    messages: Vec<ChatMsg>,
}

struct ProvConf {
    ptype: String,
    base: String,
    model: String,
    key: String,
    headers_raw: String,
    temp: f64,
    top_p: f64,
    max_tokens: i64,
    timeout: i64,
    stream: bool,
}

// 锁中毒防护：任一持锁线程 panic 后应用仍可继续运行（NFR-CON-003 单点异常不得导致进程崩溃）
fn state_lock(ctx: &AppCtx) -> std::sync::MutexGuard<'_, Value> {
    ctx.state.lock().unwrap_or_else(|e| e.into_inner())
}

fn resolve_provider(app: &AppHandle, id: &str) -> Result<ProvConf, String> {
    let ctx = app.state::<AppCtx>();
    let st = state_lock(&ctx);
    let providers = st["providers"].as_array().ok_or("providers 数据异常")?;
    let p = providers
        .iter()
        .find(|p| p["id"].as_str() == Some(id))
        .ok_or_else(|| format!("供应商 {} 不存在", id))?;
    let params = &p["params"];
    Ok(ProvConf {
        ptype: p["type"].as_str().unwrap_or("openai").to_string(),
        base: p["baseUrl"].as_str().unwrap_or("").to_string(),
        model: p["model"].as_str().unwrap_or("").to_string(),
        key: key_get(id).unwrap_or_default(),
        headers_raw: p["headers"].as_str().unwrap_or("").to_string(),
        temp: params["temp"].as_f64().unwrap_or(0.8),
        top_p: params["topP"].as_f64().unwrap_or(1.0),
        max_tokens: params["maxTokens"].as_i64().unwrap_or(1024),
        timeout: params["timeout"].as_i64().unwrap_or(30),
        stream: params["stream"].as_bool().unwrap_or(true),
    })
}

fn join_url(base: &str, path: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    format!("{}{}", b, path)
}

fn apply_custom_headers(headers: &mut reqwest::header::HeaderMap, raw: &str) {
    if raw.trim().is_empty() {
        return;
    }
    if let Ok(map) = serde_json::from_str::<serde_json::Map<String, Value>>(raw) {
        for (k, v) in map {
            if let Some(vs) = v.as_str() {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.to_lowercase().as_bytes()),
                    reqwest::header::HeaderValue::from_str(vs),
                ) {
                    headers.insert(name, val);
                }
            }
        }
    }
}

// Key 转请求头：含非法字符时返回错误而非 panic（修复：粘贴异常 Key 导致进程崩溃）
fn key_header(conf: &ProvConf, openai_bearer: bool) -> Result<Option<reqwest::header::HeaderValue>, String> {
    if conf.key.is_empty() {
        return Ok(None);
    }
    let raw = if openai_bearer {
        format!("Bearer {}", conf.key)
    } else {
        conf.key.clone()
    };
    match raw.parse() {
        Ok(v) => Ok(Some(v)),
        Err(_) => Err("API Key 含非法字符（请检查是否混入空格/换行/中文）".to_string()),
    }
}

fn build_request(app: &AppHandle, conf: &ProvConf, messages: &[ChatMsg]) -> Result<reqwest::RequestBuilder, String> {
    let client = &app.state::<AppCtx>().http;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    let url;
    let body;
    match conf.ptype.as_str() {
        "anthropic" => {
            url = if conf.base.trim_end_matches('/').ends_with("/messages") {
                conf.base.trim_end_matches('/').to_string()
            } else {
                join_url(&conf.base, "/messages")
            };
            if let Some(v) = key_header(conf, false)? {
                headers.insert("x-api-key", v);
            }
            headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
            let mut sys = String::new();
            let mut convo: Vec<(String, String)> = vec![];
            for m in messages {
                if m.role == "system" {
                    sys.push_str(&m.content);
                    sys.push('\n');
                } else {
                    let role = if m.role == "assistant" { "assistant" } else { "user" };
                    if let Some(last) = convo.last_mut() {
                        if last.0 == role {
                            last.1.push('\n');
                            last.1.push_str(&m.content);
                            continue;
                        }
                    }
                    convo.push((role.to_string(), m.content.clone()));
                }
            }
            while convo.first().map(|m| m.0 != "user").unwrap_or(false) {
                convo.remove(0);
            }
            let msgs: Vec<Value> = convo
                .into_iter()
                .map(|(r, c)| json!({"role": r, "content": c}))
                .collect();
            body = json!({
                "model": conf.model,
                "system": if sys.is_empty() { Value::Null } else { json!(sys.trim()) },
                "messages": msgs,
                "max_tokens": conf.max_tokens,
                "temperature": conf.temp,
                "top_p": conf.top_p,
                "stream": conf.stream
            });
        }
        "gemini" => {
            let method = if conf.stream { ":streamGenerateContent?alt=sse" } else { ":generateContent" };
            url = join_url(&conf.base, &format!("/models/{}{}", conf.model, method));
            if let Some(v) = key_header(conf, false)? {
                headers.insert("x-goog-api-key", v);
            }
            let mut sys = String::new();
            let mut contents: Vec<Value> = vec![];
            for m in messages {
                if m.role == "system" {
                    sys.push_str(&m.content);
                    sys.push('\n');
                } else {
                    let role = if m.role == "assistant" { "model" } else { "user" };
                    if let Some(last) = contents.last_mut() {
                        if last["role"].as_str() == Some(role) {
                            let t = m.content.clone();
                            last["parts"][0]["text"] = json!(format!("{}\n{}", last["parts"][0]["text"].as_str().unwrap_or(""), t));
                            continue;
                        }
                    }
                    contents.push(json!({"role": role, "parts": [{"text": m.content}]}));
                }
            }
            while contents.first().map(|c| c["role"].as_str() != Some("user")).unwrap_or(false) {
                contents.remove(0);
            }
            body = json!({
                "contents": contents,
                "systemInstruction": if sys.is_empty() { Value::Null } else { json!({"parts": [{"text": sys.trim()}]}) },
                "generationConfig": {
                    "temperature": conf.temp,
                    "topP": conf.top_p,
                    "maxOutputTokens": conf.max_tokens
                }
            });
        }
        _ => {
            url = if conf.base.trim_end_matches('/').ends_with("/chat/completions") {
                conf.base.trim_end_matches('/').to_string()
            } else {
                join_url(&conf.base, "/chat/completions")
            };
            if let Some(v) = key_header(conf, true)? {
                headers.insert("authorization", v);
            }
            let msgs: Vec<Value> = messages
                .iter()
                .map(|m| json!({"role": m.role, "content": m.content}))
                .collect();
            body = json!({
                "model": conf.model,
                "messages": msgs,
                "temperature": conf.temp,
                "top_p": conf.top_p,
                "max_tokens": conf.max_tokens,
                "stream": conf.stream
            });
        }
    }
    apply_custom_headers(&mut headers, &conf.headers_raw);
    Ok(client.post(&url).headers(headers).json(&body))
}

fn sse_delta(ptype: &str, line: &str) -> Option<String> {
    let payload = line.strip_prefix("data:")?.trim();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }
    let j: Value = serde_json::from_str(payload).ok()?;
    match ptype {
        "anthropic" => {
            if j["type"].as_str() == Some("content_block_delta") {
                j["delta"]["text"].as_str().map(|s| s.to_string())
            } else {
                None
            }
        }
        "gemini" => {
            let parts = j["candidates"][0]["content"]["parts"].as_array()?;
            let t: String = parts.iter().filter_map(|p| p["text"].as_str()).collect();
            if t.is_empty() { None } else { Some(t) }
        }
        _ => j["choices"][0]["delta"]["content"].as_str().map(|s| s.to_string()),
    }
}

fn full_text(ptype: &str, j: &Value) -> String {
    match ptype {
        "anthropic" => j["content"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|c| c["type"].as_str() == Some("text"))
                    .filter_map(|c| c["text"].as_str())
                    .collect::<String>()
            })
            .unwrap_or_default(),
        "gemini" => j["candidates"][0]["content"]["parts"]
            .as_array()
            .map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect::<String>())
            .unwrap_or_default(),
        _ => j["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string(),
    }
}

fn err_message(body: &str) -> String {
    if let Ok(j) = serde_json::from_str::<Value>(body) {
        let candidates: [&str; 4] = ["/error/message", "/error/msg", "/message", "/msg"];
        for path in candidates {
            if let Some(s) = j.pointer(path).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
    }
    let b: String = body.chars().take(140).collect();
    if b.trim().is_empty() { "未知错误".into() } else { b }
}

async fn run_chat(app: AppHandle, args: ChatArgs) {
    let emit_err = |status: Option<u16>, msg: String| {
        let _ = app.emit_to("bubble", "chat-error", json!({"status": status, "message": msg}));
        app_log(&app, &format!("chat-error status={:?} msg={}", status, msg));
    };
    let conf = match resolve_provider(&app, &args.provider_id) {
        Ok(c) => c,
        Err(e) => return emit_err(None, e),
    };
    app.state::<AppCtx>().stop.store(false, Ordering::SeqCst);
    // 演示模式（mock）
    if conf.ptype == "mock" {
        let last_user = args.messages.iter().rev().find(|m| m.role == "user").map(|m| m.content.clone()).unwrap_or_default();
        let persona = {
            let ctx = app.state::<AppCtx>();
            let st = state_lock(&ctx);
            st["personas"].as_array()
                .and_then(|a| a.first())
                .and_then(|p| p["name"].as_str())
                .unwrap_or("团子")
                .to_string()
        };
        let reply = mock_reply(&last_user, &persona);
        let chunk = (reply.len() / 90).max(1);
        let bytes: Vec<(usize, usize)> = (0..reply.len()).step_by(chunk).map(|i| (i, chunk)).collect();
        for (i, c) in bytes {
            if app.state::<AppCtx>().stop.load(Ordering::SeqCst) {
                let _ = app.emit_to("bubble", "chat-stopped", ());
                return;
            }
            let end = (i + c).min(reply.len());
            let _ = app.emit_to("bubble", "chat-delta", json!({"text": &reply[i..end]}));
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
        let _ = app.emit_to("bubble", "chat-done", ());
        return;
    }
    if conf.base.trim().is_empty() {
        return emit_err(None, "BaseURL 未配置".into());
    }
    let want_stream = conf.stream;
    let rb = match build_request(&app, &conf, &args.messages) {
        Ok(rb) => rb,
        Err(e) => return emit_err(None, e),
    };
    let resp = match rb.send().await {
        Ok(r) => r,
        Err(e) => {
            let msg = if e.is_timeout() {
                "连接超时：请检查网络或在模型参数中调大超时时间".to_string()
            } else if e.is_connect() {
                "无法连接到服务地址：请检查 BaseURL 与网络（本地服务请确认已启动）".to_string()
            } else {
                format!("网络错误：{}", e)
            };
            return emit_err(None, msg);
        }
    };
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return emit_err(Some(status), err_message(&body));
    }
    let is_sse = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|c| c.contains("event-stream"))
        .unwrap_or(false);
    let idle = Duration::from_secs(((conf.timeout.max(5)) as u64 * 2).max(60));
    let ptype = conf.ptype.clone();
    if want_stream && is_sse {
        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        loop {
            if app.state::<AppCtx>().stop.load(Ordering::SeqCst) {
                let _ = app.emit_to("bubble", "chat-stopped", ());
                return;
            }
            let next = match tokio::time::timeout(idle, stream.next()).await {
                Err(_) => return emit_err(None, "流式响应超时（长时间无数据）".into()),
                Ok(None) => break,
                Ok(Some(Err(e))) => return emit_err(None, format!("流中断：{}", e)),
                Ok(Some(Ok(bytes))) => bytes,
            };
            buf.push_str(&String::from_utf8_lossy(&next));
            while let Some(idx) = buf.find('\n') {
                let line: String = buf.drain(..=idx).collect();
                let line = line.trim_end();
                if let Some(text) = sse_delta(&ptype, line) {
                    let _ = app.emit_to("bubble", "chat-delta", json!({"text": text}));
                }
            }
        }
        let rest = buf.trim().to_string();
        if let Some(text) = sse_delta(&ptype, &rest) {
            let _ = app.emit_to("bubble", "chat-delta", json!({"text": text}));
        }
        let _ = app.emit_to("bubble", "chat-done", ());
    } else {
        // 非流式 / 供应商不支持流式时自动降级（FR-MDL-011）
        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => return emit_err(None, format!("读取响应失败：{}", e)),
        };
        let j: Value = match serde_json::from_str(&body) {
            Ok(j) => j,
            Err(_) => return emit_err(Some(status), "响应不是合法 JSON（请检查 BaseURL 是否为 OpenAI 兼容端点）".into()),
        };
        let text = full_text(&ptype, &j);
        if text.is_empty() {
            return emit_err(Some(status), "模型返回空内容".into());
        }
        let _ = app.emit_to("bubble", "chat-delta", json!({"text": text}));
        let _ = app.emit_to("bubble", "chat-done", ());
    }
}

fn mock_reply(last_user: &str, persona: &str) -> String {
    let t = last_user.trim();
    if t.is_empty() {
        return format!("（演示模式 · {}）在呢在呢，想聊点什么？", persona);
    }
    if t.contains("记住") {
        return "已经把这条写进**长期记忆库**啦 ✅ 之后每次对话我都会自动带上它（最多注入 5 条相关记忆）。查看与管理：「设置 → 记忆」。".into();
    }
    if t.contains("提醒") {
        return "好的，提醒已创建 ✅ 到点我会跳出来播报，还支持「稍后 5 分钟」。\n\n演示模式的提醒走的是真实本地调度器，可以试试「10 秒后提醒我测试」立刻看效果。".into();
    }
    if t.contains("记忆") {
        return "目前长期记忆还是空的～你可以对我说「记住：我喜欢蓝色」，我就会把它存进记忆库，之后的每次对话都会自动带上它。".into();
    }
    if regex_like_contains(t, &["markdown", "渲染", "代码示例", "给我示例"]) {
        return "没问题，给你一段 Markdown 渲染示例：\n\n**粗体**、*斜体*、`行内代码` 都支持。\n\n- 无序列表项一\n- 无序列表项二\n\n1. 有序列表\n2. 第二项\n\n代码块长这样：\n\n```js\nfunction pet() {\n  return \"喵 ~\";\n}\n```\n\n> 引用块也可以哦。演示模式的流式输出也是逐字蹦出来的。".into();
    }
    if regex_like_contains(t, &["你是谁", "演示模式", "你能做什么", "你会什么", "怎么用", "帮助"]) {
        return "我是「桌面 AI 宠物」桌面版内置的演示角色，负责在**不联网、不配 Key** 的情况下验证核心链路：\n\n- 点击宠物 → 微型气泡对话（Enter 发送 / Esc 关闭）\n- 主进程流式输出 + 随时停止\n- Markdown 与代码块渲染、一键复制\n- 长期记忆：对我说 `记住：我喜欢蓝色`\n- 自然语言提醒：`25 分钟后提醒我喝水`\n\n想用真实模型：打开 **设置 → 模型**，选供应商模板、填 BaseURL 和 Key，测试连接通过后即可对话。密钥会存入 Windows 凭据管理器，不会进入渲染层。".into();
    }
    if regex_like_contains(t, &["你好", "您好", "hi", "hello", "嗨", "哈喽"]) {
        return format!("你好呀～我是{}（演示模式）。没有配置 API Key 也能体验完整流程：流式输出、Markdown 渲染、长期记忆、定时提醒我都有哦。想接真实模型的话，点右上角 ⚙ 打开「模型」配置即可。", persona);
    }
    if regex_like_contains(t, &["谢谢", "感谢", "辛苦"]) {
        return "不客气～有需要随时戳我（字面意义上的戳）ฅ".into();
    }
    let cut: String = t.chars().take(36).collect();
    let tails = [
        format!("这个嘛…作为演示模式的我只能陪你聊聊啦。接上真实模型后我就能正经干活了～"),
        format!("收到！「{}」这句话我已原样记下。演示模式不会真的思考，但流程和真实模型完全一致：同样的上下文组装、主进程流式渲染与错误处理。", cut),
        format!("嗯…你说的「{}」很有意思。想让我认真回答的话，去「设置 → 模型」接一个真模型吧！", cut),
    ];
    let idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0) as usize)
        % tails.len();
    format!("（演示模式 · {}）{}", persona, tails[idx])
}

fn regex_like_contains(t: &str, keys: &[&str]) -> bool {
    let lower = t.to_lowercase();
    keys.iter().any(|k| lower.contains(k))
}

// ================= Tauri 命令 =================

#[tauri::command]
fn state_load(ctx: tauri::State<AppCtx>) -> Value {
    ctx.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn state_save(app: AppHandle, ctx: tauri::State<AppCtx>, state: Value, origin: String) -> Result<(), String> {
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        // reminders 由主进程调度器权威管理，防止前端旧快照复活已触发的提醒
        let reminders = st["reminders"].take();
        *st = state;
        st["reminders"] = reminders;
    }
    persist_state(&ctx);
    let _ = app.emit("state-changed", json!({"origin": origin}));
    Ok(())
}

#[tauri::command]
fn reset_data(app: AppHandle, ctx: tauri::State<AppCtx>) -> Result<(), String> {
    // 删除全部密钥（DS-001：安全清理）
    if let Some(providers) = ctx.state.lock().unwrap_or_else(|e| e.into_inner())["providers"].as_array() {
        for p in providers {
            if let Some(id) = p["id"].as_str() {
                key_del(id);
            }
        }
    }
    *ctx.state.lock().unwrap_or_else(|e| e.into_inner()) = default_state();
    *ctx.pending.lock().unwrap() = vec![];
    *ctx.dnd_queue.lock().unwrap() = vec![];
    persist_state(&ctx);
    if let Some(pet) = app.get_webview_window("pet") {
        let _ = pet.set_ignore_cursor_events(false);
    }
    position_pet(&app);
    rebuild_tray(&app);
    let _ = app.emit("state-reset", ());
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
    app_log(&app, "已重置全部数据");
    Ok(())
}

#[tauri::command]
fn key_set(_app: AppHandle, id: String, key: String) -> Result<(), String> {
    if key.trim().is_empty() {
        key_del(&id);
        Ok(())
    } else {
        key_set_kv(&id, key.trim())
    }
}

#[tauri::command]
fn key_delete(id: String) {
    key_del(&id);
}

#[tauri::command]
fn key_has(id: String) -> bool {
    key_get(&id).map(|k| !k.is_empty()).unwrap_or(false)
}

#[tauri::command]
async fn provider_test(app: AppHandle, id: String) -> Result<Value, String> {
    let conf = resolve_provider(&app, &id)?;
    if conf.ptype == "mock" {
        return Ok(json!({"ok": true, "latency": 118, "models": ["demo-pet-v1"], "note": "内置演示模式，随时可用"}));
    }
    if conf.base.trim().is_empty() {
        return Ok(json!({"ok": false, "message": "请先填写 BaseURL"}));
    }
    // 本地服务（Ollama / LM Studio / vLLM 等）无需 API Key；
    // 云端无 Key 时也继续测试，由服务端 401 给出准确反馈
    let is_local = {
        let b = conf.base.to_lowercase();
        b.contains("localhost") || b.contains("127.0.0.1") || b.contains("[::1]") || b.contains("0.0.0.0")
    };
    let client = app.state::<AppCtx>().http.clone();
    let mut headers = reqwest::header::HeaderMap::new();
    let url = match conf.ptype.as_str() {
        "anthropic" => {
            if let Some(v) = key_header(&conf, false)? {
                headers.insert("x-api-key", v);
            }
            headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
            join_url(&conf.base, "/models")
        }
        "gemini" => {
            if let Some(v) = key_header(&conf, false)? {
                headers.insert("x-goog-api-key", v);
            }
            join_url(&conf.base, "/models")
        }
        _ => {
            if let Some(v) = key_header(&conf, true)? {
                headers.insert("authorization", v);
            }
            if conf.base.trim_end_matches('/').ends_with("/models") {
                conf.base.trim_end_matches('/').to_string()
            } else {
                join_url(&conf.base, "/models")
            }
        }
    };
    apply_custom_headers(&mut headers, &conf.headers_raw);
    let t0 = std::time::Instant::now();
    let resp = client.get(&url).headers(headers).timeout(Duration::from_secs(12)).send().await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            let msg = if e.is_timeout() {
                "连接超时（12 秒）".to_string()
            } else if e.is_connect() {
                "无法连接：请检查 BaseURL 与网络".to_string()
            } else {
                format!("网络错误：{}", e)
            };
            return Ok(json!({"ok": false, "message": msg}));
        }
    };
    let latency = t0.elapsed().as_millis() as u64;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    if status == 401 || status == 403 {
        return Ok(json!({"ok": false, "message": format!("认证失败（{}）：API Key 无效", status)}));
    }
    if status == 404 {
        return Ok(json!({"ok": false, "message": "接口不存在（404）：请检查 BaseURL 路径".to_string()}));
    }
    if status >= 400 {
        return Ok(json!({"ok": false, "message": format!("HTTP {}：{}", status, err_message(&body))}));
    }
    let j: Value = match serde_json::from_str(&body) {
        Ok(j) => j,
        Err(_) => return Ok(json!({"ok": false, "message": "响应不是合法 JSON"})),
    };
    let models: Vec<String> = match conf.ptype.as_str() {
        "gemini" => j["models"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["name"].as_str())
                    .map(|s| s.trim_start_matches("models/").to_string())
                    .collect()
            })
            .unwrap_or_default(),
        _ => j["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["id"].as_str().or(m["name"].as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default(),
    };
    let note = if is_local && conf.key.is_empty() {
        format!("{} 个可用模型（本地服务，无需 Key）", models.len())
    } else {
        format!("{} 个可用模型", models.len())
    };
    Ok(json!({"ok": true, "latency": latency, "models": models, "note": note}))
}

#[tauri::command]
fn chat_start(app: AppHandle, args: ChatArgs) {
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        run_chat(app2, args).await;
    });
}

#[tauri::command]
fn chat_stop(ctx: tauri::State<AppCtx>) {
    ctx.stop.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn pet_clicked(app: AppHandle) -> Result<(), String> {
    show_bubble(&app)
}

#[tauri::command]
fn pet_save_pos(app: AppHandle, ctx: tauri::State<AppCtx>) -> Result<(), String> {
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    let pos = pet.outer_position().map_err(|e| e.to_string())?;
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["petPos"] = json!({"x": pos.x, "y": pos.y});
    }
    persist_state(&ctx);
    Ok(())
}

#[tauri::command]
fn pet_set_size(app: AppHandle, w: f64, h: f64) -> Result<(), String> {
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    let pos = pet.outer_position().map_err(|e| e.to_string())?;
    let size = pet.outer_size().map_err(|e| e.to_string())?;
    let new_x = pos.x + size.width as i32 / 2 - (w * sf(&pet)) as i32 / 2;
    let new_y = pos.y + size.height as i32 - (h * sf(&pet)) as i32;
    pet.set_size(Size::Logical(LogicalSize::new(w, h)))
        .map_err(|e| e.to_string())?;
    pet.set_position(Position::Physical(PhysicalPosition::new(new_x, new_y)))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_pet_scale(app: AppHandle, ctx: tauri::State<AppCtx>, scale: f64) -> Result<(), String> {
    let scale = scale.clamp(0.5, 2.0);
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["petScale"] = json!(scale);
    }
    persist_state(&ctx);
    let _ = pet.set_size(Size::Logical(LogicalSize::new(120.0 * scale, 112.0 * scale)));
    Ok(())
}

#[tauri::command]
fn set_click_through(app: AppHandle, ctx: tauri::State<AppCtx>, on: bool) -> Result<(), String> {
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    pet.set_ignore_cursor_events(on).map_err(|e| e.to_string())?;
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["clickThrough"] = json!(on);
    }
    persist_state(&ctx);
    rebuild_tray(&app);
    let _ = app.emit("click-through-changed", json!({"on": on}));
    Ok(())
}

#[tauri::command]
fn set_pet_visible(app: AppHandle, ctx: tauri::State<AppCtx>, visible: bool) -> Result<(), String> {
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    if visible {
        pet.show().map_err(|e| e.to_string())?;
    } else {
        let _ = hide_bubble(&app);
        pet.hide().map_err(|e| e.to_string())?;
    }
    *ctx.pet_visible.lock().unwrap() = visible;
    rebuild_tray(&app);
    Ok(())
}

#[tauri::command]
fn reset_pet_pos(app: AppHandle, ctx: tauri::State<AppCtx>) -> Result<(), String> {
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["petPos"] = Value::Null;
    }
    persist_state(&ctx);
    position_pet(&app);
    Ok(())
}

#[tauri::command]
fn show_bubble_cmd(app: AppHandle) -> Result<(), String> {
    show_bubble(&app)
}

#[tauri::command]
fn hide_bubble_cmd(app: AppHandle) -> Result<(), String> {
    hide_bubble(&app)
}

#[tauri::command]
fn bubble_resize(app: AppHandle, ctx: tauri::State<AppCtx>, height: f64) -> Result<(), String> {
    let bubble = app.get_webview_window("bubble").ok_or("bubble 窗口不存在")?;
    let scale = sf(&bubble);
    let h_log = height.clamp(200.0, 560.0);
    let new_h = (h_log * scale).round() as u32;
    let pos = bubble.outer_position().map_err(|e| e.to_string())?;
    let size = bubble.outer_size().map_err(|e| e.to_string())?;
    let below = *ctx.bubble_anchor_below.lock().unwrap();
    let mut new_y = if below { pos.y } else { pos.y + size.height as i32 - new_h as i32 };
    let work = work_area_at(pos.x + 10, pos.y + 10);
    new_y = new_y.clamp(work.top, (work.bottom - new_h as i32).max(work.top));
    bubble
        .set_size(Size::Physical(PhysicalSize::new(size.width, new_h)))
        .map_err(|e| e.to_string())?;
    bubble
        .set_position(Position::Physical(PhysicalPosition::new(pos.x, new_y)))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_settings_cmd(app: AppHandle) -> Result<(), String> {
    open_settings(&app)
}

#[tauri::command]
fn set_dnd(app: AppHandle, ctx: tauri::State<AppCtx>, on: bool) -> Result<(), String> {
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["dnd"] = json!(on);
    }
    persist_state(&ctx);
    if !on {
        // 关闭免打扰：补发排队提醒
        let queued: Vec<Value> = {
            let mut q = ctx.dnd_queue.lock().unwrap();
            std::mem::take(&mut *q)
        };
        if !queued.is_empty() {
            for r in queued {
                push_notice(&app, json!({"kind": "reminder", "reminder": r}));
            }
            let _ = show_bubble_reason(&app, "reminder");
        }
    }
    let _ = app.emit("dnd-changed", json!({"on": on}));
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
    rebuild_tray(&app);
    Ok(())
}

#[tauri::command]
fn reminder_add(app: AppHandle, ctx: tauri::State<AppCtx>, reminder: Value) -> Result<(), String> {
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        if !st["reminders"].is_array() {
            st["reminders"] = json!([]);
        }
        st["reminders"].as_array_mut().unwrap().push(reminder);
    }
    persist_state(&ctx);
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
    Ok(())
}

#[tauri::command]
fn reminder_snooze(app: AppHandle, ctx: tauri::State<AppCtx>, id: String) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(rems) = st["reminders"].as_array_mut() {
            if let Some(r) = rems.iter_mut().find(|r| r["id"].as_str() == Some(id.as_str())) {
                r["due"] = json!(now + 5 * 60_000);
                r["rule"] = json!("once");
                r["enabled"] = json!(true);
            } else {
                // 已触发的提醒：重新插入为稍后提醒
                let mut nr = json!({"id": id, "content": "提醒", "due": now + 5 * 60_000, "rule": "once", "enabled": true});
                nr["content"] = ctx
                    .pending
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|n| n["reminder"]["id"].as_str() == Some(id.as_str()))
                    .and_then(|n| n["reminder"]["content"].clone().into())
                    .unwrap_or_else(|| json!("提醒"));
                rems.push(nr);
            }
        }
    }
    persist_state(&ctx);
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
    Ok(())
}

#[tauri::command]
fn reminder_delete(app: AppHandle, ctx: tauri::State<AppCtx>, id: String) -> Result<(), String> {
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(rems) = st["reminders"].as_array_mut() {
            rems.retain(|r| r["id"].as_str() != Some(id.as_str()));
        }
    }
    persist_state(&ctx);
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
    Ok(())
}

#[tauri::command]
fn take_pending_notices(ctx: tauri::State<AppCtx>) -> Vec<Value> {
    let mut p = ctx.pending.lock().unwrap();
    std::mem::take(&mut *p)
}

#[tauri::command]
fn export_text(app: AppHandle, filename: String, content: String) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = filename
        .chars()
        .filter(|c| !r#"\/:*?"<>|"#.contains(*c))
        .collect();
    if safe.trim().is_empty() {
        return Err("文件名无效".into());
    }
    let path = dir.join(safe);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_app_version(app: AppHandle) -> String {
    // 版本号唯一来源：tauri.conf.json（此前硬编码曾与实际版本脱节）
    format!("{}（桌面版 MVP）", app.package_info().version)
}

// ================= 入口 =================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("无法获取应用数据目录");
            std::fs::create_dir_all(&dir).ok();
            let data_path = dir.join("data.json");
            let state = load_or_default(&data_path);
            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .build()
                .expect("HTTP 客户端构建失败");
            app.manage(AppCtx {
                data_path,
                state: Mutex::new(state),
                stop: AtomicBool::new(false),
                bubble_anchor_below: Mutex::new(false),
                fg_before: Mutex::new(0),
                pet_visible: Mutex::new(true),
                pending: Mutex::new(vec![]),
                dnd_queue: Mutex::new(vec![]),
                http,
            });

            // 气泡窗口（隐藏常驻）
            WebviewWindowBuilder::new(app, "bubble", WebviewUrl::App("bubble.html".into()))
                .inner_size(340.0, 460.0)
                .transparent(true)
                .decorations(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .shadow(false)
                .resizable(false)
                .visible(false)
                .build()?;

            // 设置窗口（隐藏预创建：首次打开即显示已加载内容，避免懒创建 WebView2 冷启动白屏）
            WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
                .title("设置 — 桌面 AI 宠物")
                .inner_size(780.0, 560.0)
                .min_inner_size(600.0, 440.0)
                .decorations(false)
                .center()
                .visible(false)
                .build()?;

            // 宠物初始位置与尺寸
            position_pet(app.handle());

            // 托盘
            let icon = app
                .default_window_icon()
                .expect("缺少应用图标")
                .clone();
            let _tray = TrayIconBuilder::with_id("deskpet-tray")
                .icon(icon)
                .tooltip("桌面 AI 宠物")
                .show_menu_on_left_click(false)
                .on_menu_event(|tray, event| {
                    let app = tray.app_handle().clone();
                    let id: String = event.id().as_ref().to_string();
                    match id.as_str() {
                        "open_chat" => {
                            let _ = show_bubble_reason(&app, "tray");
                        }
                        "new_session" => {
                            let _ = show_bubble_reason(&app, "tray");
                            let _ = app.emit_to("bubble", "new-session-req", ());
                        }
                        "toggle_pet" => {
                            let visible = {
                                let ctx = app.state::<AppCtx>();
                                let pv = *ctx.pet_visible.lock().unwrap();
                                pv
                            };
                            let _ = set_pet_visible(app.clone(), app.state::<AppCtx>(), !visible);
                        }
                        "settings" => {
                            let _ = open_settings(&app);
                        }
                        "dnd" => {
                            let on = {
                                let ctx = app.state::<AppCtx>();
                                let g = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
                                g["settings"]["dnd"].as_bool().unwrap_or(false)
                            };
                            let _ = set_dnd(app.clone(), app.state::<AppCtx>(), !on);
                        }
                        "ct" => {
                            let on = {
                                let ctx = app.state::<AppCtx>();
                                let g = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
                                g["settings"]["clickThrough"].as_bool().unwrap_or(false)
                            };
                            let _ = set_click_through(app.clone(), app.state::<AppCtx>(), !on);
                        }
                        "autostart" => {
                            let enabled = use_autostart(&app, |m| m.is_enabled().unwrap_or(false));
                            let _ = use_autostart(&app, move |m| {
                                if enabled {
                                    let _ = m.disable();
                                } else {
                                    let _ = m.enable();
                                }
                            });
                            rebuild_tray(&app);
                        }
                        "reset_pos" => {
                            let _ = reset_pet_pos(app.clone(), app.state::<AppCtx>());
                        }
                        "reset_data" => {
                            let _ = app.emit_to("bubble", "confirm-reset", ());
                            let _ = open_settings(&app);
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle().clone();
                        if bubble_visible(&app) {
                            let _ = hide_bubble(&app);
                        } else {
                            let _ = show_bubble(&app);
                        }
                    }
                })
                .build(app)?;
            rebuild_tray(app.handle());

            // 全局快捷键（注册失败不阻断启动：可能被其他程序占用，托盘左键仍可唤起气泡）
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
                let gs = app.global_shortcut();
                if let Err(e) = gs.on_shortcut("ctrl+shift+space", |app, _s, e| {
                    if e.state() == ShortcutState::Pressed {
                        if bubble_visible(app) {
                            let _ = hide_bubble(app);
                        } else {
                            let _ = show_bubble_reason(app, "shortcut");
                        }
                    }
                }) {
                    app_log(
                        app.handle(),
                        &format!("全局快捷键 Ctrl+Shift+Space 注册失败（可能被其他程序占用）：{}", e),
                    );
                }
                if let Err(e) = gs.on_shortcut("ctrl+comma", |app, _s, e| {
                    if e.state() == ShortcutState::Pressed {
                        let _ = open_settings(app);
                    }
                }) {
                    app_log(
                        app.handle(),
                        &format!("全局快捷键 Ctrl+, 注册失败（可能被其他程序占用）：{}", e),
                    );
                }
            }

            // 提醒 / 主动搭话调度线程
            start_scheduler(app.handle().clone());

            app_log(app.handle(), "DeskPet 桌面版启动完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            state_load, state_save, reset_data,
            key_set, key_delete, key_has,
            provider_test,
            chat_start, chat_stop,
            pet_clicked, pet_save_pos, pet_set_size, set_pet_scale,
            set_click_through, set_pet_visible, reset_pet_pos,
            show_bubble_cmd, hide_bubble_cmd, bubble_resize,
            open_settings_cmd, set_dnd,
            reminder_add, reminder_snooze, reminder_delete,
            take_pending_notices,
            export_text, quit_app, get_app_version
        ])
        .run(tauri::generate_context!())
        .expect("DeskPet 运行失败");
}

// ============ 回归测试（cargo test） ============
#[cfg(test)]
mod tests {
    use super::*;

        fn conf_with_key(key: &str) -> ProvConf {
            ProvConf {
                ptype: "openai".into(),
                base: "http://127.0.0.1:11434/v1".into(),
                model: "m".into(),
                key: key.into(),
                headers_raw: String::new(),
                temp: 0.8,
                top_p: 1.0,
                max_tokens: 1024,
                timeout: 30,
                stream: true,
            }
        }

        #[test]
        fn key_header_empty_returns_none() {
            // 无 Key 的本地服务（Ollama 等）不应附带认证头
            assert!(key_header(&conf_with_key(""), true).unwrap().is_none());
            assert!(key_header(&conf_with_key(""), false).unwrap().is_none());
        }

        #[test]
        fn key_header_valid_formats() {
            let v = key_header(&conf_with_key("sk-abc123"), true).unwrap().unwrap();
            assert_eq!(v.to_str().unwrap(), "Bearer sk-abc123");
            let v2 = key_header(&conf_with_key("sk-abc123"), false).unwrap().unwrap();
            assert_eq!(v2.to_str().unwrap(), "sk-abc123");
        }

        #[test]
        fn key_header_never_panics_rejects_control_chars() {
            // 回归（v0.2.2）：粘贴异常 Key 不得 panic（release 为 panic=abort，进程会静默消失）。
            // http crate 的 header 值校验规则：控制字符（\r \n \0 等不可见字节）非法；
            // 空格与 obs-text（≥0x80，含中文 UTF-8 字节）按 RFC 7230 是合法的 header 值字节——
            // 这类 Key 会随请求发出、由服务端按无效凭据拒绝（401），不属于本地校验错误。
            for bad in ["sk-1\r\n", "k\u{0}y", "k\u{7}y", "line1\nline2"] {
                let r = key_header(&conf_with_key(bad), true);
                assert!(r.is_err(), "expected Err for {:?}", bad);
                let r2 = key_header(&conf_with_key(bad), false);
                assert!(r2.is_err(), "expected Err for {:?}", bad);
            }
            // 宽松输入：不得 panic，且可解析为 header 值（安全降级为服务端 401）
            for weird in ["坏 key", " key ", "k\u{e1}y"] {
                let r = key_header(&conf_with_key(weird), true);
                assert!(r.is_ok(), "expected Ok for {:?}", weird);
            }
            assert!(key_header(&conf_with_key(""), true).unwrap().is_none());
        }

        #[test]
        fn sse_delta_openai_paths() {
            let ok = sse_delta("openai", "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}");
            assert_eq!(ok.unwrap(), "你好");
            assert_eq!(sse_delta("openai", "data: [DONE]"), None);
            assert_eq!(sse_delta("openai", ": keep-alive"), None);
            assert_eq!(sse_delta("openai", "data: "), None);
        }

        #[test]
        fn join_url_variants() {
            assert_eq!(join_url("http://a/v1", "/chat/completions"), "http://a/v1/chat/completions");
            assert_eq!(join_url("http://a/v1/", "/m"), "http://a/v1/m");
            assert_eq!(join_url(" http://a ", "/m"), "http://a/m");
        }

        #[test]
        fn err_message_extracts_nested() {
            let body = r#"{"error":{"message":"rate limited"}}"#;
            assert_eq!(err_message(body), "rate limited");
            assert_eq!(err_message(""), "未知错误");
        }

        #[test]
        fn write_state_file_holds_no_state_lock() {
            // 回归测试：v0.2.3 曾因 write_state_file 内部 state.lock() 且 tick_reminders 持锁调用，
            // 同线程重入 std::sync::Mutex 自死锁，state 锁被永久持有后整机冻结。
            // 现在写盘函数绝不接触 state 锁——持锁调用也必须立即完成（若回退，本测试将挂起）。
            let dir = std::env::temp_dir().join("deskpet-regression-lock");
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join("data.json");
            let ctx = AppCtx {
                data_path: path.clone(),
                state: Mutex::new(json!({"reminders": []})),
                stop: AtomicBool::new(false),
                bubble_anchor_below: Mutex::new(false),
                fg_before: Mutex::new(0),
                pet_visible: Mutex::new(true),
                pending: Mutex::new(vec![]),
                dnd_queue: Mutex::new(vec![]),
                http: reqwest::Client::new(),
            };
            {
                let _guard = ctx.state.lock().unwrap(); // 模拟调度器持锁
                write_state_file(&ctx, "{\"ok\":true}"); // 若内部再取锁 → 此处死锁
            }
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"ok\":true}");
            let _ = std::fs::remove_file(&path);
        }
}
