//! Страница предмета Пульта (§7.2–7.4): программа, серверы и боты, проект Godot или Unity.
//! Открывается щелчком по строке, Alt+Enter, → или Пробелом; назад — Esc или «Пульт».

use std::path::{Path, PathBuf};

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone, semibold};
use eframe::egui::{self, Key, Modifiers, RichText, Ui, Vec2};

use super::deck::{Frame, Look, Main};
use crate::app::{App, Mode, View};
use crate::config::Source;
use crate::deck::{Group, Item};
use crate::i18n::{self, t};
use crate::installs;
use crate::registry::Kind as ProjectKind;
use crate::runs::{End, Run};

/// Что попросили на странице: выполняется после отрисовки.
enum Action {
    Back,
    Page(String),
    /// Запустить с этим профилем (он же станет выбранным).
    Launch(Box<Item>, String),
    /// Выбрать профиль для главной кнопки, не запуская.
    Choose(Box<Item>, String),
    /// Главное действие предмета, как Enter на Пульте.
    Main(Box<Item>),
    /// Игра: exe из экспорта, а нет его — из исходников.
    Play(Box<Item>),
    Rebuild(Box<Item>),
    Install(Box<Item>),
    FromCode(Box<Item>),
    FromSource(Box<Item>),
    Export(Box<Item>),
    Editor(Box<Item>),
    Focus(String, u32),
    Stop(String, u32, PathBuf),
    StopNow(String, u32),
    Forge(Box<Item>),
    Pin(String),
    Remove(String),
    Profiles(PathBuf),
    Folder(PathBuf),
    File(PathBuf),
    Copy(String),
    InstallCode(PathBuf, String),
    Rollback(PathBuf, String, String),
    Url(String),
    GodotPath,
    AmberAdmin(PathBuf),
    /// Место в коде из журнала: файл, строка, столбец, папка проекта.
    CodeAt(PathBuf, u32, u32, PathBuf),
}

pub fn show(app: &mut App, ui: &mut Ui, frame: &Frame) {
    let key = app.deck_view.page.clone().unwrap_or_default();
    let Some(index) = frame.items.iter().position(|i| i.key == key) else {
        // Предмета больше нет (убрали, проект скрыли) — обратно на Пульт.
        app.deck_view.page = None;
        super::deck::show(app, ui, frame);
        return;
    };
    let (item, look) = (&frame.items[index], &frame.looks[index]);
    // Верх страницы: от него журнал службы меряет свою высоту независимо от прокрутки.
    let origin = ui.min_rect().top();
    let mut actions = keyboard(app, ui.ctx(), item, look);
    let back = w::button(ui, Kind::Ghost, Some(Icon::ArrowLeft), t("Пульт")).on_hover_text("Esc");
    if app.deck_view.scroll {
        back.scroll_to_me(Some(egui::Align::TOP));
    }
    if back.clicked() {
        actions.push(Action::Back);
    }
    ui.add_space(12.0);
    match item.kind {
        ProjectKind::Godot => godot(app, ui, item, look, &mut actions),
        ProjectKind::Unity => unity(app, ui, item, look, &mut actions),
        _ if item.group == Group::Services => services(app, ui, frame, item, look, origin, &mut actions),
        _ => program(app, ui, frame, item, look, &mut actions),
    }
    apply(app, ui.ctx(), actions);
}

/// Клавиши страницы — только когда ничто другое их не ждёт (поле в фокусе, меню, диалог).
fn keyboard(app: &App, ctx: &egui::Context, item: &Item, look: &Look) -> Vec<Action> {
    let mut actions = Vec::new();
    let free = app.palette.is_none()
        && ctx.memory(|m| m.focused().is_none() && m.top_modal_layer().is_none())
        && !ctx.any_popup_open();
    if !free {
        return actions;
    }
    let fresh_enter = ctx.input(|i| {
        i.events.iter().any(|e| matches!(e, egui::Event::Key { key: Key::Enter, pressed: true, repeat: false, .. }))
    });
    let (back, forge, from_code, _alt, enter) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::Escape) || i.consume_key(Modifiers::ALT, Key::ArrowLeft),
            i.consume_key(Modifiers::SHIFT, Key::Enter),
            i.consume_key(Modifiers::COMMAND, Key::Enter),
            // Alt+Enter на странице ничего не делает (на Пульте он её открывает) — только бы не дошёл до Enter.
            i.consume_key(Modifiers::ALT, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Enter),
        )
    });
    if back {
        actions.push(Action::Back);
    }
    let code = matches!(item.kind, ProjectKind::Rust | ProjectKind::Godot) && !item.is_self();
    if fresh_enter && forge {
        actions.push(Action::Forge(Box::new(item.clone())));
    } else if fresh_enter && from_code && code && !app.launching(&item.key) && look.running.is_none() {
        actions.push(Action::FromCode(Box::new(item.clone())));
    } else if fresh_enter
        && enter
        && !app.launching(&item.key)
        && let Some(action) = primary(app, item, look).action
        // Enter не останавливает работающую службу: пересборка — только кнопкой.
        && !matches!(action, Action::Rebuild(_))
    {
        actions.push(action);
    }
    actions
}

/// Главная кнопка страницы — она же Enter.
struct Primary {
    icon: Icon,
    text: String,
    /// Подсказка: что именно случится или почему недоступно.
    hint: String,
    action: Option<Action>,
}

fn primary(app: &App, item: &Item, look: &Look) -> Primary {
    let boxed = || Box::new(item.clone());
    let make = |icon, text: &str, hint: String, action| Primary { icon, text: text.to_owned(), hint, action };
    match item.kind {
        ProjectKind::Godot => match look.main {
            Main::Focus(pid) => {
                make(Icon::Window, t("К окну"), String::new(), Some(Action::Focus(item.name.clone(), pid)))
            }
            // Экспорта нет — «Играть» запускает из исходников.
            Main::Launch | Main::Editor => make(Icon::Play, t("Играть"), String::new(), Some(Action::Play(boxed()))),
            _ => make(Icon::Play, t("Играть"), look.hint.clone(), None),
        },
        ProjectKind::Unity => {
            let version = engine(app, item).and_then(|e| e.version.clone()).unwrap_or_default();
            let open = engine(app, item).is_some_and(|e| e.open);
            let text = if open {
                t("Unity уже открыт").to_owned()
            } else {
                format!("{} {version}", t("Открыть в Unity"))
            };
            let action = (look.main == Main::Editor).then(|| Action::Editor(boxed()));
            let hint = if action.is_some() { String::new() } else { look.hint.clone() };
            Primary { icon: Icon::Pencil, text, hint, action }
        }
        _ if item.is_self() => make(Icon::Play, t("Это окно"), t("Это окно").to_owned(), None),
        _ => {
            let profile = app.profile_of(item);
            let bin = item.bin.clone().unwrap_or_default();
            let installed = app.installs.get(&bin).is_some_and(Option::is_some);
            let with = |text: &str| format!("{text} · {}", profile.label());
            if item.group == Group::Services {
                let rebuild = t("Пересобрать и перезапустить");
                // Заменяется только своя сборка из кода; установленная и чужая — через «Остановить…».
                if let Some(run) = app.runs.iter().rev().find(|r| r.key == item.key && r.running() && r.from_code) {
                    if app.rebuilding(&item.key) {
                        return make(Icon::Refresh, rebuild, t("Уже пересобирается — ход в консоли").to_owned(), None);
                    }
                    let hint = format!(
                        "cargo build --release --bin {bin}; {} PID {} {}. {}",
                        t("затем остановить"),
                        run.pid,
                        t("и запустить с тем же профилем"),
                        t("Сборка не прошла — служба работает дальше.")
                    );
                    return make(Icon::Refresh, rebuild, hint, Some(Action::Rebuild(boxed())));
                }
                if look.running.is_some() {
                    let tracked = app.runs.iter().any(|r| r.key == item.key && r.running());
                    let hint = if tracked {
                        t("Работает установленная копия: пересобрать и перезапустить можно только сборку из кода")
                    } else {
                        t("Запущена не из Anvil — сначала остановите её")
                    };
                    return make(Icon::Refresh, rebuild, hint.to_owned(), None);
                }
            } else if let Some(run) =
                app.runs.iter().rev().find(|r| r.key == item.key && r.running() && r.profile == profile.name)
            {
                return Primary {
                    icon: Icon::Window,
                    text: with(t("К окну")),
                    hint: String::new(),
                    action: Some(Action::Focus(item.name.clone(), run.pid)),
                };
            } else if let Some((pid, _)) = look.running
                && !app.runs.iter().any(|r| r.key == item.key && r.running())
            {
                // Работает копия не из Anvil — как и на Пульте, её окно, а не вторая копия.
                return make(Icon::Window, t("К окну"), String::new(), Some(Action::Focus(item.name.clone(), pid)));
            }
            match look.main {
                Main::Install => make(Icon::Download, &look.hint, String::new(), Some(Action::Install(boxed()))),
                _ if installed && profile.source == Source::Installed => Primary {
                    icon: Icon::Play,
                    text: with(t("Запустить")),
                    hint: String::new(),
                    action: Some(Action::Launch(boxed(), profile.name.clone())),
                },
                _ => Primary {
                    icon: Icon::Play,
                    text: with(t("Собрать и запустить")),
                    hint: format!("cargo build --release --bin {bin}"),
                    action: Some(Action::Launch(boxed(), profile.name.clone())),
                },
            }
        }
    }
}

// ─── Общие части ────────────────────────────────────────────────────────────

fn engine<'a>(app: &'a App, item: &Item) -> Option<&'a crate::engines::Info> {
    app.projects.iter().find(|p| p.path == item.project).and_then(|p| p.engine.as_ref())
}

fn git<'a>(app: &'a App, item: &Item) -> Option<&'a crate::git::GitState> {
    app.projects.iter().find(|p| p.path == item.project).and_then(|p| p.git())
}

/// Часть подписи под заголовком: текст или моно.
enum Sub {
    Text(String),
    Mono(String),
}

/// Заголовок страницы: знак 56, имя 24 с бейджами, подпись; справа — кнопки (`right` рисует их
/// справа налево).
fn header(ui: &mut Ui, item: &Item, badges: &[(String, Tone)], sub: &[Sub], right: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    // Подпись одной строкой: не влезла — многоточие, целиком — в подсказке.
    let mut job = egui::text::LayoutJob::default();
    for (n, part) in sub.iter().enumerate() {
        if n > 0 {
            job.append(" · ", 0.0, egui::TextFormat::simple(egui::FontId::proportional(13.0), p.weak));
        }
        match part {
            Sub::Text(text) => {
                job.append(text, 0.0, egui::TextFormat::simple(egui::FontId::proportional(13.0), p.weak))
            }
            Sub::Mono(text) => job.append(text, 0.0, egui::TextFormat::simple(egui::FontId::monospace(12.5), p.weak)),
        }
    }
    let full = job.text.clone();
    ui.horizontal(|ui| {
        ui.set_min_height(56.0);
        w::item_mark(ui, item.mark.accent, item.mark.icon, 56.0);
        ui.add_space(16.0);
        // Кнопки — первыми, справа; имя и подпись — в том, что осталось.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            right(ui);
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new(&item.name).font(semibold(24.0)).color(p.text));
                    ui.add_space(6.0);
                    for (text, tone) in badges {
                        w::badge(ui, text, *tone);
                    }
                });
                ui.add(egui::Label::new(job).truncate()).on_hover_text(full);
            });
        });
    });
    ui.add_space(18.0);
}

