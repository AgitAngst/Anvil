//! Запуски: что Anvil запустил, чем это кончилось и где вывод.
//!
//! Каждый запуск через [`start`] получает поток-сторож: он ждёт конца процесса и сообщает код
//! выхода. Вывод служб и сборок из кода идёт в файл (`run\<бинарник>-<профиль>\out.log`), а не в
//! трубу: служба переживает закрытие Anvil. Вывод установленных программ не пишется — бережём SSD
//! (решение пользователя 27.09).
//!
//! Запускать может и окно, и поток задач (запуск после сборки), поэтому канал событий общий —
//! [`init`] ставит его один раз.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::launch::Launch;

/// Чей это запуск: предмет Пульта, профиль, источник. Без метки процесс просто стартует.
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    /// Ключ предмета Пульта.
    pub key: String,
    /// Имя для людей: «Amber», «amber-server».
    pub name: String,
    /// Профиль: «обычный», «test».
    pub profile: String,
    /// Откуда: «сборка 2353af9», «установлена 0.4.0».
    pub source: String,
    /// Писать вывод в файл.
    pub log: bool,
    /// Консольная служба: без окна консоли; останавливается через Ctrl+Break.
    pub service: bool,
    /// Своя сборка из кода: «Остановить» без вопроса (§5.12).
    pub from_code: bool,
}

/// Чем кончился запуск.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum End {
    Running,
    /// Закрылся сам, код 0.
    Closed,
    /// Кончился с ненулевым кодом, и его не останавливали.
    Crashed,
    /// Остановил Anvil.
    Stopped,
    /// Anvil перезапускали, и процесса уже нет: кода не узнать.
    Lost,
}

/// Запуск в истории.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub id: u64,
    pub key: String,
    pub name: String,
    pub profile: String,
    pub source: String,
    pub pid: u32,
    /// Секунды Unix.
    pub started: i64,
    pub ended: Option<i64>,
    pub code: Option<u32>,
    pub end: End,
    pub log: Option<PathBuf>,
    pub service: bool,
    #[serde(default)]
    pub from_code: bool,
    /// Падение видели: бейдж «упал» погашен.
    #[serde(default)]
    pub seen: bool,
    /// Его останавливает Anvil: ненулевой код — не падение.
    #[serde(default)]
    pub stopping: bool,
}

impl Run {
    pub fn running(&self) -> bool {
        self.end == End::Running
    }
}

pub enum Event {
    Started(Box<Run>),
    Exited { id: u64, code: Option<u32>, at: i64 },
}

struct Hub {
    events: Mutex<Sender<Event>>,
    ctx: egui::Context,
    /// Куда писать вывод: `%LOCALAPPDATA%\Anvil\run` или `run` рядом с портативным `anvil.toml`.
    dir: PathBuf,
    /// Файлы вывода, в которые сейчас пишут: вторая копия того же профиля пишет в свой файл, а не
    /// переименовывает чужой.
    live: Mutex<HashSet<PathBuf>>,
}

static HUB: OnceLock<Hub> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);

/// Сколько держать в одном файле вывода; старое уезжает в `out.1.log`.
const LOG_LIMIT: u64 = 5 * 1024 * 1024;

/// Завести канал событий запусков. Звать один раз, при старте окна.
pub fn init(ctx: egui::Context, dir: PathBuf, next_id: u64) -> Receiver<Event> {
    let (tx, rx) = std::sync::mpsc::channel();
    NEXT.store(next_id.max(1), Ordering::Relaxed);
    let _ = HUB.set(Hub { events: Mutex::new(tx), ctx, dir, live: Mutex::new(HashSet::new()) });
    rx
}

fn send(event: Event) {
    if let Some(hub) = HUB.get() {
        if let Ok(tx) = hub.events.lock() {
            let _ = tx.send(event);
        }
        hub.ctx.request_repaint();
    }
}

