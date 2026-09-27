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
    /// Открытая страница предмета (ключ); `None` — список Пульта.
    pub page: Option<String>,
    /// Вкладка страницы предмета.
    pub page_tab: usize,
    /// Поиск в журнале службы и «только ошибки и предупреждения».
    pub log_find: String,
    pub log_errors: bool,
}

/// Каким было окно до быстрого запуска: куда его вернуть.
#[derive(Debug, Clone, Copy)]
pub struct Restore {
    hidden: bool,
    minimized: bool,
    maximized: bool,
}

/// Проверки портов, общие с фоновыми потоками, которые подключаются.
type PortChecks = std::sync::Arc<std::sync::Mutex<HashMap<u16, PortCheck>>>;

/// Слушает ли порт по последней проверке, когда она была и идёт ли новая.
#[derive(Clone, Copy)]
struct PortCheck {
    open: bool,
    at: Option<Instant>,
    pending: bool,
}

/// Хвост файла вывода, прочитанный недавно: читается заново, только если файл изменился.
struct LogCache {
    path: PathBuf,
    len: u64,
    checked: Instant,
    lines: Vec<String>,
}

/// Что сделать, когда задача закончится.
enum After {
    /// Перепроверить зависимости проекта: `Cargo.lock` мог поменяться.
    Deps(PathBuf),
    /// Перечитать тулчейн: Rust обновился.
    Toolchain,
    /// Запустить только что поставленную программу: «Поставить v0.1.0 и запустить».
    Launch(Box<crate::deck::Item>),
    /// Запустить собранный exe: игру после экспорта Godot. Имя — для уведомления об ошибке.
    LaunchExe(String, PathBuf, Option<crate::runs::Tag>),
    /// Запомнить, из какого коммита собран exe.
    Built(PathBuf, crate::builds::Build),
}

/// Что вышло из остановки, начатой окном.
pub enum StopNote {
    Stopped(String),
    /// Мягко не закрылся за отведённое время — спросить, остановить ли принудительно.
    Timeout(String, u32),
    Failed(String, String),
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
    after_job: HashMap<JobId, Vec<After>>,
    /// История запусков (последние 200) — `cache\runs.json`.
    pub runs: Vec<crate::runs::Run>,
    runs_path: PathBuf,
    run_events: Receiver<crate::runs::Event>,
    stop_tx: Sender<StopNote>,
    stop_rx: Receiver<StopNote>,
    /// Не закрылся мягко: спросить про принудительную остановку (имя, PID, когда процесс запущен —
    /// чтобы не остановить чужой процесс, получивший тот же PID).
    pub force_confirm: Option<(String, u32, Option<i64>)>,
    /// Коммит сборки, ждущей решения в окне занятого exe.
    locked_head: Option<crate::builds::Build>,
    /// Профили правили — сохранить при закрытии окна.
    pub presets_dirty: bool,
    /// «Поставить из кода…» ждёт подтверждения: проект, бинарник.
    pub install_confirm: Option<(PathBuf, String)>,
    /// «Откатить…» ждёт подтверждения: проект, бинарник, версия.
    pub rollback_confirm: Option<(PathBuf, String, String)>,
    log_cache: Option<LogCache>,
    /// Слушает ли порт: проверка в фоне не чаще раза в 2 с; `None` — ещё идёт.
    port_checks: PortChecks,
    /// Из какого коммита собраны exe (`cache\builds.json`).
    pub build_info: crate::builds::Builds,
    builds_path: PathBuf,
    ctx: egui::Context,
    /// Поле «свой токен» в настройках (не хранится нигде, кроме keyring после «Сохранить»).
    pub token_input: String,
    gh_commands: Sender<github::Cmd>,
    gh_events: Receiver<github::Event>,
    gh_targets: Vec<Target>,
    job_commands: Sender<jobs::Cmd>,
    job_events: Receiver<jobs::Event>,
    /// Уведомления о конце долгих задач: решает поток задач, окно сообщает настройку.
    pub notifier: Arc<crate::notify::Notifier>,
    next_job: JobId,
    units: UnitsCache,
    units_path: PathBuf,
    commands: Sender<Cmd>,
    events: Receiver<Event>,
    last_fetch: Instant,
    was_focused: bool,
    /// Значок в трее; `None` — трея нет (не Windows или не создался): крестик закрывает Anvil.
    pub tray: Option<crate::tray::Tray>,
    pub tray_rx: Receiver<crate::tray::Request>,
    /// Глобальное сочетание быстрого запуска.
    pub hotkey: crate::hotkey::Hotkey,
    /// Каким состояние сочетания видели в прошлый раз: «занято» сообщается один раз.
    pub hotkey_seen: crate::hotkey::State,
    /// Щелчки по уведомлениям Windows: `open:<ключ>`, `log:<ключ>`, `again:<ключ>`, `show`.
    pub clicks: Receiver<String>,
    /// Окно спрятано в трей.
    pub hidden: bool,
    /// Выход по-настоящему (меню трея): крестик больше не прячет.
    quitting: bool,
    /// Быстрый запуск на месте главного окна: каким окно было до него.
    pub quick: Option<Restore>,
    /// Быстрый запуск уже получал фокус: потерял — закрыть.
    quick_focused: bool,
    /// Последние обычные место и размер окна: к ним окно возвращается после быстрого запуска.
    normal: Option<(egui::Pos2, egui::Vec2)>,
    /// Окно было развёрнуто на весь экран (до того, как его свернули или спрятали).
    was_maximized: bool,
    /// Спрятанное окно развернуть при показе (развернуть спрятанное — значит показать).
    maximize_on_show: bool,
    /// Когда быстрый запуск закрылся от щелчка мимо.
    quick_closed_at: Option<Instant>,
    /// Щелчок по значку трея ждёт, не двойной ли он (двойной — окно, а не быстрый запуск).
    pub quick_pending: Option<Instant>,
    /// Когда окно открывали двойным щелчком по значку: его хвостовой щелчок — не быстрый запуск.
    pub tray_shown_at: Option<Instant>,
    /// Ошибка, пока окна не видно: красная точка на значке, пока окно не откроют.
    pub alert: bool,
    /// С какого момента уведомления в окне ещё не пересланы в Windows (окно спрятано).
    pub toasts_seen: Instant,
    /// Когда меню трея собиралось последний раз.
    pub tray_built: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let config_path = config::path();
        let (config, config_error, migrated) = config::load(&config_path);
        anvil_ui::install(&cc.egui_ctx, ACCENT, config.common.theme);
        config.common.apply(&cc.egui_ctx);
        i18n::set(&cc.egui_ctx, config.common.language);

