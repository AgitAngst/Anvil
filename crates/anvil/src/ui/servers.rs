//! Вид «Серверы»: Linux-машины по SSH, серверы на них, установка модулей, обслуживание, журнал, консоль.
//!
//! Слева — машины (точка состояния, имя, адрес, число серверов) и «+»; справа — открытая машина: шапка, вкладки
//! «Обзор» / «Обслуживание» / «Журнал» / «Консоль» и внизу «Ход работы». Вся работа по SSH идёт в потоках
//! (`crate::servers`); здесь только вид, окна и вопросы владельцу: первое подключение с отпечатком ключа, «Ключ машины
//! сменился», подтверждения с командой дословно, форма установки по манифесту модуля.

use std::collections::BTreeMap;

use anvil_servers::{FieldKind, Manifest, SecretStore, SystemStore};
use anvil_ui::chrome;
use anvil_ui::theme::{radius, semibold};
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, Align, Align2, FontId, Layout, RichText, Ui, Vec2};

use crate::app::App;
use crate::i18n::{self, t};
use crate::servers::{Job, Login, Machine, Prompt, Seen, Verb};

/// Ширина списка машин.
const LIST_WIDTH: f32 = 250.0;

/// Высота «Хода работы» внизу вида (подпись и окно строк).
const LOG_HEIGHT: f32 = 176.0;

/// Вкладки открытой машины.
const TAB_OVERVIEW: usize = 0;
const TAB_MAINTENANCE: usize = 1;
const TAB_JOURNAL: usize = 2;

/// Как входить — выбор в окне машины.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginKind {
    #[default]
    Password,
    Key,
    Agent,
}

/// Окно «Новый сервер» / «Изменить сервер».
#[derive(Debug, Clone, Default)]
pub struct Edit {
    /// Номер машины, если её изменяют.
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub kind: LoginKind,
    pub password: String,
    pub key_path: String,
    pub passphrase: String,
    pub error: Option<String>,
}

impl Edit {
    fn new() -> Edit {
        Edit { port: crate::servers::SSH_PORT.to_string(), user: "root".to_owned(), ..Edit::default() }
    }

    fn of(machine: &Machine) -> Edit {
        let (kind, key_path) = match &machine.login {
            Login::Password => (LoginKind::Password, String::new()),
            Login::Key(path) => (LoginKind::Key, path.to_string_lossy().into_owned()),
            Login::Agent => (LoginKind::Agent, String::new()),
        };
        Edit {
            id: Some(machine.id.clone()),
            name: machine.name.clone(),
            host: machine.host.clone(),
            port: machine.port.to_string(),
            user: machine.user.clone(),
            kind,
            key_path,
            ..Edit::default()
        }
    }

    /// Запись из полей окна или причина, по которой она не годится.
    fn machine(&self, others: &[Machine]) -> Result<Machine, String> {
        let host = self.host.trim();
        if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(t("Введите адрес машины без пробелов: имя или IP-адрес.").to_owned());
        }
        let port: u16 = match self.port.trim().parse() {
            Ok(p) if p > 0 => p,
            _ => return Err(t("Порт SSH — число от 1 до 65535.").to_owned()),
        };
        let user = self.user.trim();
        if user.is_empty() || user.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(t("Введите логин без пробелов.").to_owned());
        }
        let name = if self.name.trim().is_empty() { host.to_owned() } else { self.name.trim().to_owned() };
        if others.iter().any(|m| m.name == name && Some(&m.id) != self.id.as_ref()) {
            return Err(t("Машина с таким названием уже есть.").to_owned());
        }
        let login = match self.kind {
            LoginKind::Password => Login::Password,
            LoginKind::Agent => Login::Agent,
            LoginKind::Key => {
                if self.key_path.trim().is_empty() {
                    return Err(t("Укажите файл закрытого ключа.").to_owned());
                }
                Login::Key(self.key_path.trim().into())
            }
        };
        Ok(Machine { name, host: host.to_owned(), port, user: user.to_owned(), login, ..Machine::default() })
    }
}

/// Окно установки: выбор модуля, затем форма из его манифеста.
#[derive(Debug, Clone, Default)]
pub struct Install {
    pub machine: String,
    pub module: Option<String>,
    /// Имя экземпляра (пусто — основной).
    pub instance: String,
    pub form: BTreeMap<String, String>,
    /// Обновление уже поставленного сервера: модуль и экземпляр выбраны.
    pub update: bool,
    pub error: Option<String>,
}

/// Подтверждение, которое ждёт владельца.
#[derive(Debug, Clone)]
pub enum Ask {
    /// Действие манифеста: команда дословно.
    Action { machine: String, module: String, instance: Option<String>, index: usize, label: String, command: String },
    /// Команда консоли.
    Console { machine: String, command: String },
    /// Удаление сервера: нужно набрать его имя.
    Uninstall {
        machine: String,
        module: String,
        instance: Option<String>,
        name: String,
        command: String,
        data_dir: String,
        typed: String,
    },
    /// Убрать машину из списка Anvil.
    Remove { machine: String },
}

/// Состояние вида (на диск не пишется).
#[derive(Default)]
pub struct View {
    pub selected: Option<String>,
    pub tab: usize,
    /// Какой сервер открыт на вкладках «Обслуживание» и «Журнал».
    pub module: Option<(String, Option<String>)>,
    pub edit: Option<Edit>,
    pub install: Option<Install>,
    pub ask: Option<Ask>,
    /// Строка консоли.
    pub console: String,
}

impl View {
    /// Если открытой машины нет в списке — открыть первую.
    pub fn select_first(&mut self, machines: &[Machine]) {
        if self.selected.as_ref().is_none_or(|id| !machines.iter().any(|m| &m.id == id)) {
            self.selected = machines.first().map(|m| m.id.clone());
        }
    }
}

/// «Имя» в кавычках языка окна.
fn q(name: &str) -> String {
    if i18n::is_english() { format!("\"{name}\"") } else { format!("«{name}»") }
}

fn manifest(app: &App, module: &str) -> Option<Manifest> {
    app.servers.registry.get(module, true).map(|m| m.manifest.clone())
}

fn module_title(app: &App, seen: &Seen) -> String {
    let name = manifest(app, &seen.module).map_or_else(|| seen.module.clone(), |m| m.name);
    match &seen.instance {
        Some(instance) => format!("{name} · {instance}"),
        None => name,
    }
}

/// Найденные на машине серверы, которые сейчас видно: скрытые модули — только при включённом переключателе.
fn shown(app: &App, machine: &Machine) -> Vec<Seen> {
    let show = app.config.show_hidden_modules;
    machine.modules.iter().filter(|s| app.servers.shown(&s.module, show)).cloned().collect()
}

// ─── Вид ───────────────────────────────────────────────────────────────────