/// Занять файл вывода; занят — взять `out-<id>.log` рядом. `false` в ответе — не занимали.
fn claim_log(path: PathBuf, id: u64) -> PathBuf {
    let Some(hub) = HUB.get() else { return path };
    let Ok(mut live) = hub.live.lock() else { return path };
    let path = if live.contains(&path) { path.with_file_name(format!("out-{id}.log")) } else { path };
    live.insert(path.clone());
    path
}

fn release_log(path: &Path) {
    if let Some(hub) = HUB.get()
        && let Ok(mut live) = hub.live.lock()
    {
        live.remove(path);
    }
}

/// Где лежит вывод запуска: `<dir>\<бинарник>-<профиль>\out.log`.
pub fn log_path(bin: &str, profile: &str) -> Option<PathBuf> {
    let hub = HUB.get()?;
    let clean = |s: &str| -> String {
        s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
    };
    let folder = if profile.is_empty() { clean(bin) } else { format!("{}-{}", clean(bin), clean(profile)) };
    Some(hub.dir.join(folder).join("out.log"))
}

/// Папка запусков по умолчанию: `%LOCALAPPDATA%\Anvil\run`; у портативного Anvil — рядом с его
/// `anvil.toml`, чтобы проверки не трогали настоящую.
pub fn default_dir(config_path: &Path) -> PathBuf {
    let portable = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    if portable.as_deref().is_some_and(|dir| config_path.parent() == Some(dir)) {
        return config_path.with_file_name("run");
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Anvil").join("run")
}

/// Запустить программу отдельно от Anvil. С меткой — запуск записывается в историю, а сторож
/// сообщит код выхода. Возвращает PID.
pub fn start(launch: &Launch) -> Result<u32, String> {
    let mut cmd = Command::new(&launch.exe);
    cmd.args(&launch.args).current_dir(&launch.dir).stdin(Stdio::null());
    for (k, v) in &launch.env {
        cmd.env(k, v);
    }
    let tag = launch.tag.as_ref();
    let service = tag.is_some_and(|t| t.service);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let log = tag.filter(|t| t.log).and_then(|t| log_path(launch_bin(launch), &t.profile)).map(|p| claim_log(p, id));
    let log = match log {
        Some(path) => {
            let file = open_log(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let err = file.try_clone().map_err(|e| e.to_string())?;
            cmd.stdout(Stdio::from(file)).stderr(Stdio::from(err));
            Some(path)
        }
        None if tag.is_some() => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
            None
        }
        None => None,
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // Вывод уходит в файл (служба, сборка из кода) — окна консоли не нужно, а своя консоль есть:
        // туда придёт Ctrl+Break. Оконной программе флаг не мешает. Без метки — как в 0.2:
        // консольная программа получает окно.
        let flags = match (tag.is_some(), service || log.is_some()) {
            (true, true) => CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP,
            (true, false) => CREATE_NEW_PROCESS_GROUP,
            (false, _) => CREATE_NEW_CONSOLE | CREATE_NEW_PROCESS_GROUP,
        };
        cmd.creation_flags(flags);
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            if let Some(path) = &log {
                release_log(path);
            }
            return Err(e.to_string());
        }
    };
    let pid = child.id();
    let Some(tag) = tag else { return Ok(pid) };

    let started = crate::i18n::now();
    send(Event::Started(Box::new(Run {
        id,
        key: tag.key.clone(),
        name: tag.name.clone(),
        profile: tag.profile.clone(),
        source: tag.source.clone(),
        pid,
        started,
        ended: None,
        code: None,
        end: End::Running,
        log: log.clone(),
        service,
        from_code: tag.from_code,
        seen: true,
        stopping: false,
    })));
    std::thread::Builder::new()
        .name(format!("anvil-run-{pid}"))
        .spawn(move || {
            let mut checked = std::time::Instant::now();
            let code = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status.code().map(|c| c as u32),
                    Ok(None) => {}
                    Err(_) => break None,
                }
                if let Some(path) = &log
                    && checked.elapsed() >= Duration::from_secs(10)
                {
                    checked = std::time::Instant::now();
                    trim_log(path);
                }
                std::thread::sleep(Duration::from_millis(400));
            };
            if let Some(path) = &log {
                release_log(path);
            }
            send(Event::Exited { id, code, at: crate::i18n::now() });
        })
        .map_err(|e| e.to_string())?;
    Ok(pid)
}

