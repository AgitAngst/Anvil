//! Палитра: найти и запустить, не снимая рук с клавиатуры. Один список в двух видах:
//! - `Ctrl+K` в окне — модальное окно 560 сверху: предметы Пульта с их профилями, действия Кузницы;
//! - быстрый запуск (§7.5, `Ctrl+Alt+Space` из любой программы) — окно без рамки 640 на месте
//!   спрятанного главного: только Пульт и его действия.
//!
//! Каждое слово запроса должно найтись в имени предмета, профиля, бинарника или действия; начало
//! слова — выше; набранное не в той раскладке тоже находится («фь еу» = «am te»). Пустой запрос —
//! «Запущено», «Закреплено» (Alt+1…9), «Недавнее».

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Mark, Palette, Tone, semibold};
use eframe::egui::{self, Align2, FontId, Key, Modifiers, Sense, Ui, Vec2};

use crate::app::{App, Mode, View};
use crate::deck::Group;
use crate::i18n::{self, t};
use crate::registry::Kind as ProjectKind;
use crate::runs::End;
use crate::tasks::{self, Task};

/// Окно быстрого запуска: ширина и высота (§7.5).
pub const QUICK_SIZE: Vec2 = Vec2::new(640.0, 460.0);

/// Открытая палитра: запрос и выбранная строка.
#[derive(Default)]
pub struct State {
    query: String,
    selected: usize,
    /// Выбор сдвинули клавишами — прокрутить к нему.
    scroll: bool,
    /// Вид быстрого запуска (окно без рамки), а не модальное окно в Anvil.
    quick: bool,
    /// Быстрый запуск: сборке мешает запущенный exe — строка спрашивает, отодвинуть ли его.
    asking: bool,
}

impl State {
    /// Быстрый запуск спрашивает, отодвинуть ли занятый exe.
    pub fn asking(&self) -> bool {
        self.asking
    }
}

/// Что делает строка.
#[derive(Clone)]
enum Command {
    /// Запустить предмет: с этим профилем (он же станет выбранным) или (`None`) главное действие —
    /// то же, что Enter на Пульте: к окну, поставить, собрать.
    Launch(Box<crate::deck::Item>, Option<String>),
    /// Собрать из кода и запустить (Ctrl+Enter): профиль станет выбранным.
    FromCode(Box<crate::deck::Item>, Option<String>),
    Page(String),
    Forge(Box<crate::deck::Item>),
    InstallCode(PathBuf, String),
    Mode(Mode),
    Select(PathBuf),
    /// Задача с явным профилем (`Some(true)` — release) или с тем, что выбран у проекта.
    Task(PathBuf, Task, Option<bool>),
    Folder(PathBuf),
    Terminal(PathBuf),
    Code(PathBuf),
    CheckDeps(PathBuf),
    Release(PathBuf),
    AmberAdmin(PathBuf),
    Update(PathBuf, String),
    Refresh,
    CheckAllDeps,
    Settings,
    Toolchain,
    Overview,
    AddFolder,
    Log,
    About,
    CheckUpdates,
}

/// Разделы списка — в этом порядке.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Section {
    Running,
    Pinned,
    Recent,
    Launch,
    Actions,
    Other,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::Running => t("Запущено"),
            Section::Pinned => t("Закреплено"),
            Section::Recent => t("Недавнее"),
            Section::Launch => t("Запуск"),
            Section::Actions => t("Действия"),
            Section::Other => t("Кузница"),
        }
    }
}

/// Строка списка.
struct Row {
    icon: Icon,
    /// Знак предмета — у строк Пульта.
    mark: Option<Mark>,
    title: String,
    /// Второй текст: у предмета — моно (аргументы, источник), у действия — проект.
    detail: Option<String>,
    mono: bool,
    /// Точка (или кольцо) и слово справа: «работает · 2 ч 14 мин», «упал в 00:28».
    state: Option<(Option<Tone>, String)>,
    keys: Option<String>,
    /// Слова для поиска сверх видимых: бинарник, профиль, действие.
    extra: String,
    section: Section,
    running: bool,
    /// Номер закреплённого (Alt+1…9) — у строки, которую запускает Alt.
    pin: Option<usize>,
    /// Когда запускали с Пульта — для «Недавнего».
    launched: Option<i64>,
    crashed: bool,
    /// Предмет строки и профиль: для Ctrl+Enter, Alt+Enter, Shift+Enter.
    target: Option<(Box<crate::deck::Item>, Option<String>)>,
    /// Что сделает Enter — подпись у выбранной строки: «запустить», «к окну», «журнал».
    verb: String,
    /// Псевдоним закреплённого: набран точно — строка первая (§5.11).
    alias: Option<String>,
    command: Command,
}

impl Row {
    fn plain(icon: Icon, title: &str, detail: Option<String>, keys: Option<&str>, command: Command) -> Row {
        Row {
            icon,
            mark: None,
            title: title.to_owned(),
            detail,
            mono: false,
            state: None,
            keys: keys.map(str::to_owned),
            extra: String::new(),
            section: Section::Other,
            running: false,
            pin: None,
            launched: None,
            crashed: false,
            target: None,
            verb: String::new(),
            alias: None,
            command,
        }
    }
}

pub fn open(app: &mut App) {
    app.palette = Some(State::default());
}

/// Открыть с набранным: печать на Пульте продолжается в палитре.
pub fn open_with(app: &mut App, text: &str) {
    app.palette = Some(State { query: text.to_owned(), ..State::default() });
}

/// Быстрый запуск: окно без рамки на месте главного.
pub fn open_quick(app: &mut App) {
    app.palette = Some(State { quick: true, ..State::default() });
}

