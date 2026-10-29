//! Серверы на Linux-машинах: список машин в `anvil.toml` (`[[servers]]`) и работа с ними по SSH в фоновых потоках.
//!
//! Что здесь: записи машин и то, что показала последняя проверка (кэш установленных модулей); цепочки шагов —
//! проверка, установка, обновление, перезапуск, копия, удаление, действие манифеста, журнал службы, консоль; поток,
//! который их выполняет, и приём его событий. Рисует всё это `ui/servers.rs`.
//!
//! **Цепочки не знают про SSH.** Они работают с двумя небольшими трейтами — [`Remote`] (команды и заливка файла) и
//! [`Releaser`] (файл выпуска), — поэтому проверяются тестами на подставных реализациях. Настоящие — [`Ssh`] и
//! [`Github`], они только вызывают ядро `anvil-servers` (его SSH и выпуски покрыты тестами самого ядра).
//!
//! **Секретов в настройках нет.** Пароль входа и ключевая фраза лежат в хранилище учётных данных системы
//! (`anvil_servers::secrets`, запись `ssh:<id машины>`), в `anvil.toml` — только отпечаток ключа машины, которому
//! владелец поверил. Любая строка вывода идёт в «Ход работы» через [`Sink`], который прячет токены и известные секреты.
//!
//! Всё, что печатается в журнал «Ход работы», — по-английски; перевод для человека делает вид.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use anvil_servers::instance::{self, LISTEN_COMMAND, PortBusy};
use anvil_servers::manifest::{Arch, CommandKind, Source};
use anvil_servers::release::{ReleaseError, Releases, Request as ReleaseRequest};
use anvil_servers::shell::{quote, word};
use anvil_servers::ssh::{self, Auth, Conn, ConnectError, Output, Target};
use anvil_servers::{Context, Fetched, Manifest, Registry, SecretStore, SystemStore, redact, status};
use anvil_ui::Tone;
use serde::{Deserialize, Serialize};

use crate::i18n::{self, t};

/// Сколько строк «Хода работы» помним на машину.
pub const LOG_LINES: usize = 400;

/// Сколько строк журнала службы просим у `journalctl`.
pub const JOURNAL_LINES: u32 = 200;

/// Порт SSH по умолчанию.
pub const SSH_PORT: u16 = 22;

// ─── Машины в настройках ───────────────────────────────────────────────────

/// Как входим на машину. Сами пароль и ключевая фраза — в хранилище учётных данных, не здесь.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Login {
    /// Пароль из хранилища.
    #[default]
    Password,
    /// Закрытый ключ из файла (фраза, если есть, — в хранилище).
    Key(PathBuf),
    /// Ключи ssh-agent.
    Agent,
}

/// Установленный на машине модуль так, как его показала последняя проверка.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Seen {
    /// `id` модуля из манифеста.
    pub module: String,
    /// Имя экземпляра; нет — основной.
    pub instance: Option<String>,
    /// `Some(false)` — служба остановлена.
    pub running: Option<bool>,
    pub version: Option<String>,
    pub address: Option<String>,
    pub fingerprint: Option<String>,
    /// Прочие поля `status --json` (секреты ядро уже заменило многоточием).
    pub extra: BTreeMap<String, String>,
    /// Порты, под которые ставили: по ним проверяют занятость у следующего экземпляра.
    pub ports: Vec<u16>,
    /// Когда проверяли (секунды UNIX).
    pub checked: i64,
}

impl Seen {
    /// Из ответа `status --json`.
    pub fn from_status(module: &str, instance: Option<&str>, status: status::Status, checked: i64) -> Seen {
        Seen {
            module: module.to_owned(),
            instance: instance.map(str::to_owned),
            running: status.running,
            version: status.version,
            address: status.address,
            fingerprint: status.fingerprint,
            extra: status.extra.into_iter().collect(),
            ports: Vec::new(),
            checked,
        }
    }

    /// Подпись экземпляра: основной — пусто.
    pub fn instance_name(&self) -> &str {
        self.instance.as_deref().unwrap_or("")
    }
}

/// Запись машины (`[[servers]]` в `anvil.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Machine {
    /// Постоянный номер: по нему живёт запись в хранилище секретов.
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub login: Login,
    /// Отпечаток ключа машины, которому владелец поверил; пусто — ещё нет (первое подключение).
    pub fingerprint: String,
    /// Архитектура по `uname -m` при последней проверке.
    pub arch: String,
    /// Модули, найденные последней проверкой.
    pub modules: Vec<Seen>,
    /// Когда проверяли (секунды UNIX); 0 — ни разу.
    pub checked: i64,
}

impl Default for Machine {
    fn default() -> Self {
        Machine {
            id: String::new(),
            name: String::new(),
            host: String::new(),
            port: SSH_PORT,
            user: "root".to_owned(),
            login: Login::default(),
            fingerprint: String::new(),
            arch: String::new(),
            modules: Vec::new(),
            checked: 0,
        }
    }
}

impl Machine {
    /// Адрес для показа: `root@203.0.113.7` или с портом, если он не 22.
    pub fn address(&self) -> String {
        if self.port == SSH_PORT { self.host.clone() } else { format!("{}:{}", self.host, self.port) }
    }

    /// Адрес с логином: `root@203.0.113.7:2222`.
    pub fn target(&self) -> String {
        format!("{}@{}", self.user, self.address())
    }

    /// Записать или заменить модуль. Порты прежней записи не теряются, если новая их не знает.
    pub fn upsert(&mut self, mut seen: Seen) {
        match self.modules.iter_mut().find(|s| s.module == seen.module && s.instance == seen.instance) {
            Some(slot) => {
                if seen.ports.is_empty() {
                    seen.ports = std::mem::take(&mut slot.ports);
                }
                *slot = seen;
            }
            None => self.modules.push(seen),
        }
    }

    pub fn remove(&mut self, module: &str, instance: Option<&str>) {
        self.modules.retain(|s| !(s.module == module && s.instance.as_deref() == instance));
    }

    /// Порты других экземпляров модуля: имя экземпляра (основной — пусто) → порты.
    pub fn other_ports(&self, module: &str, instance: Option<&str>) -> BTreeMap<String, Vec<u16>> {
        self.modules
            .iter()
            .filter(|s| s.module == module && s.instance.as_deref() != instance)
            .map(|s| (s.instance_name().to_owned(), s.ports.clone()))
            .collect()
    }

    /// Состояние для точки в списке.
    pub fn tone(&self, hidden: impl Fn(&Seen) -> bool, failed: bool, busy: bool) -> Tone {
        if busy {
            return Tone::Accent;
        }
        if failed {
            return Tone::Danger;
        }
        let shown: Vec<&Seen> = self.modules.iter().filter(|s| !hidden(s)).collect();
        if shown.is_empty() {
            Tone::Neutral
        } else if shown.iter().all(|s| s.running == Some(true)) {
            Tone::Success
        } else if shown.iter().any(|s| s.running == Some(false)) {
            Tone::Warning
        } else {
            Tone::Neutral
        }
    }
}

/// Новый постоянный номер машины.
pub fn new_id() -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    format!("m{nanos:x}{:x}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// Папка манифестов: `modules/` в папке настроек Anvil.
pub fn modules_dir(config_path: &Path) -> PathBuf {
    config_path.parent().unwrap_or(Path::new(".")).join(anvil_servers::registry::MODULES_DIR)
}

// ─── Подставные границы: машина и выпуск ───────────────────────────────────

/// Машина, с которой разговариваем. Ошибки — английский текст для журнала.
pub trait Remote {
    /// Архитектура по `uname -m`; `None` — выпусков под неё не бывает.
    fn arch(&mut self) -> Result<Option<Arch>, String>;
    /// Команда от имени входящего (для временного каталога, куда он заливает файл).
    fn user(&mut self, command: &str, line: &mut dyn FnMut(&str)) -> Result<Output, String>;
    /// Команда от root (через `sudo`, если вошли не root).
    fn root(&mut self, command: &str, line: &mut dyn FnMut(&str)) -> Result<Output, String>;
    /// Залить файл по пути от имени входящего.
    fn upload(&mut self, bytes: &[u8], path: &str, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String>;
}

/// Откуда берётся файл сервера.
pub trait Releaser {
    fn fetch(
        &mut self,
        source: &Source,
        arch: Arch,
        allow_unverified: bool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Fetched, ReleaseError>;
}

/// Настоящая машина: соединение ядра и русло для его асинхронных вызовов.
pub struct Ssh<'a> {
    rt: &'a tokio::runtime::Runtime,
    conn: Conn,
}

impl Remote for Ssh<'_> {
    fn arch(&mut self) -> Result<Option<Arch>, String> {
        self.rt.block_on(self.conn.arch()).map_err(|e| format!("{e:#}"))
    }

    fn user(&mut self, command: &str, line: &mut dyn FnMut(&str)) -> Result<Output, String> {
        self.rt.block_on(self.conn.run(command, None, |l| line(l))).map_err(|e| format!("{e:#}"))
    }

    fn root(&mut self, command: &str, line: &mut dyn FnMut(&str)) -> Result<Output, String> {
        self.rt.block_on(self.conn.root(command, |l| line(l))).map_err(|e| format!("{e:#}"))
    }

    fn upload(&mut self, bytes: &[u8], path: &str, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
        self.rt.block_on(self.conn.upload(bytes, path, progress)).map_err(|e| format!("{e:#}"))
    }
}

impl Ssh<'_> {
    fn close(self) {
        self.rt.block_on(self.conn.close());
    }
}

