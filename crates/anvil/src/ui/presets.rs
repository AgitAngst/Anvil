//! Пресеты запуска: какой бинарник и с какими аргументами запускать.

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette};
use eframe::egui;

use crate::app::App;
use crate::config::Preset;
use crate::i18n::t;
use crate::worker;

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(path) = app.presets_for.clone() else { return };
    let bins: Vec<String> = app
        .projects
        .iter()
        .find(|p| p.path == path)
        .and_then(|p| p.meta())
        .map(|m| m.bins.iter().map(|b| b.name.clone()).collect())
        .unwrap_or_default();
    let mut presets = app.config.project(&path).presets;
    let before = presets.clone();
    let mut open = true;
    let title = format!("{} · {}", t("Пресеты запуска"), worker::display_name(&path));
    chrome::dialog(ctx, "anvil-presets", &title, 640.0, &mut open, |ui| {
        let p = Palette::of(ui);
        w::note(
            ui,
            t(
                "Пресет — это бинарник и аргументы: например, клиент с тестовым профилем. Главная кнопка «Запустить» берёт выбранный пресет.",
            ),
        );
        ui.add_space(8.0);
        let mut remove = None;
        for (i, preset) in presets.iter_mut().enumerate() {
            egui::Frame::new()
                .fill(p.raised)
                .corner_radius(anvil_ui::theme::radius::CONTROL)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut preset.name).hint_text(t("Название")).desired_width(130.0),
                        );
                        let bin = w::button(ui, Kind::Secondary, Some(Icon::Terminal), &preset.bin);
                        w::menu(&bin, 220.0, |ui| {
                            for name in &bins {
                                if w::menu_item(ui, None, name, None).clicked() {
                                    preset.bin = name.clone();
                                }
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if w::icon_button(ui, Icon::Trash, t("Удалить пресет")).clicked() {
                                remove = Some(i);
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut preset.args)
                                    .hint_text(t("аргументы, например --profile test"))
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(ui.available_width()),
                            );
                        });
                    });
                });
            ui.add_space(6.0);
        }
        if let Some(i) = remove {
            presets.remove(i);
        }
        ui.add_enabled_ui(!bins.is_empty(), |ui| {
            if w::button(ui, Kind::Secondary, Some(Icon::Plus), t("Добавить пресет")).clicked() {
                let n = presets.len() + 1;
                presets.push(Preset {
                    name: format!("{} {n}", t("Пресет")),
                    bin: bins.first().cloned().unwrap_or_default(),
                    args: String::new(),
                });
            }
        });
    });
    if presets != before {
        app.config.project_mut(&path).presets = presets;
        app.save();
    }
    if !open {
        app.presets_for = None;
    }
}
