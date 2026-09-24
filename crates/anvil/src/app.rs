//! Состояние окна: проекты, выбор, настройки, связь с фоновым потоком.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use anvil_ui::widgets::Toasts;
use anvil_ui::{Accent, Tone};
use eframe::egui;

use crate::config::{self, Config};
use crate::deps;
use crate::github::{self, Auth, Release, Remotes, Target};
use crate::i18n::{self, t};
use crate::installs::{self, Installed};
use crate::jobs::{self, JobId};
use crate::procs::{self, Running, Snapshot};
use crate::registry;
use crate::tasks::{self, Job, Locked, Resolve, Task, UnitsCache};
use crate::worker::{self, Busy, Cmd, Event, Project};

pub const ACCENT: Accent = Accent::EMBER;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Commits,
    Changes,
    Ci,
    Releases,
    Install,
    Deps,
    Notes,
}

/// Что в середине окна: карточка выбранного проекта или обзор всех.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Project,
    Overview,
}

/// Что сделать, когда задача закончится.
enum After {
    /// Перепроверить зависимости проекта: `Cargo.lock` мог поменяться.
    Deps(PathBuf),
    /// Перечитать тулчейн: Rust обновился.
    Toolchain,
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
    pub view: View,
    /// Палитра `Ctrl+K`, если открыта.
    pub palette: Option<crate::ui::palette::State>,
    pub tab: Tab,
    pub toasts: Toasts,
    pub settings_open: bool,
    pub about_open: bool,
    /// Обновления самого Anvil.
    pub updater: anvil_update::Updater,
    /// Установленные копии бинарников (`%LOCALAPPDATA%\Programs`); `None` — не установлен.
    pub installs: HashMap<String, Option<Installed>>,
    /// Подтверждение удаления установки бинарника.
    pub uninstall_confirm: Option<String>,
    /// Мастер выпуска, если открыт.
    pub release: Option<crate::ui::release::Wizard>,
    /// Слежение за выпусками по тегу: проект → как идёт.
    pub watches: HashMap<PathBuf, github::Watch>,
    /// Задачи по порядку постановки: новые — в конце.
    pub jobs: Vec<Job>,
    pub log_open: bool,
    pub log_job: Option<JobId>,
    /// Сборке мешает запущенная программа: ждём выбора пользователя.
    /// Последнее — профиль сборки (release ли).
    pub locked: Option<(jobs::Spec, Vec<Locked>, Task, bool)>,
    /// Подтверждение остановки программы: имя, PID, папка проекта.
    pub stop_confirm: Option<(String, u32, PathBuf)>,
    /// Подтверждение `cargo clean` для проекта.
    pub clean_confirm: Option<PathBuf>,
    /// Окно пресетов запуска открыто для этого проекта.
    pub presets_for: Option<PathBuf>,
    /// CI и выпуски проектов на GitHub.
    pub remotes: Remotes,
    pub gh_auth: Option<Auth>,
    /// Зависимости проектов: итог последней проверки.
    pub deps: HashMap<PathBuf, deps::Report>,
    /// Какой проект сейчас проверяется.
    pub deps_busy: Option<PathBuf>,
    pub toolchain: Option<deps::Toolchain>,
    /// Ждёт подтверждения: обновить зависимости, поднять набор, обновить Rust.
    pub deps_ask: Option<crate::ui::deps::Ask>,
    /// Окно «Rust и зависимости».
    pub overview_open: bool,
    /// Сводка amber-admin о серверах Amber.
    pub amber: crate::amber::Watch,
    deps_commands: Sender<deps::Cmd>,
    deps_events: Receiver<deps::Event>,
    after_job: HashMap<JobId, After>,
    /// Поле «свой токен» в настройках (не хранится нигде, кроме keyring после «Сохранить»).
    pub token_input: String,
    gh_commands: Sender<github::Cmd>,
    gh_events: Receiver<github::Event>,
    gh_targets: Vec<Target>,
    job_commands: Sender<jobs::Cmd>,
    job_events: Receiver<jobs::Event>,
    job_notes: Sender<jobs::Event>,
    /// Уведомления о конце долгих задач: решает поток задач, окно сообщает настройку.
    pub notifier: Arc<crate::notify::Notifier>,
    next_job: JobId,
    units: UnitsCache,
    units_path: PathBuf,
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
        let notifier = Arc::new(crate::notify::Notifier::new(config.notify, config_path.with_file_name("cache")));
        let (job_commands, job_events, job_notes) = jobs::spawn(cc.egui_ctx.clone(), notifier.clone());
        let (gh_commands, gh_events) = github::spawn(cc.egui_ctx.clone(), config_path.with_file_name("cache"));
        let units_path = config_path.with_file_name("units.json");
        let (deps_commands, deps_events) = deps::spawn(cc.egui_ctx.clone(), config_path.with_file_name("cache"));
        let updater = anvil_update::Updater::new(
            anvil_update::Config::new("anvil", env!("CARGO_PKG_VERSION"), "AgitAngst/Anvil"),
            cc.egui_ctx.clone(),
        );
        let mut app = Self {
            selected: config.selected.clone(),
            config,
            config_path,
            projects: Vec::new(),
            procs: Snapshot::new(),
            busy: None,
            scanning: true,
            refreshed_at: None,
            view: View::Project,
            palette: None,
            tab: Tab::Commits,
            toasts: Toasts::default(),
            settings_open: false,
            about_open: false,
            updater,
            installs: HashMap::new(),
            uninstall_confirm: None,
            release: None,
            watches: HashMap::new(),
            jobs: Vec::new(),
            log_open: false,
            log_job: None,
            locked: None,
            stop_confirm: None,
            clean_confirm: None,
            presets_for: None,
            remotes: Remotes::new(),
            gh_auth: None,
            deps: HashMap::new(),
            deps_busy: None,
            toolchain: None,
            deps_ask: None,
            overview_open: false,
            amber: crate::amber::Watch::default(),
            deps_commands,
            deps_events,
            after_job: HashMap::new(),
            token_input: String::new(),
            gh_commands,
            gh_events,
            gh_targets: Vec::new(),
            job_commands,
            job_events,
            job_notes,
            notifier,
            next_job: 1,
            units: tasks::load_units(&units_path),
            units_path,
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
        let _ = self.commands.send(Cmd::Rescan(self.config.roots.clone(), self.config.kinds.clone()));
    }