/// Кнопки справа в заголовке, общие для всех страниц: `More`, папка, Кузница.
fn common_buttons(app: &App, ui: &mut Ui, item: &Item, look: &Look, actions: &mut Vec<Action>) {
    let more = w::icon_button(ui, Icon::More, t("Ещё действия"));
    w::menu(&more, 260.0, |ui| {
        if item.kind == ProjectKind::Rust
            && !item.is_self()
            && w::menu_item(ui, Some(Icon::Hammer), t("Запустить из кода"), None).clicked()
        {
            actions.push(Action::FromCode(Box::new(item.clone())));
        }
        let pinned = app.config.deck.pinned.contains(&item.key);
        let pin = if pinned { t("Открепить") } else { t("Закрепить") };
        if w::menu_item(ui, Some(Icon::Pin), pin, None).clicked() {
            actions.push(Action::Pin(item.key.clone()));
        }
        if w::menu_item(ui, Some(Icon::Folder), t("Папка проекта"), None).clicked() {
            actions.push(Action::Folder(item.project.clone()));
        }
        w::menu_separator(ui);
        if w::menu_item_danger(ui, Some(Icon::Close), t("Убрать с Пульта")).clicked() {
            actions.push(Action::Remove(item.key.clone()));
        }
    });
    let (hint, dir) = match &look.install_dir {
        Some(dir) => (t("Папка установки"), dir.clone()),
        None => (t("Папка проекта"), item.project.clone()),
    };
    if w::icon_button(ui, Icon::Folder, hint).clicked() {
        actions.push(Action::Folder(dir));
    }
    if w::icon_button(ui, Icon::Hammer, &format!("{} · Shift+Enter", t("В Кузнице"))).clicked() {
        actions.push(Action::Forge(Box::new(item.clone())));
    }
    ui.add_space(4.0);
}

/// Главная кнопка простая (без меню).
fn primary_button(app: &App, ui: &mut Ui, item: &Item, look: &Look, actions: &mut Vec<Action>) {
    let primary = primary(app, item, look);
    let enabled = primary.action.is_some() && !app.launching(&item.key);
    let r = ui.add_enabled_ui(enabled, |ui| w::button(ui, Kind::Primary, Some(primary.icon), &primary.text)).inner;
    let r =
        if primary.hint.is_empty() { r } else { r.on_hover_text(&primary.hint).on_disabled_hover_text(&primary.hint) };
    if r.clicked()
        && let Some(action) = primary.action
    {
        actions.push(action);
    }
}

/// Карточки в ряд одной высоты. `shares` — доли ширины; в узком окне — друг под другом.
fn cards(ui: &mut Ui, id: egui::Id, shares: &[f32], mut content: impl FnMut(usize, &mut Ui)) {
    let gap = 14.0;
    // Уже ~1250 — три карточки в ряд тесны (ключ 110 и значения не влезают): друг под другом.
    if ui.available_width() < 1250.0 {
        for i in 0..shares.len() {
            w::card(ui, |ui| content(i, ui));
            ui.add_space(gap);
        }
        return;
    }
    // Высота — по самой высокой карточке прошлого кадра.
    let height_id = id.with("height");
    let height: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(0.0);
    let mut tallest = 0.0_f32;
    let sum: f32 = shares.iter().sum();
    let free = ui.available_width() - gap * (shares.len() - 1) as f32;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, share) in shares.iter().enumerate() {
            let width = (free * share / sum).floor();
            ui.allocate_ui_with_layout(Vec2::new(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(width);
                w::card_frame(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let top = ui.min_rect().top();
                    content(i, ui);
                    let natural = ui.min_rect().bottom() - top;
                    tallest = tallest.max(natural);
                    // Добить до общей высоты. `set_min_height` считает от курсора, а не от верха.
                    let gap = ui.cursor().top() - ui.min_rect().bottom();
                    let fill = height - natural - gap;
                    if fill > 0.5 {
                        ui.add_space(fill);
                    }
                });
            });
        }
    });
    if (tallest - height).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(height_id, tallest));
        ui.ctx().request_repaint();
    }
    ui.add_space(18.0);
}

/// Радио профиля: кольцо, у выбранного — точка акцента.
fn radio(ui: &mut Ui, on: bool, label: &str) -> egui::Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::click());
    let color = if on { p.accent } else { p.weak };
    ui.painter().circle_stroke(rect.center(), 7.0, egui::Stroke::new(1.5, color));
    if on {
        ui.painter().circle_filled(rect.center(), 4.0, p.accent);
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Точка или кольцо и текст: «работает», «не запущен».
fn state_line(ui: &mut Ui, tone: Option<Tone>, text: &str) {
    let p = Palette::of(ui);
    match tone {
        Some(tone) => w::dot(ui, tone),
        None => w::ring(ui),
    };
    let color = if tone.is_some() { p.text } else { p.weak };
    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(color)).truncate());
}

/// Ссылка текстом акцента.
fn link(ui: &mut Ui, text: &str) -> egui::Response {
    let p = Palette::of(ui);
    let r = ui.add(egui::Label::new(RichText::new(text).size(13.0).color(p.accent_text)).sense(egui::Sense::click()));
    if r.hovered() {
        let y = r.rect.bottom() - 1.0;
        ui.painter().hline(r.rect.x_range(), y, egui::Stroke::new(1.0, p.accent_text));
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Моно с переносом в оставшейся ширине (команды, пути).
fn mono_wrap(ui: &mut Ui, text: &str, color: egui::Color32, reserve: f32) {
    let width = (ui.available_width() - reserve).max(80.0);
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_width(width);
        ui.add(egui::Label::new(RichText::new(text).font(egui::FontId::monospace(12.5)).color(color)).wrap());
    });
}

/// Итог запуска словами и тон точки.
pub(super) fn outcome(run: &Run) -> (String, Option<Tone>) {
    let took = run.ended.unwrap_or(run.started) - run.started;
    let code = run.code.map(crate::runs::code_text).unwrap_or_default();
    match run.end {
        End::Running => {
            (format!("{} · {}", t("работает"), i18n::uptime(i18n::now() - run.started)), Some(Tone::Success))
        }
        End::Closed => (format!("{} {} · {} {code}", t("закрыт через"), i18n::span(took), t("код")), None),
        End::Crashed => (format!("{} {} · {} {code}", t("упал через"), i18n::span(took), t("код")), Some(Tone::Danger)),
        End::Stopped => (format!("{} {}", t("остановлен через"), i18n::span(took)), None),
        End::Lost => (t("закрылся").to_owned(), None),
    }
}

/// Чип источника запуска из истории: «сборка 2353af9», «установлена 0.4.0».
fn run_chip(run: &Run) -> (String, Option<String>) {
    match run.source.split_once(' ') {
        Some((word, rest)) => (i18n::source_word(word), Some(rest.to_owned())),
        None => (i18n::source_word(&run.source), None),
    }
}

/// Файл вывода запуска, если он есть на диске.
fn log_file(run: &Run) -> Option<PathBuf> {
    run.log.clone().filter(|p| p.is_file())
}

/// Сборка из кода: хеш коммита, если собирал Anvil (и exe с тех пор не пересобирали), иначе дата.
fn build_label(app: &App, item: &Item) -> Option<(String, Option<crate::builds::Build>, i64)> {
    let bin = item.bin.as_deref()?;
    let at = app.builds.get(bin).copied().flatten()?;
    let target = app.projects.iter().find(|p| p.path == item.project).and_then(|p| p.meta())?.target_dir.clone();
    let exe = crate::launch::exe_path(&target, true, bin);
    let build = app.build_info.get(&crate::builds::key(&exe)).filter(|b| b.at + 5 >= at).cloned();
    let label = build.as_ref().map_or_else(|| i18n::date(at), crate::builds::label);
    Some((label, build, at))
}

/// Коммитов после сборки (по списку последних коммитов); `None` — коммита сборки в нём нет.
fn commits_since(git: &crate::git::GitState, commit: &str) -> Option<usize> {
    git.commits.iter().position(|c| c.hash == commit || c.full.starts_with(commit))
}

/// Вкладки с историей запусков: строки по §5.5.
fn history(app: &App, ui: &mut Ui, item: &Item, actions: &mut Vec<Action>) {
    let runs: Vec<&Run> = app.runs.iter().rev().filter(|r| r.key == item.key).take(30).collect();
    if runs.is_empty() {
        w::card(ui, |ui| {
            w::empty_state(
                ui,
                Icon::Clock,
                t("Запусков ещё не было"),
                t("Здесь появятся запуски из Anvil: когда, с каким профилем и чем кончились."),
            )
        });
        return;
    }
    let p = Palette::of(ui);
    // Файл вывода у профиля один (новый запуск дописывает его после прошлого): «Журнал» — только у
    // последнего запуска с этим файлом, иначе кнопка открыла бы чужой вывод.
    let mut logs = std::collections::HashSet::new();
    w::card_frame(ui).inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.style_mut().interaction.selectable_labels = false;
        for (n, run) in runs.iter().enumerate() {
            let log = log_file(run).filter(|p| logs.insert(p.clone()));
            ui.push_id(run.id, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_height(36.0);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let when = format!("{} {}", i18n::date(run.started), i18n::clock(run.started));
                    let cell = |ui: &mut Ui, width: f32, add: &mut dyn FnMut(&mut Ui)| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(width, 36.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_width(width);
                                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                                add(ui);
                            },
                        );
                    };
                    cell(ui, 112.0, &mut |ui| {
                        ui.label(RichText::new(&when).font(egui::FontId::monospace(12.5)).color(p.weak));
                    });
                    // У игр профилей нет: «Игра» (экспорт) или «Из исходников».
                    let profile = match (item.kind, run.profile.is_empty()) {
                        (ProjectKind::Godot | ProjectKind::Unity, _) if run.source.starts_with("исходники") => {
                            t("Из исходников").to_owned()
                        }
                        (ProjectKind::Godot | ProjectKind::Unity, _) => t("Игра").to_owned(),
                        (_, true) => t("обычный").to_owned(),
                        (_, false) => run.profile.clone(),
                    };
                    cell(ui, 132.0, &mut |ui| {
                        ui.add(egui::Label::new(RichText::new(&profile).size(14.0).color(p.text)).truncate());
                    });
                    let (word, mono) = run_chip(run);
                    cell(ui, 202.0, &mut |ui| {
                        w::source_chip(ui, &word, mono.as_deref());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        match (log.clone(), run.running() && !run.service) {
                            (_, true) => {
                                if w::button(ui, Kind::Ghost, None, t("К окну")).clicked() {
                                    actions.push(Action::Focus(run.name.clone(), run.pid));
                                }
                            }
                            (Some(path), false) => {
                                if w::button(ui, Kind::Ghost, None, t("Журнал"))
                                    .on_hover_text(path.display().to_string())
                                    .clicked()
                                {
                                    actions.push(Action::File(path));
                                }
                            }
                            (None, false) => {}
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let (text, tone) = outcome(run);
                            state_line(ui, tone, &text);
                        });
                    });
                });
            });
            if n + 1 < runs.len() {
                w::divider(ui);
            }
        }
    });
    if runs.len() == 30 {
        ui.add_space(6.0);
        w::note(ui, t("Показаны последние 30 запусков."));
    }
}