/// Поле-кнопка в шапке: выглядит как поиск, открывает палитру.
pub fn launcher(app: &mut App, ui: &mut Ui, width: f32) {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let border = if response.hovered() { p.border_strong } else { p.border };
    ui.painter().rect(rect, 6, p.field, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
    let icon = egui::Rect::from_min_size(egui::pos2(rect.left() + 9.0, rect.center().y - 8.0), Vec2::splat(16.0));
    anvil_ui::icons::paint(ui.painter(), icon, Icon::Search, p.faint);
    let hint = if app.mode == Mode::Deck {
        t("Найти или запустить…")
    } else {
        t("Найти проект или действие…")
    };
    ui.painter().text(
        egui::pos2(rect.left() + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        hint,
        FontId::proportional(14.0),
        p.faint,
    );
    let keys = egui::Rect::from_min_max(egui::pos2(rect.right() - 70.0, rect.top()), rect.max);
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(keys).layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.x = 3.0;
            w::kbd(ui, "K");
            w::kbd(ui, "Ctrl");
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, hint));
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        open(app);
    }
}

/// Клавиши списка — до поля ввода: однострочное поле само отвечает на стрелки и Enter.
struct Keys {
    down: bool,
    up: bool,
    enter: bool,
    from_code: bool,
    page: bool,
    forge: bool,
    pinned: Option<usize>,
    escape: bool,
}

fn keys(ctx: &egui::Context) -> Keys {
    // Повтор зажатой клавиши не запускает ещё раз: действует только само нажатие.
    let fresh = ctx.input(|i| {
        i.events.iter().any(|e| matches!(e, egui::Event::Key { key: Key::Enter, pressed: true, repeat: false, .. }))
    });
    const DIGITS: [Key; 9] =
        [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
    ctx.input_mut(|i| {
        // Сначала сочетания с модификаторами: Enter без них сравнивается «логически» и съел бы их.
        let from_code = i.consume_key(Modifiers::COMMAND, Key::Enter);
        let forge = i.consume_key(Modifiers::SHIFT, Key::Enter);
        let page = i.consume_key(Modifiers::ALT, Key::Enter);
        let enter = i.consume_key(Modifiers::NONE, Key::Enter);
        // Alt+цифра — только само нажатие: автоповтор зажатой клавиши не запускает ещё раз.
        let pinned = i.events.iter().find_map(|e| match e {
            egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. }
                if modifiers.alt && !modifiers.ctrl =>
            {
                DIGITS.iter().position(|d| d == key)
            }
            _ => None,
        });
        for key in DIGITS {
            while i.consume_key(Modifiers::ALT, key) {}
        }
        // Alt+1 печатает «1» в поле — убрать.
        if i.modifiers.alt {
            i.events.retain(|e| !matches!(e, egui::Event::Text(_)));
        }
        Keys {
            down: i.consume_key(Modifiers::NONE, Key::ArrowDown),
            up: i.consume_key(Modifiers::NONE, Key::ArrowUp),
            enter: fresh && enter,
            from_code: fresh && from_code,
            page: fresh && page,
            forge: fresh && forge,
            pinned,
            escape: i.consume_key(Modifiers::NONE, Key::Escape),
        }
    })
}

/// Что выбрали клавишами в этом кадре.
fn chosen_by_keys(keys: &Keys, rows: &[Row], found: &[(Section, usize)], selected: usize) -> Option<Command> {
    if let Some(n) = keys.pinned {
        return rows.iter().find(|r| r.pin == Some(n + 1)).map(|r| r.command.clone());
    }
    let row = &rows[found.get(selected)?.1];
    if keys.enter {
        return Some(row.command.clone());
    }
    let (item, profile) = row.target.clone()?;
    if keys.from_code && matches!(item.kind, ProjectKind::Rust | ProjectKind::Godot) && !item.is_self() {
        Some(Command::FromCode(item, profile))
    } else if keys.page {
        Some(Command::Page(item.key.clone()))
    } else if keys.forge {
        Some(Command::Forge(item))
    } else {
        None
    }
}

/// Палитра в окне (`Ctrl+K`).
pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut state) = app.palette.take() else { return };
    if state.quick {
        app.palette = Some(state);
        return;
    }
    let rows = rows(app, false);
    let found = arrange(&rows, &state.query, app.config.quick.layout, false);
    let keys = keys(ctx);
    move_selection(&mut state, &keys, found.len());
    let mut chosen = chosen_by_keys(&keys, &rows, &found, state.selected);

    let p = Palette::of_ctx(ctx);
    let id = egui::Id::new("anvil-palette");
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0, p.border_strong))
        .corner_radius(12)
        .shadow(ctx.global_style().visuals.window_shadow)
        .inner_margin(egui::Margin::same(8));
    let area = egui::Modal::default_area(id).anchor(Align2::CENTER_TOP, Vec2::new(0.0, 84.0));
    let before = state.query.clone();
    let response = egui::Modal::new(id)
        .area(area)
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(if p.dark { 120 } else { 70 }))
        .show(ctx, |ui| {
            ui.set_width(560.0);
            let field = egui::Id::new("anvil-palette-query");
            w::search_field_with_id(ui, field, &mut state.query, t("Найти или запустить…"), Some("Esc"), 560.0);
            ui.memory_mut(|m| m.request_focus(field));
            ui.add_space(6.0);
            if found.is_empty() {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| w::note(ui, t("Ничего не нашлось.")));
                ui.add_space(12.0);
            } else {
                egui::ScrollArea::vertical().max_height(420.0).auto_shrink([false, true]).show(ui, |ui| {
                    if let Some(command) = list(ui, &rows, &found, &mut state) {
                        chosen = Some(command);
                    }
                });
            }
            ui.add_space(4.0);
            hints(ui);
        });
    state.scroll = false;
    if state.query != before {
        state.selected = 0;
    }
    let close = response.should_close() || keys.escape;
    match chosen {
        Some(command) => run(app, ctx, command),
        None if !close => app.palette = Some(state),
        None => {}
    }
}

