//! Пульт: всё, что я запускаю, одной таблицей. Enter — запустить, Ctrl+Enter — свежую сборку из
//! кода, Shift+Enter — этот проект в Кузнице. Строки не прыгают, пока Пульт на экране.

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Mark, Palette, Tone, semibold};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText, Ui, Vec2};

use crate::app::{App, Mode};
use crate::deck::{Group, Item};
use crate::i18n::{self, t};
use crate::installs;
use crate::registry::Kind as ProjectKind;

/// Главное действие строки: что сделает Enter и кнопка справа.
#[derive(Debug, Clone, PartialEq)]
enum Main {
    /// Запустить то, что установлено (или собрано, если ставить нечего).
    Launch,
    /// Собрать из кода и запустить.
    FromCode,
    /// Программа работает — показать её окно.
    Focus(u32),
    /// Служба работает — её журнал (пока — консоль Кузницы).
    Journal,
    /// Не установлена, на GitHub есть выпуск — поставить и запустить.
    Install,
    /// Открыть проект в редакторе движка.
    Editor,
    /// Нечего делать: это окно, движка нет, проект уже открыт. Почему — в подсказке.
    Nothing,
}

/// Как строка выглядит сейчас.
pub struct Look {
    /// Чип источника: слово и моно-часть.
    chip: (String, Option<String>),
    /// Работает: PID и когда запущен.
    running: Option<(u32, Option<i64>)>,
    /// Это само окно Anvil.
    this: bool,
    state: String,
    main: Main,
    icon: Icon,
    hint: String,
    /// Где стоит установленная копия — для «Папка установки».
    install_dir: Option<PathBuf>,
}

/// Пульт на этот кадр: предметы в порядке строк и как они выглядят. Считается один раз и нужен
/// и строкам, и чипам «Запущено» в строке состояния.
pub struct Frame {
    items: Vec<Item>,
    looks: Vec<Look>,
    /// Выбранная строка: та, что выбрали, или первая, если не выбирали (или выбранной больше нет).
    selected: Option<String>,
}

impl Frame {
    pub fn new(app: &mut App) -> Frame {
        let items = app.deck_items();
        // Пока строку не выбирали — выбрана первая (проекты подгружаются не сразу, и первая меняется).
        // Выбранной больше нет (убрали, скрыли) — тоже первая: Enter действует на подсвеченную.
        let chosen = app.deck_view.selected.clone().filter(|k| items.iter().any(|i| &i.key == k));
        let selected = chosen.or_else(|| items.first().map(|i| i.key.clone()));
        let godot = if items.iter().any(|i| i.kind == ProjectKind::Godot) { app.godot_editor() } else { None };
        let looks = items.iter().map(|i| look(app, i, godot.is_some())).collect();
        Frame { items, looks, selected }
    }

    /// Чипы «Запущено»: знак, имя, время работы, ключ. Порядок — как на Пульте.
    pub fn chips(&self) -> Vec<(Mark, String, String, String)> {
        self.items
            .iter()
            .zip(&self.looks)
            .filter(|(i, l)| !i.is_self() && !l.this)
            .filter_map(|(i, l)| {
                let (_, started) = l.running?;
                let tail = started.map(|s| i18n::uptime_short(i18n::now() - s)).unwrap_or_default();
                Some((i.mark, i.name.clone(), tail, i.key.clone()))
            })
            .collect()
    }
}

/// Что попросили на Пульте: выполняется после отрисовки.
enum Action {
    Select(String),
    Main(Box<Item>),
    FromCode(Box<Item>),
    FromSource(Box<Item>),
    Forge(Box<Item>),
    Editor(Box<Item>),
    Pin(String),
    Remove(String),
    Stop(String, u32, PathBuf),
    Folder(PathBuf),
    GodotPath,
    AmberAdmin(PathBuf),
    AddFolder,
}

