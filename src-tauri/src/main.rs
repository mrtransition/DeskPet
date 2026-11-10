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
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

// ================= 默认状态（与 MVP 版一致的数据结构，密钥不入库） =================

const DEFAULT_STATE: &str = r##"{
  "v": 1,
  "providers": [
    {"id":"p-demo","name":"演示模式（内置，无需 Key）","type":"mock","baseUrl":"","model":"demo-pet-v1","headers":"","apiKey":"","isDefault":true,
     "params":{"temp":0.8,"topP":1,"maxTokens":1024,"ctxRounds":10,"ctxTokens":8000,"timeout":30,"retry":2,"stream":true}},
    {"id":"p-deepseek","name":"DeepSeek","type":"openai","baseUrl":"https://api.deepseek.com/v1","model":"deepseek-chat","headers":"","apiKey":"","isDefault":false,
     "params":{"temp":0.8,"topP":1,"maxTokens":1024,"ctxRounds":10,"ctxTokens":8000,"timeout":30,"retry":2,"stream":true}},
    {"id":"p-openai","name":"OpenAI","type":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o-mini","headers":"","apiKey":"","isDefault":false,
     "params":{"temp":0.8,"topP":1,"maxTokens":1024,"ctxRounds":10,"ctxTokens":8000,"timeout":30,"retry":2,"stream":true}},
    {"id":"p-ollama","name":"Ollama（本地）","type":"openai","baseUrl":"http://localhost:11434/v1","model":"qwen2.5:7b","headers":"","apiKey":"","isDefault":false,
     "params":{"temp":0.8,"topP":1,"maxTokens":1024,"ctxRounds":10,"ctxTokens":8000,"timeout":30,"retry":2,"stream":true}}
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
    "showInTaskbar": false, "alwaysOnTop": true,
    "chatter": "off", "chatterStart": "09:00", "chatterEnd": "22:00",
    "health": {"enabled": false, "sitMin": 60, "waterMin": 45},
    "dnd": false, "dndStart": "", "dndEnd": "",
    "clipboardWatch": false,
    "lang": "zh", "voice": false, "voiceURI": "", "voiceRate": 1,
    "summaryThreshold": 24,
    "updateUrl": "https://api.github.com/repos/mrtransition/DeskPet/releases/latest",
    "updateAuto": false,
    "toolSettings": {"enabled": true, "auth": {"get_weather": "deny", "web_search": "deny"}, "openWhitelist": ["github.com", "www.github.com", "developer.mozilla.org", "www.bing.com", "www.baidu.com", "wikipedia.org"]},
    "lastPersonaId": "ps-1", "lastProviderId": null
  },
  "stats": {"requests": 0, "tokensIn": 0, "tokensOut": 0}
}"##;

// ================= 应用上下文 =================

struct AppCtx {
    data_path: std::path::PathBuf,
    state: Mutex<Value>,
    stop: AtomicBool,
    // 并发请求代际：新一轮请求递增 seq 并清空 cancelled，旧请求看到 seq 变化即自我终止，
    // 不会因 stop 被新一轮重置而「复活」。
    chat_seq: std::sync::atomic::AtomicU64,
    chat_cancelled: AtomicBool,
    bubble_anchor_below: Mutex<bool>,
    fg_before: Mutex<usize>,
    pet_visible: Mutex<bool>,
    pending: Mutex<Vec<Value>>,
    dnd_queue: Mutex<Vec<Value>>,
    file_lock: Mutex<()>,
    // 按授权 id 索引：并发工具调用（多窗口/连续调用）不会互相覆盖等待槽
    tool_wait: Mutex<std::collections::HashMap<String, ToolWait>>,
    clipboard_last: Mutex<String>,
    http: reqwest::Client,
}

// FR-REM-006：久坐 / 喝水计时器
#[derive(Clone, Copy)]
struct HealthTimer {
    sit: std::time::Instant,
    water: std::time::Instant,
    sit_water_next: bool,
}

struct ToolDecision {
    allow: bool,
    remember: bool,
}

struct ToolWait {
    #[allow(dead_code)] // 与 map 键一致，保留便于排查授权超时的现场
    id: String,
    #[allow(dead_code)]
    name: String,
    tx: tokio::sync::oneshot::Sender<ToolDecision>,
}

fn load_or_default(path: &std::path::PathBuf) -> Value {
    // NFR-REL-002 / NFR-REL-003：主文件损坏时回退到最近一次备份，避免静默丢失全部数据
    let try_read = |p: &std::path::PathBuf| -> Option<Value> {
        std::fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    };
    if let Some(mut v) = try_read(path) {
        fill_defaults(&default_state(), &mut v);
        return v;
    }
    let bak = path.with_extension("json.bak");
    if let Some(mut v) = try_read(&bak) {
        // 保留损坏文件供排查，不让下一次写入直接覆盖现场
        if path.exists() {
            let _ = std::fs::rename(path, path.with_extension(format!("json.corrupt-{}", now_ms())));
        }
        fill_defaults(&default_state(), &mut v);
        return v;
    }
    if path.exists() {
        let _ = std::fs::rename(path, path.with_extension(format!("json.corrupt-{}", now_ms())));
    }
    default_state()
}

// 升级兼容：旧版 data.json 没有新版本新增的字段。只补缺失的键，不覆盖用户已有值；
// 数组（会话 / 供应商 / 人格）整体保留，避免默认示例数据混入用户数据。
fn fill_defaults(default: &Value, loaded: &mut Value) {
    if let (Value::Object(d), Value::Object(l)) = (default, loaded) {
        for (k, dv) in d {
            match l.get_mut(k) {
                Some(lv) => fill_defaults(dv, lv),
                None => {
                    l.insert(k.clone(), dv.clone());
                }
            }
        }
    }
}

fn default_state() -> Value {
    serde_json::from_str(DEFAULT_STATE).expect("默认状态 JSON 合法")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// 写盘专用：绝不接触 state 锁。
// 设计约束：v0.2.3 及之前本函数内部会 state.lock()，而 tick_reminders 在持锁状态下调用它，
// std::sync::Mutex 同线程重入自死锁 → state 锁被调度器线程永久持有 →
// 主线程执行 state_load 等命令时永久阻塞 → 整机冻结（点击设置按钮触发暴露）。
// 现在改为接收序列化好的快照，从 API 层面杜绝此类死锁。
//
// NFR-REL-002：先写临时文件再原子替换，避免异常退出留下半截 JSON；
// 同时保留上一份 data.json.bak 作为主文件损坏时的回退。
fn write_state_file(ctx: &AppCtx, snapshot: &str) {
    let _guard = ctx.file_lock.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = ctx.data_path.with_extension("json.tmp");
    let bak = ctx.data_path.with_extension("json.bak");
    if std::fs::write(&tmp, snapshot).is_err() {
        return;
    }
    if ctx.data_path.exists() {
        let _ = std::fs::copy(&ctx.data_path, &bak);
    }
    if std::fs::rename(&tmp, &ctx.data_path).is_err() {
        // 极少数文件系统不支持覆盖式 rename：退化为直接复制，仍保证 tmp 不残留
        let _ = std::fs::copy(&tmp, &ctx.data_path);
        let _ = std::fs::remove_file(&tmp);
    }
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

// NFR-MNT-001：统一日志级别 + JSON Lines 结构化日志，写入前脱敏。
// 保留 app_log 作为 info 级别快捷入口，避免所有调用点都传事件名。
fn app_log(app: &AppHandle, msg: &str) {
    log_event(app, "info", "app", msg);
}

fn log_event(app: &AppHandle, level: &str, event: &str, msg: &str) {
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let line = json!({
            "ts": chrono_now(),
            "level": level,
            "event": event,
            "msg": redact_secrets(msg),
        });
        let mut text = line.to_string();
        text.push('\n');
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
            let _ = f.write_all(text.as_bytes());
        }
    }
}

// 升级兼容：v2.0 及之前写的是纯文本日志。首次启动时把旧文件改名保留，
// 保证 deskpet.log 从本次运行起始终是 JSON Lines（NFR-MNT-001）。
fn migrate_legacy_log(app: &AppHandle) {
    if let Ok(dir) = app.path().app_data_dir() {
        let p = dir.join("deskpet.log");
        let Ok(text) = std::fs::read_to_string(&p) else { return };
        if has_legacy_log_lines(&text) {
            let legacy = dir.join(format!("deskpet.log.legacy-{}", now_ms()));
            let _ = std::fs::rename(&p, legacy);
        }
    }
}

// 混入任意一行旧版纯文本日志都需要归档（例如先运行旧版、再升级到 v2.1）。
fn has_legacy_log_lines(text: &str) -> bool {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .any(|l| !l.trim_start().starts_with('{'))
}

// 日志脱敏：隐藏 sk- 令牌、Bearer 令牌与显式 api_key 值。
// 不依赖正则库，按标记扫描；宁可多遮盖，也不把密钥写进日志。
fn redact_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    loop {
        let lower = rest.to_lowercase();
        let mut best: Option<(usize, &'static str)> = None;
        for pat in ["sk-", "bearer ", "api_key=", "apikey=", "api-key=", "authorization:"] {
            if let Some(pos) = lower.find(pat) {
                if best.map(|(b, _)| pos < b).unwrap_or(true) {
                    best = Some((pos, pat));
                }
            }
        }
        let Some((pos, pat)) = best else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..pos]);
        out.push_str(&rest[pos..pos + pat.len()]);
        let after = &rest[pos + pat.len()..];
        if pat == "authorization:" {
            // Authorization 头后面通常是 "Bearer <token>"，整段行内值遮盖
            out.push_str(" ***");
            break;
        }
        let secret_len = after
            .char_indices()
            .find(|(_, c)| c.is_whitespace() || matches!(c, ',' | ';' | '"' | '\''))
            .map(|(i, _)| i)
            .unwrap_or(after.len());
        out.push_str("***");
        rest = &after[secret_len..];
    }
    out
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
    let (y, mo, day) = days_to_ymd(d as i64);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+08:00", y, mo, day, h, mi, s)
}

// 公历日期换算（Howard Hinnant 算法），避免为日志时间引入额外依赖。
fn days_to_ymd(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

// 本地时间（UTC+8）当天的分钟数，用于主动搭话 / 免打扰时段判断
fn local_minutes() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + 8 * 3600;
    (secs % 86400) / 60
}

fn parse_hhmm(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (h, m) = t.split_once(':').or_else(|| t.split_once('：'))?;
    let h: i64 = h.trim().parse().ok()?;
    let m: i64 = m.trim().parse().ok()?;
    if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
        return None;
    }
    Some(h * 60 + m)
}

// 跨零点时段（如 22:00–08:00）也应正确命中
fn in_time_window(now_min: i64, start: &str, end: &str) -> bool {
    let (Some(s), Some(e)) = (parse_hhmm(start), parse_hhmm(end)) else {
        return false;
    };
    if s == e {
        return false;
    }
    if s < e {
        now_min >= s && now_min < e
    } else {
        now_min >= s || now_min < e
    }
}

fn dnd_active(st: &Value) -> bool {
    st["settings"]["dnd"].as_bool().unwrap_or(false)
        || in_time_window(
            local_minutes(),
            st["settings"]["dndStart"].as_str().unwrap_or(""),
            st["settings"]["dndEnd"].as_str().unwrap_or(""),
        )
}

fn chatter_in_window(st: &Value) -> bool {
    let start = st["settings"]["chatterStart"].as_str().unwrap_or("09:00");
    let end = st["settings"]["chatterEnd"].as_str().unwrap_or("22:00");
    if parse_hhmm(start).is_none() || parse_hhmm(end).is_none() {
        return true;
    }
    in_time_window(local_minutes(), start, end)
}

// ================= Windows 剪贴板（只读，按次/开关授权） =================
fn clipboard_read_text() -> Option<String> {
    const CF_UNICODETEXT: u32 = 13;
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let result = if IsClipboardFormatAvailable(CF_UNICODETEXT) != 0 {
            let h = GetClipboardData(CF_UNICODETEXT);
            if h.is_null() {
                None
            } else {
                let p = GlobalLock(h);
                if p.is_null() {
                    None
                } else {
                    let size = GlobalSize(h);
                    let len = size / 2;
                    let slice = std::slice::from_raw_parts(p as *const u16, len);
                    let end = slice.iter().position(|&c| c == 0).unwrap_or(len);
                    let text = String::from_utf16_lossy(&slice[..end]);
                    let _ = GlobalUnlock(h);
                    if text.trim().is_empty() { None } else { Some(text) }
                }
            }
        } else {
            None
        };
        let _ = CloseClipboard();
        result
    }
}

fn cut_chars(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

// ================= 外部打开（仅 http/https / 白名单可执行文件） =================
fn open_external_url(url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(url.trim()).map_err(|_| "URL 格式无效".to_string())?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("只允许打开 http / https 链接".into());
    }
    std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url.trim()])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开链接失败：{}", e))
}

fn host_allowed(host: &str, whitelist: &[String]) -> bool {
    let h = host.trim().trim_start_matches("www.").to_lowercase();
    whitelist.iter().any(|w| {
        let w = w.trim().trim_start_matches("www.").to_lowercase();
        !w.is_empty() && (h == w || h.ends_with(&format!(".{}", w)))
    })
}

fn open_whitelisted_url(st: &Value, url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(url.trim()).map_err(|_| "URL 格式无效".to_string())?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("只允许打开 http / https 链接".into());
    }
    let list: Vec<String> = st["settings"]["toolSettings"]["openWhitelist"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let host = u.host_str().unwrap_or("");
    if !host_allowed(host, &list) {
        return Err(format!("域名 {} 不在打开链接白名单中（设置 → 隐私 → 工具授权可添加）", host));
    }
    open_external_url(url)
}