/// Выпуски GitHub. Токен ищется при первой загрузке, а не при каждой задаче.
pub struct Github<'a> {
    rt: &'a tokio::runtime::Runtime,
    token: Option<String>,
    looked: bool,
}

impl Github<'_> {
    /// Токен GitHub: из хранилища серверов, иначе тот, что у Anvil (его собственный или у git).
    fn find(secrets: &dyn SecretStore) -> Option<String> {
        secrets.github().ok().flatten().or_else(|| crate::github::find_token().0)
    }
}

impl Releaser for Github<'_> {
    fn fetch(
        &mut self,
        source: &Source,
        arch: Arch,
        allow_unverified: bool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Fetched, ReleaseError> {
        if !self.looked {
            self.looked = true;
            self.token = Github::find(&SystemStore::default());
        }
        let releases = Releases::new(self.token.as_deref())?;
        let request = ReleaseRequest { source, arch, tag: None, allow_unverified };
        self.rt.block_on(releases.fetch(request, progress))
    }
}

// ─── Журнал и ошибки ───────────────────────────────────────────────────────

/// Что цепочка сообщает наружу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Emit {
    /// Строка «Хода работы».
    Log(String),
    /// Строка журнала службы (вкладка «Журнал»).
    Journal(String),
    /// Строка вывода консоли.
    Console(String),
    /// Сколько байт скачано или залито из скольких.
    Progress(u64, u64),
}

/// Приёмник: прячет токены и известные секреты в каждой строке до того, как она попадёт в журнал.
pub struct Sink<'a> {
    out: &'a mut dyn FnMut(Emit),
    secrets: Vec<String>,
}

impl<'a> Sink<'a> {
    pub fn new(out: &'a mut dyn FnMut(Emit)) -> Sink<'a> {
        Sink { out, secrets: Vec::new() }
    }

    /// Запомнить секрет: его точное значение в любом выводе заменяется многоточием.
    pub fn hide(&mut self, secret: &str) {
        if !secret.is_empty() && !self.secrets.iter().any(|s| s == secret) {
            self.secrets.push(secret.to_owned());
        }
    }

    fn clean(&self, line: &str) -> String {
        let known: Vec<&str> = self.secrets.iter().map(String::as_str).collect();
        redact::mask_known(&redact::mask(line), &known)
    }

    /// Строка «Хода работы».
    pub fn log(&mut self, line: impl AsRef<str>) {
        let line = self.clean(line.as_ref());
        (self.out)(Emit::Log(line));
    }

    fn journal(&mut self, line: &str) {
        let line = self.clean(line);
        (self.out)(Emit::Journal(line));
    }

    fn console(&mut self, line: &str) {
        let line = self.clean(line);
        (self.out)(Emit::Console(line));
    }

    fn progress(&mut self, got: u64, total: u64) {
        (self.out)(Emit::Progress(got, total));
    }
}

/// Почему цепочка остановилась.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// В выпуске нет контрольной суммы: владелец может согласиться и повторить.
    Unverified(String),
    /// Файл не сошёлся с суммой: ничего не залито, повторять с согласием нельзя.
    Mismatch(String),
    /// Порт занят: ничего не поставлено.
    Port(PortBusy),
    /// Форма не годится.
    Form(String),
    /// Выпусков под архитектуру машины нет.
    Arch(String),
    /// Прочее, текст по-английски.
    Other(String),
}

impl fmt::Display for Fail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fail::Unverified(reason) => write!(f, "the release cannot be verified: {reason}"),
            Fail::Mismatch(text) | Fail::Form(text) | Fail::Other(text) => f.write_str(text),
            Fail::Port(busy) => write!(f, "{busy}"),
            Fail::Arch(arch) => write!(f, "no release file for the machine architecture `{arch}`"),
        }
    }
}

impl From<ReleaseError> for Fail {
    fn from(e: ReleaseError) -> Fail {
        match e {
            ReleaseError::Unverified { reason, .. } => Fail::Unverified(reason),
            e @ ReleaseError::ChecksumMismatch { .. } => Fail::Mismatch(e.to_string()),
            e => Fail::Other(e.to_string()),
        }
    }
}

fn other(e: String) -> Fail {
    Fail::Other(e)
}

// ─── Цепочки шагов ─────────────────────────────────────────────────────────

/// Что поставить.
pub struct Deploy<'a> {
    pub manifest: &'a Manifest,
    pub instance: Option<&'a str>,
    pub form: &'a BTreeMap<String, String>,
    /// Обновление: порты свои, их не сверяем.
    pub update: bool,
    /// Владелец согласился на файл без контрольной суммы.
    pub allow_unverified: bool,
    /// Порты других экземпляров этого модуля на машине.
    pub others: &'a BTreeMap<String, Vec<u16>>,
}

/// Допустимый временный каталог: абсолютный путь из безопасных символов, не корень.
fn is_temp_dir(path: &str) -> bool {
    path.starts_with('/')
        && path.matches('/').count() >= 2
        && !path.contains("..")
        && path.chars().all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
}

/// Установка и обновление: архитектура → порты → файл выпуска → временный каталог → заливка → команда манифеста →
/// уборка каталога → состояние. Любая остановка до заливки ничего не оставляет на машине; после неё каталог убирается
/// в любом случае.
pub fn deploy(r: &mut dyn Remote, rel: &mut dyn Releaser, sink: &mut Sink, d: &Deploy) -> Result<Seen, Fail> {
    let m = d.manifest;
    let verb = if d.update { "Updating" } else { "Installing" };
    let naming = m.naming(d.instance).map_err(|e| Fail::Form(e.to_string()))?;
    // Форма проверяется до того, как мы тронули машину.
    m.form_values(d.form).map_err(|e| Fail::Form(e.to_string()))?;
    let ports = m.form_ports(d.form).map_err(|e| Fail::Form(e.to_string()))?;
    for field in m.form.iter().filter(|f| f.kind == anvil_servers::FieldKind::Password) {
        if let Some(value) = d.form.get(&field.key) {
            sink.hide(value);
        }
    }

    sink.log(format!("{verb} {} (service {})", m.name, naming.service));
    let arch = match r.arch().map_err(other)? {
        Some(arch) => arch,
        None => return Err(Fail::Arch("unknown".to_owned())),
    };
    sink.log(format!("Machine architecture: {}", arch.key()));

    if !d.update && !ports.is_empty() {
        sink.log(format!("Checking ports: {}", join_ports(&ports)));
        let out = r.root(LISTEN_COMMAND, &mut |_| {}).map_err(other)?;
        if !out.ok() {
            return Err(Fail::Other(format!("cannot read the listening ports: {}", out.failure())));
        }
        instance::check_ports(&ports, &instance::listening_ports(&out.stdout), d.others).map_err(Fail::Port)?;
    }

    sink.log(format!("Downloading the release of {}", m.source.repo));
    let fetched =
        rel.fetch(&m.source, arch, d.allow_unverified, &mut |got: u64, total: u64| sink.progress(got, total))?;
    sink.log(format!(
        "Release {}: {} ({} bytes, {})",
        fetched.tag,
        fetched.file,
        fetched.bytes.len(),
        if fetched.verified { "checksum verified" } else { "checksum NOT verified, confirmed by the owner" }
    ));
    if fetched.file.is_empty() || fetched.file.contains('/') || fetched.file.starts_with('-') {
        return Err(Fail::Other(format!("unsafe release file name `{}`", fetched.file)));
    }

    let made = r.user("mktemp -d", &mut |_| {}).map_err(other)?;
    let tmp = made.stdout.trim().to_owned();
    if !made.ok() || !is_temp_dir(&tmp) {
        return Err(Fail::Other(format!("cannot make a temporary directory: {}", made.failure())));
    }
    sink.log(format!("Temporary directory: {tmp}"));

    let result = stage(r, sink, d, &fetched, &tmp);
    // Каталог убирается всегда: и после успеха, и после любой ошибки.
    match r.root(&format!("rm -rf -- {}", word(&tmp)), &mut |_| {}) {
        Ok(out) if out.ok() => sink.log("Temporary directory removed"),
        Ok(out) => sink.log(format!("Could not remove {tmp}: {}", out.failure())),
        Err(e) => sink.log(format!("Could not remove {tmp}: {e}")),
    }
    result?;

    let mut seen = match read_status(r, sink, m, d.instance)? {
        Some(seen) => seen,
        None => Seen {
            module: m.id.clone(),
            instance: d.instance.map(str::to_owned),
            checked: i18n::now(),
            ..Seen::default()
        },
    };
    seen.ports = ports;
    sink.log(format!("{} done", if d.update { "Update" } else { "Install" }));
    Ok(seen)
}

