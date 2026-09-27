//! Какие программы сейчас запущены: имя exe → процессы.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Running {
    pub pid: u32,
    /// Полный путь к exe, если Windows его отдала.
    pub path: Option<PathBuf>,
    /// Когда процесс запущен, секунды Unix — для «работает · 2 ч 14 мин».
    pub started: Option<i64>,
}

/// Снимок процессов. Ключ — имя exe без `.exe`, в нижнем регистре.
pub type Snapshot = HashMap<String, Vec<Running>>;

pub fn key(name: &str) -> String {
    name.trim_end_matches(".exe").to_lowercase()
}

#[cfg(windows)]
pub fn snapshot() -> Snapshot {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };

    let mut out = Snapshot::new();
    // SAFETY: обычный обход снимка Toolhelp; буферы и размеры заданы явно, дескрипторы закрываются.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            let pid = entry.th32ProcessID;
            let mut path = None;
            let mut started = None;
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !handle.is_null() {
                let mut buf = [0u16; 1024];
                let mut size = buf.len() as u32;
                if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size) != 0 {
                    path = Some(PathBuf::from(String::from_utf16_lossy(&buf[..size as usize])));
                }
                let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
                let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
                if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0 {
                    started = unix_from_filetime(created);
                }
                CloseHandle(handle);
            }
            out.entry(key(&name)).or_default().push(Running { pid, path, started });
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
}

/// FILETIME (сотни наносекунд с 1601 года) → секунды Unix.
#[cfg(windows)]
fn unix_from_filetime(ft: windows_sys::Win32::Foundation::FILETIME) -> Option<i64> {
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    // 11 644 473 600 с между 1601-01-01 и 1970-01-01.
    let secs = (ticks / 10_000_000) as i64 - 11_644_473_600;
    (ticks != 0 && secs > 0).then_some(secs)
}

/// Показать окно процесса поверх остальных: развернуть, если свёрнуто, и дать фокус.
/// `false` — видимого окна у процесса нет (например, программа спряталась в трей).
#[cfg(windows)]
pub fn focus_window(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE,
        SetForegroundWindow, ShowWindow,
    };

    struct Search {
        pid: u32,
        found: HWND,
    }
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
        // SAFETY: `data` — указатель на `Search` из кадра `focus_window`, живой всё время обхода.
        let search = unsafe { &mut *(data as *mut Search) };
        let mut owner_pid = 0u32;
        // SAFETY: обычные запросы к окну, которое нам дал EnumWindows.
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut owner_pid);
            if owner_pid == search.pid && IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER).is_null() {
                search.found = hwnd;
                return 0;
            }
        }
        1
    }

    let mut search = Search { pid, found: std::ptr::null_mut() };
    // SAFETY: колбэк получает указатель на `search`, который живёт до конца вызова.
    unsafe {
        EnumWindows(Some(visit), &mut search as *mut Search as LPARAM);
        if search.found.is_null() {
            return false;
        }
        if IsIconic(search.found) != 0 {
            ShowWindow(search.found, SW_RESTORE);
        }
        SetForegroundWindow(search.found) != 0
    }
}

#[cfg(not(windows))]
pub fn focus_window(_pid: u32) -> bool {
    false
}

/// Вне Windows Anvil пока не следит за процессами.
#[cfg(not(windows))]
pub fn snapshot() -> Snapshot {
    Snapshot::new()
}

/// Дождаться, пока процесс завершится. `true` — завершился (или его уже нет).
#[cfg(windows)]
pub fn wait_exit(pid: u32, timeout: std::time::Duration) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
    // SAFETY: дескриптор открывается только на ожидание и закрывается.
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return true;
        }
        let result = WaitForSingleObject(handle, timeout.as_millis().min(u32::MAX as u128) as u32);
        CloseHandle(handle);
        result != WAIT_TIMEOUT
    }
}

#[cfg(not(windows))]
pub fn wait_exit(pid: u32, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    let path = std::path::PathBuf::from(format!("/proc/{pid}"));
    while path.exists() {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    true
}
