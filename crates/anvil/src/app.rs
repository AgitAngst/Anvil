//! Состояние окна: проекты, выбор, настройки, связь с фоновым потоком.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use anvil_ui::widgets::Toasts;
use anvil_ui::{Accent, Tone};
use eframe::egui;

use crate::config::{self, Config};
use crate::i18n::{self, t};
use crate::procs::{self, Running, Snapshot};
use crate::registry;
use crate::worker::{self, Busy, Cmd, Event, Project};

pub const ACCENT: Accent = Accent::EMBER;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Commits,
    Changes,
    Notes,
}

pub struct App {
    pub config: Config,
    pub config_path: PathBuf,
    pub projects: Vec<Project>,
    pub selected: Option<PathBuf>,
    pub procs: Snapshot,
    pub busy: Option<Busy>,
    /// Первый поиск ещё идёт: список пуст не потому, что проектов нет.
    pub scanning: bool,
    pub refreshed_at: Option<i64>,
    pub search: String,
    pub tab: Tab,
    pub toasts: Toasts,
    pub settings_open: bool,
    pub about_open: bool,
    commands: Sender<Cmd>,
    events: Receiver<Event>,
    last_fetch: Instant,
    was_focused: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let config_path = config::path();
        let (config, config_error) = config::load(&config_path);
        anvil_ui::install(&cc.egui_ctx, ACCENT, config.common.theme);
        config.common.apply(&cc.egui_ctx);
        i18n::set(&cc.egui_ctx, config.common.language);

        let (commands, events) = worker::spawn(cc.egui_ctx.clone());
        let mut app = Self {
            selected: config.selected.clone(),
            config,
            config_path,
            projects: Vec::new(),
            procs: Snapshot::new(),
            busy: None,
            scanning: true,
            refreshed_at: None,
            search: String::new(),
            tab: Tab::Commits,
            toasts: Toasts::default(),
            settings_open: false,
            about_open: false,
            commands,
            events,
            last_fetch: Instant::now(),
            was_focused: true,
        };
        if let Some(error) = config_error {
            app.toasts.push(format!("{}: {error}", t("Настройки не прочитаны")), Tone::Danger);
        }
        app.rescan();
        app
    }

    pub fn rescan(&mut self) {
        self.scanning = true;
        let _ = self.commands.send(Cmd::Rescan(self.config.roots.clone()));
    }

    pub fn refresh(&mut self) {
        let _ = self.commands.send(Cmd::Refresh { full: true });
    }

    pub fn fetch(&mut self) {
        self.last_fetch = Instant::now();
        let _ = self.commands.send(Cmd::Fetch);
    }

    pub fn save(&mut self) {
        if let Err(e) = config::save(&self.config_path, &self.config) {
            self.toasts.push(format!("{}: {e}", t("Настройки не сохранены")), Tone::Danger);
        }
    }

    /// Разобрать события фонового потока и поставить плановые дела.
    pub fn tick(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            self.apply(event);
        }

        // Вернулись в окно — перечитать: пока нас не было, могли закоммитить.
        let focused = ctx.input(|i| i.focused);
        if focused && !self.was_focused {
            let _ = self.commands.send(Cmd::Refresh { full: false });
        }
        self.was_focused = focused;

        let minutes = self.config.fetch_minutes;
        if minutes > 0 && self.last_fetch.elapsed() >= Duration::from_secs(u64::from(minutes) * 60) {
            self.fetch();
        }
        // Относительное время («5 мин назад») должно идти и без событий.
        ctx.request_repaint_after(Duration::from_secs(30));
    }

    fn apply(&mut self, event: Event) {
        match event {
            Event::Found(paths) => {
                self.projects.retain(|p| paths.contains(&p.path));
                for path in paths {
                    if !self.projects.iter().any(|p| p.path == path) {
                        self.projects.push(Project { path, meta: None, git: Ok(None), notes: Vec::new() });
                    }
                }
                self.scanning = false;
            }
            Event::Project(update) => {
                if let Some(project) = self.projects.iter_mut().find(|p| p.path == update.path) {
                    project.merge(*update);
                }
                self.refreshed_at = Some(i18n::now());
            }
            Event::Procs(snapshot) => self.procs = snapshot,
            Event::Busy(busy) => self.busy = busy,
            Event::Fetched { failed, first_error } => {
                if let Some(error) = first_error {
                    let head = if failed == 1 {
                        t("origin не ответил").to_owned()
                    } else {
                        format!("{} {}", t("origin не ответил у"), i18n::count(failed, PROJECTS_RU, PROJECTS_EN))
                    };
                    self.toasts.push(format!("{head} — {error}"), Tone::Warning);
                }
            }
        }
    }

    /// Проекты для списка: без скрытых, по фильтру поиска, свежие сверху.
    pub fn visible(&self) -> Vec<&Project> {
        let needle = self.search.trim().to_lowercase();
        let mut list: Vec<&Project> = self
            .projects
            .iter()
            .filter(|p| !self.is_hidden(&p.path))
            .filter(|p| needle.is_empty() || p.name().to_lowercase().contains(&needle))
            .collect();
        list.sort_by_key(|p| std::cmp::Reverse(p.git().map_or(0, |g| g.last_commit_time())));
        list
    }

    pub fn is_hidden(&self, path: &Path) -> bool {
        self.config.hidden.iter().any(|h| registry::same_dir(h, path))
    }

    pub fn current(&self) -> Option<&Project> {
        let visible = self.visible();
        let wanted = self.selected.as_ref();
        wanted.and_then(|s| visible.iter().copied().find(|p| &p.path == s)).or_else(|| visible.first().copied())
    }

    pub fn select(&mut self, path: PathBuf) {
        if self.selected.as_ref() != Some(&path) {
            self.selected = Some(path.clone());
            self.config.selected = Some(path);
            self.save();
        }
    }

    pub fn hide(&mut self, path: &Path) {
        self.config.hidden.push(path.to_path_buf());
        self.save();
        self.toasts.push(format!("{} {}", worker::display_name(path), t("скрыт из списка")), Tone::Neutral);
    }

    /// Запущенные экземпляры бинарника.
    pub fn running(&self, bin: &str) -> &[Running] {
        self.procs.get(&procs::key(bin)).map_or(&[], Vec::as_slice)
    }

    pub fn report(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.toasts.push(e, Tone::Danger);
        }
    }
}

pub const PROJECTS_RU: [&str; 3] = ["проекта", "проектов", "проектов"];
pub const PROJECTS_EN: [&str; 2] = ["project", "projects"];
