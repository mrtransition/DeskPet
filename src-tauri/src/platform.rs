//! 平台适配层（P-10）
//!
//! 目标：把 Windows 专有调用收敛到一个模块，业务代码只依赖本文件暴露的
//! 中性接口。当前只实现 Windows；其他平台返回可预期的降级结果，不承诺
//! 短期跨平台可用（见 docs/治理/README.md 的边界说明）。
//!
//! 这里刻意不暴露 `windows_sys` 类型，避免平台依赖向上层泄漏。

#![allow(dead_code)]

/// 屏幕工作区（已扣除任务栏），坐标使用物理像素。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[cfg(windows)]
mod imp {
    use super::WorkArea;
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, SetForegroundWindow, SetWindowPos, HWND_NOTOPMOST,
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    /// 光标所在显示器的工作区；取不到时返回一个保守的 1920×1040 区域。
    pub fn work_area_at(x: i32, y: i32) -> WorkArea {
        unsafe {
            let pt = POINT { x, y };
            let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(mon, &mut info) != 0 {
                let r: RECT = info.rcWork;
                WorkArea { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
            } else {
                WorkArea { left: 0, top: 0, right: 1920, bottom: 1040 }
            }
        }
    }

    /// 读取剪贴板纯文本；非文本 / 空内容返回 None。
    pub fn clipboard_read_text() -> Option<String> {
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

    /// 只允许 http / https 的外部链接，交给系统默认浏览器。
    pub fn open_external_url(url: &str) -> Result<(), String> {
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

    /// 当前前台窗口句柄（以 usize 传递，避免平台类型泄漏）。
    pub fn foreground_window() -> usize {
        unsafe { GetForegroundWindow() as usize }
    }

    pub fn set_foreground(hwnd: usize) {
        if hwnd != 0 {
            unsafe {
                SetForegroundWindow(hwnd as _);
            }
        }
    }

    /// 左键当前是否按下。用于 startDragging 后轮询拖动结束。
    pub fn mouse_left_down() -> bool {
        unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 }
    }

    /// 全局光标位置（物理像素）。
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut pt = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut pt) } == 0 {
            None
        } else {
            Some((pt.x, pt.y))
        }
    }

    /// 置顶层级枚举「置顶且覆盖任务栏」：周期性重新提升，避免被全屏应用盖住。
    pub fn set_topmost(hwnd: isize, on: bool) {
        let insert = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
        unsafe {
            SetWindowPos(
                hwnd as _,
                insert,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    // ---- 单实例（FR-SYS-007） ----
    // 命名互斥量判定「是否已有实例」，命名事件把重复启动通知给首个实例。
    mod single_instance {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
        use windows_sys::Win32::System::Threading::{
            CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject,
            EVENT_MODIFY_STATE, INFINITE,
        };

        const MUTEX_NAME: &str = "Local\\DeskPet-SingleInstance-com.mrtransition.deskpet";
        const EVENT_NAME: &str = "Local\\DeskPet-SecondLaunch-com.mrtransition.deskpet";

        static EVENT_HANDLE: AtomicUsize = AtomicUsize::new(0);

        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }

        pub fn acquire() -> bool {
            let name = wide(MUTEX_NAME);
            let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            let already = handle.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            if already {
                notify_first();
                return false;
            }
            let ev = wide(EVENT_NAME);
            let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, ev.as_ptr()) };
            EVENT_HANDLE.store(event as usize, Ordering::SeqCst);
            true
        }

        fn notify_first() {
            let name = wide(EVENT_NAME);
            for _ in 0..40 {
                let h = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
                if !h.is_null() {
                    unsafe { SetEvent(h) };
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }

        pub fn watch_first<F: Fn() + Send + 'static>(on_second_launch: F) {
            let raw = EVENT_HANDLE.load(Ordering::SeqCst);
            if raw == 0 {
                return;
            }
            std::thread::spawn(move || loop {
                let r = unsafe { WaitForSingleObject(raw as HANDLE, INFINITE) };
                if r != windows_sys::Win32::Foundation::WAIT_OBJECT_0 {
                    break;
                }
                on_second_launch();
            });
        }
    }

    pub fn acquire_single_instance() -> bool {
        single_instance::acquire()
    }

    pub fn watch_second_launch<F: Fn() + Send + 'static>(on_second_launch: F) {
        single_instance::watch_first(on_second_launch);
    }
}

#[cfg(not(windows))]
mod imp {
    use super::WorkArea;

    pub fn work_area_at(_x: i32, _y: i32) -> WorkArea {
        WorkArea { left: 0, top: 0, right: 1920, bottom: 1040 }
    }

    pub fn clipboard_read_text() -> Option<String> {
        None
    }

    pub fn open_external_url(_url: &str) -> Result<(), String> {
        Err("当前平台尚未适配外部链接打开".into())
    }

    pub fn foreground_window() -> usize {
        0
    }

    pub fn set_foreground(_hwnd: usize) {}

    pub fn mouse_left_down() -> bool {
        false
    }

    pub fn cursor_pos() -> Option<(i32, i32)> {
        None
    }

    pub fn set_topmost(_hwnd: isize, _on: bool) {}

    pub fn acquire_single_instance() -> bool {
        true
    }

    pub fn watch_second_launch<F: Fn() + Send + 'static>(_on_second_launch: F) {}
}

pub use imp::{
    acquire_single_instance, clipboard_read_text, cursor_pos, foreground_window, mouse_left_down,
    open_external_url, set_foreground, set_topmost, watch_second_launch, work_area_at,
};
