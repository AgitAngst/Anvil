//! Один Anvil на один `anvil.toml`: крестик прячет окно в трей, и повторный запуск (ярлык, «Пуск»)
//! не должен поднимать второй Anvil со вторым значком и занятым сочетанием — он показывает первый и
//! выходит. После обновления новая версия ждёт, пока прежняя закроется.
//!
//! Имена объектов — по пути к `anvil.toml`: переносной Anvil и установленный друг другу не мешают.

use std::path::Path;
#[cfg(windows)]
use std::sync::mpsc::Sender;

/// Имя для объектов Windows: одно на файл настроек.
#[cfg_attr(not(windows), allow(dead_code))]
fn name(config: &Path, what: &str) -> String {
    // FNV-1a по пути в нижнем регистре: коротко и одинаково в каждом запуске.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in config.to_string_lossy().to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("Local\\AgitAngst.Anvil.{hash:016x}.{what}")
}

/// Чем кончилась попытка занять место.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// Anvil с этими настройками не работал.
    Fresh,
    /// Прежний закрылся, пока ждали, — перезапуск (после обновления).
    #[cfg_attr(not(windows), allow(dead_code))]
    Restarted,
    /// Anvil с этими настройками уже работает: ему сказано показаться, этому — выйти.
    #[cfg_attr(not(windows), allow(dead_code))]
    Taken,
}

/// Занять место. Если прежний закрывается (перезапуск после обновления), подождать его до 5 с.
#[cfg(windows)]
pub fn claim(config: &Path) -> Claim {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, WAIT_ABANDONED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CreateMutexW, EVENT_MODIFY_STATE, OpenEventW, SetEvent, WaitForSingleObject,
    };
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let mutex_name = wide(&name(config, "instance"));
    // SAFETY: строки оканчиваются нулём; дескриптор мьютекса нарочно живёт до конца процесса —
    // Windows освободит его при выходе.
    unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 1, mutex_name.as_ptr());
        if mutex.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
            return Claim::Fresh;
        }
        let event_name = wide(&name(config, "show"));
        let event = OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr());
        if !event.is_null() {
            SetEvent(event);
            windows_sys::Win32::Foundation::CloseHandle(event);
        }
        let result = WaitForSingleObject(mutex, 5000);
        if result == WAIT_OBJECT_0 || result == WAIT_ABANDONED { Claim::Restarted } else { Claim::Taken }
    }
}

#[cfg(not(windows))]
pub fn claim(_config: &Path) -> Claim {
    Claim::Fresh
}

/// Ждать, пока второй запуск попросит показаться; тогда — `Show` в канал трея и разбудить окно.
#[cfg(windows)]
pub fn listen(config: &Path, tx: Sender<crate::tray::Request>, wake: impl Fn() + Send + 'static) {
    use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
    let wide: Vec<u16> = name(config, "show").encode_utf16().chain([0]).collect();
    // SAFETY: событие с автосбросом; дескриптор живёт в потоке до конца процесса.
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, wide.as_ptr()) };
    if event.is_null() {
        return;
    }
    let event = event as usize;
    let _ = std::thread::Builder::new().name("anvil-instance".into()).spawn(move || {
        loop {
            // SAFETY: дескриптор из CreateEventW выше, не закрывается.
            let result = unsafe { WaitForSingleObject(event as _, INFINITE) };
            if result != 0 || tx.send(crate::tray::Request::Show).is_err() {
                break;
            }
            wake();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_config_file() {
        let a = name(Path::new(r"C:\Users\me\AppData\Roaming\Anvil\anvil.toml"), "instance");
        let b = name(Path::new(r"c:\users\me\appdata\roaming\anvil\anvil.toml"), "instance");
        let c = name(Path::new(r"D:\dev\Anvil\target\debug\anvil.toml"), "instance");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("Local\\AgitAngst.Anvil.") && a.ends_with(".instance"));
    }
}