fn launch_bin(launch: &Launch) -> &str {
    launch.exe.file_stem().and_then(|s| s.to_str()).unwrap_or("program")
}

/// Открыть файл вывода на дозапись. Прошлый вывод уезжает в `out.1.log`.
fn open_log(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > 0) {
        let _ = std::fs::rename(path, path.with_file_name("out.1.log"));
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path)
}

/// Файл перерос предел: копия в `out.1.log`, сам файл — с нуля. Программа пишет в конец файла
/// (дозапись), поэтому продолжит с начала обнулённого.
fn trim_log(path: &Path) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LOG_LIMIT)
        && std::fs::copy(path, path.with_file_name("out.1.log")).is_ok()
        && let Ok(file) = std::fs::OpenOptions::new().write(true).open(path)
    {
        let _ = file.set_len(0);
    }
}

/// Снова следить за запуском, который остался работать с прошлого раза: PID тот же, и процесс
/// запущен тогда же (PID мог достаться другому процессу).
pub fn watch(run: &Run) {
    let (id, pid, started, log) = (run.id, run.pid, run.started, run.log.clone());
    if let Some(path) = &log
        && let Some(hub) = HUB.get()
        && let Ok(mut live) = hub.live.lock()
    {
        live.insert(path.clone());
    }
    std::thread::spawn(move || {
        // Пока ждём — следим и за размером журнала, как сторож обычного запуска.
        let code = wait_same(pid, started, || {
            if let Some(path) = &log {
                trim_log(path);
            }
        });
        if let Some(path) = &log {
            release_log(path);
        }
        send(Event::Exited { id, code, at: crate::i18n::now() });
    });
}

#[cfg(windows)]
fn wait_same(pid: u32, started: i64, mut every_10s: impl FnMut()) -> Option<u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        WaitForSingleObject,
    };
    // SAFETY: дескриптор открывается на ожидание и чтение сведений и закрывается в конце.
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        let same = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0 && {
            let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
            let secs = (ticks / 10_000_000) as i64 - 11_644_473_600;
            (secs - started).abs() <= 3
        };
        let code = if same {
            every_10s();
            while WaitForSingleObject(handle, 10_000) == WAIT_TIMEOUT {
                every_10s();
            }
            let mut code = 0u32;
            (GetExitCodeProcess(handle, &mut code) != 0).then_some(code)
        } else {
            None
        };
        CloseHandle(handle);
        code
    }
}

#[cfg(not(windows))]
fn wait_same(pid: u32, _started: i64, mut every_10s: impl FnMut()) -> Option<u32> {
    while !crate::procs::wait_exit(pid, Duration::from_secs(10)) {
        every_10s();
    }
    None
}

/// Мягко остановить: окну — «закройся» (как крестиком), консольной службе — Ctrl+Break.
/// Ctrl+Break посылает маленький помощник (`anvil --ctrl-break <pid>`): чтобы дотянуться до чужой
/// консоли, к ней надо присоединиться, а у отладочного Anvil своя консоль.
pub fn soft_stop(pid: u32, service: bool) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        if service && let Ok(me) = std::env::current_exe() {
            let _ = Command::new(me)
                .args(["--ctrl-break", &pid.to_string()])
                .creation_flags(DETACHED_PROCESS)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            return;
        }
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = service;
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
}

/// Остановить и дождаться: мягко, через `wait` — принудительно. Для сборки, которой мешает
/// запущенная программа (пользователь сам выбрал «закрыть и собрать» — второй раз не спрашиваем).
pub fn stop_blocking(pid: u32, service: bool, wait: Duration) -> Result<(), String> {
    soft_stop(pid, service);
    if crate::procs::wait_exit(pid, wait) { Ok(()) } else { force_stop(pid) }
}