pub fn show(app: &mut App, ui: &mut Ui) {
    // Порт оказался занят — назад в форму установки, с причиной под ней.
    if let Some(r) = app.servers.reopen.take() {
        app.servers_view.selected = Some(r.machine.clone());
        app.servers_view.install = Some(Install {
            machine: r.machine,
            module: Some(r.module),
            instance: r.instance.unwrap_or_default(),
            form: r.form,
            update: r.update,
            error: Some(r.message),
        });
    }
    app.servers_view.select_first(&app.config.servers);
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        w::title(ui, t("Серверы"), 22.0);
        if app.servers.any_busy() {
            w::spinner(ui, 14.0);
        }
    });
    w::note(
        ui,
        t(
            "Linux-машины по SSH: Anvil ставит на них серверы ваших программ, обновляет их и показывает журнал. Только проверка порта и ваши команды — в «Парке серверов».",
        ),
    );
    ui.add_space(12.0);
    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(Vec2::new(LIST_WIDTH, height), Layout::top_down(Align::LEFT), |ui| {
            machine_list(app, ui, &p);
        });
        ui.add_space(6.0);
        let width = ui.available_width();
        ui.allocate_ui_with_layout(Vec2::new(width, height), Layout::top_down(Align::LEFT), |ui| {
            detail(app, ui, &p);
        });
    });
}

fn machine_list(app: &mut App, ui: &mut Ui, p: &Palette) {
    let mut open = None;
    let mut add = false;
    w::card_frame(ui).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(LIST_WIDTH - 18.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            w::section_label(ui, t("Машины"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if w::icon_button(ui, Icon::Plus, t("Новый сервер…")).clicked() {
                    add = true;
                }
            });
        });
        ui.add_space(4.0);
        if app.config.servers.is_empty() {
            w::note(ui, t("Машин пока нет. Добавьте адрес, логин и способ входа."));
            ui.add_space(6.0);
            if w::button(ui, Kind::Secondary, Some(Icon::Plus), t("Новый сервер…")).clicked() {
                add = true;
            }
        }
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            for machine in &app.config.servers {
                let runtime = app.servers.runtime(&machine.id);
                let busy = runtime.is_some_and(|r| r.busy.is_some());
                let failed = runtime.is_some_and(|r| r.error.is_some());
                let show = app.config.show_hidden_modules;
                let tone = machine.tone(|s| !app.servers.shown(&s.module, show), failed, busy);
                let count = machine.modules.iter().filter(|s| app.servers.shown(&s.module, show)).count();
                let selected = app.servers_view.selected.as_ref() == Some(&machine.id);
                if machine_row(ui, p, machine, selected, tone, count).clicked() {
                    open = Some(machine.id.clone());
                }
            }
        });
    });
    if let Some(id) = open {
        app.servers_view.selected = Some(id);
        app.servers_view.module = None;
    }
    if add {
        app.servers_view.edit = Some(Edit::new());
    }
}