fn open_whitelisted_app(app: &str) -> Result<(), String> {
    let key = app.trim().to_lowercase();
    let exe = match key.as_str() {
        "notepad" | "记事本" => "notepad.exe",
        "calc" | "计算器" => "calc.exe",
        "mspaint" | "paint" | "画图" => "mspaint.exe",
        "explorer" | "资源管理器" => "explorer.exe",
        "snippingtool" | "截图工具" => "snippingtool.exe",
        _ => return Err(format!("应用「{}」不在白名单中（仅支持 notepad / calc / mspaint / explorer / snippingtool）", app)),
    };
    std::process::Command::new(exe)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动应用失败：{}", e))
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

// FR-PET-002：置顶层级枚举「置顶且覆盖任务栏」。
// tauri 的 always_on_top 只做一次 SetWindowPos(HWND_TOPMOST)；全屏应用/新建窗口后需要
// 周期性重新提升，否则宠物会被盖住。这里把窗口提为 TOPMOST 并保持置顶位。
fn apply_top_level(win: &WebviewWindow, on: bool) {
    if let Ok(hwnd) = win.hwnd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        };
        let insert = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
        unsafe {
            SetWindowPos(
                hwnd.0 as _,
                insert,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

fn save_foreground(app: &AppHandle) {
    let hwnd = unsafe { GetForegroundWindow() };
    let ctx = app.state::<AppCtx>();
    *ctx.fg_before.lock().unwrap_or_else(|e| e.into_inner()) = hwnd as usize;
}

fn restore_foreground(app: &AppHandle) {
    let ctx = app.state::<AppCtx>();
    let h = *ctx.fg_before.lock().unwrap_or_else(|e| e.into_inner());
    if h != 0 {
        unsafe { SetForegroundWindow(h as _); }
    }
    *ctx.fg_before.lock().unwrap_or_else(|e| e.into_inner()) = 0;
}

// ================= 窗口管理 =================

fn sf(w: &WebviewWindow) -> f64 {
    w.scale_factor().unwrap_or(1.0)
}

fn show_bubble(app: &AppHandle) -> Result<(), String> {
    show_bubble_reason(app, "click")
}

struct BubblePos {
    x: i32,
    y: i32,
    below: bool,
}

// FR-BUB-004：优先显示在宠物上方，空间不足翻转至下方；水平方向夹紧于当前屏幕工作区
fn compute_bubble_pos(app: &AppHandle) -> Result<BubblePos, String> {
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
    Ok(BubblePos { x, y, below })
}

fn apply_bubble_anchor(app: &AppHandle, pos: &BubblePos) -> Result<(), String> {
    let bubble = app.get_webview_window("bubble").ok_or("bubble 窗口不存在")?;
    bubble
        .set_position(Position::Physical(PhysicalPosition::new(pos.x, pos.y)))
        .map_err(|e| e.to_string())?;
    {
        let ctx = app.state::<AppCtx>();
        *ctx.bubble_anchor_below.lock().unwrap_or_else(|e| e.into_inner()) = pos.below;
    }
    let _ = app.emit_to("bubble", "bubble-anchor", json!({"below": pos.below}));
    Ok(())
}

fn show_bubble_reason(app: &AppHandle, reason: &str) -> Result<(), String> {
    let bubble = app.get_webview_window("bubble").ok_or("bubble 窗口不存在")?;
    let was_visible = bubble_visible(app);
    let pos = if reason == "reminder" {
        compute_reminder_pos(app)?
    } else {
        compute_bubble_pos(app)?
    };
    apply_bubble_anchor(app, &pos)?;
    // 只有真正从隐藏转为显示时才记录前台窗口，避免提醒在气泡已打开时覆盖原前台句柄
    if !was_visible {
        save_foreground(app);
    }
    bubble.show().map_err(|e| e.to_string())?;
    // 主动提醒不抢焦点（NFR-INT-001）；用户主动唤起时才聚焦输入框
    if reason != "reminder" {
        bubble.set_focus().map_err(|e| e.to_string())?;
    }
    // auto：本次显示是否由后端自动弹出（此前气泡是隐藏的）。前端据此在
    // 点击提醒的「知道了 / 稍后」后关闭气泡，避免提醒处理完窗口仍留在桌面上。
    let _ = app.emit_to(
        "bubble",
        "bubble-shown",
        json!({"below": pos.below, "reason": reason, "auto": !was_visible, "center": reason == "reminder"}),
    );
    Ok(())
}

// FR-REM-002：提醒在屏幕中央附近播报，而不是只在宠物旁
fn compute_reminder_pos(app: &AppHandle) -> Result<BubblePos, String> {
    let bubble = app.get_webview_window("bubble").ok_or("bubble 窗口不存在")?;
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    let p_pos = pet.outer_position().map_err(|e| e.to_string())?;
    let p_size = pet.outer_size().map_err(|e| e.to_string())?;
    let b_size = bubble.outer_size().map_err(|e| e.to_string())?;
    let work = work_area_at(p_pos.x + p_size.width as i32 / 2, p_pos.y + p_size.height as i32 / 2);
    let w = b_size.width as i32;
    let h = b_size.height as i32;
    let x = work.left + ((work.right - work.left - w) / 2).max(0);
    let y = work.top + (((work.bottom - work.top) * 38 / 100) - h / 2).max(0);
    Ok(BubblePos {
        x: x.clamp(work.left, (work.right - w).max(work.left)),
        y: y.clamp(work.top, (work.bottom - h).max(work.top)),
        below: false,
    })
}

fn hide_bubble(app: &AppHandle) -> Result<(), String> {
    let bubble = app.get_webview_window("bubble").ok_or("bubble 窗口不存在")?;
    let was_visible = bubble_visible(app);
    bubble.hide().map_err(|e| e.to_string())?;
    if was_visible {
        restore_foreground(app);
    }
    let _ = app.emit_to("pet", "chat-state", json!({"state": "idle"}));
    Ok(())
}

fn bubble_visible(app: &AppHandle) -> bool {
    app.get_webview_window("bubble")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

// FR-BUB-005：宠物拖动时气泡跟随
#[tauri::command]
fn bubble_reposition(app: AppHandle) -> Result<(), String> {
    if !bubble_visible(&app) {
        return Ok(());
    }
    let pos = compute_bubble_pos(&app)?;
    apply_bubble_anchor(&app, &pos)
}

// FR-BUB-003：拖拽气泡左右边缘调整宽度（240–480 px），松手后重新以宠物为中心
#[tauri::command]
fn bubble_set_width(
    app: AppHandle,
    ctx: tauri::State<AppCtx>,
    width: f64,
    side: Option<String>,
    finalize: Option<bool>,
) -> Result<(), String> {
    let bubble = app.get_webview_window("bubble").ok_or("bubble 窗口不存在")?;
    let scale = sf(&bubble);
    let logical = width.clamp(240.0, 480.0);
    let new_w = (logical * scale).round() as u32;
    let pos = bubble.outer_position().map_err(|e| e.to_string())?;
    let size = bubble.outer_size().map_err(|e| e.to_string())?;
    let mut x = if side.as_deref() == Some("left") {
        pos.x + size.width as i32 - new_w as i32
    } else {
        pos.x
    };
    let work = work_area_at(x + new_w as i32 / 2, pos.y + size.height as i32 / 2);
    x = x.clamp(work.left, (work.right - new_w as i32).max(work.left));
    bubble
        .set_size(Size::Physical(PhysicalSize::new(new_w, size.height)))
        .map_err(|e| e.to_string())?;
    bubble
        .set_position(Position::Physical(PhysicalPosition::new(x, pos.y)))
        .map_err(|e| e.to_string())?;
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["bubbleW"] = json!(logical);
    }
    persist_state(&ctx);
    if finalize.unwrap_or(false) {
        bubble_reposition(app.clone())?;
    }
    Ok(())
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
    let pet = app.get_webview_window("pet");
    let dpi = pet.as_ref().map(|w| sf(w)).unwrap_or(1.0);
    let pet_scale = {
        let ctx = app.state::<AppCtx>();
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["petScale"].as_f64().unwrap_or(1.0)
    };
    let w = (120.0 * pet_scale * dpi) as i32;
    let h = (112.0 * pet_scale * dpi) as i32;
    let work = work_area_at(1600, 900);
    (work.right - w - 24, work.bottom - h - 24)
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
    let dpi = sf(&pet);
    let w = (120.0 * scale * dpi).round() as i32;
    let h = (112.0 * scale * dpi).round() as i32;
    // 越界回收（FR-PET-005）：以完整窗口尺寸夹紧，而不是只留 60px 可见
    let work = work_area_at(x + w / 2, y + h / 2);
    let x = x.clamp(work.left, (work.right - w).max(work.left));
    let y = y.clamp(work.top, (work.bottom - h).max(work.top));
    let _ = pet.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

// FR-PET-005：分辨率/显示器拓扑变化后，宠物若已不在可视工作区内则自动回收
fn ensure_pet_on_screen(app: &AppHandle) {
    let pet = match app.get_webview_window("pet") {
        Some(p) => p,
        None => return,
    };
    let (pos, size) = match (pet.outer_position(), pet.outer_size()) {
        (Ok(p), Ok(s)) => (p, s),
        _ => return,
    };
    let w = size.width as i32;
    let h = size.height as i32;
    let work = work_area_at(pos.x + w / 2, pos.y + h / 2);
    let x = pos.x.clamp(work.left, (work.right - w).max(work.left));
    let y = pos.y.clamp(work.top, (work.bottom - h).max(work.top));
    if x != pos.x || y != pos.y {
        let _ = pet.set_position(Position::Physical(PhysicalPosition::new(x, y)));
        let _ = app.emit("state-changed", json!({"origin": "backend"}));
    }
}

fn rebuild_tray(app: &AppHandle) {
    let (dnd, ct, pet_visible) = {
        let ctx = app.state::<AppCtx>();
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        let dnd = st["settings"]["dnd"].as_bool().unwrap_or(false);
        let ct = st["settings"]["clickThrough"].as_bool().unwrap_or(false);
        let pv = *ctx.pet_visible.lock().unwrap_or_else(|e| e.into_inner());
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

// DS-001 / NFR-SEC-001：旧版本曾把明文 apiKey 写入 data.json。
// 启动时将其迁移到系统凭据管理器并立即从业务库删除；即使写盘失败也不会再次进入渲染层。
fn migrate_legacy_secrets(app: &AppHandle) {
    let ctx = app.state::<AppCtx>();
    let mut changed = false;
    {
        let mut st = state_lock(&ctx);
        if let Some(providers) = st["providers"].as_array_mut() {
            for p in providers.iter_mut() {
                let legacy = p
                    .get("apiKey")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string());
                let Some(legacy) = legacy else { continue };
                let id = p["id"].as_str().unwrap_or("").to_string();
                if !legacy.is_empty()
                    && !id.is_empty()
                    && key_get(&id).map(|k| k.is_empty()).unwrap_or(true)
                {
                    let _ = key_set_kv(&id, &legacy);
                }
                if let Some(obj) = p.as_object_mut() {
                    obj.remove("apiKey");
                }
                changed = true;
            }
        }
    }
    if changed {
        persist_state(&ctx);
        log_event(app, "info", "secrets.migrated", "已将旧版明文 API Key 迁移到系统凭据管理器");
    }
}

// ================= 提醒调度（主进程线程） =================

fn start_scheduler(app: AppHandle) {
    std::thread::spawn(move || {
        let mut chatter_at = std::time::Instant::now() + Duration::from_secs(120);
        let mut health = HealthTimer {
            sit: std::time::Instant::now(),
            water: std::time::Instant::now(),
            sit_water_next: false,
        };
        let mut tick: u64 = 0;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            tick = tick.wrapping_add(1);
            tick_reminders(&app);
            tick_chatter(&app, &mut chatter_at);
            tick_health(&app, &mut health);
            // FR-REM-007：剪贴板变化感知（默认关闭，需用户在设置中显式开启）
            if tick % 2 == 0 {
                tick_clipboard(&app);
            }
            // FR-PET-005：定期检查显示器拓扑/分辨率变化后宠物是否仍在可视区
            if tick % 5 == 0 {
                ensure_pet_on_screen(&app);
            }
        }
    });
}

// 纯函数便于回归测试：返回（本次触发的提醒，状态是否变化）。
// 重复提醒必须“先触发、再顺延”，否则每日/间隔提醒会静默跳过播报。
fn process_reminders(rems: &mut Vec<Value>, now: i64) -> (Vec<Value>, bool) {
    let mut fired: Vec<Value> = vec![];
    let mut keep: Vec<Value> = vec![];
    let mut changed = false;
    for r in rems.iter() {
        let enabled = r["enabled"].as_bool().unwrap_or(true);
        let due = r["due"].as_i64().unwrap_or(0);
        if !enabled || due > now {
            keep.push(r.clone());
            continue;
        }
        let rule = r["rule"].as_str().unwrap_or("once");
        let step: Option<i64> = match rule {
            "daily" => Some(86_400_000),
            "weekly" => Some(7 * 86_400_000),
            "interval" => Some(r["intervalMin"].as_i64().unwrap_or(30).max(1) * 60_000),
            _ => None,
        };
        fired.push(r.clone());
        changed = true;
        if let Some(step) = step {
            let mut nr = r.clone();
            let mut nd = due;
            while nd <= now {
                nd += step;
            }
            nr["due"] = json!(nd);
            keep.push(nr);
        }
    }
    *rems = keep;
    (fired, changed)
}

fn tick_reminders(app: &AppHandle) {
    let now = now_ms();
    let (fired, snapshot) = {
        let ctx = app.state::<AppCtx>();
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut fired: Vec<Value> = vec![];
        let mut changed = false;
        if let Some(rems) = st["reminders"].as_array_mut() {
            let (f, c) = process_reminders(rems, now);
            fired = f;
            changed = c;
        }
        // 持锁期间仅做序列化（快照），写盘必须在锁释放后进行（见 write_state_file 注释）
        let snap = if changed {
            serde_json::to_string_pretty(&*st).ok()
        } else {
            None
        };
        (fired, snap)
    };
    if let Some(s) = snapshot {
        let ctx = app.state::<AppCtx>();
        write_state_file(&ctx, &s);
        let _ = app.emit("state-changed", json!({"origin": "backend"}));
    }
    if fired.is_empty() {
        return;
    }
    let dnd = {
        let ctx = app.state::<AppCtx>();
        let g = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        dnd_active(&g)
    };
    if dnd {
        let ctx = app.state::<AppCtx>();
        ctx.dnd_queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(fired);
        return;
    }
    for r in fired {
        push_notice(app, json!({"kind": "reminder", "reminder": r}));
        let _ = app.emit_to("pet", "reminder-fx", json!({"kind": "happy"}));
    }
    let _ = show_bubble_reason(app, "reminder");
}

fn push_notice(app: &AppHandle, notice: Value) {
    let ctx = app.state::<AppCtx>();
    ctx.pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(notice);
}

fn tick_chatter(app: &AppHandle, chatter_at: &mut std::time::Instant) {
    if std::time::Instant::now() < *chatter_at {
        return;
    }
    let interval = {
        let ctx = app.state::<AppCtx>();
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        if dnd_active(&st) || !chatter_in_window(&st) {
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

// FR-REM-006：久坐 / 喝水健康提醒（主进程计时，不依赖前端存活）。
// 命中时先在宠物旁轻提示，同时占用一个「健康提醒」槽位，避免和主动搭话叠在一起。
fn tick_health(app: &AppHandle, at: &mut HealthTimer) {
    let now = std::time::Instant::now();
    let (sit_min, water_min, enabled) = {
        let ctx = app.state::<AppCtx>();
        let st = state_lock(&ctx);
        if dnd_active(&st) {
            return;
        }
        let hs = &st["settings"]["health"];
        (
            hs["sitMin"].as_u64().unwrap_or(0),
            hs["waterMin"].as_u64().unwrap_or(0),
            hs["enabled"].as_bool().unwrap_or(false),
        )
    };
    if !enabled {
        return;
    }
    // 用户在气泡内交互即视为「离开久坐」，重置计时
    if bubble_visible(app) {
        *at = HealthTimer { sit: now, water: now, ..*at };
        return;
    }
    let mut fire: Option<&str> = None;
    if sit_min >= 5 && now.duration_since(at.sit).as_secs() >= sit_min * 60 {
        at.sit = now;
        fire = Some(if at.sit_water_next {
            "站起来走两步，顺便接杯水吧～"
        } else {
            "已经坐了一小时啦，起来活动一下肩膀和腰吧。"
        });
        at.sit_water_next = !at.sit_water_next;
    }
    if water_min >= 5 && now.duration_since(at.water).as_secs() >= water_min * 60 {
        at.water = now;
        if fire.is_none() {
            fire = Some("该喝水啦，一口气喝半杯再继续～");
        }
    }
    if let Some(text) = fire {
        let _ = app.emit_to("pet", "chatter", json!({"text": text, "kind": "health"}));
        let _ = app.emit_to("pet", "reminder-fx", json!({"kind": "happy"}));
    }
}

// FR-REM-007：检测剪贴板变化，在宠物旁给出「点我处理」提示。
// 默认关闭；只读取纯文本，最多保留 2000 字符，不写日志、不自动发送给模型。
fn tick_clipboard(app: &AppHandle) {
    let enabled = {
        let ctx = app.state::<AppCtx>();
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["clipboardWatch"].as_bool().unwrap_or(false) && !dnd_active(&st)
    };
    if !enabled {
        return;
    }
    let ctx = app.state::<AppCtx>();
    let now_text = clipboard_read_text().unwrap_or_default();
    let text: String = now_text.chars().take(2000).collect();
    if text.trim().len() < 4 {
        return;
    }
    {
        let mut last = ctx.clipboard_last.lock().unwrap_or_else(|e| e.into_inner());
        if *last == text {
            return;
        }
        *last = text.clone();
    }
    let _ = app.emit_to(
        "pet",
        "clipboard-changed",
        json!({"preview": cut_chars(text.trim(), 80)}),
    );
    let _ = app.emit_to("pet", "reminder-fx", json!({"kind": "happy"}));
}

// ================= 工具调用（FR-TOOL-001/003/004） =================

fn tool_defs_raw() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "create_reminder",
                "description": "创建一条本地定时提醒。用户说「提醒我…」时调用。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "content": {"type": "string", "description": "提醒内容，如「喝水」"},
                        "minutes": {"type": "number", "description": "多少分钟后提醒（相对时间）"},
                        "at": {"type": "string", "description": "本地时间 HH:MM（绝对时间，若已过则顺延到明天）"},
                        "rule": {"type": "string", "enum": ["once", "daily", "weekly", "interval"], "description": "重复规则，默认 once"},
                        "intervalMin": {"type": "number", "description": "rule=interval 时的间隔分钟数"}
                    },
                    "required": ["content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_clipboard",
                "description": "读取当前剪贴板中的纯文本。敏感能力，首次调用需用户授权。",
                "parameters": {"type": "object", "properties": {"max_chars": {"type": "number", "description": "最多返回字符数，默认 2000"}}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "open_url",
                "description": "在默认浏览器中打开链接。仅允许白名单域名。",
                "parameters": {"type": "object", "properties": {"url": {"type": "string", "description": "完整 http/https 链接"}}, "required": ["url"]}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "open_app",
                "description": "启动白名单内的本地应用：notepad / calc / mspaint / explorer / snippingtool。",
                "parameters": {"type": "object", "properties": {"app": {"type": "string", "description": "应用名"}}, "required": ["app"]}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "查询城市当前天气（Open-Meteo，无需 API Key）。",
                "parameters": {"type": "object", "properties": {"city": {"type": "string", "description": "城市名，如 杭州"}}, "required": ["city"]}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "使用 DuckDuckGo 做一次网页搜索，返回标题、链接与摘要。",
                "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "搜索关键词"}}, "required": ["query"]}
            }
        }
    ])
}

// 按协议把工具定义转成请求体片段（FR-TOOL-001、NFR-INT-004）
fn tool_defs(ptype: &str) -> Value {
    let openai = tool_defs_raw();
    match ptype {
        "anthropic" => {
            let list: Vec<Value> = openai
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|t| {
                            json!({
                                "name": t["function"]["name"],
                                "description": t["function"]["description"],
                                "input_schema": t["function"]["parameters"],
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            json!(list)
        }
        "gemini" => {
            let list: Vec<Value> = openai
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|t| {
                            json!({
                                "name": t["function"]["name"],
                                "description": t["function"]["description"],
                                "parameters": t["function"]["parameters"],
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            json!([{ "functionDeclarations": list }])
        }
        _ => openai,
    }
}

// 把客户端解析到的工具调用还原为各协议的 assistant 消息结构
fn tool_calls_payload(ptype: &str, calls: &[ToolCall]) -> Value {
    match ptype {
        "anthropic" => json!(calls
            .iter()
            .map(|c| {
                json!({
                    "type": "tool_use",
                    "id": c.id,
                    "name": c.name,
                    "input": tool_call_args(c),
                })
            })
            .collect::<Vec<_>>()),
        "gemini" => json!(calls
            .iter()
            .map(|c| json!({ "functionCall": { "name": c.name, "args": tool_call_args(c) } }))
            .collect::<Vec<_>>()),
        _ => json!(calls
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "type": "function",
                    "function": { "name": c.name, "arguments": if c.args.trim().is_empty() { "{}".to_string() } else { c.args.clone() } },
                })
            })
            .collect::<Vec<_>>()),
    }
}

fn tool_label(name: &str) -> &'static str {
    match name {
        "create_reminder" => "创建提醒",
        "read_clipboard" => "读取剪贴板",
        "open_url" => "打开链接",
        "open_app" => "打开应用",
        "get_weather" => "查询天气",
        "web_search" => "联网搜索",
        _ => "工具",
    }
}

fn tool_auth_state(app: &AppHandle, name: &str) -> String {
    let ctx = app.state::<AppCtx>();
    let st = state_lock(&ctx);
    st["settings"]["toolSettings"]["auth"][name]
        .as_str()
        .unwrap_or("ask")
        .to_string()
}

fn set_tool_auth(app: &AppHandle, name: &str, value: &str) {
    let ctx = app.state::<AppCtx>();
    {
        let mut st = state_lock(&ctx);
        if !st["settings"]["toolSettings"]["auth"].is_object() {
            st["settings"]["toolSettings"]["auth"] = json!({});
        }
        st["settings"]["toolSettings"]["auth"][name] = json!(value);
    }
    persist_state(&ctx);
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
}

// FR-TOOL-003 / AC-012：首次调用弹窗授权；支持「始终允许/拒绝」，可随时撤销。
async fn request_tool_auth(app: &AppHandle, name: &str, args: &Value) -> bool {
    let state = tool_auth_state(app, name);
    if state == "allow" {
        return true;
    }
    if state == "deny" {
        let _ = app.emit_to(
            "bubble",
            "chat-tool",
            json!({"tool": name, "label": tool_label(name), "ok": false, "summary": "此前已被用户拒绝授权"}),
        );
        return false;
    }
    let id = format!("tool-{}-{}", now_ms(), rand_suffix());
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let ctx = app.state::<AppCtx>();
        let mut wait = ctx.tool_wait.lock().unwrap_or_else(|e| e.into_inner());
        wait.insert(id.clone(), ToolWait { id: id.clone(), name: name.to_string(), tx });
    }
    let _ = show_bubble_reason(app, "tool");
    let preview = cut_chars(&args.to_string(), 400);
    let _ = app.emit_to(
        "bubble",
        "tool-auth-request",
        json!({
            "id": id,
            "tool": name,
            "label": tool_label(name),
            "args": preview,
            "description": match name {
                "read_clipboard" => "读取你当前剪贴板中的纯文本内容（内容会作为工具结果发回模型）",
                "open_url" => "在你的默认浏览器中打开一个链接（仅限白名单域名）",
                "open_app" => "在你的电脑上启动一个白名单应用",
                "create_reminder" => "在本地创建一条定时提醒",
                "get_weather" => "向 Open-Meteo 发送城市名以查询天气",
                "web_search" => "向 DuckDuckGo 发送搜索关键词",
                _ => "执行一个本地工具",
            },
            // NFR-PRV-003：敏感能力要明确告知用户在本地还是出站
            "egress": match name {
                "get_weather" | "web_search" => "outbound",
                "open_url" => "outbound",
                _ => "local",
            }
        }),
    );
    match tokio::time::timeout(Duration::from_secs(120), rx).await {
        Ok(Ok(d)) => {
            if d.remember {
                set_tool_auth(app, name, if d.allow { "allow" } else { "deny" });
            }
            d.allow
        }
        _ => {
            let ctx = app.state::<AppCtx>();
            ctx.tool_wait
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            let _ = app.emit_to(
                "bubble",
                "chat-tool",
                json!({"tool": name, "label": tool_label(name), "ok": false, "summary": "授权超时，已拒绝执行"}),
            );
            false
        }
    }
}

fn weather_text(code: i64) -> &'static str {
    match code {
        0 => "晴",
        1..=3 => "多云",
        45 | 48 => "雾",
        51..=57 => "毛毛雨",
        61..=67 => "雨",
        71..=77 | 85 | 86 => "雪",
        80..=82 => "阵雨",
        95..=99 => "雷雨",
        _ => "未知天气",
    }
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
}

async fn execute_tool(app: &AppHandle, conf: &ProvConf, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "create_reminder" => {
            let content = args["content"].as_str().unwrap_or("提醒").trim().to_string();
            if content.is_empty() {
                return Err("提醒内容为空".into());
            }
            let now = now_ms();
            let mut due = args["due"].as_i64().unwrap_or(0);
            if due <= 0 {
                if let Some(at) = args["at"].as_str().and_then(parse_hhmm) {
                    let cur = local_minutes();
                    let mut diff = at - cur;
                    if diff <= 0 {
                        diff += 1440;
                    }
                    due = now + diff * 60_000;
                } else {
                    let minutes = args["minutes"].as_f64().unwrap_or(25.0).clamp(0.1, 10080.0);
                    due = now + (minutes * 60_000.0).round() as i64;
                }
            }
            let rule = args["rule"].as_str().unwrap_or("once");
            let rule = if ["daily", "weekly", "interval"].contains(&rule) { rule } else { "once" };
            let interval = args["intervalMin"].as_i64().unwrap_or(30).clamp(1, 10080);
            let rem = json!({
                "id": format!("rem-tool-{}", now),
                "content": cut_chars(&content, 60),
                "due": due,
                "rule": rule,
                "intervalMin": if rule == "interval" { json!(interval) } else { Value::Null },
                "enabled": true,
                "createdBy": "tool"
            });
            {
                let ctx = app.state::<AppCtx>();
                let mut st = state_lock(&ctx);
                if !st["reminders"].is_array() {
                    st["reminders"] = json!([]);
                }
                st["reminders"].as_array_mut().unwrap().push(rem.clone());
            }
            let ctx = app.state::<AppCtx>();
            persist_state(&ctx);
            let _ = app.emit("state-changed", json!({"origin": "backend"}));
            Ok(json!({"created": true, "reminder": rem}))
        }
        "read_clipboard" => {
            let text = clipboard_read_text().ok_or("剪贴板中没有可读取的纯文本")?;
            let max = args["max_chars"].as_i64().unwrap_or(2000).clamp(1, 8000) as usize;
            let cut_text = cut_chars(&text, max);
            Ok(json!({"text": cut_text, "length": text.chars().count(), "truncated": text.chars().count() > max}))
        }
        "open_url" => {
            let url = args["url"].as_str().unwrap_or("");
            let ctx = app.state::<AppCtx>();
            {
                let st = state_lock(&ctx);
                open_whitelisted_url(&st, url)?;
            }
            Ok(json!({"opened": true, "url": url}))
        }
        "open_app" => {
            let name = args["app"].as_str().unwrap_or("");
            open_whitelisted_app(name)?;
            Ok(json!({"opened": true, "app": name}))
        }
        "get_weather" => {
            let city = args["city"].as_str().unwrap_or("").trim();
            if city.is_empty() {
                return Err("缺少城市名".into());
            }
            let client = client_for(app, conf)?;
            let geo_url = format!(
                "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=zh&format=json",
                urlencode(city)
            );
            let geo: Value = client
                .get(&geo_url)
                .timeout(Duration::from_secs(12))
                .send()
                .await
                .map_err(|e| format!("地理编码请求失败：{}", e))?
                .json()
                .await
                .map_err(|e| format!("地理编码响应无效：{}", e))?;
            let loc = &geo["results"][0];
            if loc.is_null() {
                return Err(format!("没有找到城市「{}」", city));
            }
            let (lat, lon) = (loc["latitude"].as_f64().unwrap_or(0.0), loc["longitude"].as_f64().unwrap_or(0.0));
            let label = loc["name"].as_str().unwrap_or(city);
            let url = format!(
                "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m&timezone=auto",
                lat, lon
            );
            let j: Value = client
                .get(&url)
                .timeout(Duration::from_secs(12))
                .send()
                .await
                .map_err(|e| format!("天气请求失败：{}", e))?
                .json()
                .await
                .map_err(|e| format!("天气响应无效：{}", e))?;
            let cur = &j["current"];
            let temp = cur["temperature_2m"].as_f64().unwrap_or(0.0);
            let hum = cur["relative_humidity_2m"].as_f64().unwrap_or(0.0);
            let wind = cur["wind_speed_10m"].as_f64().unwrap_or(0.0);
            let code = cur["weather_code"].as_i64().unwrap_or(-1);
            Ok(json!({
                "city": label,
                "weather": weather_text(code),
                "temperature_c": temp,
                "humidity": hum,
                "wind_kmh": wind,
                "summary": format!("{}：{}，{}°C，湿度 {}%，风速 {} km/h", label, weather_text(code), temp, hum, wind)
            }))
        }
        "web_search" => {
            let q = args["query"].as_str().unwrap_or("").trim();
            if q.is_empty() {
                return Err("缺少搜索关键词".into());
            }
            let client = client_for(app, conf)?;
            let url = format!("https://html.duckduckgo.com/html/?q={}", urlencode(q));
            let body = client
                .get(&url)
                .header("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) DeskPet/2.0")
                .timeout(Duration::from_secs(12))
                .send()
                .await
                .map_err(|e| format!("搜索请求失败：{}", e))?
                .text()
                .await
                .map_err(|e| format!("搜索结果读取失败：{}", e))?;
            let mut results: Vec<Value> = vec![];
            for chunk in body.split("result__a").skip(1) {
                if results.len() >= 5 {
                    break;
                }
                let href = chunk
                    .split("href=\"")
                    .nth(1)
                    .and_then(|s| s.split('"').next())
                    .unwrap_or("");
                let title = chunk
                    .split('>')
                    .nth(1)
                    .and_then(|s| s.split("</a>").next())
                    .map(html_unescape)
                    .unwrap_or_default();
                if href.starts_with("http") && !title.trim().is_empty() {
                    results.push(json!({"title": title.trim(), "url": href}));
                }
            }
            Ok(json!({"query": q, "count": results.len(), "results": results}))
        }
        _ => Err(format!("未知工具：{}", name)),
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ================= 工具调用增量解析（OpenAI SSE / Ollama NDJSON） =================

#[derive(Default)]
struct ToolAcc {
    map: std::collections::BTreeMap<i64, ToolCall>,
}

struct ToolCall {
    id: String,
    name: String,
    args: String,
}

impl ToolAcc {
    fn push_openai(&mut self, deltas: &Value) {
        let Some(arr) = deltas.as_array() else { return };
        for d in arr {
            let idx = d["index"].as_i64().unwrap_or(0);
            let e = self.map.entry(idx).or_insert_with(|| ToolCall {
                id: String::new(),
                name: String::new(),
                args: String::new(),
            });
            if let Some(id) = d["id"].as_str() {
                if !id.is_empty() {
                    e.id = id.to_string();
                }
            }
            if let Some(n) = d["function"]["name"].as_str() {
                if !n.is_empty() && !e.name.contains(n) {
                    e.name.push_str(n);
                }
            }
            if let Some(a) = d["function"]["arguments"].as_str() {
                e.args.push_str(a);
            }
        }
    }

    fn push_ollama(&mut self, calls: &Value) {
        let Some(arr) = calls.as_array() else { return };
        for (i, c) in arr.iter().enumerate() {
            let idx = i as i64;
            let e = self.map.entry(idx).or_insert_with(|| ToolCall {
                id: format!("ollama-{}-{}", now_ms(), i),
                name: String::new(),
                args: String::new(),
            });
            if e.name.is_empty() {
                e.name = c["function"]["name"].as_str().unwrap_or("").to_string();
            }
            let a = &c["function"]["arguments"];
            e.args = if let Some(s) = a.as_str() {
                s.to_string()
            } else if a.is_object() {
                a.to_string()
            } else {
                String::new()
            };
        }
    }

    fn into_calls_snapshot(&self) -> Vec<&ToolCall> {
        self.map.values().filter(|c| !c.name.trim().is_empty()).collect()
    }

    fn into_calls(self) -> Vec<ToolCall> {
        self.map
            .into_values()
            .filter(|c| !c.name.trim().is_empty())
            .collect()
    }
}

fn sse_tool_deltas(line: &str) -> Option<Value> {
    let payload = line.strip_prefix("data:")?.trim();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }
    let j: Value = serde_json::from_str(payload).ok()?;
    let tc = &j["choices"][0]["delta"]["tool_calls"];
    if tc.is_array() {
        Some(tc.clone())
    } else {
        None
    }
}

fn ollama_tool_calls(line: &str) -> Option<Value> {
    let j: Value = serde_json::from_str(line).ok()?;
    let tc = &j["message"]["tool_calls"];
    if tc.is_array() {
        Some(tc.clone())
    } else {
        None
    }
}

fn nonstream_tool_calls(j: &Value) -> Option<Value> {
    let tc = if j["choices"][0]["message"]["tool_calls"].is_array() {
        j["choices"][0]["message"]["tool_calls"].clone()
    } else {
        j["message"]["tool_calls"].clone()
    };
    if tc.is_array() {
        Some(tc)
    } else {
        None
    }
}

fn tool_call_args(c: &ToolCall) -> Value {
    serde_json::from_str(&c.args).unwrap_or_else(|_| json!({}))
}

// ================= 聊天流式（主进程，FR-MDL-013） =================

#[derive(serde::Deserialize, Clone, Default)]
struct ChatMsg {
    role: String,
    content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(serde::Deserialize)]
struct ChatArgs {
    #[serde(rename = "providerId")]
    provider_id: String,
    messages: Vec<ChatMsg>,
}

#[derive(Clone)]
struct ProvConf {
    ptype: String,
    base: String,
    model: String,
    key: String,
    headers_raw: String,
    proxy: String,
    proxy_type: String,
    temp: f64,
    top_p: f64,
    max_tokens: i64,
    ctx_tokens: i64,
    timeout: i64,
    retry: i64,
    stream: bool,
    // Ollama 原生协议标记：/v1（OpenAI 兼容层）无法传 num_ctx，会导致服务端按
    // OLLAMA_CONTEXT_LENGTH（常见 131072）或模型原生 262144 上下文分配 KV cache，
    // 在内存紧张的机器上与视觉投影层叠加后直接 OOM。
    ollama: bool,
}

// 锁中毒防护：任一持锁线程 panic 后应用仍可继续运行（NFR-CON-003 单点异常不得导致进程崩溃）
// 请求是否已被用户停止，或已被更新的一轮请求取代
fn chat_aborted(app: &AppHandle, seq: u64) -> bool {
    let ctx = app.state::<AppCtx>();
    ctx.stop.load(Ordering::SeqCst)
        || ctx.chat_cancelled.load(Ordering::SeqCst)
        || ctx.chat_seq.load(Ordering::SeqCst) != seq
}

fn bump_chat_seq(app: &AppHandle) -> u64 {
    let ctx = app.state::<AppCtx>();
    let n = ctx.chat_seq.fetch_add(1, Ordering::SeqCst) + 1;
    ctx.chat_cancelled.store(false, Ordering::SeqCst);
    ctx.stop.store(false, Ordering::SeqCst);
    n
}

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
    let base = p["baseUrl"].as_str().unwrap_or("").to_string();
    Ok(ProvConf {
        ptype: p["type"].as_str().unwrap_or("openai").to_string(),
        ollama: is_ollama_endpoint(&base),
        base,
        model: p["model"].as_str().unwrap_or("").to_string(),
        key: key_get(id).unwrap_or_default(),
        headers_raw: p["headers"].as_str().unwrap_or("").to_string(),
        proxy: p["proxy"].as_str().unwrap_or("").to_string(),
        proxy_type: p["proxyType"].as_str().unwrap_or("auto").to_string(),
        temp: params["temp"].as_f64().unwrap_or(0.8),
        top_p: params["topP"].as_f64().unwrap_or(1.0),
        max_tokens: params["maxTokens"].as_i64().unwrap_or(1024),
        ctx_tokens: params["ctxTokens"].as_i64().unwrap_or(8000).max(512),
        timeout: params["timeout"].as_i64().unwrap_or(30).clamp(5, 120),
        retry: params["retry"].as_i64().unwrap_or(2).clamp(0, 5),
        stream: params["stream"].as_bool().unwrap_or(true),
    })
}

fn join_url(base: &str, path: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    format!("{}{}", b, path)
}

// 应用内 HTTP / SOCKS5 代理（FR-MDL-008）已于 SRS v1.2 移出产品范围：设置页不提供输入入口，
// 普通用户请求直接复用应用级客户端，遵循操作系统与 reqwest 的环境变量代理设置。
// 这里仅保留对历史数据中 proxy / proxyType 字段的兼容读取，避免旧配置导致请求失败或迁移丢失；
// 不新增入口、不写入默认值（见默认状态与 state_save 白名单）。
// 未配置时复用应用级客户端；配置后按代理类型构建独立客户端（reqwest socks feature）。
fn client_for(app: &AppHandle, conf: &ProvConf) -> Result<reqwest::Client, String> {
    let proxy = conf.proxy.trim();
    if proxy.is_empty() {
        return Ok(app.state::<AppCtx>().http.clone());
    }
    let builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(15));
    let ptype = conf.proxy_type.trim().to_lowercase();
    let p = if ptype == "http" || ptype == "https" {
        reqwest::Proxy::http(proxy).map_err(|e| format!("HTTP 代理配置无效：{}", e))?
    } else if ptype == "socks5" || ptype == "socks5h" || proxy.to_lowercase().starts_with("socks5") {
        reqwest::Proxy::all(proxy).map_err(|e| format!("SOCKS5 代理配置无效：{}", e))?
    } else {
        reqwest::Proxy::all(proxy).map_err(|e| format!("代理配置无效：{}", e))?
    };
    builder
        .proxy(p)
        .build()
        .map_err(|e| format!("代理客户端构建失败：{}", e))
}