/// Вкладка «Папки»: подпись, путь, кнопка.
fn folders(ui: &mut Ui, rows: &[(&str, PathBuf)], actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    w::card_frame(ui).inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for (n, (label, path)) in rows.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.set_min_height(36.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(150.0, 20.0), egui::Sense::hover());
                ui.painter().text(
                    egui::pos2(rect.left(), rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    *label,
                    egui::FontId::proportional(13.0),
                    p.weak,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::icon_button(ui, Icon::Folder, t("Открыть папку")).clicked() {
                        actions.push(Action::Folder(path.clone()));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let text = path.display().to_string();
                        ui.add(
                            egui::Label::new(RichText::new(&text).font(egui::FontId::monospace(12.5)).color(p.text))
                                .truncate(),
                        )
                        .on_hover_text(&text);
                    });
                });
            });
            if n + 1 < rows.len() {
                w::divider(ui);
            }
        }
    });
}

/// Коммиты проекта: ветка, чисто ли, ссылка на GitHub, последние коммиты.
fn commits(app: &App, ui: &mut Ui, item: &Item, limit: usize, actions: &mut Vec<Action>) {
    let Some(git) = git(app, item).cloned() else {
        w::card(ui, |ui| w::empty_state(ui, Icon::Branch, t("Папка не под git — истории нет"), ""));
        return;
    };
    let p = Palette::of(ui);
    w::card_frame(ui).inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.style_mut().interaction.selectable_labels = false;
        ui.horizontal(|ui| {
            ui.set_min_height(36.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
            anvil_ui::icons::paint(ui.painter(), rect, Icon::Branch, p.weak);
            w::mono(ui, git.branch.as_deref().unwrap_or("HEAD"), Some(p.text));
            ui.add_space(4.0);
            if git.dirty() {
                let files = i18n::count(git.changes.len(), ["файл", "файла", "файлов"], ["file", "files"]);
                state_line(ui, Some(Tone::Warning), &format!("{} · {files}", t("есть правки")));
            } else {
                state_line(ui, Some(Tone::Success), t("чисто"));
            }
            if let Some(url) = git.github() {
                ui.label(RichText::new("·").color(p.weak));
                let slug = url.trim_start_matches("https://github.com/").to_owned();
                if link(ui, &slug).clicked() {
                    actions.push(Action::Url(url));
                }
            }
        });
        for commit in git.commits.iter().take(limit) {
            w::divider(ui);
            ui.horizontal(|ui| {
                ui.set_min_height(36.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_sized(
                    Vec2::new(70.0, 20.0),
                    egui::Label::new(
                        RichText::new(&commit.hash).font(egui::FontId::monospace(12.5)).color(p.accent_text),
                    ),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let when = format!("{} {}", i18n::date(commit.time), i18n::clock(commit.time));
                    ui.label(RichText::new(when).font(egui::FontId::monospace(12.5)).color(p.weak));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(&commit.subject).size(14.0).color(p.text)).truncate())
                            .on_hover_text(format!("{} · {}", commit.author, commit.subject));
                    });
                });
            });
        }
        if git.commits.len() > limit {
            w::divider(ui);
            ui.horizontal(|ui| {
                ui.set_min_height(32.0);
                if link(ui, &i18n::more_in_forge(git.commits.len() - limit)).clicked() {
                    actions.push(Action::Forge(Box::new(item.clone())));
                }
            });
        }
    });
}

// ─── Программа (§7.2) ───────────────────────────────────────────────────────

fn program(app: &mut App, ui: &mut Ui, frame: &Frame, item: &Item, look: &Look, actions: &mut Vec<Action>) {
    let bin = item.bin.clone().unwrap_or_default();
    let project = app.projects.iter().find(|p| p.path == item.project);
    let meta = project.and_then(|p| p.meta()).cloned();
    let installed = app.installs.get(&bin).and_then(Option::as_ref).cloned();

    let mut badges = Vec::new();
    match installed.as_ref().and_then(|i| i.current.as_deref()) {
        Some(version) => badges.push((format!("{} {version}", t("установлена")), Tone::Neutral)),
        None if !item.is_self() => badges.push((t("не установлена").to_owned(), Tone::Neutral)),
        None => {}
    }
    if look.running.is_some() {
        badges.push((t("работает").to_owned(), Tone::Success));
    }
    let mut sub = Vec::new();
    if let Some(description) = meta.as_ref().and_then(|m| m.description.clone()).filter(|d| !d.is_empty()) {
        sub.push(Sub::Text(description));
    }
    sub.push(Sub::Mono(bin.clone()));
    sub.push(Sub::Mono(item.project.display().to_string()));
    header(ui, item, &badges, &sub, |ui| {
        common_buttons(app, ui, item, look, actions);
        let primary = primary(app, item, look);
        // «К окну» — у программы, запущенной не тем профилем, что на главной кнопке.
        if let Some((pid, _)) = look.running.filter(|_| !look.this)
            && !matches!(primary.action, Some(Action::Focus(..)))
        {
            if w::button(ui, Kind::Secondary, Some(Icon::Window), t("К окну")).clicked() {
                actions.push(Action::Focus(item.name.clone(), pid));
            }
            ui.add_space(4.0);
        }
        split_primary(app, ui, item, primary, actions);
    });

    let presets = app.config.project(&item.project).presets;
    let profiles = crate::deck::profiles(item, &presets, installed.is_some());
    let chosen = app.config.deck.profile.get(&item.key).cloned().unwrap_or_default();
    let neighbors: Vec<(&Item, &Look)> =
        frame.items.iter().zip(&frame.looks).filter(|(i, _)| i.project == item.project && i.key != item.key).collect();
    let tools: Vec<String> = meta
        .as_ref()
        .map(|m| {
            m.bins
                .iter()
                .filter(|b| b.hint.hidden || crate::deck::role(b) == crate::deck::Role::Tool)
                .map(|b| b.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let version_card = version_data(app, item, meta.as_ref(), installed.as_ref());
    // Какой профиль запустит Enter (главная кнопка) — у его строки подсказка «· Enter».
    let enter = match primary(app, item, look).action {
        Some(Action::Launch(_, name)) => Some(name),
        _ => None,
    };
    cards(ui, egui::Id::new(("page-cards", &item.key)), &[620.0, 370.0, 366.0], |i, ui| match i {
        0 => profiles_card(app, ui, item, &profiles, &chosen, enter.as_deref(), installed.as_ref(), actions),
        1 => version_card_ui(ui, item, &version_card, actions),
        _ => neighbors_card(ui, item, &neighbors, &tools, actions),
    });

    let count = app.runs.iter().filter(|r| r.key == item.key).count();
    let runs_tab = format!("{} · {count}", t("Запуски"));
    let mut tab = app.deck_view.page_tab.min(2);
    w::tabs(ui, &mut tab, &[&runs_tab, t("Что нового"), t("Папки")]);
    app.deck_view.page_tab = tab;
    ui.add_space(12.0);
    match tab {
        0 => history(app, ui, item, actions),
        1 => whats_new(app, ui, item, &bin, actions),
        _ => {
            let mut rows = vec![(t("Проект"), item.project.clone())];
            if installed.is_some() {
                rows.push((t("Установка"), installs::root(&bin)));
            }
            if let Some(meta) = &meta {
                let exe = crate::launch::exe_path(&meta.target_dir, true, &bin);
                if let Some(dir) = exe.parent().filter(|d| d.is_dir()) {
                    rows.push((t("Сборка из кода"), dir.to_path_buf()));
                }
            }
            // Журналы — там, где лежит вывод последнего запуска (у каждого профиля своя папка).
            let last_log = app.runs.iter().rev().filter(|r| r.key == item.key).find_map(|r| r.log.clone());
            if let Some(dir) = last_log.and_then(|p| p.parent().map(Path::to_path_buf)).filter(|d| d.is_dir()) {
                rows.push((t("Журналы"), dir));
            }
            folders(ui, &rows, actions);
        }
    }
}

/// Раздельная главная кнопка: слева действие, в меню — профили.
fn split_primary(app: &App, ui: &mut Ui, item: &Item, primary: Primary, actions: &mut Vec<Action>) {
    let enabled = primary.action.is_some() && !app.launching(&item.key);
    let split = ui
        .add_enabled_ui(!item.is_self(), |ui| {
            w::split_button(ui, Kind::Primary, Some(primary.icon), &primary.text, t("Профили"))
        })
        .inner;
    let main = if primary.hint.is_empty() { split.main } else { split.main.on_hover_text(&primary.hint) };
    if main.clicked()
        && enabled
        && let Some(action) = primary.action
    {
        actions.push(action);
    }
    w::menu(&split.menu, 300.0, |ui| profiles_menu(app, ui, item, actions));
}

/// Меню профилей главной кнопки.
fn profiles_menu(app: &App, ui: &mut Ui, item: &Item, actions: &mut Vec<Action>) {
    let installed = item.bin.as_ref().is_some_and(|b| app.installs.get(b).is_some_and(Option::is_some));
    let presets = app.config.project(&item.project).presets;
    let chosen = app.config.deck.profile.get(&item.key).cloned().unwrap_or_default();
    for profile in crate::deck::profiles(item, &presets, installed) {
        let running = app.runs.iter().rev().find(|r| r.key == item.key && r.running() && r.profile == profile.name);
        let text = match running {
            Some(_) if item.group == Group::Services => format!("{} — {}", profile.label(), t("работает")),
            Some(_) => format!("{} — {}", profile.label(), t("запущен · К окну")),
            None if profile.name == chosen => format!("{} {}", profile.label(), t("(выбран)")),
            None => profile.label(),
        };
        let icon = if profile.name == chosen { Icon::Check } else { Icon::Play };
        if w::menu_item(ui, Some(icon), &text, None).clicked() {
            actions.push(Action::Launch(Box::new(item.clone()), profile.name.clone()));
        }
    }
    w::menu_separator(ui);
    if w::menu_item(ui, Some(Icon::Pencil), t("Профили…"), None).clicked() {
        actions.push(Action::Profiles(item.project.clone()));
    }
}

#[allow(clippy::too_many_arguments)]
fn profiles_card(
    app: &App,
    ui: &mut Ui,
    item: &Item,
    profiles: &[crate::deck::Profile],
    chosen: &str,
    enter: Option<&str>,
    installed: Option<&installs::Installed>,
    actions: &mut Vec<Action>,
) {
    let p = Palette::of(ui);
    w::card_title(ui, Icon::Play, t("Профили запуска"));
    for (n, profile) in profiles.iter().enumerate() {
        if n > 0 {
            w::divider(ui);
        }
        let selected = profile.name == chosen;
        let last = app.runs.iter().rev().find(|r| r.key == item.key && r.profile == profile.name);
        let running = last.filter(|r| r.running());
        ui.push_id(("profile", n), |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                let label = profile.label();
                let radio = radio(ui, selected, &label);
                let font = if selected { semibold(14.0) } else { egui::FontId::proportional(14.0) };
                let name = ui
                    .add(egui::Label::new(RichText::new(&label).font(font).color(p.text)).sense(egui::Sense::click()));
                if (radio.clicked() || name.clicked()) && !selected {
                    actions.push(Action::Choose(Box::new(item.clone()), profile.name.clone()));
                }
                let args = |ui: &mut Ui| {
                    if profile.args.trim().is_empty() {
                        ui.label(RichText::new(t("без аргументов")).size(13.0).color(p.weak));
                    } else {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&profile.args).font(egui::FontId::monospace(12.5)).color(p.weak),
                            )
                            .truncate(),
                        );
                    }
                };
                if item.is_self() {
                    args(ui);
                    return;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    match running {
                        Some(run) => {
                            let (stop, action) = if run.from_code {
                                (t("Остановить"), Action::StopNow(item.name.clone(), run.pid))
                            } else {
                                (t("Остановить…"), Action::Stop(item.name.clone(), run.pid, item.project.clone()))
                            };
                            if w::icon_button(ui, Icon::Stop, stop).clicked() {
                                actions.push(action);
                            }
                            if !run.service && w::icon_button(ui, Icon::Window, t("К окну")).clicked() {
                                actions.push(Action::Focus(item.name.clone(), run.pid));
                            }
                        }
                        None => {
                            if w::icon_button(ui, Icon::Pencil, t("Изменить профиль…")).clicked() {
                                actions.push(Action::Profiles(item.project.clone()));
                            }
                            let hint = if enter == Some(profile.name.as_str()) {
                                format!("{} · Enter", t("Запустить"))
                            } else {
                                t("Запустить").to_owned()
                            };
                            let enabled = !app.launching(&item.key);
                            if ui.add_enabled_ui(enabled, |ui| w::icon_button(ui, Icon::Play, &hint)).inner.clicked() {
                                actions.push(Action::Launch(Box::new(item.clone()), profile.name.clone()));
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        args(ui);
                    });
                });
            });
            ui.horizontal(|ui| {
                ui.set_min_height(24.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add_space(24.0);
                let (word, mono) = match running {
                    Some(run) => run_chip(run),
                    None if profile.source == Source::Installed && installed.is_some() => {
                        (t("установлена").to_owned(), installed.and_then(|i| i.current.clone()))
                    }
                    None => match build_label(app, item) {
                        Some((label, _, _)) => (t("сборка").to_owned(), Some(label)),
                        None => (t("не собран").to_owned(), None),
                    },
                };
                w::source_chip(ui, &word, mono.as_deref());
                ui.add_space(4.0);
                match (running, last) {
                    (Some(run), _) => {
                        let text = format!(
                            "{} · PID {} · {}",
                            t("работает"),
                            run.pid,
                            i18n::uptime(i18n::now() - run.started)
                        );
                        state_line(ui, Some(Tone::Success), &text);
                    }
                    (None, Some(run)) if run.end == End::Crashed => {
                        let took = run.ended.unwrap_or(run.started) - run.started;
                        let code = run.code.map(crate::runs::code_text).unwrap_or_default();
                        let text = format!(
                            "{} · {} {code}",
                            i18n::crashed_at(run.ended.unwrap_or(run.started), took),
                            t("код")
                        );
                        state_line(ui, Some(Tone::Danger), &text);
                    }
                    _ => state_line(ui, None, t("не запущен")),
                }
                if let Some(path) = last.and_then(log_file) {
                    ui.add_space(2.0);
                    if link(ui, t("Журнал")).on_hover_text(path.display().to_string()).clicked() {
                        actions.push(Action::File(path));
                    }
                }
            });
        });
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if w::button(ui, Kind::Ghost, Some(Icon::Plus), t("Профиль…")).clicked() {
            actions.push(Action::Profiles(item.project.clone()));
        }
        ui.add_space(8.0);
        w::note(ui, t("Главная кнопка запускает выбранный профиль"));
    });
}

