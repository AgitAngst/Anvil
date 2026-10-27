//! Окно «Добавить проект»: репозитории аккаунта GitHub, папка на диске, адрес репозитория.

use std::path::PathBuf;

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::github::{AccountRepo, Repo};
use crate::i18n::t;
use crate::sources;

/// Строка списка, собранная заранее: во время отрисовки `app` нужен изменяемым.
struct Row {
    repo: Repo,
    private: bool,
    fork: bool,
    description: Option<String>,
    /// Лежит на диске (проект с таким origin уже найден).
    on_disk: bool,
}

/// Что попросили в окне: выполняется после отрисовки.
enum Action {
    Clone(Repo),
    Refresh,
    OpenSettings,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let mut open = app.sources.open;
    chrome::dialog(ctx, "anvil-add-project", t("Добавить проект"), 680.0, &mut open, |ui| {
        let mut tab = app.sources.tab;
        w::tabs(ui, &mut tab, &[t("С GitHub"), t("Папка"), t("Адрес")]);
        if tab != app.sources.tab {
            app.open_add_project(tab);
        }
        ui.add_space(14.0);
        match app.sources.tab {
            0 => github_tab(app, ui),
            1 => folder_tab(app, ui),
            _ => address_tab(app, ui),
        }
        if let Some(error) = app.sources.error.clone() {
            ui.add_space(10.0);
            w::banner(ui, Tone::Danger, t("Не удалось клонировать"), &error, |_| {});
        }
        if app.sources.tab != 1 {
            ui.add_space(12.0);
            clone_target(app, ui);
        }
    });
    app.sources.open = open;
}

/// Понятный текст вместо кода ответа GitHub.
fn explain(error: &str) -> String {
    if error.starts_with("401") {
        t("GitHub отклонил токен: проверьте его в Настройки → Кузница.").to_owned()
    } else if error.contains("rate limit") {
        t("Достигнут лимит запросов GitHub: попробуйте позже.").to_owned()
    } else {
        format!("{}: {error}", t("GitHub не ответил"))
    }
}

fn github_tab(app: &mut App, ui: &mut Ui) {
    let mut action = None;
    match app.sources.account.as_ref() {
        None if app.sources.loading => loading(ui),
        None => {
            w::note(ui, t("Список ещё не загружен."));
        }
        Some(Err(e)) if e == "no token" => {
            w::empty_state(
                ui,
                Icon::Lock,
                t("Нужен токен GitHub"),
                t("Добавьте его в Настройки → Кузница: Anvil покажет ваши репозитории и сможет их клонировать."),
            );
            ui.vertical_centered(|ui| {
                if w::button(ui, Kind::Secondary, Some(Icon::Gear), t("Открыть настройки")).clicked() {
                    action = Some(Action::OpenSettings);
                }
            });
        }
        Some(Err(e)) => {
            let text = explain(e);
            w::banner(ui, Tone::Danger, t("GitHub не отдал список"), &text, |ui| {
                if w::button(ui, Kind::Secondary, None, t("Повторить")).clicked() {
                    action = Some(Action::Refresh);
                }
            });
        }
        Some(Ok(list)) => {
            let rows = rows(app, list);
            list_view(app, ui, rows, &mut action);
        }
    }
    match action {
        Some(Action::Clone(repo)) => app.start_clone(repo),
        Some(Action::Refresh) => app.list_account(),
        Some(Action::OpenSettings) => {
            app.sources.open = false;
            app.settings_tab = 4;
            app.settings_open = true;
        }
        None => {}
    }
}

fn loading(ui: &mut Ui) {
    ui.horizontal(|ui| {
        w::spinner(ui, 14.0);
        w::note(ui, t("Спрашиваю GitHub…"));
    });
}

fn rows(app: &App, list: &[AccountRepo]) -> Vec<Row> {
    let needle = app.sources.filter.trim().to_lowercase();
    list.iter()
        .filter(|r| needle.is_empty() || r.repo.slug().to_lowercase().contains(&needle))
        .map(|r| Row {
            repo: r.repo.clone(),
            private: r.private,
            fork: r.fork,
            description: r.description.clone(),
            on_disk: sources::local_for(&r.repo, &app.projects).is_some(),
        })
        .collect()
}

