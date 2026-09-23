//! Очередь задач: cargo-команды по одной, с живым логом, ходом, ошибками и отменой.
//!
//! Задачи идут строго по очереди: cargo всё равно держит замок на папке сборки, а две большие
//! сборки разом у Amber уже роняли компоновщик по памяти.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use eframe::egui;

use anvil_update::install::Source;

use crate::installs;
use crate::launch;
use crate::run;

pub type JobId = u64;

/// Что запустить.
#[derive(Debug, Clone)]
pub struct Spec {
    pub project: PathBuf,
    /// Команда так, как её набрали бы в терминале: `cargo build --release`.
    pub title: String,
    pub program: String,
    pub args: Vec<String>,
    /// Вывод в `--message-format=json`: ход по единицам сборки и разобранные ошибки.
    pub json: bool,
    /// Сколько единиц сборки было в прошлый раз у этой же команды — для полосы хода.
    pub expected_units: Option<u32>,
    /// Что сделать до команды: остановить программу, отодвинуть занятый exe.
    pub before: Vec<Before>,
    /// Что сделать после успеха: запустить собранную программу.
    pub after: Option<launch::Launch>,
    /// Поставить собранный exe в `%LOCALAPPDATA%\Programs` (после успешной сборки).
    pub install: Option<InstallStep>,
    /// Задача без процесса: скачать выпуск с GitHub и поставить.
    pub download: Option<Download>,
    /// Ход — в байтах (скачивание), а не в единицах сборки.
    pub bytes: bool,
    /// Задача-сценарий: шаги по порядку, до первой ошибки (выпуск версии).
    pub script: Option<Vec<Step>>,
}

/// Шаг сценария.
#[derive(Debug, Clone)]
pub enum Step {
    /// Записать файлы: правки манифестов, заметки к выпуску.
    Write(Vec<(PathBuf, String)>),
    /// Выполнить команду в папке проекта; вывод — в лог.
    Run(String, Vec<String>),
    /// Упаковать exe по соглашению: `<bin>-X.Y.Z-windows-x64.zip` и `SHA256SUMS` в `out`.
    Package { exes: Vec<(String, PathBuf)>, version: anvil_update::Version, out: PathBuf },
    /// Создать GitHub Release и залить всё из `out`.
    Publish { repo: String, tag: String, notes: String, prerelease: bool, out: PathBuf },
    /// Запомнить файлы как есть: если дальше что-то упадёт, они вернутся (обновление зависимостей
    /// откатывает `Cargo.lock` и манифесты, когда тесты покраснели).
    Snapshot(Vec<PathBuf>),
}

/// Поставить свежесобранный exe новой версией установки.
#[derive(Debug, Clone)]
pub struct InstallStep {
    pub bin: String,
    /// Что собралось: `target\release\<bin>.exe`.
    pub exe: PathBuf,
    /// Метка версии: `0.1.0-3f89301`.
    pub label: String,
}

/// Скачать архив выпуска, сверить SHA-256 и поставить.
#[derive(Debug, Clone)]
pub struct Download {
    pub bin: String,
    pub version: String,
    pub asset: String,
    /// Прямые адреса и адреса в API: с токеном берутся вторые — так видны и приватные выпуски.
    pub asset_url: String,
    pub asset_api: String,
    pub sums_url: String,
    pub sums_api: String,
}