fn is_azure_endpoint(base: &str) -> bool {
    base.to_lowercase().contains(".openai.azure.com")
}

// 识别本地/远程 Ollama：默认端口 11434、主机名含 ollama、或 BaseURL 已带 /api 路径。
// 命中后改用原生 /api/chat，才能显式传 options.num_ctx（OpenAI 兼容层会忽略该字段）。
fn is_ollama_endpoint(base: &str) -> bool {
    let b = base.trim();
    if b.is_empty() {
        return false;
    }
    match reqwest::Url::parse(b) {
        Ok(u) => {
            if u.port() == Some(11434) {
                return true;
            }
            let host = u.host_str().unwrap_or("").to_lowercase();
            if host.contains("ollama") {
                return true;
            }
            let path = u.path().trim_end_matches('/').to_lowercase();
            path.ends_with("/api") || path.contains("/api/")
        }
        Err(_) => false,
    }
}

// http://host:11434/v1 → http://host:11434（去掉 OpenAI 兼容层后缀）
fn ollama_root(base: &str) -> String {
    let mut b = base.trim().trim_end_matches('/').to_string();
    for suf in ["/v1", "/api"] {
        if b.to_lowercase().ends_with(suf) {
            b.truncate(b.len() - suf.len());
            b = b.trim_end_matches('/').to_string();
        }
    }
    b
}

fn ollama_chat_url(base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if b.to_lowercase().ends_with("/api/chat") {
        return b.to_string();
    }
    format!("{}/api/chat", ollama_root(base))
}

// Ollama 的 num_ctx 上限：过大的上下文会在加载时把 KV cache 撑爆内存（默认 32K 足够桌面宠物对话）
fn effective_ctx_tokens(conf: &ProvConf) -> i64 {
    if conf.ollama {
        conf.ctx_tokens.clamp(512, 32768)
    } else {
        conf.ctx_tokens
    }
}

// Azure OpenAI 需要 api-version 查询参数；用户填写的 BaseURL 可能已带部署路径
fn azure_url(base: &str, path: &str) -> String {
    let b = base.trim();
    let mut url = if b.trim_end_matches('/').contains("/chat/completions") {
        b.trim_end_matches('/').to_string()
    } else {
        join_url(b, path)
    };
    if !url.contains("api-version=") {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str("api-version=2024-02-01");
    }
    url
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

fn openai_messages(messages: &[ChatMsg]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| {
            let mut o = json!({"role": m.role, "content": m.content});
            if let Some(tc) = &m.tool_calls {
                o["tool_calls"] = tc.clone();
            }
            // OpenAI 协议不允许 assistant 消息 content 为 null 且同时含 tool_calls 时缺省字段
            if m.role == "assistant" && m.content.is_empty() && m.tool_calls.is_some() {
                o["content"] = Value::Null;
            }
            if let Some(id) = &m.tool_call_id {
                o["tool_call_id"] = json!(id);
            }
            if let Some(n) = &m.name {
                o["name"] = json!(n);
                o["tool_name"] = json!(n);
            }
            o
        })
        .collect()
}

