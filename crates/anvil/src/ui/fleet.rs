//! «Парк серверов»: записи из `anvil.toml`, их состояние, обновление и откат командами; серверы Amber — из сводки amber-admin.

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::fleet::{Entry, Group, State};
use crate::i18n::{self, t};

/// Что запустить после подтверждения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    Update,
    Rollback,
}

#[derive(Default)]
pub struct Form {
    pub entry: Entry,
}

enum Action {
    Refresh,
    Add,
    Remove(String),
    Ask(String, What),
}

fn group_label(group: Group) -> &'static str {
    match group {
        Group::Server => t("Серверы"),
        Group::Bot => t("Боты"),
        Group::Service => t("Службы"),
    }
}

fn state_text(state: State) -> (Tone, &'static str) {
    match state {
        State::Up => (Tone::Success, t("жив")),
        State::Down => (Tone::Danger, t("не отвечает")),
        State::Unknown => (Tone::Neutral, t("не проверяется")),
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let mut action = None;
    ui.horizontal(|ui| {
        w::title(ui, t("Парк серверов"), 22.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::icon_button(ui, Icon::Refresh, t("Проверить сейчас")).clicked() {
                action = Some(Action::Refresh);
            }
            if w::button(ui, Kind::Secondary, Some(Icon::Plus), t("Добавить…")).clicked() {
                action = Some(Action::Add);
            }
            if app.fleet_busy() {
                w::spinner(ui, 14.0);
            }
        });
    });
    w::note(
        ui,
        t(
            "Серверы и боты семьи. Anvil проверяет порт или адрес; обновление и откат — командами, которые вы записали сами.",
        ),
    );
    ui.add_space(12.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        amber_rows(app, ui, &p);
        for group in [Group::Server, Group::Bot, Group::Service] {
            let entries: Vec<Entry> = app.config.fleet.iter().filter(|e| e.group == group).cloned().collect();
            if entries.is_empty() {
                continue;
            }
            w::section_label(ui, group_label(group));
            ui.add_space(4.0);
            for entry in &entries {
                entry_row(app, ui, entry, &p, &mut action);
            }
            ui.add_space(12.0);
        }
        if app.config.fleet.is_empty() {
            w::empty_state(
                ui,
                Icon::Server,
                t("Записей нет"),
                t("Добавьте сервер или бота: имя, как проверять (tcp:хост:порт или адрес http) и, если нужно, команды обновления и отката."),
            );
        }
    });
    match action {
        Some(Action::Refresh) => app.refresh_fleet(),
        Some(Action::Add) => app.fleet_form = Some(Form::default()),
        Some(Action::Remove(name)) => {
            app.config.fleet.retain(|e| e.name != name);
            app.save();
        }
        Some(Action::Ask(name, what)) => app.ask_fleet(&name, what),
        None => {}
    }
}

fn amber_rows(app: &mut App, ui: &mut Ui, p: &Palette) {
    let Some(Ok(summary)) = app.amber.get().cloned() else { return };
    if summary.servers.is_empty() {
        return;
    }
    w::section_label(ui, t("Серверы Amber (из amber-admin)"));
    ui.add_space(4.0);
    for server in &summary.servers {
        let (tone, state) = super::amber::health(&server.health);
        ui.horizontal(|ui| {
            w::dot(ui, tone);
            ui.label(RichText::new(crate::amber::label(&server.name)).size(13.5).color(p.text));
            w::note(ui, state);
            if let Some(version) = &server.version {
                w::mono(ui, version, Some(p.weak));
            }
            if let (Some(online), Some(members)) = (server.online, server.members) {
                w::note(ui, format!("{online}/{members}"));
            }
        });
    }
    w::note(ui, t("Обновляется в amber-admin."));
    ui.add_space(12.0);
}