#[derive(Debug, Clone)]
pub enum Before {
    /// Попросить программу закрыться; не закрылась за 5 с — остановить.
    Stop(u32),
    /// Переименовать занятый exe, чтобы сборка могла записать новый.
    MoveAside(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Обычный вывод.
    Text,
    /// Первая строка ошибки компилятора.
    Error,
    /// Первая строка предупреждения.
    Warning,
    /// Заметка самого Anvil: что он сделал до и после команды.
    Note,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

/// Ошибка или предупреждение компилятора с местом в коде.
#[derive(Debug, Clone)]
pub struct Diag {
    pub level: Level,
    pub message: String,
    /// Файл относительно корня проекта, строка, столбец.
    pub place: Option<(PathBuf, u32, u32)>,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub ok: bool,
    pub cancelled: bool,
    pub code: Option<i32>,
    pub units: u32,
    /// Тесты: прошло и упало (сумма по всем «test result:»).
    pub tests: Option<(u32, u32)>,
    /// Запуск программы после сборки: PID или ошибка.
    pub launched: Option<Result<u32, String>>,
    /// Установка: какая версия встала или почему нет.
    pub installed: Option<Result<String, String>>,
    /// Выпуск: страница GitHub Release, если его создали отсюда.
    pub published: Option<String>,
}

pub enum Event {
    Started(JobId),
    Lines(JobId, Vec<Line>),
    Units(JobId, u32),
    Diag(JobId, Diag),
    Finished(JobId, Outcome),
    /// Сообщение не о задаче: например, итог остановки программы.
    Note(String, bool),
}

pub enum Cmd {
    Run(JobId, Box<Spec>),
    Cancel(JobId),
}

pub fn spawn(ctx: egui::Context) -> (Sender<Cmd>, Receiver<Event>, Sender<Event>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let events = event_tx.clone();
    std::thread::Builder::new()
        .name("anvil-jobs".into())
        .spawn(move || Runner { ctx, events: event_tx, queue: VecDeque::new() }.run(cmd_rx))
        .expect("spawn jobs");
    (cmd_tx, event_rx, events)
}

struct Runner {
    ctx: egui::Context,
    events: Sender<Event>,
    queue: VecDeque<(JobId, Spec)>,
}

/// Что пришло от потоков, читающих вывод.
enum Output {
    Out(String),
    Err(String),
    Closed,
}

impl Runner {
    fn send(&self, event: Event) {
        let _ = self.events.send(event);
        self.ctx.request_repaint();
    }

    fn run(mut self, commands: Receiver<Cmd>) {
        loop {
            while self.queue.is_empty() {
                match commands.recv() {
                    Ok(cmd) => {
                        self.accept(cmd, None);
                    }
                    Err(_) => return,
                }
            }
            let (id, spec) = self.queue.pop_front().expect("queue is not empty");
            self.execute(id, spec, &commands);
        }
    }

    /// Команда во время работы или ожидания. Возвращает `true`, если отменили текущую задачу.
    fn accept(&mut self, cmd: Cmd, current: Option<JobId>) -> bool {
        match cmd {
            Cmd::Run(id, spec) => self.queue.push_back((id, *spec)),
            Cmd::Cancel(id) if Some(id) == current => return true,
            Cmd::Cancel(id) => {
                if let Some(pos) = self.queue.iter().position(|(queued, _)| *queued == id) {
                    self.queue.remove(pos);
                    self.send(Event::Finished(id, Outcome { cancelled: true, ..Outcome::default() }));
                }
            }
        }
        false
    }

    fn note(&self, id: JobId, text: impl Into<String>) {
        self.send(Event::Lines(id, vec![Line { kind: LineKind::Note, text: text.into() }]));
    }

    fn execute(&mut self, id: JobId, spec: Spec, commands: &Receiver<Cmd>) {
        self.send(Event::Started(id));
        for step in &spec.before {
            match step {
                Before::Stop(pid) => {
                    self.note(id, format!("› stop PID {pid}"));
                    if let Err(e) = launch::stop(*pid, &spec.project) {
                        self.note(id, format!("  {e}"));
                    }
                }
                Before::MoveAside(exe) => match launch::move_aside(exe) {
                    Ok(to) => self.note(id, format!("› {} → {}", exe.display(), to.display())),
                    Err(e) => {
                        self.note(id, format!("› {}: {e}", exe.display()));
                        self.send(Event::Finished(id, Outcome::default()));
                        return;
                    }
                },
            }
        }
        self.note(id, format!("› {}", spec.title));

        if let Some(download) = &spec.download {
            let outcome = self.download(id, download);
            self.send(Event::Finished(id, outcome));
            return;
        }
        if let Some(steps) = &spec.script {
            let outcome = self.script(id, &spec.project, steps);
            self.send(Event::Finished(id, outcome));
            return;
        }

        let started = Instant::now();
        let mut child = match run::command(&spec.program, &spec.project)
            .args(&spec.args)
            .env("CARGO_TERM_COLOR", "never")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                self.note(id, format!("{}: {e}", spec.program));
                self.send(Event::Finished(id, Outcome::default()));
                return;
            }
        };

        let (out_tx, out_rx) = std::sync::mpsc::channel();
        read_lines(child.stdout.take(), out_tx.clone(), Output::Out);
        read_lines(child.stderr.take(), out_tx, Output::Err);

        let mut outcome = Outcome::default();
        let mut lines = Vec::new();
        let mut last_flush = Instant::now();
        let mut open_pipes = 2;
        let mut cancelled = false;
        while open_pipes > 0 {
            match out_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Output::Out(text)) => self.stdout_line(id, &spec, text, &mut lines, &mut outcome),
                Ok(Output::Err(text)) => lines.push(Line { kind: stderr_kind(&text), text }),
                Ok(Output::Closed) => open_pipes -= 1,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            while let Ok(cmd) = commands.try_recv() {
                if self.accept(cmd, Some(id)) && !cancelled {
                    cancelled = true;
                    kill_tree(&mut child, &spec.project);
                    lines.push(Line { kind: LineKind::Note, text: "› cancelled".into() });
                }
            }
            if !lines.is_empty() && (last_flush.elapsed() >= Duration::from_millis(100) || lines.len() > 500) {
                self.send(Event::Lines(id, std::mem::take(&mut lines)));
                last_flush = Instant::now();
            }
        }
        if !lines.is_empty() {
            self.send(Event::Lines(id, lines));
        }