/// Что показывает карточка «Версия» — считается до отрисовки.
struct VersionCard {
    installed: Option<String>,
    release: Option<(String, Option<String>)>,
    /// Выпуск новее установленного.
    newer: bool,
    /// Коммитов после тега, хеш HEAD, когда.
    code: Option<(u32, String, i64, bool)>,
    build: Option<(String, i64)>,
    /// Из какого коммита сборка, известно (собирал Anvil).
    build_known: bool,
    note: String,
    /// На что откатиться: предыдущая установленная версия.
    previous: Option<String>,
    install_label: Option<String>,
    bin: String,
}

fn version_data(
    app: &App,
    item: &Item,
    meta: Option<&crate::registry::Meta>,
    installed: Option<&installs::Installed>,
) -> VersionCard {
    let bin = item.bin.clone().unwrap_or_default();
    let remote = app.remotes.get(&item.project);
    let release = super::install::release_for(remote, &bin, app.config.common.prerelease);
    let newer = super::install::newer(installed, release.as_ref()).is_some();
    let git = git(app, item);
    let head = git.and_then(|g| g.commits.first());
    let code = git.zip(head).map(|(g, c)| (g.since_tag, c.hash.clone(), c.time, g.dirty()));
    let built = build_label(app, item);
    let version = meta.and_then(|m| m.version.clone()).unwrap_or_else(|| "0.0.0".into());
    let build = built.as_ref().map(|(label, build, at)| match build {
        Some(b) => (installs::local_label(&version, Some(&b.commit), b.dirty), b.at),
        None => (label.clone(), *at),
    });
    let mut notes = Vec::new();
    match (git, built.as_ref().and_then(|(_, b, _)| b.as_ref())) {
        (Some(git), Some(build)) => match commits_since(git, &build.commit) {
            Some(0) if !git.dirty() => notes.push(t("Сборка — из последнего коммита.").to_owned()),
            Some(0) => notes.push(t("Сборка — из последнего коммита; с тех пор есть правки.").to_owned()),
            Some(n) => notes.push(format!(
                "{} {}.",
                t("После сборки —"),
                i18n::count(n, ["коммит", "коммита", "коммитов"], ["commit", "commits"])
            )),
            None => notes.push(t("Сборка старше последних коммитов.").to_owned()),
        },
        // Без git коммита сборки не знает никто — говорить не о чем.
        (Some(_), None) if built.is_some() => {
            notes.push(t("Сборку делали не из Anvil: из какого она коммита, неизвестно.").to_owned())
        }
        _ => {}
    }
    // Откат — на версию, поставленную раньше текущей (не «вперёд» после прошлого отката).
    let previous = installed.and_then(|i| {
        let current_at = i.versions.iter().find(|(v, _)| Some(v) == i.current.as_ref()).map(|(_, at)| *at)?;
        i.versions
            .iter()
            .filter(|(v, at)| Some(v) != i.current.as_ref() && *at < current_at)
            .max_by_key(|(_, at)| *at)
            .map(|(v, _)| v.clone())
    });
    match installed {
        Some(i) if previous.is_none() && i.versions.len() > 1 => {
            notes.push(t("Откатываться некуда: текущая — самая ранняя из установленных.").to_owned())
        }
        Some(_) if previous.is_none() => notes.push(t("Откатываться некуда: других версий нет.").to_owned()),
        _ => {}
    }
    let install_label = meta.map(|m| {
        installs::local_label(
            m.version.as_deref().unwrap_or("0.0.0"),
            head.map(|c| c.hash.as_str()),
            git.is_some_and(|g| g.dirty()),
        )
    });
    VersionCard {
        installed: installed.and_then(|i| i.current.clone()),
        release: release.map(|(r, v)| (format!("v{v}"), Some(r.url.clone()))),
        newer,
        code,
        build_known: built.as_ref().is_some_and(|(_, b, _)| b.is_some()),
        build,
        note: notes.join(" "),
        previous,
        install_label,
        bin,
    }
}

fn version_card_ui(ui: &mut Ui, item: &Item, card: &VersionCard, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    w::card_title(ui, Icon::Package, t("Версия"));
    ui.spacing_mut().item_spacing.y = 6.0;
    w::field_row(ui, t("Установлена"), |ui| match &card.installed {
        Some(v) => {
            w::mono(ui, v, Some(p.text));
            w::badge(ui, t("текущая"), Tone::Neutral);
        }
        None => {
            ui.label(RichText::new(t("не установлена")).size(13.0).color(p.weak));
        }
    });
    w::field_row(ui, t("Выпуск"), |ui| match &card.release {
        Some((v, url)) => {
            let r = w::mono(ui, v, Some(p.text));
            if let Some(url) = url {
                r.on_hover_text(url);
            }
            ui.spacing_mut().item_spacing.x = 6.0;
            if card.newer {
                state_line(ui, Some(Tone::Accent), t("новее установленной"));
            } else if card.installed.is_some() {
                state_line(ui, Some(Tone::Success), t("актуально"));
            }
        }
        None => {
            ui.label(RichText::new(t("на GitHub нет")).size(13.0).color(p.weak));
        }
    });
    w::field_row(ui, t("В коде"), |ui| match &card.code {
        Some((ahead, hash, time, dirty)) => {
            ui.spacing_mut().item_spacing.x = 6.0;
            if *ahead > 0 {
                let text = format!(
                    "+{}",
                    i18n::count(*ahead as usize, ["коммит", "коммита", "коммитов"], ["commit", "commits"])
                );
                w::badge(ui, &text, Tone::Accent);
            }
            w::mono(ui, hash, Some(p.accent_text));
            let when = if *dirty {
                format!("{} · {}", i18n::ago(*time), t("есть правки"))
            } else {
                i18n::ago(*time)
            };
            ui.add(egui::Label::new(RichText::new(when).size(13.0).color(p.weak)).truncate());
        }
        None => {
            ui.label(RichText::new(t("без git")).size(13.0).color(p.weak));
        }
    });
    w::field_row(ui, t("Сборка"), |ui| match &card.build {
        // Коммит сборки знает только Anvil; собранное не им — просто дата файла.
        Some((label, at)) if card.build_known => {
            ui.spacing_mut().item_spacing.x = 6.0;
            w::mono(ui, label, Some(p.text));
            let when = if i18n::when(*at) == i18n::clock(*at) { i18n::at(*at) } else { i18n::date(*at) };
            ui.label(RichText::new(when).size(13.0).color(p.weak));
        }
        Some((_, at)) => {
            ui.label(RichText::new(i18n::date_at(*at)).size(13.0).color(p.text));
        }
        None => {
            ui.label(RichText::new(t("не собрана")).size(13.0).color(p.weak));
        }
    });
    if !card.note.is_empty() {
        ui.add_space(4.0);
        w::note(ui, &card.note);
    }
    if item.is_self() {
        return;
    }
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        let hint =
            card.install_label.as_ref().map(|l| format!("cargo build --release --bin {} → versions\\{l}", card.bin));
        let r = w::button(ui, Kind::Secondary, Some(Icon::Download), t("Поставить из кода…"));
        let r = match &hint {
            Some(h) => r.on_hover_text(h),
            None => r,
        };
        if r.clicked() {
            actions.push(Action::InstallCode(item.project.clone(), card.bin.clone()));
        }
        let rollback = ui
            .add_enabled_ui(card.previous.is_some(), |ui| w::button(ui, Kind::Ghost, None, t("Откатить…")))
            .inner
            .on_disabled_hover_text(t("Более ранних установленных версий нет"));
        if rollback.clicked()
            && let Some(version) = &card.previous
        {
            actions.push(Action::Rollback(item.project.clone(), card.bin.clone(), version.clone()));
        }
    });
}

