//! «Ход работы»: планы долгих задач с ходом и сессии Claude.

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::i18n::{self, t};
use crate::work::{Plan, Session, StageState};

/// Законченный план показываем ещё столько после последней правки.
const FINISHED_FOR_SECS: f64 = 24.0 * 3600.0;

fn tone(state: StageState) -> Tone {
    match state {
        StageState::Done => Tone::Success,
        StageState::Running => Tone::Accent,
        StageState::Failed => Tone::Danger,
        StageState::Pending | StageState::Skipped => Tone::Neutral,
    }
}

fn minutes(value: f64) -> String {
    let total = value.round().max(0.0) as i64;
    if total >= 60 {
        format!("{} {} {} {}", total / 60, t("ч"), total % 60, t("мин"))
    } else {
        format!("{total} {}", t("мин"))
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let now = i18n::now() as f64;
    let english = i18n::is_english();
    let mut refresh = false;
    ui.horizontal(|ui| {
        w::title(ui, t("Ход работы"), 22.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::icon_button(ui, Icon::Refresh, t("Перечитать")).clicked() {
                refresh = true;
            }
            if let Some(snapshot) = &app.work {
                w::note(ui, format!("{} {}", t("Прочитано"), i18n::ago(snapshot.read)));
            }
        });
    });
    w::note(ui, t("Планы пишет скилл progress в папку .claude/progress, сессии берутся из .claude/projects."));
    ui.add_space(12.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let Some(snapshot) = &app.work else {
            w::note(ui, t("Читаю…"));
            return;
        };
        let plans: Vec<&Plan> =
            snapshot.plans.iter().filter(|plan| !plan.finished || now - plan.updated <= FINISHED_FOR_SECS).collect();
        w::section_label(ui, t("Задачи"));
        ui.add_space(4.0);
        if plans.is_empty() {
            w::empty_state(
                ui,
                Icon::Clock,
                t("Планов нет"),
                t("Долгая задача с планом появится здесь, пока идёт, и ещё сутки после конца."),
            );
        }
        for plan in plans {
            w::card(ui, |ui| plan_card(ui, plan, now, english));
            ui.add_space(8.0);
        }
        ui.add_space(10.0);
        w::section_label(ui, t("Сессии Claude за сутки"));
        ui.add_space(4.0);
        if snapshot.sessions.is_empty() {
            w::note(ui, t("Сессий за последние сутки нет."));
        }
        for session in &snapshot.sessions {
            session_row(ui, session, &p);
        }
    });
    if refresh {
        app.refresh_work();
    }
}

fn plan_card(ui: &mut Ui, plan: &Plan, now: f64, english: bool) {
    let p = Palette::of(ui);
    ui.set_width(ui.available_width());
    ui.horizontal(|ui| {
        ui.label(RichText::new(plan.title(english)).size(15.0).color(p.text));
        let (label, badge_tone) = if plan.failed() {
            (t("упала"), Tone::Danger)
        } else if plan.finished {
            (t("готово"), Tone::Success)
        } else {
            (t("идёт"), Tone::Accent)
        };
        w::badge(ui, label, badge_tone);
        w::note(ui, &plan.project);
    });
    ui.add_space(6.0);
    let fraction = if plan.finished { 1.0 } else { plan.fraction(now) };
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 190.0).max(120.0);
        w::progress(ui, Some(fraction), width);
        w::note(ui, format!("{:.0} %", fraction * 100.0));
        if !plan.finished {
            let left = plan.remaining_minutes(now);
            if left > 0.0 {
                w::note(ui, format!("· {} {}", t("осталось ≈"), minutes(left)));
            }
        }
    });
    ui.add_space(6.0);
    for stage in &plan.stages {
        ui.horizontal(|ui| {
            w::dot(ui, tone(stage.state));
            let name = if english && !stage.en.is_empty() { &stage.en } else { &stage.ru };
            let color = if stage.state == StageState::Skipped { p.faint } else { p.text };
            ui.label(RichText::new(name).size(13.0).color(color));
            if stage.est > 0.0 {
                w::note(ui, format!("≈ {}", minutes(stage.est)));
            }
        });
        if let Some(note) = &stage.note {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                ui.add(egui::Label::new(RichText::new(note).size(12.0).color(p.weak)).truncate());
            });
        }
    }
}

fn session_row(ui: &mut Ui, session: &Session, p: &Palette) {
    ui.horizontal(|ui| {
        w::dot(ui, if session.active { Tone::Accent } else { Tone::Neutral });
        ui.label(RichText::new(&session.project).size(13.0).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            w::note(ui, i18n::ago(session.modified));
            if session.active {
                w::badge(ui, t("работает"), Tone::Accent);
            }
            let title = if session.title.is_empty() { t("без названия") } else { session.title.as_str() };
            ui.add(egui::Label::new(RichText::new(title).size(12.5).color(p.weak)).truncate());
        });
    });
    ui.add_space(2.0);
}