/// Быстрый запуск: весь кадр — окно без рамки (§7.5).
pub fn show_quick(app: &mut App, ui: &mut Ui) {
    let Some(mut state) = app.palette.take() else {
        // Палитру закрыли иначе — вернуть окно.
        app.end_quick(false);
        return;
    };
    let ctx = ui.ctx().clone();
    let rows = rows(app, true);
    let found = arrange(&rows, &state.query, app.config.quick.layout, true);
    let keys = keys(&ctx);
    let p = Palette::of(ui);

    // Сборке мешает запущенный exe — вопрос в строке: Enter — отодвинуть и собрать, Esc — отмена.
    if state.asking {
        if app.locked.is_none() {
            state.asking = false;
        } else if keys.enter {
            app.resolve_locked(Some(tasks::Resolve::MoveAside));
            state.asking = false;
            app.palette = Some(state);
            if app.config.quick.close_after {
                app.end_quick(false);
            }
            return;
        } else if keys.escape {
            app.resolve_locked(None);
            state.asking = false;
            app.palette = Some(state);
            return;
        }
    } else if keys.escape {
        app.palette = Some(state);
        app.end_quick(false);
        return;
    }
    move_selection(&mut state, &keys, found.len());
    let mut chosen = if state.asking { None } else { chosen_by_keys(&keys, &rows, &found, state.selected) };

    let rect = ui.max_rect();
    ui.painter().rect(rect, 0, p.card, egui::Stroke::new(1.0, p.border_strong), egui::StrokeKind::Inside);
    let before = state.query.clone();
    let inner = rect.shrink(1.0);
    let mut content =
        ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    content.spacing_mut().item_spacing.y = 0.0;
    // Поле 40 с отступом 8.
    egui::Frame::new().inner_margin(egui::Margin::same(8)).show(&mut content, |ui| {
        ui.set_width(ui.available_width());
        quick_field(ui, &mut state.query);
    });
    w::divider(&mut content);
    let footer = 31.0;
    let list_h = (content.available_height() - footer).max(60.0);
    content.allocate_ui_with_layout(Vec2::new(inner.width(), list_h), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_min_height(list_h);
        ui.set_width(inner.width());
        if state.asking {
            asking_row(app, ui);
        } else if found.is_empty() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| w::note(ui, t("Ничего не нашлось.")));
        } else {
            egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 0)).show(ui, |ui| {
                    if let Some(command) = list(ui, &rows, &found, &mut state) {
                        chosen = Some(command);
                    }
                });
            });
        }
    });
    // Подвал 30 на `surface` с линией сверху: подсказки клавиш (§5.10).
    let footer_rect = egui::Rect::from_min_max(egui::pos2(inner.left(), inner.bottom() - 30.0), inner.max);
    content.painter().rect_filled(footer_rect, 0, p.surface);
    content.painter().hline(footer_rect.x_range(), footer_rect.top(), egui::Stroke::new(1.0, p.border));
    let mut footer_ui = content.new_child(
        egui::UiBuilder::new()
            .max_rect(footer_rect.shrink2(Vec2::new(12.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    hints(&mut footer_ui);

    state.scroll = false;
    if state.query != before {
        state.selected = 0;
    }
    match chosen {
        Some(command) => {
            app.palette = Some(state);
            run(app, &ctx, command);
        }
        None => app.palette = Some(state),
    }
}

/// Поле быстрого запуска: 40, значок поиска, текст 17, справа [Esc] (§5.6).
fn quick_field(ui: &mut Ui, query: &mut String) {
    let p = Palette::of(ui);
    let id = egui::Id::new("anvil-quick-query");
    egui::Frame::new()
        .fill(p.field)
        .stroke(egui::Stroke::new(1.0, p.accent))
        .corner_radius(6)
        .inner_margin(egui::Margin { left: 12, right: 10, top: 0, bottom: 0 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(38.0);
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                anvil_ui::icons::paint(ui.painter(), icon, Icon::Search, p.weak);
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    w::kbd(ui, "Esc");
                    ui.add_space(8.0);
                    let edit = egui::TextEdit::singleline(query)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .font(FontId::proportional(17.0))
                        .text_color(p.text)
                        .hint_text(egui::RichText::new(t("Найти или запустить…")).color(p.faint))
                        .desired_width(ui.available_width());
                    ui.add(edit);
                });
            });
        });
    ui.memory_mut(|m| m.request_focus(id));
}

/// Вопрос быстрого запуска: сборке мешает запущенный exe.
fn asking_row(app: &App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let locked = app.locked.as_ref().and_then(|(_, locked, _, _)| locked.first());
    let bin = locked.and_then(|l| l.exe.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // Профиль той копии, что держит exe, — если её запускал Anvil.
    let profile = locked
        .and_then(|l| app.runs.iter().rev().find(|r| r.running() && l.pids.contains(&r.pid)))
        .map(|r| r.profile.clone())
        .unwrap_or_default();
    egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        w::banner(
            ui,
            Tone::Warning,
            &i18n::already_running(&bin, &profile),
            t("Enter — отодвинуть exe и собрать · Esc — отмена"),
            |_| {},
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(t("Запущенная программа доработает как есть, новая сборка ляжет рядом."))
                .size(13.0)
                .color(p.weak),
        );
    });
}

/// Подсказки клавиш (§5.10). [Esc] — уже в поле ввода: в подвале его нет, иначе ряд не влезает
/// в 560 палитры и 640 быстрого запуска.
fn hints(ui: &mut Ui) {
    let p = Palette::of(ui);
    let groups: [(&[&str], &str); 4] = [
        (&["Enter"], t("запустить")),
        (&["Ctrl", "Enter"], t("из кода")),
        (&["Alt", "Enter"], t("страница")),
        (&["Shift", "Enter"], t("в Кузнице")),
    ];
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, (keys, label)) in groups.iter().enumerate() {
            if i > 0 {
                ui.add_space(12.0);
            }
            for (k, key) in keys.iter().enumerate() {
                if k > 0 {
                    ui.add_space(3.0);
                }
                w::kbd(ui, key);
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new(*label).size(12.0).color(p.weak));
        }
    });
}

fn move_selection(state: &mut State, keys: &Keys, len: usize) {
    if len == 0 {
        state.selected = 0;
        return;
    }
    if keys.down {
        state.selected = (state.selected + 1) % len;
        state.scroll = true;
    }
    if keys.up {
        state.selected = (state.selected + len - 1) % len;
        state.scroll = true;
    }
    state.selected = state.selected.min(len - 1);
}