fn entry_row(app: &App, ui: &mut Ui, entry: &Entry, p: &Palette, action: &mut Option<Action>) {
    let status = app.fleet_status.get(&entry.name);
    let (tone, text) = status.map_or((Tone::Neutral, t("ещё не проверяли")), |s| state_text(s.state));
    ui.horizontal(|ui| {
        w::dot(ui, tone);
        ui.label(RichText::new(&entry.name).size(13.5).color(p.text));
        w::note(ui, text);
        if let Some(status) = status {
            if let Some(version) = &status.version {
                w::mono(ui, version, Some(p.weak));
            }
            if let Some(ms) = status.latency_ms {
                w::note(ui, format!("{ms} {}", t("мс")));
            }
            w::note(ui, i18n::ago(status.checked));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::icon_button(ui, Icon::Trash, t("Убрать запись")).clicked() {
                *action = Some(Action::Remove(entry.name.clone()));
            }
            if !entry.rollback_cmd.trim().is_empty() && w::button(ui, Kind::Ghost, None, t("Откатить")).clicked()
            {
                *action = Some(Action::Ask(entry.name.clone(), What::Rollback));
            }
            if !entry.update_cmd.trim().is_empty()
                && w::button(ui, Kind::Secondary, Some(Icon::Download), t("Обновить")).clicked()
            {
                *action = Some(Action::Ask(entry.name.clone(), What::Update));
            }
        });
    });
    if let Some(error) = status.and_then(|s| s.error.as_ref()) {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.add(egui::Label::new(RichText::new(error).size(12.0).color(p.weak)).truncate());
        });
    }
    ui.add_space(4.0);
}

/// Окна поверх: форма записи и подтверждение команды.
pub fn dialogs(app: &mut App, ctx: &egui::Context) {
    form(app, ctx);
    ask(app, ctx);
}

fn form(app: &mut App, ctx: &egui::Context) {
    let Some(mut form) = app.fleet_form.take() else { return };
    let mut open = true;
    let mut save = false;
    chrome::dialog(ctx, "anvil-fleet-form", t("Запись парка"), 520.0, &mut open, |ui| {
        w::section_label(ui, t("Имя"));
        ui.add(egui::TextEdit::singleline(&mut form.entry.name).desired_width(ui.available_width()));
        ui.add_space(8.0);
        w::segmented(
            ui,
            &mut form.entry.group,
            &[(Group::Server, None, t("Сервер")), (Group::Bot, None, t("Бот")), (Group::Service, None, t("Служба"))],
        );
        ui.add_space(8.0);
        w::section_label(ui, t("Как проверять"));
        ui.add(
            egui::TextEdit::singleline(&mut form.entry.probe)
                .hint_text("tcp:host:port  /  https://host/health")
                .desired_width(ui.available_width()),
        );
        let bad_probe = !form.entry.probe.trim().is_empty()
            && crate::fleet::parse_probe(&form.entry.probe) == crate::fleet::Probe::None;
        if bad_probe {
            w::note(ui, t("Нужно tcp:хост:порт или адрес на http:// или https://."));
        }
        ui.add_space(8.0);
        w::section_label(ui, t("Команда обновления (по желанию)"));
        ui.add(egui::TextEdit::singleline(&mut form.entry.update_cmd).desired_width(ui.available_width()));
        ui.add_space(8.0);
        w::section_label(ui, t("Команда отката (по желанию)"));
        ui.add(egui::TextEdit::singleline(&mut form.entry.rollback_cmd).desired_width(ui.available_width()));
        ui.add_space(12.0);
        let name = form.entry.name.trim();
        let taken = app.config.fleet.iter().any(|e| e.name == name);
        if taken {
            w::note(ui, t("Запись с таким именем уже есть."));
        }
        ui.add_enabled_ui(!name.is_empty() && !taken && !bad_probe, |ui| {
            if w::button(ui, Kind::Primary, None, t("Добавить")).clicked() {
                save = true;
            }
        });
    });
    if save {
        form.entry.name = form.entry.name.trim().to_owned();
        app.config.fleet.push(form.entry);
        app.save();
        app.refresh_fleet();
    } else if open {
        app.fleet_form = Some(form);
    }
}

fn ask(app: &mut App, ctx: &egui::Context) {
    let Some((name, what, command)) = app.fleet_ask.clone() else { return };
    let heading = match what {
        What::Update => format!("{} {name}?", t("Обновить")),
        What::Rollback => format!("{} {name}?", t("Откатить")),
    };
    let body = |ui: &mut Ui| {
        let p = Palette::of(ui);
        w::note(ui, t("Будет выполнено на этом ПК:"));
        ui.add_space(4.0);
        w::mono(ui, &command, Some(p.text));
    };
    let label = match what {
        What::Update => t("Обновить"),
        What::Rollback => t("Откатить"),
    };
    match w::confirm(ctx, "anvil-fleet-ask", &heading, body, label, what == What::Rollback) {
        Some(true) => {
            app.fleet_ask = None;
            app.run_fleet(name, what, command);
        }
        Some(false) => app.fleet_ask = None,
        None => {}
    }
}
