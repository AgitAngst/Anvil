//! Профили запуска (в 0.2 — «пресеты»): какой бинарник, откуда и с какими аргументами запускать.

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette};
use eframe::egui;

use crate::app::App;
use crate::config::{Preset, Source};
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
    let title = format!("{} · {}", t("Профили запуска"), worker::display_name(&path));
    chrome::dialog(ctx, "anvil-presets", &title, 720.0, &mut open, |ui| {
        let p = Palette::of(ui);
        w::note(
            ui,
            t(
                "Профиль — это бинарник, откуда его брать и с какими аргументами: например, клиент с тестовым профилем из кода. «Обычный» профиль есть всегда: установленная копия без аргументов.",
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
                            egui::TextEdit::singleline(&mut preset.name).hint_text(t("Название")).desired_width(120.0),
                        );
                        let bin = w::button(ui, Kind::Secondary, Some(Icon::Terminal), &preset.bin);
                        w::menu(&bin, 220.0, |ui| {
                            for name in &bins {
                                if w::menu_item(ui, None, name, None).clicked() {
                                    preset.bin = name.clone();
                                }
                            }
                        });
                        w::segmented(
                            ui,
                            &mut preset.source,
                            &[(Source::Code, None, t("из кода")), (Source::Installed, None, t("установленная"))],
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if w::icon_button(ui, Icon::Trash, t("Удалить профиль")).clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut preset.args)
                                .hint_text(t("аргументы, например --profile test"))
                                .font(egui::TextStyle::Monospace)
                                .desired_width(ui.available_width() - 170.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut preset.ready)
                                .hint_text(t("готов: port:18731"))
                                .font(egui::TextStyle::Monospace)
                                .desired_width(160.0),
                        )
                        .on_hover_text(t("Когда считать запущенным: port:18731 — порт слушает, window — окно появилось, 5s — через 5 с. Пусто — сразу."));
                    });
                });
            ui.add_space(6.0);
        }
        if let Some(i) = remove {
            presets.remove(i);
        }
        ui.add_enabled_ui(!bins.is_empty(), |ui| {
            if w::button(ui, Kind::Secondary, Some(Icon::Plus), t("Добавить профиль")).clicked() {
                let n = presets.len() + 1;
                presets.push(Preset {
                    name: format!("{} {n}", t("профиль")),
                    bin: bins.first().cloned().unwrap_or_default(),
                    ..Preset::default()
                });
            }
        });
    });
    // Правки видны сразу, а на диск ложатся, когда окно закрыли: не на каждую нажатую клавишу.
    if presets != before {
        follow_renames(app, &path, &before, &presets);
        app.config.project_mut(&path).presets = presets;
        app.deck_view.order.clear();
        app.presets_dirty = true;
    }
    if !open {
        app.presets_for = None;
        if std::mem::take(&mut app.presets_dirty) {
            app.save();
        }
    }
}

/// Выбор профиля хранится по имени: переименовали — выбор идёт следом, удалили — выбор сбрасывается
/// на «обычный». Иначе Enter молча запускал бы другое.
fn follow_renames(app: &mut App, path: &std::path::Path, before: &[Preset], after: &[Preset]) {
    let mut renamed: Vec<(String, String, String)> = Vec::new();
    if before.len() == after.len() {
        for (old, new) in before.iter().zip(after) {
            if old.bin == new.bin && old.name != new.name {
                renamed.push((old.bin.clone(), old.name.clone(), new.name.clone()));
            }
        }
    }
    for (bin, old, new) in renamed {
        let key = crate::deck::key(path, &bin);
        if app.config.deck.profile.get(&key) == Some(&old) {
            app.config.deck.profile.insert(key, new.clone());
        }
        let settings = app.config.project_mut(path);
        if settings.run.as_deref() == Some(old.as_str()) {
            settings.run = Some(new);
        }
    }
    // Выбранного профиля больше нет — выбор снимается.
    let keys: Vec<String> = app
        .config
        .deck
        .profile
        .iter()
        .filter(|(key, name)| {
            key.starts_with(&format!("{}|", path.to_string_lossy().to_lowercase()))
                && !after.iter().any(|p| &p.name == *name && key.ends_with(&format!("|{}", p.bin)))
        })
        .map(|(key, _)| key.clone())
        .collect();
    for key in keys {
        app.config.deck.profile.remove(&key);
    }
}