/// Строки по разделам; возвращает команду щелчка.
fn list(ui: &mut Ui, rows: &[Row], found: &[(Section, usize)], state: &mut State) -> Option<Command> {
    let mut chosen = None;
    ui.spacing_mut().item_spacing.y = 1.0;
    let mut section = None;
    for (n, &(sec, index)) in found.iter().enumerate() {
        if section != Some(sec) {
            section = Some(sec);
            ui.add_space(if n == 0 { 6.0 } else { 10.0 });
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                w::section_label(ui, sec.label());
            });
            ui.add_space(4.0);
        }
        let row = &rows[index];
        let selected = n == state.selected;
        let r = if row.mark.is_some() || row.section != Section::Other {
            draw_item(ui, row, selected, n)
        } else {
            draw_row(ui, row, selected)
        };
        if selected && state.scroll {
            r.scroll_to_me(None);
        }
        if r.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
            state.selected = n;
        }
        if r.clicked() {
            chosen = Some(row.command.clone());
        }
    }
    chosen
}

/// Текст в одну строку; не влез — многоточие.
fn one_line(ui: &Ui, text: &str, font: FontId, color: egui::Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    ui.painter().layout_job(job)
}

/// Строка предмета или действия Пульта: 44, знак 28, имя, моно, справа — состояние и клавиши.
fn draw_item(ui: &mut Ui, row: &Row, selected: bool, n: usize) -> egui::Response {
    let p = Palette::of(ui);
    let label = match (&row.detail, &row.state) {
        (Some(d), Some((_, s))) => format!("{}, {d}, {s}", row.title),
        (Some(d), None) => format!("{}, {d}", row.title),
        (None, Some((_, s))) => format!("{}, {s}", row.title),
        (None, None) => row.title.clone(),
    };
    let r = w::list_row(ui, egui::Id::new(("palette-row", n, &row.title)), selected, 44.0, &label);
    let rect = r.rect;
    let mark = egui::Rect::from_min_size(egui::pos2(rect.left() + 12.0, rect.center().y - 14.0), Vec2::splat(28.0));
    match row.mark {
        Some(m) => w::paint_mark(ui, mark, m.accent, m.icon),
        None => {
            // Действие: квадрат `raised` со значком.
            ui.painter().rect_filled(mark, 7, p.raised);
            let icon = egui::Rect::from_center_size(mark.center(), Vec2::splat(16.0));
            anvil_ui::icons::paint(ui.painter(), icon, row.icon, p.weak);
        }
    }
    // Справа: состояние, клавиши; у выбранной — [Enter] запустить.
    let mut right = rect.right() - 12.0;
    let painter = ui.painter();
    let mut keys: Vec<String> = Vec::new();
    if let Some(k) = &row.keys {
        keys.push(k.clone());
    }
    if selected && row.keys.is_none() && !row.verb.is_empty() {
        keys.push("Enter".to_owned());
    }
    for key in keys.iter().rev() {
        if selected && key == "Enter" {
            let g = painter.layout_no_wrap(row.verb.clone(), FontId::proportional(12.0), p.weak);
            right -= g.size().x;
            painter.galley(egui::pos2(right, rect.center().y - g.size().y / 2.0), g, p.weak);
            right -= 6.0;
        }
        for part in key.split('+').collect::<Vec<_>>().into_iter().rev() {
            let g = painter.layout_no_wrap(part.to_owned(), FontId::monospace(11.5), p.weak);
            let size = Vec2::new(g.size().x + 10.0, 19.0);
            let k = egui::Rect::from_min_size(egui::pos2(right - size.x, rect.center().y - size.y / 2.0), size);
            painter.rect(k, 4, p.raised, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
            painter.galley(k.center() - g.size() / 2.0, g, p.weak);
            right = k.left() - 3.0;
        }
        right -= 9.0;
    }
    if let Some((tone, text)) = &row.state {
        let g = one_line(ui, text, FontId::proportional(12.0), p.weak, 180.0);
        right -= g.size().x;
        ui.painter().galley(egui::pos2(right, rect.center().y - g.size().y / 2.0), g, p.weak);
        let dot = egui::pos2(right - 9.0, rect.center().y);
        match tone {
            Some(tone) => ui.painter().circle_filled(dot, 4.0, tone.color(&p)),
            None => ui.painter().circle_stroke(dot, 3.5, egui::Stroke::new(1.5, p.weak)),
        };
        right = dot.x - 12.0;
    }
    let left = rect.left() + 52.0;
    let font = if selected { semibold(14.0) } else { FontId::proportional(14.0) };
    let title = one_line(ui, &row.title, font, p.text, (right - left).max(40.0));
    let title_w = title.size().x;
    ui.painter().galley(egui::pos2(left, rect.center().y - title.size().y / 2.0), title, p.text);
    if let Some(detail) = &row.detail {
        let x = left + title_w + 8.0;
        let font = if row.mono { FontId::monospace(12.5) } else { FontId::proportional(13.0) };
        if right - x > 30.0 {
            let g = one_line(ui, detail, font, p.weak, right - x);
            ui.painter().galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, p.weak);
        }
    }
    r
}

/// Строка действия Кузницы: 34, значок, название, проект, клавиши.
fn draw_row(ui: &mut Ui, item: &Row, selected: bool) -> egui::Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    // Выбранная строка — как на Пульте: `raised` и полоска акцента, без цветного текста.
    if selected {
        ui.painter().rect_filled(rect, 6, p.raised);
        let bar = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - 9.0), Vec2::new(3.0, 18.0));
        ui.painter().rect_filled(bar, 2, p.accent);
    }
    let icon = egui::Rect::from_min_size(egui::pos2(rect.left() + 10.0, rect.center().y - 8.0), Vec2::splat(16.0));
    anvil_ui::icons::paint(ui.painter(), icon, item.icon, if selected { p.text } else { p.weak });
    let title = ui.painter().text(
        egui::pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        &item.title,
        if selected { semibold(14.0) } else { FontId::proportional(14.0) },
        p.text,
    );
    if let Some(project) = &item.detail {
        ui.painter().text(
            egui::pos2(title.right() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            project,
            FontId::proportional(13.0),
            p.weak,
        );
    }
    if let Some(keys) = &item.keys {
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            keys,
            FontId::proportional(12.5),
            p.faint,
        );
    }
    let label = match &item.detail {
        Some(project) => format!("{} · {project}", item.title),
        None => item.title.clone(),
    };
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &label));
    response
}