fn list_view(app: &mut App, ui: &mut Ui, rows: Vec<Row>, action: &mut Option<Action>) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        w::search_field(ui, &mut app.sources.filter, t("Найти репозиторий"), None, 300.0);
        if app.sources.loading {
            w::spinner(ui, 14.0);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!app.sources.loading, |ui| {
                if w::button(ui, Kind::Ghost, None, t("Обновить список")).clicked() {
                    *action = Some(Action::Refresh);
                }
            });
        });
    });
    ui.add_space(8.0);
    if rows.is_empty() {
        w::note(ui, t("Ничего не найдено."));
        return;
    }
    let cloning = app.sources.cloning.clone();
    egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show(ui, |ui| {
        for row in &rows {
            ui.horizontal(|ui| {
                ui.label(RichText::new(row.repo.slug()).size(13.5).color(p.text));
                w::badge(ui, if row.private { t("закрытый") } else { t("публичный") }, Tone::Neutral);
                if row.fork {
                    w::note(ui, t("форк"));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if row.on_disk {
                        w::badge(ui, t("Подключён"), Tone::Success);
                    } else if cloning.as_ref() == Some(&row.repo) {
                        w::note(ui, t("Клонирую…"));
                        w::spinner(ui, 14.0);
                    } else {
                        ui.add_enabled_ui(cloning.is_none(), |ui| {
                            if w::button(ui, Kind::Secondary, Some(Icon::Download), t("Клонировать")).clicked()
                            {
                                *action = Some(Action::Clone(row.repo.clone()));
                            }
                        });
                    }
                });
            });
            if let Some(description) = &row.description {
                ui.add(egui::Label::new(RichText::new(description).size(12.0).color(p.weak)).truncate());
            }
            ui.add_space(4.0);
            w::divider(ui);
            ui.add_space(4.0);
        }
    });
}

fn folder_tab(app: &mut App, ui: &mut Ui) {
    w::empty_state(
        ui,
        Icon::Folder,
        t("Папка с проектом"),
        t("Выберите папку проекта или папку, в которой лежат проекты: Anvil найдёт в ней Rust, Godot и Unity."),
    );
    ui.vertical_centered(|ui| {
        if w::button(ui, Kind::Primary, Some(Icon::Plus), t("Выбрать папку…")).clicked() {
            app.sources.open = false;
            super::settings::add_root(app);
        }
    });
}

fn address_tab(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Адрес репозитория"));
    ui.add_space(4.0);
    let edit = egui::TextEdit::singleline(&mut app.sources.address)
        .hint_text(t("https://github.com/владелец/репозиторий или владелец/репозиторий"))
        .desired_width(ui.available_width());
    ui.add(edit);
    let typed = !app.sources.address.trim().is_empty();
    let repo = sources::parse_address(&app.sources.address);
    ui.add_space(6.0);
    if typed && repo.is_none() {
        w::note(ui, t("Не похоже на адрес репозитория GitHub."));
    }
    ui.add_space(8.0);
    let busy = app.sources.cloning.is_some();
    ui.horizontal(|ui| {
        ui.add_enabled_ui(repo.is_some() && !busy, |ui| {
            if w::button(ui, Kind::Primary, Some(Icon::Download), t("Клонировать")).clicked()
                && let Some(repo) = repo.clone()
            {
                app.start_clone(repo);
            }
        });
        if busy {
            w::spinner(ui, 14.0);
            w::note(ui, t("Клонирую…"));
        }
    });
}

/// Куда клонируется, и как это поменять.
fn clone_target(app: &mut App, ui: &mut Ui) {
    let root: Option<PathBuf> = sources::clone_root(&app.config);
    ui.horizontal(|ui| {
        w::note(ui, t("Клонируется в:"));
        match &root {
            Some(path) => {
                w::mono(ui, &path.to_string_lossy(), None);
            }
            None => {
                w::note(ui, t("папка не выбрана"));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::button(ui, Kind::Ghost, None, t("Изменить…")).clicked()
                && let Some(dir) = rfd::FileDialog::new().set_title(t("Папка клонов")).pick_folder()
            {
                app.config.clone_dir = Some(dir);
                app.save();
            }
        });
    });
}