fn neighbors_card(ui: &mut Ui, item: &Item, neighbors: &[(&Item, &Look)], tools: &[String], actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let project = crate::worker::display_name(&item.project);
    w::card_title(ui, Icon::Layers, &i18n::from_project(&project));
    if neighbors.is_empty() && tools.is_empty() {
        w::note(ui, t("Других программ в проекте нет."));
        return;
    }
    ui.style_mut().interaction.selectable_labels = false;
    let rows = neighbors.len() + tools.len();
    let mut n = 0;
    for (other, look) in neighbors {
        ui.horizontal(|ui| {
            ui.set_min_height(36.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            w::item_mark(ui, other.mark.accent, other.mark.icon, 28.0);
            ui.add_space(4.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                // Не запущенную программу — запустить сразу; остальное — открыть её страницу.
                let launch = look.running.is_none() && other.group == Group::Programs && look.main != Main::Nothing;
                if launch {
                    if w::icon_button(ui, look.icon, &look.hint).clicked() {
                        actions.push(Action::Main(Box::new((*other).clone())));
                    }
                } else if w::icon_button(ui, Icon::ArrowRight, t("Открыть")).clicked() {
                    actions.push(Action::Page(other.key.clone()));
                }
                let tone = look.running.map(|_| Tone::Success);
                let state = match (look.running, look.port) {
                    (Some(_), Some(port)) => format!("{} · :{port}", t("работает")),
                    (Some(_), None) => t("работает").to_owned(),
                    (None, _) => look.state.clone(),
                };
                ui.allocate_ui_with_layout(
                    Vec2::new(140.0, 28.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(140.0);
                        ui.spacing_mut().item_spacing.x = 6.0;
                        state_line(ui, tone, &state);
                    },
                );
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(&other.name).size(14.0).color(p.text)).truncate());
                });
            });
        });
        n += 1;
        if n < rows {
            w::divider(ui);
        }
    }
    for tool in tools {
        ui.horizontal(|ui| {
            ui.set_min_height(36.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
            anvil_ui::icons::paint(ui.painter(), rect.shrink(6.0), Icon::Gear, p.weak);
            ui.add_space(4.0);
            w::mono(ui, tool, None);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(t("служебный · в Кузнице")).size(13.0).color(p.weak));
            });
        });
        n += 1;
        if n < rows {
            w::divider(ui);
        }
    }
}

fn whats_new(app: &App, ui: &mut Ui, item: &Item, bin: &str, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let remote = app.remotes.get(&item.project);
    let release = super::install::release_for(remote, bin, app.config.common.prerelease)
        .map(|(r, _)| r.clone())
        .or_else(|| remote.and_then(|r| r.releases.iter().find(|r| !r.draft).cloned()));
    w::card(ui, |ui| {
        match &release {
            Some(release) => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
                    anvil_ui::icons::paint(ui.painter(), rect, Icon::Package, p.weak);
                    let title = if release.name.is_empty() || release.name == release.tag {
                        release.tag.clone()
                    } else {
                        format!("{} · {}", release.tag, release.name)
                    };
                    ui.label(RichText::new(title).font(semibold(14.5)).color(p.text));
                    if release.prerelease {
                        w::badge(ui, t("предварительный"), Tone::Warning);
                    }
                    ui.label(RichText::new(i18n::date(release.published)).size(13.0).color(p.weak));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::button(ui, Kind::Ghost, Some(Icon::ArrowRight), t("Заметки выпуска")).clicked()
                        {
                            actions.push(Action::Url(release.url.clone()));
                        }
                    });
                });
                if let Some(from) = remote.and_then(|r| r.releases_from.as_deref()) {
                    w::note(ui, format!("{} {from}", t("Выпуски с GitHub берутся из")));
                }
            }
            None => {
                w::note(ui, t("Выпусков на GitHub нет."));
            }
        }
        let Some(git) = git(app, item) else { return };
        ui.add_space(12.0);
        let (title, shown) = match &git.last_tag {
            Some(tag) => (
                format!(
                    "{} {tag} · {}",
                    t("В коде после"),
                    i18n::count(git.since_tag as usize, ["коммит", "коммита", "коммитов"], ["commit", "commits"])
                ),
                git.since_tag as usize,
            ),
            None => (t("Последние коммиты").to_owned(), git.commits.len()),
        };
        w::section_label(ui, &title);
        ui.add_space(4.0);
        if shown == 0 {
            w::note(ui, t("После выпуска в коде ничего не менялось."));
            return;
        }
        let limit = shown.min(5).min(git.commits.len());
        for commit in git.commits.iter().take(limit) {
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_sized(
                    Vec2::new(70.0, 20.0),
                    egui::Label::new(
                        RichText::new(&commit.hash).font(egui::FontId::monospace(12.5)).color(p.accent_text),
                    ),
                );
                ui.add(egui::Label::new(RichText::new(&commit.subject).size(14.0).color(p.text)).truncate());
            });
        }
        if shown > limit && link(ui, &i18n::more_in_forge(shown - limit)).clicked() {
            actions.push(Action::Forge(Box::new(item.clone())));
        }
    });
}

// ─── Серверы и боты (§7.3) ──────────────────────────────────────────────────

fn services(
    app: &mut App,
    ui: &mut Ui,
    frame: &Frame,
    item: &Item,
    look: &Look,
    origin: f32,
    actions: &mut Vec<Action>,
) {
    let p = Palette::of(ui);
    ui.label(RichText::new(t("Серверы и боты")).font(semibold(24.0)).color(p.text));
    w::note(
        ui,
        t(
            "Локальные запускает Anvil и пишет их вывод в файл. Удалёнными управляет amber-admin — здесь только его сводка.",
        ),
    );
    ui.add_space(18.0);
    let remote = super::deck::remote_card_data(app);
    let left = |ui: &mut Ui, actions: &mut Vec<Action>| {
        w::section_label(ui, t("На этом компьютере"));
        ui.add_space(8.0);
        w::card_frame(ui).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let rows: Vec<usize> =
                (0..frame.items.len()).filter(|&i| frame.items[i].group == Group::Services).collect();
            for (n, &i) in rows.iter().enumerate() {
                service_row(ui, &frame.items[i], &frame.looks[i], frame.items[i].key == item.key, actions);
                if n + 1 < rows.len() {
                    w::divider(ui);
                }
            }
        });
        if let Some((summary, dir)) = &remote {
            ui.add_space(18.0);
            w::section_label(ui, &format!("{} · amber-admin", t("Удалённые")));
            ui.add_space(8.0);
            if super::deck::remote_card(ui, summary) {
                actions.push(Action::AmberAdmin(dir.clone()));
            }
        }
    };
    // Правой колонке нужно ~800: заголовок с двумя кнопками и панель журнала.
    if ui.available_width() >= 1150.0 {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(340.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(340.0);
                left(ui, actions);
            });
            ui.add_space(14.0);
            let width = ui.available_width();
            ui.allocate_ui_with_layout(Vec2::new(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(width);
                service_detail(app, ui, item, look, origin, actions);
            });
        });
    } else {
        service_detail(app, ui, item, look, origin, actions);
        ui.add_space(18.0);
        left(ui, actions);
    }
}

/// Строка службы в левом списке: знак, имя, справа состояние.
fn service_row(ui: &mut Ui, item: &Item, look: &Look, selected: bool, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let label = format!("{}, {}", item.name, look.state);
    let r = w::list_row(ui, egui::Id::new(("service-row", &item.key)), selected, 44.0, &label);
    if r.clicked() && !selected {
        actions.push(Action::Page(item.key.clone()));
    }
    let rect = r.rect;
    let mark = egui::Rect::from_min_size(egui::pos2(rect.left() + 12.0, rect.center().y - 14.0), Vec2::splat(28.0));
    w::paint_mark(ui, mark, item.mark.accent, item.mark.icon);
    let font = if selected { semibold(14.0) } else { egui::FontId::proportional(14.0) };
    let state_w = 170.0;
    let name_w = (rect.width() - 52.0 - state_w - 16.0).max(40.0);
    let name = one_line(ui, &item.name, font, p.text, name_w);
    ui.painter().galley(egui::pos2(rect.left() + 52.0, rect.center().y - name.size().y / 2.0), name, p.text);
    let x = rect.right() - state_w - 8.0;
    let lit = look.running.is_some();
    if lit {
        ui.painter().circle_filled(egui::pos2(x + 5.0, rect.center().y), 4.0, p.success);
    } else {
        ui.painter().circle_stroke(egui::pos2(x + 5.0, rect.center().y), 3.5, egui::Stroke::new(1.5, p.weak));
    }
    let state = if lit {
        look.running.and_then(|(_, s)| s).map_or_else(
            || t("работает").to_owned(),
            |s| format!("{} · {}", t("работает"), i18n::uptime(i18n::now() - s)),
        )
    } else {
        look.state.clone()
    };
    let color = if lit { p.text } else { p.weak };
    let galley = one_line(ui, &state, egui::FontId::proportional(13.0), color, state_w - 16.0);
    ui.painter().galley(egui::pos2(x + 16.0, rect.center().y - galley.size().y / 2.0), galley, p.text);
}

/// Текст в одну строку; не влез — многоточие.
fn one_line(ui: &Ui, text: &str, font: egui::FontId, color: egui::Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    ui.painter().layout_job(job)
}