        let (commands, events) = worker::spawn(cc.egui_ctx.clone());
        let cache = config_path.with_file_name("cache");
        let _ = std::fs::create_dir_all(&cache);
        let runs_path = cache.join("runs.json");
        let runs = crate::runs::load(&runs_path);
        let next_run = runs.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        let run_events = crate::runs::init(cc.egui_ctx.clone(), crate::runs::default_dir(&config_path), next_run);
        // Что осталось работать с прошлого раза — снова под присмотром: код выхода не потеряется.
        for run in runs.iter().filter(|r| r.running()) {
            crate::runs::watch(run);
        }
        let builds_path = cache.join("builds.json");
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let notifier = Arc::new(crate::notify::Notifier::new(config.notify, config_path.with_file_name("cache")));
        notifier.set_crash(config.notify_crash);
        let (job_commands, job_events, _) = jobs::spawn(cc.egui_ctx.clone(), notifier.clone());
        // Трей, сочетание и щелчки по уведомлениям будят окно, даже спрятанное.
        let wake = |ctx: &egui::Context| {
            let ctx = ctx.clone();
            move || ctx.request_repaint_of(egui::ViewportId::ROOT)
        };
        let (clicks_tx, clicks) = std::sync::mpsc::channel();
        notifier.set_clicks(clicks_tx, wake(&cc.egui_ctx));
        let (tray_tx, tray_rx) = std::sync::mpsc::channel();
        #[cfg(windows)]
        crate::instance::listen(&config_path, tray_tx.clone(), wake(&cc.egui_ctx));
        let tray = crate::tray::Tray::new(tray_tx, cc.egui_ctx.clone(), Vec::new()).ok();
        let hotkey =
            crate::hotkey::Hotkey::spawn(crate::hotkey::Combo::parse(&config.quick.hotkey), wake(&cc.egui_ctx));
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
            runs,
            runs_path,
            run_events,
            stop_tx,
            stop_rx,
            force_confirm: None,
            locked_head: None,
            presets_dirty: false,
            install_confirm: None,
            rollback_confirm: None,
            log_cache: None,
            port_checks: Default::default(),
            build_info: crate::builds::load(&builds_path),
            builds_path,
            ctx: cc.egui_ctx.clone(),
            token_input: String::new(),
            gh_commands,
            gh_events,
            gh_targets: Vec::new(),
            job_commands,
            job_events,
            notifier,
            next_job: 1,
            units: tasks::load_units(&units_path),
            units_path,
            commands,
            events,
            last_fetch: Instant::now(),
            was_focused: true,
            tray,
            tray_rx,
            hotkey,
            hotkey_seen: crate::hotkey::State::Pending,
            clicks,
            hidden: false,
            quitting: false,
            quick: None,
            quick_focused: false,
            normal: None,
            was_maximized: false,
            maximize_on_show: false,
            quick_closed_at: None,
            quick_pending: None,
            tray_shown_at: None,
            alert: false,
            toasts_seen: Instant::now(),
            tray_built: Instant::now() - Duration::from_secs(10),
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
        self.window_tick(ctx);
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
        while let Ok(event) = self.run_events.try_recv() {
            self.apply_run(event);
        }
        while let Ok(note) = self.stop_rx.try_recv() {
            match note {
                StopNote::Stopped(name) => self.toasts.push(format!("{name} {}", t("остановлен")), Tone::Success),
                StopNote::Timeout(name, pid) => {
                    let started = self.procs.values().flatten().find(|r| r.pid == pid).and_then(|r| r.started);
                    self.force_confirm = Some((name, pid, started));
                }
                StopNote::Failed(name, e) => self.toasts.push(format!("{name}: {e}"), Tone::Danger),
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
                    // В Кузнице выбрали другой проект — страница прежнего предмета закрывается.
                    if self.deck_view.page.as_ref().is_some_and(|k| !ours(k)) {
                        self.deck_view.page = None;
                    }
                    if !self.deck_view.selected.as_ref().is_some_and(ours) {
                        self.deck_view.selected = items.iter().find(|i| i.project == project).map(|i| i.key.clone());
                    }
                    self.deck_view.scroll = true;
                }
            }
        }
        self.mode = mode;
    }

    // ─── Окно и трей ───────────────────────────────────────────────────────────

    /// Крестик, фокус быстрого запуска, запоминание места окна. Зовётся и у спрятанного окна.
    fn window_tick(&mut self, ctx: &egui::Context) {
        let (close, focused, minimized, maximized, outer, inner) = ctx.input(|i| {
            let v = i.viewport();
            (
                v.close_requested(),
                v.focused,
                v.minimized.unwrap_or(false),
                v.maximized.unwrap_or(false),
                v.outer_rect,
                v.inner_rect,
            )
        });
        // Выход из меню трея или перезапуск после обновления — закрытие настоящее.
        if close && !self.quitting && !self.updater.restarting() {
            if self.quick.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.end_quick(false);
            } else if self.tray.is_some() && self.config.window.close_to_tray {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_window();
                if !self.config.window.tray_hint_shown {
                    self.config.window.tray_hint_shown = true;
                    self.save();
                    self.notifier.hint(
                        t("Anvil работает в трее").to_owned(),
                        t("Крестик прячет окно. Выход — в меню значка; запущенное продолжит работать.").to_owned(),
                    );
                }
            }
        }
        match (self.quick.is_some(), focused) {
            (true, Some(true)) => self.quick_focused = true,
            // Щёлкнули мимо — быстрый запуск закрывается, как у любого лаунчера.
            (true, Some(false)) if self.quick_focused => {
                self.quick_closed_at = Some(Instant::now());
                self.end_quick(false);
            }
            _ => {}
        }
        if self.quick.is_none() && !self.hidden && !minimized {
            // Окно снова перед глазами (как угодно: из панели задач тоже) — точка на значке гаснет.
            if focused == Some(true) {
                self.alert = false;
            }
            self.was_maximized = maximized;
            if !maximized && let (Some(outer), Some(inner)) = (outer, inner) {
                self.normal = Some((outer.min, inner.size()));
            }
        }
    }

    /// Показать окно: из трея, по уведомлению, «Открыть Anvil».
    pub fn show_window(&mut self) {
        if self.quitting {
            return;
        }
        if self.quick.is_some() {
            self.end_quick(true);
            return;
        }
        self.hidden = false;
        self.alert = false;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        // Развернуть спрятанное окно нельзя (оно бы показалось) — только теперь, когда оно видно.
        if std::mem::take(&mut self.maximize_on_show) {
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Спрятать окно в трей.
    pub fn hide_window(&mut self) {
        self.hidden = true;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    /// Выйти по-настоящему: запущенное продолжит работать.
    pub fn quit(&mut self) {
        self.quitting = true;
        self.end_quick(false);
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Быстрый запуск (сочетание, щелчок по значку). Окно на экране — в нём палитра; спрятано или
    /// свёрнуто — оно само на время становится окном быстрого запуска: без рамки, поверх всех, 640
    /// в ширину (§7.5), над монитором под курсором. Повтор сочетания закрывает.
    pub fn quick_launch(&mut self) {
        if self.quitting {
            return;
        }
        if self.quick.is_some() {
            self.end_quick(false);
            return;
        }
        let (minimized, monitor, ppp) = self.ctx.input(|i| {
            let v = i.viewport();
            (v.minimized.unwrap_or(false), v.monitor_size, v.native_pixels_per_point.unwrap_or(1.0))
        });
        if !self.hidden && !minimized {
            self.show_window();
            crate::ui::palette::open(self);
            return;
        }
        let maximized = self.was_maximized || self.maximize_on_show;
        self.quick = Some(Restore { hidden: self.hidden, minimized, maximized });
        self.maximize_on_show = false;
        self.quick_focused = false;
        let size = crate::ui::palette::QUICK_SIZE;
        let pos = quick_position(size, ppp).unwrap_or_else(|| {
            let monitor = monitor.unwrap_or(egui::vec2(1920.0, 1080.0));
            egui::pos2(((monitor.x - size.x) / 2.0).max(0.0), (monitor.y * 0.2).max(0.0))
        });
        let ctx = self.ctx.clone();
        use egui::ViewportCommand as V;
        // Свёрнутое окно сначала разворачивается: размер свёрнутому не задать — Windows вернёт
        // прежний. Развёрнутое на весь экран — сначала в обычное.
        if minimized {
            ctx.send_viewport_cmd(V::Minimized(false));
        }
        if maximized {
            ctx.send_viewport_cmd(V::Maximized(false));
        }
        for command in [
            V::MinInnerSize(egui::Vec2::ZERO),
            V::Decorations(false),
            V::Resizable(false),
            V::InnerSize(size),
            V::OuterPosition(pos),
            V::WindowLevel(egui::WindowLevel::AlwaysOnTop),
            V::Visible(true),
            V::Focus,
        ] {
            ctx.send_viewport_cmd(command);
        }
        self.hidden = false;
        crate::ui::palette::open_quick(self);
    }

    /// Закрыть быстрый запуск: окно возвращается, каким было (спрятанным, свёрнутым). `show` —
    /// открыть обычное окно (страница, Кузница, «Открыть Anvil»).
    pub fn end_quick(&mut self, show: bool) {
        let Some(restore) = self.quick.take() else { return };
        // Вопрос «exe занят» остался без ответа — сборку не ставим: окно уходит, спросить негде.
        let asking = self.palette.as_ref().is_some_and(crate::ui::palette::State::asking);
        self.palette = None;
        if asking && !show {
            self.resolve_locked(None);
        }
        let ctx = self.ctx.clone();
        use egui::ViewportCommand as V;
        let back_hidden = !show && restore.hidden;
        // Прячется сразу; вид и место возвращаются уже невидимому окну — без мелькания.
        if back_hidden {
            ctx.send_viewport_cmd(V::Visible(false));
        }
        for command in [
            V::WindowLevel(egui::WindowLevel::Normal),
            V::Decorations(true),
            V::Resizable(true),
            V::MinInnerSize(egui::vec2(1040.0, 640.0)),
        ] {
            ctx.send_viewport_cmd(command);
        }
        let (pos, size) = self.normal.unwrap_or((egui::pos2(80.0, 60.0), egui::vec2(1440.0, 900.0)));
        ctx.send_viewport_cmd(V::InnerSize(size));
        ctx.send_viewport_cmd(V::OuterPosition(pos));
        if show {
            self.hidden = false;
            self.alert = false;
            ctx.send_viewport_cmd(V::Visible(true));
            if restore.maximized {
                ctx.send_viewport_cmd(V::Maximized(true));
            }
            ctx.send_viewport_cmd(V::Focus);
        } else if back_hidden {
            // Развернуть спрятанное — значит показать: развернётся, когда его откроют.
            self.maximize_on_show = restore.maximized;
        } else if restore.minimized {
            if restore.maximized {
                ctx.send_viewport_cmd(V::Maximized(true));
            }
            ctx.send_viewport_cmd(V::Minimized(true));
        }
        self.hidden = back_hidden;
    }

    /// Щелчок по значку трея мог закрыть быстрый запуск (окно потеряло фокус) — тогда тот же
    /// щелчок не открывает его снова.
    pub fn quick_just_closed(&self, within: Duration) -> bool {
        self.quick_closed_at.is_some_and(|at| at.elapsed() < within)
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
                let profile = self.profile_of(item);
                let installed = item.bin.as_ref().is_some_and(|b| self.installs.get(b).is_some_and(Option::is_some));
                if installed && !from_code && profile.source == crate::config::Source::Installed {
                    self.launch_installed_as(item, &profile)
                } else {
                    // Занятый exe — окно выбора; задача ещё не встала, но выбор сделан.
                    self.run_from_code(item, &profile);
                    true
                }
            }
            registry::Kind::Godot if from_code => self.export_game(item, true),
            registry::Kind::Godot => {
                let engine = self.projects.iter().find(|p| p.path == item.project).and_then(|p| p.engine.as_ref());
                let export = engine.and_then(|e| e.export.clone()).filter(|e| e.is_file());
                let at = engine.and_then(|e| e.exported_at).map(i18n::date).unwrap_or_default();
                match export {
                    Some(exe) => {
                        let tag = self.game_tag(item, format!("экспорт {at}"));
                        self.start_tagged(&item.name, &exe, &[], None, Some(tag))
                    }
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

    /// Выбранный профиль предмета.
    pub fn profile_of(&self, item: &crate::deck::Item) -> crate::deck::Profile {
        let installed = item.bin.as_ref().is_some_and(|b| self.installs.get(b).is_some_and(Option::is_some));
        let presets = self.config.project(&item.project).presets;
        let profiles = crate::deck::profiles(item, &presets, installed);
        crate::deck::chosen(&profiles, self.config.deck.profile.get(&item.key).map(String::as_str)).clone()
    }

    /// Метка запуска игры: без журнала (игра — не служба), профиль «обычный».
    fn game_tag(&self, item: &crate::deck::Item, source: String) -> crate::runs::Tag {
        crate::runs::Tag {
            key: item.key.clone(),
            name: item.name.clone(),
            profile: String::new(),
            source,
            log: false,
            service: false,
            from_code: false,
        }
    }

    /// Метка запуска: чей он, какой профиль, откуда.
    fn tag(
        &self,
        item: &crate::deck::Item,
        profile: &crate::deck::Profile,
        source: String,
        from_code: bool,
    ) -> crate::runs::Tag {
        let service = item.group == crate::deck::Group::Services;
        crate::runs::Tag {
            key: item.key.clone(),
            name: item.name.clone(),
            profile: profile.name.clone(),
            source,
            // Вывод пишется у служб и у сборок из кода; у установленных программ — нет (бережём SSD).
            log: service || from_code,
            service,
            from_code,
        }
    }

    /// Установленная копия с аргументами профиля.
    fn launch_installed_as(&mut self, item: &crate::deck::Item, profile: &crate::deck::Profile) -> bool {
        let Some(bin) = item.bin.clone() else { return false };
        let current = installs::root(&bin).join("current");
        let version =
            self.installs.get(&bin).and_then(Option::as_ref).and_then(|i| i.current.clone()).unwrap_or_default();
        let launch = crate::launch::Launch {
            exe: current.join(format!("{bin}{}", std::env::consts::EXE_SUFFIX)),
            args: crate::launch::split_args(&crate::launch::expand_env(&profile.args)),
            dir: profile.cwd.clone().unwrap_or(current),
            env: profile.env.clone(),
            // Источник хранится по-русски: история не зависит от языка (см. `i18n::source_word`).
            tag: Some(self.tag(item, profile, format!("установлена {version}"), false)),
        };
        match crate::launch::start(&launch) {
            Ok(_) => true,
            Err(e) => {
                self.toasts.push(format!("{}: {e}", item.name), Tone::Danger);
                false
            }
        }
    }

    /// Собрать release из кода и запустить с профилем: сборка — задачей в консоли, запуск — после.
    fn run_from_code(&mut self, item: &crate::deck::Item, profile: &crate::deck::Profile) {
        let Some(bin) = item.bin.clone() else { return };
        let args = crate::launch::split_args(&crate::launch::expand_env(&profile.args));
        let head = self.head(&item.project);
        let source = match &head {
            Some(build) => format!("сборка {}", crate::builds::label(build)),
            None => "сборка".to_owned(),
        };
        let tag = self.tag(item, profile, source, true);
        let (env, cwd) = (profile.env.clone(), profile.cwd.clone());
        self.start_task_with(&item.project, Task::Run { bin, args }, true, true, move |spec| {
            if let Some(after) = &mut spec.after {
                after.tag = Some(tag);
                after.env = env;
                if let Some(dir) = cwd {
                    after.dir = dir;
                }
            }
        });
    }

    /// «Пересобрать и перезапустить» службу или программу из кода: сборка release; удалась —
    /// прежняя копия останавливается и запускается новая с тем же профилем; не удалась — ничего не
    /// останавливается. Работающий exe отодвигается, чтобы сборка могла записать новый.
    pub fn rebuild_restart(&mut self, item: &crate::deck::Item) {
        let Some(bin) = item.bin.clone() else { return };
        // Заменяется только своя сборка из кода: установленную и чужую так не трогаем (§5.12).
        let Some(run) = self.runs.iter().rev().find(|r| r.key == item.key && r.running() && r.from_code).cloned()
        else {
            return;
        };
        if self.rebuilding(&item.key) {
            return;
        }
        let installed = self.installs.get(&bin).is_some_and(Option::is_some);
        let presets = self.config.project(&item.project).presets;
        let profiles = crate::deck::profiles(item, &presets, installed);
        let mut profile =
            profiles.iter().find(|p| p.name == run.profile).cloned().unwrap_or_else(|| self.profile_of(item));
        profile.source = crate::config::Source::Code;
        let args = crate::launch::split_args(&crate::launch::expand_env(&profile.args));
        let source = match self.head(&item.project) {
            Some(build) => format!("сборка {}", crate::builds::label(&build)),
            None => "сборка".to_owned(),
        };
        let tag = self.tag(item, &profile, source, true);
        let exe = self
            .projects
            .iter()
            .find(|p| p.path == item.project)
            .and_then(|p| p.meta())
            .map(|m| crate::launch::exe_path(&m.target_dir, true, &bin));
        let running_there = exe.as_ref().is_some_and(|e| !crate::launch::running_from(&self.procs, e).is_empty());
        let replace = jobs::Replace { pid: run.pid, started: run.started, service: run.service };
        let (env, cwd) = (profile.env.clone(), profile.cwd.clone());
        self.start_task_with(&item.project, Task::Run { bin, args }, true, false, move |spec| {
            if running_there && let Some(exe) = exe {
                spec.before.push(jobs::Before::MoveAside(exe));
            }
            spec.replace = Some(replace);
            if let Some(after) = &mut spec.after {
                after.tag = Some(tag);
                after.env = env;
                if let Some(dir) = cwd {
                    after.dir = dir;
                }
            }
        });
        self.deck_view.launching.insert(item.key.clone(), Instant::now());
    }

    /// Предмет сейчас пересобирается с заменой (задача ещё не кончилась).
    pub fn rebuilding(&self, key: &str) -> bool {
        self.jobs.iter().any(|j| {
            j.finished.is_none()
                && j.spec.replace.is_some()
                && j.spec.after.as_ref().and_then(|a| a.tag.as_ref()).is_some_and(|t| t.key == key)
        })
    }

    /// Открыть страницу предмета.
    pub fn open_page(&mut self, key: String) {
        self.seen(&key);
        // Страница открывается сверху, а не на прокрутке Пульта.
        self.deck_view.scroll = true;
        self.deck_view.selected = Some(key.clone());
        if self.deck_view.page.as_ref() != Some(&key) {
            self.deck_view.page_tab = 0;
            self.deck_view.log_find.clear();
        }
        self.deck_view.page = Some(key);
    }

    /// Хвост файла вывода: перечитывается не чаще раза в секунду и только если файл изменился.
    pub fn log_lines(&mut self, path: &Path) -> Vec<String> {
        let fresh =
            self.log_cache.as_ref().is_some_and(|c| c.path == path && c.checked.elapsed() < Duration::from_secs(1));
        if !fresh {
            let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let same = self.log_cache.as_ref().is_some_and(|c| c.path == path && c.len == len);
            if same {
                if let Some(cache) = &mut self.log_cache {
                    cache.checked = Instant::now();
                }
            } else {
                let lines = crate::runs::tail(path, 800);
                self.log_cache = Some(LogCache { path: path.to_path_buf(), len, checked: Instant::now(), lines });
            }
            self.ctx.request_repaint_after(Duration::from_secs(1));
        }
        self.log_cache.as_ref().map(|c| c.lines.clone()).unwrap_or_default()
    }

    /// Слушает ли порт на этом компьютере. Подключение пробуется в фоне (окно не ждёт), не чаще
    /// раза в 2 с; до первого ответа — «не слушает».
    pub fn port_open(&mut self, port: u16) -> bool {
        let Ok(mut checks) = self.port_checks.lock() else { return false };
        let entry = checks.entry(port).or_insert(PortCheck { open: false, at: None, pending: false });
        let stale = entry.at.is_none_or(|at| at.elapsed() >= Duration::from_secs(2));
        if stale && !entry.pending {
            // Пока идёт новая проверка, показывается прошлый ответ.
            entry.pending = true;
            let (checks, ctx) = (self.port_checks.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                let open = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok();
                if let Ok(mut checks) = checks.lock() {
                    checks.insert(port, PortCheck { open, at: Some(Instant::now()), pending: false });
                }
                ctx.request_repaint();
            });
        }
        let open = entry.open;
        drop(checks);
        self.ctx.request_repaint_after(Duration::from_secs(2));
        open
    }

    /// Коммит, из которого соберётся проект сейчас: HEAD и есть ли правки.
    fn head(&self, project: &Path) -> Option<crate::builds::Build> {
        let git = self.projects.iter().find(|p| p.path == project)?.git()?;
        let commit = git.commits.first()?.hash.clone();
        Some(crate::builds::Build { commit, dirty: git.dirty(), at: i18n::now() })
    }

    /// Godot из исходников: движок запускает главную сцену проекта, без экспорта.
    pub fn play_from_source(&mut self, item: &crate::deck::Item) -> bool {
        let Some(godot) = self.godot_editor() else {
            self.toasts.push(t("Godot не найден: укажите путь к редактору в меню строки"), Tone::Warning);
            return false;
        };
        let dir = item.project.to_string_lossy().into_owned();
        let tag = self.game_tag(item, "исходники".to_owned());
        self.start_tagged(&item.name, &godot, &["--path".into(), dir], Some(&item.project), Some(tag))
    }

    /// Godot: экспортировать игру под Windows (задача в консоли, ошибки видны там же); `play` —
    /// и запустить.
    pub fn export_game(&mut self, item: &crate::deck::Item, play: bool) -> bool {
        let project = self.projects.iter().find(|p| p.path == item.project);
        let Some(export) = project.and_then(|p| p.engine.as_ref()).and_then(|e| e.export.clone()) else {
            self.toasts.push(t("В export_presets.cfg нет пресета «Windows Desktop»"), Tone::Warning);
            return false;
        };
        let Some(godot) = self.godot_editor() else {
            self.toasts.push(t("Godot не найден: укажите путь к редактору в меню строки"), Tone::Warning);
            return false;
        };
        // Запущенную игру экспорт перезаписать не сможет.
        if !crate::launch::running_from(&self.procs, &export).is_empty() {
            self.toasts.push(
                format!("{}: {}", item.name, t("игра запущена — закройте её, чтобы экспортировать")),
                Tone::Warning,
            );
            return false;
        }
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
        if play {
            let tag = self.game_tag(item, format!("экспорт {}", i18n::date(i18n::now())));
            self.after_job.entry(id).or_default().push(After::LaunchExe(item.name.clone(), export, Some(tag)));
        }
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
        self.start_tagged(name, exe, args, dir, None)
    }

    /// То же с меткой: запуск попадёт в историю, код выхода будет известен.
    fn start_tagged(
        &mut self,
        name: &str,
        exe: &Path,
        args: &[String],
        dir: Option<&Path>,
        tag: Option<crate::runs::Tag>,
    ) -> bool {
        let dir = dir.map(Path::to_path_buf).or_else(|| exe.parent().map(Path::to_path_buf)).unwrap_or_default();
        let launch = crate::launch::Launch { exe: exe.to_path_buf(), args: args.to_vec(), dir, env: Vec::new(), tag };
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
            let _ = bin;
            self.after_job.entry(id).or_default().push(After::Launch(Box::new(item.clone())));
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
        // Запуск из Кузницы тоже попадает в историю: чей он и какой профиль (по аргументам).
        let tag = match &task {
            Task::Run { bin, args } => {
                let item = self.deck_items().into_iter().find(|i| i.project == path && i.bin.as_ref() == Some(bin));
                item.map(|item| {
                    let presets = self.config.project(path).presets;
                    let preset = presets.iter().find(|p| {
                        &p.bin == bin && crate::launch::split_args(&crate::launch::expand_env(&p.args)) == *args
                    });
                    let profile = crate::deck::Profile {
                        name: preset.map(|p| p.name.clone()).unwrap_or_default(),
                        source: crate::config::Source::Code,
                        args: String::new(),
                        cwd: preset.and_then(|p| p.cwd.clone()),
                        env: preset
                            .map(|p| p.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default(),
                        port: None,
                    };
                    let source = match self.head(path) {
                        Some(build) => format!("сборка {}", crate::builds::label(&build)),
                        None => "сборка".to_owned(),
                    };
                    (self.tag(&item, &profile, source, true), profile)
                })
            }
            _ => None,
        };
        self.start_task_with(path, task, release, true, move |spec| {
            if let (Some(after), Some((tag, profile))) = (&mut spec.after, tag) {
                after.tag.get_or_insert(tag);
                if after.env.is_empty() {
                    after.env = profile.env;
                }
                if let Some(dir) = profile.cwd {
                    after.dir = dir;
                }
            }
        })
    }

    /// Поставить задачу, поправив её перед постановкой (метка запуска, окружение, папка).
    /// Сборка exe из кода запоминает коммит — отсюда «сборка 2353af9».
    fn start_task_with(
        &mut self,
        path: &Path,
        task: Task,
        release: bool,
        check_locked: bool,
        tweak: impl FnOnce(&mut jobs::Spec),
    ) -> Option<JobId> {
        let project = self.projects.iter().find(|p| p.path == path)?;
        let meta = project.meta().cloned();
        let mut spec = tasks::spec(path, meta.as_ref(), &task, release, self.config.build_jobs);
        tweak(&mut spec);
        let exe = spec.after.as_ref().map(|a| a.exe.clone()).or_else(|| spec.install.as_ref().map(|i| i.exe.clone()));
        let head = self.head(path);
        let locked = if check_locked { tasks::locked(meta.as_ref(), &task, release, &self.procs) } else { Vec::new() };
        if locked.is_empty() {
            let id = self.enqueue(spec);
            if let (Some(exe), Some(build)) = (exe, head) {
                self.after_job.entry(id).or_default().push(After::Built(exe, build));
            }
            Some(id)
        } else {
            self.locked = Some((spec, locked, task, release));
            self.locked_head = head;
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
        // «Закрыть и собрать»: остановка — своя, не падение; у службы — Ctrl+Break.
        for before in &mut spec.before {
            if let jobs::Before::Stop(pid, service) = before {
                for run in self.runs.iter_mut().filter(|r| r.pid == *pid && r.running()) {
                    run.stopping = true;
                    *service = run.service;
                }
            }
        }
        let exe = spec.after.as_ref().map(|a| a.exe.clone()).or_else(|| spec.install.as_ref().map(|i| i.exe.clone()));
        let head = self.locked_head.take();
        let id = self.enqueue(spec);
        if let (Some(exe), Some(build)) = (exe, head) {
            self.after_job.entry(id).or_default().push(After::Built(exe, build));
        }
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

    /// Остановить мягко: окно — как крестиком, служба — Ctrl+Break. Не закрылась за 5 с — окно
    /// спросит, остановить ли принудительно (§5.12). Всё — в фоне.
    pub fn stop_run(&mut self, name: String, pid: u32) {
        let service = self.runs.iter().rev().find(|r| r.pid == pid && r.running()).is_some_and(|r| r.service);
        for run in self.runs.iter_mut().filter(|r| r.pid == pid && r.running()) {
            run.stopping = true;
        }
        let (tx, ctx) = (self.stop_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            crate::runs::soft_stop(pid, service);
            let note = if crate::procs::wait_exit(pid, Duration::from_secs(5)) {
                StopNote::Stopped(name)
            } else {
                StopNote::Timeout(name, pid)
            };
            let _ = tx.send(note);
            ctx.request_repaint();
        });
    }

    /// Остановить принудительно (после подтверждения) — вместе с дочерними процессами. Только если
    /// это всё ещё тот же процесс: иначе PID мог достаться другому.
    pub fn force_stop(&mut self, name: String, pid: u32, started: Option<i64>) {
        let now = self.procs.values().flatten().find(|r| r.pid == pid).map(|r| r.started);
        let same = match (now, started) {
            (None, _) => false,
            (Some(Some(a)), Some(b)) => (a - b).abs() <= 3,
            (Some(_), _) => true,
        };
        if !same {
            self.toasts.push(format!("{name} {}", t("остановлен")), Tone::Success);
            return;
        }
        let (tx, ctx) = (self.stop_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let note = match crate::runs::force_stop(pid) {
                Ok(()) => StopNote::Stopped(name),
                Err(e) => StopNote::Failed(name, e),
            };
            let _ = tx.send(note);
            ctx.request_repaint();
        });
    }

    /// Принудительно не останавливать: программа остаётся работать, и её конец снова считается
    /// настоящим (упадёт — будет «упал», а не «остановлен»).
    pub fn cancel_stop(&mut self, pid: u32) {
        for run in self.runs.iter_mut().filter(|r| r.pid == pid && r.running()) {
            run.stopping = false;
        }
    }

    /// Свой ли это запуск из кода: такой останавливается без вопроса.
    pub fn own_run(&self, pid: u32) -> bool {
        self.runs.iter().rev().find(|r| r.pid == pid && r.running()).is_some_and(|r| r.from_code)
    }

    // ─── Запуски ───────────────────────────────────────────────────────────────

    fn apply_run(&mut self, event: crate::runs::Event) {
        match event {
            crate::runs::Event::Started(run) => self.runs.push(*run),
            crate::runs::Event::Exited { id, code, at } => {
                let Some(run) = self.runs.iter_mut().find(|r| r.id == id) else { return };
                // Процесс закончился сам — спрашивать про принудительную остановку уже незачем.
                if self.force_confirm.as_ref().is_some_and(|(_, pid, _)| *pid == run.pid) {
                    self.force_confirm = None;
                }
                run.ended = Some(at);
                run.code = code;
                run.end = match (run.stopping, code) {
                    (true, _) => crate::runs::End::Stopped,
                    (false, Some(0)) => crate::runs::End::Closed,
                    (false, Some(_)) => crate::runs::End::Crashed,
                    (false, None) => crate::runs::End::Lost,
                };
                if run.end == crate::runs::End::Crashed {
                    run.seen = false;
                    let run = run.clone();
                    self.crashed(&run);
                }
            }
        }
        crate::runs::save(&self.runs_path, &self.runs);
    }

    /// Программа упала: уведомление Windows, если окно не впереди, иначе — в окне.
    fn crashed(&mut self, run: &crate::runs::Run) {
        let who = if run.profile.is_empty() { run.name.clone() } else { format!("{} · {}", run.name, run.profile) };
        let title = i18n::crashed(&who);
        let took = run.ended.unwrap_or(run.started) - run.started;
        let code = run.code.map(crate::runs::code_text).unwrap_or_default();
        let body = format!("{} · {} {code}", i18n::after_launch(took), t("код"));
        if !self.notifier.crash(title.clone(), capital(&body), &run.key) {
            self.toasts.push(format!("{title} — {body}"), Tone::Danger);
            // Уведомлять о падениях выключено — и из трея в Windows не пересылать.
            if !self.config.notify_crash {
                self.toasts_seen = Instant::now();
            }
        }
    }

    /// Падение предмета, которое ещё не видели (последнее по времени).
    pub fn unseen_crash(&self, key: &str) -> Option<&crate::runs::Run> {
        self.runs
            .iter()
            .filter(|r| r.key == key && r.end == crate::runs::End::Crashed && !r.seen)
            .max_by_key(|r| r.ended.unwrap_or(r.started))
    }

    /// Падение видели — погасить бейдж «упал».
    pub fn seen(&mut self, key: &str) {
        let mut changed = false;
        for run in self.runs.iter_mut().filter(|r| r.key == key && !r.seen) {
            run.seen = true;
            changed = true;
        }
        if changed {
            crate::runs::save(&self.runs_path, &self.runs);
        }
    }

    fn job_mut(&mut self, id: JobId) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    fn apply_job(&mut self, ctx: &egui::Context, event: jobs::Event) {
        match event {
            jobs::Event::Stopping(pid) => {
                for run in self.runs.iter_mut().filter(|r| r.pid == pid && r.running()) {
                    run.stopping = true;
                }
            }
            jobs::Event::StopTimedOut(pid) => {
                let name = self.runs.iter().rev().find(|r| r.pid == pid && r.running()).map(|r| r.name.clone());
                let started = self.procs.values().flatten().find(|r| r.pid == pid).and_then(|r| r.started);
                self.force_confirm = Some((name.unwrap_or_else(|| format!("PID {pid}")), pid, started));
            }
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
        }
    }

    fn finished(&mut self, ctx: &egui::Context, id: JobId, outcome: jobs::Outcome) {
        for after in self.after_job.remove(&id).unwrap_or_default() {
            match after {
                After::Deps(path) => self.check_deps(vec![path], true),
                After::Toolchain => self.check_toolchain(true),
                After::Launch(item) if outcome.ok && matches!(outcome.installed, Some(Ok(_))) => {
                    if let Some(bin) = &item.bin {
                        self.installs.insert(bin.clone(), installs::scan(bin));
                    }
                    // Только что поставленное — «обычным» профилем из установки, с меткой.
                    let profile = crate::deck::Profile {
                        name: String::new(),
                        source: crate::config::Source::Installed,
                        args: String::new(),
                        cwd: None,
                        env: Vec::new(),
                        port: None,
                    };
                    self.launch_installed_as(&item, &profile);
                }
                After::LaunchExe(name, exe, tag) if outcome.ok => {
                    self.start_tagged(&name, &exe, &[], None, tag);
                }
                After::Built(exe, mut build) if outcome.ok => {
                    // Время — конец сборки: по нему видно, не пересобрали ли exe потом без Anvil.
                    build.at = i18n::now();
                    self.build_info.insert(crate::builds::key(&exe), build);
                    crate::builds::save(&self.builds_path, &self.build_info);
                }
                _ => {}
            }
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
        // О долгой задаче Windows уже уведомил поток задач — второй раз из трея не пересылать.
        if took >= crate::notify::LONG && self.config.notify {
            self.toasts_seen = Instant::now();
        }
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
        self.after_job.entry(id).or_default().push(after);
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
/// Где встать окну быстрого запуска: по центру монитора под курсором, в верхней пятой части его
/// рабочей области (в точках egui).
#[cfg(windows)]
fn quick_position(size: egui::Vec2, ppp: f32) -> Option<egui::Pos2> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    // SAFETY: структуры на стеке, размер MONITORINFO задан перед вызовом.
    unsafe {
        let mut cursor = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut cursor) == 0 {
            return None;
        }
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let work = info.rcWork;
        let ppp = ppp.max(0.5);
        let (w, h) = ((work.right - work.left) as f32 / ppp, (work.bottom - work.top) as f32 / ppp);
        let (x, y) = (work.left as f32 / ppp, work.top as f32 / ppp);
        Some(egui::pos2(x + ((w - size.x) / 2.0).max(0.0), y + h * 0.2))
    }
}

#[cfg(not(windows))]
fn quick_position(_size: egui::Vec2, _ppp: f32) -> Option<egui::Pos2> {
    None
}

pub fn duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs} {}", t("с"))
    } else {
        format!("{} {} {:02} {}", secs / 60, t("мин"), secs % 60, t("с"))
    }
}

/// С заглавной: «через 12 с после запуска» в начале уведомления — «Через 12 с после запуска».
fn capital(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}