// Anthropic：tool_use / tool_result 必须以 content block 形式出现（FR-TOOL-001）
fn anthropic_block(m: &ChatMsg) -> Vec<Value> {
    if m.role == "assistant" {
        if let Some(calls) = &m.tool_calls {
            return calls
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|c| {
                            json!({
                                "type": "tool_use",
                                "id": c["id"].as_str().unwrap_or(""),
                                "name": c["name"].as_str().unwrap_or(""),
                                "input": c["input"].clone(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
    }
    if m.role == "tool" {
        return vec![json!({
            "type": "tool_result",
            "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
            "content": m.content,
        })];
    }
    vec![]
}

// Gemini：functionCall / functionResponse 同样只能出现在 parts 里
fn gemini_parts(m: &ChatMsg) -> Vec<Value> {
    if m.role == "tool" {
        return vec![json!({
            "functionResponse": {
                "name": m.name.clone().unwrap_or_default(),
                "response": serde_json::from_str::<Value>(&m.content).unwrap_or_else(|_| json!({"result": m.content})),
            }
        })];
    }
    if m.role == "assistant" {
        if let Some(calls) = &m.tool_calls {
            return calls
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|c| json!({"functionCall": {"name": c["name"], "args": c["input"]}}))
                        .collect()
                })
                .unwrap_or_default();
        }
    }
    if m.content.is_empty() {
        vec![]
    } else {
        vec![json!({"text": m.content})]
    }
}

fn build_request(
    app: &AppHandle,
    conf: &ProvConf,
    messages: &[ChatMsg],
    tools: Option<&Value>,
) -> Result<reqwest::RequestBuilder, String> {
    let client = client_for(app, conf)?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    let url;
    let mut body;
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
            // (role, 纯文本片段, 结构化 content blocks)；含工具调用时走 blocks 形式
            let mut convo: Vec<(String, String, Vec<Value>)> = vec![];
            for m in messages {
                if m.role == "system" {
                    sys.push_str(&m.content);
                    sys.push('\n');
                    continue;
                }
                let block = anthropic_block(m);
                let role = if m.role == "assistant" { "assistant" } else { "user" };
                if let Some(last) = convo.last_mut() {
                    if last.0 == role {
                        if !m.content.is_empty() {
                            last.1.push('\n');
                            last.1.push_str(&m.content);
                        }
                        last.2.extend(block);
                        continue;
                    }
                }
                convo.push((role.to_string(), m.content.clone(), block));
            }
            while convo.first().map(|m| m.0 != "user").unwrap_or(false) {
                convo.remove(0);
            }
            let msgs: Vec<Value> = convo
                .into_iter()
                .map(|(r, c, blocks)| {
                    if blocks.is_empty() {
                        json!({"role": r, "content": c})
                    } else {
                        json!({"role": r, "content": blocks})
                    }
                })
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
                    let parts = gemini_parts(m);
                    if let Some(last) = contents.last_mut() {
                        if last["role"].as_str() == Some(role) {
                            let arr = last["parts"].as_array_mut().unwrap();
                            for p in parts {
                                arr.push(p);
                            }
                            continue;
                        }
                    }
                    contents.push(json!({"role": role, "parts": parts}));
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
            let azure = is_azure_endpoint(&conf.base);
            let msgs = openai_messages(messages);
            if conf.ollama && !azure && !conf.base.trim_end_matches('/').ends_with("/chat/completions") {
                // 原生 Ollama：显式限制 num_ctx（KV cache 上限）与 num_predict，
                // 并关闭思考模式，避免 262144 原生上下文 + 视觉投影层把内存撑爆
                url = ollama_chat_url(&conf.base);
                if let Some(v) = key_header(conf, true)? {
                    headers.insert("authorization", v);
                }
                body = json!({
                    "model": conf.model,
                    "messages": msgs,
                    "stream": conf.stream,
                    "keep_alive": "10m",
                    "think": false,
                    "options": {
                        "num_ctx": effective_ctx_tokens(conf),
                        "num_predict": conf.max_tokens,
                        "temperature": conf.temp,
                        "top_p": conf.top_p
                    }
                });
            } else {
                url = if azure {
                    azure_url(&conf.base, "/chat/completions")
                } else if conf.base.trim_end_matches('/').ends_with("/chat/completions") {
                    conf.base.trim_end_matches('/').to_string()
                } else {
                    join_url(&conf.base, "/chat/completions")
                };
                if azure {
                    // Azure OpenAI 使用 api-key 头，Bearer 仅适用于 Entra ID 令牌
                    if let Some(v) = key_header(conf, false)? {
                        headers.insert("api-key", v);
                    }
                } else if let Some(v) = key_header(conf, true)? {
                    headers.insert("authorization", v);
                }
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
    }
    apply_custom_headers(&mut headers, &conf.headers_raw);
    // FR-TOOL-001：工具定义按协议注入；tool_choice 由服务端决定何时调用
    if let Some(t) = tools {
        if t.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            body["tools"] = t.clone();
            if conf.ptype != "anthropic" && conf.ptype != "gemini" {
                body["tool_choice"] = json!("auto");
            }
        }
    }
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

// ================= Ollama 原生协议（/api/chat，NDJSON） =================

fn ollama_delta(line: &str) -> Option<String> {
    let j: Value = serde_json::from_str(line).ok()?;
    let t = j["message"]["content"]
        .as_str()
        .or_else(|| j["response"].as_str())
        .unwrap_or("");
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn ollama_line_error(line: &str) -> Option<String> {
    let j: Value = serde_json::from_str(line).ok()?;
    j["error"].as_str().map(|s| s.to_string())
}

fn ollama_text(j: &Value) -> String {
    j["message"]["content"]
        .as_str()
        .or_else(|| j["response"].as_str())
        .unwrap_or("")
        .to_string()
}

// 本地模型加载 OOM：Ollama 会在 500 响应里原样带回 llama-server 的启动错误
fn is_local_oom(msg: &str) -> bool {
    let l = msg.to_lowercase();
    [
        "out-of-memory",
        "out of memory",
        "failed to allocate",
        "ggml_assert",
        "llama-server startup failed",
        "terminateprocess",
        "内存不足",
    ]
    .iter()
    .any(|k| l.contains(k))
}

// 仅当确实是 OOM 时才附上本地模型的排障建议，其它错误（模型不存在等）原样透出
fn ollama_error_text(model: &str, err: &str) -> String {
    if is_local_oom(err) {
        local_oom_hint(model, err)
    } else {
        err.to_string()
    }
}

fn local_oom_hint(model: &str, detail: &str) -> String {
    format!(
        "本地模型「{m}」加载失败：内存不足（Ollama 启动 llama-server 时无法分配内存）。\n\
         这是 Ollama 在加载模型时系统可用内存不够，不是 DeskPet 的请求格式问题。可依次尝试：\n\
         ① 关闭其他占用内存较多的程序后重试；\n\
         ② 设置 → 模型 → 该供应商：把「上下文 Token 上限」调到 4096 或更低（决定 KV cache 大小）；\n\
         ③ 换更小的模型；qwen3.5 等视觉模型即使纯文本对话也要额外加载 600MB+ 图像投影层；\n\
         ④ 终端执行 ollama stop {m} 释放已加载模型，或重启 Ollama 服务。\n\n\
         原始错误：{d}",
        m = model,
        d = detail
    )
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

fn retry_after_secs(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

fn is_context_too_long(msg: &str) -> bool {
    let l = msg.to_lowercase();
    [
        "context length",
        "context_length",
        "maximum context",
        "context window",
        "too many tokens",
        "token limit",
        "reduce the length",
        "上下文长度",
        "上下文超",
        "超出最大",
    ]
    .iter()
    .any(|k| l.contains(k))
}

// FR-MDL-012：上下文超长时自动裁剪较早消息（保留全部 system 消息）后重试
fn trim_context(messages: &mut Vec<ChatMsg>) -> bool {
    let mut removed = 0;
    let mut i = 0;
    while i < messages.len() && removed < 2 {
        if messages[i].role != "system" {
            messages.remove(i);
            removed += 1;
        } else {
            i += 1;
        }
    }
    removed > 0
}

// 粗粒度 token 估算：CJK 约 1 字 1 token，其余约 4 字符 1 token
fn est_tokens(s: &str) -> i64 {
    let mut cjk = 0i64;
    let mut other = 0i64;
    for c in s.chars() {
        let u = c as u32;
        if (0x2e80..=0x9fff).contains(&u) || (0xff00..=0xffef).contains(&u) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + (other + 3) / 4
}

// 发送前按 ctxTokens 预算裁剪（保留 system 与最近消息），与前端双重保险
fn trim_to_budget(messages: &mut Vec<ChatMsg>, budget: i64) -> bool {
    let total = |ms: &Vec<ChatMsg>| ms.iter().map(|m| est_tokens(&m.content)).sum::<i64>();
    let mut trimmed = false;
    while messages.len() > 1 && total(messages) > budget {
        match messages.iter().position(|m| m.role != "system") {
            Some(idx) => {
                messages.remove(idx);
                trimmed = true;
            }
            None => break,
        }
    }
    trimmed
}

fn net_error_msg(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "连接超时：请检查网络或在模型参数中调大超时时间".into()
    } else if e.is_connect() {
        "无法连接到服务地址：请检查 BaseURL 与网络（本地服务请确认已启动）".into()
    } else {
        format!("网络错误：{}", e)
    }
}

fn host_of(base: &str) -> String {
    reqwest::Url::parse(base.trim())
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "invalid-url".into())
}

// NFR-SEC-003 / 3.2.4：出站记录只写域名与状态，绝不写密钥、请求头或请求体。
// v1.2 起隐私页不再提供逐请求审计表（SRS v1.2 已移出产品范围），
// 这里改为写入脱敏的本地 JSON Lines 日志，排障时可直接查看 deskpet.log。
fn record_outbound(
    app: &AppHandle,
    host: &str,
    ptype: &str,
    model: &str,
    status: Option<u16>,
    ok: bool,
    latency_ms: u64,
    attempt: i64,
) {
    let msg = json!({
        "host": host,
        "type": ptype,
        "model": model,
        "status": status,
        "ok": ok,
        "latency": latency_ms,
        "attempt": attempt,
    })
    .to_string();
    log_event(app, if ok { "info" } else { "warn" }, "outbound", &msg);
}

// 一次请求的流式结果：正文增量已通过事件推给气泡，这里只回传工具调用。
struct TurnOutcome {
    text: String,
    calls: Vec<ToolCall>,
}

// FR-TOOL-001/004 与 FR-MDL-009/010/011/012：单轮请求 + 重试/裁剪 + 流式解析。
// 工具调用循环在 run_chat 中完成，本函数只负责「发出一次请求并读完响应」。
#[allow(clippy::too_many_arguments)]
async fn run_turn(
    app: &AppHandle,
    seq: u64,
    conf: &ProvConf,
    messages: &mut Vec<ChatMsg>,
    tools: Option<&Value>,
) -> Result<TurnOutcome, (Option<u16>, String, Option<u64>)> {
    let emit_notice = |msg: String| {
        let _ = app.emit_to("bubble", "chat-notice", json!({"message": msg}));
    };
    let want_stream = conf.stream;
    let host = host_of(&conf.base);
    // FR-MDL-012：发送前按供应商 ctxTokens 预算裁剪，预留输出空间
    let ctx_budget = effective_ctx_tokens(conf);
    let budget = (ctx_budget - conf.max_tokens).max(256);
    if trim_to_budget(messages, budget) {
        emit_notice(format!("上下文超出 {} tokens 预算，已自动裁剪较早消息", ctx_budget));
    }
    let mut attempt: i64 = 0;
    let mut trims = 0;
    loop {
        if chat_aborted(app, seq) {
            return Err((None, "__stopped__".into(), None));
        }
        let rb = match build_request(app, conf, messages, tools) {
            Ok(rb) => rb,
            Err(e) => return Err((None, e, None)),
        };
        let t0 = std::time::Instant::now();
        // FR-MDL-009：超时参数作用于建连/首字节；流式阶段另用空闲超时
        let sent = tokio::time::timeout(Duration::from_secs(conf.timeout as u64), rb.send()).await;
        let resp = match sent {
            Err(_) => {
                record_outbound(app, &host, &conf.ptype, &conf.model, None, false, t0.elapsed().as_millis() as u64, attempt);
                if attempt < conf.retry {
                    attempt += 1;
                    emit_notice(format!("连接超时，正在重试（{}/{}）…", attempt, conf.retry));
                    tokio::time::sleep(Duration::from_millis(300u64 << attempt.min(3))).await;
                    continue;
                }
                return Err((None, "连接超时：请检查网络或在模型参数中调大超时时间".into(), None));
            }
            Ok(Err(e)) => {
                record_outbound(app, &host, &conf.ptype, &conf.model, None, false, t0.elapsed().as_millis() as u64, attempt);
                let msg = net_error_msg(&e);
                if attempt < conf.retry {
                    attempt += 1;
                    emit_notice(format!("{}，正在重试（{}/{}）…", msg, attempt, conf.retry));
                    tokio::time::sleep(Duration::from_millis(300u64 << attempt.min(3))).await;
                    continue;
                }
                return Err((None, msg, None));
            }
            Ok(Ok(r)) => r,
        };
        let status_code = resp.status();
        let latency = t0.elapsed().as_millis() as u64;
        if !status_code.is_success() {
            let retry_after = retry_after_secs(resp.headers());
            let body = resp.text().await.unwrap_or_default();
            let mut msg = err_message(&body);
            record_outbound(app, &host, &conf.ptype, &conf.model, Some(status_code.as_u16()), false, latency, attempt);
            // 本地模型 OOM：重试只会再次拉起一个失败的 llama-server，直接给出可操作建议
            if is_local_oom(&msg) || is_local_oom(&body) {
                msg = local_oom_hint(&conf.model, &msg);
                return Err((Some(status_code.as_u16()), msg, None));
            }
            // 上下文超长：自动裁剪较早消息并重试（最多 2 次）
            if status_code.as_u16() == 400 && is_context_too_long(&msg) && trims < 2 {
                if trim_context(messages) {
                    trims += 1;
                    emit_notice("上下文超长，已自动裁剪较早消息并重试…".into());
                    continue;
                }
            }
            // 5xx 自动重试；429 按规范显示倒计时，不自动重试
            if status_code.is_server_error() && attempt < conf.retry {
                attempt += 1;
                emit_notice(format!("服务端错误（{}），正在重试（{}/{}）…", status_code.as_u16(), attempt, conf.retry));
                tokio::time::sleep(Duration::from_millis(300u64 << attempt.min(3))).await;
                continue;
            }
            return Err((Some(status_code.as_u16()), msg, retry_after));
        }
        record_outbound(app, &host, &conf.ptype, &conf.model, Some(status_code.as_u16()), true, latency, attempt);
        if chat_aborted(app, seq) {
            return Err((None, "__stopped__".into(), None));
        }
        let is_sse = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|c| c.contains("event-stream"))
            .unwrap_or(false);
        let idle = Duration::from_secs(((conf.timeout * 2).clamp(30, 240)) as u64);
        let ptype = conf.ptype.clone();
        let native_ollama = conf.ollama && !conf.base.trim_end_matches('/').ends_with("/chat/completions");
        let mut text = String::new();
        let mut acc = ToolAcc::default();
        if native_ollama {
            // Ollama 原生 /api/chat：流式为 NDJSON（每行一个 JSON），非流式为单个 JSON
            let is_ndjson = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|c| c.contains("ndjson"))
                .unwrap_or(want_stream);
            if want_stream && is_ndjson {
                use futures_util::StreamExt;
                let mut stream = resp.bytes_stream();
                let mut buf = String::new();
                loop {
                    if chat_aborted(app, seq) {
                        return Err((None, "__stopped__".into(), None));
                    }
                    let next = match tokio::time::timeout(idle, stream.next()).await {
                        Err(_) => return Err((None, "流式响应超时（长时间无数据）".into(), None)),
                        Ok(None) => break,
                        Ok(Some(Err(e))) => return Err((None, format!("流中断：{}", e), None)),
                        Ok(Some(Ok(bytes))) => bytes,
                    };
                    buf.push_str(&String::from_utf8_lossy(&next));
                    while let Some(idx) = buf.find('\n') {
                        let line: String = buf.drain(..=idx).collect();
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        if let Some(err) = ollama_line_error(line) {
                            return Err((Some(status_code.as_u16()), ollama_error_text(&conf.model, &err), None));
                        }
                        if let Some(tc) = ollama_tool_calls(line) {
                            acc.push_ollama(&tc);
                        }
                        if let Some(t) = ollama_delta(line) {
                            let _ = app.emit_to("bubble", "chat-delta", json!({"text": t}));
                            text.push_str(&t);
                        }
                    }
                }
                let rest = buf.trim().to_string();
                if let Some(err) = ollama_line_error(&rest) {
                    return Err((Some(status_code.as_u16()), ollama_error_text(&conf.model, &err), None));
                }
                if let Some(tc) = ollama_tool_calls(&rest) {
                    acc.push_ollama(&tc);
                }
                if let Some(t) = ollama_delta(&rest) {
                    let _ = app.emit_to("bubble", "chat-delta", json!({"text": t}));
                    text.push_str(&t);
                }
            } else {
                let body = match resp.text().await {
                    Ok(b) => b,
                    Err(e) => return Err((None, format!("读取响应失败：{}", e), None)),
                };
                let j: Value = match serde_json::from_str(&body) {
                    Ok(j) => j,
                    Err(_) => return Err((Some(status_code.as_u16()), "Ollama 返回的不是合法 JSON".into(), None)),
                };
                if let Some(err) = j["error"].as_str() {
                    return Err((Some(status_code.as_u16()), ollama_error_text(&conf.model, err), None));
                }
                if let Some(tc) = j["message"]["tool_calls"].as_array() {
                    acc.push_ollama(&Value::Array(tc.clone()));
                }
                text = ollama_text(&j);
                if text.is_empty() && acc.into_calls_snapshot().is_empty() {
                    return Err((
                        Some(status_code.as_u16()),
                        "模型返回空内容（若为思考型模型，可调大 Max Tokens 或改用非思考模型）".into(),
                        None,
                    ));
                }
                if !text.is_empty() {
                    let _ = app.emit_to("bubble", "chat-delta", json!({"text": text}));
                }
                let calls = acc.into_calls();
                return Ok(TurnOutcome { text, calls });
            }
        } else if want_stream && is_sse {
            use futures_util::StreamExt;
            let mut stream = resp.bytes_stream();
            let mut buf = String::new();
            loop {
                if chat_aborted(app, seq) {
                    return Err((None, "__stopped__".into(), None));
                }
                let next = match tokio::time::timeout(idle, stream.next()).await {
                    Err(_) => return Err((None, "流式响应超时（长时间无数据）".into(), None)),
                    Ok(None) => break,
                    Ok(Some(Err(e))) => return Err((None, format!("流中断：{}", e), None)),
                    Ok(Some(Ok(bytes))) => bytes,
                };
                buf.push_str(&String::from_utf8_lossy(&next));
                while let Some(idx) = buf.find('\n') {
                    let line: String = buf.drain(..=idx).collect();
                    let line = line.trim_end();
                    // OpenAI 兼容：工具调用以 delta.tool_calls 增量下发
                    if let Some(tc) = sse_tool_deltas(line) {
                        acc.push_openai(&tc);
                    }
                    if let Some(t) = sse_delta(&ptype, line) {
                        let _ = app.emit_to("bubble", "chat-delta", json!({"text": t}));
                        text.push_str(&t);
                    }
                }
            }
            let rest = buf.trim().to_string();
            if let Some(tc) = sse_tool_deltas(&rest) {
                acc.push_openai(&tc);
            }
            if let Some(t) = sse_delta(&ptype, &rest) {
                let _ = app.emit_to("bubble", "chat-delta", json!({"text": t}));
                text.push_str(&t);
            }
        } else {
            // 非流式 / 供应商不支持流式时自动降级（FR-MDL-011）
            let body = match resp.text().await {
                Ok(b) => b,
                Err(e) => return Err((None, format!("读取响应失败：{}", e), None)),
            };
            let j: Value = match serde_json::from_str(&body) {
                Ok(j) => j,
                Err(_) => return Err((Some(status_code.as_u16()), "响应不是合法 JSON（请检查 BaseURL 是否为 OpenAI 兼容端点）".into(), None)),
            };
            // Gemini 的函数调用在 candidates[0].content.parts[].functionCall
            if ptype == "gemini" {
                if let Some(parts) = j["candidates"][0]["content"]["parts"].as_array() {
                    let calls: Vec<Value> = parts
                        .iter()
                        .filter_map(|p| p.get("functionCall"))
                        .map(|fc| {
                            json!({
                                "function": {
                                    "name": fc["name"].as_str().unwrap_or(""),
                                    "arguments": fc.get("args").cloned().unwrap_or_else(|| json!({})),
                                }
                            })
                        })
                        .collect();
                    if !calls.is_empty() {
                        acc.push_ollama(&Value::Array(calls));
                    }
                }
            } else if let Some(tc) = nonstream_tool_calls(&j) {
                acc.push_openai(&tc);
            }
            text = full_text(&ptype, &j);
            if text.is_empty() && acc.into_calls_snapshot().is_empty() {
                return Err((Some(status_code.as_u16()), "模型返回空内容".into(), None));
            }
            if !text.is_empty() {
                let _ = app.emit_to("bubble", "chat-delta", json!({"text": text}));
            }
            let calls = acc.into_calls();
            return Ok(TurnOutcome { text, calls });
        }
        let calls = acc.into_calls();
        return Ok(TurnOutcome { text, calls });
    }
}

// 工具结果回传给模型的消息体：OpenAI 用 role=tool，Anthropic / Gemini 复用 user 角色（见 build_request）
fn tool_result_msgs(results: &[(String, String, String)]) -> Vec<ChatMsg> {
    results
        .iter()
        .map(|(id, name, content)| ChatMsg {
            role: "tool".into(),
            content: content.clone(),
            tool_calls: None,
            tool_call_id: Some(id.clone()),
            name: Some(name.clone()),
        })
        .collect()
}

async fn run_chat(app: AppHandle, args: ChatArgs) {
    let emit_err = |status: Option<u16>, msg: String, retry_after: Option<u64>| {
        let _ = app.emit_to(
            "bubble",
            "chat-error",
            json!({"status": status, "message": msg, "retryAfter": retry_after}),
        );
        log_event(&app, "error", "chat.error", &format!("status={:?} msg={}", status, msg));
    };
    let conf = match resolve_provider(&app, &args.provider_id) {
        Ok(c) => c,
        Err(e) => return emit_err(None, e, None),
    };
    // 新一轮请求：递增代际。旧请求随后会看到 seq 变化而退出，
    // 不会再出现「停止后旧请求复活、霸占前端 streaming 状态」的死锁。
    let seq = bump_chat_seq(&app);
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
        // 按字符（而非字节）切块：中文/emoji 是多字节，按字节切片会落在非字符边界直接 panic，
        // release 构建 panic="abort" → 整个进程静默退出（v0.2.7 演示模式对话崩溃根因）
        for piece in mock_chunks(&reply) {
            if chat_aborted(&app, seq) {
                let _ = app.emit_to("bubble", "chat-stopped", ());
                return;
            }
            let _ = app.emit_to("bubble", "chat-delta", json!({"text": piece}));
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
        let _ = app.emit_to("bubble", "chat-done", ());
        return;
    }
    if conf.base.trim().is_empty() {
        return emit_err(None, "BaseURL 未配置".into(), None);
    }
    // FR-TOOL-001：仅在用户开启工具且供应商协议支持时注入工具定义
    let tools_enabled = {
        let ctx = app.state::<AppCtx>();
        let st = state_lock(&ctx);
        st["settings"]["toolSettings"]["enabled"].as_bool().unwrap_or(true)
    };
    let tool_payload = if tools_enabled && ["openai", "anthropic", "gemini"].contains(&conf.ptype.as_str()) {
        Some(tool_defs(&conf.ptype))
    } else {
        None
    };
    let mut messages = args.messages.clone();
    let max_hops = 4;
    for hop in 0..=max_hops {
        let tools = if hop < max_hops { tool_payload.as_ref() } else { None };
        let outcome = match run_turn(&app, seq, &conf, &mut messages, tools).await {
            Ok(o) => o,
            Err((_, msg, _)) if msg == "__stopped__" => {
                let _ = app.emit_to("bubble", "chat-stopped", ());
                return;
            }
            Err((status, msg, retry_after)) => return emit_err(status, msg, retry_after),
        };
        if outcome.calls.is_empty() {
            let _ = app.emit_to("bubble", "chat-done", ());
            return;
        }
        // FR-TOOL-004：记录 assistant 的工具调用消息，逐个执行后以结构化结果回传
        messages.push(ChatMsg {
            role: "assistant".into(),
            content: outcome.text.clone(),
            tool_calls: Some(tool_calls_payload(&conf.ptype, &outcome.calls)),
            tool_call_id: None,
            name: None,
        });
        let mut results: Vec<(String, String, String)> = vec![];
        for call in outcome.calls.iter() {
            let args_value = tool_call_args(call);
            let _ = app.emit_to("bubble", "tool-pending", json!({"tool": call.name, "label": tool_label(&call.name)}));
            let allowed = request_tool_auth(&app, &call.name, &args_value).await;
            let (ok, payload, summary) = if !allowed {
                (false, json!({"error": "用户未授权该工具"}), "未授权，已跳过".to_string())
            } else if chat_aborted(&app, seq) {
                (false, json!({"error": "用户已停止生成"}), "已停止".to_string())
            } else {
                match execute_tool(&app, &conf, &call.name, &args_value).await {
                    Ok(v) => {
                        let s = tool_result_summary(&call.name, &v);
                        (true, v, s)
                    }
                    Err(e) => (false, json!({"error": e.clone()}), format!("执行失败：{}", e)),
                }
            };
            let _ = app.emit_to(
                "bubble",
                "chat-tool",
                json!({"tool": call.name, "label": tool_label(&call.name), "ok": ok, "summary": summary}),
            );
            results.push((call.id.clone(), call.name.clone(), payload.to_string()));
        }
        messages.extend(tool_result_msgs(&results));
        if hop == max_hops - 1 {
            let _ = app.emit_to("bubble", "chat-notice", json!({"message": "工具调用轮次已达上限，正在整理结果…"}));
        }
    }
    let _ = app.emit_to("bubble", "chat-done", ());
}

// 工具执行结果的一句话摘要（展示在气泡内的工具卡片上，不参与上下文）
fn tool_result_summary(name: &str, v: &Value) -> String {
    let brief = |k: &str| v[k].as_str().map(|s| s.to_string());
    match name {
        "create_reminder" => brief("content")
            .map(|c| format!("已创建提醒：{}", c))
            .unwrap_or_else(|| "已创建提醒".into()),
        "read_clipboard" => format!("读取到 {} 个字符", v["length"].as_i64().unwrap_or(0)),
        "open_url" => format!("已打开 {}", brief("url").unwrap_or_default()),
        "open_app" => format!("已启动 {}", brief("app").unwrap_or_default()),
        "get_weather" => brief("summary").unwrap_or_else(|| "已获取天气".into()),
        "web_search" => format!("找到 {} 条结果", v["count"].as_i64().unwrap_or(0)),
        _ => "已完成".into(),
    }
}

// 演示模式逐字流式：按字符切块（绝不在 UTF-8 多字节序列中间切断）
fn mock_chunks(reply: &str) -> Vec<String> {
    let chars: Vec<char> = reply.chars().collect();
    if chars.is_empty() {
        return vec![];
    }
    let chunk = (chars.len() / 90).max(1);
    chars.chunks(chunk).map(|c| c.iter().collect()).collect()
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

// 合并前端全量快照：提醒与部分设置以主进程为准（便于单元测试）
fn merge_state_snapshot(st: &mut Value, state: Value, origin: &str) {
    let reminders = st["reminders"].take();
    // 宠物位置只由 pet_save_pos / 复位命令写入，任何窗口的全量快照都不得回滚
    let pet_pos = st["settings"]["petPos"].clone();
    // 非设置窗口不会修改这些设置项；保留后端权威值，避免多窗口全量保存互相覆盖
    let preserved: Vec<(String, Value)> = if origin == "settings" {
        vec![]
    } else {
        [
            "petScale", "clickThrough", "dnd", "bubbleW", "showInTaskbar", "alwaysOnTop",
            "bubbleIdleClose", "chatter", "enterSend", "newSessionPerClick", "lowFps", "customSkin",
            "summaryThreshold", "voice", "voiceURI", "voiceRate", "reminderSound", "health",
            "clipboardWatch", "toolSettings", "updateUrl", "updateAuto", "lang", "dndStart",
            "dndEnd", "skinPack", "skins", "activeSkin",
        ]
        .iter()
        .filter(|k| !st["settings"][*k].is_null())
        .map(|k| (k.to_string(), st["settings"][*k].clone()))
        .collect()
    };
    *st = state;
    st["reminders"] = reminders;
    if !pet_pos.is_null() {
        st["settings"]["petPos"] = pet_pos;
    }
    for (k, v) in preserved {
        st["settings"][k] = v;
    }
}

#[tauri::command]
fn state_save(app: AppHandle, ctx: tauri::State<AppCtx>, state: Value, origin: String) -> Result<(), String> {
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        merge_state_snapshot(&mut st, state, &origin);
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
    *ctx.pending.lock().unwrap_or_else(|e| e.into_inner()) = vec![];
    *ctx.dnd_queue.lock().unwrap_or_else(|e| e.into_inner()) = vec![];
    persist_state(&ctx);
    if let Some(pet) = app.get_webview_window("pet") {
        let _ = pet.set_ignore_cursor_events(false);
    }
    position_pet(&app);
    rebuild_tray(&app);
    let _ = app.emit("state-reset", ());
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
    log_event(&app, "warn", "data.reset", "已重置全部数据");
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
    let is_azure = conf.ptype == "openai" && is_azure_endpoint(&conf.base);
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
            if is_azure {
                // Azure 的 /models 不可用：用最小 chat/completions 请求验证部署与密钥
                if let Some(v) = key_header(&conf, false)? {
                    headers.insert("api-key", v);
                }
                azure_url(&conf.base, "/chat/completions")
            } else {
                if let Some(v) = key_header(&conf, true)? {
                    headers.insert("authorization", v);
                }
                if conf.base.trim_end_matches('/').ends_with("/models") {
                    conf.base.trim_end_matches('/').to_string()
                } else if conf.ollama {
                    // Ollama 原生模型列表；用户填 .../v1 或裸地址都能测通
                    format!("{}/api/tags", ollama_root(&conf.base))
                } else {
                    join_url(&conf.base, "/models")
                }
            }
        }
    };
    apply_custom_headers(&mut headers, &conf.headers_raw);
    let host = host_of(&conf.base);
    let t0 = std::time::Instant::now();
    let resp = if is_azure {
        let body = json!({
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1
        });
        client
            .post(&url)
            .headers(headers)
            .json(&body)
            .timeout(Duration::from_secs(15))
            .send()
            .await
    } else {
        client
            .get(&url)
            .headers(headers)
            .timeout(Duration::from_secs(12))
            .send()
            .await
    };
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            record_outbound(&app, &host, &conf.ptype, &conf.model, None, false, t0.elapsed().as_millis() as u64, 0);
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
    record_outbound(&app, &host, &conf.ptype, &conf.model, Some(status), (200..400).contains(&status), latency, 0);
    if status == 401 || status == 403 {
        return Ok(json!({"ok": false, "message": format!("认证失败（{}）：API Key 无效", status)}));
    }
    if status == 404 {
        return Ok(json!({"ok": false, "message": "接口不存在（404）：请检查 BaseURL 路径".to_string()}));
    }
    if status >= 400 {
        return Ok(json!({"ok": false, "message": format!("HTTP {}：{}", status, err_message(&body))}));
    }
    if is_azure {
        // 成功即说明部署名、api-version 与 api-key 均有效
        return Ok(json!({
            "ok": true,
            "latency": latency,
            "models": [conf.model],
            "note": "Azure OpenAI 连接成功（已发送最小测试请求）"
        }));
    }
    let j: Value = match serde_json::from_str(&body) {
        Ok(j) => j,
        Err(_) => return Ok(json!({"ok": false, "message": "响应不是合法 JSON"})),
    };
    let models: Vec<String> = if conf.ollama && j["models"].is_array() {
        // Ollama /api/tags：{"models":[{"name":"qwen2.5:7b",...}]}
        j["models"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["name"].as_str().or(m["model"].as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        match conf.ptype.as_str() {
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
        }
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

// FR-MEM-003：会话超过阈值后，用模型把较早的消息压缩成摘要。
// 渲染层只发送待压缩的历史片段，主进程复用现有三协议请求构造器（非流式），
// 因此 OpenAI / Anthropic / Gemini / Ollama 都能生成摘要，且密钥仍不出主进程。
#[tauri::command]
async fn chat_summarize(
    app: AppHandle,
    provider_id: String,
    messages: Vec<ChatMsg>,
) -> Result<Value, String> {
    if messages.iter().all(|m| m.role == "system" || m.content.trim().is_empty()) {
        return Ok(json!({"summary": "", "skipped": true}));
    }
    let conf = resolve_provider(&app, &provider_id)?;
    if conf.ptype == "mock" {
        let text = mock_summary(&messages);
        log_event(
            &app,
            "info",
            "chat.summarized",
            &format!("mock 压缩 {} 条消息，摘要 {} 字", messages.len(), text.chars().count()),
        );
        return Ok(json!({"summary": text, "mock": true}));
    }
    if conf.base.trim().is_empty() {
        return Err("BaseURL 未配置，无法生成摘要".into());
    }
    let mut c = conf.clone();
    c.stream = false;
    c.temp = 0.2;
    c.max_tokens = c.max_tokens.clamp(128, 512);
    // 已有摘要一并交给模型，保证多次压缩后上下文仍然连贯
    let previous: String = messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let mut transcript = String::new();
    for m in messages.iter().filter(|m| m.role != "system") {
        let role = if m.role == "assistant" { "助手" } else { "用户" };
        if m.content.trim().is_empty() {
            continue;
        }
        transcript.push_str(role);
        transcript.push_str("：");
        transcript.push_str(m.content.trim());
        transcript.push('\n');
    }
    let user_content = if previous.is_empty() {
        format!("对话记录：\n{}", transcript)
    } else {
        format!("已有摘要：\n{}\n\n新增对话记录：\n{}", previous, transcript)
    };
    let prompt = vec![
        ChatMsg {
            role: "system".into(),
            content: "你是一个会话摘要助手。把给定对话压缩成简洁的要点，保留用户的事实、偏好、目标、待办与已确定的结论；不要编造，不要输出与摘要无关的话。用第三人称，200 字以内。".into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        ChatMsg {
            role: "user".into(),
            content: user_content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];
    let rb = build_request(&app, &c, &prompt, None)?;
    let host = host_of(&c.base);
    let t0 = std::time::Instant::now();
    // 摘要在发送前同步执行，必须比正常对话更快失败：即使供应商超时设为 120s，
    // 也最多等 30s，避免一次摘要请求把用户的新消息卡住。
    let summary_timeout = Duration::from_secs(c.timeout.clamp(5, 30) as u64);
    let resp = rb
        .timeout(summary_timeout)
        .send()
        .await
        .map_err(|e| {
            record_outbound(
                &app,
                &host,
                &c.ptype,
                &c.model,
                None,
                false,
                t0.elapsed().as_millis() as u64,
                0,
            );
            format!("摘要请求失败：{}", net_error_msg(&e))
        })?;
    let status = resp.status();
    let latency = t0.elapsed().as_millis() as u64;
    let body = resp.text().await.unwrap_or_default();
    record_outbound(
        &app,
        &host,
        &c.ptype,
        &c.model,
        Some(status.as_u16()),
        status.is_success(),
        latency,
        0,
    );
    if !status.is_success() {
        return Err(format!(
            "摘要生成失败（HTTP {}）：{}",
            status.as_u16(),
            err_message(&body)
        ));
    }
    let j: Value = serde_json::from_str(&body).map_err(|_| "摘要响应不是合法 JSON".to_string())?;
    let text = if c.ollama && !c.base.trim_end_matches('/').ends_with("/chat/completions") {
        ollama_text(&j)
    } else {
        full_text(&c.ptype, &j)
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("模型返回了空摘要".into());
    }
    log_event(
        &app,
        "info",
        "chat.summarized",
        &format!("压缩 {} 条消息，摘要 {} 字", messages.len(), text.chars().count()),
    );
    Ok(json!({"summary": text}))
}

// 演示模式摘要：无需联网，抽取用户侧关键句，保证完整链路可体验
fn mock_summary(messages: &[ChatMsg]) -> String {
    let mut parts: Vec<String> = vec![];
    for m in messages.iter().filter(|m| m.role == "user") {
        let t = m.content.trim();
        if t.is_empty() {
            continue;
        }
        parts.push(cut_chars(t, 40));
        if parts.len() >= 4 {
            break;
        }
    }
    if parts.is_empty() {
        "（较早对话没有可摘要的内容）".into()
    } else {
        format!("较早对话涉及：{}。", parts.join("；"))
    }
}

// FR-TOOL-003 / AC-012：气泡内授权卡片的回执（允许 / 拒绝，可勾选记住）
// FR-REM-007：用户在宠物旁点击「点我处理」后读取一次剪贴板（用户显式触发，非后台采集）
// FR-SKN-002：皮肤包格式（.deskpet-skin，实为 zip）
//   skin.json  {"name":"…","version":"1.0","states":{"idle":"idle.png", …},"sound":{"reminder":"ding.ogg"}}
//   *.png / *.gif / *.webp  图集或各状态帧；sound/*.ogg|*.mp3|*.wav 可选
// 校验点：压缩包 ≤ 128 MB、单文件 ≤ 64 MB、总解压 ≤ 256 MB、条目数 ≤ 200；拒绝路径穿越。
// 图片文件大小不再设置 1.5 MB 级别的小上限，仅保留防止压缩炸弹的保护性上限。
const SKIN_MAX_ZIP: usize = 128 * 1024 * 1024;
const SKIN_MAX_FILE: u64 = 64 * 1024 * 1024;
const SKIN_MAX_TOTAL: u64 = 256 * 1024 * 1024;
const SKIN_MAX_ENTRIES: usize = 200;
const SKIN_STATES: [&str; 8] = ["idle", "drag", "click", "talk", "sleep", "work", "happy", "error"];

fn mime_of(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    if n.ends_with(".png") { Some("image/png") }
    else if n.ends_with(".gif") { Some("image/gif") }
    else if n.ends_with(".webp") { Some("image/webp") }
    else { None }
}

fn read_skin_zip(raw: &[u8]) -> Result<Value, String> {
    use std::io::Read;
    if raw.len() > SKIN_MAX_ZIP {
        return Err(format!("皮肤包超过 {} MB 上限", SKIN_MAX_ZIP / 1024 / 1024));
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(raw)).map_err(|e| format!("不是有效的皮肤包（zip）：{}", e))?;
    if zip.len() > SKIN_MAX_ENTRIES {
        return Err("皮肤包条目过多".into());
    }
    let mut total: u64 = 0;
    let mut files: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut sounds: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut manifest: Option<Value> = None;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let raw_name = f.mangled_name();
        let name = raw_name
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("皮肤包内文件名编码异常")?
            .to_string();
        // 只取 basename：压缩包内即便写了 ../ 也无法落到包外
        if name.starts_with('.') || name.contains(std::path::MAIN_SEPARATOR) || name.contains('/') {
            continue;
        }
        if f.size() > SKIN_MAX_FILE {
            return Err(format!(
                "{} 超过 {} MB 单文件上限",
                name,
                SKIN_MAX_FILE / 1024 / 1024
            ));
        }
        total += f.size();
        if total > SKIN_MAX_TOTAL {
            return Err("皮肤包解压后总大小超限".into());
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf).map_err(|e| format!("读取 {} 失败：{}", name, e))?;
        if name.eq_ignore_ascii_case("skin.json") {
            let txt = String::from_utf8(buf).map_err(|_| "skin.json 不是 UTF-8 文本")?;
            manifest = Some(serde_json::from_str(&txt).map_err(|e| format!("skin.json 解析失败：{}", e))?);
            continue;
        }
        let lower = name.to_lowercase();
        if lower.ends_with(".png") || lower.ends_with(".gif") || lower.ends_with(".webp") {
            let mime = mime_of(&name).unwrap_or("image/png");
            files.insert(
                name.clone(),
                format!("data:{};base64,{}", mime, base64_encode(&buf)),
            );
        } else if lower.ends_with(".ogg") || lower.ends_with(".mp3") || lower.ends_with(".wav") {
            let mime = if lower.ends_with(".mp3") { "audio/mpeg" } else if lower.ends_with(".wav") { "audio/wav" } else { "audio/ogg" };
            sounds.insert(name.clone(), format!("data:{};base64,{}", mime, base64_encode(&buf)));
        }
    }
    let manifest = manifest.ok_or("皮肤包缺少 skin.json")?;
    let name = manifest["name"].as_str().unwrap_or("导入皮肤").trim();
    if name.is_empty() {
        return Err("skin.json 缺少 name".into());
    }
    // 状态机配置：states 里的文件名必须真实存在于包内，否则该状态回退到 idle
    let mut states: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let src_states = manifest["states"].as_object().cloned().unwrap_or_default();
    let fallback = if files.contains_key("idle.png") {
        "idle.png".to_string()
    } else {
        files.keys().next().cloned().unwrap_or_default()
    };
    for st in SKIN_STATES {
        let key = src_states
            .get(st)
            .and_then(|v| v.as_str())
            .map(|s| s.rsplit('/').next().unwrap_or(s).to_string());
        let pick = match key {
            Some(k) if files.contains_key(&k) => k,
            _ => fallback.clone(),
        };
        if !pick.is_empty() {
            states.insert(st.to_string(), pick);
        }
    }
    if states.is_empty() {
        return Err("皮肤包里没有任何图片资源".into());
    }
    let mut group = serde_json::Map::new();
    if let Some(g) = manifest["group"].as_object() {
        for (k, v) in g {
            let Some(f) = v.as_str() else { continue };
            let base = f.rsplit('/').next().unwrap_or(f).to_string();
            if files.contains_key(&base) {
                group.insert(k.clone(), Value::String(base));
            }
        }
    }
    Ok(json!({
        "name": cut_chars(name, 40),
        "version": manifest["version"].as_str().unwrap_or("1.0"),
        "states": states,
        "group": group,
        "files": files,
        "sounds": sounds,
        "byteSize": total,
        "ts": now_ms(),
    }))
}

// FR-SKN-003：Codex Pet Contract（OpenAI Codex 桌面端「Pets」自定义宠物包）
//   目录结构  ${CODEX_HOME:-~/.codex}/pets/<pet-id>/{pet.json, spritesheet.webp}
//   pet.json  {"id":"…","displayName":"…","description":"…","spritesheetPath":"spritesheet.webp"}
//   图集      1536×1872 px = 8 列 × 9 行，单格 192×208，透明背景，未用格子必须完全透明。
//   行序固定：0 idle(6) / 1 running-right(8) / 2 running-left(8) / 3 waving(4) /
//             4 jumping(5) / 5 failed(8) / 6 waiting(6) / 7 running(6) / 8 review(6)
//   导入时按行拆帧（SVG 图集用 viewBox 裁切，等价于无损切片），渲染层按该行的
//   帧时长逐帧播放；导出时把 DeskPet 八态重新拼回 8×9 契约图集。
const CODEX_CELL_W: u32 = 192;
const CODEX_CELL_H: u32 = 208;
const CODEX_COLS: u32 = 8;
const CODEX_ROWS: usize = 9;
const CODEX_ATLAS_W: u32 = CODEX_CELL_W * CODEX_COLS; // 1536
const CODEX_ATLAS_H: u32 = CODEX_CELL_H * CODEX_ROWS as u32; // 1872
const CODEX_SHEET_MAX: usize = 64 * 1024 * 1024;
// Codex 行名（行序即数组下标），与契约的 row 0..8 一一对应
const CODEX_ROW_NAMES: [&str; CODEX_ROWS] = [
    "idle",
    "running-right",
    "running-left",
    "waving",
    "jumping",
    "failed",
    "waiting",
    "running",
    "review",
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum CodexKind {
    Png,
    Gif,
    Webp,
    Svg,
}

impl CodexKind {
    fn mime(self) -> &'static str {
        match self {
            CodexKind::Png => "image/png",
            CodexKind::Gif => "image/gif",
            CodexKind::Webp => "image/webp",
            CodexKind::Svg => "image/svg+xml",
        }
    }
}

fn codex_kind_of(name: &str) -> Option<CodexKind> {
    let n = name.to_lowercase();
    if n.ends_with(".png") {
        Some(CodexKind::Png)
    } else if n.ends_with(".webp") {
        Some(CodexKind::Webp)
    } else if n.ends_with(".gif") {
        Some(CodexKind::Gif)
    } else if n.ends_with(".svg") {
        Some(CodexKind::Svg)
    } else {
        None
    }
}

fn u16_be(b: &[u8], i: usize) -> u16 {
    ((b[i] as u16) << 8) | b[i + 1] as u16
}

fn u24_be(b: &[u8], i: usize) -> u32 {
    ((b[i] as u32) << 16) | ((b[i + 1] as u32) << 8) | b[i + 2] as u32
}

// 只读文件头取尺寸：PNG / WebP / GIF / SVG
fn codex_image_size(raw: &[u8], kind: CodexKind) -> Option<(u32, u32)> {
    match kind {
        CodexKind::Png => {
            if raw.len() < 26 || &raw[..8] != b"\x89PNG\r\n\x1a\n" || &raw[12..16] != b"IHDR" {
                return None;
            }
            Some((
                u32::from_be_bytes([raw[16], raw[17], raw[18], raw[19]]),
                u32::from_be_bytes([raw[20], raw[21], raw[22], raw[23]]),
            ))
        }
        CodexKind::Gif => {
            if raw.len() < 10 || (!raw.starts_with(b"GIF87a") && !raw.starts_with(b"GIF89a")) {
                return None;
            }
            Some((
                raw[6] as u32 | ((raw[7] as u32) << 8),
                raw[8] as u32 | ((raw[9] as u32) << 8),
            ))
        }
        CodexKind::Webp => {
            if raw.len() < 30 || !raw.starts_with(b"RIFF") || &raw[8..12] != b"WEBP" {
                return None;
            }
            Some(if &raw[12..16] == b"VP8X" {
                (u24_be(raw, 24) + 1, u24_be(raw, 27) + 1)
            } else if &raw[12..16] == b"VP8L" {
                let bits = u32::from_le_bytes([raw[21], raw[22], raw[23], raw[24]]);
                ((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1)
            } else if &raw[12..16] == b"VP8 " {
                (u16_be(raw, 26) as u32 & 0x3FFF, u16_be(raw, 28) as u32 & 0x3FFF)
            } else {
                return None;
            })
        }
        CodexKind::Svg => {
            let head = String::from_utf8_lossy(&raw[..raw.len().min(4096)]);
            let start = head.find("<svg")?;
            let tag = &head[start..];
            let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
            let num_attr = |key: &str| -> Option<f64> {
                let at = tag.find(key)?;
                let after = tag[at + key.len()..].trim_start();
                let after = after.strip_prefix('=')?.trim_start();
                let quote = after.chars().next()?;
                let rest = &after[1..];
                let end = rest.find(quote)?;
                rest[..end]
                    .trim()
                    .trim_end_matches("px")
                    .parse::<f64>()
                    .ok()
            };
            let vb = tag.find("viewBox").and_then(|i| {
                let rest = &tag[i + 7..];
                let rest = rest.split(['"', '\'']).find(|s| s.trim().len() > 3)?;
                let nums: Vec<f64> = rest
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                if nums.len() >= 4 {
                    Some((nums[2], nums[3]))
                } else {
                    None
                }
            });
            let (w, h) = match vb {
                Some(v) => v,
                None => (num_attr("width")?, num_attr("height")?),
            };
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            Some((w.round() as u32, h.round() as u32))
        }
    }
}

// GIF 首帧时长（1/100 s）与 NETSCAPE 循环次数：Codex 允许精灵图本身是动图
fn codex_gif_timing(raw: &[u8]) -> (Option<u16>, Option<u16>) {
    if raw.len() < 13 {
        return (None, None);
    }
    let mut i = 13 + if raw[10] & 0x80 != 0 {
        3 * (1 << ((raw[10] & 0x07) as usize + 1))
    } else {
        0
    };
    let (mut delay, mut loops) = (None, None);
    while i + 1 < raw.len() {
        match raw[i] {
            0x21 => {
                let label = raw[i + 1];
                if label == 0xF9 && i + 5 < raw.len() {
                    delay = delay.or(Some(u16::from_le_bytes([raw[i + 4], raw[i + 5]])));
                } else if label == 0xFF && i + 14 <= raw.len() && &raw[i + 3..i + 14] == b"NETSCAPE2.0" && i + 17 < raw.len() {
                    loops = Some(u16::from_le_bytes([raw[i + 16], raw[i + 17]]));
                }
                let mut p = i + 2;
                while p < raw.len() {
                    let n = raw[p] as usize;
                    p += 1 + n;
                    if n == 0 {
                        break;
                    }
                }
                i = p;
            }
            // 首个图像描述符：之后的字节是像素数据，不再解析
            0x2C => break,
            _ => break,
        }
    }
    (delay, loops)
}

fn codex_slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_matches('-').chars().take(40).collect::<String>()
}

// 只接受「一张矩形图 + 若干基础图形」的极简 SVG：元素 / 属性白名单，
// 禁止脚本、外链、外部样式与动画；属性值一律要求数字序列 + 单位，
// 防止通过 id / url(#…) 注入任何可执行的引用。
fn codex_svg_sanitize(svg: &str) -> Result<String, String> {
    let lower = svg.to_lowercase();
    for bad in [
        "<script", "<foreignobject", "<iframe", "<image", "<use", "<style", "<animate", "<set",
        "<handler", "<a ", "xlink:", "href", "<!entity", "<!doctype", "onload", "onerror",
        "onclick", "javascript:", "url(", "@import", "<！",
    ] {
        if lower.contains(bad) {
            return Err("SVG 图集含有不被允许的元素或属性（脚本 / 外链 / 样式 / 动画）".into());
        }
    }
    let start = lower.find("<svg").ok_or("SVG 图集缺少 <svg> 根元素")?;
    let end = lower.rfind("</svg>").ok_or("SVG 图集缺少 </svg>")?;
    if end <= start {
        return Err("SVG 图集结构无效".into());
    }
    // 根元素开标签结束位置
    let open_end = svg[start..].find('>').ok_or("SVG 根元素未闭合")? + start;
    let body = &svg[open_end + 1..end];
    // 只保留根 svg 的 viewBox，其余根属性全部丢弃（宽高由渲染层决定）
    let root = &svg[start..open_end];
    let view_box = {
        let l = root.to_lowercase();
        let at = l.find("viewbox").ok_or("SVG 根元素缺少 viewBox")?;
        let rest = &root[at..];
        let q = rest.find(|c| c == '"' || c == '\'');
        match q {
            Some(i) => {
                let quote = rest.as_bytes()[i] as char;
                let after = &rest[i + 1..];
                let close = after.find(quote).ok_or("viewBox 引号未闭合")?;
                after[..close].trim().to_string()
            }
            None => return Err("SVG viewBox 格式无效".into()),
        }
    };
    let mut clean = String::with_capacity(body.len() + 128);
    let mut rest = body;
    let mut depth = 0usize;
    while let Some(lt) = rest.find('<') {
        let text = &rest[..lt];
        if !text.trim().is_empty() && depth == 0 {
            rest = &rest[lt..];
            continue;
        }
        if text.contains(|c: char| c.is_ascii_alphabetic()) {
            return Err("SVG 图集不支持自由文本".into());
        }
        let gt = rest[lt..].find('>').ok_or("SVG 元素未闭合")? + lt;
        let tag = rest[lt..=gt].trim().to_string();
        let ltag = tag.to_lowercase();
        if ltag.starts_with("</") {
            let name: String = ltag[2..].chars().filter(|c| c.is_ascii_alphabetic()).collect();
            if depth == 0 {
                return Err("SVG 图集标签不匹配".into());
            }
            depth -= 1;
            clean.push_str(&format!("</{}>", name));
        } else {
            let self_close = ltag.ends_with("/>");
            let name: String = ltag[1..].chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            if !["g", "rect", "circle", "ellipse", "line", "polyline", "polygon", "path"].contains(&name.as_str()) {
                return Err(format!("SVG 图集包含不被支持的标签 <{}>", name));
            }
            let attrs = codex_svg_attrs(&tag, &name)?;
            if attrs.is_empty() && name != "g" {
                return Err(format!("SVG <{}> 缺少属性", name));
            }
            clean.push('<');
            clean.push_str(&name);
            clean.push(' ');
            clean.push_str(&attrs);
            if self_close {
                clean.push_str("/>");
            } else {
                clean.push('>');
                depth += 1;
            }
        }
        rest = &rest[gt + 1..];
    }
    if depth != 0 {
        return Err("SVG 图集存在未闭合元素".into());
    }
    Ok(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{}" style="overflow:hidden">{}</svg>"#,
        view_box, clean
    ))
}

fn codex_svg_attrs(tag: &str, name: &str) -> Result<String, String> {
    let inner = tag
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_end_matches('/');
    let mut rest = &inner[inner.len().min(name.len())..];
    let mut out = Vec::new();
    while let Some(eq) = rest.find('=') {
        let key: String = rest[..eq].trim().to_string();
        if key.is_empty() {
            rest = &rest[eq + 1..];
            continue;
        }
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next().ok_or("SVG 属性值缺失")?;
        if quote != '"' && quote != '\'' {
            return Err(format!("SVG 属性 {} 未加引号", key));
        }
        let tail = &after[1..];
        let close = tail.find(quote).ok_or("SVG 属性引号未闭合")?;
        let value = &tail[..close];
        if !codex_svg_value_ok(&key, value) {
            return Err(format!("SVG 属性 {} 的取值不被允许：{}", key, cut_chars(value, 30)));
        }
        out.push(format!("{}=\"{}\"", key, value));
        rest = &tail[close + 1..];
    }
    // 标签名之后不应有裸文本
    if rest.chars().any(|c| c.is_ascii_alphabetic()) {
        return Err("SVG 标签格式无效".into());
    }
    Ok(out.join(" "))
}

fn codex_svg_value_ok(key: &str, value: &str) -> bool {
    match key {
        "fill" | "stroke" => {
            let v = value.trim();
            (v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit()) && matches!(v.len(), 4 | 7 | 9))
                || v.eq_ignore_ascii_case("none")
        }
        "fill-opacity" | "stroke-opacity" | "opacity" => {
            value.trim().parse::<f64>().map(|n| (0.0..=1.0).contains(&n)).unwrap_or(false)
        }
        "stroke-width" => value.trim().parse::<f64>().map(|n| n >= 0.0 && n < 4096.0).unwrap_or(false),
        "stroke-linecap" => matches!(value, "butt" | "round" | "square"),
        "stroke-linejoin" => matches!(value, "miter" | "round" | "bevel"),
        "transform" => {
            let v = value.trim();
            v.starts_with("translate(") || v.starts_with("scale(") || v.starts_with("rotate(")
        }
        _ => {
            // 几何属性：数字列表（允许单位后缀与逗号 / 空白分隔）
            if value.is_empty() {
                return false;
            }
            let mut ok = key_is_geom(key);
            for tok in value.split(|c: char| c.is_whitespace() || c == ',') {
                if tok.is_empty() {
                    continue;
                }
                let t = tok.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%');
                if t.parse::<f64>().is_err() {
                    ok = false;
                    break;
                }
            }
            ok
        }
    }
}

fn key_is_geom(key: &str) -> bool {
    matches!(
        key,
        "x" | "y" | "x1" | "y1" | "x2" | "y2" | "width" | "height" | "cx" | "cy" | "r" | "rx"
            | "ry" | "d" | "points"
    )
}

// 把整张 Codex 图集在 SVG 内逐行裁切：每行一条 <g>，内含 N 个 <svg viewBox=格子> 的帧。
// 相对图集渲染时按 cellW / cellH 缩放，整行宽度正好铺满 192px，天然没有取整误差；
// 单独渲染某一帧时，内部位移再按帧序左移 cellW，得到该帧的稳定画面。
fn codex_row_groups(svg_body: &str) -> String {
    let mut out = String::new();
    for (idx, max) in (0..CODEX_ROWS).zip([6u32, 8, 8, 4, 5, 8, 6, 6, 6]) {
        out.push_str(&format!(
            r#"<g id="codex-row-{}" data-codex-row="{}" style="display:none">"#,
            idx, idx
        ));
        for col in 0..max {
            out.push_str(&format!(
                r#"<svg x="{}" y="{}" width="{}" height="{}" viewBox="{} {} {} {}" overflow="hidden"><g transform="translate({} 0)">{}</g></svg>"#,
                col * CODEX_CELL_W,
                idx as u32 * CODEX_CELL_H,
                CODEX_CELL_W,
                CODEX_CELL_H,
                col * CODEX_CELL_W,
                idx as u32 * CODEX_CELL_H,
                CODEX_CELL_W,
                CODEX_CELL_H,
                (col * CODEX_CELL_W).wrapping_neg(),
                svg_body
            ));
        }
        out.push_str("</g>");
    }
    out
}

// 渲染层只认「图集 + 行分组」：anim 由前端按行帧时长驱动 display 切换
fn codex_svg_atlas(svg_body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" style="overflow:hidden">{}</svg>"#,
        CODEX_ATLAS_W,
        CODEX_ATLAS_H,
        codex_row_groups(svg_body)
    )
}

fn codex_row_max(row: usize) -> usize {
    [6usize, 8, 8, 4, 5, 8, 6, 6, 6][row.min(CODEX_ROWS - 1)]
}

// PNG 图集：按 192×208 网格逐格编码成独立 PNG；整格全透明的列视为「未使用列」跳过
fn codex_slice_png(
    raw: &[u8],
    files: &mut std::collections::BTreeMap<String, String>,
    rows: &mut Vec<(usize, usize, Vec<u16>)>,
) -> Result<(), String> {
    let img = png_decode(raw)?;
    let cell = vec![0u8; (CODEX_CELL_W * CODEX_CELL_H * 4) as usize];
    for row in 0..CODEX_ROWS {
        let mut frames = 0usize;
        for col in 0..codex_row_max(row) {
            let mut buf = cell.clone();
            let mut visible = false;
            for y in 0..CODEX_CELL_H {
                for x in 0..CODEX_CELL_W {
                    let p = img.pixel(
                        col as u32 * CODEX_CELL_W + x,
                        row as u32 * CODEX_CELL_H + y,
                    );
                    let o = ((y * CODEX_CELL_W + x) * 4) as usize;
                    buf[o..o + 4].copy_from_slice(&p);
                    if p[3] > 8 {
                        visible = true;
                    }
                }
            }
            if !visible {
                break;
            }
            frames += 1;
            // 每行同时保留「整行帧条」（渲染层按行播放）与「单帧」（逐格替换 / 导出）
            let mut strip = Vec::with_capacity(CODEX_CELL_W as usize * 4 * frames);
            // 帧条按真实帧数拼，前面已切好的格子直接复用
            let strip_w = CODEX_CELL_W as usize * frames;
            strip.resize(strip_w * CODEX_CELL_H as usize * 4, 0);
            for y in 0..CODEX_CELL_H as usize {
                for x in 0..CODEX_CELL_W as usize {
                    let src = (y * CODEX_CELL_W as usize + x) * 4;
                    let dst = (y * strip_w + (frames - 1) * CODEX_CELL_W as usize + x) * 4;
                    strip[dst..dst + 4].copy_from_slice(&buf[src..src + 4]);
                }
            }
            let frame_png = png_encode_rgba(CODEX_CELL_W, CODEX_CELL_H, &buf)?;
            files.insert(
                format!("codex-frame-{}-{}.png", row, col),
                format!("data:image/png;base64,{}", base64_encode(&frame_png)),
            );
            files.insert(
                format!("codex-row-{}-{}.png", row, CODEX_ROW_NAMES[row]),
                format!("data:image/png;base64,{}", base64_encode(&png_encode_rgba(CODEX_CELL_W * frames as u32, CODEX_CELL_H, &strip)?)),
            );
        }
        if frames > 0 {
            // 图集本身没有时序，按契约默认节奏播放；可由前端在「逐状态替换」里改
            let delay = 120u16;
            rows.push((row, frames, vec![delay; frames]));
        }
    }
    Ok(())
}

// 最小 PNG 编码 + 解码：只处理「8 位、非隔行、无调色板」的 PNG，
// 覆盖 Codex 图集的实际形态；遇到更复杂的 PNG 直接拒绝并提示用户换 WebP / SVG。
const CODEX_PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for b in bytes {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = Vec::with_capacity(data.len() + 4);
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&png_crc32(&body).to_be_bytes());
}

// rgba: 每行 width*4 字节，不加 filter 字节之外的冗余数据
fn png_encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("PNG 像素数据长度不匹配".into());
    }
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for y in 0..height as usize {
        raw.push(0u8); // filter: None
        raw.extend_from_slice(&rgba[y * width as usize * 4..(y + 1) * width as usize * 4]);
    }
    let mut out = Vec::with_capacity(raw.len() / 2 + 1024);
    out.extend_from_slice(&CODEX_PNG_SIG);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr);
    png_chunk(&mut out, b"IDAT", &zlib_deflate(&raw)?);
    png_chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