/// Шаги над временным каталогом: заливка, права, команда `install` (или `update`).
fn stage(r: &mut dyn Remote, sink: &mut Sink, d: &Deploy, fetched: &Fetched, tmp: &str) -> Result<(), Fail> {
    let path = format!("{tmp}/{}", fetched.file);
    sink.log(format!("Uploading {} ({} bytes)", fetched.file, fetched.bytes.len()));
    r.upload(&fetched.bytes, &path, &mut |got: u64, total: u64| sink.progress(got, total)).map_err(other)?;
    let chmod = r.user(&format!("chmod 755 {}", word(&path)), &mut |_| {}).map_err(other)?;
    if !chmod.ok() {
        return Err(Fail::Other(format!("cannot make the file executable: {}", chmod.failure())));
    }
    // Бинарник выпуска ставит сам себя на место, поэтому `{bin}` в команде — загруженный файл.
    let mut staged = d.manifest.clone();
    staged.binary = Some(path);
    let kind = if d.update && staged.commands.update.is_some() { CommandKind::Update } else { CommandKind::Install };
    let ctx = Context { instance: d.instance, dir: Some(tmp) };
    let command = staged.command(kind, d.form, &ctx).map_err(|e| Fail::Form(e.to_string()))?;
    let out = run_logged(r, sink, &command, Stream::Log)?;
    if out.ok() { Ok(()) } else { Err(Fail::Other(out.failure())) }
}

fn join_ports(ports: &[u16]) -> String {
    ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ")
}

/// Куда идут строки вывода команды.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stream {
    Log,
    Journal,
    Console,
}

/// Выполнить команду от root, показав её дословно: `$ команда`, строки вывода и код выхода.
fn run_logged(r: &mut dyn Remote, sink: &mut Sink, command: &str, to: Stream) -> Result<Output, Fail> {
    sink.log(format!("$ {command}"));
    let out = r
        .root(command, &mut |line: &str| match to {
            Stream::Log => sink.log(line),
            Stream::Journal => sink.journal(line),
            Stream::Console => {
                sink.console(line);
                sink.log(line);
            }
        })
        .map_err(other)?;
    sink.log(format!("exit code {}", out.code));
    Ok(out)
}

/// Состояние модуля по его `status --json`. `None` — модуль не отвечает: не установлен или не запускается.
pub fn read_status(
    r: &mut dyn Remote,
    sink: &mut Sink,
    m: &Manifest,
    instance: Option<&str>,
) -> Result<Option<Seen>, Fail> {
    if m.commands.status.is_none() {
        return Ok(None);
    }
    let ctx = Context { instance, dir: None };
    let command = m.command(CommandKind::Status, &BTreeMap::new(), &ctx).map_err(|e| Fail::Form(e.to_string()))?;
    let out = r.root(&command, &mut |_| {}).map_err(other)?;
    let label = instance.map_or(m.name.clone(), |i| format!("{} / {i}", m.name));
    if !out.ok() {
        sink.log(format!("{label}: no answer to status (exit code {})", out.code));
        return Ok(None);
    }
    match status::parse(&out.stdout) {
        Ok(parsed) => {
            let state = match parsed.running {
                Some(true) => "running",
                Some(false) => "stopped",
                None => "state unknown",
            };
            sink.log(format!(
                "{label}: {state}{}",
                parsed.version.as_ref().map_or(String::new(), |v| format!(", {v}"))
            ));
            Ok(Some(Seen::from_status(&m.id, instance, parsed, i18n::now())))
        }
        Err(e) => {
            sink.log(format!("{label}: status is not understood ({e})"));
            Ok(Some(Seen {
                module: m.id.clone(),
                instance: instance.map(str::to_owned),
                checked: i18n::now(),
                ..Seen::default()
            }))
        }
    }
}

/// Проверка машины: архитектура и состояние каждого модуля из `manifests` (основной и известные экземпляры).
/// Возвращает найденное: модулей, которых не видно, в ответе нет.
pub fn check(
    r: &mut dyn Remote,
    sink: &mut Sink,
    manifests: &[Manifest],
    known: &[Seen],
) -> Result<(Option<Arch>, Vec<Seen>), Fail> {
    sink.log("Checking the machine");
    let arch = r.arch().map_err(other)?;
    sink.log(format!("Machine architecture: {}", arch.map_or("unknown", Arch::key)));
    let mut found = Vec::new();
    for m in manifests {
        let mut instances: Vec<Option<&str>> = vec![None];
        if m.instances {
            instances.extend(known.iter().filter(|s| s.module == m.id).filter_map(|s| s.instance.as_deref()).map(Some));
        }
        for instance in instances {
            if let Some(mut seen) = read_status(r, sink, m, instance)? {
                // Порты из прошлых записей остаются: статус их не сообщает.
                if let Some(old) = known.iter().find(|s| s.module == m.id && s.instance.as_deref() == instance) {
                    seen.ports = old.ports.clone();
                }
                found.push(seen);
            }
        }
    }
    sink.log(format!("Found {} server(s)", found.len()));
    Ok((arch, found))
}

/// Что сделать со службой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Start,
    Stop,
    Restart,
}

impl Verb {
    pub fn word(self) -> &'static str {
        match self {
            Verb::Start => "start",
            Verb::Stop => "stop",
            Verb::Restart => "restart",
        }
    }
}

/// Запустить, остановить или перезапустить службу и перечитать состояние.
pub fn service(
    r: &mut dyn Remote,
    sink: &mut Sink,
    m: &Manifest,
    instance: Option<&str>,
    verb: Verb,
) -> Result<Seen, Fail> {
    let naming = m.naming(instance).map_err(|e| Fail::Form(e.to_string()))?;
    let command = format!("systemctl {} {}", verb.word(), word(&naming.service));
    let out = run_logged(r, sink, &command, Stream::Log)?;
    if !out.ok() {
        return Err(Fail::Other(out.failure()));
    }
    Ok(read_status(r, sink, m, instance)?.unwrap_or(Seen {
        module: m.id.clone(),
        instance: instance.map(str::to_owned),
        checked: i18n::now(),
        ..Seen::default()
    }))
}

/// Резервная копия командой манифеста в `/var/backups/anvil/<служба>-<время>`.
pub fn backup(r: &mut dyn Remote, sink: &mut Sink, m: &Manifest, instance: Option<&str>) -> Result<(), Fail> {
    let naming = m.naming(instance).map_err(|e| Fail::Form(e.to_string()))?;
    let dir = format!("/var/backups/anvil/{}-{}", naming.service, i18n::now());
    let made = run_logged(r, sink, &format!("mkdir -p -- {}", quote(&dir)), Stream::Log)?;
    if !made.ok() {
        return Err(Fail::Other(made.failure()));
    }
    let ctx = Context { instance, dir: Some(&dir) };
    let command = m.command(CommandKind::Backup, &BTreeMap::new(), &ctx).map_err(|e| Fail::Form(e.to_string()))?;
    let out = run_logged(r, sink, &command, Stream::Log)?;
    if !out.ok() {
        return Err(Fail::Other(out.failure()));
    }
    sink.log(format!("Backup is in {dir} on the machine"));
    Ok(())
}

/// Удаление сервера командой `uninstall` манифеста.
pub fn uninstall(r: &mut dyn Remote, sink: &mut Sink, m: &Manifest, instance: Option<&str>) -> Result<(), Fail> {
    let command = m
        .command(CommandKind::Uninstall, &BTreeMap::new(), &Context { instance, dir: None })
        .map_err(|e| Fail::Form(e.to_string()))?;
    let out = run_logged(r, sink, &command, Stream::Log)?;
    if out.ok() { Ok(()) } else { Err(Fail::Other(out.failure())) }
}

/// Действие манифеста № `index`. Подтверждение — до вызова (в виде), здесь команда только выполняется.
pub fn action(
    r: &mut dyn Remote,
    sink: &mut Sink,
    m: &Manifest,
    instance: Option<&str>,
    index: usize,
) -> Result<Option<Seen>, Fail> {
    let command = m.action_command(index, &Context { instance, dir: None }).map_err(|e| Fail::Form(e.to_string()))?;
    let out = run_logged(r, sink, &command, Stream::Log)?;
    if !out.ok() {
        return Err(Fail::Other(out.failure()));
    }
    read_status(r, sink, m, instance)
}