/// Строка машины: точка состояния, имя, под ним адрес, справа число серверов.
fn machine_row(
    ui: &mut Ui,
    p: &Palette,
    machine: &Machine,
    selected: bool,
    tone: Tone,
    count: usize,
) -> egui::Response {
    let id = ui.make_persistent_id(("servers-row", &machine.id));
    let label = format!("{}, {}", machine.name, machine.address());
    let response = w::list_row(ui, id, selected, 52.0, &label);
    let rect = response.rect;
    let pill = if count > 0 { 34.0 } else { 0.0 };
    let clip = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 30.0, rect.top()),
        egui::pos2(rect.right() - 10.0 - pill, rect.bottom()),
    );
    let painter = ui.painter_at(clip);
    ui.painter().circle_filled(egui::pos2(rect.left() + 16.0, rect.center().y - 9.0), 4.0, tone.color(p));
    let font = if selected { semibold(14.0) } else { FontId::proportional(14.0) };
    painter.text(egui::pos2(clip.left(), rect.top() + 17.0), Align2::LEFT_CENTER, &machine.name, font, p.text);
    painter.text(
        egui::pos2(clip.left(), rect.top() + 37.0),
        Align2::LEFT_CENTER,
        machine.target(),
        FontId::monospace(12.0),
        p.weak,
    );
    if count > 0 {
        let color = Tone::Neutral.color(p);
        let galley = ui.painter().layout_no_wrap(count.to_string(), semibold(11.5), color);
        let size = Vec2::new(galley.size().x + 12.0, 18.0);
        let pill = egui::Rect::from_center_size(egui::pos2(rect.right() - 12.0 - size.x / 2.0, rect.center().y), size);
        ui.painter().rect_filled(pill, 9, p.badge_fill(color));
        ui.painter().galley(pill.center() - galley.size() / 2.0, galley, color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Что владелец нажал на странице машины.
enum Act {
    Check,
    Install,
    Edit,
    Remove,
    Service(Seen, Verb),
    Update(Seen),
    Backup(Seen),
    Uninstall(Seen),
    Action(Seen, usize),
    Journal(Seen),
    Console(String),
    OpenJournal(Seen),
    CopyLog,
}

fn detail(app: &mut App, ui: &mut Ui, p: &Palette) {
    let Some(machine) =
        app.servers_view.selected.as_ref().and_then(|id| app.config.servers.iter().find(|m| &m.id == id)).cloned()
    else {
        w::empty_state(
            ui,
            Icon::Terminal,
            t("Машина не выбрана"),
            t("Добавьте машину кнопкой «+» слева: Anvil подключится по SSH и покажет, какие серверы на ней стоят."),
        );
        return;
    };
    let mut act = None;
    let runtime = app.servers.runtime(&machine.id);
    let busy = runtime.and_then(|r| r.busy);
    let error = runtime.and_then(|r| r.error.clone());
    let modules = shown(app, &machine);
    let can_install = app.servers.registry.modules(app.config.show_hidden_modules).next().is_some();

    // Шапка: имя и состояние; главная кнопка левее остальных действий.
    ui.horizontal(|ui| {
        w::title(ui, &machine.name, 22.0);
        match (busy, &error) {
            (Some(what), _) => {
                w::badge(ui, what, Tone::Accent);
            }
            (None, Some(_)) => {
                w::badge(ui, t("Ошибка"), Tone::Danger);
            }
            (None, None) if machine.fingerprint.is_empty() => {
                w::badge(ui, t("Ключ не подтверждён"), Tone::Warning);
            }
            (None, None) if machine.checked == 0 => {
                w::badge(ui, t("Не проверена"), Tone::Neutral);
            }
            (None, None) => {
                w::badge(ui, t("Проверена"), Tone::Success);
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let more = w::icon_button(ui, Icon::More, t("Ещё"));
            let entries = [(t("Изменить…"), None), (t("Убрать из списка…"), None)];
            w::menu_fit(&more, 190.0, &entries, |ui| {
                if w::menu_item(ui, Some(Icon::Pencil), t("Изменить…"), None).clicked() {
                    act = Some(Act::Edit);
                }
                w::menu_separator(ui);
                if w::menu_item_danger(ui, Some(Icon::Trash), t("Убрать из списка…")).clicked() {
                    act = Some(Act::Remove);
                }
            });
            let wait = t("Подождите: на машине идёт работа.");
            if gated(ui, busy.is_none(), wait, |ui| w::button(ui, Kind::Secondary, Some(Icon::Refresh), t("Проверить")))
            {
                act = Some(Act::Check);
            }
            let why = if can_install {
                wait
            } else {
                t("Нет ни одного модуля: положите манифест в папку modules (Настройки → Кузница → Серверы).")
            };
            if gated(ui, busy.is_none() && can_install, why, |ui| {
                w::button(ui, Kind::Primary, Some(Icon::Plus), t("Поставить сервер…"))
            }) {
                act = Some(Act::Install);
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        w::mono(ui, &machine.target(), Some(p.weak));
        if !machine.arch.is_empty() {
            w::note(ui, format!("· {}", machine.arch));
        }
        if machine.checked > 0 {
            w::note(ui, format!("· {} {}", t("проверена"), i18n::ago(machine.checked)));
        }
    });
    ui.add_space(8.0);
    if let Some(error) = &error {
        w::banner(ui, Tone::Danger, t("Не получилось"), error, |_| {});
        ui.add_space(8.0);
    }

    let mut tab = app.servers_view.tab;
    w::tabs(ui, &mut tab, &[t("Обзор"), t("Обслуживание"), t("Журнал"), t("Консоль")]);
    app.servers_view.tab = tab;
    ui.add_space(10.0);

    let body_height = (ui.available_height() - LOG_HEIGHT - 8.0).max(120.0);
    egui::ScrollArea::vertical().id_salt("servers-body").max_height(body_height).auto_shrink([false, true]).show(
        ui,
        |ui| match tab {
            TAB_OVERVIEW => overview(app, ui, p, &machine, &modules, busy.is_some(), &mut act),
            TAB_MAINTENANCE => maintenance(app, ui, p, &machine, &modules, busy.is_some(), &mut act),
            TAB_JOURNAL => journal(app, ui, p, &machine, &modules, busy.is_some(), &mut act),
            _ => console(app, ui, p, &machine, busy.is_some(), &mut act),
        },
    );
    ui.add_space(8.0);
    work_log(app, ui, p, &machine, &mut act);
    if let Some(Act::CopyLog) = act {
        if let Some(runtime) = app.servers.runtime(&machine.id) {
            ui.ctx().copy_text(runtime.log.iter().cloned().collect::<Vec<_>>().join("\n"));
            app.toasts.push(t("Ход работы скопирован"), Tone::Success);
        }
        return;
    }
    if let Some(act) = act {
        apply(app, &machine, act);
    }
}

/// Кнопка, которая бывает недоступной: серая и с причиной в подсказке. Возвращает, нажали ли её.
fn gated(ui: &mut Ui, enabled: bool, why: &str, button: impl FnOnce(&mut Ui) -> egui::Response) -> bool {
    ui.add_enabled_ui(enabled, |ui| button(ui).on_disabled_hover_text(why).clicked()).inner
}

fn state_badge(ui: &mut Ui, running: Option<bool>) {
    match running {
        Some(true) => w::badge(ui, t("Работает"), Tone::Success),
        Some(false) => w::badge(ui, t("Остановлен"), Tone::Warning),
        None => w::badge(ui, t("Состояние неизвестно"), Tone::Neutral),
    };
}

fn overview(
    app: &App,
    ui: &mut Ui,
    p: &Palette,
    machine: &Machine,
    modules: &[Seen],
    busy: bool,
    act: &mut Option<Act>,
) {
    if modules.is_empty() {
        let text = if machine.fingerprint.is_empty() {
            t(
                "Ключ машины ещё не подтверждён. Нажмите «Проверить»: Anvil покажет отпечаток, и после вашего подтверждения подключится.",
            )
        } else if machine.checked == 0 {
            t("Машину ещё не проверяли. Нажмите «Проверить»: Anvil найдёт, какие серверы на ней стоят.")
        } else {
            t("Anvil не нашёл на этой машине известных серверов. Поставьте сервер кнопкой «Поставить сервер…».")
        };
        w::empty_state(ui, Icon::Server, t("Серверов нет"), text);
        return;
    }
    for seen in modules {
        let known = manifest(app, &seen.module);
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Server, &module_title(app, seen));
            ui.horizontal_wrapped(|ui| {
                state_badge(ui, seen.running);
                if let Some(version) = &seen.version {
                    w::mono(ui, version, Some(p.weak));
                }
                w::note(ui, format!("· {} {}", t("проверен"), i18n::ago(seen.checked)));
            });
            ui.add_space(6.0);
            if let Some(address) = &seen.address {
                w::field_row(ui, t("Адрес"), |ui| {
                    w::mono(ui, address, Some(p.text));
                });
            }
            if let Some(fingerprint) = &seen.fingerprint {
                w::field_row(ui, t("Отпечаток"), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(fingerprint).font(FontId::monospace(12.5)).color(p.text))
                            .truncate(),
                    )
                    .on_hover_text(fingerprint);
                });
            }
            for (key, value) in &seen.extra {
                w::field_row(ui, key, |ui| {
                    w::mono(ui, value, Some(p.text));
                });
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(!busy && known.is_some(), |ui| {
                    if seen.running == Some(false) {
                        if w::button(ui, Kind::Secondary, Some(Icon::Play), t("Запустить")).clicked() {
                            *act = Some(Act::Service(seen.clone(), Verb::Start));
                        }
                    } else {
                        if w::button(ui, Kind::Secondary, Some(Icon::Refresh), t("Перезапустить")).clicked()
                        {
                            *act = Some(Act::Service(seen.clone(), Verb::Restart));
                        }
                        if w::button(ui, Kind::Ghost, Some(Icon::Stop), t("Остановить")).clicked() {
                            *act = Some(Act::Service(seen.clone(), Verb::Stop));
                        }
                    }
                });
                if w::button(ui, Kind::Ghost, Some(Icon::File), t("Журнал")).clicked() {
                    *act = Some(Act::OpenJournal(seen.clone()));
                }
            });
            if known.is_none() {
                w::note(ui, t("Манифеста этого сервера нет в папке modules: управлять им из Anvil нельзя."));
            }
        });
        ui.add_space(12.0);
    }
}

/// Какой сервер открыт на вкладке: выбранный, иначе первый. При нескольких серверах — выбор сверху.
fn pick<'a>(app: &mut App, ui: &mut Ui, modules: &'a [Seen]) -> Option<&'a Seen> {
    let current = app
        .servers_view
        .module
        .as_ref()
        .and_then(|(m, i)| modules.iter().position(|s| &s.module == m && &s.instance == i))
        .unwrap_or(0);
    let mut index = current;
    if modules.len() > 1 {
        let titles: Vec<String> = modules.iter().map(|s| module_title(app, s)).collect();
        let options: Vec<(usize, &str)> = titles.iter().enumerate().map(|(i, name)| (i, name.as_str())).collect();
        ui.horizontal(|ui| {
            w::note(ui, t("Сервер"));
            w::select(ui, &mut index, &options, 240.0);
        });
        ui.add_space(8.0);
    }
    let seen = modules.get(index)?;
    app.servers_view.module = Some((seen.module.clone(), seen.instance.clone()));
    Some(seen)
}