        let status = child.wait().ok();
        outcome.code = status.and_then(|s| s.code());
        outcome.cancelled = cancelled;
        outcome.ok = !cancelled && status.is_some_and(|s| s.success());
        self.note(
            id,
            format!("› {} ({:.1} s)", if outcome.ok { "ok" } else { "failed" }, started.elapsed().as_secs_f32()),
        );

        if outcome.ok
            && let Some(step) = &spec.install
        {
            let result = install_local(step);
            match &result {
                Ok(version) => self.note(id, format!("› {} {version} → current", step.bin)),
                Err(e) => self.note(id, format!("› {}: {e}", step.bin)),
            }
            outcome.ok = result.is_ok();
            outcome.installed = Some(result);
        }

        if outcome.ok
            && let Some(launch) = &spec.after
        {
            let result = launch::start(launch);
            match &result {
                Ok(pid) => self.note(id, format!("› {} (PID {pid})", launch.exe.display())),
                Err(e) => self.note(id, format!("› {}: {e}", launch.exe.display())),
            }
            outcome.launched = Some(result);
        }
        self.send(Event::Finished(id, outcome));
    }

    /// Сценарий: шаги по порядку; первая ошибка останавливает всё, что дальше.
    fn script(&self, id: JobId, dir: &Path, steps: &[Step]) -> Outcome {
        let mut outcome = Outcome::default();
        let started = Instant::now();
        // Что вернуть, если шаг упадёт: путь и прежнее содержимое (`None` — файла не было).
        let mut saved: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
        for (n, step) in steps.iter().enumerate() {
            self.send(Event::Units(id, n as u32));
            let result: Result<(), String> = match step {
                Step::Write(files) => files.iter().try_for_each(|(path, text)| {
                    self.note(id, format!("› write {}", path.display()));
                    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
                }),
                Step::Run(program, args) => {
                    self.note(id, format!("› {program} {}", args.join(" ")));
                    match run::command(program, dir).args(args).env("CARGO_TERM_COLOR", "never").output() {
                        Ok(out) => {
                            let text = [out.stdout, out.stderr].concat();
                            let lines: Vec<Line> = String::from_utf8_lossy(&text)
                                .lines()
                                .filter(|l| !l.trim().is_empty())
                                .map(|l| Line { kind: stderr_kind(l), text: format!("  {l}") })
                                .collect();
                            if !lines.is_empty() {
                                self.send(Event::Lines(id, lines));
                            }
                            if out.status.success() { Ok(()) } else { Err(format!("{program}: {}", out.status)) }
                        }
                        Err(e) => Err(format!("{program}: {e}")),
                    }
                }
                Step::Package { exes, version, out } => {
                    self.note(id, format!("› package → {}", out.display()));
                    crate::release::package(exes, version, out).map(|files| {
                        for file in files {
                            self.note(id, format!("  {}", file.display()));
                        }
                    })
                }
                Step::Publish { repo, tag, notes, prerelease, out } => {
                    self.note(id, format!("› GitHub Release {repo} {tag}"));
                    let files: Vec<PathBuf> = std::fs::read_dir(out)
                        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
                        .unwrap_or_default();
                    crate::release::publish(repo, tag, notes, *prerelease, &files).map(|url| {
                        self.note(id, format!("  {url}"));
                        outcome.published = Some(url);
                    })
                }
                Step::Snapshot(files) => {
                    for path in files {
                        saved.push((path.clone(), std::fs::read(path).ok()));
                    }
                    Ok(())
                }
            };
            if let Err(e) = result {
                self.note(id, format!("› {e}"));
                for (path, bytes) in &saved {
                    let restored = match bytes {
                        Some(bytes) => std::fs::write(path, bytes),
                        None => std::fs::remove_file(path),
                    };
                    match restored {
                        Ok(()) => self.note(id, format!("› restored {}", path.display())),
                        Err(e) => self.note(id, format!("› {}: {e}", path.display())),
                    }
                }
                self.note(id, format!("› failed ({:.1} s)", started.elapsed().as_secs_f32()));
                return outcome;
            }
        }
        self.send(Event::Units(id, steps.len() as u32));
        self.note(id, format!("› ok ({:.1} s)", started.elapsed().as_secs_f32()));
        outcome.ok = true;
        outcome
    }

    /// Скачать выпуск и поставить — ход в килобайтах идёт в полосу, как единицы сборки.
    fn download(&self, id: JobId, d: &Download) -> Outcome {
        let mut outcome = Outcome::default();
        let result = (|| {
            let http = reqwest::blocking::Client::builder()
                .user_agent(concat!("anvil/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(120))
                .build()
                .map_err(|e| e.to_string())?;
            // Токен — тот же, что у потока GitHub; через окно он не проходит.
            let (token, _) = crate::github::find_token();
            let token = token.as_deref();
            let (asset_url, sums_url) =
                if token.is_some() { (&d.asset_api, &d.sums_api) } else { (&d.asset_url, &d.sums_url) };
            let sums = anvil_update::install::fetch_text(&http, Source { url: sums_url, token })?;
            let expected = anvil_update::expected_sum(&sums, &d.asset)
                .ok_or_else(|| format!("{} is not listed in SHA256SUMS", d.asset))?;
            self.note(id, format!("› SHA256SUMS: {expected}"));
            let root = installs::root(&d.bin);
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let layout = anvil_update::Layout::Managed { root: root.clone() };
            let exe_name = format!("{}{}", d.bin, std::env::consts::EXE_SUFFIX);
            let mut last = Instant::now();
            let progress = |done: u64, _total: u64| {
                if last.elapsed() >= Duration::from_millis(150) {
                    last = Instant::now();
                    self.send(Event::Units(id, (done / 1024) as u32));
                }
            };
            let source = Source { url: asset_url, token };
            let exe = anvil_update::install::install_archive(
                &http,
                &layout,
                source,
                &d.asset,
                &expected,
                &d.version,
                std::ffi::OsStr::new(&exe_name),
                progress,
            )?;
            self.note(id, format!("› {}", exe.display()));
            installs::create_shortcut(&d.bin).map(|lnk| self.note(id, format!("› {}", lnk.display())))?;
            Ok(d.version.clone())
        })();
        match &result {
            Ok(version) => self.note(id, format!("› {} {version} → current", d.bin)),
            Err(e) => self.note(id, format!("› {e}")),
        }
        outcome.ok = result.is_ok();
        outcome.installed = Some(result);
        outcome
    }

    fn stdout_line(&self, id: JobId, spec: &Spec, text: String, lines: &mut Vec<Line>, outcome: &mut Outcome) {
        if spec.json && text.starts_with('{') {
            match parse_message(&text) {
                Some(Message::Unit) => {
                    outcome.units += 1;
                    self.send(Event::Units(id, outcome.units));
                }
                Some(Message::Diagnostic { diag, rendered }) => {
                    let mut rendered = rendered.lines();
                    if let Some(first) = rendered.next() {
                        let kind = match diag.as_ref().map(|d| d.level) {
                            Some(Level::Error) => LineKind::Error,
                            Some(Level::Warning) => LineKind::Warning,
                            None if first.starts_with("error") => LineKind::Error,
                            None if first.starts_with("warning") => LineKind::Warning,
                            None => LineKind::Text,
                        };
                        lines.push(Line { kind, text: first.to_owned() });
                    }
                    lines.extend(rendered.map(|l| Line { kind: LineKind::Text, text: l.to_owned() }));
                    if let Some(diag) = diag {
                        self.send(Event::Diag(id, diag));
                    }
                }
                Some(Message::Other) => {}
                None => lines.push(Line { kind: LineKind::Text, text }),
            }
            return;
        }
        if let Some((passed, failed)) = test_result(&text) {
            let (p, f) = outcome.tests.unwrap_or((0, 0));
            outcome.tests = Some((p + passed, f + failed));
        }
        lines.push(Line { kind: LineKind::Text, text });
    }
}

/// Поставить свежесобранный exe: копия во временную папку → `versions\<метка>` → `current` → ярлык.
fn install_local(step: &InstallStep) -> Result<String, String> {
    let root = installs::root(&step.bin);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let staged = installs::stage_local(&step.exe, &root, &step.label)?;
    if let Err(e) = anvil_update::install::install_version(&staged, &root, &step.label) {
        anvil_update::install::remove_dir_later(staged);
        return Err(e);
    }
    installs::create_shortcut(&step.bin)?;
    Ok(step.label.clone())
}

/// Строки cargo в stderr: «error…» и «warning…» подсвечиваются, остальное — обычный текст.
fn stderr_kind(text: &str) -> LineKind {
    if text.starts_with("error") {
        LineKind::Error
    } else if text.starts_with("warning") {
        LineKind::Warning
    } else {
        LineKind::Text
    }
}

/// Читать поток построчно в отдельном потоке. Невалидный UTF-8 не рвёт чтение.
fn read_lines(pipe: Option<impl Read + Send + 'static>, tx: Sender<Output>, wrap: fn(String) -> Output) {
    let Some(pipe) = pipe else {
        let _ = tx.send(Output::Closed);
        return;
    };
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_owned();
                    if tx.send(wrap(text)).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = tx.send(Output::Closed);
    });
}