/// Строки по разделам: номера в `rows`. Пустой запрос — «Запущено», «Закреплено», «Недавнее» (а в
/// окне ещё действия Кузницы); иначе — всё подходящее, лучшее сверху.
fn arrange(rows: &[Row], query: &str, layout: bool, quick: bool) -> Vec<(Section, usize)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        let mut out: Vec<(Section, usize)> = Vec::new();
        let mut taken = std::collections::HashSet::new();
        for (i, row) in rows.iter().enumerate() {
            if row.running && taken.insert(i) {
                out.push((Section::Running, i));
            }
        }
        let mut pins: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].pin.is_some()).collect();
        pins.sort_by_key(|&i| rows[i].pin);
        for i in pins {
            if taken.insert(i) {
                out.push((Section::Pinned, i));
            }
        }
        let mut recent: Vec<usize> =
            (0..rows.len()).filter(|&i| rows[i].launched.is_some() && !taken.contains(&i)).collect();
        recent.sort_by_key(|&i| std::cmp::Reverse(rows[i].launched));
        for i in recent.into_iter().take(5) {
            taken.insert(i);
            out.push((Section::Recent, i));
        }
        // Нечего показать — все предметы.
        if out.is_empty() {
            out.extend((0..rows.len()).filter(|&i| rows[i].section == Section::Launch).map(|i| (Section::Launch, i)));
        }
        if !quick {
            out.extend((0..rows.len()).filter(|&i| rows[i].section == Section::Other).map(|i| (Section::Other, i)));
        }
        return out;
    }
    let swapped: Vec<String> = words.iter().map(|w| crate::layout::swapped(w)).collect();
    let mut scored: Vec<(Section, usize, bool, usize)> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !quick || row.section != Section::Other)
        .filter_map(|(i, row)| {
            score(row, &words, layout.then_some(swapped.as_slice())).map(|s| (row.section, s, !row.crashed, i))
        })
        .collect();
    // Раздел, потом совпадение, при равном — упавшее выше, потом порядок строк.
    scored.sort();
    // Точно набранный псевдоним закреплённого — первым, над всеми разделами.
    // В любой раскладке, как и остальной поиск (§5.11).
    let query = words.join(" ");
    let other = layout.then(|| swapped.join(" "));
    let exact = |i: usize| rows[i].alias.as_deref().is_some_and(|a| a == query || other.as_deref() == Some(a));
    let (mut first, rest): (Vec<_>, Vec<_>) = scored.into_iter().partition(|&(_, _, _, i)| exact(i));
    first.extend(rest);
    first.into_iter().map(|(section, _, _, i)| (section, i)).collect()
}

/// Каждое слово должно найтись; чем чаще это начало слова, тем выше строка. `swapped` — те же
/// слова в другой раскладке: берётся лучшее из двух.
fn score(row: &Row, words: &[String], swapped: Option<&[String]>) -> Option<usize> {
    let texts = [
        row.title.to_lowercase(),
        row.detail.as_deref().unwrap_or_default().to_lowercase(),
        row.extra.to_lowercase(),
        row.alias.as_deref().unwrap_or_default().to_lowercase(),
    ];
    let starts = |word: &str| {
        texts
            .iter()
            .any(|text| text.split([' ', '·', '-', '(', '\\', '/', ':', '|']).any(|part| part.starts_with(word)))
    };
    let cost = |word: &str| {
        if starts(word) {
            Some(0)
        } else if texts.iter().any(|text| text.contains(word)) {
            Some(1)
        } else {
            None
        }
    };
    words.iter().enumerate().try_fold(0, |total, (n, word)| {
        let other = swapped.and_then(|s| s.get(n)).and_then(|w| cost(w));
        let best = [cost(word), other].into_iter().flatten().min()?;
        Some(total + best)
    })
}

