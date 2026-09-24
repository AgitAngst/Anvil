//! Палитра `Ctrl+K`: найти проект или действие и выполнить, не снимая рук с клавиатуры.
//! «собрать amber release», «тесты tetra», «обзор» — каждое слово запроса должно найтись
//! в названии действия или имени проекта.

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Palette, semibold};
use eframe::egui::{self, Align2, FontId, Key, Modifiers, Sense, Ui, Vec2};

use crate::app::{App, View};
use crate::i18n::t;
use crate::registry::Kind as ProjectKind;
use crate::tasks::{self, Task};

/// Открытая палитра: запрос и выбранная строка.
#[derive(Default)]
pub struct State {
    query: String,
    selected: usize,
    /// Выбор сдвинули клавишами — прокрутить к нему.
    scroll: bool,
}

/// Что делает строка палитры.
#[derive(Clone)]
enum Command {
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

struct Item {
    icon: Icon,
    title: String,
    /// Имя проекта, к которому относится действие.
    project: Option<String>,
    keys: Option<&'static str>,
    command: Command,
}

pub fn open(app: &mut App) {
    app.palette = Some(State::default());
}

/// Поле-кнопка в шапке: выглядит как поиск, открывает палитру.
pub fn launcher(app: &mut App, ui: &mut Ui, width: f32) {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let border = if response.hovered() { p.border_strong } else { p.border };
    ui.painter().rect(rect, 6, p.field, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
    let icon = egui::Rect::from_min_size(egui::pos2(rect.left() + 9.0, rect.center().y - 8.0), Vec2::splat(16.0));
    anvil_ui::icons::paint(ui.painter(), icon, Icon::Search, p.faint);
    let hint = t("Найти проект или действие…");
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

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut state) = app.palette.take() else { return };
    let items = items(app);
    let found = filter(&items, &state.query);

    // Стрелки и Enter — до поля ввода: однострочное поле само на них отвечает.
    let (down, up, enter) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::Enter),
        )
    });
    if !found.is_empty() {
        if down {
            state.selected = (state.selected + 1) % found.len();
            state.scroll = true;
        }
        if up {
            state.selected = (state.selected + found.len() - 1) % found.len();
            state.scroll = true;
        }
    }
    state.selected = state.selected.min(found.len().saturating_sub(1));
    let mut chosen = enter.then(|| found.get(state.selected).map(|&i| items[i].command.clone())).flatten();

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
            w::search_field_with_id(ui, field, &mut state.query, t("Проект или действие…"), Some("Esc"), 560.0);
            ui.memory_mut(|m| m.request_focus(field));
            ui.add_space(6.0);
            if found.is_empty() {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| w::note(ui, t("Ничего не нашлось.")));
                ui.add_space(12.0);
                return;
            }
            egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for (row, &index) in found.iter().enumerate() {
                    let item = &items[index];
                    let selected = row == state.selected;
                    let r = draw_row(ui, item, selected);
                    if selected && state.scroll {
                        r.scroll_to_me(None);
                    }
                    if r.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                        state.selected = row;
                    }
                    if r.clicked() {
                        chosen = Some(item.command.clone());
                    }
                }
            });
        });
    state.scroll = false;
    if state.query != before {
        state.selected = 0;
    }
    let close = response.should_close();
    match chosen {
        Some(command) => run(app, ctx, command),
        None if !close => app.palette = Some(state),
        None => {}
    }
}

fn draw_row(ui: &mut Ui, item: &Item, selected: bool) -> egui::Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 6, p.soft(p.accent));
    }
    let icon = egui::Rect::from_min_size(egui::pos2(rect.left() + 10.0, rect.center().y - 8.0), Vec2::splat(16.0));
    anvil_ui::icons::paint(ui.painter(), icon, item.icon, if selected { p.accent_text } else { p.weak });
    let title = ui.painter().text(
        egui::pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        &item.title,
        if selected { semibold(14.0) } else { FontId::proportional(14.0) },
        p.text,
    );
    if let Some(project) = &item.project {
        ui.painter().text(
            egui::pos2(title.right() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            project,
            FontId::proportional(13.0),
            p.weak,
        );
    }
    if let Some(keys) = item.keys {
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            keys,
            FontId::proportional(12.5),
            p.faint,
        );
    }
    let label = match &item.project {
        Some(project) => format!("{} · {project}", item.title),
        None => item.title.clone(),
    };
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &label));
    response
}

/// Номера подходящих строк, лучшие сверху. Пустой запрос — всё по порядку.
fn filter(items: &[Item], query: &str) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut scored: Vec<(usize, usize)> =
        items.iter().enumerate().filter_map(|(i, item)| score(item, &words).map(|s| (s, i))).collect();
    scored.sort_by_key(|&(s, i)| (s, i));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// Каждое слово должно найтись; чем чаще это начало слова, тем выше строка.
fn score(item: &Item, words: &[String]) -> Option<usize> {
    let title = item.title.to_lowercase();
    let project = item.project.as_deref().unwrap_or_default().to_lowercase();
    let starts = |text: &str, word: &str| text.split([' ', '·', '-', '(']).any(|part| part.starts_with(word));
    words.iter().try_fold(0, |total, word| {
        let cost = if starts(&title, word) || starts(&project, word) {
            0
        } else if title.contains(word.as_str()) || project.contains(word.as_str()) {
            1
        } else {
            return None;
        };
        Some(total + cost)
    })
}