/// Остановить принудительно — вместе с дочерними процессами.
pub fn force_stop(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() || !crate::procs::alive(pid) {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
        }
    }
    #[cfg(not(windows))]
    {
        Command::new("kill").args(["-9", &pid.to_string()]).status().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Помощник: послать Ctrl+Break группе процессов `pid` (служба запущена своей группой). Зовётся
/// из `main` при `--ctrl-break <pid>`; своей консоли у помощника нет.
#[cfg(windows)]
pub fn ctrl_break(pid: u32) -> bool {
    use windows_sys::Win32::System::Console::{
        AttachConsole, CTRL_BREAK_EVENT, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
    };
    // SAFETY: помощник только присоединяется к чужой консоли, глушит сигнал для себя и посылает его.
    unsafe {
        FreeConsole();
        if AttachConsole(pid) == 0 {
            return false;
        }
        SetConsoleCtrlHandler(None, 1);
        let ok = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) != 0;
        FreeConsole();
        ok
    }
}

/// Код выхода для людей: десятичный, а коды Windows — шестнадцатерично (`0xC0000005`), и что он
/// значит, если это известно.
pub fn code_text(code: u32) -> String {
    let number = if code < 0x8000_0000 { code.to_string() } else { format!("{code:#010X}").replace("0X", "0x") };
    match code_meaning(code) {
        Some(meaning) => format!("{number} ({meaning})"),
        None => number,
    }
}

fn code_meaning(code: u32) -> Option<&'static str> {
    use crate::i18n::t;
    Some(match code {
        101 => t("паника Rust"),
        0xC000_0005 => t("нарушение доступа"),
        0xC000_00FD => t("переполнение стека"),
        0xC000_0409 => t("аварийное завершение"),
        0xC000_013A => t("прерван Ctrl+C"),
        0xC000_0135 => t("нет нужной DLL"),
        _ => return None,
    })
}

/// История запусков: `runs.json` рядом с `anvil.toml`. Хранится последних 200.
pub fn load(path: &Path) -> Vec<Run> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(path: &Path, runs: &[Run]) {
    let from = runs.len().saturating_sub(200);
    if let Ok(text) = serde_json::to_string(&runs[from..]) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_in_words() {
        crate::i18n::tests::russian();
        assert_eq!(code_text(0), "0");
        assert_eq!(code_text(101), "101 (паника Rust)");
        assert_eq!(code_text(0xC000_0005), "0xC0000005 (нарушение доступа)");
        assert_eq!(code_text(0xE000_0001), "0xE0000001");
    }

    #[test]
    fn log_grows_into_a_second_file() {
        let dir = std::env::temp_dir().join(format!("anvil-runs-log-{}", std::process::id()));
        let path = dir.join("out.log");
        let mut file = open_log(&path).unwrap();
        use std::io::Write;
        writeln!(file, "first run").unwrap();
        drop(file);
        // Новый запуск: прошлый вывод уезжает в out.1.log.
        let _file = open_log(&path).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("out.1.log")).unwrap(), "first run\n");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn history_keeps_the_last_two_hundred() {
        let path = std::env::temp_dir().join(format!("anvil-runs-{}.json", std::process::id()));
        let run = |id| Run {
            id,
            key: "k".into(),
            name: "n".into(),
            profile: "p".into(),
            source: "s".into(),
            pid: 1,
            started: 0,
            ended: None,
            code: None,
            end: End::Closed,
            log: None,
            service: false,
            from_code: false,
            seen: true,
            stopping: false,
        };
        let runs: Vec<Run> = (1..=250).map(run).collect();
        save(&path, &runs);
        let back = load(&path);
        assert_eq!(back.len(), 200);
        assert_eq!(back[0].id, 51);
        std::fs::remove_file(&path).unwrap();
    }
}