fn no_modules(ui: &mut Ui) {
    w::empty_state(ui, Icon::Server, t("Серверов нет"), t("Сначала поставьте сервер или проверьте машину."));
}

fn maintenance(
    app: &mut App,
    ui: &mut Ui,
    p: &Palette,
    machine: &Machine,
    modules: &[Seen],
    busy: bool,
    act: &mut Option<Act>,
) {
    let _ = machine;
    let Some(seen) = pick(app, ui, modules) else { return no_modules(ui) };
    let Some(m) = manifest(app, &seen.module) else {
        w::note(ui, t("Манифеста этого сервера нет в папке modules: обслуживать его из Anvil нельзя."));
        return;
    };
    w::card(ui, |ui| {
        w::card_title(ui, Icon::Download, t("Обновление и копия"));
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal_wrapped(|ui| {
                if w::button(ui, Kind::Secondary, Some(Icon::Download), t("Обновить…")).clicked() {
                    *act = Some(Act::Update(seen.clone()));
                }
                let has_backup = m.commands.backup.is_some();
                if gated(ui, has_backup, t("В манифесте нет команды backup."), |ui| {
                    w::button(ui, Kind::Secondary, Some(Icon::Save), t("Сделать копию"))
                }) {
                    *act = Some(Act::Backup(seen.clone()));
                }
            });
        });
        ui.add_space(4.0);
        w::note(
            ui,
            t(
                "Обновление ставит последний выпуск поверх: файл проверяется по SHA256SUMS. Копия ложится на самой машине, в /var/backups/anvil.",
            ),
        );
    });
    ui.add_space(12.0);
    if !m.actions.is_empty() {
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Terminal, t("Действия"));
            for (index, action) in m.actions.iter().enumerate() {
                let command = m
                    .action_command(index, &anvil_servers::Context { instance: seen.instance.as_deref(), dir: None })
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        let label = if action.confirm { format!("{}…", action.label) } else { action.label.clone() };
                        if w::button(ui, Kind::Secondary, None, &label).clicked() {
                            *act = Some(Act::Action(seen.clone(), index));
                        }
                    });
                    ui.add(
                        egui::Label::new(RichText::new(&command).font(FontId::monospace(12.0)).color(p.weak))
                            .truncate(),
                    )
                    .on_hover_text(&command);
                });
                ui.add_space(4.0);
            }
        });
        ui.add_space(12.0);
    }
    w::card(ui, |ui| {
        w::card_title(ui, Icon::Trash, t("Удаление"));
        w::note(
            ui,
            t("Выполнит команду uninstall этого сервера. Вернуть удалённое нельзя; команду вы увидите дословно."),
        );
        ui.add_space(6.0);
        let why = if busy {
            t("Подождите: на машине идёт работа.")
        } else {
            t("В манифесте нет команды uninstall.")
        };
        if gated(ui, !busy && m.commands.uninstall.is_some(), why, |ui| {
            w::button(ui, Kind::Danger, Some(Icon::Trash), t("Удалить сервер…"))
        }) {
            *act = Some(Act::Uninstall(seen.clone()));
        }
    });
}

fn journal(
    app: &mut App,
    ui: &mut Ui,
    p: &Palette,
    machine: &Machine,
    modules: &[Seen],
    busy: bool,
    act: &mut Option<Act>,
) {
    let Some(seen) = pick(app, ui, modules) else { return no_modules(ui) };
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!busy, |ui| {
            if w::button(ui, Kind::Secondary, Some(Icon::Refresh), t("Обновить журнал")).clicked() {
                *act = Some(Act::Journal(seen.clone()));
            }
        });
        w::note(ui, t("Последние строки journalctl службы сервера."));
    });
    ui.add_space(6.0);
    let lines = app.servers.runtime(&machine.id).map(|r| r.journal.clone()).unwrap_or_default();
    if lines.is_empty() {
        w::note(ui, t("Журнал не загружен: нажмите «Обновить журнал»."));
        return;
    }
    let text = lines.join("\n");
    mono_box(ui, p, &text, 360.0, "servers-journal");
}

fn console(app: &mut App, ui: &mut Ui, p: &Palette, machine: &Machine, busy: bool, act: &mut Option<Act>) {
    w::note(
        ui,
        t("Команда выполняется на машине от root. Перед запуском Anvil покажет её дословно и спросит подтверждение."),
    );
    ui.add_space(6.0);
    let mut submit = false;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled_ui(!busy && !app.servers_view.console.trim().is_empty(), |ui| {
                if w::button(ui, Kind::Secondary, Some(Icon::Terminal), t("Выполнить…")).clicked() {
                    submit = true;
                }
            });
            let edit = egui::TextEdit::singleline(&mut app.servers_view.console)
                .font(FontId::monospace(13.0))
                .hint_text("uptime")
                .desired_width(ui.available_width());
            let response = ui.add(edit);
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !busy {
                submit = true;
            }
        });
    });
    if submit && !app.servers_view.console.trim().is_empty() {
        *act = Some(Act::Console(app.servers_view.console.trim().to_owned()));
    }
    ui.add_space(8.0);
    let lines = app.servers.runtime(&machine.id).map(|r| r.console.clone()).unwrap_or_default();
    if lines.is_empty() {
        w::note(ui, t("Вывод команд появится здесь."));
    } else {
        mono_box(ui, p, &lines.join("\n"), 300.0, "servers-console");
    }
}

/// Моноширинный вывод в рамке: можно выделить и скопировать.
fn mono_box(ui: &mut Ui, p: &Palette, text: &str, height: f32, id: &str) {
    egui::Frame::new()
        .fill(p.field)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(radius::CONTROL)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::both()
                .id_salt(id)
                .max_height(height)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(text).font(FontId::monospace(12.5)).color(p.text))
                            .selectable(true),
                    );
                });
        });
}