fn service_detail(app: &mut App, ui: &mut Ui, item: &Item, look: &Look, origin: f32, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let bin = item.bin.clone().unwrap_or_default();
    let run = app.runs.iter().rev().find(|r| r.key == item.key && r.running()).cloned();
    let last = app.runs.iter().rev().find(|r| r.key == item.key).cloned();
    let presets = app.config.project(&item.project).presets;
    let installed = app.installs.get(&bin).is_some_and(Option::is_some);
    let profiles = crate::deck::profiles(item, &presets, installed);
    // Профиль того, что работает; в покое — выбранный.
    let profile = match &run {
        Some(run) => profiles.iter().find(|p| p.name == run.profile).cloned().unwrap_or_else(|| app.profile_of(item)),
        None => app.profile_of(item),
    };
    let ready = presets.iter().find(|p| p.bin == bin && p.name == profile.name).map(|p| p.ready.trim().to_owned());

    // Заголовок службы: знак 40, имя, бейдж, адрес; справа — действия.
    ui.horizontal(|ui| {
        ui.set_min_height(40.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        w::item_mark(ui, item.mark.accent, item.mark.icon, 40.0);
        ui.add_space(4.0);
        ui.label(RichText::new(&item.name).font(semibold(17.0)).color(p.text));
        if look.running.is_some() {
            w::badge(ui, t("работает"), Tone::Success);
        } else if last.as_ref().is_some_and(|r| r.end == End::Crashed) {
            w::badge(ui, &i18n::crashed(""), Tone::Danger);
        }
        if let Some(port) = profile.port {
            w::mono(ui, &format!("127.0.0.1:{port}"), None);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let more = w::icon_button(ui, Icon::More, t("Ещё действия"));
            w::menu(&more, 300.0, |ui| {
                profiles_menu(app, ui, item, actions);
                w::menu_separator(ui);
                let pinned = app.config.deck.pinned.contains(&item.key);
                if w::menu_item(ui, Some(Icon::Pin), if pinned { t("Открепить") } else { t("Закрепить") }, None)
                    .clicked()
                {
                    actions.push(Action::Pin(item.key.clone()));
                }
                if w::menu_item(ui, Some(Icon::Folder), t("Папка проекта"), None).clicked() {
                    actions.push(Action::Folder(item.project.clone()));
                }
                w::menu_separator(ui);
                if w::menu_item_danger(ui, Some(Icon::Close), t("Убрать с Пульта")).clicked() {
                    actions.push(Action::Remove(item.key.clone()));
                }
            });
            if w::icon_button(ui, Icon::Hammer, &format!("{} · Shift+Enter", t("В Кузнице"))).clicked() {
                actions.push(Action::Forge(Box::new(item.clone())));
            }
            ui.add_space(4.0);
            if let Some((pid, _)) = look.running {
                let (text, action) = if look.own {
                    (t("Остановить"), Action::StopNow(item.name.clone(), pid))
                } else {
                    (t("Остановить…"), Action::Stop(item.name.clone(), pid, item.project.clone()))
                };
                if w::button(ui, Kind::Secondary, Some(Icon::Stop), text).clicked() {
                    actions.push(action);
                }
                ui.add_space(4.0);
            }
            primary_button(app, ui, item, look, actions);
        });
    });
    ui.add_space(14.0);

    // Поля: профиль, команда, сборка, готовность, данные, сколько работает.
    w::card(ui, |ui| {
        if run.is_none()
            && look.running.is_none()
            && let Some(last) = last.as_ref().filter(|r| r.end == End::Crashed)
        {
            let took = last.ended.unwrap_or(last.started) - last.started;
            let code = last.code.map(crate::runs::code_text).unwrap_or_default();
            let title =
                format!("{} {} · {} {code} · {}", item.name, t("завершился"), t("код"), i18n::after_launch(took));
            let log = log_file(last);
            w::banner(ui, Tone::Danger, &title, "", |ui| {
                if let Some(path) = log
                    && w::button(ui, Kind::Ghost, None, t("Журнал")).clicked()
                {
                    actions.push(Action::File(path));
                }
            });
            ui.add_space(10.0);
        }
        ui.spacing_mut().item_spacing.y = 6.0;
        w::field_row(ui, t("Профиль"), |ui| {
            ui.label(RichText::new(profile.label()).size(14.0).color(p.text));
        });
        let command =
            if profile.args.trim().is_empty() { bin.clone() } else { format!("{bin} {}", profile.args.trim()) };
        w::field_row(ui, t("Команда"), |ui| {
            mono_wrap(ui, &command, p.text, 40.0);
            if w::icon_button(ui, Icon::Copy, t("Копировать")).clicked() {
                actions.push(Action::Copy(command.clone()));
            }
        });
        w::field_row(ui, t("Сборка"), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            w::source_chip(ui, &look.chip.0, look.chip.1.as_deref());
            if let Some((_, build, at)) = build_label(app, item) {
                let when = if i18n::when(at) == i18n::clock(at) { i18n::at(at) } else { i18n::date(at) };
                let mut text = format!("{} {when}", t("собрана"));
                if let (Some(build), Some(git)) = (build, git(app, item))
                    && let Some(n) = commits_since(git, &build.commit).filter(|n| *n > 0)
                {
                    let commits = i18n::count(n, ["коммит", "коммита", "коммитов"], ["commit", "commits"]);
                    text = format!("{text} · {} {commits}", t("после неё"));
                }
                ui.label(RichText::new(text).size(13.0).color(p.weak));
            }
        });
        w::field_row(ui, t("Готов, когда"), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            match (profile.port, ready.as_deref()) {
                (Some(port), _) if run.is_some() || look.running.is_some() => {
                    if app.port_open(port) {
                        state_line(ui, Some(Tone::Success), &format!("{} {port} {}", t("порт"), t("слушает")));
                    } else {
                        state_line(ui, Some(Tone::Warning), &format!("{} {port} {}", t("порт"), t("пока не слушает")));
                    }
                }
                (Some(port), _) => state_line(ui, None, &format!("{} {port} {}", t("порт"), t("слушает"))),
                (None, Some("window")) => state_line(ui, None, t("появится окно")),
                (None, Some(text)) if text.ends_with('s') && text.trim_end_matches('s').parse::<i64>().is_ok() => {
                    let secs = text.trim_end_matches('s').parse::<i64>().unwrap_or(0);
                    state_line(ui, None, &i18n::after_launch(secs));
                }
                _ => state_line(ui, None, t("сразу после запуска")),
            }
        });
        if let Some(dir) = &profile.cwd {
            let shown = dir.display().to_string();
            w::field_row(ui, t("Данные"), |ui| {
                mono_wrap(ui, &shown, p.weak, 40.0);
                if w::icon_button(ui, Icon::Folder, t("Открыть папку")).clicked() {
                    actions.push(Action::Folder(PathBuf::from(crate::launch::expand_env(&shown))));
                }
            });
        }
        w::field_row(ui, t("Работает"), |ui| match look.running {
            Some((pid, Some(started))) => {
                let text = format!("PID {pid} · {} · {}", i18n::since(started), i18n::uptime(i18n::now() - started));
                ui.label(RichText::new(text).size(13.0).color(p.text));
            }
            Some((pid, None)) => {
                ui.label(RichText::new(format!("PID {pid}")).size(13.0).color(p.text));
            }
            None => {
                ui.label(RichText::new(t("не запущен")).size(13.0).color(p.weak));
            }
        });
    });
    ui.add_space(14.0);
    let log = run
        .as_ref()
        .and_then(|r| r.log.clone())
        .or_else(|| app.runs.iter().rev().filter(|r| r.key == item.key).find_map(|r| r.log.clone()));
    log_card(app, ui, item, log, origin, actions);
}

/// Журнал службы: поиск, «только ошибки», копирование, строки по §5.9.
fn log_card(app: &mut App, ui: &mut Ui, item: &Item, log: Option<PathBuf>, origin: f32, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let lines = log.as_deref().map(|path| app.log_lines(path)).unwrap_or_default();
    let mut find = std::mem::take(&mut app.deck_view.log_find);
    let mut errors = app.deck_view.log_errors;
    let needle = find.trim().to_lowercase();
    let shown: Vec<&String> = lines
        .iter()
        .filter(|l| {
            !errors || matches!(crate::runs::log_level(l), crate::runs::LogLevel::Error | crate::runs::LogLevel::Warn)
        })
        .filter(|l| needle.is_empty() || l.to_lowercase().contains(&needle))
        .collect();
    w::card_frame(ui).inner_margin(egui::Margin::ZERO).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
                anvil_ui::icons::paint(ui.painter(), rect, Icon::Terminal, p.weak);
                ui.label(RichText::new(t("Журнал")).font(semibold(14.5)).color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if let Some(path) = &log {
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let open = format!("{} {name}", t("Открыть"));
                        if w::button(ui, Kind::Ghost, Some(Icon::File), &open)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            actions.push(Action::File(path.clone()));
                        }
                    }
                    if w::button(ui, Kind::Ghost, Some(Icon::Copy), t("Копировать")).clicked() {
                        actions.push(Action::Copy(shown.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n")));
                    }
                    w::toggle(ui, &mut errors, t("Только ошибки и предупреждения"));
                    // Узкое окно — поле короче, чтобы не налезать на заголовок.
                    let width = (ui.available_width() - 110.0).clamp(120.0, 240.0);
                    let id = egui::Id::new(("log-find", &item.key));
                    w::search_field_with_id(ui, id, &mut find, t("Найти в журнале"), None, width);
                });
            });
        });
        w::divider(ui);
        // Журнал тянется до низа окна (под ним подпись и край карточки), но не короче 260. Прокрутка
        // страницы высоту не меняет: иначе, прокручивая вниз, журнал бы рос без конца.
        let scrolled = (ui.clip_rect().top() - origin).max(0.0);
        let height = (ui.clip_rect().bottom() - ui.cursor().top() - scrolled - 110.0).max(260.0);
        egui::Frame::new().fill(p.field).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Строки вплотную: `show_rows` считает высоту строки вместе с этим отступом.
            ui.spacing_mut().item_spacing.y = 0.0;
            if log.is_none() {
                ui.set_min_height(height);
                w::note(ui, t("Журнала ещё нет: его пишет запуск из Anvil"));
                return;
            }
            if shown.is_empty() {
                ui.set_min_height(height);
                let text = if lines.is_empty() {
                    t("Вывода пока нет.")
                } else {
                    t("Под фильтр ничего не попало.")
                };
                w::note(ui, text);
                return;
            }
            egui::ScrollArea::both()
                .id_salt(("service-log", &item.key))
                .auto_shrink([false, false])
                .max_height(height)
                .min_scrolled_height(height)
                .stick_to_bottom(true)
                .show_rows(ui, 18.0, shown.len(), |ui, rows| {
                    for line in &shown[rows] {
                        let (job, place) = log_line(&p, line);
                        let galley = ui.painter().layout_job(job);
                        let width = galley.size().x.max(ui.available_width());
                        let sense = if place.is_some() { egui::Sense::click() } else { egui::Sense::hover() };
                        let (rect, r) = ui.allocate_exact_size(Vec2::new(width, 18.0), sense);
                        let y = rect.center().y - galley.size().y / 2.0;
                        ui.painter().galley(egui::pos2(rect.left(), y), galley, p.text);
                        // Место в коде — щелчок открывает его в VS Code (§5.9).
                        if let Some((file, row, col)) = place {
                            let r =
                                r.on_hover_text(t("Открыть в VS Code")).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if r.clicked() {
                                actions.push(Action::CodeAt(item.project.join(file), row, col, item.project.clone()));
                            }
                        }
                    }
                });
        });
        w::divider(ui);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 7)).show(ui, |ui| {
            let place = log.as_deref().map(short_path).unwrap_or_default();
            let text = if place.is_empty() {
                t("Вывод служб пишется в файл — служба переживёт закрытие Anvil.").to_owned()
            } else {
                format!(
                    "{} {place} ({}) — {}",
                    t("Вывод пишется в"),
                    t("2 файла по 5 МБ"),
                    t("служба переживёт закрытие Anvil.")
                )
            };
            ui.add(egui::Label::new(RichText::new(text).size(12.0).color(p.weak)).truncate());
        });
    });
    app.deck_view.log_find = find;
    app.deck_view.log_errors = errors;
}

