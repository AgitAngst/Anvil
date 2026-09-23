//! Окно настроек: общие для семьи и свои — папки, скрытые проекты, опрос origin.

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette};
use eframe::egui::{self, Ui};

use crate::app::App;
use crate::i18n::{self, t};
use crate::worker;

/// Выбрать папку и добавить её к корням поиска.
pub fn add_root(app: &mut App) {
    let Some(dir) = rfd::FileDialog::new().set_title(t("Папка с проектами")).pick_folder() else {
        return;
    };
    if !app.config.roots.iter().any(|r| crate::registry::same_dir(r, &dir)) {
        app.config.roots.push(dir);
        app.save();
        app.rescan();
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let mut open = app.settings_open;
    chrome::dialog(ctx, "anvil-settings", t("Настройки"), 520.0, &mut open, |ui| {
        if chrome::common_settings(ui, &mut app.config.common) {
            i18n::set(ui.ctx(), app.config.common.language);
            app.save();
        }
        ui.add_space(12.0);
        roots(app, ui);
        ui.add_space(12.0);
        hidden(app, ui);
        ui.add_space(12.0);
        fetch(app, ui);
    });
    app.settings_open = open;
}

fn roots(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Папки с проектами"));
    ui.add_space(2.0);
    w::note(ui, t("Проект — это сама папка или её подпапка, где есть Cargo.toml."));
    ui.add_space(4.0);
    let mut remove = None;
    for (i, root) in app.config.roots.iter().enumerate() {
        path_row(ui, &root.to_string_lossy(), |ui| {
            if w::icon_button(ui, Icon::Trash, t("Убрать папку")).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        app.config.roots.remove(i);
        app.save();
        app.rescan();
    }
    ui.add_space(4.0);
    if w::button(ui, Kind::Secondary, Some(Icon::Plus), t("Добавить папку…")).clicked() {
        add_root(app);
    }
}

fn hidden(app: &mut App, ui: &mut Ui) {
    if app.config.hidden.is_empty() {
        return;
    }
    w::section_label(ui, t("Скрытые проекты"));
    ui.add_space(4.0);
    let mut restore = None;
    for (i, path) in app.config.hidden.iter().enumerate() {
        path_row(ui, &worker::display_name(path), |ui| {
            if w::button(ui, Kind::Ghost, None, t("Вернуть")).clicked() {
                restore = Some(i);
            }
        });
    }
    if let Some(i) = restore {
        app.config.hidden.remove(i);
        app.save();
    }
}

fn fetch(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Опрос origin"));
    ui.add_space(2.0);
    chrome::setting_row(ui, t("Проверять новые коммиты"), |ui| {
        let before = app.config.fetch_minutes;
        let mut minutes = before;
        w::segmented(
            ui,
            &mut minutes,
            &[(0, None, t("вручную")), (5, None, t("5 мин")), (15, None, t("15 мин")), (60, None, t("час"))],
        );
        if minutes != before {
            app.config.fetch_minutes = minutes;
            app.save();
        }
    });
    w::note(ui, t("git fetch только узнаёт о новом на origin и ничего не меняет в рабочей копии."));
}

fn path_row(ui: &mut Ui, text: &str, trailing: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(anvil_ui::theme::radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(10, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                w::mono(ui, text, Some(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
            });
        });
    ui.add_space(4.0);
}
