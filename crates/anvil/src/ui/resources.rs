//! «Ресурсы и бюджет»: диски и их SMART, размер сборок, минуты GitHub Actions.

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::i18n::{self, t};
use crate::resources::{Build, Disk, Smart};
use crate::worker::display_name;

enum Action {
    Refresh,
    Clean(PathBuf),
}

/// `12,3 ГБ`, `640 МБ`.
pub fn bytes_text(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB { format!("{:.1} {}", b / GB, t("ГБ")) } else { format!("{:.0} {}", b / MB, t("МБ")) }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let mut action = None;
    ui.horizontal(|ui| {
        w::title(ui, t("Ресурсы и бюджет"), 22.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!app.resources_busy(), |ui| {
                if w::icon_button(ui, Icon::Refresh, t("Перечитать")).clicked() {
                    action = Some(Action::Refresh);
                }
            });
            if app.resources_busy() {
                w::spinner(ui, 14.0);
            }
            if let Some(snapshot) = &app.resources {
                w::note(ui, format!("{} {}", t("Прочитано"), i18n::ago(snapshot.read)));
            }
        });
    });
    ui.add_space(10.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let Some(snapshot) = &app.resources else {
            w::note(ui, t("Читаю…"));
            return;
        };
        w::section_label(ui, t("Диски"));
        ui.add_space(4.0);
        if snapshot.disks.is_empty() {
            w::note(ui, t("Диски не определились."));
        }
        for disk in &snapshot.disks {
            disk_row(ui, disk, &p);
        }
        ui.add_space(12.0);
        w::section_label(ui, t("Износ дисков (SMART)"));
        ui.add_space(4.0);
        match &snapshot.smart {
            Ok(list) if !list.is_empty() => {
                for smart in list {
                    smart_row(ui, smart, &p);
                }
                if list.iter().all(|s| s.wear.is_none() && s.temp.is_none()) {
                    w::note(ui, t("Износ и температуру Windows отдаёт не всем дискам и не без прав администратора."));
                }
            }
            _ => {
                w::note(ui, t("Windows не отдала сведения о дисках (для части дисков нужны права администратора)."));
            }
        }
        ui.add_space(12.0);
        w::section_label(ui, t("Сборки (target)"));
        ui.add_space(4.0);
        if snapshot.builds.is_empty() {
            w::note(ui, t("Проектов на Rust не найдено."));
        }
        let total: u64 = snapshot.builds.iter().map(|b| b.bytes).sum();
        if total > 0 {
            w::note(ui, format!("{} {}", t("Всего:"), bytes_text(total)));
        }
        for build in &snapshot.builds {
            build_row(ui, build, &p, &mut action);
        }
        ui.add_space(12.0);
        minutes(app, ui, &p);
    });
    match action {
        Some(Action::Refresh) => app.refresh_resources(),
        Some(Action::Clean(path)) => app.clean_confirm = Some(path),
        None => {}
    }
}

fn disk_row(ui: &mut Ui, disk: &Disk, p: &Palette) {
    let used = disk.total.saturating_sub(disk.free);
    let fraction = used as f32 / disk.total.max(1) as f32;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{}:", disk.letter)).font(anvil_ui::semibold(13.5)).color(p.text));
        w::progress(ui, Some(fraction), 260.0);
        w::note(
            ui,
            format!(
                "{} {} {}, {} {}",
                bytes_text(disk.free),
                t("свободно из"),
                bytes_text(disk.total),
                (fraction * 100.0).round(),
                "%"
            ),
        );
    });
}

fn smart_row(ui: &mut Ui, smart: &Smart, p: &Palette) {
    ui.horizontal(|ui| {
        let healthy = smart.health.eq_ignore_ascii_case("healthy");
        w::dot(ui, if healthy { Tone::Success } else { Tone::Warning });
        ui.label(RichText::new(&smart.name).size(13.5).color(p.text));
        if !smart.media.is_empty() {
            w::note(ui, &smart.media);
        }
        if let Some(size) = smart.size {
            w::note(ui, bytes_text(size));
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        let mut parts = Vec::new();
        if smart.health.eq_ignore_ascii_case("healthy") {
            parts.push(t("исправен").to_owned());
        } else if !smart.health.is_empty() {
            parts.push(smart.health.clone());
        }
        if let Some(wear) = smart.wear {
            parts.push(format!("{} {wear} %", t("износ")));
        }
        if let Some(temp) = smart.temp {
            parts.push(format!("{temp} °C"));
        }
        if let Some(hours) = smart.hours {
            parts.push(format!("{hours} {}", t("ч работы")));
        }
        if let Some(errors) = smart.errors {
            parts.push(format!("{errors} {}", t("ошибок чтения")));
        }
        if parts.is_empty() {
            w::note(ui, t("нет данных"));
        } else {
            w::note(ui, parts.join(" · "));
        }
    });
    ui.add_space(4.0);
}

fn build_row(ui: &mut Ui, build: &Build, p: &Palette, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(display_name(&build.project)).size(13.5).color(p.text));
        w::note(ui, bytes_text(build.bytes));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::button(ui, Kind::Ghost, Some(Icon::Trash), t("Очистить")).clicked() {
                *action = Some(Action::Clean(build.project.clone()));
            }
            ui.add(
                egui::Label::new(RichText::new(build.target.to_string_lossy()).size(12.0).color(p.faint)).truncate(),
            );
        });
    });
}

/// Минуты Actions закрытых репозиториев за месяц — оценка по длительности прогонов.
fn minutes(app: &mut App, ui: &mut Ui, p: &Palette) {
    w::section_label(ui, t("Минуты GitHub Actions в этом месяце"));
    ui.add_space(4.0);
    let mut total = 0.0;
    let mut rows = Vec::new();
    for project in app.visible() {
        if crate::sources::is_private(&app.config, &project.path, app.remotes.get(&project.path)) != Some(true) {
            continue;
        }
        match app.minutes.get(&project.path) {
            Some(Ok(usage)) => {
                total += usage.minutes;
                rows.push((project.name(), Some(*usage), None));
            }
            Some(Err(e)) => rows.push((project.name(), None, Some(e.clone()))),
            None => rows.push((project.name(), None, None)),
        }
    }
    if rows.is_empty() {
        w::note(ui, t("Закрытых репозиториев с GitHub не найдено: у публичных минуты бесплатные."));
        return;
    }
    let mut quota = app.config.actions_quota;
    let fraction = if quota == 0 { 0.0 } else { (total / quota as f64) as f32 };
    ui.horizontal(|ui| {
        w::progress(ui, Some(fraction), 260.0);
        w::note(ui, format!("≈ {:.0} {} ", total, t("мин из")));
        ui.add(egui::DragValue::new(&mut quota).range(0..=100_000).speed(10));
    });
    if quota != app.config.actions_quota {
        app.config.actions_quota = quota;
        app.save();
    }
    w::note(
        ui,
        t(
            "Оценка по длительности прогонов, округлённой вверх до минуты; Windows у GitHub считается за две, здесь — за одну.",
        ),
    );
    ui.add_space(4.0);
    for (name, usage, error) in rows {
        ui.horizontal(|ui| {
            ui.label(RichText::new(name).size(13.5).color(p.text));
            match (usage, error) {
                (Some(u), _) => {
                    let more = if u.capped { "≥ " } else { "" };
                    w::note(ui, format!("{more}{:.0} {} · {} {}", u.minutes, t("мин"), u.runs, t("прогонов")));
                }
                (None, Some(e)) => {
                    w::note(ui, e);
                }
                (None, None) => {
                    w::note(ui, t("Читаю…"));
                }
            }
        });
    }
}
