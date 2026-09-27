//! «Запускать Anvil при входе в Windows, сразу в трей»: значение `Anvil` в
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` — путь к этому exe с `--tray`.

/// Ключ командной строки: запуститься спрятанным в трей.
pub const TRAY_ARG: &str = "--tray";

#[cfg(windows)]
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const VALUE: &str = "Anvil";
/// Где Диспетчер задач и «Параметры» отмечают, что автозапуск выключен.
#[cfg(windows)]
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// Строка для `Run`: этот exe в кавычках и `--tray`.
#[cfg(windows)]
fn command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" {TRAY_ARG}", exe.display()))
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Сырые байты значения в `HKCU`; `None` — значения нет или не прочитать.
#[cfg(windows)]
fn read(key: &str, value: &str, kind: windows_sys::Win32::System::Registry::REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegGetValueW};
    let (key, value) = (wide(key), wide(value));
    let mut buf = vec![0u8; 2048];
    let mut size = buf.len() as u32;
    // SAFETY: буфер и его размер в байтах переданы вместе; строки оканчиваются нулём.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            kind,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    (status == 0).then(|| {
        buf.truncate(size as usize);
        buf
    })
}

/// Включён ли автозапуск именно этого exe (другой Anvil в `Run` — не этот).
#[cfg(windows)]
pub fn enabled() -> bool {
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_BINARY, RRF_RT_REG_SZ};
    let Some(expected) = command() else { return false };
    let Some(bytes) = read(RUN, VALUE, RRF_RT_REG_SZ) else { return false };
    let words: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    if !String::from_utf16_lossy(&words).trim_end_matches('\0').eq_ignore_ascii_case(&expected) {
        return false;
    }
    // Выключили в Диспетчере задач или в «Параметрах»: значение в `Run` остаётся, а в
    // `StartupApproved` первый байт нечётный (01, 03, 07 — выключено).
    !read(APPROVED, VALUE, RRF_RT_REG_BINARY).is_some_and(|b| b.first().is_some_and(|x| x & 1 == 1))
}

/// Включить или выключить.
#[cfg(windows)]
pub fn set(on: bool) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{REG_SZ, RegDeleteValueW, RegSetValueExW};
    let value = wide(VALUE);
    let data = if on { Some(wide(&command().ok_or("current_exe")?)) } else { None };
    with_key(RUN, |key| match &data {
        // SAFETY: ключ открыт; строки оканчиваются нулём и живут до конца вызова.
        Some(data) => unsafe {
            RegSetValueExW(key, value.as_ptr(), 0, REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32)
        },
        // Значения и так нет — выключено.
        // SAFETY: ключ открыт; строка оканчивается нулём.
        None => match unsafe { RegDeleteValueW(key, value.as_ptr()) } {
            2 => 0,
            status => status,
        },
    })?;
    // Отметку «выключено» из Диспетчера задач — прочь: иначе включённое здесь не сработает.
    // Нет ключа или значения — и не было отметки.
    // SAFETY: ключ открыт; строка оканчивается нулём.
    let cleared = with_key(APPROVED, |key| match unsafe { RegDeleteValueW(key, value.as_ptr()) } {
        2 => 0,
        status => status,
    });
    match cleared {
        Err(e) if on && e != "registry: 2" => Err(e),
        _ => Ok(()),
    }
}

/// Открыть ключ `HKCU` на запись и сделать с ним `f` (он вернёт код ошибки Windows).
#[cfg(windows)]
fn with_key(name: &str, f: impl FnOnce(windows_sys::Win32::System::Registry::HKEY) -> u32) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, RegCloseKey, RegOpenKeyExW};
    let name = wide(name);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: строка оканчивается нулём; ключ закрывается сразу после `f`.
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, name.as_ptr(), 0, KEY_SET_VALUE, &mut key) };
    if status != 0 {
        return Err(format!("registry: {status}"));
    }
    let status = f(key);
    // SAFETY: ключ открыт выше.
    unsafe { RegCloseKey(key) };
    if status == 0 { Ok(()) } else { Err(format!("registry: {status}")) }
}

#[cfg(not(windows))]
pub fn enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set(_on: bool) -> Result<(), String> {
    Err("autostart is Windows only".to_owned())
}

/// Запущен ли Anvil с `--tray` (автозапуск): окно сразу спрятано.
pub fn tray_start() -> bool {
    std::env::args().skip(1).any(|a| a == TRAY_ARG)
}