/// «Ход работы»: до 400 строк на машину, «Скопировать».
fn work_log(app: &App, ui: &mut Ui, p: &Palette, machine: &Machine, act: &mut Option<Act>) {
    let runtime = app.servers.runtime(&machine.id);
    ui.horizontal(|ui| {
        w::section_label(ui, t("Ход работы"));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled_ui(runtime.is_some_and(|r| !r.log.is_empty()), |ui| {
                if w::button(ui, Kind::Ghost, Some(Icon::Copy), t("Скопировать")).clicked() {
                    *act = Some(Act::CopyLog);
                }
            });
            if let Some(runtime) = runtime
                && let Some(what) = runtime.busy
            {
                match runtime.progress {
                    Some((got, total)) if total > 0 => {
                        w::progress(ui, Some(got as f32 / total as f32), 120.0);
                    }
                    _ => {
                        w::spinner(ui, 14.0);
                    }
                }
                w::note(ui, what);
            }
        });
    });
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(p.field)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(radius::CONTROL)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(LOG_HEIGHT - 52.0);
            egui::ScrollArea::vertical().id_salt("servers-log").stick_to_bottom(true).auto_shrink([false, false]).show(
                ui,
                |ui| match runtime {
                    Some(runtime) if !runtime.log.is_empty() => {
                        for line in &runtime.log {
                            ui.add(
                                egui::Label::new(RichText::new(line).font(FontId::monospace(12.5)).color(p.text))
                                    .selectable(true),
                            );
                        }
                    }
                    _ => {
                        w::note(ui, t("Здесь пойдёт ход установки и других дел на машине."));
                    }
                },
            );
        });
}

fn apply(app: &mut App, machine: &Machine, act: Act) {
    let show = app.config.show_hidden_modules;
    let start = |app: &mut App, job: Job| {
        app.servers.start(machine, job, show);
    };
    match act {
        Act::Check => start(app, Job::Check),
        Act::Install => {
            app.servers_view.install = Some(Install { machine: machine.id.clone(), ..Install::default() });
        }
        Act::Edit => app.servers_view.edit = Some(Edit::of(machine)),
        Act::Remove => app.servers_view.ask = Some(Ask::Remove { machine: machine.id.clone() }),
        Act::Service(seen, verb) => {
            start(app, Job::Service { module: seen.module, instance: seen.instance, verb });
        }
        Act::Update(seen) => {
            let form = default_form(app, &seen.module);
            app.servers_view.install = Some(Install {
                machine: machine.id.clone(),
                module: Some(seen.module),
                instance: seen.instance.unwrap_or_default(),
                form,
                update: true,
                error: None,
            });
        }
        Act::Backup(seen) => start(app, Job::Backup { module: seen.module, instance: seen.instance }),
        Act::Uninstall(seen) => {
            let Some(m) = manifest(app, &seen.module) else { return };
            let ctx = anvil_servers::Context { instance: seen.instance.as_deref(), dir: None };
            let Ok(command) = m.command(anvil_servers::CommandKind::Uninstall, &BTreeMap::new(), &ctx) else { return };
            let data_dir = m.naming(seen.instance.as_deref()).map(|n| n.data_dir).unwrap_or_default();
            app.servers_view.ask = Some(Ask::Uninstall {
                machine: machine.id.clone(),
                name: seen.instance.clone().unwrap_or_else(|| m.name.clone()),
                module: seen.module,
                instance: seen.instance,
                command,
                data_dir,
                typed: String::new(),
            });
        }
        Act::Action(seen, index) => {
            let Some(m) = manifest(app, &seen.module) else { return };
            let ctx = anvil_servers::Context { instance: seen.instance.as_deref(), dir: None };
            let Ok(command) = m.action_command(index, &ctx) else { return };
            let label = m.actions.get(index).map(|a| a.label.clone()).unwrap_or_default();
            if m.actions.get(index).is_some_and(|a| a.confirm) {
                app.servers_view.ask = Some(Ask::Action {
                    machine: machine.id.clone(),
                    module: seen.module,
                    instance: seen.instance,
                    index,
                    label,
                    command,
                });
            } else {
                start(app, Job::Action { module: seen.module, instance: seen.instance, index });
            }
        }
        Act::Journal(seen) => start(app, Job::Journal { module: seen.module, instance: seen.instance }),
        Act::OpenJournal(seen) => {
            app.servers_view.module = Some((seen.module.clone(), seen.instance.clone()));
            app.servers_view.tab = TAB_JOURNAL;
            start(app, Job::Journal { module: seen.module, instance: seen.instance });
        }
        Act::Console(command) => {
            app.servers_view.ask = Some(Ask::Console { machine: machine.id.clone(), command });
        }
        Act::CopyLog => {}
    }
}

/// Значения формы по умолчанию: у переключателей — их исходное состояние.
fn default_form(app: &App, module: &str) -> BTreeMap<String, String> {
    let mut form = BTreeMap::new();
    if let Some(m) = manifest(app, module) {
        for field in m.form.iter().filter(|f| f.kind == FieldKind::Bool) {
            form.insert(field.key.clone(), field.default.clone().unwrap_or_else(|| "false".to_owned()));
        }
    }
    form
}

// ─── Окна поверх ───────────────────────────────────────────────────────────

pub fn dialogs(app: &mut App, ctx: &egui::Context) {
    edit_dialog(app, ctx);
    install_dialog(app, ctx);
    ask_dialog(app, ctx);
    prompt_dialog(app, ctx);
}

fn error_line(ui: &mut Ui, text: &str) {
    let p = Palette::of(ui);
    ui.horizontal_wrapped(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
        anvil_ui::icons::paint(ui.painter(), rect, Icon::Warning, p.danger);
        ui.label(RichText::new(text).size(13.0).color(p.danger));
    });
}

fn edit_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut edit) = app.servers_view.edit.take() else { return };
    let mut open = true;
    let mut save = false;
    let title = if edit.id.is_some() { t("Изменить сервер") } else { t("Новый сервер") };
    let editing = edit.id.is_some();
    chrome::dialog(ctx, "anvil-servers-edit", title, 540.0, &mut open, |ui| {
        w::section_label(ui, t("Адрес"));
        ui.add(
            egui::TextEdit::singleline(&mut edit.host)
                .hint_text("203.0.113.7  /  server.example.org")
                .desired_width(ui.available_width()),
        );
        ui.add_space(8.0);
        w::section_label(ui, t("Название"));
        ui.add(
            egui::TextEdit::singleline(&mut edit.name)
                .hint_text(if edit.host.trim().is_empty() {
                    t("как показывать в списке")
                } else {
                    edit.host.trim()
                })
                .desired_width(ui.available_width()),
        );
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            w::section_label(&mut cols[0], t("Логин"));
            let width = cols[0].available_width();
            cols[0].add(egui::TextEdit::singleline(&mut edit.user).desired_width(width));
            w::section_label(&mut cols[1], t("Порт SSH"));
            let width = cols[1].available_width();
            cols[1].add(egui::TextEdit::singleline(&mut edit.port).desired_width(width));
        });
        ui.add_space(8.0);
        w::section_label(ui, t("Как входить"));
        w::segmented(
            ui,
            &mut edit.kind,
            &[
                (LoginKind::Password, None, t("Пароль")),
                (LoginKind::Key, None, t("Файл ключа")),
                (LoginKind::Agent, None, t("ssh-agent")),
            ],
        );
        ui.add_space(8.0);
        match edit.kind {
            LoginKind::Password => {
                w::section_label(ui, t("Пароль"));
                ui.add(
                    egui::TextEdit::singleline(&mut edit.password)
                        .password(true)
                        .hint_text(if editing {
                            t("оставьте пустым, чтобы не менять")
                        } else {
                            ""
                        })
                        .desired_width(ui.available_width()),
                );
                w::note(ui, t("Пароль хранится в хранилище учётных данных системы, не в настройках Anvil."));
            }
            LoginKind::Key => {
                w::section_label(ui, t("Файл закрытого ключа"));
                ui.horizontal(|ui| {
                    if w::button(ui, Kind::Secondary, Some(Icon::Folder), t("Выбрать…")).clicked()
                        && let Some(path) = rfd::FileDialog::new().set_title(t("Файл закрытого ключа")).pick_file()
                    {
                        edit.key_path = path.to_string_lossy().into_owned();
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut edit.key_path)
                            .font(FontId::monospace(12.5))
                            .desired_width(ui.available_width()),
                    );
                });
                ui.add_space(6.0);
                w::section_label(ui, t("Фраза ключа (если есть)"));
                ui.add(
                    egui::TextEdit::singleline(&mut edit.passphrase)
                        .password(true)
                        .hint_text(if editing {
                            t("оставьте пустым, чтобы не менять")
                        } else {
                            ""
                        })
                        .desired_width(ui.available_width()),
                );
            }
            LoginKind::Agent => {
                w::note(
                    ui,
                    t("Ключи берутся у ssh-agent: службы OpenSSH Authentication Agent или Pageant. Пароль не нужен."),
                );
            }
        }
        ui.add_space(12.0);
        if let Some(error) = &edit.error {
            error_line(ui, error);
            ui.add_space(6.0);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if editing { t("Сохранить") } else { t("Добавить") };
            if w::button(ui, Kind::Primary, None, label).clicked() {
                save = true;
            }
        });
    });
    if save {
        match edit.machine(&app.config.servers) {
            Ok(built) => {
                finish_edit(app, edit, built);
                return;
            }
            Err(error) => edit.error = Some(error),
        }
    }
    if open {
        app.servers_view.edit = Some(edit);
    }
}