pub fn show(app: &mut App, ui: &mut Ui, frame: &Frame) {
    let (items, looks) = (&frame.items, &frame.looks);
    let mut actions = keyboard(app, ui.ctx(), items, frame.selected.as_deref());
    let p = Palette::of(ui);

    ui.label(RichText::new(t("Пульт")).font(semibold(24.0)).color(p.text));
    let running = looks.iter().filter(|l| l.running.is_some() && !l.this).count();
    let subtitle = format!(
        "{} · {}",
        i18n::count(running, ["работает", "работают", "работают"], ["running", "running"]),
        i18n::count(items.len(), ["в списке", "в списке", "в списке"], ["item", "items"]),
    );
    w::note(ui, subtitle);
    ui.add_space(18.0);

    if items.is_empty() {
        empty(app, ui, &mut actions);
        apply(app, ui.ctx(), actions);
        return;
    }

    let remote = remote_card_data(app);
    let wide = ui.available_width() >= 1100.0;
    let right_w = 380.0;
    if wide && remote.is_some() {
        ui.horizontal_top(|ui| {
            let left_w = ui.available_width() - right_w - 14.0;
            ui.allocate_ui_with_layout(Vec2::new(left_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(left_w);
                groups(app, ui, frame, &mut actions);
            });
            ui.add_space(14.0);
            ui.allocate_ui_with_layout(Vec2::new(right_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(right_w);
                right_column(ui, remote.as_ref(), &mut actions);
            });
        });
    } else {
        groups(app, ui, frame, &mut actions);
        if remote.is_some() {
            ui.add_space(4.0);
            right_column(ui, remote.as_ref(), &mut actions);
        }
    }
    apply(app, ui.ctx(), actions);
}

/// Клавиши Пульта — только когда ничто другое их не ждёт: ни палитра, ни диалог, ни меню, ни
/// элемент в фокусе (у кнопки в фокусе Enter — её щелчок, у поля — ввод).
fn keyboard(app: &mut App, ctx: &egui::Context, items: &[Item], selected: Option<&str>) -> Vec<Action> {
    let mut actions = Vec::new();
    let free = app.palette.is_none()
        && ctx.memory(|m| m.focused().is_none() && m.top_modal_layer().is_none())
        && !ctx.any_popup_open();
    if !free || items.is_empty() {
        return actions;
    }
    // Повтор зажатой клавиши не запускает ещё раз: действует только само нажатие.
    let fresh_enter = ctx.input(|i| {
        i.events.iter().any(|e| matches!(e, egui::Event::Key { key: Key::Enter, pressed: true, repeat: false, .. }))
    });
    let (from_code, forge, _page, enter, up, down, pin) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::COMMAND, Key::Enter),
            i.consume_key(Modifiers::SHIFT, Key::Enter),
            i.consume_key(Modifiers::ALT, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::P)),
        )
    });
    let mut index = selected.and_then(|k| items.iter().position(|i| i.key == k)).unwrap_or(0);
    if up || down {
        index = if down { (index + 1).min(items.len() - 1) } else { index.saturating_sub(1) };
        app.deck_view.selected = Some(items[index].key.clone());
        app.deck_view.scroll = true;
    }
    let item = &items[index];
    if fresh_enter && from_code {
        actions.push(Action::FromCode(Box::new(item.clone())));
    } else if fresh_enter && forge {
        actions.push(Action::Forge(Box::new(item.clone())));
    } else if fresh_enter && enter {
        actions.push(Action::Main(Box::new(item.clone())));
    }
    if pin {
        actions.push(Action::Pin(item.key.clone()));
    }
    // Печать на Пульте открывает палитру с набранным.
    let typed: String = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    });
    if !typed.trim().is_empty() && !ctx.input(|i| i.modifiers.command || i.modifiers.alt) {
        super::palette::open_with(app, typed.trim_start());
    }
    actions
}