/// Остановить cargo вместе с rustc и компоновщиком, которых он запустил.
fn kill_tree(child: &mut Child, dir: &Path) {
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let killed = run::command("taskkill", dir).args(["/T", "/F", "/PID", &pid]).output();
        if killed.is_ok_and(|o| o.status.success()) {
            return;
        }
    }
    let _ = dir;
    let _ = child.kill();
}

enum Message {
    /// Готова единица сборки (крейт или запуск build.rs) — шаг полосы хода.
    Unit,
    Diagnostic {
        diag: Option<Diag>,
        rendered: String,
    },
    Other,
}

fn parse_message(text: &str) -> Option<Message> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let reason = value.get("reason")?.as_str()?;
    Some(match reason {
        "compiler-artifact" | "build-script-executed" => Message::Unit,
        "compiler-message" => {
            let message = value.get("message")?;
            let rendered = message.get("rendered").and_then(|r| r.as_str()).unwrap_or_default().to_owned();
            let level = match message.get("level").and_then(|l| l.as_str()) {
                Some("error" | "error: internal compiler error") => Some(Level::Error),
                Some("warning") => Some(Level::Warning),
                _ => None,
            };
            let place = message.get("spans").and_then(|s| s.as_array()).and_then(|spans| {
                let span = spans.iter().find(|s| s.get("is_primary").and_then(|p| p.as_bool()) == Some(true))?;
                Some((
                    PathBuf::from(span.get("file_name")?.as_str()?),
                    span.get("line_start")?.as_u64()? as u32,
                    span.get("column_start")?.as_u64()? as u32,
                ))
            });
            // «N warnings emitted» и прочие итоги без места в коде — только в лог.
            let diag = match (level, place) {
                (Some(level), Some(place)) => Some(Diag {
                    level,
                    message: message.get("message").and_then(|m| m.as_str()).unwrap_or_default().to_owned(),
                    place: Some(place),
                }),
                _ => None,
            };
            Message::Diagnostic { diag, rendered }
        }
        _ => Message::Other,
    })
}

