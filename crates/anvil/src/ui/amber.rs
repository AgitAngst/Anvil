//! Строка серверов Amber в карточке проекта, где есть amber-admin: одна строка по сводке,
//! подробности — в самом amber-admin.

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone, semibold};
use eframe::egui::{self, RichText, Ui};

use crate::amber::{self, Server};
use crate::app::App;
use crate::i18n::{self, t};
use crate::worker::Project;

fn has_admin(project: &Project) -> bool {
    project.meta().is_some_and(|m| m.bins.iter().any(|b| b.name == amber::ADMIN))
}

/// Нарисовать строку, если у проекта есть amber-admin. `true` — попросили открыть amber-admin.
pub fn line(app: &mut App, ui: &mut Ui, project: &Project) -> bool {
    if !has_admin(project) {
        return false;
    }
    let summary = app.amber.get().cloned();
    let installed = app.installs.get(amber::ADMIN).is_some_and(Option::is_some);
    let p = Palette::of(ui);
    let mut open = false;
    w::card_frame(ui).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
            anvil_ui::icons::paint(ui.painter(), rect, Icon::Server, p.weak);
            ui.label(RichText::new(t("Серверы Amber")).font(semibold(13.5)).color(p.text));
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let hint = if installed {
                    t("Запустить установленный amber-admin")
                } else {
                    t("Собрать и запустить amber-admin из проекта")
                };
                if w::button(ui, Kind::Secondary, None, t("Открыть amber-admin")).on_hover_text(hint).clicked() {
                    open = true;
                }
                if let Some(Ok(summary)) = &summary {
                    ui.add_space(6.0);
                    w::note(ui, format!("{} {}", t("проверено"), i18n::ago(summary.checked_at)));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| match &summary {
                    None => {
                        w::note(ui, t("сводки нет — проверьте серверы в amber-admin"));
                    }
                    Some(Err(e)) => {
                        w::note(ui, t("сводка не читается")).on_hover_text(e);
                    }
                    Some(Ok(summary)) if summary.servers.is_empty() => {
                        w::note(ui, t("серверов в amber-admin нет"));
                    }
                    Some(Ok(summary)) => {
                        for (i, server) in summary.servers.iter().enumerate() {
                            if i > 0 {
                                ui.add_space(10.0);
                            }
                            server_chip(ui, server);
                        }
                    }
                });
            });
        });
    });
    ui.add_space(14.0);
    open
}

/// Серверы из сводки — у проекта с amber-admin; у остальных пусто.
pub fn servers(app: &mut App, project: &Project) -> Vec<Server> {
    match app.amber.get() {
        Some(Ok(summary)) if has_admin(project) => summary.servers.clone(),
        _ => Vec::new(),
    }
}

/// Для обзора: точки серверов в карточке проекта.
pub fn dots(ui: &mut Ui, servers: &[Server]) {
    if servers.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        w::note(ui, t("Серверы Amber"));
        for server in servers {
            let (tone, state) = health(&server.health);
            w::dot(ui, tone).on_hover_text(format!("{}: {state}", server.name));
        }
    });
}

/// «● home 0.3.0 · 2/5»: состояние, имя, версия, сколько участников в сети.
fn server_chip(ui: &mut Ui, server: &Server) {
    let p = Palette::of(ui);
    let (tone, state) = health(&server.health);
    let mut hint = format!("{}: {state}", server.name);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        w::dot(ui, tone);
        ui.label(RichText::new(&server.name).size(13.5).color(p.text));
        if let Some(version) = &server.version {
            w::mono(ui, version, None);
        }
        if let (Some(online), Some(members)) = (server.online, server.members) {
            w::note(ui, format!("{online}/{members}"));
            hint.push_str(&format!("\n{}: {online}/{members}", t("в сети")));
        }
    })
    .response
    .on_hover_text(hint);
}

fn health(code: &str) -> (Tone, &'static str) {
    match code {
        "up" => (Tone::Success, t("работает")),
        "down" => (Tone::Danger, t("служба остановлена")),
        "unreachable" => (Tone::Danger, t("недоступен")),
        "bare" => (Tone::Neutral, t("Amber не стоит")),
        _ => (Tone::Neutral, t("не проверен")),
    }
}
