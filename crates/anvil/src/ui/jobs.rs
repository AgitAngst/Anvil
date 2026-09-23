//! Задачи в окне: ход в строке состояния и панель лога с ошибками.

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone, semibold};
use eframe::egui::{self, FontId, RichText, Ui};

use crate::app::{self, App};
use crate::i18n::{self, t};
use crate::jobs::{Level, LineKind};
use crate::tasks::Job;
use crate::worker;

/// Цвет и подпись состояния задачи.
fn state(job: &Job) -> (Tone, String) {
    match &job.finished {
        None if job.queued() => (Tone::Neutral, t("в очереди").to_owned()),
        None => (Tone::Accent, t("идёт").to_owned()),
        Some((outcome, _)) if outcome.cancelled => (Tone::Neutral, t("отменена").to_owned()),
        Some((outcome, _)) if outcome.ok => (Tone::Success, t("готово").to_owned()),
        Some(_) if job.errors() > 0 => {
            (Tone::Danger, i18n::count(job.errors(), ["ошибка", "ошибки", "ошибок"], ["error", "errors"]))
        }
        Some(_) => (Tone::Danger, t("не удалось").to_owned()),
    }
}

/// Левая часть строки состояния: идущая задача или итог последней. `false` — задач ещё не было.
pub fn status(app: &mut App, ui: &mut Ui) -> bool {
    let p = Palette::of(ui);
    let running = app.jobs.iter().find(|j| j.running()).map(|j| j.id);
    let shown =
        running.or_else(|| app.jobs.iter().rev().find(|j| j.finished.is_some() && j.started.is_some()).map(|j| j.id));
    let Some(id) = shown else { return false };
    let queued = app.jobs.iter().filter(|j| j.queued()).count();
    let Some(job) = app.jobs.iter().find(|j| j.id == id) else { return false };

    let name = worker::display_name(&job.project);
    if job.running() {
        w::spinner(ui, 14.0);
        ui.label(RichText::new(&name).font(semibold(13.0)).color(p.text));
        w::mono(ui, &job.spec.title, Some(p.weak));
        ui.add_space(6.0);
        w::progress(ui, job.progress(), 160.0);
        let units = match job.spec.expected_units {
            Some(total) => format!("{}/{total}", job.units.min(total)),
            None if job.units > 0 => job.units.to_string(),
            None => String::new(),
        };
        if !units.is_empty() {
            w::mono(ui, &units, None);
        }
        w::mono(ui, &app::duration(job.elapsed()), None);
        let id = job.id;
        if w::icon_button(ui, Icon::Stop, t("Отменить задачу")).clicked() {
            app.cancel(id);
        }
    } else {
        let (tone, text) = state(job);
        w::dot(ui, tone);
        let summary = format!("{name} · {} — {text}, {}", job.spec.title, app::duration(job.elapsed()));
        let r = ui.add(egui::Label::new(RichText::new(summary).size(13.0).color(p.weak)).sense(egui::Sense::click()));
        if r.on_hover_text(t("Открыть лог")).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            app.log_open = true;
            app.log_job = Some(id);
        }
    }
    if queued > 0 {
        w::badge(ui, &format!("{} {queued}", t("ещё")), Tone::Neutral);
    }
    true
}

/// Панель лога над строкой состояния.
pub fn panel(app: &mut App, ui: &mut Ui) {
    if !app.log_open {
        return;
    }
    let p = Palette::of(ui);
    egui::Panel::bottom("anvil-log")
        .resizable(true)
        .default_size(320.0)
        .size_range(160.0..=700.0)
        .frame(egui::Frame::new().fill(p.surface).stroke(egui::Stroke::new(1.0, p.border)))
        .show(ui, |ui| {
            egui::Panel::left("anvil-log-jobs")
                .resizable(false)
                .exact_size(290.0)
                .frame(egui::Frame::new().fill(p.surface).inner_margin(egui::Margin::symmetric(8, 10)))
                .show(ui, |ui| job_list(app, ui));
            egui::CentralPanel::no_frame()
                .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin::symmetric(16, 10)))
                .show(ui, |ui| job_view(app, ui));
        });
}