/// Записать машину в настройки, секреты — в хранилище учётных данных, и проверить её.
fn finish_edit(app: &mut App, edit: Edit, mut built: Machine) {
    let store = SystemStore::default();
    let mut secret_problem = None;
    match edit.id.as_deref().and_then(|id| app.config.servers.iter().position(|m| m.id == id)) {
        Some(index) => {
            let old = &mut app.config.servers[index];
            built.id = old.id.clone();
            built.modules = std::mem::take(&mut old.modules);
            built.arch = std::mem::take(&mut old.arch);
            built.checked = old.checked;
            // Другой адрес или порт — другая машина: ключ подтверждается заново.
            if old.host == built.host && old.port == built.port {
                built.fingerprint = std::mem::take(&mut old.fingerprint);
            }
            *old = built.clone();
        }
        None => {
            built.id = crate::servers::new_id();
            app.config.servers.push(built.clone());
        }
    }
    let secret = match edit.kind {
        LoginKind::Password => Some(edit.password.as_str()),
        LoginKind::Key => Some(edit.passphrase.as_str()),
        LoginKind::Agent => None,
    };
    match secret {
        Some(value) if !value.is_empty() => {
            if let Err(e) = store.set_machine(&built.id, value) {
                secret_problem = Some(e.to_string());
            }
        }
        Some(_) => {}
        None => {
            let _ = store.forget_machine(&built.id);
        }
    }
    if let Some(problem) = secret_problem {
        app.toasts.push(format!("{}: {problem}", t("Пароль не сохранён")), Tone::Danger);
    }
    app.servers_view.selected = Some(built.id.clone());
    app.save();
    let show = app.config.show_hidden_modules;
    app.servers.start(&built, Job::Check, show);
}

fn install_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut install) = app.servers_view.install.take() else { return };
    let Some(machine) = app.config.servers.iter().find(|m| m.id == install.machine).cloned() else { return };
    let show = app.config.show_hidden_modules;
    let modules: Vec<Manifest> = app.servers.registry.modules(show).map(|m| m.manifest.clone()).collect();
    let chosen = install.module.as_ref().and_then(|id| manifest(app, id));
    let mut open = true;
    let mut go = false;
    let mut back = false;
    let mut choose = None;
    let folder = crate::servers::modules_dir(&app.config_path);
    let title = match (&chosen, install.update) {
        (Some(m), true) => format!("{} {}", t("Обновить"), q(&m.name)),
        (Some(m), false) => format!("{} {}", t("Поставить"), q(&m.name)),
        (None, _) => format!("{} {}", t("Поставить сервер на"), q(&machine.name)),
    };
    chrome::dialog(ctx, "anvil-servers-install", &title, 560.0, &mut open, |ui| {
        let Some(m) = &chosen else {
            // Шаг 1: какой сервер ставить.
            if modules.is_empty() {
                w::empty_state(
                    ui,
                    Icon::Package,
                    t("Модулей нет"),
                    t("Модуль — это манифест сервера: положите файл .toml в папку modules рядом с настройками Anvil."),
                );
                w::mono(ui, &folder.to_string_lossy(), None);
                return;
            }
            w::note(
                ui,
                t("Выберите сервер. Файл возьмётся из последнего выпуска на GitHub и проверится по SHA256SUMS."),
            );
            ui.add_space(8.0);
            let rows: Vec<(Option<Icon>, &str, &str)> =
                modules.iter().map(|m| (Some(Icon::Server), m.name.as_str(), m.source.repo.as_str())).collect();
            if let Some(i) = w::nav_rows(ui, &rows) {
                choose = Some(modules[i].id.clone());
            }
            return;
        };
        // Шаг 2: форма из манифеста.
        if m.instances && !install.update {
            w::section_label(ui, t("Имя экземпляра"));
            ui.add(
                egui::TextEdit::singleline(&mut install.instance)
                    .hint_text(t("основной — оставьте пустым"))
                    .desired_width(ui.available_width()),
            );
            w::note(
                ui,
                t(
                    "У каждого экземпляра своя служба, пользователь, папка данных и порты. Строчные латинские буквы, цифры и дефис.",
                ),
            );
            ui.add_space(8.0);
        }
        if install.update {
            w::note(ui, t("Поставит последний выпуск поверх; значения ниже нужны, только если сервер их спрашивает."));
            ui.add_space(6.0);
        }
        for field in &m.form {
            let value = install.form.entry(field.key.clone()).or_default();
            match field.kind {
                FieldKind::Bool => {
                    let mut on = matches!(value.as_str(), "true" | "on" | "yes" | "1");
                    w::switch(ui, &mut on, &field.label);
                    *value = if on { "true" } else { "false" }.to_owned();
                }
                FieldKind::Choice => {
                    w::section_label(ui, &field.label);
                    let options: Vec<(usize, &str)> =
                        field.options.iter().enumerate().map(|(i, o)| (i, o.as_str())).collect();
                    let mut index = field
                        .options
                        .iter()
                        .position(|o| o == value || (value.is_empty() && field.default.as_ref() == Some(o)))
                        .unwrap_or(0);
                    w::select(ui, &mut index, &options, 240.0);
                    *value = field.options.get(index).cloned().unwrap_or_default();
                }
                kind => {
                    w::section_label(ui, &field.label);
                    let width =
                        if matches!(kind, FieldKind::Number | FieldKind::Port) { 140.0 } else { ui.available_width() };
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .password(kind == FieldKind::Password)
                            .hint_text(field.default.clone().unwrap_or_default())
                            .desired_width(width),
                    );
                }
            }
            ui.add_space(8.0);
        }
        if let Some(error) = &install.error {
            error_line(ui, error);
            ui.add_space(6.0);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if install.update { t("Обновить") } else { t("Поставить") };
            if w::button(ui, Kind::Primary, None, label).clicked() {
                go = true;
            }
            if !install.update && w::button(ui, Kind::Ghost, Some(Icon::ArrowLeft), t("Назад")).clicked() {
                back = true;
            }
        });
    });
    if let Some(id) = choose {
        install.form = default_form(app, &id);
        install.module = Some(id);
        install.error = None;
    }
    if back {
        install.module = None;
        install.error = None;
    }
    if go && let Some(m) = &chosen {
        let instance = (!install.instance.trim().is_empty()).then(|| install.instance.trim().to_owned());
        let checked = m
            .form_values(&install.form)
            .map(|_| ())
            .map_err(|e| e.to_string())
            .and_then(|()| m.naming(instance.as_deref()).map(|_| ()).map_err(|e| e.to_string()));
        match checked {
            Err(error) => install.error = Some(error),
            Ok(()) => {
                let job = Job::Deploy {
                    module: m.id.clone(),
                    instance,
                    form: install.form.clone(),
                    update: install.update,
                    allow_unverified: false,
                };
                if !app.servers.start(&machine, job, show) {
                    install.error = Some(t("На машине сейчас идёт другая работа: подождите.").to_owned());
                } else {
                    app.servers_view.tab = TAB_OVERVIEW;
                    return;
                }
            }
        }
    }
    if open {
        app.servers_view.install = Some(install);
    }
}