/// Все строки: сначала выбранный проект, потом «перейти» к остальным и их действия, потом общее.
fn items(app: &App) -> Vec<Item> {
    let mut out = Vec::new();
    let current = app.current().map(|p| p.path.clone());
    let mut projects = app.visible();
    projects.sort_by_key(|p| Some(&p.path) != current.as_ref());
    for project in &projects {
        let path = project.path.clone();
        let name = project.name();
        let item = |icon, title: &str, keys, command| Item {
            icon,
            title: title.to_owned(),
            project: Some(name.clone()),
            keys,
            command,
        };
        if Some(&path) != current.as_ref() {
            out.push(item(Icon::ArrowRight, t("Перейти"), None, Command::Select(path.clone())));
        }
        if project.kind == ProjectKind::Rust {
            let settings = app.config.project(&path);
            let meta = project.meta();
            if let Some((label, bin, args)) = tasks::run_target(meta, &settings.presets, settings.run.as_deref()) {
                let title = format!("{} {label}", t("Запустить"));
                out.push(item(Icon::Play, &title, None, Command::Task(path.clone(), Task::Run { bin, args }, None)));
            }
            out.push(item(
                Icon::Hammer,
                t("Собрать debug"),
                None,
                Command::Task(path.clone(), Task::Build, Some(false)),
            ));
            out.push(item(
                Icon::Hammer,
                t("Собрать release"),
                None,
                Command::Task(path.clone(), Task::Build, Some(true)),
            ));
            out.push(item(Icon::Check, t("Тесты"), None, Command::Task(path.clone(), Task::Test, None)));
            out.push(item(Icon::Search, "Clippy", None, Command::Task(path.clone(), Task::Clippy, None)));
            out.push(item(
                Icon::Search,
                t("Проверить форматирование"),
                None,
                Command::Task(path.clone(), Task::Fmt, None),
            ));
            out.push(item(Icon::Package, t("Проверить зависимости"), None, Command::CheckDeps(path.clone())));
            if project.git().is_some() {
                out.push(item(Icon::Rocket, t("Выпуск…"), None, Command::Release(path.clone())));
            }
            let remote = app.remotes.get(&path);
            for bin in meta.map(|m| m.bins.as_slice()).unwrap_or_default() {
                if bin.name == crate::amber::ADMIN {
                    out.push(item(Icon::Server, t("Открыть amber-admin"), None, Command::AmberAdmin(path.clone())));
                }
                let installed = app.installs.get(&bin.name).and_then(Option::as_ref);
                let release = super::install::release_for(remote, &bin.name, app.config.common.prerelease);
                if let Some(version) = super::install::newer(installed, release.as_ref()) {
                    let title = format!("{} {} → {version}", t("Обновить"), bin.name);
                    out.push(item(Icon::Download, &title, None, Command::Update(path.clone(), bin.name.clone())));
                }
            }
        }
        out.push(item(Icon::Folder, t("Открыть папку"), None, Command::Folder(path.clone())));
        out.push(item(Icon::Terminal, t("Терминал в папке"), None, Command::Terminal(path.clone())));
        out.push(item(Icon::Code, t("Открыть в VS Code"), None, Command::Code(path.clone())));
    }
    let global =
        |icon, title: &str, keys, command| Item { icon, title: title.to_owned(), project: None, keys, command };
    let overview = if app.view == View::Overview {
        t("Карточка проекта")
    } else {
        t("Обзор проектов")
    };
    out.extend([
        global(Icon::Tiles, overview, Some("Ctrl+0"), Command::Overview),
        global(Icon::Refresh, t("Обновить всё и спросить origin"), Some("F5"), Command::Refresh),
        global(Icon::Package, t("Проверить зависимости всех проектов"), None, Command::CheckAllDeps),
        global(Icon::Layers, t("Rust и зависимости"), None, Command::Toolchain),
        global(Icon::Terminal, if app.log_open { t("Скрыть лог") } else { t("Показать лог") }, None, Command::Log),
        global(Icon::Plus, t("Добавить папку с проектами…"), None, Command::AddFolder),
        global(Icon::Gear, t("Настройки"), Some("Ctrl+,"), Command::Settings),
        global(Icon::Download, t("Проверить обновления Anvil"), None, Command::CheckUpdates),
        global(Icon::Info, t("О программе"), None, Command::About),
    ]);
    out
}

fn run(app: &mut App, ctx: &egui::Context, command: Command) {
    match command {
        Command::Select(path) => {
            app.select(path);
            app.view = View::Project;
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
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, project: Option<&str>) -> Item {
        Item {
            icon: Icon::Play,
            title: title.to_owned(),
            project: project.map(str::to_owned),
            keys: None,
            command: Command::Refresh,
        }
    }

    #[test]
    fn every_word_must_match() {
        let items = [
            item("Собрать debug", Some("Amber")),
            item("Собрать release", Some("Amber")),
            item("Собрать release", Some("Tetrachrome")),
            item("Настройки", None),
        ];
        assert_eq!(filter(&items, "собрать amber release"), vec![1]);
        assert_eq!(filter(&items, "release"), vec![1, 2]);
        assert_eq!(filter(&items, "tetra"), vec![2]);
        assert_eq!(filter(&items, "").len(), 4);
        assert!(filter(&items, "нет такого").is_empty());
    }

    #[test]
    fn word_starts_rank_higher() {
        let items = [item("Проверить форматирование", None), item("Открыть папку", Some("Amber"))];
        // «пап» — начало слова во второй строке, в первой не найдено вовсе.
        assert_eq!(filter(&items, "пап"), vec![1]);
        let items = [item("Показать лог", None), item("Логи CI", None)];
        assert_eq!(filter(&items, "лог"), vec![0, 1]);
        let items = [item("Каталог", None), item("Лог", None)];
        assert_eq!(filter(&items, "лог"), vec![1, 0]);
    }
}
