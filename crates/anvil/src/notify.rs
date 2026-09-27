//! Уведомления Windows: долгая задача кончилась или программа упала, а окно Anvil не впереди;
//! ошибка, пока Anvil в трее. Щелчки по ним возвращаются в окно, пока Anvil запущен.
//!
//! Решает поток задач, а не окно: свёрнутое окно не рисует кадров и узнало бы о конце задачи,
//! только когда его развернут. Впереди ли окно — спрашивается у Windows.
//!
//! Тост WinRT от программы без установщика: Windows показывает его, если его AppUserModelID
//! описан в `HKCU\Software\Classes\AppUserModelId\<id>` (имя и значок). Запись делается при первом
//! уведомлении и касается только этого ключа.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Задача короче — без уведомления: её конец и так видно.
pub const LONG: Duration = Duration::from_secs(15);

#[cfg(windows)]
const AUMID: &str = "AgitAngst.Anvil";

/// Куда отдать щелчок по уведомлению и как разбудить окно (оно может быть спрятано в трей).
type Clicks = (Sender<String>, Arc<dyn Fn() + Send + Sync>);

/// Уведомление: заголовок, текст; `launch` — что вернётся при щелчке по нему; кнопки — подпись и
/// что вернётся при щелчке по ней. Щелчки приходят, пока Anvil запущен.
#[cfg_attr(not(windows), allow(dead_code))]
struct Toast {
    title: String,
    body: String,
    launch: String,
    buttons: Vec<(String, String)>,
}

/// Настройка «уведомлять» (её меняет окно) и папка для значка.
pub struct Notifier {
    enabled: AtomicBool,
    crash: AtomicBool,
    cache: PathBuf,
    clicks: Mutex<Option<Clicks>>,
}

impl Notifier {
    pub fn new(enabled: bool, cache: PathBuf) -> Self {
        Self { enabled: AtomicBool::new(enabled), crash: AtomicBool::new(true), cache, clicks: Mutex::new(None) }
    }

    /// Куда отдавать щелчки по уведомлениям: строка `launch` или кнопки.
    pub fn set_clicks(&self, tx: Sender<String>, wake: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut clicks) = self.clicks.lock() {
            *clicks = Some((tx, Arc::new(wake)));
        }
    }

    /// Уведомлять ли о падениях программ, запущенных из Anvil.
    pub fn set_crash(&self, on: bool) {
        self.crash.store(on, Ordering::Relaxed);
    }

    /// Программа упала, а окно не впереди (свёрнуто, в трее, перекрыто) — уведомить. Щелчок —
    /// страница предмета, кнопки — «Журнал» и «Запустить снова» (§7.9). `true` — ушло в Windows.
    pub fn crash(&self, title: String, body: String, key: &str) -> bool {
        let show_it = self.crash.load(Ordering::Relaxed) && !in_front();
        if show_it {
            let buttons = vec![
                (crate::i18n::t("Журнал").to_owned(), format!("log:{key}")),
                (crate::i18n::t("Запустить снова").to_owned(), format!("again:{key}")),
            ];
            self.show(Toast { title, body, launch: format!("open:{key}"), buttons });
        }
        show_it
    }

    /// Ошибка, пока окна Anvil не видно (оно в трее): иначе о ней никто не узнает. Щелчок —
    /// открыть Anvil.
    pub fn alert(&self, title: String, body: String) {
        self.show(Toast { title, body, launch: "show".to_owned(), buttons: Vec::new() });
    }

    /// Разовая подсказка: крестик не закрыл Anvil, а спрятал в трей.
    pub fn hint(&self, title: String, body: String) {
        self.show(Toast { title, body, launch: "show".to_owned(), buttons: Vec::new() });
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    /// Задача кончилась: долгая, а окно не впереди — уведомить.
    pub fn job_done(&self, title: String, body: String, took: Duration) {
        if self.enabled.load(Ordering::Relaxed) && took >= LONG && !in_front() {
            self.show(Toast { title, body, launch: "show".to_owned(), buttons: Vec::new() });
        }
    }

    /// Показать в фоне.
    fn show(&self, toast: Toast) {
        let clicks = self.clicks.lock().ok().and_then(|c| c.clone());
        let cache = self.cache.clone();
        std::thread::spawn(move || {
            if let Err(e) = imp(&toast, &cache, clicks) {
                eprintln!("notification: {e}");
            }
        });
    }
}

/// Окно Anvil сейчас впереди: не свёрнуто и не перекрыто другой программой. Консоль отладочной
/// сборки числится за тем же процессом, но окном Anvil не считается.
#[cfg(windows)]
fn in_front() -> bool {
    use windows_sys::Win32::System::Console::GetConsoleWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };
    let mut pid = 0u32;
    // SAFETY: вызовы без указателей на наши данные, кроме `pid`, который живёт до конца блока.
    unsafe {
        let window = GetForegroundWindow();
        // Невидимое окно значка трея после щелчка по нему — не «Anvil впереди».
        if window.is_null() || window == GetConsoleWindow() || IsIconic(window) != 0 || IsWindowVisible(window) == 0 {
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

/// Показанные уведомления: живут, пока их могут нажать (Windows держит обработчик у объекта).
#[cfg(windows)]
static LIVE: Mutex<std::collections::VecDeque<windows::UI::Notifications::ToastNotification>> =
    Mutex::new(std::collections::VecDeque::new());

#[cfg(windows)]
fn imp(toast: &Toast, cache: &Path, clicks: Option<Clicks>) -> Result<(), String> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastActivatedEventArgs, ToastNotification, ToastNotificationManager};
    use windows::core::{HSTRING, IInspectable, Interface, Ref};

    register(cache)?;
    let buttons: String = toast
        .buttons
        .iter()
        .map(|(label, arg)| {
            format!(
                "<action content=\"{}\" arguments=\"{}\" activationType=\"foreground\"/>",
                escape(label),
                escape(arg)
            )
        })
        .collect();
    let actions = if buttons.is_empty() { String::new() } else { format!("<actions>{buttons}</actions>") };
    let xml = format!(
        "<toast launch=\"{}\" activationType=\"foreground\"><visual><binding template=\"ToastGeneric\">\
         <text>{}</text><text>{}</text></binding></visual>{actions}</toast>",
        escape(&toast.launch),
        escape(&toast.title),
        escape(&toast.body)
    );
    let err = |e: windows::core::Error| e.message();
    let doc = XmlDocument::new().map_err(err)?;
    doc.LoadXml(&HSTRING::from(xml)).map_err(err)?;
    let note = ToastNotification::CreateToastNotification(&doc).map_err(err)?;
    if let Some((tx, wake)) = clicks {
        // Обработчик зовётся в потоке COM, не в окне: строку — в канал, окно — разбудить.
        let handler = TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_: Ref<ToastNotification>, args: Ref<IInspectable>| {
                let arg = args
                    .as_ref()
                    .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
                    .and_then(|a| a.Arguments().ok())
                    .map(|h| h.to_string_lossy())
                    .unwrap_or_default();
                let _ = tx.send(arg);
                wake();
                Ok(())
            },
        );
        note.Activated(&handler).map_err(err)?;
    }
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID)).map_err(err)?;
    notifier.Show(&note).map_err(err)?;
    if let Ok(mut live) = LIVE.lock() {
        live.push_back(note);
        while live.len() > 8 {
            live.pop_front();
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn imp(_toast: &Toast, _cache: &Path, _clicks: Option<Clicks>) -> Result<(), String> {
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
