//! Обзор (`Ctrl+0`): все проекты сеткой карточек — взгляд «одним глазом». Щелчок — к проекту.

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Palette, Tone, semibold};
use eframe::egui::{self, RichText, Sense, Ui};

use crate::app::{App, View};
use crate::i18n::{self, t};
use crate::registry::Kind as ProjectKind;
use crate::worker::Project;

/// Ширина, от которой в ряд встаёт ещё одна карточка.
const CARD_WIDTH: f32 = 330.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        w::title(ui, t("Обзор"), 22.0);
        ui.add_space(8.0);
        w::note(ui, i18n::count(app.visible().len(), ["проект", "проекта", "проектов"], ["project", "projects"]));
    });
    ui.add_space(12.0);
    let projects: Vec<Project> = app.visible().into_iter().cloned().collect();
    if projects.is_empty() {
        w::empty_state(ui, Icon::Tiles, t("Проектов не видно"), t("Добавьте папку с проектами в настройках."));
        return;
    }
    let columns = ((ui.available_width() / CARD_WIDTH).floor() as usize).clamp(1, 4);
    let mut open: Option<PathBuf> = None;
    for row in projects.chunks(columns) {
        ui.columns(columns, |cols| {
            for (col, project) in cols.iter_mut().zip(row) {
                if card(app, col, project) {
                    open = Some(project.path.clone());
                }
            }
        });
        ui.add_space(12.0);
    }
    if let Some(path) = open {
        app.select(path);
        app.view = View::Project;
    }
}

/// Карточка проекта. `true` — по ней щёлкнули.
fn card(app: &mut App, ui: &mut Ui, project: &Project) -> bool {
    let p = Palette::of(ui);
    let servers = super::amber::servers(app, project);
    let remote = app.remotes.get(&project.path);
    let report = app.deps.get(&project.path);
    let (tone, trailing) = super::sidebar::status(project, remote, &app.installs, report);
    let inner = w::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(150.0);
        ui.horizontal(|ui| {
            w::dot(ui, tone);
            ui.label(RichText::new(project.name()).font(semibold(16.0)).color(p.text));
            if let Some(version) = project.meta().and_then(|m| m.version.as_deref()) {
                w::mono(ui, &format!("v{version}"), None);
            }
            if project.kind != ProjectKind::Rust {
                w::badge(ui, project.kind.label(), Tone::Accent);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some((text, tone)) = &trailing {
                    w::badge(ui, text, *tone);
                }
            });
        });
        ui.add_space(6.0);
        match project.git() {
            Some(git) => {
                ui.horizontal(|ui| {
                    w::mono(ui, git.branch.as_deref().unwrap_or("HEAD"), Some(p.text));
                    w::note(ui, "·");
                    if git.dirty() {
                        let n = git.changes.len();
                        w::note(ui, i18n::count(n, ["изменение", "изменения", "изменений"], ["change", "changes"]));
                    } else {
                        w::note(ui, t("чисто"));
                    }
                    if git.upstream.is_some() {
                        w::note(ui, "·");
                        w::note(ui, format!("↑{} ↓{}", git.ahead, git.behind));
                    }
                    super::github::ci_badge(ui, remote);
                });
                if let Some(commit) = git.commits.first() {
                    ui.horizontal(|ui| {
                        w::mono(ui, &commit.hash, Some(p.accent_text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            w::note(ui, i18n::ago(commit.time));
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let label = egui::Label::new(RichText::new(&commit.subject).size(13.0).color(p.weak));
                                ui.add(label.truncate());
                            });
                        });
                    });
                }
            }
            None => {
                w::note(ui, t("Папка не под git."));
            }
        }
        // Запущенные программы проекта.
        let running: Vec<String> = project
            .meta()
            .into_iter()
            .flat_map(|m| m.bins.iter())
            .filter(|b| !app.running(&b.name).is_empty())
            .map(|b| b.name.clone())
            .collect();
        if !running.is_empty() {
            ui.horizontal(|ui| {
                w::dot(ui, Tone::Success);
                ui.label(RichText::new(format!("{} {}", t("запущен"), running.join(", "))).size(13.0).color(p.success));
            });
        }
        if let Some(report) = report {
            let vulnerable = report.vulnerabilities();
            let direct = report.updates.iter().filter(|c| c.direct).count();
            let majors = report.majors().count();
            if vulnerable + direct + majors > 0 {
                ui.horizontal(|ui| {
                    if vulnerable > 0 {
                        w::badge(ui, &format!("RustSec {vulnerable}"), Tone::Danger);
                    }
                    if direct + majors > 0 {
                        let text = i18n::count(
                            direct + majors,
                            ["обновление", "обновления", "обновлений"],
                            ["update", "updates"],
                        );
                        w::badge(ui, &text, Tone::Accent);
                    }
                });
            }
        }
        super::amber::dots(ui, &servers);
    });
    let response = ui.interact(inner.response.rect, ui.id().with(("overview", &project.path)), Sense::click());
    if response.hovered() {
        ui.painter().rect_stroke(inner.response.rect, 10, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, project.name()));
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}