/// Команда журнала службы: последние [`JOURNAL_LINES`] строк `journalctl`.
pub fn journal_command(m: &Manifest, instance: Option<&str>) -> Result<String, Fail> {
    let naming = m.naming(instance).map_err(|e| Fail::Form(e.to_string()))?;
    Ok(format!("journalctl -u {} -n {JOURNAL_LINES} --no-pager", word(&naming.service)))
}

/// Журнал службы — в окно вкладки «Журнал».
pub fn journal(r: &mut dyn Remote, sink: &mut Sink, m: &Manifest, instance: Option<&str>) -> Result<(), Fail> {
    let command = journal_command(m, instance)?;
    let out = run_logged(r, sink, &command, Stream::Journal)?;
    if out.ok() { Ok(()) } else { Err(Fail::Other(out.failure())) }
}

/// Команда консоли — от root; подтверждение, как и для действий, — до вызова.
pub fn console(r: &mut dyn Remote, sink: &mut Sink, command: &str) -> Result<(), Fail> {
    let out = run_logged(r, sink, command, Stream::Console)?;
    if out.ok() { Ok(()) } else { Err(Fail::Other(format!("exit code {}", out.code))) }
}

// ─── Задачи, события и состояние ───────────────────────────────────────────

/// Что сделать на машине.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Check,
    Deploy {
        module: String,
        instance: Option<String>,
        form: BTreeMap<String, String>,
        update: bool,
        allow_unverified: bool,
    },
    Service {
        module: String,
        instance: Option<String>,
        verb: Verb,
    },
    Backup {
        module: String,
        instance: Option<String>,
    },
    Uninstall {
        module: String,
        instance: Option<String>,
    },
    Action {
        module: String,
        instance: Option<String>,
        index: usize,
    },
    Journal {
        module: String,
        instance: Option<String>,
    },
    Console {
        command: String,
    },
}

impl Job {
    /// Короткая подпись для строки состояния и уведомлений.
    pub fn label(&self) -> &'static str {
        match self {
            Job::Check => t("Проверка"),
            Job::Deploy { update: false, .. } => t("Установка"),
            Job::Deploy { update: true, .. } => t("Обновление"),
            Job::Service { verb: Verb::Start, .. } => t("Запуск"),
            Job::Service { verb: Verb::Stop, .. } => t("Остановка"),
            Job::Service { verb: Verb::Restart, .. } => t("Перезапуск"),
            Job::Backup { .. } => t("Копия"),
            Job::Uninstall { .. } => t("Удаление"),
            Job::Action { .. } => t("Действие"),
            Job::Journal { .. } => t("Журнал"),
            Job::Console { .. } => t("Консоль"),
        }
    }
}

/// Что получилось.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    Checked { arch: Option<String>, seen: Vec<Seen>, probed: Vec<String> },
    Seen(Seen),
    Removed { module: String, instance: Option<String> },
    Done,
}

/// Почему не получилось.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Первое подключение: отпечаток ключа, которому надо поверить (или нет).
    Trust(String),
    /// Ключ машины не тот, которому поверили. Ни один секрет не отправлен.
    KeyChanged {
        was: String,
        now: String,
    },
    Fail(Fail),
}

/// Событие из рабочего потока.
pub enum Event {
    Emit(String, Emit),
    Done { machine: String, job: Box<Job>, result: Result<Applied, Problem> },
}

/// Вопрос владельцу, на который ждёт задача.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    Trust { machine: String, fingerprint: String, job: Job },
    KeyChanged { machine: String, was: String, now: String, job: Job },
    Unverified { machine: String, reason: String, job: Job },
}

/// Порт оказался занят: вернуть владельца в форму установки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reopen {
    pub machine: String,
    pub module: String,
    pub instance: Option<String>,
    pub form: BTreeMap<String, String>,
    pub update: bool,
    pub message: String,
}

/// Что известно о ходе дел на машине (не пишется на диск).
#[derive(Debug, Default)]
pub struct Runtime {
    /// «Ход работы»: до [`LOG_LINES`] строк.
    pub log: VecDeque<String>,
    /// Что делается сейчас.
    pub busy: Option<&'static str>,
    pub progress: Option<(u64, u64)>,
    /// Последняя ошибка проверки или задачи — полоса над карточками.
    pub error: Option<String>,
    pub journal: Vec<String>,
    pub console: Vec<String>,
}

impl Runtime {
    fn push(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }
}

/// Что приём событий просит сделать окно.
#[derive(Default)]
pub struct Applying {
    /// Настройки изменились — сохранить.
    pub save: bool,
    pub toasts: Vec<(String, Tone)>,
}

/// Как разбудить окно из рабочего потока.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Реестр модулей, ход дел на машинах и канал событий.
pub struct State {
    pub registry: Registry,
    pub runtime: std::collections::HashMap<String, Runtime>,
    pub prompt: Option<Prompt>,
    pub reopen: Option<Reopen>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    wake: Wake,
}

impl State {
    pub fn new(wake: Wake, config_path: &Path) -> State {
        let (tx, rx) = channel();
        State {
            registry: Registry::load(anvil_servers::builtin::BUILTIN, config_path.parent()),
            runtime: Default::default(),
            prompt: None,
            reopen: None,
            tx,
            rx,
            wake,
        }
    }

    /// Перечитать манифесты из папки настроек.
    pub fn reload(&mut self, config_path: &Path) {
        self.registry = Registry::load(anvil_servers::builtin::BUILTIN, config_path.parent());
    }

    pub fn runtime(&self, machine: &str) -> Option<&Runtime> {
        self.runtime.get(machine)
    }

    pub fn busy(&self, machine: &str) -> bool {
        self.runtime.get(machine).is_some_and(|r| r.busy.is_some())
    }

    pub fn any_busy(&self) -> bool {
        self.runtime.values().any(|r| r.busy.is_some())
    }

    /// Показывается ли найденный на машине модуль: скрытые — только при включённом переключателе.
    pub fn shown(&self, module: &str, show_hidden: bool) -> bool {
        show_hidden || self.registry.get(module, true).is_none_or(|m| !m.manifest.hidden)
    }

    /// Запустить задачу на машине в своём потоке. `false` — нельзя (машина занята или нет манифеста).
    pub fn start(&mut self, machine: &Machine, job: Job, show_hidden: bool) -> bool {
        if self.busy(&machine.id) {
            return false;
        }
        let runtime = self.runtime.entry(machine.id.clone()).or_default();
        let manifests: Vec<Manifest> = match &job {
            Job::Check => self.registry.modules(show_hidden).map(|m| m.manifest.clone()).collect(),
            Job::Console { .. } => Vec::new(),
            Job::Deploy { module, .. }
            | Job::Service { module, .. }
            | Job::Backup { module, .. }
            | Job::Uninstall { module, .. }
            | Job::Action { module, .. }
            | Job::Journal { module, .. } => match self.registry.get(module, show_hidden) {
                Some(found) => vec![found.manifest.clone()],
                None => {
                    runtime.push(format!("Module `{module}` is not available"));
                    return false;
                }
            },
        };
        runtime.busy = Some(job.label());
        runtime.progress = None;
        match &job {
            Job::Journal { .. } => runtime.journal.clear(),
            Job::Console { command } => runtime.console.push(format!("$ {command}")),
            _ => {}
        }
        let request = Request { machine: machine.clone(), manifests, job };
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        let id = machine.id.clone();
        let spawned = std::thread::Builder::new().name("anvil-servers".into()).spawn(move || run(request, tx, wake));
        if spawned.is_err() {
            self.finish(&id);
            return false;
        }
        true
    }

    fn finish(&mut self, machine: &str) {
        if let Some(runtime) = self.runtime.get_mut(machine) {
            runtime.busy = None;
            runtime.progress = None;
        }
    }

