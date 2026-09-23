//! Какие программы сейчас запущены: имя exe → процессы.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Running {
    pub pid: u32,
    /// Полный путь к exe, если Windows его отдала.
    pub path: Option<PathBuf>,
}

/// Снимок процессов. Ключ — имя exe без `.exe`, в нижнем регистре.
pub type Snapshot = HashMap<String, Vec<Running>>;

pub fn key(name: &str) -> String {
    name.trim_end_matches(".exe").to_lowercase()
}

#[cfg(windows)]
pub fn snapshot() -> Snapshot {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
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
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !handle.is_null() {
                let mut buf = [0u16; 1024];
                let mut size = buf.len() as u32;
                if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size) != 0 {
                    path = Some(PathBuf::from(String::from_utf16_lossy(&buf[..size as usize])));
                }
                CloseHandle(handle);
            }
            out.entry(key(&name)).or_default().push(Running { pid, path });
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
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