// zlib 包装（最小体积，依赖已在依赖树中的 flate2 / miniz_oxide）
fn zlib_deflate(raw: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(raw).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())
}

struct PngImage {
    width: u32,
    height: u32,
    bpp: usize,
    pixels: Vec<u8>,
}

fn png_decode(raw: &[u8]) -> Result<PngImage, String> {
    const UNSUPPORTED: &str = "PNG 图集格式较为特殊（调色板 / 隔行 / 16 位），请另存为 WebP 或 SVG 后重试";
    if raw.len() < 33 || raw[..8] != CODEX_PNG_SIG {
        return Err("不是有效的 PNG".into());
    }
    let be32 = |i: usize| u32::from_be_bytes([raw[i], raw[i + 1], raw[i + 2], raw[i + 3]]);
    let (width, height) = (be32(16), be32(20));
    let (depth, color, interlace) = (raw[24], raw[25], raw[28]);
    if width == 0 || height == 0 {
        return Err("PNG 尺寸无效".into());
    }
    if depth != 8 || interlace != 0 || !matches!(color, 0 | 2 | 4 | 6) {
        return Err(UNSUPPORTED.into());
    }
    let bpp = match color {
        0 => 1,
        2 => 3,
        4 => 2,
        _ => 4,
    };
    let mut idat = Vec::new();
    let mut i = 8;
    while i + 8 <= raw.len() {
        let len = be32(i) as usize;
        let kind = &raw[i + 4..i + 8];
        let data_start = i + 8;
        if data_start + len + 4 > raw.len() {
            break;
        }
        if kind == b"IDAT" {
            idat.extend_from_slice(&raw[data_start..data_start + len]);
        } else if kind == b"IEND" {
            break;
        }
        i = data_start + len + 4;
    }
    let row_bytes = width as usize * bpp;
    let expect = (row_bytes + 1) * height as usize;
    let raw_px = zlib_inflate(&idat, expect)?;
    if raw_px.len() < expect {
        return Err("PNG 数据不完整".into());
    }
    let mut pixels = vec![0u8; row_bytes * height as usize];
    let mut prev = vec![0u8; row_bytes];
    for y in 0..height as usize {
        let ft = raw_px[y * (row_bytes + 1)];
        let line = &raw_px[y * (row_bytes + 1) + 1..(y + 1) * (row_bytes + 1)];
        let mut cur = line.to_vec();
        for x in 0..row_bytes {
            let a = if x >= bpp { cur[x - bpp] } else { 0 };
            let b = prev[x];
            let c = if x >= bpp { prev[x - bpp] } else { 0 };
            let v = match ft {
                0 => 0,
                1 => a as i32,
                2 => b as i32,
                3 => ((a as i32) + (b as i32)) / 2,
                4 => {
                    let p = a as i32 + b as i32 - c as i32;
                    let (pa, pb, pc) = ((p - a as i32).abs(), (p - b as i32).abs(), (p - c as i32).abs());
                    if pa <= pb && pa <= pc { a as i32 } else if pb <= pc { b as i32 } else { c as i32 }
                }
                _ => return Err("PNG 行过滤类型无效".into()),
            };
            cur[x] = cur[x].wrapping_add(v as u8);
        }
        pixels[y * row_bytes..(y + 1) * row_bytes].copy_from_slice(&cur);
        prev = cur;
    }
    Ok(PngImage { width, height, bpp, pixels })
}

impl PngImage {
    fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * self.bpp;
        let p = &self.pixels[i..i + self.bpp];
        match self.bpp {
            1 => [p[0], p[0], p[0], 255],
            2 => [p[0], p[0], p[0], p[1]],
            3 => [p[0], p[1], p[2], 255],
            _ => [p[0], p[1], p[2], p[3]],
        }
    }
}

// zlib 容器 + inflate。输出长度上界由 IHDR 推得，超出即视为非法压缩流
fn zlib_inflate(data: &[u8], max_out: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let dec = flate2::read::ZlibDecoder::new(data);
    let mut out = Vec::with_capacity(max_out.min(8 * 1024 * 1024));
    dec.take(max_out as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("PNG 压缩流解析失败：{}", e))?;
    if out.len() > max_out {
        return Err("PNG 压缩流长度异常".into());
    }
    Ok(out)
}

// 图集读取：PNG 做逐格确定性切片；SVG 用 viewBox 虚拟裁切（无损、任意缩放）；
// 其它格式（WebP / 动图 GIF）整幅交给渲染层，作为待机帧等比铺满。
fn codex_read_atlas(
    raw: &[u8],
    kind: CodexKind,
    name: &str,
    files: &mut std::collections::BTreeMap<String, String>,
    rows: &mut Vec<(usize, usize, Vec<u16>)>,
) -> Result<(), String> {
    let (w, h) = codex_image_size(raw, kind).ok_or("无法读取图集尺寸，文件可能已损坏")?;
    if w != CODEX_ATLAS_W || h != CODEX_ATLAS_H {
        return Err(format!(
            "图集尺寸 {}×{} 不符合 Codex Pet Contract：必须 {}×{}（{} 列 × {} 行，单格 {}×{}）",
            w, h, CODEX_ATLAS_W, CODEX_ATLAS_H, CODEX_COLS, CODEX_ROWS, CODEX_CELL_W, CODEX_CELL_H
        ));
    }
    if raw.len() > CODEX_SHEET_MAX {
        return Err("图集超过 64 MB".into());
    }
    match kind {
        CodexKind::Svg => {
            let body = codex_svg_sanitize(&String::from_utf8_lossy(raw))?;
            let atlas = codex_svg_atlas(&body);
            let data = format!("data:image/svg+xml;base64,{}", base64_encode(atlas.as_bytes()));
            // SVG 图集是矢量切片：每行按契约最大帧数全部可用
            for row in 0..CODEX_ROWS {
                let frames = codex_row_max(row);
                files.insert(
                    format!("codex-row-{}-{}", row, CODEX_ROW_NAMES[row]),
                    data.clone(),
                );
                rows.push((row, frames, vec![120; frames]));
            }
            files.insert("spritesheet.svg".into(), data);
        }
        CodexKind::Png => codex_slice_png(raw, files, rows)?,
        _ => {
            // WebP / 动图：整幅按单帧播放，等比缩放进 192×208 格子
            files.insert(
                "spritesheet.webp".into(),
                format!("data:{};base64,{}", kind.mime(), base64_encode(raw)),
            );
            let delay = codex_gif_timing(raw).0.map(|d| (d.max(2) as u16) * 10).unwrap_or(120);
            rows.push((0, 1, vec![delay]));
        }
    }
    let _ = name;
    Ok(())
}