    /// Принять события потоков: записать в настройки, что нашли, и вернуть, о чём сказать владельцу.
    pub fn poll(&mut self, machines: &mut [Machine]) -> Applying {
        let mut applying = Applying::default();
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Emit(id, emit) => {
                    let runtime = self.runtime.entry(id).or_default();
                    match emit {
                        Emit::Log(line) => runtime.push(line),
                        Emit::Journal(line) => runtime.journal.push(line),
                        Emit::Console(line) => runtime.console.push(line),
                        Emit::Progress(got, total) => runtime.progress = Some((got, total)),
                    }
                }
                Event::Done { machine, job, result } => self.done(machines, &machine, *job, result, &mut applying),
            }
        }
        applying
    }

    fn done(
        &mut self,
        machines: &mut [Machine],
        id: &str,
        job: Job,
        result: Result<Applied, Problem>,
        out: &mut Applying,
    ) {
        self.finish(id);
        let Some(machine) = machines.iter_mut().find(|m| m.id == id) else { return };
        let name = machine.name.clone();
        let label = job.label();
        match result {
            Ok(applied) => {
                if let Some(runtime) = self.runtime.get_mut(id) {
                    runtime.error = None;
                }
                let quiet = matches!(job, Job::Check | Job::Journal { .. } | Job::Console { .. });
                match applied {
                    Applied::Checked { arch, seen, probed } => {
                        machine.modules.retain(|s| !probed.contains(&s.module));
                        for found in seen {
                            machine.upsert(found);
                        }
                        machine.arch = arch.unwrap_or_default();
                        machine.checked = i18n::now();
                        out.save = true;
                    }
                    Applied::Seen(seen) => {
                        machine.upsert(seen);
                        out.save = true;
                    }
                    Applied::Removed { module, instance } => {
                        machine.remove(&module, instance.as_deref());
                        out.save = true;
                    }
                    Applied::Done => {}
                }
                if !quiet {
                    out.toasts.push((format!("{name}: {label} — {}", t("готово")), Tone::Success));
                }
            }
            Err(Problem::Trust(fingerprint)) => {
                self.prompt = Some(Prompt::Trust { machine: id.to_owned(), fingerprint, job });
            }
            Err(Problem::KeyChanged { was, now }) => {
                if let Some(runtime) = self.runtime.get_mut(id) {
                    runtime.push(format!("The machine key changed: was {was}, now {now}. Nothing was sent."));
                }
                self.prompt = Some(Prompt::KeyChanged { machine: id.to_owned(), was, now, job });
            }
            Err(Problem::Fail(Fail::Unverified(reason))) => {
                self.prompt = Some(Prompt::Unverified { machine: id.to_owned(), reason, job });
            }
            Err(Problem::Fail(fail)) => {
                let message = fail.to_string();
                if let Some(runtime) = self.runtime.get_mut(id) {
                    runtime.push(format!("Failed: {message}"));
                    runtime.error = Some(describe(&fail));
                }
                if let (Fail::Port(busy), Job::Deploy { module, instance, form, update, .. }) = (&fail, &job) {
                    self.reopen = Some(Reopen {
                        machine: id.to_owned(),
                        module: module.clone(),
                        instance: instance.clone(),
                        form: form.clone(),
                        update: *update,
                        message: port_message(busy),
                    });
                }
                out.toasts.push((format!("{name}: {label} — {}", t("не вышло")), Tone::Danger));
            }
        }
    }
}

/// Причина для человека: начало по-русски (или по-английски), подробность — как её назвало ядро.
pub fn describe(fail: &Fail) -> String {
    match fail {
        Fail::Mismatch(detail) => {
            format!("{} ({detail})", t("Контрольная сумма файла не сошлась — на машину ничего не залито"))
        }
        Fail::Port(busy) => port_message(busy),
        Fail::Arch(_) => t("Для архитектуры этой машины выпусков нет").to_owned(),
        Fail::Unverified(_) => t("В выпуске нет контрольной суммы").to_owned(),
        Fail::Form(detail) | Fail::Other(detail) => detail.clone(),
    }
}

/// «Порт 8740 занят…» на языке окна.
pub fn port_message(busy: &PortBusy) -> String {
    use anvil_servers::instance::PortHolder;
    let port = busy.port;
    match (&busy.holder, i18n::is_english()) {
        (PortHolder::Machine, false) => {
            format!("Порт {port} уже слушает другая программа на машине. Выберите другой порт.")
        }
        (PortHolder::Machine, true) => {
            format!("Port {port} is already listened on by another program on the machine. Pick another port.")
        }
        (PortHolder::Instance(name), false) if name.is_empty() => {
            format!("Порт {port} занят основным экземпляром. Выберите другой порт.")
        }
        (PortHolder::Instance(name), false) => format!("Порт {port} занят экземпляром «{name}». Выберите другой порт."),
        (PortHolder::Instance(name), true) if name.is_empty() => {
            format!("Port {port} is used by the main instance. Pick another port.")
        }
        (PortHolder::Instance(name), true) => format!("Port {port} is used by instance \"{name}\". Pick another port."),
        (PortHolder::Form, false) => format!("Порт {port} указан в форме дважды."),
        (PortHolder::Form, true) => format!("Port {port} is given twice in the form."),
    }
}

// ─── Рабочий поток ─────────────────────────────────────────────────────────

/// Что отдано потоку: машина, манифесты, которые ей нужны, и сама задача.
pub struct Request {
    pub machine: Machine,
    pub manifests: Vec<Manifest>,
    pub job: Job,
}

fn run(request: Request, tx: Sender<Event>, wake: Wake) {
    let id = request.machine.id.clone();
    let result = work(&request, &tx, &wake);
    let _ = tx.send(Event::Done { machine: id, job: Box::new(request.job), result });
    wake();
}

fn work(request: &Request, tx: &Sender<Event>, wake: &Wake) -> Result<Applied, Problem> {
    let machine = &request.machine;
    let id = machine.id.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Problem::Fail(Fail::Other(format!("cannot start the network runtime: {e}"))))?;
    let secrets = SystemStore::default();
    let (mut ssh, password) = connect(&rt, machine, &secrets)?;
    let mut github = Github { rt: &rt, token: None, looked: false };
    let mut out = |emit: Emit| {
        let _ = tx.send(Event::Emit(id.clone(), emit));
        wake();
    };
    let mut sink = Sink::new(&mut out);
    if let Some(password) = &password {
        sink.hide(password);
    }
    let result = perform(&mut ssh, &mut github, &mut sink, request);
    ssh.close();
    result.map_err(Problem::Fail)
}

/// Вход: сначала ключ машины (первое подключение — спросить), потом секреты. Второе в ответе — пароль входа, чтобы
/// журнал прятал его, даже если он вдруг окажется в выводе.
fn connect<'a>(
    rt: &'a tokio::runtime::Runtime,
    machine: &Machine,
    secrets: &dyn SecretStore,
) -> Result<(Ssh<'a>, Option<String>), Problem> {
    let fail = |text: String| Problem::Fail(Fail::Other(text));
    if machine.fingerprint.trim().is_empty() {
        // Разведка не говорит о нас ничего: ни логина, ни пароля.
        let fingerprint = rt.block_on(ssh::probe(&machine.host, machine.port)).map_err(|e| fail(e.to_string()))?;
        return Err(Problem::Trust(fingerprint));
    }
    let secret = |what: &str| match secrets.machine(&machine.id) {
        Ok(found) => Ok(found),
        Err(e) => Err(fail(format!("{what}: {e}"))),
    };
    let (auth, hidden) = match &machine.login {
        Login::Password => match secret("cannot read the saved password")? {
            Some(password) => (Auth::Password(password.clone()), Some(password)),
            None => return Err(fail("no saved password for this machine: open Edit and enter it".to_owned())),
        },
        Login::Key(path) => {
            let passphrase = secrets.machine(&machine.id).ok().flatten();
            (Auth::Key { path: path.clone(), passphrase: passphrase.clone() }, passphrase)
        }
        Login::Agent => (Auth::Agent, None),
    };
    let target =
        Target { host: &machine.host, port: machine.port, user: &machine.user, auth, trusted: &machine.fingerprint };
    match rt.block_on(ssh::connect(target)) {
        Ok(conn) => Ok((Ssh { rt, conn }, hidden)),
        Err(ConnectError::KeyChanged { expected, got }) => Err(Problem::KeyChanged { was: expected, now: got }),
        Err(e) => Err(fail(e.to_string())),
    }
}