fn job_list(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        w::section_label(ui, t("Задачи"));
    });
    ui.add_space(4.0);
    if app.jobs.is_empty() {
        w::note(ui, t("Здесь появятся сборки, тесты и запуски."));
        return;
    }
    let mut pick = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for job in app.jobs.iter().rev() {
            let selected = app.log_job == Some(job.id);
            let (tone, _) = state(job);
            let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
            if selected {
                ui.painter().rect_filled(rect, 6, p.soft(p.accent));
            } else if r.hovered() {
                ui.painter().rect_filled(rect, 6, p.hover);
            }
            ui.painter().circle_filled(egui::pos2(rect.left() + 12.0, rect.top() + 14.0), 4.0, tone.color(&p));
            let x = rect.left() + 24.0;
            ui.painter().text(
                egui::pos2(x, rect.top() + 14.0),
                egui::Align2::LEFT_CENTER,
                worker::display_name(&job.project),
                semibold(13.0),
                p.text,
            );
            ui.painter().text(
                egui::pos2(rect.right() - 8.0, rect.top() + 14.0),
                egui::Align2::RIGHT_CENTER,
                app::duration(job.elapsed()),
                FontId::proportional(12.0),
                p.weak,
            );
            let title = job.spec.title.trim_start_matches("cargo ");
            let galley = ui.painter().layout(title.to_owned(), FontId::monospace(11.5), p.weak, rect.width() - 32.0);
            let row = galley.rows.first().map(|r| r.text()).unwrap_or_default();
            ui.painter().text(
                egui::pos2(x, rect.top() + 32.0),
                egui::Align2::LEFT_CENTER,
                row,
                FontId::monospace(11.5),
                p.weak,
            );
            if r.on_hover_text(&job.spec.title).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                pick = Some(job.id);
            }
        }
    });
    if pick.is_some() {
        app.log_job = pick;
    }
}

fn job_view(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let Some(job) = app.log_job.and_then(|id| app.jobs.iter().find(|j| j.id == id)) else {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::icon_button(ui, Icon::Close, t("Закрыть лог")).clicked() {
                    app.log_open = false;
                }
            });
        });
        return;
    };
    let id = job.id;
    let (tone, text) = state(job);
    let can_cancel = job.finished.is_none();
    let (errors, warnings) = (job.errors(), job.warnings());
    let mut close = false;
    let mut cancel = false;
    let mut copy = None;
    ui.horizontal(|ui| {
        w::mono(ui, &job.spec.title, Some(p.text));
        w::badge(ui, &text, tone);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = w::icon_button(ui, Icon::Close, t("Закрыть лог")).clicked();
            if w::button(ui, Kind::Ghost, Some(Icon::File), t("Копировать лог")).clicked() {
                copy = Some(job.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"));
            }
            if can_cancel {
                cancel = w::button(ui, Kind::Danger, Some(Icon::Stop), t("Отменить")).clicked();
            }
        });
    });
    ui.add_space(4.0);

    let tab_id = ui.id().with("log-tab");
    let mut tab: usize = ui.data(|d| d.get_temp(tab_id)).unwrap_or(0);
    let problems = if errors + warnings > 0 {
        format!("{} · {}", t("Ошибки и предупреждения"), errors + warnings)
    } else {
        t("Ошибки и предупреждения").to_owned()
    };
    w::tabs(ui, &mut tab, &[t("Вывод"), &problems]);
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.add_space(6.0);

    let mut open_at = None;
    if tab == 0 {
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
        egui::ScrollArea::both().auto_shrink([false, false]).stick_to_bottom(true).show_rows(
            ui,
            row_height,
            job.lines.len(),
            |ui, rows| {
                for line in &job.lines[rows] {
                    let color = match line.kind {
                        LineKind::Text => p.text,
                        LineKind::Error => p.danger,
                        LineKind::Warning => p.warning,
                        LineKind::Note => p.accent_text,
                    };
                    ui.add(
                        egui::Label::new(RichText::new(&line.text).font(FontId::monospace(12.5)).color(color)).extend(),
                    );
                }
            },
        );
    } else if job.diags.is_empty() {
        w::note(ui, t("Компилятор ни о чём не предупредил."));
    } else {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Сначала ошибки, потом предупреждения.
            let mut diags: Vec<_> = job.diags.iter().collect();
            diags.sort_by_key(|d| d.level != Level::Error);
            for diag in diags {
                ui.horizontal(|ui| {
                    let (label, tone) = match diag.level {
                        Level::Error => (t("ошибка"), Tone::Danger),
                        Level::Warning => (t("предупреждение"), Tone::Warning),
                    };
                    w::badge(ui, label, tone);
                    if let Some((file, line, col)) = &diag.place {
                        let place = format!("{}:{line}:{col}", file.display());
                        let r = ui.add(
                            egui::Label::new(RichText::new(&place).font(FontId::monospace(12.5)).color(p.accent_text))
                                .sense(egui::Sense::click()),
                        );
                        if r.on_hover_text(t("Открыть в VS Code"))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            open_at = Some((job.project.join(file), *line, *col, job.project.clone()));
                        }
                    }
                    ui.label(&diag.message);
                });
            }
        });
    }

    if let Some(text) = copy {
        ui.ctx().copy_text(text);
        app.toasts.push(t("Лог скопирован"), Tone::Neutral);
    }
    if cancel {
        app.cancel(id);
    }
    if close {
        app.log_open = false;
    }
    if let Some((file, line, col, dir)) = open_at {
        app.report(crate::open::code_at(&file, line, col, &dir));
    }
}
