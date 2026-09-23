//! Отрисовка окна. Каждая часть — в своём файле.

mod project;
mod settings;
mod sidebar;

use anvil_ui::chrome::{self, AboutAction, AppInfo};
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, Ui};

use crate::app::App;
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
    chrome::side_panel(ui, "projects", 260.0, |ui| sidebar::show(app, ui));
    chrome::content(ui, |ui| project::show(app, ui));

    settings::show(app, &ctx);
    let info = info();
    if chrome::about(&ctx, &mut app.about_open, &info, None) == Some(AboutAction::CheckUpdates) {
        app.toasts.push(t("Проверка обновлений появится вместе с anvil-update"), Tone::Neutral);
    }
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
        ctx.memory_mut(|m| m.request_focus(search_id()));
    }
}

fn search_id() -> egui::Id {
    egui::Id::new("anvil-search")
}

fn top_bar(app: &mut App, ui: &mut Ui) {
    chrome::top_bar(ui, |ui| {
        let info = info();
        chrome::brand(ui, info.icon, info.name);
        ui.add_space(18.0);
        w::search_field_with_id(ui, search_id(), &mut app.search, t("Найти проект…"), Some("Ctrl+K"), 320.0);

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
            w::note(ui, text);
        });
    });
}