fn ask_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut ask) = app.servers_view.ask.take() else { return };
    let show = app.config.show_hidden_modules;
    let machine_of = |id: &str| app.config.servers.iter().find(|m| m.id == id).cloned();
    let verbatim = |ui: &mut Ui, name: &str, command: &str| {
        let p = Palette::of(ui);
        w::note(ui, format!("{} {}:", t("Будет выполнено на машине"), q(name)));
        w::note(ui, t("от root, дословно так:"));
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(p.field)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(radius::CONTROL)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::Label::new(RichText::new(command).font(FontId::monospace(12.5)).color(p.text))
                        .selectable(true),
                );
            });
    };
    match &mut ask {
        Ask::Action { machine, module, instance, index, label, command } => {
            let Some(target) = machine_of(machine) else { return };
            let heading = format!("{} {}?", label, q(&target.name));
            let body = |ui: &mut Ui| verbatim(ui, &target.name, command);
            match w::confirm(ctx, "anvil-servers-ask", &heading, body, t("Выполнить"), false) {
                Some(true) => {
                    app.servers.start(
                        &target,
                        Job::Action { module: module.clone(), instance: instance.clone(), index: *index },
                        show,
                    );
                }
                Some(false) => {}
                None => app.servers_view.ask = Some(ask),
            }
        }
        Ask::Console { machine, command } => {
            let Some(target) = machine_of(machine) else { return };
            let heading = format!("{} {}?", t("Выполнить на"), q(&target.name));
            let body = |ui: &mut Ui| verbatim(ui, &target.name, command);
            match w::confirm(ctx, "anvil-servers-ask", &heading, body, t("Выполнить"), false) {
                Some(true) => {
                    app.servers_view.console.clear();
                    app.servers.start(&target, Job::Console { command: command.clone() }, show);
                }
                Some(false) => {}
                None => app.servers_view.ask = Some(ask),
            }
        }
        Ask::Remove { machine } => {
            let Some(target) = machine_of(machine) else { return };
            let heading = format!("{} {} {}", t("Убрать"), q(&target.name), t("из списка?"));
            let body = |ui: &mut Ui| {
                w::note(ui, t("Anvil забудет адрес, ключ машины и сохранённый пароль."));
                w::note(ui, t("Сама машина и серверы на ней не меняются."));
            };
            match w::confirm(ctx, "anvil-servers-ask", &heading, body, t("Убрать"), true) {
                Some(true) => {
                    let _ = SystemStore::default().forget_machine(&target.id);
                    app.config.servers.retain(|m| m.id != target.id);
                    app.servers.runtime.remove(&target.id);
                    app.save();
                }
                Some(false) => {}
                None => app.servers_view.ask = Some(ask),
            }
        }
        Ask::Uninstall { machine, module, instance, name, command, data_dir, typed } => {
            let Some(target) = machine_of(machine) else { return };
            let mut open = true;
            let mut go = false;
            let title = format!("{} {}?", t("Удалить"), q(name));
            let matches = typed.trim() == name.as_str();
            chrome::dialog(ctx, "anvil-servers-uninstall", &title, 520.0, &mut open, |ui| {
                verbatim(ui, &target.name, command);
                ui.add_space(8.0);
                let p = Palette::of(ui);
                ui.horizontal_wrapped(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
                    anvil_ui::icons::paint(ui.painter(), rect, Icon::Warning, p.warning);
                    let text = format!(
                        "{} {data_dir} {}",
                        t("Служба, пользователь и данные сервера, в том числе"),
                        t("могут быть удалены. Вернуть их нельзя.")
                    );
                    ui.label(RichText::new(text).size(13.0).color(p.weak));
                });
                ui.add_space(8.0);
                w::section_label(ui, &format!("{} {}", t("Чтобы подтвердить, введите"), q(name)));
                ui.add(egui::TextEdit::singleline(typed).desired_width(ui.available_width()));
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if gated(
                        ui,
                        matches,
                        t("Введите имя сервера, чтобы разрешить удаление."),
                        |ui| w::button(ui, Kind::Danger, Some(Icon::Trash), t("Удалить сервер")),
                    ) {
                        go = true;
                    }
                });
            });
            if go {
                app.servers.start(&target, Job::Uninstall { module: module.clone(), instance: instance.clone() }, show);
            } else if open {
                app.servers_view.ask = Some(ask);
            }
        }
    }
}

