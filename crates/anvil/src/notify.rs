//! Уведомление Windows, когда долгая задача кончилась, а окно Anvil не впереди.
//!
//! Решает поток задач, а не окно: свёрнутое окно не рисует кадров и узнало бы о конце задачи,
//! только когда его развернут. Впереди ли окно — спрашивается у Windows.
//!
//! Тост WinRT от программы без установщика: Windows показывает его, если его AppUserModelID
//! описан в `HKCU\Software\Classes\AppUserModelId\<id>` (имя и значок). Запись делается при первом
//! уведомлении и касается только этого ключа.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Задача короче — без уведомления: её конец и так видно.
pub const LONG: Duration = Duration::from_secs(15);

#[cfg(windows)]
const AUMID: &str = "AgitAngst.Anvil";

/// Настройка «уведомлять» (её меняет окно) и папка для значка.
pub struct Notifier {
    enabled: AtomicBool,
    cache: PathBuf,
}

impl Notifier {
    pub fn new(enabled: bool, cache: PathBuf) -> Self {
        Self { enabled: AtomicBool::new(enabled), cache }
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    /// Задача кончилась: долгая, а окно не впереди — уведомить.
    pub fn job_done(&self, title: String, body: String, took: Duration) {
        if self.enabled.load(Ordering::Relaxed) && took >= LONG && !in_front() {
            show(title, body, self.cache.clone());
        }
    }
}

/// Окно Anvil сейчас впереди: не свёрнуто и не перекрыто другой программой. Консоль отладочной
/// сборки числится за тем же процессом, но окном Anvil не считается.
#[cfg(windows)]
fn in_front() -> bool {
    use windows_sys::Win32::System::Console::GetConsoleWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, IsIconic};
    let mut pid = 0u32;
    // SAFETY: вызовы без указателей на наши данные, кроме `pid`, который живёт до конца блока.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() || window == GetConsoleWindow() || IsIconic(window) != 0 {
            return false;
        }
        GetWindowThreadProcessId(window, &mut pid);
    }
    pid == std::process::id()
}

#[cfg(not(windows))]
fn in_front() -> bool {
    true
}

/// Показать уведомление в фоне. `cache` — куда положить значок для Windows.
fn show(title: String, body: String, cache: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = imp(&title, &body, &cache) {
            eprintln!("notification: {e}");
        }
    });
}

#[cfg(windows)]
fn imp(title: &str, body: &str, cache: &Path) -> Result<(), String> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::core::HSTRING;

    register(cache)?;
    let xml = format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape(title),
        escape(body)
    );
    let err = |e: windows::core::Error| e.message();
    let doc = XmlDocument::new().map_err(err)?;
    doc.LoadXml(&HSTRING::from(xml)).map_err(err)?;
    let toast = ToastNotification::CreateToastNotification(&doc).map_err(err)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID)).map_err(err)?;
    notifier.Show(&toast).map_err(err)
}

#[cfg(not(windows))]
fn imp(_title: &str, _body: &str, _cache: &Path) -> Result<(), String> {
    Ok(())
}

/// Описать AUMID для Windows: имя «Anvil» и значок (PNG в папке кеша).
#[cfg(windows)]
fn register(cache: &Path) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW,
        RegSetValueExW,
    };

    let icon = cache.join("anvil.png");
    if !icon.exists() {
        std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
        std::fs::write(&icon, include_bytes!(concat!(env!("OUT_DIR"), "/anvil.png"))).map_err(|e| e.to_string())?;
    }
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let subkey = wide(&format!(r"Software\Classes\AppUserModelId\{AUMID}"));
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: строки оканчиваются нулём и живут до конца вызовов; ключ закрывается ниже.
    unsafe {
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        );
        if status != 0 {
            return Err(format!("registry: {status}"));
        }
        let icon = icon.to_string_lossy().into_owned();
        for (name, value) in [("DisplayName", "Anvil"), ("IconUri", icon.as_str())] {
            let (name, value) = (wide(name), wide(value));
            RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, value.as_ptr().cast(), (value.len() * 2) as u32);
        }
        RegCloseKey(key);
    }
    Ok(())
}

#[cfg(windows)]
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn escapes_xml() {
        assert_eq!(super::escape(r#"a < b & "c""#), "a &lt; b &amp; &quot;c&quot;");
    }
}