/// Выполнить задачу на открытой машине.
pub fn perform(
    r: &mut dyn Remote,
    rel: &mut dyn Releaser,
    sink: &mut Sink,
    request: &Request,
) -> Result<Applied, Fail> {
    let one = || request.manifests.first().ok_or_else(|| Fail::Other("the module is not available".to_owned()));
    match &request.job {
        Job::Check => {
            let (arch, seen) = check(r, sink, &request.manifests, &request.machine.modules)?;
            let probed = request.manifests.iter().map(|m| m.id.clone()).collect();
            Ok(Applied::Checked { arch: arch.map(|a| a.key().to_owned()), seen, probed })
        }
        Job::Deploy { instance, form, update, allow_unverified, module } => {
            let others = request.machine.other_ports(module, instance.as_deref());
            let deploy_plan = Deploy {
                manifest: one()?,
                instance: instance.as_deref(),
                form,
                update: *update,
                allow_unverified: *allow_unverified,
                others: &others,
            };
            deploy(r, rel, sink, &deploy_plan).map(Applied::Seen)
        }
        Job::Service { instance, verb, .. } => service(r, sink, one()?, instance.as_deref(), *verb).map(Applied::Seen),
        Job::Backup { instance, .. } => backup(r, sink, one()?, instance.as_deref()).map(|()| Applied::Done),
        Job::Uninstall { module, instance } => {
            uninstall(r, sink, one()?, instance.as_deref())?;
            Ok(Applied::Removed { module: module.clone(), instance: instance.clone() })
        }
        Job::Action { instance, index, .. } => {
            Ok(action(r, sink, one()?, instance.as_deref(), *index)?.map_or(Applied::Done, Applied::Seen))
        }
        Job::Journal { instance, .. } => journal(r, sink, one()?, instance.as_deref()).map(|()| Applied::Done),
        Job::Console { command } => console(r, sink, command).map(|()| Applied::Done),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anvil_servers::MemoryStore;

    const MANIFEST: &str = r#"
id = "relay"
name = "Relay"
instances = true
service = "relay"
binary = "/opt/relay/relay-server"

[source]
repo = "octo/relay"
assets = { x86_64 = "relay-server", aarch64 = "relay-server-arm64" }

[ports]
tcp = [8740]

[commands]
install = "{bin} install {args}"
status = "{bin} status --json"
backup = "{bin} backup --to {dir}"
uninstall = "{bin} uninstall"

[[form]]
key = "port"
label = "Port"
kind = "port"
default = 8740
arg = "--port {value}"

[[form]]
key = "key"
label = "Key"
kind = "password"
arg = "--key {value}"

[[action]]
label = "Restart"
cmd = "systemctl restart {service}"
"#;

    fn manifest() -> Manifest {
        Manifest::parse(MANIFEST).unwrap()
    }

    /// Подставная машина: пишет все команды и отвечает по сценарию.
    #[derive(Default)]
    struct Fake {
        arch: Option<Arch>,
        listening: String,
        /// Все команды по порядку: `user: …`, `root: …`, `upload: …`.
        seen: Vec<String>,
        install_code: u32,
        /// Что `install` напечатает (например, секрет, который не должен дойти до журнала).
        install_says: Vec<String>,
        installed: bool,
        mktemp_fails: bool,
    }

    impl Fake {
        fn new() -> Fake {
            Fake { arch: Some(Arch::X86_64), ..Fake::default() }
        }
    }

    impl Remote for Fake {
        fn arch(&mut self) -> Result<Option<Arch>, String> {
            self.seen.push("arch".into());
            Ok(self.arch)
        }

        fn user(&mut self, command: &str, _: &mut dyn FnMut(&str)) -> Result<Output, String> {
            self.seen.push(format!("user: {command}"));
            if command == "mktemp -d" {
                if self.mktemp_fails {
                    return Ok(Output { code: 1, stderr: "mktemp: no space left".into(), ..Output::default() });
                }
                return Ok(Output { stdout: "/tmp/tmp.AbCd12\n".into(), ..Output::default() });
            }
            Ok(Output::default())
        }

        fn root(&mut self, command: &str, line: &mut dyn FnMut(&str)) -> Result<Output, String> {
            self.seen.push(format!("root: {command}"));
            if command == LISTEN_COMMAND {
                return Ok(Output { stdout: self.listening.clone(), ..Output::default() });
            }
            if command.contains(" install ") || command.ends_with(" install") {
                for text in &self.install_says {
                    line(text);
                }
                if self.install_code == 0 {
                    self.installed = true;
                }
                return Ok(Output {
                    code: self.install_code,
                    stderr: "Error: install broke".into(),
                    ..Output::default()
                });
            }
            if command.contains("status --json") {
                if !self.installed {
                    return Ok(Output { code: 127, ..Output::default() });
                }
                return Ok(Output {
                    stdout: r#"{"name":"relay","version":"1.2.3","running":true,"address":"203.0.113.7:8740","clients":4,"token":"hunter2hunter2"}"#.into(),
                    ..Output::default()
                });
            }
            Ok(Output::default())
        }

        fn upload(&mut self, bytes: &[u8], path: &str, _: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
            self.seen.push(format!("upload: {path} ({} bytes)", bytes.len()));
            Ok(())
        }
    }

    /// Подставной выпуск.
    struct Fixed(Result<Fetched, ReleaseError>, usize);

    impl Releaser for Fixed {
        fn fetch(
            &mut self,
            _: &Source,
            arch: Arch,
            allow: bool,
            _: &mut dyn FnMut(u64, u64),
        ) -> Result<Fetched, ReleaseError> {
            self.1 += 1;
            assert_eq!(arch, Arch::X86_64);
            match (&self.0, allow) {
                (Err(ReleaseError::Unverified { .. }), true) => {
                    Ok(Fetched { tag: "v1".into(), file: "relay-server".into(), bytes: vec![1, 2, 3], verified: false })
                }
                (r, _) => r.clone(),
            }
        }
    }

    fn release() -> Fixed {
        Fixed(Ok(Fetched { tag: "v1".into(), file: "relay-server".into(), bytes: vec![0; 10], verified: true }), 0)
    }

    fn form(port: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("port".to_owned(), port.to_owned()), ("key".to_owned(), "s3cr3t-key".to_owned())])
    }

    fn run_deploy(
        fake: &mut Fake,
        rel: &mut Fixed,
        update: bool,
        allow: bool,
        form: &BTreeMap<String, String>,
    ) -> (Result<Seen, Fail>, Vec<Emit>) {
        let m = manifest();
        let others = BTreeMap::new();
        let mut emitted = Vec::new();
        let mut out = |e: Emit| emitted.push(e);
        let mut sink = Sink::new(&mut out);
        let d = Deploy { manifest: &m, instance: None, form, update, allow_unverified: allow, others: &others };
        let result = deploy(fake, rel, &mut sink, &d);
        (result, emitted)
    }

    fn logs(emitted: &[Emit]) -> String {
        emitted
            .iter()
            .filter_map(|e| if let Emit::Log(l) = e { Some(l.as_str()) } else { None })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn install_goes_through_every_step_and_cleans_up() {
        let mut fake = Fake::new();
        let mut rel = release();
        let (result, emitted) = run_deploy(&mut fake, &mut rel, false, false, &form("8740"));
        let seen = result.unwrap();
        assert_eq!((seen.module.as_str(), seen.running, seen.version.as_deref()), ("relay", Some(true), Some("1.2.3")));
        assert_eq!(seen.ports, [8740]);
        assert_eq!(seen.extra.get("clients").map(String::as_str), Some("4"));
        let steps: Vec<&str> = fake.seen.iter().map(String::as_str).collect();
        assert_eq!(steps[0], "arch");
        assert_eq!(steps[1], format!("root: {LISTEN_COMMAND}"));
        assert_eq!(steps[2], "user: mktemp -d");
        assert_eq!(steps[3], "upload: /tmp/tmp.AbCd12/relay-server (10 bytes)");
        assert_eq!(steps[4], "user: chmod 755 /tmp/tmp.AbCd12/relay-server");
        // Команда `install` запускает загруженный файл, а не будущий путь.
        assert!(steps[5].starts_with("root: /tmp/tmp.AbCd12/relay-server install --port 8740 --key"), "{}", steps[5]);
        assert_eq!(steps[6], "root: rm -rf -- /tmp/tmp.AbCd12");
        assert!(steps[7].contains("/opt/relay/relay-server status --json"), "status runs the installed binary");
        let log = logs(&emitted);
        assert!(log.contains("exit code 0") && log.contains("Install done"), "{log}");
        assert!(!log.contains("s3cr3t-key"), "a password field never reaches the work log: {log}");
    }

    #[test]
    fn a_busy_port_stops_everything_before_any_download() {
        let mut fake = Fake::new();
        fake.listening = "tcp LISTEN 0 4096 0.0.0.0:8740 0.0.0.0:*\n".into();
        let mut rel = release();
        let (result, _) = run_deploy(&mut fake, &mut rel, false, false, &form("8740"));
        let Err(Fail::Port(busy)) = result else { panic!("{result:?}") };
        assert_eq!(busy.port, 8740);
        assert_eq!(rel.1, 0, "nothing was downloaded");
        assert!(!fake.seen.iter().any(|s| s.contains("mktemp") || s.starts_with("upload")), "{:?}", fake.seen);
        // Обновление свои порты не сверяет: служба и держит этот порт.
        let mut again = Fake::new();
        again.listening = fake.listening.clone();
        assert!(run_deploy(&mut again, &mut release(), true, false, &form("8740")).0.is_ok());
    }

    #[test]
    fn a_checksum_mismatch_uploads_nothing() {
        let mut fake = Fake::new();
        let mismatch =
            ReleaseError::ChecksumMismatch { file: "relay-server".into(), tag: "v1".into(), sums: "SHA256SUMS".into() };
        let mut rel = Fixed(Err(mismatch), 0);
        let (result, _) = run_deploy(&mut fake, &mut rel, false, true, &form("8740"));
        let Err(Fail::Mismatch(text)) = result else { panic!("{result:?}") };
        assert!(text.contains("checksum mismatch"), "{text}");
        assert!(!fake.seen.iter().any(|s| s.contains("mktemp") || s.starts_with("upload")), "{:?}", fake.seen);
    }

    #[test]
    fn a_release_without_sums_asks_and_then_goes_on_when_allowed() {
        let unverified = ReleaseError::Unverified { tag: "v1".into(), reason: "the release has no SHA256SUMS".into() };
        let mut fake = Fake::new();
        let mut rel = Fixed(Err(unverified.clone()), 0);
        let (result, _) = run_deploy(&mut fake, &mut rel, false, false, &form("8740"));
        assert_eq!(result.unwrap_err(), Fail::Unverified("the release has no SHA256SUMS".into()));
        assert!(!fake.seen.iter().any(|s| s.starts_with("upload")));

        let mut fake = Fake::new();
        let mut rel = Fixed(Err(unverified), 0);
        let (result, emitted) = run_deploy(&mut fake, &mut rel, false, true, &form("8740"));
        assert!(result.is_ok());
        assert!(logs(&emitted).contains("NOT verified"));
    }

    #[test]
    fn a_failed_install_still_removes_the_temporary_directory() {
        let mut fake = Fake::new();
        fake.install_code = 2;
        let mut rel = release();
        let (result, emitted) = run_deploy(&mut fake, &mut rel, false, false, &form("8741"));
        assert_eq!(result.unwrap_err(), Fail::Other("install broke".into()));
        assert!(fake.seen.contains(&"root: rm -rf -- /tmp/tmp.AbCd12".to_owned()), "{:?}", fake.seen);
        assert!(!fake.seen.iter().any(|s| s.contains("status --json")), "no status after a failure");
        assert!(logs(&emitted).contains("exit code 2"));
    }

    #[test]
    fn a_bad_form_never_touches_the_machine() {
        let mut fake = Fake::new();
        let (result, _) = run_deploy(&mut fake, &mut release(), false, false, &form("1; reboot"));
        assert!(matches!(result, Err(Fail::Form(_))), "{result:?}");
        assert!(fake.seen.is_empty(), "{:?}", fake.seen);
    }

    #[test]
    fn an_unknown_architecture_and_a_broken_temp_directory_stop_cleanly() {
        let mut fake = Fake::new();
        fake.arch = None;
        assert!(matches!(run_deploy(&mut fake, &mut release(), false, false, &form("8740")).0, Err(Fail::Arch(_))));

        let mut fake = Fake::new();
        fake.mktemp_fails = true;
        let (result, _) = run_deploy(&mut fake, &mut release(), false, false, &form("8740"));
        assert!(matches!(&result, Err(Fail::Other(t)) if t.contains("temporary")), "{result:?}");
        assert!(!fake.seen.iter().any(|s| s.starts_with("upload") || s.contains("rm -rf")), "{:?}", fake.seen);
    }

    #[test]
    fn tokens_and_known_secrets_never_reach_the_work_log() {
        let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        let mut fake = Fake::new();
        fake.install_says = vec![format!("fetching with token={token}"), "hello pa55w0rd-of-login there".into()];
        let m = manifest();
        let others = BTreeMap::new();
        let f = form("8740");
        let mut emitted = Vec::new();
        let mut out = |e: Emit| emitted.push(e);
        let mut sink = Sink::new(&mut out);
        sink.hide("pa55w0rd-of-login");
        sink.hide(token);
        let d =
            Deploy { manifest: &m, instance: None, form: &f, update: false, allow_unverified: false, others: &others };
        deploy(&mut fake, &mut release(), &mut sink, &d).unwrap();
        let all = format!("{emitted:?}");
        assert!(!all.contains(token) && !all.contains("abcdefghijkl"), "{all}");
        assert!(!all.contains("pa55w0rd-of-login"), "{all}");
        assert!(all.contains('…'), "the secret is replaced by an ellipsis: {all}");
    }

    #[test]
    fn check_reads_the_main_server_and_known_instances_only() {
        let mut fake = Fake::new();
        fake.installed = true;
        let known = vec![
            Seen { module: "relay".into(), instance: Some("family".into()), ports: vec![9001], ..Seen::default() },
            Seen { module: "other".into(), ..Seen::default() },
        ];
        let m = manifest();
        let mut emitted = Vec::new();
        let mut out = |e: Emit| emitted.push(e);
        let mut sink = Sink::new(&mut out);
        let (arch, found) = check(&mut fake, &mut sink, &[m], &known).unwrap();
        assert_eq!(arch, Some(Arch::X86_64));
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].instance.as_deref(), Some("family"));
        assert_eq!(found[1].ports, [9001], "ports of earlier installs survive a check");
        assert!(fake.seen.iter().any(|s| s.contains("relay-family") || s.contains("status --json")));

        // Ничего не установлено: модулей нет, а не ошибка.
        let mut empty = Fake::new();
        let m = manifest();
        let mut out = |_: Emit| {};
        let mut sink = Sink::new(&mut out);
        let (_, found) = check(&mut empty, &mut sink, &[m], &[]).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn commands_beyond_the_contract_run_verbatim_and_are_shown() {
        let m = manifest();
        let mut fake = Fake::new();
        fake.installed = true;
        let mut emitted = Vec::new();
        let mut out = |e: Emit| emitted.push(e);
        let mut sink = Sink::new(&mut out);
        let seen = action(&mut fake, &mut sink, &m, Some("family"), 0).unwrap();
        assert!(seen.is_some());
        assert_eq!(fake.seen[0], "root: systemctl restart relay-family");
        service(&mut fake, &mut sink, &m, None, Verb::Stop).unwrap();
        assert!(fake.seen.contains(&"root: systemctl stop relay".to_owned()));
        assert!(logs(&emitted).contains("$ systemctl restart relay-family"));
        assert_eq!(journal_command(&m, None).unwrap(), "journalctl -u relay -n 200 --no-pager");
    }

    #[test]
    fn uninstall_and_console_report_the_exit_code() {
        let m = manifest();
        let mut fake = Fake::new();
        let mut out = |_: Emit| {};
        let mut sink = Sink::new(&mut out);
        uninstall(&mut fake, &mut sink, &m, None).unwrap();
        assert_eq!(fake.seen[0], "root: /opt/relay/relay-server uninstall");
        console(&mut fake, &mut sink, "uptime").unwrap();
        assert_eq!(fake.seen[1], "root: uptime");
    }

    #[test]
    fn journal_lines_go_to_the_journal_tab_not_the_work_log() {
        struct Says;
        impl Remote for Says {
            fn arch(&mut self) -> Result<Option<Arch>, String> {
                Ok(None)
            }
            fn user(&mut self, _: &str, _: &mut dyn FnMut(&str)) -> Result<Output, String> {
                Ok(Output::default())
            }
            fn root(&mut self, _: &str, line: &mut dyn FnMut(&str)) -> Result<Output, String> {
                line("Oct 29 relay[1]: started");
                Ok(Output::default())
            }
            fn upload(&mut self, _: &[u8], _: &str, _: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
                Ok(())
            }
        }
        let mut emitted = Vec::new();
        let mut out = |e: Emit| emitted.push(e);
        let mut sink = Sink::new(&mut out);
        journal(&mut Says, &mut sink, &manifest(), None).unwrap();
        assert!(emitted.contains(&Emit::Journal("Oct 29 relay[1]: started".into())));
        assert!(!logs(&emitted).contains("started"));
    }

    #[test]
    fn the_machine_list_round_trips_in_toml_without_secrets() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Wrap {
            servers: Vec<Machine>,
        }
        let mut one = Machine {
            id: "m1".into(),
            name: "Demo".into(),
            host: "203.0.113.7".into(),
            port: 2222,
            login: Login::Key(PathBuf::from("keys/demo")),
            fingerprint: "SHA256:abc".into(),
            arch: "x86_64".into(),
            ..Machine::default()
        };
        one.upsert(Seen {
            module: "relay".into(),
            instance: Some("family".into()),
            running: Some(true),
            version: Some("1.2.3".into()),
            extra: BTreeMap::from([("clients".to_owned(), "4".to_owned())]),
            ports: vec![9001],
            checked: 5,
            ..Seen::default()
        });
        let two = Machine {
            id: "m2".into(),
            name: "Second".into(),
            host: "198.51.100.2".into(),
            login: Login::Agent,
            ..Machine::default()
        };
        let three = Machine { id: "m3".into(), host: "h".into(), ..Machine::default() };
        let wrap = Wrap { servers: vec![one, two, three] };
        let text = toml::to_string_pretty(&wrap).unwrap();
        assert!(text.contains("[[servers]]"), "{text}");
        assert!(!text.to_lowercase().contains("passw") || text.contains("login = \"password\""), "{text}");
        assert_eq!(toml::from_str::<Wrap>(&text).unwrap(), wrap);
        // Запись без полей берёт умолчания.
        let minimal: Wrap = toml::from_str("[[servers]]\nname = 'x'\nhost = 'h'\n").unwrap();
        assert_eq!((minimal.servers[0].port, minimal.servers[0].user.as_str()), (22, "root"));
        assert_eq!(minimal.servers[0].login, Login::Password);
    }

    #[test]
    fn a_saved_password_lives_in_the_store_and_not_in_the_file() {
        let store = MemoryStore::default();
        store.set_machine("m1", "very-secret-pass").unwrap();
        let machine = Machine { id: "m1".into(), name: "Demo".into(), host: "h".into(), ..Machine::default() };
        let text = toml::to_string_pretty(&machine).unwrap();
        assert!(!text.contains("very-secret-pass"));
        assert_eq!(store.machine("m1").unwrap().as_deref(), Some("very-secret-pass"));
    }

    #[test]
    fn upsert_keeps_ports_and_the_tone_follows_the_modules() {
        let mut m = Machine { id: "m".into(), ..Machine::default() };
        m.upsert(Seen { module: "a".into(), running: Some(true), ports: vec![1], ..Seen::default() });
        m.upsert(Seen { module: "a".into(), running: Some(false), ..Seen::default() });
        assert_eq!(m.modules.len(), 1);
        assert_eq!(m.modules[0].ports, [1]);
        assert_eq!(m.tone(|_| false, false, false), Tone::Warning);
        assert_eq!(m.tone(|_| true, false, false), Tone::Neutral, "hidden modules do not colour the dot");
        assert_eq!(m.tone(|_| false, true, false), Tone::Danger);
        assert_eq!(m.tone(|_| false, false, true), Tone::Accent);
        assert_eq!(m.other_ports("a", Some("x")).len(), 1);
        m.remove("a", None);
        assert!(m.modules.is_empty());
    }

    #[test]
    fn temp_directories_are_checked_before_anything_is_deleted() {
        assert!(is_temp_dir("/tmp/tmp.AbCd12"));
        for bad in ["/", "/tmp", "tmp/x", "/tmp/../etc", "/tmp/a b", "/tmp/a;b", ""] {
            assert!(!is_temp_dir(bad), "{bad:?}");
        }
    }

    /// Папка настроек с манифестами: годный, скрытый и битый (без команды `install`).
    fn settings_with_modules() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("anvil-servers-test-{}", new_id()));
        let modules = dir.join("modules");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::write(modules.join("a-relay.toml"), MANIFEST).unwrap();
        let hidden = MANIFEST.replace("id = \"relay\"", "id = \"secret\"\nhidden = true");
        std::fs::write(modules.join("b-secret.toml"), hidden).unwrap();
        let broken = MANIFEST.replace("install = \"{bin} install {args}\"\n", "");
        std::fs::write(modules.join("c-broken.toml"), broken).unwrap();
        dir.join("anvil.toml")
    }

    fn state_in(config_path: &Path) -> State {
        State::new(Arc::new(|| {}), config_path)
    }

    #[test]
    fn hidden_modules_appear_only_with_the_switch_and_broken_ones_are_listed_with_a_reason() {
        let config = settings_with_modules();
        let state = state_in(&config);
        // Встроенные модули (сейчас один) в сравнение не входят: тест про файлы из папки настроек.
        let ids = |show| {
            let all = state.registry.modules(show).map(|m| m.manifest.id.clone());
            all.filter(|id| id != "ampulse").collect::<Vec<_>>()
        };
        assert_eq!(ids(false), ["relay"], "the switch is off: the hidden module is absent from the choice");
        assert_eq!(ids(true), ["relay", "secret"]);
        assert!(state.shown("relay", false) && !state.shown("secret", false) && state.shown("secret", true));
        assert!(state.shown("unknown", false), "a module without a manifest is not hidden, it is simply unknown");
        let broken: Vec<_> = state.registry.broken(false).map(|b| (b.origin.label(), b.reason.clone())).collect();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].0, "c-broken.toml");
        assert!(broken[0].1.starts_with("no install command"), "{}", broken[0].1);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn a_hidden_module_found_on_a_machine_stays_out_of_the_dot_while_the_switch_is_off() {
        let config = settings_with_modules();
        let state = state_in(&config);
        let machine = Machine {
            id: "m".into(),
            modules: vec![
                Seen { module: "relay".into(), running: Some(true), ..Seen::default() },
                Seen { module: "secret".into(), running: Some(false), ..Seen::default() },
            ],
            ..Machine::default()
        };
        let off = machine.tone(|s| !state.shown(&s.module, false), false, false);
        assert_eq!(off, Tone::Success, "the stopped hidden module does not colour the dot");
        assert_eq!(machine.tone(|s| !state.shown(&s.module, true), false, false), Tone::Warning);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn finished_jobs_update_the_cache_and_ask_the_owner_where_needed() {
        let config = settings_with_modules();
        let mut state = state_in(&config);
        let machine =
            Machine { id: "m1".into(), name: "Demo".into(), host: "203.0.113.5".into(), ..Machine::default() };
        let mut machines = vec![machine];
        let relay = Seen { module: "relay".into(), running: Some(true), ports: vec![8740], ..Seen::default() };
        let mut applying = Applying::default();
        state.runtime.entry("m1".into()).or_default().busy = Some("Check");
        let checked = Applied::Checked { arch: Some("x86_64".into()), seen: vec![relay], probed: vec!["relay".into()] };
        state.done(&mut machines, "m1", Job::Check, Ok(checked), &mut applying);
        assert!(applying.save && state.runtime("m1").unwrap().busy.is_none());
        assert_eq!((machines[0].arch.as_str(), machines[0].modules.len()), ("x86_64", 1));

        // Первое подключение: задача ждёт ответа владельца, ключ не записан.
        state.done(&mut machines, "m1", Job::Check, Err(Problem::Trust("SHA256:abc".into())), &mut applying);
        assert!(matches!(state.prompt, Some(Prompt::Trust { .. })) && machines[0].fingerprint.is_empty());

        // Ключ сменился: прежний отпечаток не затёрт, секреты не ушли.
        machines[0].fingerprint = "SHA256:old".into();
        let changed = Problem::KeyChanged { was: "SHA256:old".into(), now: "SHA256:new".into() };
        state.done(&mut machines, "m1", Job::Check, Err(changed), &mut applying);
        assert!(
            matches!(&state.prompt, Some(Prompt::KeyChanged { was, now, .. }) if was == "SHA256:old" && now == "SHA256:new")
        );
        assert_eq!(machines[0].fingerprint, "SHA256:old");

        // Нет суммы: вопрос; занятый порт: форма возвращается с причиной.
        let deploy = Job::Deploy {
            module: "relay".into(),
            instance: Some("x".into()),
            form: form("8740"),
            update: false,
            allow_unverified: false,
        };
        let unverified = Problem::Fail(Fail::Unverified("no SHA256SUMS".into()));
        state.done(&mut machines, "m1", deploy.clone(), Err(unverified), &mut applying);
        assert!(matches!(state.prompt, Some(Prompt::Unverified { .. })));
        let busy = PortBusy { port: 8740, holder: instance::PortHolder::Machine };
        state.done(&mut machines, "m1", deploy, Err(Problem::Fail(Fail::Port(busy))), &mut applying);
        let reopen = state.reopen.take().unwrap();
        assert_eq!((reopen.module.as_str(), reopen.instance.as_deref()), ("relay", Some("x")));
        assert!(reopen.message.contains("8740"), "{}", reopen.message);
        assert!(state.runtime("m1").unwrap().error.is_some());

        // Удаление убирает сервер из кэша.
        let gone = Applied::Removed { module: "relay".into(), instance: None };
        state.done(
            &mut machines,
            "m1",
            Job::Uninstall { module: "relay".into(), instance: None },
            Ok(gone),
            &mut applying,
        );
        assert!(machines[0].modules.is_empty());
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn the_work_log_keeps_the_last_400_lines() {
        let mut runtime = Runtime::default();
        for n in 0..(LOG_LINES + 25) {
            runtime.push(format!("line {n}"));
        }
        assert_eq!(runtime.log.len(), LOG_LINES);
        assert_eq!(runtime.log.front().map(String::as_str), Some("line 25"));
    }

    #[test]
    fn the_modules_folder_is_next_to_the_settings() {
        assert_eq!(modules_dir(Path::new("C:/x/Anvil/anvil.toml")), Path::new("C:/x/Anvil").join("modules"));
    }
}