/// `…\run\amber-server-test\out.log` — хвост пути, по которому видно, чей это журнал.
fn short_path(path: &Path) -> String {
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = path.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let run =
        path.parent().and_then(Path::parent).and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned());
    match run {
        Some(run) => format!("…{sep}{run}{sep}{dir}{sep}{file}", sep = std::path::MAIN_SEPARATOR),
        None => path.display().to_string(),
    }
}

/// Строка журнала: время приглушённо, место в коде — цветом акцента, остальное — цветом уровня.
/// Второе — место в коде (файл, строка, столбец), если оно есть.
fn log_line(p: &Palette, line: &str) -> (egui::text::LayoutJob, Option<(PathBuf, u32, u32)>) {
    use crate::runs::LogLevel;
    let color = match crate::runs::log_level(line) {
        LogLevel::Error => p.danger,
        LogLevel::Warn => p.warning,
        LogLevel::Debug => p.weak,
        LogLevel::Info | LogLevel::Plain => p.text,
    };
    let font = egui::FontId::monospace(12.5);
    let format = |color| egui::TextFormat::simple(font.clone(), color);
    let mut job = egui::text::LayoutJob::default();
    let end = line.find(char::is_whitespace).unwrap_or(0);
    let head = &line[..end];
    let time = end > 0 && head.contains(':') && head.starts_with(|c: char| c.is_ascii_digit());
    let start = if time {
        job.append(head, 0.0, format(p.weak));
        end
    } else {
        0
    };
    let rest = &line[start..];
    match code_place(rest) {
        Some((range, file, row, col)) => {
            job.append(&rest[..range.start], 0.0, format(color));
            job.append(&rest[range.clone()], 0.0, format(p.accent_text));
            job.append(&rest[range.end..], 0.0, format(color));
            (job, Some((file, row, col)))
        }
        None => {
            job.append(rest, 0.0, format(color));
            (job, None)
        }
    }
}

/// Место в коде в строке журнала: `src/main.rs:31:13` (и `.gd`, `.cs`). Границы — по байтам строки.
fn code_place(line: &str) -> Option<(std::ops::Range<usize>, PathBuf, u32, u32)> {
    for ext in [".rs:", ".gd:", ".cs:"] {
        let Some(at) = line.find(ext) else { continue };
        let stop = |c: char| {
            c.is_whitespace() || matches!(c, '\'' | '"' | '`' | '(' | ')' | '<' | '>' | ',' | '«' | '»' | '[' | ']')
        };
        let begin = line[..at].rfind(stop).map_or(0, |i| i + line[i..].chars().next().map_or(1, char::len_utf8));
        let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
        let after = at + ext.len();
        let row_len = digits(&line[after..]);
        if row_len == 0 || begin >= at {
            continue;
        }
        let row: u32 = line[after..after + row_len].parse().ok()?;
        let mut end = after + row_len;
        let mut col = 1;
        if line[end..].starts_with(':') {
            let n = digits(&line[end + 1..]);
            if n > 0 {
                col = line[end + 1..end + 1 + n].parse().unwrap_or(1);
                end += 1 + n;
            }
        }
        let file = PathBuf::from(&line[begin..at + ext.len() - 1]);
        return Some((begin..end, file, row, col));
    }
    None
}

// ─── Godot (§7.4) ───────────────────────────────────────────────────────────

fn godot(app: &mut App, ui: &mut Ui, item: &Item, look: &Look, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let editor = app.godot_editor();
    let Some(info) = engine(app, item).cloned() else { return };
    let editor_version = editor.as_deref().and_then(crate::engines::godot_editor_version);
    let need = info.version.clone().unwrap_or_default();
    // Шаблоны — по имени редактора (stable/beta, .NET); не разобрать имя — не судим.
    let templates = editor.as_deref().and_then(|exe| {
        let name = crate::engines::godot_templates_name(exe)?;
        let present = crate::engines::godot_templates(exe, &name);
        Some((editor_version.clone().unwrap_or_else(|| name.clone()), present))
    });
    let git = git(app, item).cloned();
    // Коммитов после экспорта — по времени коммитов.
    let after = match (&git, info.exported_at) {
        (Some(git), Some(at)) => Some(git.commits.iter().filter(|c| c.time > at).count()),
        _ => None,
    };
    let exported = info.export.as_ref().is_some_and(|e| e.is_file());
    let relative = info.export.as_ref().map(|e| {
        e.strip_prefix(&item.project).map(Path::to_path_buf).unwrap_or_else(|_| e.clone()).display().to_string()
    });

    let mut badges = vec![(format!("Godot {need}").trim().to_owned(), Tone::Neutral)];
    match after {
        _ if !exported => badges.push((t("без сборки").to_owned(), Tone::Neutral)),
        Some(0) => badges.push((t("сборка свежая").to_owned(), Tone::Success)),
        Some(n) => badges.push((
            format!(
                "{} {}",
                t("сборка старше кода:"),
                i18n::count(n, ["коммит", "коммита", "коммитов"], ["commit", "commits"])
            ),
            Tone::Warning,
        )),
        None => {}
    }
    if look.running.is_some() {
        badges.push((t("работает").to_owned(), Tone::Success));
    }
    header(ui, item, &badges, &[Sub::Mono(item.project.display().to_string())], |ui| {
        common_buttons(app, ui, item, look, actions);
        let found = editor.is_some();
        let open = ui
            .add_enabled_ui(found, |ui| w::button(ui, Kind::Secondary, Some(Icon::Pencil), t("Открыть в Godot")))
            .inner
            .on_disabled_hover_text(t("Godot не найден — укажите путь в карточке «Движок»"));
        if open.clicked() {
            actions.push(Action::Editor(Box::new(item.clone())));
        }
        ui.add_space(4.0);
        let primary = primary(app, item, look);
        let enabled = primary.action.is_some() && !app.launching(&item.key);
        let split = ui
            .add_enabled_ui(enabled, |ui| {
                w::split_button(ui, Kind::Primary, Some(primary.icon), &primary.text, t("Как играть"))
            })
            .inner;
        let main = if primary.hint.is_empty() { split.main } else { split.main.on_hover_text(&primary.hint) };
        if main.clicked()
            && enabled
            && let Some(action) = primary.action
        {
            actions.push(action);
        }
        w::menu(&split.menu, 300.0, |ui| {
            let game = format!("{} · {}", t("Игра"), relative.clone().unwrap_or_default());
            let first = if after.is_some_and(|n| n > 0) { 1 } else { 0 };
            // Код новее exe — первым пунктом «Экспортировать и играть».
            for n in 0..3 {
                match (n + 3 - first) % 3 {
                    0 => {
                        let r =
                            ui.add_enabled_ui(exported, |ui| w::menu_item(ui, Some(Icon::Check), &game, None)).inner;
                        if r.clicked() {
                            actions.push(Action::Play(Box::new(item.clone())));
                        }
                    }
                    1 => {
                        let r = ui
                            .add_enabled_ui(found, |ui| w::menu_item(ui, Some(Icon::Play), t("Из исходников"), None))
                            .inner;
                        if r.clicked() {
                            actions.push(Action::FromSource(Box::new(item.clone())));
                        }
                    }
                    _ => {
                        let r = ui
                            .add_enabled_ui(found && info.export.is_some() && look.running.is_none(), |ui| {
                                w::menu_item(ui, Some(Icon::Refresh), t("Экспортировать и играть"), Some("Ctrl+Enter"))
                            })
                            .inner;
                        if r.clicked() {
                            actions.push(Action::FromCode(Box::new(item.clone())));
                        }
                    }
                }
            }
        });
    });

    // Не больше одного баннера: сначала — нет Godot, потом — нет шаблонов экспорта.
    if editor.is_none() {
        let title = format!("Godot {need} {}", t("не найден"));
        // «Играть можно» — только если есть что: собранной игре движок не нужен.
        let text = if exported {
            t("Играть можно: собранной игре движок не нужен.")
        } else {
            ""
        };
        w::banner(ui, Tone::Warning, title.trim(), text, |ui| {
            if w::button(ui, Kind::Secondary, Some(Icon::Folder), t("Указать путь…")).clicked() {
                actions.push(Action::GodotPath);
            }
        });
        ui.add_space(14.0);
    } else if let Some((version, false)) = &templates
        && info.export.is_some()
    {
        let title = format!("{} {version}", t("Экспорт невозможен: нет шаблонов"));
        w::banner(ui, Tone::Warning, &title, "", |ui| {
            if w::button(ui, Kind::Ghost, None, t("Где взять")).clicked() {
                actions.push(Action::Url("https://godotengine.org/download/archive/".into()));
            }
        });
        ui.add_space(14.0);
    }

    let game_running = look.running.is_some();
    let source_running = app.runs.iter().any(|r| r.key == item.key && r.running() && r.source.starts_with("исходники"));
    cards(ui, egui::Id::new(("page-cards", &item.key)), &[1.0, 1.0, 1.0], |i, ui| match i {
        0 => {
            w::card_title(ui, Icon::Package, t("Сборка"));
            ui.spacing_mut().item_spacing.y = 6.0;
            match (&relative, exported) {
                (Some(file), true) => {
                    w::field_row(ui, t("Файл"), |ui| {
                        w::mono(ui, file, Some(p.text));
                        if let Some(size) = info.size {
                            ui.label(RichText::new(megabytes(size)).size(13.0).color(p.weak));
                        }
                    });
                    if let Some(at) = info.exported_at {
                        w::field_row(ui, t("Собрана"), |ui| {
                            let text = i18n::date_at(at);
                            ui.label(RichText::new(text).size(13.0).color(p.text));
                        });
                    }
                    w::field_row(ui, t("Свежесть"), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        match after {
                            Some(0) => state_line(ui, Some(Tone::Success), t("после экспорта коммитов не было")),
                            Some(n) => {
                                let commits = i18n::count(n, ["коммит", "коммита", "коммитов"], ["commit", "commits"]);
                                state_line(ui, Some(Tone::Warning), &format!("{} {commits}", t("после экспорта —")));
                            }
                            None => state_line(ui, None, t("без git — не узнать")),
                        }
                    });
                }
                (Some(file), false) => {
                    w::field_row(ui, t("Файл"), |ui| {
                        w::mono(ui, file, None);
                    });
                    w::note(ui, t("Игру ещё не экспортировали."));
                }
                (None, _) => {
                    w::note(ui, t("В export_presets.cfg нет пресета «Windows Desktop»"));
                }
            }
            if let Some(file) = &relative {
                ui.add_space(10.0);
                let text = if exported {
                    t("Экспортировать заново")
                } else {
                    t("Экспортировать")
                };
                let why = if editor.is_none() {
                    t("Godot не найден")
                } else {
                    t("Игра запущена — закройте её, чтобы экспортировать")
                };
                let can = editor.is_some() && !game_running;
                let r = ui
                    .add_enabled_ui(can, |ui| w::button(ui, Kind::Secondary, Some(Icon::Refresh), text))
                    .inner
                    .on_disabled_hover_text(why);
                if r.clicked() {
                    actions.push(Action::Export(Box::new(item.clone())));
                }
                ui.add_space(6.0);
                let command =
                    format!("godot --headless --export-release \"Windows Desktop\" {}", file.replace('\\', "/"));
                mono_wrap(ui, &command, p.weak, 0.0);
            }
        }
        1 => {
            w::card_title(ui, Icon::Play, t("Профили запуска"));
            let chip_export = info.exported_at.map(i18n::date);
            engine_profile(
                ui,
                t("Игра"),
                relative.as_deref().unwrap_or("—"),
                true,
                (t("экспорт"), chip_export.as_deref()),
                game_running,
                exported,
                t("Запустить игру"),
                || actions.push(Action::Play(Box::new(item.clone()))),
            );
            w::divider(ui);
            let source = format!("godot --path {}", item.project.display());
            let chip = editor_version.clone().map(|v| format!("Godot {v}"));
            engine_profile(
                ui,
                t("Из исходников"),
                &source,
                false,
                (chip.as_deref().unwrap_or("Godot"), None),
                source_running,
                editor.is_some(),
                t("Играть из исходников"),
                || actions.push(Action::FromSource(Box::new(item.clone()))),
            );
            ui.add_space(8.0);
            w::note(ui, t("Редактор открывается кнопкой «Открыть в Godot»."));
        }
        _ => {
            w::card_title(ui, Icon::Gear, t("Движок"));
            ui.spacing_mut().item_spacing.y = 6.0;
            w::field_row(ui, "Godot", |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                match &editor_version {
                    Some(v) => {
                        w::mono(ui, v, Some(p.text));
                        state_line(ui, Some(Tone::Success), t("найден"));
                    }
                    None if editor.is_some() => state_line(ui, Some(Tone::Success), t("найден")),
                    None => state_line(ui, Some(Tone::Warning), t("не найден")),
                }
            });
            if let Some(path) = &editor {
                w::field_row(ui, t("Путь"), |ui| mono_wrap(ui, &path.display().to_string(), p.weak, 0.0));
            }
            if let Some((version, present)) = &templates {
                w::field_row(ui, t("Шаблоны"), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if *present {
                        state_line(ui, Some(Tone::Success), &format!("{} {version}", t("есть, версия")));
                    } else {
                        state_line(ui, Some(Tone::Warning), &format!("{} {version}", t("нет, нужна версия")));
                    }
                });
            }
            w::field_row(ui, t("Рендер"), |ui| {
                ui.label(RichText::new(info.render.clone().unwrap_or_else(|| "—".into())).size(13.0).color(p.text));
            });
            ui.add_space(10.0);
            if w::button(ui, Kind::Secondary, Some(Icon::Folder), t("Путь к Godot…")).clicked() {
                actions.push(Action::GodotPath);
            }
            ui.add_space(6.0);
            w::note(ui, t("Экспорт и тесты — в Кузнице."));
        }
    });

    engine_tabs(app, ui, item, info.export.as_ref().and_then(|e| e.parent().map(Path::to_path_buf)), actions);
}

