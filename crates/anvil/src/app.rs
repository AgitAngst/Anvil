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

/// Что в середине окна Кузницы: карточка выбранного проекта или обзор всех.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Project,
    Overview,
}

/// Режим окна: Пульт — запускать, Кузница — собирать и выпускать.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Deck,
    Forge,
}

/// Что Пульт помнит, пока окно открыто. На диск не пишется: ходить стрелками — не повод трогать
/// `anvil.toml`.
#[derive(Default)]
pub struct DeckView {
    /// Выбранный предмет (ключ).
    pub selected: Option<String>,
    /// Порядок строк. Пока Пульт на экране, строки не прыгают: порядок пересобирается при входе на
    /// Пульт, по F5 и когда предметы появились или пропали.
    pub order: Vec<String>,
    /// Выбор сдвинули клавишами — прокрутить к нему.
    pub scroll: bool,
    /// Что и когда запускали: повторный Enter не запускает вторую копию.
    pub launching: HashMap<String, Instant>,
}

/// Что сделать, когда задача закончится.
enum After {
    /// Перепроверить зависимости проекта: `Cargo.lock` мог поменяться.
    Deps(PathBuf),
    /// Перечитать тулчейн: Rust обновился.
    Toolchain,
    /// Запустить только что поставленную программу: «Поставить v0.1.0 и запустить».
    Launch(String),
    /// Запустить собранный exe: игру после экспорта Godot. Имя — для уведомления об ошибке.
    LaunchExe(String, PathBuf),
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
    pub mode: Mode,
    pub deck_view: DeckView,
    /// Когда собран release-exe бинарника (`target\release`), секунды Unix; `None` — не собран.
    pub builds: HashMap<String, Option<i64>>,
    /// Редактор Godot: `None` — ещё не искали, `Some(None)` — не нашёлся.
    godot_editor: Option<Option<PathBuf>>,
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
        let (config, config_error, migrated) = config::load(&config_path);
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
            mode: Mode::Deck,
            deck_view: DeckView::default(),
            builds: HashMap::new(),
            godot_editor: None,
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
        if migrated {
            app.save();
        }
        app.rescan();
        app
    }

    pub fn rescan(&mut self) {
        self.scanning = true;
        let _ = self.commands.send(Cmd::Rescan(self.config.roots.clone(), self.config.kinds.clone()));
    }

    /// Перечитать всё (F5): проекты, GitHub, редактор Godot; строки Пульта встают по-новому.
    pub fn refresh(&mut self) {
        let _ = self.commands.send(Cmd::Refresh { full: true });
        let _ = self.gh_commands.send(github::Cmd::Refresh);
        self.godot_editor = None;
        self.deck_view.order.clear();
    }

    /// Задать путь к редактору Godot.
    pub fn set_godot(&mut self, path: PathBuf) {
        self.config.godot = Some(path);
        self.godot_editor = None;
        self.save();
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
                        self.projects.push(Project {
                            path,
                            kind,
                            meta: None,
                            git: Ok(None),
                            notes: Vec::new(),
                            engine: None,
                        });
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

    /// Обзор ↔ карточка проекта (`Ctrl+0`). Обзор живёт в Кузнице.
    pub fn toggle_view(&mut self) {
        if self.mode == Mode::Deck {
            self.mode = Mode::Forge;
            self.view = View::Overview;
            return;
        }
        self.view = match self.view {
            View::Project => View::Overview,
            View::Overview => View::Project,
        };
    }

    /// Сменить режим. Выбор общий: в Кузнице открывается проект выбранной строки Пульта, на Пульте
    /// выбирается строка проекта, открытого в Кузнице. На Пульт — с пересобранным порядком строк:
    /// пока нас не было, запускали.
    pub fn set_mode(&mut self, mode: Mode) {
        if mode == self.mode {
            return;
        }
        let items = self.deck_items();
        match mode {
            Mode::Forge => {
                let key = self.deck_view.selected.clone();
                let item = key.and_then(|k| items.iter().find(|i| i.key == k)).or_else(|| items.first());
                if let Some(project) = item.map(|i| i.project.clone()) {
                    if self.selected.as_ref() != Some(&project) {
                        self.view = View::Project;
                    }
                    self.select(project);
                }
            }
            Mode::Deck => {
                self.deck_view.order.clear();
                if let Some(project) = self.current().map(|p| p.path.clone()) {
                    let ours = |k: &String| items.iter().any(|i| &i.key == k && i.project == project);
                    if !self.deck_view.selected.as_ref().is_some_and(ours) {
                        self.deck_view.selected = items.iter().find(|i| i.project == project).map(|i| i.key.clone());
                    }
                    self.deck_view.scroll = true;
                }
            }
        }
        self.mode = mode;
    }

    // ─── Пульт ────────────────────────────────────────────────────────────────

    /// Предметы Пульта в порядке строк, без убранных.
    pub fn deck_items(&mut self) -> Vec<crate::deck::Item> {
        let items = crate::deck::items(self.visible(), |path| self.config.project(path).icon);
        let fresh: Vec<&str> = {
            let mut keys: Vec<&str> = items
                .iter()
                .map(|i| i.key.as_str())
                .filter(|k| !self.config.deck.removed.iter().any(|r| r == k))
                .collect();
            keys.sort_unstable();
            keys
        };
        let mut known: Vec<&str> = self.deck_view.order.iter().map(String::as_str).collect();
        known.sort_unstable();
        if fresh != known {
            self.deck_view.order = crate::deck::order(&items, &self.config.deck);
        }
        let order = &self.deck_view.order;
        let mut ordered: Vec<crate::deck::Item> = items.into_iter().filter(|i| order.contains(&i.key)).collect();
        ordered.sort_by_key(|i| order.iter().position(|k| *k == i.key));
        ordered
    }

    /// Редактор Godot: ищется один раз (в `PATH` это обход папок) и заново — по F5 и после
    /// «Путь к Godot…».
    pub fn godot_editor(&mut self) -> Option<PathBuf> {
        if self.godot_editor.is_none() {
            self.godot_editor = Some(crate::engines::godot_editor(self.config.godot.as_deref()));
        }
        self.godot_editor.clone().flatten()
    }

    /// Предмет только что запускали: второй Enter подряд (или повтор клавиши) не запускает копию,
    /// пока снимок процессов не увидел первую.
    pub fn launching(&self, key: &str) -> bool {
        self.deck_view.launching.get(key).is_some_and(|at| at.elapsed() < Duration::from_secs(4))
    }

    /// Запомнить, что предмет запускали с Пульта: по этому сортируется группа.
    fn mark_launched(&mut self, key: &str) {
        self.deck_view.launching.insert(key.to_owned(), Instant::now());
        self.config.deck.launched.insert(key.to_owned(), i18n::now());
        self.save();
    }

    /// Запустить предмет. `from_code` — свежую сборку из кода (`Ctrl+Enter`; у Godot — экспорт и
    /// игра), иначе — то, что установлено (или собрано, если ставить нечего).
    pub fn launch_item(&mut self, item: &crate::deck::Item, from_code: bool) {
        if item.is_self() || self.launching(&item.key) {
            return;
        }
        let started = match item.kind {
            registry::Kind::Rust => {
                let Some(bin) = item.bin.clone() else { return };
                let installed = self.installs.get(&bin).is_some_and(Option::is_some);
                if installed && !from_code {
                    self.launch_installed(&bin)
                } else {
                    // Занятый exe — окно выбора; задача ещё не встала, но выбор сделан.
                    self.start_task_as(&item.project, Task::Run { bin, args: Vec::new() }, true);
                    true
                }
            }
            registry::Kind::Godot if from_code => self.export_and_play(item),
            registry::Kind::Godot => {
                let project = self.projects.iter().find(|p| p.path == item.project);
                let export = project.and_then(|p| p.engine.as_ref()).and_then(|e| e.export.clone());
                match export.filter(|e| e.is_file()) {
                    Some(exe) => self.start_program(&item.name, &exe, &[], None),
                    None => self.play_from_source(item),
                }
            }
            registry::Kind::Unity => return self.open_editor(item),
            registry::Kind::Git => return,
        };
        if started {
            self.mark_launched(&item.key);
        }
    }

    /// Godot из исходников: движок запускает главную сцену проекта, без экспорта.
    pub fn play_from_source(&mut self, item: &crate::deck::Item) -> bool {
        let Some(godot) = self.godot_editor() else {
            self.toasts.push(t("Godot не найден: укажите путь к редактору в меню строки"), Tone::Warning);
            return false;
        };
        let dir = item.project.to_string_lossy().into_owned();
        self.start_program(&item.name, &godot, &["--path".into(), dir], Some(&item.project))
    }

    /// Godot: экспортировать игру под Windows (задача в консоли, ошибки видны там же) и запустить.
    fn export_and_play(&mut self, item: &crate::deck::Item) -> bool {
        let project = self.projects.iter().find(|p| p.path == item.project);
        let Some(export) = project.and_then(|p| p.engine.as_ref()).and_then(|e| e.export.clone()) else {
            self.toasts.push(t("В export_presets.cfg нет пресета «Windows Desktop»"), Tone::Warning);
            return false;
        };
        let Some(godot) = self.godot_editor() else {
            self.toasts.push(t("Godot не найден: укажите путь к редактору в меню строки"), Tone::Warning);
            return false;
        };
        if let Some(dir) = export.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let args = vec![
            "--headless".into(),
            "--path".into(),
            item.project.to_string_lossy().into_owned(),
            "--export-release".into(),
            "Windows Desktop".into(),
            export.to_string_lossy().into_owned(),
        ];
        let title = format!("godot --export-release \"Windows Desktop\" {}", export.display());
        let steps = vec![jobs::Step::Run(godot.to_string_lossy().into_owned(), args)];
        let id = self.enqueue(tasks::script_spec(&item.project, title, steps));
        self.after_job.insert(id, After::LaunchExe(item.name.clone(), export));
        true
    }

    /// Открыть проект Godot или Unity в редакторе.
    pub fn open_editor(&mut self, item: &crate::deck::Item) {
        let dir = item.project.to_string_lossy().into_owned();
        let started = match item.kind {
            registry::Kind::Godot => match self.godot_editor() {
                Some(godot) => {
                    self.start_program(&item.name, &godot, &["-e".into(), "--path".into(), dir], Some(&item.project))
                }
                None => {
                    self.toasts.push(t("Godot не найден: укажите путь к редактору в меню строки"), Tone::Warning);
                    false
                }
            },
            registry::Kind::Unity => {
                let engine = self.projects.iter().find(|p| p.path == item.project).and_then(|p| p.engine.as_ref());
                let version = engine.and_then(|e| e.version.clone()).unwrap_or_default();
                match engine.and_then(|e| e.editor.clone()) {
                    Some(unity) => {
                        self.start_program(&item.name, &unity, &["-projectPath".into(), dir], Some(&item.project))
                    }
                    None => {
                        self.toasts.push(format!("Unity {version} {}", t("не найден в Unity Hub")), Tone::Warning);
                        false
                    }
                }
            }
            _ => false,
        };
        if started {
            self.mark_launched(&item.key);
        }
    }

    /// Запустить exe отдельно от Anvil; ошибку — в уведомление. `true` — запустилось.
    fn start_program(&mut self, name: &str, exe: &Path, args: &[String], dir: Option<&Path>) -> bool {
        let dir = dir.map(Path::to_path_buf).or_else(|| exe.parent().map(Path::to_path_buf)).unwrap_or_default();
        let launch = crate::launch::Launch { exe: exe.to_path_buf(), args: args.to_vec(), dir };
        match crate::launch::start(&launch) {
            Ok(_) => true,
            Err(e) => {
                self.toasts.push(format!("{name}: {e}"), Tone::Danger);
                false
            }
        }
    }

    /// Поставить выпуск с GitHub и сразу запустить.
    pub fn install_and_launch(&mut self, item: &crate::deck::Item) {
        let Some(bin) = item.bin.clone() else { return };
        if self.launching(&item.key) {
            return;
        }
        let remote = self.remotes.get(&item.project).cloned();
        let Some((release, _)) = crate::ui::install::release_for(remote.as_ref(), &bin, self.config.common.prerelease)
        else {
            return;
        };
        let release = release.clone();
        if let Some(id) = self.install_release(&item.project, &bin, &release) {
            self.after_job.insert(id, After::Launch(bin));
            self.mark_launched(&item.key);
        }
    }

    /// Показать окно запущенной программы.
    pub fn focus(&mut self, name: &str, pid: u32) {
        if !procs::focus_window(pid) {
            self.toasts.push(format!("{name}: {}", t("окна не видно — возможно, программа в трее")), Tone::Neutral);
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
            Some(After::Launch(bin)) if outcome.ok && matches!(outcome.installed, Some(Ok(_))) => {
                self.installs.insert(bin.clone(), installs::scan(&bin));
                self.launch_installed(&bin);
            }
            Some(After::Launch(_)) => {}
            Some(After::LaunchExe(name, exe)) if outcome.ok => {
                self.start_program(&name, &exe, &[], None);
            }
            Some(After::LaunchExe(..)) => {}
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

    /// Перечитать, что установлено для бинарников проекта и когда собран их release-exe.
    pub fn refresh_installs(&mut self, path: &Path) {
        let Some(meta) = self.projects.iter().find(|p| p.path == path).and_then(|p| p.meta()) else { return };
        let bins: Vec<(String, PathBuf)> = meta
            .bins
            .iter()
            .map(|b| (b.name.clone(), crate::launch::exe_path(&meta.target_dir, true, &b.name)))
            .collect();
        for (bin, exe) in bins {
            self.installs.insert(bin.clone(), installs::scan(&bin));
            let built = std::fs::metadata(&exe)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            self.builds.insert(bin, built);
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
    pub fn install_release(&mut self, path: &Path, bin: &str, release: &Release) -> Option<JobId> {
        let version = anvil_update::Version::parse(&release.tag)?;
        let name = anvil_update::asset_name(bin, &version);
        let (Some(asset), Some(sums)) =
            (release.assets.iter().find(|a| a.name == name), release.assets.iter().find(|a| a.name == "SHA256SUMS"))
        else {
            self.toasts.push(t("В выпуске нет архива по соглашению или SHA256SUMS"), Tone::Danger);
            return None;
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
        Some(self.enqueue(tasks::download_spec(path, download, asset.size)))
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

    /// Запустить установленную копию (через `current`). `true` — запустилась.
    pub fn launch_installed(&mut self, bin: &str) -> bool {
        let current = installs::root(bin).join("current");
        let exe = current.join(format!("{bin}{}", std::env::consts::EXE_SUFFIX));
        self.start_program(bin, &exe, &[], Some(&current))
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