/// `test result: ok. 12 passed; 0 failed; …` → (12, 0).
fn test_result(line: &str) -> Option<(u32, u32)> {
    let (_, counts) = line.strip_prefix("test result: ")?.split_once(". ")?;
    let (mut passed, mut failed) = (None, 0);
    for part in counts.split(';') {
        let mut words = part.split_whitespace();
        let (Some(n), Some(word)) = (words.next().and_then(|n| n.parse().ok()), words.next()) else { continue };
        match word {
            "passed" => passed = Some(n),
            "failed" => failed = n,
            _ => {}
        }
    }
    Some((passed?, failed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_lines() {
        assert_eq!(test_result("test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured"), Some((12, 0)));
        assert_eq!(test_result("test result: FAILED. 3 passed; 2 failed; 0 ignored"), Some((3, 2)));
        assert_eq!(test_result("running 3 tests"), None);
    }

    #[test]
    fn compiler_message_with_place() {
        let text = r#"{"reason":"compiler-message","message":{"rendered":"error[E0599]: no method\n --> src/ui/project.rs:368:57\n","level":"error","message":"no method named `x`","spans":[{"file_name":"src\\ui\\project.rs","line_start":368,"column_start":57,"is_primary":true}]}}"#;
        let Some(Message::Diagnostic { diag: Some(diag), rendered }) = parse_message(text) else {
            panic!("не разобрано")
        };
        assert_eq!(diag.level, Level::Error);
        assert_eq!(diag.place, Some((PathBuf::from("src\\ui\\project.rs"), 368, 57)));
        assert!(rendered.starts_with("error[E0599]"));
    }

    #[test]
    fn summary_message_has_no_place() {
        let text = r#"{"reason":"compiler-message","message":{"rendered":"warning: 2 warnings emitted\n","level":"warning","message":"2 warnings emitted","spans":[]}}"#;
        assert!(matches!(parse_message(text), Some(Message::Diagnostic { diag: None, .. })));
    }

    #[test]
    fn artifacts_count_as_units() {
        assert!(matches!(parse_message(r#"{"reason":"compiler-artifact","fresh":true}"#), Some(Message::Unit)));
        assert!(matches!(parse_message(r#"{"reason":"build-finished","success":true}"#), Some(Message::Other)));
        assert!(parse_message("not json").is_none());
    }
}