// Codex 行 -> DeskPet 八态：同义动作合并，缺失状态由前端回退 idle
fn codex_state_map(available: &[usize]) -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    let has = |row: usize| available.contains(&row);
    let put = |map: &mut serde_json::Map<String, Value>, st: &str, row: usize| {
        if !map.contains_key(st) && has(row) {
            map.insert(st.into(), Value::String(CODEX_ROW_NAMES[row].into()));
        }
    };
    put(&mut map, "idle", 0);
    let single = available.len() == 1; // WebP / 动图整幅只有一行，所有状态都用它
    put(&mut map, "work", if single { 0 } else { 7 }); // running：无方向性的「在干活」
    put(&mut map, "talk", if single { 0 } else { 6 }); // waiting：等待用户输入
    put(&mut map, "happy", if single { 0 } else { 3 }); // waving
    put(&mut map, "error", if single { 0 } else { 5 }); // failed
    put(&mut map, "click", if single { 0 } else { 4 }); // jumping
    put(&mut map, "drag", if single { 0 } else { 1 }); // running-right
    put(&mut map, "sleep", 0); // 契约没有睡眠行，回退待机
    if !map.contains_key("idle") {
        map.insert("idle".into(), Value::String("idle".into()));
    }
    map
}

// Codex 宠物包（zip）解析：只认 pet.json + spritesheet，严格校验契约
fn read_codex_zip(raw: &[u8]) -> Result<Value, String> {
    use std::io::Read;
    if raw.len() > SKIN_MAX_ZIP {
        return Err(format!("宠物包超过 {} MB 上限", SKIN_MAX_ZIP / 1024 / 1024));
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(raw))
        .map_err(|e| format!("不是有效的 zip：{}", e))?;
    if zip.len() > SKIN_MAX_ENTRIES {
        return Err("宠物包条目过多".into());
    }
    let mut total: u64 = 0;
    let mut entries: std::collections::BTreeMap<String, Vec<u8>> = std::collections::BTreeMap::new();
    let mut manifest: Option<Value> = None;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let name = f
            .mangled_name()
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("宠物包内文件名编码异常")?
            .to_string();
        // 只取 basename：即便条目写了 ../ 也无法逃出包外
        if name.starts_with('.') || name.contains('/') || name.contains('\\') {
            continue;
        }
        if f.size() > SKIN_MAX_FILE {
            return Err(format!("{} 超过 {} MB 单文件上限", name, SKIN_MAX_FILE / 1024 / 1024));
        }
        total += f.size();
        if total > SKIN_MAX_TOTAL {
            return Err("宠物包解压后总大小超限".into());
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf).map_err(|e| format!("读取 {} 失败：{}", name, e))?;
        if name.eq_ignore_ascii_case("pet.json") {
            let txt = String::from_utf8(buf).map_err(|_| "pet.json 不是 UTF-8 文本")?;
            manifest = Some(serde_json::from_str(&txt).map_err(|e| format!("pet.json 解析失败：{}", e))?);
            continue;
        }
        if codex_kind_of(&name).is_none() {
            continue;
        }
        entries.insert(name, buf);
    }
    let manifest = manifest.ok_or(
        "这不是 Codex 宠物包：缺少 pet.json（DeskPet 皮肤包请使用 .deskpet-skin.zip）",
    )?;
    if !manifest.is_object() {
        return Err("pet.json 必须是 JSON 对象".into());
    }
    let display = manifest["displayName"]
        .as_str()
        .or_else(|| manifest["name"].as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let fallback_id = manifest["id"].as_str().unwrap_or("").trim().to_string();
    if display.is_empty() && fallback_id.is_empty() {
        return Err("pet.json 缺少 id / displayName".into());
    }
    let display = if display.is_empty() {
        fallback_id.clone()
    } else {
        display
    };
    let mut id = codex_slug(&fallback_id);
    if id.is_empty() {
        id = codex_slug(&display);
    }
    if id.is_empty() {
        id = "codex-pet".into();
    }

    let declared = manifest["spritesheetPath"]
        .as_str()
        .unwrap_or("spritesheet.webp")
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("spritesheet.webp")
        .to_string();
    let sheet_kind = codex_kind_of(&declared).ok_or("spritesheetPath 必须是 .png / .webp / .gif / .svg")?;
    let (sheet_name, sheet_raw) = match entries.get(&declared) {
        Some(raw) => (declared.clone(), raw.clone()),
        None => entries
            .iter()
            .find(|(k, v)| {
                codex_kind_of(k) == Some(sheet_kind)
                    && codex_image_size(v, sheet_kind) == Some((CODEX_ATLAS_W, CODEX_ATLAS_H))
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .ok_or_else(|| format!("pet.json 声明的 spritesheetPath（{}）在包内不存在", declared))?,
    };

    let mut files: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut rows: Vec<(usize, usize, Vec<u16>)> = Vec::new();
    codex_read_atlas(&sheet_raw, sheet_kind, &sheet_name, &mut files, &mut rows)?;
    let available: Vec<usize> = rows.iter().map(|(r, _, _)| *r).collect();
    let row_meta: Vec<Value> = rows
        .iter()
        .map(|(r, n, d)| {
            // 在行尾把 durations 延长到 8 个，前端按列改时长时不必处理变长数组
            let mut all = [0u16; 8];
            for (i, v) in d.iter().enumerate().take(8) {
                all[i] = *v;
            }
            json!({
                "name": CODEX_ROW_NAMES[*r],
                "row": r,
                "frames": n,
                "delay": 120,
                "durations": all,
            })
        })
        .collect();

    Ok(json!({
        "format": "codex",
        "schema": "codex-pet-contract-1",
        "name": cut_chars(&display, 40),
        "id": cut_chars(&id, 40),
        "description": cut_chars(manifest["description"].as_str().unwrap_or(""), 240),
        "version": manifest["version"].as_str().unwrap_or("codex"),
        "license": manifest["license"].as_str().unwrap_or(""),
        "spritesheetPath": sheet_name,
        "atlas": {
            "width": CODEX_ATLAS_W, "height": CODEX_ATLAS_H,
            "cellW": CODEX_CELL_W, "cellH": CODEX_CELL_H,
            "cols": CODEX_COLS, "rows": CODEX_ROWS,
            "transparent": true,
            "spec": "codex-pet-contract",
        },
        "rows": row_meta,
        "states": codex_state_map(&available),
        "files": files,
        "sounds": {},
        "byteSize": total,
        "ts": now_ms(),
    }))
}

// DeskPet -> Codex 导出：Codex 来源按原帧还原，其余来源按八态映射回填契约行
fn codex_export_pack(pack: &Value) -> Result<(String, String, Vec<u8>), String> {
    let files = pack["files"].as_object().ok_or("皮肤包没有图片资源")?;
    let raw_frames: std::collections::BTreeMap<(usize, usize), String> = pack["frames"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| {
                    Some((
                        (f["row"].as_u64()? as usize, f["col"].as_u64()? as usize),
                        f["src"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    let break_on_missing = pack["format"].as_str() == Some("codex");
    let mut rows: Vec<(usize, Vec<Vec<u8>>)> = Vec::new();
    for row in 0..CODEX_ROWS {
        let name = CODEX_ROW_NAMES[row];
        let mut cells: Vec<Vec<u8>> = Vec::new();
        for col in 0..codex_row_max(row) {
            // 非 Codex 来源不经过逐帧记录，直接整行回填
            if break_on_missing && !codex_row_has_frame(pack, files, row, col) {
                break;
            }
            // 1) 行内在渲染层维护过的帧：优先使用逐帧记录
            let frame_src = raw_frames
                .get(&(row, col))
                .map(|s| s.as_str())
                .or_else(|| {
                    files
                        .get(&format!("codex-frame-{}-{}.png", row, col))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                });
            let cell = frame_src
                .and_then(codex_cell_from_url)
                .or_else(|| {
                    // 2) 兼容旧缓存：整行帧条被替换成了同一张 PNG
                    let row_src = files
                        .get(&format!("codex-row-{}-{}.png", row, name))
                        .and_then(|v| v.as_str())
                        .filter(|s| s.starts_with("data:image/png"))?;
                    let (_, payload) = row_src.split_once(',')?;
                    let img = png_decode(&base64_decode(payload).ok()?).ok()?;
                    if img.width == CODEX_CELL_W * codex_row_max(row) as u32 && img.height == CODEX_CELL_H {
                        Some(codex_cell_at(&img, col as u32))
                    } else {
                        codex_cell_from_url(row_src)
                    }
                });
            match cell {
                Some(c) => cells.push(c),
                None => break,
            }
        }
        // 3) 非 Codex 来源：从 DeskPet 八态映射回契约行（等比缩放 + 贴底）。
        //    DeskPet 皮肤包是「一状态一图」，整行复制同一张，保证每行帧数符合契约。
        if pack["format"].as_str() != Some("codex") {
            if let Some(cell) = codex_cell_from_state(pack, name) {
                cells.push(cell);
            }
        }
        if !cells.is_empty() {
            rows.push((row, cells));
        }
    }
    if rows.is_empty() {
        return Err("没有可导出的 Codex 行（需要至少一张待机图）".into());
    }
    let missing: Vec<&str> = (0..CODEX_ROWS)
        .filter(|r| !rows.iter().any(|(i, _)| i == r))
        .map(|r| CODEX_ROW_NAMES[r])
        .collect();

    // 4) 拼 8×9 图集：未使用的格子保持完全透明
    let mut canvas = vec![0u8; (CODEX_ATLAS_W * CODEX_ATLAS_H * 4) as usize];
    for (row, cells) in &rows {
        for (col, cell) in cells.iter().enumerate() {
            for y in 0..CODEX_CELL_H {
                let dst = ((row * CODEX_CELL_H as usize + y as usize) * CODEX_ATLAS_W as usize
                    + col * CODEX_CELL_W as usize)
                    * 4;
                let src = (y * CODEX_CELL_W) as usize * 4;
                canvas[dst..dst + CODEX_CELL_W as usize * 4]
                    .copy_from_slice(&cell[src..src + CODEX_CELL_W as usize * 4]);
            }
        }
    }
    let png = png_encode_rgba(CODEX_ATLAS_W, CODEX_ATLAS_H, &canvas)?;
    let display = pack["name"].as_str().unwrap_or("DeskPet 皮肤").trim();
    let display = if display.is_empty() { "DeskPet 皮肤" } else { display };
    let raw_id = pack["id"].as_str().unwrap_or("");
    let mut id = codex_slug(raw_id);
    if id.is_empty() {
        id = codex_slug(display);
    }
    if id.is_empty() {
        id = "deskpet-skin".into();
    }
    let mut manifest = json!({
        "id": id,
        "displayName": display,
        "description": pack["description"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("由 DeskPet 导出的 Codex 宠物：{}", cut_chars(display, 32))),
        "spritesheetPath": "spritesheet.png",
    });
    if !missing.is_empty() {
        manifest["deskpetNotes"] = json!({
            "missingRows": missing,
            "source": "deskpet",
        });
    }
    Ok((id, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?, png))
}

// 帧是否存在由 rows[].frames 决定（Codex 来源），否则看逐帧记录里是否出现过
fn codex_row_has_frame(pack: &Value, files: &serde_json::Map<String, Value>, row: usize, col: usize) -> bool {
    let codex_frames = pack["rows"]
        .as_array()
        .and_then(|a| a.iter().find(|r| r["row"].as_u64() == Some(row as u64)))
        .and_then(|r| r["frames"].as_u64())
        .map(|n| (col as u64) < n);
    match codex_frames {
        Some(has) => has,
        None => files
            .get(&format!("codex-frame-{}-{}.png", row, col))
            .map(|v| v.as_str().map(|s| !s.is_empty()).unwrap_or(true))
            .unwrap_or(false),
    }
}

fn codex_cell_at(img: &PngImage, col: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((CODEX_CELL_W * CODEX_CELL_H * 4) as usize);
    for y in 0..CODEX_CELL_H {
        for x in 0..CODEX_CELL_W {
            out.extend_from_slice(&img.pixel(col * CODEX_CELL_W + x, y));
        }
    }
    out
}

fn codex_cell_from_url(url: &str) -> Option<Vec<u8>> {
    let (_, payload) = url.split_once(',')?;
    let raw = base64_decode(payload).ok()?;
    let img = png_decode(&raw).ok()?;
    if img.width == CODEX_CELL_W && img.height == CODEX_CELL_H {
        Some(codex_cell_rgba(&img))
    } else {
        None
    }
}

fn codex_cell_rgba(img: &PngImage) -> Vec<u8> {
    let mut out = Vec::with_capacity((CODEX_CELL_W * CODEX_CELL_H * 4) as usize);
    for y in 0..CODEX_CELL_H {
        for x in 0..CODEX_CELL_W {
            out.extend_from_slice(&img.pixel(x, y));
        }
    }
    out
}

// 八态 -> 行：talk 优先取 waiting 语义（同一动作），其余按行名直取
fn codex_cell_from_state(pack: &Value, row_name: &str) -> Option<Vec<u8>> {
    let files = pack["files"].as_object()?;
    let state = match row_name {
        "idle" => "idle",
        "running" => "work",
        "waiting" => "talk",
        "waving" => "happy",
        "failed" => "error",
        "jumping" => "click",
        "running-right" => "drag",
        "running-left" => "drag",
        _ => return None,
    };
    // 依次尝试：显式映射的帧 -> 该状态的映射文件 -> 同名 PNG 资源
    let states = pack["states"].as_object();
    let mut urls: Vec<String> = Vec::new();
    if let Some(file) = states.and_then(|m| m.get(state)).and_then(|v| v.as_str()) {
        if let Some(u) = files.get(file).and_then(|v| v.as_str()) {
            urls.push(u.to_string());
        }
    }
    for (k, v) in files.iter() {
        if k.to_lowercase().ends_with(".png") {
            if let Some(u) = v.as_str().filter(|u| u.starts_with("data:image/png")) {
                urls.push(u.to_string());
                break;
            }
        }
    }
    let mut decoded = None;
    for url in urls.iter().filter(|u| u.starts_with("data:image/png")) {
        if let Some((_, payload)) = url.split_once(',') {
            if let Some(img) = base64_decode(payload).ok().and_then(|raw| png_decode(&raw).ok()) {
                decoded = Some(img);
                break;
            }
        }
    }
    let img = decoded?;
    let mut out = vec![0u8; (CODEX_CELL_W * CODEX_CELL_H * 4) as usize];
    // 等比缩放进格子并底部居中（Codex 要求同一条地面线）
    let scale = (CODEX_CELL_W as f64 / img.width as f64)
        .min(CODEX_CELL_H as f64 / img.height as f64);
    let w = (img.width as f64 * scale).round().max(1.0) as u32;
    let h = (img.height as f64 * scale).round().max(1.0) as u32;
    let ox = (CODEX_CELL_W - w) / 2;
    let oy = CODEX_CELL_H - h;
    for y in 0..h {
        for x in 0..w {
            let sx = (x as f64 / scale).min((img.width - 1) as f64) as u32;
            let sy = (y as f64 / scale).min((img.height - 1) as f64) as u32;
            let p = img.pixel(sx, sy);
            let o = (((y + oy) * CODEX_CELL_W + x + ox) * 4) as usize;
            out[o..o + 4].copy_from_slice(&p);
        }
    }
    Some(out)
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// 渲染层只传 base64（Tauri IPC 的数组序列化开销很大）
#[tauri::command]
fn skin_pack_import(data: String) -> Result<Value, String> {
    let raw = base64_decode(&data).map_err(|e| format!("皮肤包数据无效：{}", e))?;
    read_skin_zip(&raw)
}

// FR-SKN-003：Codex 宠物包（pet.json + 8×9 图集）导入 / 导出
#[tauri::command]
fn codex_pet_import(data: String) -> Result<Value, String> {
    let raw = base64_decode(&data).map_err(|e| format!("宠物包数据无效：{}", e))?;
    read_codex_zip(&raw)
}

#[tauri::command]
fn codex_pet_export(app: AppHandle, pack: Value) -> Result<String, String> {
    let (id, manifest, png) = codex_export_pack(&pack)?;
    let dir = app_export_dir(&app)?.join("codex-pets").join(sanitize_export_filename(&id)?);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("spritesheet.png"), png).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("pet.json"), manifest).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().to_string())
}

fn base64_decode(s: &str) -> Result<Vec<u8>, &'static str> {
    let mut buf: Vec<u8> = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' => continue,
            _ => return Err("非 base64 字符"),
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            buf.push((acc >> bits) as u8);
        }
    }
    Ok(buf)
}

// 皮肤维护：把前端维护后的皮肤包重新打包为 .deskpet-skin.zip（导出 / 备份）
fn is_safe_skin_entry(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.chars().any(|c| c.is_control())
}

fn data_url_bytes(data: &str) -> Result<(String, Vec<u8>), String> {
    let (meta, payload) = data.split_once(',').ok_or("资源不是有效的 data URL")?;
    if !meta.starts_with("data:") || !meta.contains(";base64") {
        return Err("资源不是 base64 data URL".into());
    }
    let mime = meta[5..]
        .split(';')
        .next()
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = base64_decode(payload).map_err(|e| format!("资源 base64 解码失败：{}", e))?;
    Ok((mime, bytes))
}

fn skin_zip_bytes(pack: &Value) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let files = pack["files"].as_object().ok_or("皮肤包没有图片资源")?;
    let sounds = pack["sounds"].as_object();
    let states = pack["states"].as_object();

    // 只导出标准八态中真实存在的映射，避免把损坏的引用写进新包
    let mut state_map = serde_json::Map::new();
    if let Some(states) = states {
        for st in SKIN_STATES {
            let Some(name) = states.get(st).and_then(|v| v.as_str()) else {
                continue;
            };
            if files.contains_key(name) && is_safe_skin_entry(name) {
                state_map.insert(st.to_string(), Value::String(name.to_string()));
            }
        }
    }
    if state_map.is_empty() {
        return Err("没有可导出的状态图片".into());
    }

    let name = pack["name"].as_str().unwrap_or("我的皮肤").trim();
    let name = if name.is_empty() { "我的皮肤" } else { name };
    let mut manifest = json!({
        "name": name,
        "version": pack["version"].as_str().unwrap_or("1.0"),
        "states": state_map,
    });
    if let Some(groups) = pack["group"].as_object() {
        let mut group = serde_json::Map::new();
        for (k, v) in groups {
            let Some(name) = v.as_str() else { continue };
            if files.contains_key(name) && is_safe_skin_entry(name) {
                group.insert(k.clone(), Value::String(name.to_string()));
            }
        }
        if !group.is_empty() {
            manifest["group"] = Value::Object(group);
        }
    }
    let mut sound_map = serde_json::Map::new();
    if let Some(sounds) = sounds {
        for (k, v) in sounds {
            let Some(data) = v.as_str() else { continue };
            // 前端 sounds 以「键 -> data URL」保存，导出时按 key 生成文件名
            let mime = data.split_once(',').map(|(m, _)| m).unwrap_or("");
            let ext = if mime.contains("mpeg") {
                "mp3"
            } else if mime.contains("wav") {
                "wav"
            } else {
                "ogg"
            };
            let safe = format!("{}.{}", k, ext)
                .chars()
                .filter(|c| !r#"\/:*?"<>|"#.contains(*c) && !c.is_control())
                .collect::<String>();
            if is_safe_skin_entry(&safe) {
                sound_map.insert(k.clone(), Value::String(safe));
            }
        }
    }
    if !sound_map.is_empty() {
        manifest["sound"] = Value::Object(sound_map.clone());
    }

    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("skin.json", opts)
        .map_err(|e| format!("写入 skin.json 失败：{}", e))?;
    zip.write_all(manifest.to_string().as_bytes())
        .map_err(|e| format!("写入 skin.json 失败：{}", e))?;

    // 只写入仍被状态机或 group 引用的图片；未引用资源由维护工具的「清理」显式处理
    let mut referenced: std::collections::BTreeSet<String> = manifest["states"]
        .as_object()
        .map(|m| m.values().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default();
    if let Some(group) = manifest["group"].as_object() {
        referenced.extend(
            group
                .values()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string()),
        );
    }
    for file_name in referenced {
        if !is_safe_skin_entry(&file_name) {
            continue;
        }
        let Some(data) = files.get(&file_name).and_then(|v| v.as_str()) else {
            continue;
        };
        let (_, bytes) = data_url_bytes(data)?;
        zip.start_file(file_name.as_str(), opts)
            .map_err(|e| format!("写入 {} 失败：{}", file_name, e))?;
        zip.write_all(&bytes)
            .map_err(|e| format!("写入 {} 失败：{}", file_name, e))?;
    }

    // 音效按 manifest.sound 的 key 保存，重新导入时即使被按文件名索引也不影响提醒音回退
    if let Some(sounds) = sounds {
        for (key, data) in sounds {
            let Some(data) = data.as_str() else { continue };
            let Some(entry) = sound_map.get(key).and_then(|v| v.as_str()) else {
                continue;
            };
            let (_, bytes) = data_url_bytes(data)?;
            zip.start_file(entry, opts)
                .map_err(|e| format!("写入 {} 失败：{}", entry, e))?;
            zip.write_all(&bytes)
                .map_err(|e| format!("写入 {} 失败：{}", entry, e))?;
        }
    }

    let cursor = zip
        .finish()
        .map_err(|e| format!("生成皮肤包失败：{}", e))?;
    Ok(cursor.into_inner())
}

#[tauri::command]
fn skin_pack_export(app: AppHandle, pack: Value) -> Result<String, String> {
    let bytes = skin_zip_bytes(&pack)?;
    let dir = app_export_dir(&app)?;
    let name = pack["name"].as_str().unwrap_or("deskpet-skin").trim();
    let name = if name.is_empty() { "deskpet-skin" } else { name };
    let safe = sanitize_export_filename(&format!("{}.deskpet-skin.zip", name))?;
    let path = dir.join(&safe);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn skin_image_export(app: AppHandle, name: String, data: String) -> Result<String, String> {
    let (mime, bytes) = data_url_bytes(&data)?;
    let ext = if mime.contains("gif") {
        "gif"
    } else if mime.contains("webp") {
        "webp"
    } else {
        "png"
    };
    let dir = app_export_dir(&app)?;
    let raw = name.trim();
    let stem = std::path::Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("skin-image");
    let safe = sanitize_export_filename(&format!("{}.{}", stem, ext))?;
    let path = dir.join(&safe);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn clipboard_read() -> Result<Value, String> {
    let text = clipboard_read_text().ok_or("剪贴板中没有纯文本")?;
    Ok(json!({"text": cut_chars(&text, 8000), "truncated": text.chars().count() > 8000}))
}

#[tauri::command]
fn tool_auth_reply(ctx: tauri::State<AppCtx>, id: String, allow: bool, remember: bool) -> Result<(), String> {
    let w = ctx
        .tool_wait
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id)
        .ok_or("授权请求已过期或已被处理")?;
    // 接收端已超时退出时发送会失败，属于正常情况
    let _ = w.tx.send(ToolDecision { allow, remember });
    Ok(())
}

// 授权 id 需要唯一（同一毫秒内可能连续发起多个工具调用）
fn rand_suffix() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{:x}", n)
}

#[tauri::command]
fn chat_stop(ctx: tauri::State<AppCtx>) {
    ctx.stop.store(true, Ordering::SeqCst);
    ctx.chat_cancelled.store(true, Ordering::SeqCst);
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
    // 前端据此更新 petSc 与舞台 transform（此前无事件导致缩放视觉失效）
    let _ = app.emit_to("pet", "state-changed", json!({"origin": "backend"}));
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
fn set_pet_taskbar(app: AppHandle, ctx: tauri::State<AppCtx>, show: bool) -> Result<(), String> {
    // FR-PET-003：宠物窗口默认不显示任务栏图标，用户可配置
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    pet.set_skip_taskbar(!show).map_err(|e| e.to_string())?;
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["showInTaskbar"] = json!(show);
    }
    persist_state(&ctx);
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
    Ok(())
}

#[tauri::command]
fn set_pet_always_on_top(app: AppHandle, ctx: tauri::State<AppCtx>, on: bool) -> Result<(), String> {
    // FR-PET-002：默认置顶，支持配置（关闭后普通层级）
    let pet = app.get_webview_window("pet").ok_or("pet 窗口不存在")?;
    pet.set_always_on_top(on).map_err(|e| e.to_string())?;
    apply_top_level(&pet, on);
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["settings"]["alwaysOnTop"] = json!(on);
    }
    persist_state(&ctx);
    let _ = app.emit("state-changed", json!({"origin": "backend"}));
    Ok(())
}

#[tauri::command]
fn privacy_info(app: AppHandle, ctx: tauri::State<AppCtx>) -> Value {
    // FR-SYS-003 / 3.2.4 / NFR-PRV-001~003：隐私页所需事实，只读且不包含任何密钥内容
    let dir = app
        .path()
        .app_data_dir()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default();
    let legacy_plaintext = {
        let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        st["providers"]
            .as_array()
            .map(|a| {
                a.iter().any(|p| {
                    p.get("apiKey")
                        .and_then(|v| v.as_str())
                        .map(|s| !s.trim().is_empty())
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    };
    json!({
        "dataDir": dir,
        // 仅返回中性技术标识，文案与语言由前端 i18n 决定，避免英文界面出现中文
        "keyStoreName": KEY_SERVICE,
        "keyStore": "Windows Credential Manager",
        "telemetry": false,
        "localOnly": true,
        "legacyPlaintextKeys": legacy_plaintext,
    })
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
    *ctx.pet_visible.lock().unwrap_or_else(|e| e.into_inner()) = visible;
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
    let h_log = height.clamp(80.0, 360.0);
    let new_h = (h_log * scale).round() as u32;
    let pos = bubble.outer_position().map_err(|e| e.to_string())?;
    let size = bubble.outer_size().map_err(|e| e.to_string())?;
    let below = *ctx.bubble_anchor_below.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut q = ctx.dnd_queue.lock().unwrap_or_else(|e| e.into_inner());
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
fn reminder_snooze(
    app: AppHandle,
    ctx: tauri::State<AppCtx>,
    id: String,
    content: Option<String>,
) -> Result<(), String> {
    // FR-REM：稍后提醒必须保留原始内容。已触发的一次性提醒不在 reminders 中，
    // 因此优先使用前端传来的内容，其次从待播报队列中找回。
    let now = now_ms();
    let pending_content = {
        let p = ctx.pending.lock().unwrap_or_else(|e| e.into_inner());
        p.iter()
            .find(|n| n["reminder"]["id"].as_str() == Some(id.as_str()))
            .and_then(|n| n["reminder"]["content"].as_str().map(|s| s.to_string()))
    };
    let text = content
        .filter(|s| !s.trim().is_empty())
        .or(pending_content)
        .unwrap_or_else(|| "提醒".to_string());
    {
        let mut st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
        if !st["reminders"].is_array() {
            st["reminders"] = json!([]);
        }
        if let Some(rems) = st["reminders"].as_array_mut() {
            if let Some(r) = rems.iter_mut().find(|r| r["id"].as_str() == Some(id.as_str())) {
                r["due"] = json!(now + 5 * 60_000);
                r["rule"] = json!("once");
                r["enabled"] = json!(true);
                if r["content"].as_str().map(|s| s.trim().is_empty()).unwrap_or(true) {
                    r["content"] = json!(text);
                }
            } else {
                rems.push(json!({
                    "id": id,
                    "content": text,
                    "due": now + 5 * 60_000,
                    "rule": "once",
                    "enabled": true
                }));
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
    let mut p = ctx.pending.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *p)
}

// 导出目录统一放在应用数据目录的 exports 下
fn app_export_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

// 只允许纯文件名：拒绝路径分隔符、控制字符、"."/".." 与 Windows 保留设备名
fn sanitize_export_filename(filename: &str) -> Result<String, String> {
    let mut safe: String = filename
        .chars()
        .filter(|c| !r#"\/:*?"<>|"#.contains(*c) && !c.is_control())
        .collect();
    safe = safe.trim().trim_end_matches(['.', ' ']).to_string();
    if safe.is_empty() || safe == "." || safe == ".." {
        return Err("文件名无效".into());
    }
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = safe.split('.').next().unwrap_or("").trim().to_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        safe = format!("_{}", safe);
    }
    Ok(safe)
}

#[tauri::command]
fn export_text(app: AppHandle, filename: String, content: String) -> Result<String, String> {
    let dir = app_export_dir(&app)?;
    let safe = sanitize_export_filename(&filename)?;
    let path = dir.join(&safe);
    if path.parent() != Some(dir.as_path()) {
        return Err("文件名无效".into());
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_app_version(app: AppHandle) -> String {
    // 版本号唯一来源：tauri.conf.json（此前硬编码曾与实际版本脱节）。
    // 只返回纯版本号：语言与「桌面版」等措辞由前端 i18n 决定，避免英文界面出现中文。
    app.package_info().version.to_string()
}

// FR-SYS-004：检查更新。仅在用户点击「检查更新」时发起一次 HTTPS GET，
// 读取托管在 updateUrl 的 JSON（形如 {"version":"2.0.0","notes":"…","url":"https://…"}）。
// 不做后台轮询；安装包必须由用户手动下载并自行运行，应用不会静默执行任何可执行文件。
fn version_tuple(v: &str) -> (u64, u64, u64) {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or("");
    let mut it = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

#[tauri::command]
async fn update_check(app: AppHandle, manual: Option<bool>) -> Result<Value, String> {
    let (url, enabled) = {
        let ctx = app.state::<AppCtx>();
        let st = state_lock(&ctx);
        (
            st["settings"]["updateUrl"].as_str().unwrap_or("").to_string(),
            st["settings"]["updateAuto"].as_bool().unwrap_or(false),
        )
    };
    if url.trim().is_empty() {
        return Ok(json!({"ok": false, "message": "未配置更新源地址"}));
    }
    // 自动检查（manual=false）需用户开启；手动点击「检查更新」不受开关限制
    if !enabled && !manual.unwrap_or(false) {
        return Ok(json!({"ok": false, "message": "已关闭更新检查"}));
    }
    let u = reqwest::Url::parse(url.trim()).map_err(|_| "更新地址格式无效".to_string())?;
    if u.scheme() != "https" {
        return Err("更新地址必须使用 HTTPS（NFR-SEC-004）".into());
    }
    let host = u.host_str().unwrap_or("").to_string();
    let client = app.state::<AppCtx>().http.clone();
    let t0 = std::time::Instant::now();
    let resp = client
        .get(u)
        .header("user-agent", "DeskPet-UpdateCheck")
        .timeout(Duration::from_secs(12))
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            record_outbound(&app, &host, "update", "", None, false, t0.elapsed().as_millis() as u64, 0);
            return Ok(json!({"ok": false, "message": net_error_msg(&e)}));
        }
    };
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    record_outbound(&app, &host, "update", "", Some(status), (200..300).contains(&status), t0.elapsed().as_millis() as u64, 0);
    if !(200..300).contains(&status) {
        return Ok(json!({"ok": false, "message": format!("更新源返回 HTTP {}", status)}));
    }
    let j: Value = serde_json::from_str(&body).map_err(|_| "更新清单不是合法 JSON".to_string())?;
    let latest = j["version"].as_str().unwrap_or("").to_string();
    if latest.is_empty() {
        return Ok(json!({"ok": false, "message": "更新清单缺少 version 字段"}));
    }
    let current = app.package_info().version.to_string();
    let newer = version_tuple(&latest) > version_tuple(&current);
    let dl = j["url"].as_str().unwrap_or("").to_string();
    if newer && !dl.is_empty() && !dl.starts_with("https://") {
        return Ok(json!({"ok": false, "message": "更新下载地址必须使用 HTTPS"}));
    }
    Ok(json!({
        "ok": true,
        "current": current,
        "latest": latest,
        "newer": newer,
        "notes": cut_chars(j["notes"].as_str().unwrap_or(""), 600),
        "url": dl,
        "checkedAt": now_ms(),
    }))
}

// FR-SYS-004：用户确认后手动下载安装包到 exports 目录，再由用户自行运行；
// 应用只负责下载与校验大小，绝不自动执行。
#[tauri::command]
async fn update_download(app: AppHandle, url: String) -> Result<String, String> {
    let u = reqwest::Url::parse(url.trim()).map_err(|_| "下载地址无效".to_string())?;
    if u.scheme() != "https" {
        return Err("下载地址必须使用 HTTPS（NFR-SEC-004）".into());
    }
    let host = u.host_str().unwrap_or("").to_string();
    let name = u
        .path_segments()
        .and_then(|s| s.last())
        .filter(|s| !s.is_empty() && s.len() < 120)
        .unwrap_or("DeskPet-setup")
        .to_string();
    let client = app.state::<AppCtx>().http.clone();
    let t0 = std::time::Instant::now();
    let resp = client
        .get(u)
        .header("user-agent", "DeskPet-UpdateCheck")
        .timeout(Duration::from_secs(120))
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            record_outbound(&app, &host, "update-dl", "", None, false, t0.elapsed().as_millis() as u64, 0);
            return Err(net_error_msg(&e));
        }
    };
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        record_outbound(&app, &host, "update-dl", "", Some(status), false, t0.elapsed().as_millis() as u64, 0);
        return Err(format!("下载失败：HTTP {}", status));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("下载中断：{}", e))?;
    if bytes.len() > 200 * 1024 * 1024 {
        return Err("安装包超过 200 MB，已中止".into());
    }
    record_outbound(&app, &host, "update-dl", "", Some(status), true, t0.elapsed().as_millis() as u64, 0);
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = name.chars().filter(|c| !r#"\/*:"<>?|"#.contains(*c)).collect();
    let path = dir.join(format!("{}-{}", now_ms(), safe));
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    log_event(&app, "info", "update.downloaded", &format!("已下载更新包（{} 字节），等待用户手动安装", bytes.len()));
    Ok(path.to_string_lossy().to_string())
}

// 在资源管理器中定位已下载的安装包（仅打开文件夹，不执行文件）
#[tauri::command]
fn reveal_file(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("文件不存在".into());
    }
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", p.to_string_lossy()))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开目录失败：{}", e))
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
                chat_seq: std::sync::atomic::AtomicU64::new(0),
                chat_cancelled: AtomicBool::new(false),
                bubble_anchor_below: Mutex::new(false),
                fg_before: Mutex::new(0),
                pet_visible: Mutex::new(true),
                pending: Mutex::new(vec![]),
                dnd_queue: Mutex::new(vec![]),
                file_lock: Mutex::new(()),
                tool_wait: Mutex::new(std::collections::HashMap::new()),
                clipboard_last: Mutex::new(String::new()),
                http,
            });

            // DS-001 / NFR-SEC-001：迁移旧版可能残留的明文 API Key
            migrate_legacy_secrets(app.handle());

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

            // FR-PET-002/003：应用持久化的置顶层级与任务栏显示设置
            let (show_in_taskbar, always_on_top) = {
                let ctx = app.state::<AppCtx>();
                let st = ctx.state.lock().unwrap_or_else(|e| e.into_inner());
                (
                    st["settings"]["showInTaskbar"].as_bool().unwrap_or(false),
                    st["settings"]["alwaysOnTop"].as_bool().unwrap_or(true),
                )
            };
            if let Some(pet) = app.get_webview_window("pet") {
                let _ = pet.set_skip_taskbar(!show_in_taskbar);
                let _ = pet.set_always_on_top(always_on_top);
            }

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
                                let pv = *ctx.pet_visible.lock().unwrap_or_else(|e| e.into_inner());
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

            migrate_legacy_log(app.handle());
            log_event(app.handle(), "info", "app.started", "DeskPet 桌面版启动完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            state_load, state_save, reset_data,
            key_set, key_delete, key_has,
            provider_test,
            chat_start, chat_stop, chat_summarize, tool_auth_reply, clipboard_read, skin_pack_import,
            skin_pack_export, skin_image_export, codex_pet_import, codex_pet_export,
            pet_clicked, pet_save_pos, pet_set_size, set_pet_scale,
            set_click_through, set_pet_visible, reset_pet_pos,
            show_bubble_cmd, hide_bubble_cmd, bubble_resize,
            bubble_reposition, bubble_set_width,
            open_settings_cmd, set_dnd,
            set_pet_taskbar, set_pet_always_on_top,
            privacy_info,
            reminder_add, reminder_snooze, reminder_delete,
            take_pending_notices,
            export_text, quit_app, get_app_version, update_check, update_download, reveal_file
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
                proxy: String::new(),
                proxy_type: "auto".into(),
                temp: 0.8,
                top_p: 1.0,
                max_tokens: 1024,
                ctx_tokens: 8000,
                timeout: 30,
                retry: 2,
                stream: true,
                ollama: true,
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
        fn summary_mock_uses_user_turns_and_never_leaks_empty() {
            let msgs = vec![
                ChatMsg { role: "user".into(), content: "我喜欢蓝色".into(), tool_calls: None, tool_call_id: None, name: None },
                ChatMsg { role: "assistant".into(), content: "记住了".into(), tool_calls: None, tool_call_id: None, name: None },
                ChatMsg { role: "user".into(), content: "下周要交周报".into(), tool_calls: None, tool_call_id: None, name: None },
            ];
            let s = mock_summary(&msgs);
            assert!(s.contains("我喜欢蓝色"));
            assert!(s.contains("下周要交周报"));
            assert!(!s.contains("记住了"));
            assert!(!mock_summary(&[]).is_empty());
        }

        #[test]
        fn logs_redact_common_secret_shapes() {
            let out = redact_secrets("key=sk-abc123DEF456 bearer xyzTOKEN authorization: Bearer secret-value");
            assert!(!out.contains("abc123DEF456"), "{}", out);
            assert!(!out.contains("xyzTOKEN"), "{}", out);
            assert!(!out.contains("secret-value"), "{}", out);
            assert!(out.contains("***"));
            // 普通文案不被误伤
            assert_eq!(redact_secrets("提醒已创建"), "提醒已创建");
        }

        #[test]
        fn log_timestamp_is_iso_and_civil_dates_are_correct() {
            assert_eq!(days_to_ymd(0), (1970, 1, 1));
            assert_eq!(days_to_ymd(10957), (2000, 1, 1));
            // 闰日：2024-02-29
            assert_eq!(days_to_ymd(19782), (2024, 2, 29));
            let ts = chrono_now();
            assert_eq!(ts.len(), 25, "{}", ts);
            assert_eq!(&ts[4..5], "-");
            assert_eq!(&ts[10..11], "T");
            assert!(ts.ends_with("+08:00"), "{}", ts);
        }

        #[test]
        fn legacy_log_detection_handles_mixed_files() {
            assert!(!has_legacy_log_lines(""));
            assert!(!has_legacy_log_lines("{\"ts\":\"x\"}\n\n{\"ts\":\"y\"}\n"));
            assert!(has_legacy_log_lines("[D1 00:00:00] 旧日志\n{\"ts\":\"y\"}\n"));
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
                chat_seq: std::sync::atomic::AtomicU64::new(0),
                chat_cancelled: AtomicBool::new(false),
                bubble_anchor_below: Mutex::new(false),
                fg_before: Mutex::new(0),
                pet_visible: Mutex::new(true),
                pending: Mutex::new(vec![]),
                dnd_queue: Mutex::new(vec![]),
                file_lock: Mutex::new(()),
                tool_wait: Mutex::new(std::collections::HashMap::new()),
                clipboard_last: Mutex::new(String::new()),
                http: reqwest::Client::new(),
            };
            {
                let _guard = ctx.state.lock().unwrap(); // 模拟调度器持锁
                write_state_file(&ctx, "{\"ok\":true}"); // 若内部再取锁 → 此处死锁
            }
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"ok\":true}");
            let _ = std::fs::remove_file(&path);
        }

        fn rem(id: &str, due: i64, rule: &str) -> Value {
            json!({"id": id, "content": id, "due": due, "rule": rule, "enabled": true})
        }

        #[test]
        fn reminders_fire_before_reschedule() {
            // 回归：每日提醒曾被直接顺延且不播报；重复提醒必须先触发再顺延
            let now = 1_000_000_000_000;
            let mut rems = vec![
                rem("daily", now - 1, "daily"),
                rem("weekly", now - 1, "weekly"),
                json!({"id":"interval","content":"interval","due": now - 1,"rule":"interval","intervalMin":30,"enabled":true}),
                rem("once", now - 1, "once"),
                rem("future", now + 60_000, "daily"),
            ];
            let (fired, changed) = process_reminders(&mut rems, now);
            let ids: Vec<&str> = fired.iter().filter_map(|r| r["id"].as_str()).collect();
            assert_eq!(ids, vec!["daily", "weekly", "interval", "once"]);
            assert!(changed);
            // once 被移除；其余重复项保留且 due 已推进到未来
            assert_eq!(rems.len(), 4);
            for r in &rems {
                if r["id"] == "future" {
                    assert_eq!(r["due"].as_i64().unwrap(), now + 60_000);
                } else {
                    assert!(r["due"].as_i64().unwrap() > now, "{:?}", r);
                }
            }
            let daily = rems.iter().find(|r| r["id"] == "daily").unwrap();
            assert_eq!(daily["due"].as_i64().unwrap(), now - 1 + 86_400_000);
            let interval = rems.iter().find(|r| r["id"] == "interval").unwrap();
            assert_eq!(interval["due"].as_i64().unwrap(), now - 1 + 1_800_000);
        }

        #[test]
        fn reminders_disabled_and_far_past() {
            let now = 1_700_000_000_000;
            let mut rems = vec![
                json!({"id":"off","content":"off","due":now - 1,"rule":"once","enabled":false}),
                rem("stale", now - 3 * 86_400_000, "daily"),
                json!({"id":"bad","content":"bad","due":now - 1,"rule":"interval","intervalMin":0,"enabled":true}),
            ];
            let (fired, changed) = process_reminders(&mut rems, now);
            assert_eq!(fired.len(), 2); // off 不触发
            assert!(changed);
            // intervalMin 非法时按 1 分钟兜底，继续推进到未来
            let bad = rems.iter().find(|r| r["id"] == "bad").unwrap();
            assert!(bad["due"].as_i64().unwrap() > now);
        }

        #[test]
        fn context_trim_keeps_system_messages() {
            let mut msgs = vec![
                ChatMsg { role: "system".into(), content: "SYS".into(), ..Default::default() },
                ChatMsg { role: "user".into(), content: "u1".into(), ..Default::default() },
                ChatMsg { role: "assistant".into(), content: "a1".into(), ..Default::default() },
                ChatMsg { role: "user".into(), content: "u2".into(), ..Default::default() },
            ];
            assert!(trim_context(&mut msgs));
            assert_eq!(msgs.len(), 2);
            assert_eq!(msgs[0].role, "system");
            assert_eq!(msgs[1].content, "u2");
            // 只剩 system 时无法再裁剪
            let mut only_sys = vec![ChatMsg { role: "system".into(), content: "SYS".into(), ..Default::default() }];
            assert!(!trim_context(&mut only_sys));
        }

        #[test]
        fn detects_context_overflow_messages() {
            assert!(is_context_too_long("This model's maximum context length is 8192 tokens"));
            assert!(is_context_too_long("上下文长度超出限制"));
            assert!(!is_context_too_long("rate limit exceeded"));
        }

        #[test]
        fn azure_url_and_endpoint_detection() {
            assert!(is_azure_endpoint("https://foo.openai.azure.com/openai/deployments/gpt4"));
            assert!(!is_azure_endpoint("https://api.openai.com/v1"));
            let u = azure_url("https://foo.openai.azure.com/openai/deployments/gpt4", "/chat/completions");
            assert!(u.ends_with("/chat/completions?api-version=2024-02-01"), "{}", u);
            let u2 = azure_url("https://foo.openai.azure.com/x?api-version=2024-06-01", "/chat/completions");
            assert!(u2.contains("api-version=2024-06-01"));
            assert_eq!(u2.matches("api-version=").count(), 1);
        }

        #[test]
        fn state_merge_protects_backend_authority() {
            let mut st = json!({
                "reminders": [{"id":"r1","content":"keep"}],
                "settings": {"petScale": 1.4, "dnd": true, "petPos": {"x": 10, "y": 20}, "skin": "blue", "customSkin": {"name":"a.gif"}},
                "sessions": []
            });
            let incoming = json!({
                "reminders": [],
                "settings": {"petScale": 1.0, "dnd": false, "petPos": {"x": 0, "y": 0}, "skin": "orange", "customSkin": null},
                "sessions": [{"id":"s1"}]
            });
            merge_state_snapshot(&mut st, incoming, "bubble");
            assert_eq!(st["reminders"][0]["id"], "r1");
            assert_eq!(st["settings"]["petScale"], 1.4);
            assert_eq!(st["settings"]["dnd"], true);
            assert_eq!(st["settings"]["petPos"]["x"], 10);
            assert_eq!(st["settings"]["customSkin"]["name"], "a.gif");
            // 非设置窗口可以修改皮肤
            assert_eq!(st["settings"]["skin"], "orange");
            assert_eq!(st["sessions"][0]["id"], "s1");

            // 设置窗口的修改是权威的
            let incoming2 = json!({
                "reminders": [],
                "settings": {"petScale": 0.8, "dnd": false, "petPos": {"x": 0, "y": 0}},
                "sessions": []
            });
            merge_state_snapshot(&mut st, incoming2, "settings");
            assert_eq!(st["settings"]["petScale"], 0.8);
            assert_eq!(st["settings"]["dnd"], false);
            // 宠物位置仍由后端命令维护
            assert_eq!(st["settings"]["petPos"]["x"], 10);
            assert_eq!(st["reminders"][0]["id"], "r1");
        }

        #[test]
        fn upgrade_fills_missing_defaults_without_overwriting_user_data() {
            let mut loaded = json!({
                "settings": {"lowFps": false, "voice": true, "bubbleW": 300},
                "sessions": [{"id": "s-user"}],
                "providers": [{"id": "p-user"}]
            });
            fill_defaults(&default_state(), &mut loaded);
            // 旧文件缺失的新字段补默认值
            assert_eq!(loaded["settings"]["summaryThreshold"], 24);
            assert_eq!(loaded["settings"]["voiceRate"], 1);
            assert_eq!(loaded["settings"]["dndStart"], "");
            // 用户已有值不被覆盖
            assert_eq!(loaded["settings"]["lowFps"], false);
            assert_eq!(loaded["settings"]["voice"], true);
            assert_eq!(loaded["settings"]["bubbleW"], 300);
            // 数组整体保留，不混入默认示例
            assert_eq!(loaded["sessions"][0]["id"], "s-user");
            assert_eq!(loaded["providers"][0]["id"], "p-user");
            // 嵌套对象也只补缺失键
            assert!(loaded["settings"]["toolSettings"]["openWhitelist"].is_array());
        }

        #[test]
        fn write_state_file_is_atomic_and_backs_up() {
            let dir = std::env::temp_dir().join("deskpet-regression-atomic");
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join("data.json");
            let ctx = AppCtx {
                data_path: path.clone(),
                state: Mutex::new(json!({})),
                stop: AtomicBool::new(false),
                chat_seq: std::sync::atomic::AtomicU64::new(0),
                chat_cancelled: AtomicBool::new(false),
                bubble_anchor_below: Mutex::new(false),
                fg_before: Mutex::new(0),
                pet_visible: Mutex::new(true),
                pending: Mutex::new(vec![]),
                dnd_queue: Mutex::new(vec![]),
                file_lock: Mutex::new(()),
                tool_wait: Mutex::new(std::collections::HashMap::new()),
                clipboard_last: Mutex::new(String::new()),
                http: reqwest::Client::new(),
            };
            write_state_file(&ctx, "{\"v\":1}");
            write_state_file(&ctx, "{\"v\":2}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"v\":2}");
            let bak = path.with_extension("json.bak");
            assert_eq!(std::fs::read_to_string(&bak).unwrap(), "{\"v\":1}");
            assert!(!path.with_extension("json.tmp").exists());
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn token_budget_trims_oldest_non_system() {
            // CJK 约 1 字 1 token；仅 system 时不得无限裁剪
            let mut msgs = vec![
                ChatMsg { role: "system".into(), content: "系统提示".into(), ..Default::default() },
                ChatMsg { role: "user".into(), content: "很早的消息".repeat(50), ..Default::default() },
                ChatMsg { role: "assistant".into(), content: "较早的回复".repeat(50), ..Default::default() },
                ChatMsg { role: "user".into(), content: "最新问题".into(), ..Default::default() },
            ];
            let before = msgs.len();
            assert!(trim_to_budget(&mut msgs, 80));
            assert!(msgs.len() < before);
            assert_eq!(msgs[0].role, "system");
            assert_eq!(msgs.last().unwrap().content, "最新问题");
            assert!(est_tokens("你好") >= 2);
            assert!(est_tokens("hello") <= 3);
            let mut only_sys = vec![ChatMsg { role: "system".into(), content: "很长".repeat(200), ..Default::default() }];
            assert!(!trim_to_budget(&mut only_sys, 10));
        }

        #[test]
        fn mock_chunks_split_on_char_boundary() {
            // 回归：v0.2.7 按字节切片，中文/emoji 落在非字符边界 → panic → 进程崩溃
            let s = "你好，我是团子🐱。**粗体** abc";
            let out = mock_chunks(s);
            assert_eq!(out.concat(), s);
            let long = "专注搭子番茄钟提醒".repeat(60);
            let parts = mock_chunks(&long);
            assert!(parts.len() > 1);
            assert_eq!(parts.concat(), long);
            assert!(mock_chunks("").is_empty());
        }

        #[test]
        fn ollama_endpoint_detection_and_urls() {
            assert!(is_ollama_endpoint("http://localhost:11434/v1"));
            assert!(is_ollama_endpoint("http://127.0.0.1:11434"));
            assert!(is_ollama_endpoint("https://ollama.example.com/v1"));
            assert!(!is_ollama_endpoint("https://api.deepseek.com/v1"));
            assert_eq!(ollama_chat_url("http://localhost:11434/v1"), "http://localhost:11434/api/chat");
            assert_eq!(ollama_chat_url("http://localhost:11434"), "http://localhost:11434/api/chat");
            assert_eq!(ollama_chat_url("http://localhost:11434/api"), "http://localhost:11434/api/chat");
            assert_eq!(ollama_chat_url("http://localhost:11434/api/chat"), "http://localhost:11434/api/chat");
            assert_eq!(ollama_root("http://localhost:11434/v1/"), "http://localhost:11434");
        }

        #[test]
        fn ollama_ndjson_parsing_and_oom_hint() {
            let line = r#"{"message":{"role":"assistant","content":"hi"},"done":false}"#;
            assert_eq!(ollama_delta(line).unwrap(), "hi");
            assert!(ollama_delta(r#"{"message":{"content":""},"done":true}"#).is_none());
            assert!(ollama_line_error(
                r#"{"error":"llama-server reported out-of-memory during startup"}"#
            )
            .unwrap()
            .contains("out-of-memory"));
            assert!(ollama_delta(r#"{"response":"pong","done":true}"#).unwrap() == "pong");
            assert!(is_local_oom(
                "llama-server startup failed before projector CPU offload retry: GGML_ASSERT(buffer) failed"
            ));
            assert!(is_local_oom("failed to allocate CPU buffer of size 671354880"));
            assert!(!is_local_oom("rate limited"));
            let hint = local_oom_hint("qwen3.5:2b", "out-of-memory");
            assert!(hint.contains("qwen3.5:2b"));
            assert!(hint.contains("上下文 Token 上限"));
            // 非 OOM 错误不应被误包装成内存建议
            assert_eq!(
                ollama_error_text("m", "model 'x' not found"),
                "model 'x' not found"
            );
            assert!(ollama_error_text("m", "failed to allocate CPU buffer").contains("内存不足"));
        }

        // FR-SKN-002：皮肤包解析（正常、缺清单、路径穿越、超大文件）
        fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
            use std::io::Write;
            let mut buf = std::io::Cursor::new(Vec::new());
            {
                let mut w = zip::ZipWriter::new(&mut buf);
                let opts = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored);
                for (name, data) in entries {
                    w.start_file(*name, opts).unwrap();
                    w.write_all(data).unwrap();
                }
                w.finish().unwrap();
            }
            buf.into_inner()
        }

#[cfg(test)]
mod codex_contract_tests {
    use super::*;

    fn tiny_png(width: u32, height: u32, fill: [u8; 4]) -> Vec<u8> {
        let px = fill.repeat((width * height) as usize);
        png_encode_rgba(width, height, &px).unwrap()
    }

    fn atlas_png() -> Vec<u8> {
        // 每行只用契约规定的前几列，其余列保持完全透明
        let (w, h) = (CODEX_ATLAS_W, CODEX_ATLAS_H);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for (row, cols) in [(0usize, 6u32), (1, 8), (2, 8), (3, 4), (4, 5), (5, 8), (6, 6), (7, 6), (8, 6)] {
            for col in 0..cols {
                for y in 0..CODEX_CELL_H {
                    for x in 0..CODEX_CELL_W {
                        let o = (((row as u32 * CODEX_CELL_H + y) * w) + col * CODEX_CELL_W + x) * 4;
                        px[o as usize..o as usize + 4]
                            .copy_from_slice(&[30 + row as u8, 60 + col as u8, 120, 255]);
                    }
                }
            }
        }
        png_encode_rgba(w, h, &px).unwrap()
    }

    #[test]
    fn png_edit_roundtrip_is_lossless() {
        let img = atlas_png();
        let back = png_decode(&img).unwrap();
        assert_eq!((back.width, back.height), (CODEX_ATLAS_W, CODEX_ATLAS_H));
        assert_eq!(back.pixel(0, 0), [30, 60, 120, 255]);
        // 第 0 行第 6 列之外：未使用列必须仍然完全透明
        let unused = back.pixel(6 * CODEX_CELL_W + 10, 10);
        assert_eq!(unused[3], 0);
        // 第 4 行只要 5 帧，第 5 列必须透明
        assert_eq!(back.pixel(5 * CODEX_CELL_W + 10, 4 * CODEX_CELL_H + 10)[3], 0);
    }

    #[test]
    fn codex_zip_parses_contract_atlas() {
        let zip = build_zip(&[
            ("pet.json", r#"{"id":"round-cat-codex","displayName":"Round Cat","description":"demo","spritesheetPath":"spritesheet.png"}"#.as_bytes()),
            ("spritesheet.png", &atlas_png()),
            ("../../evil.png", &tiny_png(8, 8, [255, 0, 0, 255])),
        ]);
        let pack = read_codex_zip(&zip).unwrap();
        assert_eq!(pack["format"], "codex");
        assert_eq!(pack["name"], "Round Cat");
        assert_eq!(pack["id"], "round-cat-codex");
        assert_eq!(pack["spritesheetPath"], "spritesheet.png");
        assert_eq!(pack["atlas"]["width"], 1536);
        assert_eq!(pack["rows"].as_array().unwrap().len(), 9);
        // 行名与契约行序一致
        let names: Vec<&str> = pack["rows"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
        assert_eq!(names, CODEX_ROW_NAMES.to_vec());
        // 未使用列不产生帧：idle 6 帧、waving 4 帧
        let rows = pack["rows"].as_array().unwrap();
        assert_eq!(rows[0]["frames"], 6);
        assert_eq!(rows[3]["frames"], 4);
        // 八态映射：running -> work，waiting -> talk，failed -> error
        assert_eq!(pack["states"]["idle"], "idle");
        assert_eq!(pack["states"]["work"], "running");
        assert_eq!(pack["states"]["talk"], "waiting");
        assert_eq!(pack["states"]["error"], "failed");
        assert_eq!(pack["states"]["drag"], "running-right");
        // 路径穿越的条目被丢弃
        assert!(!pack["files"].as_object().unwrap().keys().any(|k| k.contains("evil")));
    }

    #[test]
    fn codex_zip_rejects_wrong_size_and_missing_manifest() {
        let small = tiny_png(192, 208, [1, 2, 3, 255]);
        let bad = build_zip(&[
            ("pet.json", br#"{"id":"a","displayName":"A","spritesheetPath":"spritesheet.png"}"#),
            ("spritesheet.png", &small),
        ]);
        assert!(read_codex_zip(&bad).unwrap_err().contains("1536"));

        let sheet = atlas_png();
        let no_manifest = build_zip(&[("spritesheet.png", &sheet)]);
        assert!(read_codex_zip(&no_manifest).unwrap_err().contains("pet.json"));

        let no_sheet = build_zip(&[("pet.json", br#"{"id":"a","displayName":"A"}"#)]);
        assert!(read_codex_zip(&no_sheet).is_err());
    }

    // 用占位符 Q 代替双引号，避免与 Rust 字符串转义互相干扰
    fn q(t: &str) -> String {
        t.replace('Q', "\"")
    }

    #[test]
    fn codex_svg_atlas_is_sliceable() {
        let svg = q("Q<svg xmlns=Qhttp://www.w3.org/2000/svgQ viewBox=Q0 0 1536 1872Q>Q<rect x=Q0Q y=Q0Q width=Q192Q height=Q208Q fill=Q#FFAA00Q/>Q</svg>Q");
        let zip = build_zip(&[
            ("pet.json", r#"{"id":"svg-pet","displayName":"SVG Pet","spritesheetPath":"spritesheet.svg"}"#.as_bytes()),
            ("spritesheet.svg", svg.as_bytes()),
        ]);
        let pack = read_codex_zip(&zip).unwrap();
        let sheet = pack["files"]["codex-row-0-idle"].as_str().unwrap();
        assert!(sheet.starts_with("data:image/svg+xml;base64,"));
        assert_eq!(pack["rows"].as_array().unwrap().len(), 9);
    }

    #[test]
    fn codex_svg_rejects_scripts_and_links() {
        let bad = q("Q<svg xmlns=Qhttp://www.w3.org/2000/svgQ viewBox=Q0 0 1536 1872Q>Q<scriptQalert(1)Q</scriptQ>Q<rect width=Q1Q height=Q1Q/>Q</svg>Q");
        assert!(codex_svg_sanitize(&bad).is_err());
        let bad_link = q("Q<svg xmlns=Qhttp://www.w3.org/2000/svgQ viewBox=Q0 0 1536 1872Q>Q<image href=Qhttps://evil/x.pngQ/>Q</svg>Q");
        assert!(codex_svg_sanitize(&bad_link).is_err());
        let ok = q("Q<svg xmlns=Qhttp://www.w3.org/2000/svgQ viewBox=Q0 0 1536 1872Q>Q<g fill=Q#123456Q>Q<rect x=Q1Q y=Q2Q width=Q3Q height=Q4Q/>Q</g>Q</svg>Q");
        let out = codex_svg_sanitize(&ok).unwrap();
        assert!(out.contains("viewBox=\"0 0 1536 1872\""));
        assert!(out.contains("<rect x=\"1\" y=\"2\" width=\"3\" height=\"4\"/>"));
    }

    const MANIFEST: &str = "{\"id\":\"round-cat-codex\",\"displayName\":\"Round Cat\",\"spritesheetPath\":\"spritesheet.png\"}";

    #[test]
    fn codex_export_builds_contract_atlas() {
        let pack = read_codex_zip(&build_zip(&[
            ("pet.json", MANIFEST.as_bytes()),
            ("spritesheet.png", &atlas_png()),
        ]))
        .unwrap();
        let (id, manifest, png) = codex_export_pack(&pack).unwrap();
        assert_eq!(id, "round-cat-codex");
        let m: Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(m["spritesheetPath"], "spritesheet.png");
        assert_eq!(m["displayName"], "Round Cat");
        let img = png_decode(&png).unwrap();
        assert_eq!((img.width, img.height), (1536, 1872));
        // 往返后像素保持一致（含未使用列的透明）
        // 导出时错误地走了八态回填（第 2 列）而非原始帧
        assert_eq!(img.pixel(0, 0), [30, 60, 120, 255]);
        assert_eq!(img.pixel(6 * CODEX_CELL_W + 10, 10)[3], 0);
    }

    #[test]
    fn codex_export_from_deskpet_states() {
        let png = tiny_png(64, 64, [9, 9, 9, 255]);
        let url = format!("data:image/png;base64,{}", base64_encode(&png));
        let files = json!({ "idle.png": url });
        let pack = json!({
            "name": "八态猫", "version": "1.0",
            "states": {"idle": "idle.png", "drag": "idle.png", "click": "idle.png", "talk": "idle.png",
                       "sleep": "idle.png", "work": "idle.png", "happy": "idle.png", "error": "idle.png"},
            "files": files, "sounds": {}
        });
        let (id, _, out) = codex_export_pack(&pack).unwrap();
        assert_eq!(id, "deskpet-skin");
        let img = png_decode(&out).unwrap();
        // 单图会被等比缩放并贴底放进格子（64×64 在 192×208 格内居中，底部对齐）
        assert_eq!(img.pixel(96, CODEX_CELL_H - 1), [9, 9, 9, 255]);
        assert_eq!(img.pixel(96, CODEX_CELL_H - 64), [9, 9, 9, 255]);
        // 等比缩放：64×64 图缩到 192×192 贴底，不拉伸、不裁切
        assert_eq!(img.pixel(0, CODEX_CELL_H - 192 + 96), [9, 9, 9, 255]);
        assert_eq!(img.pixel(96, CODEX_CELL_H - 192 - 1)[3], 0);
        // DeskPet 来源每行只有一帧（单状态一图），契约允许短行；多帧需在维护工具里补
        let cell_mid = |row: u32, col: u32| (col * CODEX_CELL_W + 96, row * CODEX_CELL_H + 16);
        let (x, y) = cell_mid(0, 0);
        assert_eq!(img.pixel(x, y)[3], 255);
        let (x, y) = cell_mid(0, 1);
        assert_eq!(img.pixel(x, y)[3], 0);

    }
}

        #[test]
        fn skin_pack_parses_manifest_and_states() {
            let png: &[u8] = b"PNGDATA";
            let zip = build_zip(&[
                ("skin.json", r#"{"name":"测试猫","version":"1.2","states":{"idle":"idle.png","happy":"../happy.png"},"sound":{"reminder":"ding.ogg"}}"#.as_bytes()),
                ("idle.png", png),
                ("happy.png", png),
                ("ding.ogg", b"OGGDATA"),
            ]);
            let pack = read_skin_zip(&zip).unwrap();
            assert_eq!(pack["name"], "测试猫");
            assert_eq!(pack["version"], "1.2");
            assert_eq!(pack["states"]["idle"], "idle.png");
            // 条目里写 ../ 也只取 basename，无法逃出包外
            assert_eq!(pack["states"]["happy"], "happy.png");
            // 未在 states 中声明的状态回退到 idle
            assert_eq!(pack["states"]["talk"], "idle.png");
            assert!(pack["files"]["idle.png"].as_str().unwrap().starts_with("data:image/png;base64,"));
            assert!(pack["sounds"]["ding.ogg"].as_str().unwrap().starts_with("data:audio/ogg;base64,"));
            assert!(read_skin_zip(b"not a zip").is_err());
        }

        #[test]
        fn skin_pack_rejects_missing_manifest_and_bad_name() {
            let zip = build_zip(&[("idle.png", b"x")]);
            assert!(read_skin_zip(&zip).unwrap_err().contains("skin.json"));
            let zip2 = build_zip(&[
                ("skin.json", r#"{"name":"","states":{"idle":"idle.png"}}"#.as_bytes()),
                ("idle.png", b"x"),
            ]);
            assert!(read_skin_zip(&zip2).is_err());
            let zip3 = build_zip(&[("skin.json", r#"{"name":"空包"}"#.as_bytes())]);
            assert!(read_skin_zip(&zip3).unwrap_err().contains("图片"));
        }


        // 回归：旧请求不得因新一轮请求重置 stop 而「复活」，且被新请求取代后必须自认过期
        #[test]
        fn chat_abort_respects_generation() {
            let stop = AtomicBool::new(false);
            let cancelled = AtomicBool::new(false);
            let seq = std::sync::atomic::AtomicU64::new(5);
            let aborted = |seq_seen: u64| {
                stop.load(Ordering::SeqCst)
                    || cancelled.load(Ordering::SeqCst)
                    || seq.load(Ordering::SeqCst) != seq_seen
            };
            assert!(!aborted(5));
            // 用户点停止
            stop.store(true, Ordering::SeqCst);
            cancelled.store(true, Ordering::SeqCst);
            assert!(aborted(5));
            // 新一轮请求：代际 +1 并清空 stop/cancelled
            seq.fetch_add(1, Ordering::SeqCst);
            stop.store(false, Ordering::SeqCst);
            cancelled.store(false, Ordering::SeqCst);
            // 新请求继续跑，旧请求因为代际不一致仍然退出（关键：不会复活并抢占前端状态）
            assert!(!aborted(6));
            assert!(aborted(5));
        }

        #[test]
        fn base64_roundtrip() {
            for s in [&b""[..], b"a", b"ab", b"abc", b"hello world!", &[0u8, 255, 128, 7]] {
                let enc = base64_encode(s);
                assert_eq!(base64_decode(&enc).unwrap(), s.to_vec(), "{}", enc);
            }
            assert!(base64_decode("a$b").is_err());
            assert_eq!(base64_decode("aGVs\nbG8=").unwrap(), b"hello".to_vec());
        }

        // 产品相关性收敛（SRS v1.2 / 审计 D1~D5）：默认状态不得再带开发者面板字段
        #[test]
        fn default_state_has_no_developer_panel_fields() {
            let v = default_state();
            for k in ["priceIn", "priceOut"] {
                assert!(v["settings"].get(k).is_none(), "默认状态不应包含 {}", k);
            }
            // 自动更新默认关闭（减少隐藏出站），手动检查仍可用
            assert_eq!(v["settings"]["updateAuto"], false);
            // 更新仓库名大小写必须与 GitHub 实际仓库一致，否则 404
            let url = v["settings"]["updateUrl"].as_str().unwrap();
            assert!(url.contains("/repos/mrtransition/DeskPet/"), "{}", url);
            // 实验性工具默认拒绝出站
            assert_eq!(v["settings"]["toolSettings"]["auth"]["get_weather"], "deny");
            assert_eq!(v["settings"]["toolSettings"]["auth"]["web_search"], "deny");
            // 健康提醒与剪贴板感知默认关闭
            assert_eq!(v["settings"]["health"]["enabled"], false);
            assert_eq!(v["settings"]["clipboardWatch"], false);
        }

        // 费用估算与出站审计 UI 已移出用户界面：对应命令不得再注册（否则等于保留隐藏入口）
        #[test]
        fn removed_commands_are_not_registered() {
            let src = include_str!("main.rs");
            for cmd in ["usage_snapshot", "usage_reset", "outbound_log", "outbound_clear"] {
                assert!(
                    !src.contains(&format!("fn {}(", cmd)),
                    "{} 应已移除", cmd
                );
            }
        }

        #[test]
        fn version_compare_for_update() {
            assert!(version_tuple("v2.0.0") > version_tuple("1.9.9"));
            assert!(version_tuple("2.0.0-beta") > version_tuple("1.9.9"));
            assert_eq!(version_tuple("0.2.8"), (0, 2, 8));
            assert_eq!(version_tuple("bad"), (0, 0, 0));
            assert!(version_tuple("2.0") > version_tuple("1.99.99"));
        }

        // FR-TOOL-001/004：工具调用消息转换（OpenAI / Anthropic / Gemini）
        #[test]
        fn tool_call_payloads_per_protocol() {
            let calls = vec![ToolCall { id: "c1".into(), name: "read_clipboard".into(), args: r#"{"max_chars":10}"#.into() }];
            let o = tool_calls_payload("openai", &calls);
            assert_eq!(o[0]["function"]["name"], "read_clipboard");
            assert_eq!(o[0]["id"], "c1");
            let a = tool_calls_payload("anthropic", &calls);
            assert_eq!(a[0]["type"], "tool_use");
            assert_eq!(a[0]["input"]["max_chars"], 10);
            let g = tool_calls_payload("gemini", &calls);
            assert_eq!(g[0]["functionCall"]["name"], "read_clipboard");

            let tool_msg = ChatMsg {
                role: "tool".into(),
                content: r#"{"ok":true}"#.into(),
                tool_calls: None,
                tool_call_id: Some("c1".into()),
                name: Some("read_clipboard".into()),
            };
            let ab = anthropic_block(&tool_msg);
            assert_eq!(ab[0]["type"], "tool_result");
            assert_eq!(ab[0]["tool_use_id"], "c1");
            let gp = gemini_parts(&tool_msg);
            assert_eq!(gp[0]["functionResponse"]["name"], "read_clipboard");
            // OpenAI 协议下 tool 消息必须带 tool_call_id
            let om = openai_messages(std::slice::from_ref(&tool_msg));
            assert_eq!(om[0]["role"], "tool");
            assert_eq!(om[0]["tool_call_id"], "c1");
        }


        // 端到端：真实生成的皮肤包样例（assets/skin-packs）必须能被解析
        #[test]
        fn repo_sample_skin_pack_parses() {
            let path = std::path::Path::new("../assets/skin-packs/round-cat.deskpet-skin.zip");
            if !path.exists() {
                return; // 样例未随仓库分发时跳过
            }
            let raw = std::fs::read(path).unwrap();
            let pack = read_skin_zip(&raw).unwrap();
            assert_eq!(pack["name"], "圆头小猫");
            for st in SKIN_STATES {
                assert!(
                    pack["states"][st].as_str().map(|f| !f.is_empty()).unwrap_or(false),
                    "状态 {} 缺失",
                    st
                );
            }
            assert_eq!(pack["files"].as_object().unwrap().len(), 8);
        }

        // 契约允许 spritesheet 为 WebP（社区宠物包默认如此）：整幅按单帧播放，仍需通过尺寸校验
        #[test]
        fn codex_webp_pet_summary_is_playable() {
            let path = std::path::Path::new("../assets/skin-packs/codex-round-cat-pack.zip");
            if !path.exists() {
                return;
            }
            let pack = read_codex_zip(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(pack["spritesheetPath"], "spritesheet.webp");
            let rows = pack["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 1);
            assert!(pack["files"]["spritesheet.webp"].as_str().unwrap().starts_with("data:image/webp;base64,"));
            assert_eq!(pack["states"]["idle"], "idle");
            // 八态齐全（缺行的状态仍由前端回退到 idle 行）
            for st in SKIN_STATES {
                assert!(pack["states"][st].as_str().is_some(), "状态 {} 未映射", st);
            }
        }

        // 端到端：仓库内的 Codex 契约样例（assets/skin-packs/codex-round-cat）必须可导入并导出
        #[test]
        fn repo_sample_codex_pet_parses_and_exports() {
            let path = std::path::Path::new("../assets/skin-packs/codex-round-cat-pack.zip");
            if !path.exists() {
                return;
            }
            let pack = read_codex_zip(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(pack["name"], "Codex 圆头小猫");
            assert_eq!(pack["id"], "codex-round-cat");
            let rows = pack["rows"].as_array().unwrap();
            if pack["spritesheetPath"] == "spritesheet.webp" {
                // 仓库样例已换成 WebP（与社区 Codex 宠物一致）：整幅按单帧播放
                assert_eq!(rows.len(), 1);
                assert!(pack["files"]["spritesheet.webp"].as_str().unwrap().starts_with("data:image/webp;base64,"));
                assert_eq!(pack["states"]["work"], "idle");
                return;
            }
            assert_eq!(rows.len(), 9);
            let frames: u32 = rows.iter().map(|r| r["frames"].as_u64().unwrap() as u32).sum();
            assert_eq!(frames, 6 + 8 + 8 + 4 + 5 + 8 + 6 + 6 + 6);
            assert_eq!(pack["states"]["work"], "running");
            let (_, manifest, png) = codex_export_pack(&pack).unwrap();
            let m: Value = serde_json::from_str(&manifest).unwrap();
            assert_eq!(m["id"], "codex-round-cat");
            let img = png_decode(&png).unwrap();
            assert_eq!((img.width, img.height), (1536, 1872));
            // 第 0 行第 0 帧的图案是「身体圆」，中心位于格内 y≈168 附近
            assert_eq!(img.pixel(96, 168)[3], 255);
        }

        // 皮肤维护工具：维护后的皮肤包导出后必须仍能被导入器解析
        #[test]
        fn skin_pack_export_roundtrip() {
            let path = std::path::Path::new("../assets/skin-packs/round-cat.deskpet-skin.zip");
            if !path.exists() {
                return;
            }
            let raw = std::fs::read(path).unwrap();
            let pack = read_skin_zip(&raw).unwrap();
            let zipped = skin_zip_bytes(&pack).unwrap();
            let again = read_skin_zip(&zipped).unwrap();
            assert_eq!(again["name"], "圆头小猫");
            for st in SKIN_STATES {
                assert!(again["states"][st].as_str().is_some(), "状态 {} 丢失", st);
            }
            assert_eq!(again["files"].as_object().unwrap().len(), 8);
        }

        #[test]
        fn skin_zip_rejects_empty_states() {
            let pack = json!({
                "name": "空包",
                "files": {"a.png": "data:image/png;base64,AA=="},
                "states": {"idle": "missing.png"}
            });
            assert!(skin_zip_bytes(&pack).unwrap_err().contains("状态"));
        }

        #[test]
        fn tool_acc_merges_openai_stream_deltas() {
            let mut acc = ToolAcc::default();
            acc.push_openai(&json!([{"index":0,"id":"call_1","function":{"name":"get_","arguments":"{\"ci"}}]));
            acc.push_openai(&json!([{"index":0,"function":{"name":"weather","arguments":"ty\":\"杭州\"}"}}]));
            let calls = acc.into_calls();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].name, "get_weather");
            assert_eq!(tool_call_args(&calls[0])["city"], "杭州");
        }
}