    pub fn refresh(&mut self) {
        let _ = self.commands.send(Cmd::Refresh { full: true });
        let _ = self.gh_commands.send(github::Cmd::Refresh);
    }

    /// Сохранить свой токен GitHub (`None` — забыть).
    pub fn set_token(&mut self, token: Option<String>) {
        self.gh_auth = None;
        let _ = self.gh_commands.send(github::Cmd::SetToken(token));
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
        while let Ok(event) = self.job_events.try_recv() {
            self.apply_job(ctx, event);
        }
        while let Ok(event) = self.deps_events.try_recv() {
            match event {
                deps::Event::Report(path, report) => {
                    self.deps.insert(path, *report);
                }
                deps::Event::Toolchain(toolchain) => self.toolchain = Some(toolchain),
                deps::Event::Busy(path) => self.deps_busy = path,
            }
        }
        while let Ok(event) = self.gh_events.try_recv() {
            match event {
                github::Event::Remote(path, remote) => {
                    self.remotes.insert(path, *remote);
                }
                github::Event::Auth(auth) => self.gh_auth = Some(auth),
                github::Event::Watch(path, watch) => {
                    self.watches.insert(path, *watch);
                }
                github::Event::TokenSaved(Ok(())) => self.toasts.push(t("Токен GitHub обновлён"), Tone::Success),
                github::Event::TokenSaved(Err(e)) => {
                    self.toasts.push(format!("{}: {e}", t("Токен не сохранён")), Tone::Danger)
                }
            }
        }
        // Проекты на GitHub изменились (нашлись, сменили ветку) — сказать потоку GitHub.
        let targets = github::targets(&self.projects, &self.config);
        if targets != self.gh_targets {
            let _ = self.gh_commands.send(github::Cmd::Targets(targets.clone()));
            self.gh_targets = targets;
        }
        if self.jobs.iter().any(Job::running) {
            // Время задачи в строке состояния идёт каждую секунду.
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        // Вернулись в окно — перечитать: пока нас не было, могли закоммитить.
        let focused = ctx.input(|i| i.focused);
        if focused && !self.was_focused {
            let _ = self.commands.send(Cmd::Refresh { full: false });
        }
        self.was_focused = focused;

        self.updater.auto(&self.config.common);

        let minutes = self.config.fetch_minutes;
        if minutes > 0 && self.last_fetch.elapsed() >= Duration::from_secs(u64::from(minutes) * 60) {
            self.fetch();
        }
        // Относительное время («5 мин назад») должно идти и без событий.
        ctx.request_repaint_after(Duration::from_secs(30));
    }

    fn apply(&mut self, event: Event) {
        match event {
            Event::Found(found) => {
                // Зависимости: сразу — прошлое знание из кеша, в фоне — проверка устаревшего.
                let rust: Vec<PathBuf> =
                    found.iter().filter(|(_, k)| *k == registry::Kind::Rust).map(|(p, _)| p.clone()).collect();
                self.check_deps(rust, false);
                self.check_toolchain(false);
                self.projects.retain(|p| found.iter().any(|(path, _)| *path == p.path));
                for (path, kind) in found {
                    if !self.projects.iter().any(|p| p.path == path) {
                        self.projects.push(Project { path, kind, meta: None, git: Ok(None), notes: Vec::new() });
                    }
                }
                self.scanning = false;
            }
            Event::Project(update) => {
                let path = update.path.clone();
                if let Some(project) = self.projects.iter_mut().find(|p| p.path == update.path) {
                    project.merge(*update);
                }
                self.refresh_installs(&path);
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

    /// Проекты для списка: без скрытых, свежие сверху.
    pub fn visible(&self) -> Vec<&Project> {
        let mut list: Vec<&Project> = self.projects.iter().filter(|p| !self.is_hidden(&p.path)).collect();
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

    /// Обзор ↔ карточка проекта (`Ctrl+0`).
    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            View::Project => View::Overview,
            View::Overview => View::Project,
        };
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

    // ─── Задачи ────────────────────────────────────────────────────────────────

    /// Попросить задачу у проекта. Если сборке мешает запущенная программа — спросить, как быть.
    pub fn start_task(&mut self, path: &Path, task: Task) -> Option<JobId> {
        let release = self.config.project(path).release;
        self.start_task_as(path, task, release)
    }

    /// То же, но с явным профилем: «Собрать release» из палитры не трогает переключатель проекта.
    pub fn start_task_as(&mut self, path: &Path, task: Task, release: bool) -> Option<JobId> {
        let project = self.projects.iter().find(|p| p.path == path)?;
        let meta = project.meta().cloned();
        let spec = tasks::spec(path, meta.as_ref(), &task, release, self.config.build_jobs);
        let locked = tasks::locked(meta.as_ref(), &task, release, &self.procs);
        if locked.is_empty() {
            Some(self.enqueue(spec))
        } else {
            self.locked = Some((spec, locked, task, release));
            None
        }
    }

    /// Пользователь выбрал, как обойти занятый exe (`None` — передумал).
    pub fn resolve_locked(&mut self, how: Option<Resolve>) {
        let Some((mut spec, locked, task, release)) = self.locked.take() else { return };
        let Some(how) = how else { return };
        let meta = self.projects.iter().find(|p| p.path == spec.project).and_then(|p| p.meta()).cloned();
        let release = !matches!(task, Task::Test) && release;
        tasks::resolve(&mut spec, &locked, how, meta.as_ref(), release);
        self.enqueue(spec);
    }

    pub fn enqueue(&mut self, mut spec: jobs::Spec) -> JobId {
        let key = tasks::units_key(&spec);
        spec.expected_units = self.units.get(&key).copied();
        // Отодвинутые раньше exe, которые уже никто не держит, — убрать.
        if let Some(after) = &spec.after
            && let Some(dir) = after.exe.parent()
        {
            crate::launch::clean_moved(dir);
        }
        let id = self.next_job;
        self.next_job += 1;
        self.jobs.push(Job {
            id,
            project: spec.project.clone(),
            spec: spec.clone(),
            lines: Vec::new(),
            diags: Vec::new(),
            units: 0,
            started: None,
            finished: None,
            units_key: key,
        });
        // Хранить последние 30 задач: логи больших сборок весят заметно.
        let excess = self.jobs.len().saturating_sub(30);
        if excess > 0 {
            let old: Vec<JobId> =
                self.jobs.iter().filter(|j| j.finished.is_some()).take(excess).map(|j| j.id).collect();
            self.jobs.retain(|j| !old.contains(&j.id));
        }
        let showing_running = self.jobs.iter().any(|j| Some(j.id) == self.log_job && j.running());
        if !showing_running {
            self.log_job = Some(id);
        }
        let _ = self.job_commands.send(jobs::Cmd::Run(id, Box::new(spec)));
        id
    }

    /// Следить за выпуском по тегу на GitHub.
    pub fn watch_release(&mut self, path: &Path, repo: github::Repo, tag: String) {
        self.watches.remove(path);
        let releases = self.gh_targets.iter().find(|t| t.path == path).and_then(|t| t.releases.clone());
        let _ = self.gh_commands.send(github::Cmd::Watch { path: path.to_path_buf(), repo, releases, tag });
    }

    pub fn cancel(&mut self, id: JobId) {
        let _ = self.job_commands.send(jobs::Cmd::Cancel(id));
    }

    /// Остановить запущенную программу (после подтверждения) — в фоне, итог придёт уведомлением.
    pub fn stop_program(&mut self, name: String, pid: u32, dir: PathBuf) {
        let notes = self.job_notes.clone();
        std::thread::spawn(move || {
            let event = match crate::launch::stop(pid, &dir) {
                Ok(()) => jobs::Event::Note(format!("{name} {}", t("остановлен")), true),
                Err(e) => jobs::Event::Note(format!("{name}: {e}"), false),
            };
            let _ = notes.send(event);
        });
    }

    fn job_mut(&mut self, id: JobId) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    fn apply_job(&mut self, ctx: &egui::Context, event: jobs::Event) {
        match event {
            jobs::Event::Started(id) => {
                if let Some(job) = self.job_mut(id) {
                    job.started = Some(Instant::now());
                }
                self.log_job = Some(id);
            }
            jobs::Event::Lines(id, lines) => {
                if let Some(job) = self.job_mut(id) {
                    job.lines.extend(lines);
                }
            }
            jobs::Event::Units(id, units) => {
                if let Some(job) = self.job_mut(id) {
                    job.units = units;
                }
            }
            jobs::Event::Diag(id, diag) => {
                if let Some(job) = self.job_mut(id) {
                    job.diags.push(diag);
                }
            }
            jobs::Event::Finished(id, outcome) => self.finished(ctx, id, outcome),
            jobs::Event::Note(text, ok) => self.toasts.push(text, if ok { Tone::Success } else { Tone::Danger }),
        }
    }

    fn finished(&mut self, ctx: &egui::Context, id: JobId, outcome: jobs::Outcome) {
        match self.after_job.remove(&id) {
            Some(After::Deps(path)) => self.check_deps(vec![path], true),
            Some(After::Toolchain) => self.check_toolchain(true),
            None => {}
        }
        let Some(job) = self.job_mut(id) else { return };
        let took = job.started.map_or(Duration::ZERO, |s| s.elapsed());
        let was_started = job.started.is_some();
        job.finished = Some((outcome.clone(), took));
        let name = worker::display_name(&job.project);
        let (key, project) = (job.units_key.clone(), job.project.clone());
        if outcome.installed.is_some() {
            self.refresh_installs(&project);
        }
        if outcome.ok && outcome.units > 0 {
            self.units.insert(key, outcome.units);
            tasks::save_units(&self.units_path, &self.units);
        }
        if !was_started {
            return;
        }
        let (text, tone) = tasks::summary(&outcome, took);
        self.toasts.push(format!("{name}: {text}"), tone);
        match &outcome.installed {
            Some(Ok(version)) => self.toasts.push(format!("{name}: {} {version}", t("установлена")), Tone::Success),
            Some(Err(e)) => self.toasts.push(format!("{name}: {e}"), Tone::Danger),
            None => {}
        }
        if let Some(Err(e)) = &outcome.launched {
            self.toasts.push(format!("{}: {e}", t("Не запустилось")), Tone::Danger);
        }
        // Окно не в фокусе — мигнуть на панели задач: дело сделано (уведомление о долгой задаче
        // уже послал поток задач).
        if !ctx.input(|i| i.focused) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
        }
    }

    // ─── Установка ─────────────────────────────────────────────────────────────

    /// Перечитать, что установлено для бинарников проекта.
    pub fn refresh_installs(&mut self, path: &Path) {
        let bins: Vec<String> = self
            .projects
            .iter()
            .find(|p| p.path == path)
            .and_then(|p| p.meta())
            .map(|m| m.bins.iter().map(|b| b.name.clone()).collect())
            .unwrap_or_default();
        for bin in bins {
            self.installs.insert(bin.clone(), installs::scan(&bin));
        }
    }

    /// Собрать release и поставить с меткой `версия-хеш`.
    pub fn install_local(&mut self, path: &Path, bin: &str) {
        let Some(project) = self.projects.iter().find(|p| p.path == path) else { return };
        let git = project.git();
        let label = installs::local_label(
            project.meta().and_then(|m| m.version.as_deref()).unwrap_or("0.0.0"),
            git.and_then(|g| g.commits.first()).map(|c| c.hash.as_str()),
            git.is_some_and(|g| g.dirty()),
        );
        self.start_task(path, Task::Install { bin: bin.to_owned(), label });
    }

    /// Скачать выпуск с GitHub и поставить.
    pub fn install_release(&mut self, path: &Path, bin: &str, release: &Release) {
        let Some(version) = anvil_update::Version::parse(&release.tag) else { return };
        let name = anvil_update::asset_name(bin, &version);
        let (Some(asset), Some(sums)) =
            (release.assets.iter().find(|a| a.name == name), release.assets.iter().find(|a| a.name == "SHA256SUMS"))
        else {
            self.toasts.push(t("В выпуске нет архива по соглашению или SHA256SUMS"), Tone::Danger);
            return;
        };
        let download = crate::jobs::Download {
            bin: bin.to_owned(),
            version: version.to_string(),
            asset: name,
            asset_url: asset.url.clone(),
            asset_api: asset.api_url.clone(),
            sums_url: sums.url.clone(),
            sums_api: sums.api_url.clone(),
        };
        self.enqueue(tasks::download_spec(path, download, asset.size));
    }

    /// Сделать активной другую установленную версию — откат или возврат.
    pub fn activate(&mut self, path: &Path, bin: &str, version: &str) {
        match anvil_update::install::activate(&installs::root(bin), version) {
            Ok(()) => {
                let running = self
                    .running(bin)
                    .iter()
                    .any(|r| r.path.as_ref().is_some_and(|p| installs::inside(p, &installs::root(bin))));
                let text = if running {
                    format!("{bin}: {} {version} — {}", t("текущая"), t("перезапустите программу"))
                } else {
                    format!("{bin}: {} {version}", t("текущая"))
                };
                self.toasts.push(text, Tone::Success);
            }
            Err(e) => self.toasts.push(format!("{bin}: {e}"), Tone::Danger),
        }
        self.refresh_installs(path);
    }

    /// Запустить установленную копию (через `current`).
    pub fn launch_installed(&mut self, bin: &str) {
        let current = installs::root(bin).join("current");
        let launch = crate::launch::Launch {
            exe: current.join(format!("{bin}{}", std::env::consts::EXE_SUFFIX)),
            args: Vec::new(),
            dir: current,
        };
        if let Err(e) = crate::launch::start(&launch) {
            self.toasts.push(format!("{bin}: {e}"), Tone::Danger);
        }
    }

    /// amber-admin: установленная копия — сразу, иначе сборка и запуск из проекта.
    pub fn open_amber_admin(&mut self, dir: &Path) {
        if self.installs.get(crate::amber::ADMIN).is_some_and(Option::is_some) {
            self.launch_installed(crate::amber::ADMIN);
        } else {
            self.start_task(dir, Task::Run { bin: crate::amber::ADMIN.to_owned(), args: Vec::new() });
        }
    }

    /// Удалить установку целиком (после подтверждения). Запущенную — не удаляем.
    pub fn uninstall(&mut self, path: &Path, bin: &str) {
        let root = installs::root(bin);
        if self.running(bin).iter().any(|r| r.path.as_ref().is_some_and(|p| installs::inside(p, &root))) {
            self.toasts
                .push(format!("{bin}: {}", t("программа запущена из установки — сначала закройте её")), Tone::Warning);
            return;
        }
        installs::remove_shortcut(bin);
        match anvil_update::install::uninstall(&root) {
            Ok(()) => self.toasts.push(format!("{bin}: {}", t("установка удалена")), Tone::Neutral),
            Err(e) => self.toasts.push(format!("{bin}: {e}"), Tone::Danger),
        }
        self.refresh_installs(path);
    }

    // ─── Зависимости ──────────────────────────────────────────────────────────

    /// Проверить зависимости проектов. `force` — даже если итог свежий.
    pub fn check_deps(&mut self, projects: Vec<PathBuf>, force: bool) {
        let _ = self.deps_commands.send(deps::Cmd::Check { projects, force });
    }

    pub fn check_toolchain(&mut self, force: bool) {
        let _ = self.deps_commands.send(deps::Cmd::Toolchain { force });
    }

    /// Запустить подтверждённое: обновление зависимостей, перевод на тег набора, обновление Rust.
    pub fn start_deps(&mut self, ask: crate::ui::deps::Ask) {
        use crate::jobs::Step;
        use crate::ui::deps::Ask;
        let jobs = self.config.build_jobs;
        let mut test = vec!["test".to_owned(), "--workspace".to_owned()];
        if jobs > 0 {
            test.extend(["-j".to_owned(), jobs.to_string()]);
        }
        let (spec, after) = match ask {
            Ask::Update(path) => {
                let steps = vec![
                    Step::Snapshot(vec![path.join("Cargo.lock")]),
                    Step::Run("cargo".into(), vec!["update".into()]),
                    Step::Run("cargo".into(), test),
                ];
                (tasks::script_spec(&path, "cargo update → cargo test".into(), steps), After::Deps(path))
            }
            Ask::Kit(path, tag) => {
                let edits = match deps::kit_edits(&path, &tag) {
                    Ok(edits) => edits,
                    Err(e) => {
                        self.toasts.push(format!("{}: {e}", worker::display_name(&path)), Tone::Danger);
                        return;
                    }
                };
                let mut files: Vec<PathBuf> = edits.iter().map(|(p, _)| p.clone()).collect();
                files.push(path.join("Cargo.lock"));
                let steps = vec![Step::Snapshot(files), Step::Write(edits), Step::Run("cargo".into(), test)];
                (tasks::script_spec(&path, format!("anvil kit → {tag}"), steps), After::Deps(path))
            }
            Ask::Rustup(name) => {
                // Своей папки у тулчейна нет: задача живёт в папке кеша, в списке задач — «Rust».
                let dir = self.config_path.with_file_name("cache").join("Rust");
                let _ = std::fs::create_dir_all(&dir);
                let steps = vec![Step::Run("rustup".into(), vec!["update".into(), name.clone()])];
                (tasks::script_spec(&dir, format!("rustup update {name}"), steps), After::Toolchain)
            }
        };
        let id = self.enqueue(spec);
        self.after_job.insert(id, after);
        self.log_open = true;
    }

    pub fn report(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.toasts.push(e, Tone::Danger);
        }
    }
}

pub const PROJECTS_RU: [&str; 3] = ["проекта", "проектов", "проектов"];
pub const PROJECTS_EN: [&str; 2] = ["project", "projects"];

/// Длительность коротко: «41 с», «2 мин 05 с».
pub fn duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs} {}", t("с"))
    } else {
        format!("{} {} {:02} {}", secs / 60, t("мин"), secs % 60, t("с"))
    }
}