fn groups(app: &App, ui: &mut Ui, frame: &Frame, actions: &mut Vec<Action>) {
    let (items, looks, selected) = (&frame.items, &frame.looks, &frame.selected);
    for group in Group::ALL {
        let rows: Vec<usize> = (0..items.len()).filter(|&i| items[i].group == group).collect();
        if rows.is_empty() {
            continue;
        }
        let label = match group {
            Group::Programs => t("Программы"),
            Group::Services => t("Серверы и боты"),
            Group::Games => t("Godot и Unity"),
        };
        w::section_label(ui, &format!("{label} · {}", rows.len()));
        ui.add_space(8.0);
        w::card_frame(ui).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (n, &i) in rows.iter().enumerate() {
                let is_selected = selected.as_deref() == Some(items[i].key.as_str());
                row(app, ui, &items[i], &looks[i], is_selected, actions);
                if n + 1 < rows.len() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
                    let p = Palette::of(ui);
                    ui.painter().hline(
                        (rect.left() + 12.0)..=(rect.right() - 12.0),
                        rect.center().y,
                        egui::Stroke::new(1.0, p.border),
                    );
                }
            }
        });
        ui.add_space(18.0);
    }
}

/// Строка предмета: знак, имя и подпись, источник, состояние, главное действие и меню.
fn row(app: &App, ui: &mut Ui, item: &Item, look: &Look, selected: bool, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let label =
        format!("{}, {}, {} {}", item.name, look.state, look.chip.0, look.chip.1.as_deref().unwrap_or_default());
    let id = egui::Id::new(("deck-row", &item.key));
    let response = w::list_row(ui, id, selected, 44.0, &label);
    if selected && app.deck_view.scroll {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    if response.clicked() {
        actions.push(Action::Select(item.key.clone()));
    }
    if response.double_clicked() {
        actions.push(Action::Main(Box::new(item.clone())));
    }
    let rect = response.rect;
    let mark = egui::Rect::from_min_size(egui::pos2(rect.left() + 12.0, rect.center().y - 14.0), Vec2::splat(28.0));
    w::paint_mark(ui, mark, item.mark.accent, item.mark.icon);

    // Справа — постоянные колонки: источник 190, состояние 230, действия 98. Имя — в том, что осталось.
    let right = 190.0 + 16.0 + 230.0 + 16.0 + 98.0;
    let name_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 52.0, rect.top()),
        egui::pos2((rect.right() - right - 8.0).max(rect.left() + 60.0), rect.bottom()),
    );
    // Надписи строки не выделяются и не ловят щелчков: щелчок по имени — щелчок по строке.
    let mut name_ui = ui.new_child(
        egui::UiBuilder::new()
            .id(id.with("name"))
            .max_rect(name_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    name_ui.style_mut().interaction.selectable_labels = false;
    name_ui.set_clip_rect(name_rect.intersect(ui.clip_rect()));
    name_ui.spacing_mut().item_spacing.x = 8.0;
    let font = if selected { semibold(14.0) } else { egui::FontId::proportional(14.0) };
    name_ui.label(RichText::new(&item.name).font(font).color(p.text));
    if !item.caption.is_empty() {
        w::mono(&mut name_ui, &item.caption, None);
    }
    if item.no_git {
        w::badge(&mut name_ui, t("без git"), Tone::Neutral);
    }

    let cols = egui::Rect::from_min_max(egui::pos2(rect.right() - right, rect.top()), rect.max);
    let mut right_ui = ui.new_child(
        egui::UiBuilder::new()
            .id(id.with("actions"))
            .max_rect(cols.shrink2(Vec2::new(8.0, 0.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    right_ui.style_mut().interaction.selectable_labels = false;
    right_ui.spacing_mut().item_spacing.x = 4.0;
    let more = w::icon_button(&mut right_ui, Icon::More, t("Ещё действия"));
    menu(app, &more, item, look, actions);
    let enabled = look.main != Main::Nothing;
    let main = right_ui
        .add_enabled_ui(enabled, |ui| w::icon_button(ui, look.icon, &look.hint))
        .inner
        .on_disabled_hover_text(&look.hint);
    if main.clicked() {
        actions.push(Action::Main(Box::new(item.clone())));
    }
    right_ui.add_space(12.0);
    right_ui.allocate_ui_with_layout(Vec2::new(230.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let lit = look.running.is_some() || look.this;
        if lit {
            w::dot(ui, Tone::Success);
        } else {
            w::ring(ui);
        }
        let color = if lit { p.text } else { p.weak };
        ui.add(egui::Label::new(RichText::new(&look.state).size(13.0).color(color)).truncate());
    });
    right_ui.add_space(16.0);
    right_ui.allocate_ui_with_layout(Vec2::new(190.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        w::source_chip(ui, &look.chip.0, look.chip.1.as_deref());
    });
}

/// Меню строки: ещё способы запустить, Кузница, закрепить, остановить, убрать.
fn menu(app: &App, more: &egui::Response, item: &Item, look: &Look, actions: &mut Vec<Action>) {
    w::menu(more, 290.0, |ui| {
        match item.kind {
            ProjectKind::Rust if !item.is_self() => {
                if w::menu_item(ui, Some(Icon::Hammer), t("Запустить из кода"), Some("Ctrl+Enter")).clicked()
                {
                    actions.push(Action::FromCode(Box::new(item.clone())));
                }
            }
            ProjectKind::Godot => {
                if w::menu_item(ui, Some(Icon::Refresh), t("Экспортировать и играть"), Some("Ctrl+Enter")).clicked()
                {
                    actions.push(Action::FromCode(Box::new(item.clone())));
                }
                if w::menu_item(ui, Some(Icon::Play), t("Играть из исходников"), None).clicked() {
                    actions.push(Action::FromSource(Box::new(item.clone())));
                }
                if w::menu_item(ui, Some(Icon::Pencil), t("Открыть в Godot"), None).clicked() {
                    actions.push(Action::Editor(Box::new(item.clone())));
                }
                if w::menu_item(ui, Some(Icon::Folder), t("Путь к Godot…"), None).clicked() {
                    actions.push(Action::GodotPath);
                }
            }
            ProjectKind::Unity if w::menu_item(ui, Some(Icon::Pencil), t("Открыть в Unity"), None).clicked() => {
                actions.push(Action::Editor(Box::new(item.clone())));
            }
            _ => {}
        }
        if w::menu_item(ui, Some(Icon::Hammer), t("Открыть в Кузнице"), Some("Shift+Enter")).clicked() {
            actions.push(Action::Forge(Box::new(item.clone())));
        }
        w::menu_separator(ui);
        let pinned = app.config.deck.pinned.contains(&item.key);
        let pin = if pinned { t("Открепить") } else { t("Закрепить") };
        if w::menu_item(ui, Some(Icon::Pin), pin, Some("Ctrl+P")).clicked() {
            actions.push(Action::Pin(item.key.clone()));
        }
        if let Some(dir) = &look.install_dir
            && w::menu_item(ui, Some(Icon::Folder), t("Папка установки"), None).clicked()
        {
            actions.push(Action::Folder(dir.clone()));
        }
        if w::menu_item(ui, Some(Icon::Folder), t("Папка проекта"), None).clicked() {
            actions.push(Action::Folder(item.project.clone()));
        }
        w::menu_separator(ui);
        match look.running.filter(|_| !look.this) {
            Some((pid, _)) => {
                if w::menu_item_danger(ui, Some(Icon::Stop), t("Остановить…")).clicked() {
                    actions.push(Action::Stop(item.name.clone(), pid, item.project.clone()));
                }
            }
            // Недоступное не прячется — объясняется.
            None => {
                ui.add_enabled_ui(false, |ui| w::menu_item(ui, Some(Icon::Stop), t("Остановить… (не запущено)"), None));
            }
        }
        if w::menu_item_danger(ui, Some(Icon::Close), t("Убрать с Пульта")).clicked() {
            actions.push(Action::Remove(item.key.clone()));
        }
    });
}

/// Что сейчас с предметом и что сделает Enter. Без обращений к диску: всё уже прочитано фоновым
/// потоком или закешировано в `App`.
fn look(app: &App, item: &Item, godot_found: bool) -> Look {
    let project = app.projects.iter().find(|p| p.path == item.project);
    match item.kind {
        ProjectKind::Godot => {
            let engine = project.and_then(|p| p.engine.as_ref());
            let export = engine.and_then(|e| e.export.clone());
            let built = engine.and_then(|e| e.exported_at);
            let running = export.as_ref().and_then(|exe| {
                let stem = exe.file_stem()?.to_string_lossy().into_owned();
                app.running(&stem)
                    .iter()
                    .find(|r| r.path.as_deref().is_some_and(|p| crate::registry::same_dir(p, exe)))
                    .map(|r| (r.pid, r.started))
            });
            let chip = match built {
                Some(at) => (t("экспорт").to_owned(), Some(i18n::date(at))),
                None => (t("без сборки").to_owned(), None),
            };
            let (main, icon, hint) = if let Some((pid, _)) = running {
                (Main::Focus(pid), Icon::Window, t("К окну").to_owned())
            } else if built.is_some() {
                (Main::Launch, Icon::Play, t("Играть").to_owned())
            } else if godot_found {
                (Main::Editor, Icon::Pencil, t("Открыть в Godot").to_owned())
            } else {
                (Main::Nothing, Icon::Play, t("Нет сборки, и Godot не найден").to_owned())
            };
            Look {
                chip,
                state: state(running, t("не запущена")),
                running,
                this: false,
                main,
                icon,
                hint,
                install_dir: None,
            }
        }
        ProjectKind::Unity => {
            let engine = project.and_then(|p| p.engine.as_ref());
            let version = engine.and_then(|e| e.version.clone()).unwrap_or_default();
            let open = engine.is_some_and(|e| e.open);
            let (main, hint) = if open {
                (Main::Nothing, t("Проект уже открыт в Unity").to_owned())
            } else if engine.is_some_and(|e| e.editor.is_some()) {
                (Main::Editor, format!("{} {version}", t("Открыть в Unity")))
            } else {
                (Main::Nothing, format!("Unity {version} {}", t("не найден в Unity Hub")))
            };
            let state = if open { t("открыт в Unity") } else { t("не открыт") }.to_owned();
            Look {
                chip: (t("без сборки").to_owned(), None),
                running: None,
                this: false,
                state,
                main,
                icon: Icon::Pencil,
                hint,
                install_dir: None,
            }
        }
        ProjectKind::Rust | ProjectKind::Git => rust_look(app, item),
    }
}

fn rust_look(app: &App, item: &Item) -> Look {
    let bin = item.bin.as_deref().unwrap_or_default();
    let installed = app.installs.get(bin).and_then(Option::as_ref);
    let root = installs::root(bin);
    let install_dir = installed.map(|_| root.clone());
    let target =
        app.projects.iter().find(|p| p.path == item.project).and_then(|p| p.meta()).map(|m| m.target_dir.clone());
    let built = app.builds.get(bin).copied().flatten();
    let service = item.group == Group::Services;

    if item.is_self() {
        let exe = std::env::current_exe().ok();
        let from_install = exe.as_deref().is_some_and(|e| installs::inside(e, &root));
        let chip = if from_install {
            (t("установлена").to_owned(), installed.and_then(|i| i.current.clone()))
        } else {
            (t("портативная").to_owned(), Some(env!("CARGO_PKG_VERSION").to_owned()))
        };
        return Look {
            chip,
            running: Some((std::process::id(), None)),
            this: true,
            state: t("это окно").to_owned(),
            main: Main::Nothing,
            icon: Icon::Play,
            hint: t("Это окно").to_owned(),
            install_dir,
        };
    }

    let instance = app.running(bin).first().cloned();
    let running = instance.as_ref().map(|r| (r.pid, r.started));
    let remote = app.remotes.get(&item.project);
    let release = super::install::release_for(remote, bin, app.config.common.prerelease);
    let offered = release.as_ref().map(|(_, v)| format!("v{v}")).filter(|_| installed.is_none());

    // Чип говорит о том, что запущено сейчас, а в покое — о том, что запустит Enter.
    let chip = match &instance {
        Some(r) if r.path.as_deref().is_some_and(|p| installs::inside(p, &root)) => {
            (t("установлена").to_owned(), installed.and_then(|i| i.current.clone()))
        }
        Some(r) if r.path.as_deref().zip(target.as_deref()).is_some_and(|(p, t)| installs::inside(p, t)) => {
            (t("сборка").to_owned(), built.map(i18n::date))
        }
        Some(_) => (t("не из Anvil").to_owned(), None),
        None => match (installed, built) {
            (Some(i), _) => (t("установлена").to_owned(), i.current.clone()),
            // Enter поставит выпуск с GitHub, а не запустит здешнюю сборку.
            (None, _) if offered.is_some() => (t("не установлена").to_owned(), None),
            (None, Some(at)) => (t("сборка").to_owned(), Some(i18n::date(at))),
            (None, None) if service => (t("не собран").to_owned(), None),
            (None, None) => (t("не установлена").to_owned(), None),
        },
    };
    let (main, icon, hint) = match (&instance, service) {
        (Some(r), false) => (Main::Focus(r.pid), Icon::Window, t("К окну").to_owned()),
        (Some(_), true) => (Main::Journal, Icon::Terminal, t("Журнал").to_owned()),
        (None, _) if installed.is_some() => (Main::Launch, Icon::Play, t("Запустить").to_owned()),
        (None, _) if offered.is_some() => {
            let v = offered.clone().unwrap_or_default();
            (Main::Install, Icon::Download, format!("{} {v} {}", t("Поставить"), t("и запустить")))
        }
        (None, _) => (Main::FromCode, Icon::Play, t("Собрать и запустить").to_owned()),
    };
    let idle = match &offered {
        Some(v) if instance.is_none() => i18n::on_github(v),
        _ => t("не запущен").to_owned(),
    };
    Look { chip, state: state(running, &idle), running, this: false, main, icon, hint, install_dir }
}

/// «работает · 2 ч 14 мин» или то, что сказано для покоя.
fn state(running: Option<(u32, Option<i64>)>, idle: &str) -> String {
    match running {
        Some((_, Some(started))) => format!("{} · {}", t("работает"), i18n::uptime(i18n::now() - started)),
        Some((_, None)) => t("работает").to_owned(),
        None => idle.to_owned(),
    }
}

/// Главное действие предмета — то же, что Enter на Пульте. Нужно и палитре.
pub fn run_main(app: &mut App, item: &Item) {
    app.deck_view.selected = Some(item.key.clone());
    let godot = item.kind == ProjectKind::Godot && app.godot_editor().is_some();
    let look = look(app, item, godot);
    match look.main {
        Main::Launch => app.launch_item(item, false),
        Main::FromCode => app.launch_item(item, true),
        Main::Focus(pid) => app.focus(&item.name, pid),
        Main::Journal => {
            app.set_mode(Mode::Forge);
            app.select(item.project.clone());
            app.view = crate::app::View::Project;
            app.log_open = true;
        }
        Main::Install => app.install_and_launch(item),
        Main::Editor => app.open_editor(item),
        Main::Nothing => {}
    }
}

/// Сводка amber-admin, если среди проектов есть amber-admin.
fn remote_card_data(app: &mut App) -> Option<(Result<crate::amber::Summary, String>, PathBuf)> {
    let dir = app
        .projects
        .iter()
        .find(|p| p.meta().is_some_and(|m| m.bins.iter().any(|b| b.name == crate::amber::ADMIN)))?
        .path
        .clone();
    let summary = app.amber.get()?.clone();
    Some((summary, dir))
}

fn right_column(
    ui: &mut Ui,
    remote: Option<&(Result<crate::amber::Summary, String>, PathBuf)>,
    actions: &mut Vec<Action>,
) {
    let Some((summary, dir)) = remote else { return };
    let p = Palette::of(ui);
    w::section_label(ui, t("Удалённые серверы"));
    ui.add_space(8.0);
    w::card(ui, |ui| {
        match summary {
            Err(e) => {
                w::note(ui, t("сводка amber-admin не читается")).on_hover_text(e);
            }
            Ok(summary) if summary.servers.is_empty() => {
                w::note(ui, t("серверов в amber-admin нет"));
            }
            Ok(summary) => {
                for server in &summary.servers {
                    let (tone, state) = super::amber::health(&server.health);
                    let name = crate::amber::label(&server.name);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        w::dot(ui, tone);
                        ui.label(RichText::new(name).size(14.0).color(p.text));
                        if let Some(version) = &server.version {
                            ui.add_space(2.0);
                            w::mono(ui, version, None);
                        }
                        // Словом, а не только цветом точки: «2 из 2 в сети» или что с сервером.
                        let text = match (server.health.as_str(), server.online, server.members) {
                            ("up", Some(online), Some(members)) => {
                                format!("{online} {} {members} {}", t("из"), t("в сети"))
                            }
                            _ => state.to_owned(),
                        };
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(text).size(13.0).color(p.text));
                        });
                    })
                    .response
                    .on_hover_text(format!("{name}: {state}"));
                }
                ui.add_space(6.0);
                let note = format!(
                    "{} {}. {}",
                    t("Сводка amber-admin, проверено в"),
                    i18n::clock(summary.checked_at),
                    t("Anvil к серверам не подключается.")
                );
                w::note(ui, note);
            }
        }
        ui.add_space(10.0);
        if w::button(ui, Kind::Secondary, Some(Icon::Server), t("Открыть amber-admin")).clicked() {
            actions.push(Action::AmberAdmin(dir.clone()));
        }
    });
}