/// Все строки: предметы Пульта с профилями, их действия; в окне — ещё проекты и общее.
fn rows(app: &mut App, quick: bool) -> Vec<Row> {
    let frame = super::deck::Frame::new(app);
    let mut out = Vec::new();
    let now = i18n::now();
    let mut pin = 0;
    let mut actions = Vec::new();
    for (item, look) in frame.items.iter().zip(&frame.looks) {
        if item.is_self() {
            continue;
        }
        let pinned = app.config.deck.pinned.contains(&item.key).then(|| {
            pin += 1;
            pin
        });
        let pin_keys = pinned.filter(|n| *n <= 9).map(|n| format!("Alt+{n}"));
        let launched = app.config.deck.launched.get(&item.key).copied();
        // Как запрос: строчными, пробелы — по одному.
        let alias = pinned
            .and(app.config.deck.aliases.get(&item.key))
            .map(|a| a.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase())
            .filter(|a| !a.is_empty());
        let boxed = || Box::new(item.clone());
        if item.kind != ProjectKind::Rust || item.bin.is_none() {
            // Godot и Unity: одна строка — главное действие.
            let detail = match &look.chip {
                (word, Some(rest)) => format!("{word} {rest}"),
                (word, None) => word.clone(),
            };
            let state = look.running.map(|_| (Some(Tone::Success), look.state.clone()));
            out.push(Row {
                icon: Icon::Play,
                mark: Some(item.mark),
                title: item.name.clone(),
                detail: Some(detail),
                mono: false,
                state,
                keys: pin_keys.clone(),
                extra: format!("{} {}", item.caption, t("запустить")),
                section: Section::Launch,
                running: look.running.is_some(),
                pin: pinned.filter(|n| *n <= 9),
                launched,
                crashed: false,
                target: Some((boxed(), None)),
                verb: lower(&look.hint),
                alias: alias.clone(),
                command: Command::Launch(boxed(), None),
            });
            continue;
        }
        let bin = item.bin.clone().unwrap_or_default();
        let installed = app.installs.get(&bin).and_then(Option::as_ref).and_then(|i| i.current.clone());
        let presets = app.config.project(&item.project).presets;
        // Выбранный профиль — как его понимает запуск (профиль пропал из настроек — «обычный»).
        let chosen = app.profile_of(item).name;
        let built = super::page::build_label(app, item).map(|(label, _, _)| label);
        let untracked = look.running.is_some() && !app.runs.iter().any(|r| r.key == item.key && r.running());
        for profile in crate::deck::profiles(item, &presets, installed.is_some()) {
            let is_chosen = profile.name == chosen;
            let last = app.runs.iter().rev().find(|r| r.key == item.key && r.profile == profile.name);
            let run = last.filter(|r| r.running());
            let mut detail = Vec::new();
            if !profile.args.trim().is_empty() {
                detail.push(profile.args.trim().to_owned());
            }
            detail.push(match (run, profile.source, &installed) {
                (Some(run), _, _) => match run.source.split_once(' ') {
                    Some((word, rest)) => format!("{} {rest}", i18n::source_word(word)),
                    None => i18n::source_word(&run.source),
                },
                (None, crate::config::Source::Installed, Some(v)) => format!("{} {v}", t("установлена")),
                (None, _, _) => match &built {
                    Some(label) => format!("{} {label}", t("сборка")),
                    None => t("не собран").to_owned(),
                },
            });
            // Работает не из Anvil — считается «обычным».
            let running = run.is_some() || (untracked && profile.name.is_empty());
            let state = if running {
                let started = run.map(|r| r.started).or(look.running.and_then(|(_, s)| s));
                let text = match (item.group, profile.port) {
                    (Group::Services, Some(port)) => format!("{} · :{port}", t("работает")),
                    _ => match started {
                        Some(s) => format!("{} · {}", t("работает"), i18n::uptime(now - s)),
                        None => t("работает").to_owned(),
                    },
                };
                Some((Some(Tone::Success), text))
            } else {
                last.filter(|r| r.end == End::Crashed && !r.seen)
                    .map(|r| (Some(Tone::Danger), i18n::crashed_short(r.ended.unwrap_or(r.started))))
            };
            let title =
                if profile.name.is_empty() { item.name.clone() } else { format!("{} · {}", item.name, profile.name) };
            // Выбранный профиль запускается как Enter на Пульте (к окну, поставить), как и копия,
            // запущенная не из Anvil (её окно, а не вторая копия); остальные — именно этим профилем.
            let as_deck = is_chosen || (running && run.is_none());
            let command = if as_deck {
                Command::Launch(boxed(), None)
            } else {
                Command::Launch(boxed(), Some(profile.name.clone()))
            };
            let verb = if as_deck {
                deck_verb(&look.main)
            } else if running {
                if item.group == Group::Services { t("журнал") } else { t("к окну") }.to_owned()
            } else if profile.source == crate::config::Source::Installed && installed.is_some() {
                t("запустить").to_owned()
            } else {
                t("собрать и запустить").to_owned()
            };
            out.push(Row {
                icon: Icon::Play,
                mark: Some(item.mark),
                title: title.clone(),
                detail: Some(detail.join(" · ")),
                mono: true,
                state: state.clone(),
                keys: if is_chosen { pin_keys.clone() } else { None },
                extra: format!(
                    "{bin} {} {} {}",
                    profile.label(),
                    t("запустить"),
                    if is_chosen { alias.clone().unwrap_or_default() } else { String::new() }
                ),
                section: Section::Launch,
                running,
                pin: if is_chosen { pinned.filter(|n| *n <= 9) } else { None },
                launched: if is_chosen { launched } else { None },
                crashed: matches!(state, Some((Some(Tone::Danger), _))),
                target: Some((boxed(), Some(profile.name.clone()))),
                verb,
                alias: if is_chosen { alias.clone() } else { None },
                command,
            });
            if is_chosen {
                actions.push(Row {
                    icon: Icon::Hammer,
                    mark: None,
                    title: format!("{} · {title}", t("Собрать из кода и запустить")),
                    detail: Some(format!("cargo build --release --bin {bin}")),
                    mono: true,
                    state: None,
                    keys: Some("Ctrl+Enter".to_owned()),
                    extra: format!("{bin} {}", t("из кода")),
                    section: Section::Actions,
                    running: false,
                    pin: None,
                    launched: None,
                    crashed: false,
                    target: Some((boxed(), None)),
                    verb: t("собрать и запустить").to_owned(),
                    alias: None,
                    command: Command::FromCode(boxed(), None),
                });
            }
        }
        if item.group == Group::Programs {
            actions.push(Row {
                icon: Icon::Download,
                mark: None,
                title: format!("{} · {}…", t("Поставить из кода"), item.name),
                detail: Some(bin.clone()),
                mono: true,
                state: None,
                keys: None,
                extra: t("установить").to_owned(),
                section: Section::Actions,
                running: false,
                pin: None,
                launched: None,
                crashed: false,
                target: Some((boxed(), None)),
                verb: t("поставить").to_owned(),
                alias: None,
                command: Command::InstallCode(item.project.clone(), bin.clone()),
            });
        }
    }
    out.append(&mut actions);
    if !quick {
        forge_rows(app, &mut out);
    }
    out
}

/// Первая буква — строчная: подпись «Играть» → «играть» у клавиши Enter.
fn lower(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_lowercase().chain(chars).collect()).unwrap_or_default()
}

/// Что сделает Enter по строке Пульта — по её главному действию.
fn deck_verb(main: &super::deck::Main) -> String {
    use super::deck::Main;
    match main {
        Main::Launch => t("запустить"),
        Main::FromCode => t("собрать и запустить"),
        Main::Focus(_) => t("к окну"),
        Main::Journal => t("журнал"),
        Main::Install => t("поставить и запустить"),
        Main::Editor => t("открыть в редакторе"),
        Main::Nothing => "",
    }
    .to_owned()
}