/// Вопросы, которые задала задача: первое подключение, смена ключа, выпуск без суммы.
fn prompt_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(prompt) = app.servers.prompt.clone() else { return };
    let show = app.config.show_hidden_modules;
    match prompt {
        Prompt::Trust { machine, fingerprint, job } => {
            let Some(target) = app.config.servers.iter().find(|m| m.id == machine).cloned() else {
                app.servers.prompt = None;
                return;
            };
            let (mut open, mut trust) = (true, false);
            chrome::dialog(
                ctx,
                "anvil-servers-trust",
                t("Первое подключение"),
                540.0,
                &mut open,
                |ui| {
                    w::note(
                        ui,
                        format!("{} {} ({}).", t("Anvil ещё не знает ключ машины"), q(&target.name), target.address()),
                    );
                    w::note(
                        ui,
                        t("Пока вы не подтвердите отпечаток, пароль и другие секреты на машину не отправляются."),
                    );
                    ui.add_space(10.0);
                    w::section_label(ui, t("Отпечаток ключа машины"));
                    fingerprint_box(ui, &fingerprint);
                    ui.add_space(6.0);
                    w::note(ui, t("Сверьте его с панелью хостинга или на самой машине:"));
                    w::mono(ui, "ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub", None);
                    ui.add_space(12.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if w::button(ui, Kind::Primary, Some(Icon::Lock), t("Доверять и подключиться")).clicked()
                        {
                            trust = true;
                        }
                    });
                },
            );
            if trust {
                trust_key(app, &machine, &fingerprint);
                app.servers.prompt = None;
                restart(app, &machine, job, show);
            } else if !open {
                app.servers.prompt = None;
            }
        }
        Prompt::KeyChanged { machine, was, now, job } => {
            let Some(target) = app.config.servers.iter().find(|m| m.id == machine).cloned() else {
                app.servers.prompt = None;
                return;
            };
            let (mut open, mut trust) = (true, false);
            chrome::dialog(
                ctx,
                "anvil-servers-keychanged",
                t("Ключ машины сменился"),
                540.0,
                &mut open,
                |ui| {
                    w::banner(
                        ui,
                        Tone::Danger,
                        t("Anvil не подключился"),
                        t("Пароль и другие секреты не отправлены."),
                        |_| {},
                    );
                    ui.add_space(10.0);
                    w::note(ui, format!("{} {} ({}).", t("Машина"), q(&target.name), target.address()));
                    w::note(
                        ui,
                        t(
                            "Так бывает после переустановки системы — и когда машину подменили по дороге. Сверьте новый отпечаток с панелью хостинга, прежде чем доверять.",
                        ),
                    );
                    ui.add_space(8.0);
                    w::section_label(ui, t("Был"));
                    fingerprint_box(ui, &was);
                    ui.add_space(6.0);
                    w::section_label(ui, t("Стал"));
                    fingerprint_box(ui, &now);
                    ui.add_space(12.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if w::button(ui, Kind::Danger, Some(Icon::Lock), t("Поверить новому ключу")).clicked()
                        {
                            trust = true;
                        }
                    });
                },
            );
            if trust {
                trust_key(app, &machine, &now);
                app.servers.prompt = None;
                restart(app, &machine, job, show);
            } else if !open {
                app.servers.prompt = None;
            }
        }
        Prompt::Unverified { machine, reason, job } => {
            let Some(target) = app.config.servers.iter().find(|m| m.id == machine).cloned() else {
                app.servers.prompt = None;
                return;
            };
            let heading = t("Поставить без проверки суммы?");
            let body = |ui: &mut Ui| {
                w::note(
                    ui,
                    t(
                        "В выпуске нет контрольной суммы для этого файла: Anvil не может проверить, что его не подменили.",
                    ),
                );
                w::mono(ui, &reason, None);
                ui.add_space(4.0);
                w::note(
                    ui,
                    t(
                        "Продолжайте, только если полностью доверяете репозиторию. Файл будет залит на машину и запущен от root.",
                    ),
                );
            };
            match w::confirm(ctx, "anvil-servers-unverified", heading, body, t("Поставить без проверки"), true)
            {
                Some(true) => {
                    app.servers.prompt = None;
                    let job = match job {
                        Job::Deploy { module, instance, form, update, .. } => {
                            Job::Deploy { module, instance, form, update, allow_unverified: true }
                        }
                        other => other,
                    };
                    app.servers.start(&target, job, show);
                }
                Some(false) => app.servers.prompt = None,
                None => {}
            }
        }
    }
}

/// Отпечаток в рамке: моноширинный, можно выделить и скопировать.
fn fingerprint_box(ui: &mut Ui, text: &str) {
    let p = Palette::of(ui);
    egui::Frame::new()
        .fill(p.field)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).font(FontId::monospace(13.0)).color(p.text)).selectable(true));
        });
}

fn trust_key(app: &mut App, machine: &str, fingerprint: &str) {
    if let Some(m) = app.config.servers.iter_mut().find(|m| m.id == machine) {
        m.fingerprint = fingerprint.to_owned();
        app.save();
    }
}

fn restart(app: &mut App, machine: &str, job: Job, show: bool) {
    if let Some(target) = app.config.servers.iter().find(|m| m.id == machine).cloned() {
        app.servers.start(&target, job, show);
    }
}

// ─── Настройки ─────────────────────────────────────────────────────────────

/// Раздел «Серверы» в настройках: скрытые модули, папка манифестов, битые манифесты с причиной.
pub fn settings(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Серверы"));
    ui.add_space(2.0);
    let before = app.config.show_hidden_modules;
    w::switch(ui, &mut app.config.show_hidden_modules, t("Скрытые модули"));
    if app.config.show_hidden_modules != before {
        app.save();
    }
    w::note(ui, t("Модули с hidden = true в манифесте видны в «Серверы» только при включённом переключателе."));
    ui.add_space(6.0);
    let folder = crate::servers::modules_dir(&app.config_path);
    w::note(ui, t("Манифесты модулей — файлы .toml в папке:"));
    w::mono(ui, &folder.to_string_lossy(), None);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if w::button(ui, Kind::Secondary, Some(Icon::Folder), t("Открыть папку")).clicked() {
            let _ = std::fs::create_dir_all(&folder);
            if let Err(e) = crate::open::folder(&folder) {
                app.toasts.push(format!("{}: {e}", t("Не открылось")), Tone::Danger);
            }
        }
        if w::button(ui, Kind::Ghost, Some(Icon::Refresh), t("Перечитать")).clicked() {
            app.servers.reload(&app.config_path);
        }
    });
    ui.add_space(6.0);
    let show = app.config.show_hidden_modules;
    let loaded = app.servers.registry.modules(show).count();
    w::note(ui, format!("{} {}", t("Загружено модулей:"), loaded));
    let broken: Vec<(String, String)> =
        app.servers.registry.broken(show).map(|b| (b.origin.label(), b.reason.clone())).collect();
    if !broken.is_empty() {
        ui.add_space(6.0);
        w::note(ui, t("Не приняты — причина указана справа:"));
        for (file, reason) in &broken {
            ui.horizontal(|ui| {
                let p = Palette::of(ui);
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
                anvil_ui::icons::paint(ui.painter(), rect, Icon::Warning, p.warning);
                w::mono(ui, file, Some(p.text));
                ui.add(egui::Label::new(RichText::new(reason).size(13.0).color(p.weak)).truncate())
                    .on_hover_text(reason);
            });
        }
    }
}