fn empty(app: &App, ui: &mut Ui, actions: &mut Vec<Action>) {
    if app.scanning {
        w::empty_state(ui, Icon::Search, t("Ищу проекты…"), t("Смотрю папки из настроек."));
        return;
    }
    w::empty_state(
        ui,
        Icon::Tiles,
        t("Пульт пуст"),
        t("Добавьте папку с проектами: Anvil найдёт программы, службы и проекты Godot и Unity"),
    );
    ui.vertical_centered(|ui| {
        if w::button(ui, Kind::Secondary, Some(Icon::Plus), t("Добавить папку…")).clicked() {
            actions.push(Action::AddFolder);
        }
    });
}

fn apply(app: &mut App, ctx: &egui::Context, actions: Vec<Action>) {
    app.deck_view.scroll = false;
    for action in actions {
        match action {
            Action::Select(key) => app.deck_view.selected = Some(key),
            Action::Main(item) => run_main(app, &item),
            Action::FromCode(item) => {
                app.deck_view.selected = Some(item.key.clone());
                app.launch_item(&item, true);
            }
            Action::FromSource(item) => {
                app.play_from_source(&item);
            }
            Action::Forge(item) => {
                app.deck_view.selected = Some(item.key.clone());
                app.set_mode(Mode::Forge);
                app.view = crate::app::View::Project;
            }
            Action::Editor(item) => app.open_editor(&item),
            Action::Pin(key) => {
                let pinned = &mut app.config.deck.pinned;
                match pinned.iter().position(|k| *k == key) {
                    Some(i) => {
                        pinned.remove(i);
                    }
                    None => pinned.push(key),
                }
                app.save();
                app.deck_view.order.clear();
            }
            Action::Remove(key) => {
                app.config.deck.removed.push(key);
                app.save();
                app.toasts.push(t("Убрано с Пульта — вернуть можно в настройках"), Tone::Neutral);
            }
            Action::Stop(name, pid, dir) => app.stop_confirm = Some((name, pid, dir)),
            Action::Folder(dir) => app.report(crate::open::folder(&dir)),
            Action::GodotPath => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Godot", &["exe"]).pick_file() {
                    app.set_godot(path);
                }
            }
            Action::AmberAdmin(dir) => app.open_amber_admin(&dir),
            Action::AddFolder => super::settings::add_root(app),
        }
    }
    // Время работы в строках идёт раз в минуту.
    ctx.request_repaint_after(std::time::Duration::from_secs(60));
}