/// Действия Кузницы: выбранный проект первым, «перейти» к остальным и их действия, потом общее.
fn forge_rows(app: &App, out: &mut Vec<Row>) {
    let current = app.current().map(|p| p.path.clone());
    let mut projects = app.visible();
    projects.sort_by_key(|p| Some(&p.path) != current.as_ref());
    for project in &projects {
        let path = project.path.clone();
        let name = project.name();
        let item = |icon, title: &str, command| Row::plain(icon, title, Some(name.clone()), None, command);
        if Some(&path) != current.as_ref() {
            out.push(item(Icon::ArrowRight, t("Перейти"), Command::Select(path.clone())));
        }
        if project.kind == ProjectKind::Rust {
            let settings = app.config.project(&path);
            let meta = project.meta();
            if let Some((label, bin, args)) = tasks::run_target(meta, &settings.presets, settings.run.as_deref()) {
                let title = format!("{} {label}", t("Запустить"));
                out.push(item(Icon::Play, &title, Command::Task(path.clone(), Task::Run { bin, args }, None)));
            }
            out.push(item(Icon::Hammer, t("Собрать debug"), Command::Task(path.clone(), Task::Build, Some(false))));
            out.push(item(Icon::Hammer, t("Собрать release"), Command::Task(path.clone(), Task::Build, Some(true))));
            out.push(item(Icon::Check, t("Тесты"), Command::Task(path.clone(), Task::Test, None)));
            out.push(item(Icon::Search, "Clippy", Command::Task(path.clone(), Task::Clippy, None)));
            out.push(item(Icon::Search, t("Проверить форматирование"), Command::Task(path.clone(), Task::Fmt, None)));
            out.push(item(Icon::Package, t("Проверить зависимости"), Command::CheckDeps(path.clone())));
            if project.git().is_some() {
                out.push(item(Icon::Rocket, t("Выпуск…"), Command::Release(path.clone())));
            }
            let remote = app.remotes.get(&path);
            for bin in meta.map(|m| m.bins.as_slice()).unwrap_or_default() {
                if bin.name == crate::amber::ADMIN {
                    out.push(item(Icon::Server, t("Открыть amber-admin"), Command::AmberAdmin(path.clone())));
                }
                let installed = app.installs.get(&bin.name).and_then(Option::as_ref);
                let release = super::install::release_for(remote, &bin.name, app.config.common.prerelease);
                if let Some(version) = super::install::newer(installed, release.as_ref()) {
                    let title = format!("{} {} → {version}", t("Обновить"), bin.name);
                    out.push(item(Icon::Download, &title, Command::Update(path.clone(), bin.name.clone())));
                }
            }
        }
        out.push(item(Icon::Folder, t("Открыть папку"), Command::Folder(path.clone())));
        out.push(item(Icon::Terminal, t("Терминал в папке"), Command::Terminal(path.clone())));
        out.push(item(Icon::Code, t("Открыть в VS Code"), Command::Code(path.clone())));
    }
    let global = |icon, title: &str, keys, command| Row::plain(icon, title, None, keys, command);
    let overview = if app.mode == Mode::Forge && app.view == View::Overview {
        t("Карточка проекта")
    } else {
        t("Обзор проектов")
    };
    out.extend([
        global(Icon::Play, t("Пульт"), Some("Ctrl+1"), Command::Mode(Mode::Deck)),
        global(Icon::Hammer, t("Кузница"), Some("Ctrl+2"), Command::Mode(Mode::Forge)),
        global(Icon::Tiles, overview, Some("Ctrl+0"), Command::Overview),
        global(Icon::Refresh, t("Обновить всё и спросить origin"), Some("F5"), Command::Refresh),
        global(Icon::Package, t("Проверить зависимости всех проектов"), None, Command::CheckAllDeps),
        global(Icon::Layers, t("Rust и зависимости"), None, Command::Toolchain),
        global(
            Icon::Terminal,
            if app.log_open { t("Скрыть консоль") } else { t("Показать консоль") },
            Some("Ctrl+L"),
            Command::Log,
        ),
        global(Icon::Plus, t("Добавить папку с проектами…"), None, Command::AddFolder),
        global(Icon::Gear, t("Настройки"), Some("Ctrl+,"), Command::Settings),
        global(Icon::Download, t("Проверить обновления Anvil"), None, Command::CheckUpdates),
        global(Icon::Info, t("О программе"), None, Command::About),
    ]);
}