/// Строка профиля игры: радио, имя, моно; кнопка «Играть»; чип и состояние.
#[allow(clippy::too_many_arguments)]
fn engine_profile(
    ui: &mut Ui,
    name: &str,
    what: &str,
    selected: bool,
    chip: (&str, Option<&str>),
    running: bool,
    can: bool,
    hint: &str,
    mut play: impl FnMut(),
) {
    let p = Palette::of(ui);
    ui.push_id(name, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            radio(ui, selected, name);
            let font = if selected { semibold(14.0) } else { egui::FontId::proportional(14.0) };
            ui.label(RichText::new(name).font(font).color(p.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled_ui(can && !running, |ui| w::icon_button(ui, Icon::Play, hint)).inner.clicked() {
                    play();
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(what).font(egui::FontId::monospace(12.5)).color(p.weak))
                            .truncate(),
                    );
                });
            });
        });
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_space(24.0);
            w::source_chip(ui, chip.0, chip.1);
            ui.add_space(4.0);
            if running {
                state_line(ui, Some(Tone::Success), t("работает"));
            } else {
                state_line(ui, None, t("не запущена"));
            }
        });
    });
}

/// Вкладки Godot и Unity: коммиты, запуски, папки.
fn engine_tabs(app: &mut App, ui: &mut Ui, item: &Item, build: Option<PathBuf>, actions: &mut Vec<Action>) {
    let count = app.runs.iter().filter(|r| r.key == item.key).count();
    let runs_tab = format!("{} · {count}", t("Запуски"));
    let mut tab = app.deck_view.page_tab.min(2);
    w::tabs(ui, &mut tab, &[t("Коммиты"), &runs_tab, t("Папки")]);
    app.deck_view.page_tab = tab;
    ui.add_space(12.0);
    match tab {
        0 => commits(app, ui, item, 8, actions),
        1 => history(app, ui, item, actions),
        _ => {
            let mut rows = vec![(t("Проект"), item.project.clone())];
            if let Some(dir) = build.filter(|d| d.is_dir()) {
                rows.push((t("Сборка"), dir));
            }
            folders(ui, &rows, actions);
        }
    }
}

fn megabytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 10.0 { format!("{mb:.0} {}", t("МБ")) } else { format!("{mb:.1} {}", t("МБ")) }
}

// ─── Unity ──────────────────────────────────────────────────────────────────

fn unity(app: &mut App, ui: &mut Ui, item: &Item, look: &Look, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let Some(info) = engine(app, item).cloned() else { return };
    let version = info.version.clone().unwrap_or_default();
    let mut badges = vec![(format!("Unity {version}").trim().to_owned(), Tone::Neutral)];
    if info.open {
        badges.push((t("открыт в Unity").to_owned(), Tone::Success));
    }
    if item.no_git {
        badges.push((t("без git").to_owned(), Tone::Neutral));
    } else if let Some(git) = git(app, item).filter(|g| g.dirty()) {
        let files = i18n::count(git.changes.len(), ["файл", "файла", "файлов"], ["file", "files"]);
        badges.push((files, Tone::Warning));
    }
    header(ui, item, &badges, &[Sub::Mono(item.project.display().to_string())], |ui| {
        common_buttons(app, ui, item, look, actions);
        primary_button(app, ui, item, look, actions);
    });
    if info.editor.is_none() {
        let title = format!("Unity {version} {}", t("не найден в Unity Hub"));
        w::banner(ui, Tone::Warning, title.trim(), t("Поставьте эту версию редактора через Unity Hub."), |_| {});
        ui.add_space(14.0);
    }
    w::card(ui, |ui| {
        w::card_title(ui, Icon::Gear, t("Движок"));
        ui.spacing_mut().item_spacing.y = 6.0;
        w::field_row(ui, "Unity", |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            w::mono(ui, &version, Some(p.text));
            if info.editor.is_some() {
                state_line(ui, Some(Tone::Success), t("найден"));
            } else {
                state_line(ui, Some(Tone::Warning), t("не найден в Unity Hub"));
            }
        });
        if let Some(path) = &info.editor {
            w::field_row(ui, t("Путь"), |ui| mono_wrap(ui, &path.display().to_string(), p.weak, 0.0));
        }
        w::field_row(ui, t("Проект"), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if info.open {
                state_line(ui, Some(Tone::Success), t("открыт в Unity"));
            } else {
                state_line(ui, None, t("не открыт"));
            }
        });
        ui.add_space(6.0);
        w::note(ui, t("Проект открывается в редакторе; собранную игру Anvil пока не запускает."));
    });
    ui.add_space(18.0);
    engine_tabs(app, ui, item, None, actions);
}

// ─── Действия ───────────────────────────────────────────────────────────────

fn apply(app: &mut App, ctx: &egui::Context, actions: Vec<Action>) {
    app.deck_view.scroll = false;
    for action in actions {
        match action {
            Action::Back => {
                app.deck_view.page = None;
                // Назад — к той же строке Пульта.
                app.deck_view.scroll = true;
            }
            Action::Page(key) => {
                app.open_page(key);
                app.deck_view.scroll = true;
            }
            Action::Launch(item, name) => super::deck::launch_profile(app, &item, &name),
            Action::Choose(item, name) => {
                if super::deck::choose_profile(app, &item, &name) {
                    app.save();
                }
            }
            Action::Main(item) => super::deck::run_main(app, &item),
            Action::Play(item) => app.launch_item(&item, false),
            Action::Rebuild(item) => app.rebuild_restart(&item),
            Action::Install(item) => app.install_and_launch(&item),
            Action::FromCode(item) => app.launch_item(&item, true),
            Action::FromSource(item) => {
                app.play_from_source(&item);
            }
            Action::Export(item) => {
                app.export_game(&item, false);
            }
            Action::Editor(item) => app.open_editor(&item),
            Action::Focus(name, pid) => app.focus(&name, pid),
            Action::Stop(name, pid, dir) => app.stop_confirm = Some((name, pid, dir)),
            Action::StopNow(name, pid) => app.stop_run(name, pid),
            Action::Forge(item) => {
                app.deck_view.selected = Some(item.key.clone());
                app.set_mode(Mode::Forge);
                app.view = View::Project;
            }
            Action::Pin(key) => super::deck::toggle_pin(app, key),
            Action::Remove(key) => {
                super::deck::remove(app, key);
                app.deck_view.page = None;
            }
            Action::Profiles(project) => app.presets_for = Some(project),
            Action::Folder(dir) => app.report(crate::open::folder(&dir)),
            Action::File(path) => app.report(crate::open::file(&path)),
            Action::Copy(text) => {
                ctx.copy_text(text);
                app.toasts.push(t("Скопировано"), Tone::Neutral);
            }
            Action::InstallCode(project, bin) => app.install_confirm = Some((project, bin)),
            Action::Rollback(project, bin, version) => app.rollback_confirm = Some((project, bin, version)),
            Action::Url(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            Action::GodotPath => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Godot", &["exe"]).pick_file() {
                    app.set_godot(path);
                }
            }
            Action::AmberAdmin(dir) => app.open_amber_admin(&dir),
            Action::CodeAt(file, line, col, dir) => app.report(crate::open::code_at(&file, line, col, &dir)),
        }
    }
    // Время работы идёт раз в минуту.
    ctx.request_repaint_after(std::time::Duration::from_secs(60));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_places_in_log_lines() {
        let line = r"thread 'main' (33656) panicked at src\main.rs:31:13:";
        let (range, file, row, col) = code_place(line).unwrap();
        assert_eq!(&line[range], r"src\main.rs:31:13");
        assert_eq!((file, row, col), (PathBuf::from(r"src\main.rs"), 31, 13));
        // Перед местом — русский текст: границы по байтам не рвут букву.
        let line = "ошибка в «scripts/player.gd:12»";
        let (range, file, row, col) = code_place(line).unwrap();
        assert_eq!(&line[range], "scripts/player.gd:12");
        assert_eq!((file, row, col), (PathBuf::from("scripts/player.gd"), 12, 1));
        assert!(code_place("12:00:01  INFO  tick 3").is_none());
        assert!(code_place("see main.rs: nothing").is_none());
    }
}
