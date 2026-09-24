//! Отрисовка окна. Каждая часть — в своём файле.

mod amber;
pub mod deps;
mod dialogs;
mod github;
mod install;
mod jobs;
mod overview;
pub mod palette;
mod presets;
mod project;
pub mod release;
mod settings;
mod sidebar;

use anvil_ui::chrome::{self, AboutAction, AppInfo};
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, Ui};

use crate::app::{App, View};
use crate::i18n::{self, t};
use crate::worker::Busy;

pub fn info() -> AppInfo {
    AppInfo {
        name: "Anvil",
        icon: Icon::Hammer,
        version: env!("CARGO_PKG_VERSION"),
        tagline: t("Командный центр Rust-программ"),
        repository: env!("CARGO_PKG_REPOSITORY"),
    }
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    shortcuts(app, &ctx);
    top_bar(app, ui);
    status_bar(app, ui);
    jobs::panel(app, ui);
    chrome::side_panel(ui, "projects", 260.0, |ui| sidebar::show(app, ui));
    chrome::content(ui, |ui| {
        let updater = app.updater.clone();
        if anvil_update::ui::banner(ui, &updater, &mut app.config.common) {
            app.save();
        }
        match app.view {
            View::Project => project::show(app, ui),
            View::Overview => overview::show(app, ui),
        }
    });

    settings::show(app, &ctx);
    presets::show(app, &ctx);
    release::show(app, &ctx);
    dialogs::show(app, &ctx);
    deps::overview(app, &ctx);
    deps::confirm(app, &ctx);
    let info = info();
    let status = anvil_update::ui::about_status(&ctx, &app.updater);
    if chrome::about(&ctx, &mut app.about_open, &info, status.as_deref()) == Some(AboutAction::CheckUpdates) {
        app.updater.check(app.config.common.prerelease, None, true);
    }
    palette::show(app, &ctx);
    app.toasts.show(&ctx);
}

fn shortcuts(app: &mut App, ctx: &egui::Context) {
    use egui::{Key, KeyboardShortcut, Modifiers};
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
        app.refresh();
        app.fetch();
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma))) {
        app.settings_open = true;
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::K))) {
        if app.palette.is_some() {
            app.palette = None;
        } else {
            palette::open(app);
        }
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0))) {
        app.toggle_view();
    }
}

fn top_bar(app: &mut App, ui: &mut Ui) {
    chrome::top_bar(ui, |ui| {
        let info = info();
        chrome::brand(ui, info.icon, info.name);
        ui.add_space(18.0);
        palette::launcher(app, ui, 340.0);
        ui.add_space(10.0);
        let mut view = app.view;
        w::segmented(
            ui,
            &mut view,
            &[(View::Project, Some(Icon::File), t("Проект")), (View::Overview, Some(Icon::Tiles), t("Обзор"))],
        )
        .on_hover_text("Ctrl+0");
        app.view = view;

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let gear = w::icon_button(ui, Icon::Gear, t("Меню"));
            w::menu(&gear, 240.0, |ui| {
                if w::menu_item(ui, Some(Icon::Gear), t("Настройки"), Some("Ctrl+,")).clicked() {
                    app.settings_open = true;
                }
                if w::menu_item(ui, Some(Icon::Info), t("О программе"), None).clicked() {
                    app.about_open = true;
                }
                w::menu_separator(ui);
                if w::menu_item(ui, Some(Icon::File), t("Открыть anvil.toml"), None).clicked() {
                    let dir = app.config_path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                    if !app.config_path.exists() {
                        app.save();
                    }
                    app.report(crate::open::code(&app.config_path, &dir));
                }
            });
            let busy = app.busy.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                let hint = t("Перечитать всё и спросить origin (F5)");
                if w::button(ui, Kind::Secondary, Some(Icon::Refresh), t("Обновить")).on_hover_text(hint).clicked()
                {
                    app.refresh();
                    app.fetch();
                }
            });
        });
    });
}

fn status_bar(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    chrome::status_bar(ui, |ui| {
        if jobs::status(app, ui) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| log_toggle(app, ui));
            return;
        }
        match app.busy {
            Some(busy) => {
                w::spinner(ui, 14.0);
                let text = match busy {
                    Busy::Scanning => t("Ищу проекты…"),
                    Busy::Refreshing => t("Читаю состояние…"),
                    Busy::Fetching => t("Спрашиваю origin…"),
                };
                ui.label(egui::RichText::new(text).size(13.0).color(p.text));
            }
            None => {
                w::dot(ui, Tone::Success);
                let when = app.refreshed_at.map(i18n::ago).unwrap_or_else(|| "—".into());
                w::note(ui, format!("{} {when}", t("Состояние прочитано")));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let roots = app.config.roots.len();
            let total = app.visible().len();
            let text = format!(
                "{} · {}",
                i18n::count(total, ["проект", "проекта", "проектов"], ["project", "projects"]),
                i18n::count(roots, ["папка", "папки", "папок"], ["folder", "folders"]),
            );
            log_toggle(app, ui);
            w::note(ui, text);
        });
    });
}

fn log_toggle(app: &mut App, ui: &mut Ui) {
    let hint = if app.log_open { t("Скрыть лог") } else { t("Показать лог") };
    if w::button(ui, Kind::Ghost, Some(Icon::Terminal), t("Лог")).on_hover_text(hint).clicked() {
        app.log_open = !app.log_open;
    }
}
