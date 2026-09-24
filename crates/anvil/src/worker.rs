//! Фоновый поток: ищет проекты, читает git и cargo, следит за процессами.
//! Окно никогда не ждёт внешних команд — только получает готовые события.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::git::{self, GitState};
use crate::procs::{self, Snapshot};
use crate::registry::{self, Kind, Meta};

/// Всё, что известно о проекте.
#[derive(Debug, Clone)]
pub struct Project {
    pub path: PathBuf,
    pub kind: Kind,
    /// Сведения cargo; `None` — ещё не прочитаны (или в этом обновлении не перечитывались).
    pub meta: Option<Result<Meta, String>>,
    /// `Ok(None)` — папка не под git.
    pub git: Result<Option<GitState>, String>,
    /// Заметки проекта и время их правки.
    pub notes: Vec<(String, i64)>,
}

impl Project {
    pub fn name(&self) -> String {
        display_name(&self.path)
    }

    pub fn git(&self) -> Option<&GitState> {
        self.git.as_ref().ok().and_then(Option::as_ref)
    }

    pub fn meta(&self) -> Option<&Meta> {
        self.meta.as_ref()?.as_ref().ok()
    }

    /// Принять свежие сведения; то, что не перечитывалось, остаётся прежним.
    pub fn merge(&mut self, update: Project) {
        if update.meta.is_some() {
            self.meta = update.meta;
        }
        self.git = update.git;
        self.notes = update.notes;
    }
}

/// Имя проекта — имя папки с заглавной буквы: `amber` → `Amber`, `FFMincer` как есть.
pub fn display_name(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut chars = name.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or(name)
}

pub enum Cmd {
    /// Заново найти проекты включённых видов в этих корнях и прочитать всё.
    Rescan(Vec<PathBuf>, Vec<String>),
    /// Перечитать git и заметки у всех (cargo — только если `full`).
    Refresh { full: bool },
    /// Спросить origin у всех проектов, потом перечитать.
    Fetch,
}

pub enum Event {
    /// Найденные проекты, в порядке поиска. Пришедших раньше, но пропавших — убрать.
    Found(Vec<(PathBuf, Kind)>),
    Project(Box<Project>),
    Procs(Snapshot),
    /// Чем поток сейчас занят; `None` — ничем.
    Busy(Option<Busy>),
    /// Итог опроса origin: сколько проектов не ответило и первая ошибка.
    Fetched {
        failed: usize,
        first_error: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Scanning,
    Refreshing,
    Fetching,
}

const PROCS_EVERY: Duration = Duration::from_secs(3);
const GIT_EVERY: Duration = Duration::from_secs(20);

pub fn spawn(ctx: egui::Context) -> (Sender<Cmd>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("anvil-worker".into())
        .spawn(move || Worker { ctx, events: event_tx, found: Vec::new() }.run(cmd_rx))
        .expect("spawn worker");
    (cmd_tx, event_rx)
}

struct Worker {
    ctx: egui::Context,
    events: Sender<Event>,
    found: Vec<(PathBuf, Kind)>,
}

impl Worker {
    fn send(&self, event: Event) {
        let _ = self.events.send(event);
        self.ctx.request_repaint();
    }

    fn run(mut self, commands: Receiver<Cmd>) {
        let mut last_procs = Instant::now() - PROCS_EVERY;
        let mut last_git = Instant::now();
        loop {
            if last_procs.elapsed() >= PROCS_EVERY {
                self.send(Event::Procs(procs::snapshot()));
                last_procs = Instant::now();
            }
            if last_git.elapsed() >= GIT_EVERY && !self.found.is_empty() {
                self.refresh(false, false);
                last_git = Instant::now();
            }
            match commands.recv_timeout(PROCS_EVERY.saturating_sub(last_procs.elapsed())) {
                Ok(Cmd::Rescan(roots, kinds)) => {
                    self.send(Event::Busy(Some(Busy::Scanning)));
                    self.found = registry::scan(&roots, &kinds);
                    self.send(Event::Found(self.found.clone()));
                    self.refresh(true, true);
                    last_git = Instant::now();
                }
                Ok(Cmd::Refresh { full }) => {
                    self.refresh(full, true);
                    last_git = Instant::now();
                }
                Ok(Cmd::Fetch) => {
                    self.fetch();
                    last_git = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Прочитать все проекты параллельно. `show` — показывать ли занятость (фоновое тихое обновление — нет).
    fn refresh(&self, full: bool, show: bool) {
        if show {
            self.send(Event::Busy(Some(Busy::Refreshing)));
        }
        std::thread::scope(|scope| {
            for (path, kind) in &self.found {
                scope.spawn(move || {
                    let project = Project {
                        path: path.clone(),
                        kind: *kind,
                        // cargo — только у проектов на Rust.
                        meta: (full && *kind == Kind::Rust).then(|| registry::meta(path)),
                        git: git::read(path),
                        notes: registry::notes(path),
                    };
                    self.send(Event::Project(Box::new(project)));
                });
            }
        });
        if show {
            self.send(Event::Busy(None));
        }
    }

    fn fetch(&self) {
        self.send(Event::Busy(Some(Busy::Fetching)));
        let results: Vec<Result<(), String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = self
                .found
                .iter()
                .map(|(p, _)| p)
                .filter(|p| p.join(".git").exists())
                .map(|path| scope.spawn(move || git::fetch(path).map_err(|e| format!("{}: {e}", display_name(path)))))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("fetch panicked".into()))).collect()
        });
        let errors: Vec<String> = results.into_iter().filter_map(Result::err).collect();
        self.send(Event::Fetched { failed: errors.len(), first_error: errors.into_iter().next() });
        self.refresh(false, true);
    }
}
