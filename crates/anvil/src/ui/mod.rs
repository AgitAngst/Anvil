//! Отрисовка окна. Каждая часть — в своём файле.

mod amber;
mod deck;
pub mod deps;
mod dialogs;
mod github;
pub mod install;
mod jobs;
mod overview;
mod page;
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

use crate::app::{App, Mode, View};
use crate::i18n::{self, t};
use crate::worker::Busy;

pub fn info() -> AppInfo {
    AppInfo {
        name: "Anvil",
        icon: Icon::Hammer,
        version: env!("CARGO_PKG_VERSION"),
        tagline: t("Пульт и кузница моих программ"),
        repository: env!("CARGO_PKG_REPOSITORY"),
    }
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    shortcuts(app, &ctx);
    // Пульт считается раз за кадр: он нужен и строкам, и чипам в строке состояния.
    let deck = deck::Frame::new(app);
    top_bar(app, ui);
    status_bar(app, ui, &deck.chips());
    jobs::panel(app, ui);
    if app.mode == Mode::Forge {
        chrome::side_panel(ui, "projects", 260.0, |ui| sidebar::show(app, ui));
    }
    chrome::content(ui, |ui| {
        let updater = app.updater.clone();
        if anvil_update::ui::banner(ui, &updater, &mut app.config.common) {
            app.save();
        }
        match (app.mode, app.view) {
            (Mode::Deck, _) if app.deck_view.page.is_some() => page::show(app, ui, &deck),
            (Mode::Deck, _) => deck::show(app, ui, &deck),
            (Mode::Forge, View::Project) => project::show(app, ui),
            (Mode::Forge, View::Overview) => overview::show(app, ui),
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
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1))) {
        app.set_mode(Mode::Deck);
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num2))) {
        app.set_mode(Mode::Forge);
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::L))) {
        app.log_open = !app.log_open;
    }
}

fn top_bar(app: &mut App, ui: &mut Ui) {
    chrome::top_bar(ui, |ui| {
        let info = info();
        chrome::brand(ui, info.icon, info.name);
        ui.add_space(18.0);
        let mut mode = app.mode;
        w::segmented(
            ui,
            &mut mode,
            &[(Mode::Deck, Some(Icon::Play), t("Пульт")), (Mode::Forge, Some(Icon::Hammer), t("Кузница"))],
        )
        .on_hover_text("Ctrl+1 · Ctrl+2");
        if mode != app.mode {
            app.set_mode(mode);
        }
        ui.add_space(10.0);
        palette::launcher(app, ui, 340.0);

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
                if w::icon_button(ui, Icon::Refresh, t("Перечитать всё · F5")).clicked() {
                    app.refresh();
                    app.fetch();
                }
            });
        });
    });
}

fn status_bar(app: &mut App, ui: &mut Ui, chips: &[(anvil_ui::Mark, String, String, String)]) {
    let p = Palette::of(ui);
    chrome::status_bar(ui, |ui| {
        if jobs::status(app, ui) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| right_side(app, ui, chips));
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
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| right_side(app, ui, chips));
    });
}

/// Правая часть строки состояния: что запущено и кнопка консоли.
fn right_side(app: &mut App, ui: &mut Ui, chips: &[(anvil_ui::Mark, String, String, String)]) {
    let p = Palette::of(ui);
    console_toggle(app, ui);
    if chips.is_empty() {
        return;
    }
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), egui::Stroke::new(1.0, p.border));
    ui.add_space(4.0);
    ui.spacing_mut().item_spacing.x = 6.0;
    for (mark, name, tail, key) in chips.iter().rev() {
        // Чип — страница того, что запущено: там журнал, остановка, профили.
        if w::running_chip(ui, mark.accent, mark.icon, name, tail).clicked() {
            app.set_mode(Mode::Deck);
            app.open_page(key.clone());
        }
    }
    ui.add_space(2.0);
    w::section_label(ui, t("Запущено"));
}

/// Консоль — задачи и их вывод. На Пульте закрыта, пока не понадобится.
fn console_toggle(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let hint = if app.log_open {
        t("Скрыть консоль · Ctrl+L")
    } else {
        t("Показать консоль · Ctrl+L")
    };
    let r = w::button(ui, Kind::Ghost, Some(Icon::Terminal), t("Консоль"));
    if app.log_open {
        // Открытая консоль — кнопка нажата.
        ui.painter().rect_filled(r.rect, anvil_ui::theme::radius::CONTROL, p.raised);
        let icon = egui::Rect::from_min_size(
            egui::pos2(r.rect.left() + 12.0, r.rect.center().y - 8.0),
            egui::Vec2::splat(16.0),
        );
        anvil_ui::icons::paint(ui.painter(), icon, Icon::Terminal, p.text);
        ui.painter().text(
            egui::pos2(icon.right() + 7.0, r.rect.center().y),
            egui::Align2::LEFT_CENTER,
            t("Консоль"),
            egui::FontId::proportional(14.0),
            p.text,
        );
    }
    // Для диктора — переключатель: слышно, открыта ли консоль.
    let open = app.log_open;
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, open, t("Консоль")));
    if r.on_hover_text(hint).clicked() {
        app.log_open = !app.log_open;
    }
}