fn run(app: &mut App, ctx: &egui::Context, command: Command) {
    let quick = app.quick.is_some();
    // Быстрый запуск: то, что открывает окно (страница, Кузница, вопрос; журнал работающей службы),
    // — в обычном окне.
    let opens_window = match &command {
        Command::Page(_) | Command::Forge(_) | Command::InstallCode(..) => true,
        Command::Launch(item, profile) => super::deck::opens_page(app, item, profile.as_deref()),
        _ => false,
    };
    if quick && opens_window {
        app.end_quick(true);
    }
    let locked_before = app.locked.is_some();
    match command {
        Command::Launch(item, None) => super::deck::run_main(app, &item),
        Command::Launch(item, Some(profile)) => super::deck::launch_profile(app, &item, &profile),
        Command::FromCode(item, profile) => {
            if let Some(profile) = profile
                && super::deck::choose_profile(app, &item, &profile)
            {
                app.save();
            }
            app.deck_view.selected = Some(item.key.clone());
            app.launch_item(&item, true);
        }
        Command::Page(key) => {
            app.set_mode(Mode::Deck);
            app.open_page(key);
        }
        Command::Forge(item) => {
            app.deck_view.selected = Some(item.key.clone());
            app.select(item.project.clone());
            app.set_mode(Mode::Forge);
            app.view = View::Project;
        }
        Command::InstallCode(project, bin) => app.install_confirm = Some((project, bin)),
        Command::Mode(mode) => app.set_mode(mode),
        Command::Select(path) => {
            app.select(path);
            app.view = View::Project;
            app.set_mode(Mode::Forge);
        }
        Command::Task(path, task, release) => {
            app.select(path.clone());
            match release {
                Some(release) => app.start_task_as(&path, task, release),
                None => app.start_task(&path, task),
            };
        }
        Command::Folder(path) => app.report(crate::open::folder(&path)),
        Command::Terminal(path) => app.report(crate::open::terminal(&path)),
        Command::Code(path) => app.report(crate::open::code(&path, &path)),
        Command::CheckDeps(path) => {
            app.select(path.clone());
            app.view = View::Project;
            app.set_mode(Mode::Forge);
            app.tab = crate::app::Tab::Deps;
            app.check_deps(vec![path], true);
        }
        Command::Release(path) => {
            app.select(path.clone());
            app.open_release(&path);
        }
        Command::AmberAdmin(path) => app.open_amber_admin(&path),
        Command::Update(path, bin) => {
            let remote = app.remotes.get(&path).cloned();
            let release = super::install::release_for(remote.as_ref(), &bin, app.config.common.prerelease);
            if let Some((release, _)) = release {
                let release = release.clone();
                app.install_release(&path, &bin, &release);
            }
        }
        Command::Refresh => {
            app.refresh();
            app.fetch();
        }
        Command::CheckAllDeps => {
            let paths = app.projects.iter().filter(|p| p.kind == ProjectKind::Rust).map(|p| p.path.clone()).collect();
            app.check_deps(paths, true);
        }
        Command::Settings => app.settings_open = true,
        Command::Toolchain => app.overview_open = true,
        Command::Overview => app.toggle_view(),
        Command::AddFolder => super::settings::add_root(app),
        Command::Log => app.log_open = !app.log_open,
        Command::About => app.about_open = true,
        Command::CheckUpdates => app.updater.check(app.config.common.prerelease, None, true),
    }
    if quick && !opens_window {
        // Сборке мешает запущенный exe — спросить в строке; иначе — спрятаться сразу (§7.5).
        if !locked_before && app.locked.is_some() {
            if let Some(state) = &mut app.palette {
                state.asking = true;
            }
        } else if app.config.quick.close_after {
            app.end_quick(false);
        } else if let Some(state) = &mut app.palette {
            state.query.clear();
            state.selected = 0;
            state.scroll = true;
        }
    } else if !quick {
        app.palette = None;
    }
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, project: Option<&str>) -> Row {
        Row::plain(Icon::Play, title, project.map(str::to_owned), None, Command::Refresh)
    }

    fn found(rows: &[Row], query: &str) -> Vec<usize> {
        arrange(rows, query, true, false).into_iter().map(|(_, i)| i).collect()
    }

    #[test]
    fn every_word_must_match() {
        let items = [
            item("Собрать debug", Some("Amber")),
            item("Собрать release", Some("Amber")),
            item("Собрать release", Some("Tetrachrome")),
            item("Настройки", None),
        ];
        assert_eq!(found(&items, "собрать amber release"), vec![1]);
        assert_eq!(found(&items, "release"), vec![1, 2]);
        assert_eq!(found(&items, "tetra"), vec![2]);
        assert_eq!(found(&items, "").len(), 4);
        assert!(found(&items, "нет такого").is_empty());
    }

    #[test]
    fn word_starts_rank_higher() {
        let items = [item("Проверить форматирование", None), item("Открыть папку", Some("Amber"))];
        // «пап» — начало слова во второй строке, в первой не найдено вовсе.
        assert_eq!(found(&items, "пап"), vec![1]);
        let items = [item("Показать консоль", None), item("Консоль CI", None)];
        assert_eq!(found(&items, "кон"), vec![0, 1]);
        let items = [item("Каталог", None), item("Лог", None)];
        assert_eq!(found(&items, "лог"), vec![1, 0]);
    }

    #[test]
    fn wrong_layout_still_finds() {
        let mut test = item("Amber · test", Some("--profile test · сборка 2353af9"));
        test.section = Section::Launch;
        let mut plain = item("Amber", Some("установлена 0.4.0"));
        plain.section = Section::Launch;
        let items = [plain, test, item("Собрать release", Some("Amber"))];
        assert_eq!(found(&items, "фь еу"), vec![1]);
        assert_eq!(found(&items, "am te"), vec![1]);
        assert_eq!(found(&items, "cj,hfnm"), vec![2]);
        // Выключено в настройках — только как набрано.
        assert!(arrange(&items, "фь еу", false, false).is_empty());
    }

    #[test]
    fn empty_query_shows_running_pinned_recent() {
        let mut a = item("A", None);
        a.section = Section::Launch;
        a.running = true;
        let mut b = item("B", None);
        b.section = Section::Launch;
        b.pin = Some(1);
        let mut c = item("C", None);
        c.section = Section::Launch;
        c.launched = Some(10);
        let mut d = item("D", None);
        d.section = Section::Launch;
        d.launched = Some(20);
        let other = item("Настройки", None);
        let rows = [a, b, c, d, other];
        let quick: Vec<(Section, usize)> = arrange(&rows, "", true, true);
        assert_eq!(
            quick,
            vec![(Section::Running, 0), (Section::Pinned, 1), (Section::Recent, 3), (Section::Recent, 2)]
        );
        // В окне — ещё действия Кузницы.
        assert_eq!(arrange(&rows, "", true, false).last(), Some(&(Section::Other, 4)));
    }

    #[test]
    fn exact_alias_comes_first() {
        let mut amber = item("Amber", None);
        amber.section = Section::Launch;
        let mut server = item("amber-server · test", None);
        server.section = Section::Launch;
        server.alias = Some("as".into());
        let rows = [amber, server, item("Настройки", Some("Amber"))];
        assert_eq!(found(&rows, "as"), vec![1]);
        assert_eq!(found(&rows, "AS"), vec![1]);
        // В другой раскладке — тоже точно.
        assert_eq!(found(&rows, "фы"), vec![1]);
        // Не точно — как обычное совпадение: «am» — начало имени у обоих.
        assert_eq!(found(&rows, "am"), vec![0, 1, 2]);
    }

    #[test]
    fn crashed_rises_on_equal_match() {
        let mut ok = item("Amber", None);
        ok.section = Section::Launch;
        let mut crashed = item("Amber · test", None);
        crashed.section = Section::Launch;
        crashed.crashed = true;
        assert_eq!(found(&[ok, crashed], "amber"), vec![1, 0]);
    }
}
