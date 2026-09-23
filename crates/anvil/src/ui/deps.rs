//! Зависимости: вкладка проекта, окно «Rust и зависимости», подтверждения обновлений.

use std::path::{Path, PathBuf};

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone, semibold};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::deps::{Change, Report, Toolchain};
use crate::i18n::{self, t};
use crate::worker::{self, Project};

/// Что попросили на вкладке или в окне; выполняется, когда `app` снова свободен.
pub enum Action {
    Check(PathBuf),
    Ask(Ask),
    Url(String),
}

/// Что подтверждается перед запуском.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// `cargo update` совместимых, затем тесты; красно — `Cargo.lock` возвращается.
    Update(PathBuf),
    /// Перевести программу на тег набора.
    Kit(PathBuf, String),
    /// `rustup update` активного тулчейна.
    Rustup(String),
}

pub fn run(app: &mut App, ctx: &egui::Context, action: Action) {
    match action {
        Action::Check(path) => app.check_deps(vec![path], true),
        Action::Ask(ask) => app.deps_ask = Some(ask),
        Action::Url(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
    }
}

/// Сколько всего внимания у проекта: уязвимости и доступные обновления — для подписи вкладки.
pub fn attention(report: Option<&Report>) -> usize {
    report.map_or(0, |r| r.advisories.len() + r.updates.iter().filter(|c| c.direct).count() + r.majors().count())
}

// ─── Вкладка проекта ────────────────────────────────────────────────────────

pub fn tab(app: &App, ui: &mut Ui, project: &Project) -> Vec<Action> {
    let mut actions = Vec::new();
    let p = Palette::of(ui);
    let path = project.path.clone();
    let report = app.deps.get(&path);
    let checking = app.deps_busy.as_ref() == Some(&path);

    ui.horizontal(|ui| {
        let when = match report {
            Some(r) => format!("{} {}", t("Проверено"), i18n::ago(r.checked)),
            None if checking => t("Проверяю…").to_owned(),
            None => t("Ещё не проверяли").to_owned(),
        };
        ui.label(RichText::new(when).size(13.0).color(p.weak));
        if checking {
            w::spinner(ui, 14.0);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!checking, |ui| {
                let hint = t("Спросить crates.io и сверить с базой RustSec заново");
                if w::button(ui, Kind::Secondary, Some(Icon::Refresh), t("Проверить")).on_hover_text(hint).clicked()
                {
                    actions.push(Action::Check(path.clone()));
                }
            });
        });
    });
    ui.add_space(8.0);

    let Some(report) = report else {
        w::empty_state(
            ui,
            Icon::Package,
            t("Зависимости ещё не проверены"),
            t("Anvil спросит crates.io, что обновилось, и сверит Cargo.lock с базой уязвимостей RustSec."),
        );
        return actions;
    };
    if let Some(error) = &report.error {
        w::banner(ui, Tone::Warning, t("crates.io не ответил"), error, |_| {});
        ui.add_space(8.0);
    }

    // Уязвимости — первыми.
    if !report.advisories.is_empty() {
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Warning, t("RustSec"));
            for hit in &report.advisories {
                advisory_row(ui, hit, &mut actions);
            }
            w::note(
                ui,
                t(
                    "Обычно помогает «Обновить совместимые»; если исправление только в новой мажорной версии — обновить её руками.",
                ),
            );
        });
        ui.add_space(12.0);
    }

    // Совместимые обновления.
    let direct_updates: Vec<&Change> = report.updates.iter().filter(|c| c.direct).collect();
    let chained = report.updates.len() - direct_updates.len();
    w::card(ui, |ui| {
        ui.horizontal(|ui| {
            w::card_title(ui, Icon::ArrowUp, t("Совместимые обновления"));
            if !report.updates.is_empty() {
                w::badge(ui, &report.updates.len().to_string(), Tone::Accent);
            }
            if !report.updates.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let hint = t("cargo update, потом тесты; если красно — Cargo.lock вернётся как был");
                    if w::button(ui, Kind::Primary, Some(Icon::ArrowUp), t("Обновить совместимые"))
                        .on_hover_text(hint)
                        .clicked()
                    {
                        actions.push(Action::Ask(Ask::Update(path.clone())));
                    }
                });
            }
        });
        if report.updates.is_empty() {
            w::note(ui, t("Всё уже на последних совместимых версиях."));
        }
        for change in &direct_updates {
            change_row(ui, change, false, &mut actions);
        }
        if chained > 0 {
            let title = format!(
                "{} {}",
                t("По цепочке:"),
                i18n::count(chained, ["пакет", "пакета", "пакетов"], ["package", "packages"])
            );
            egui::CollapsingHeader::new(RichText::new(title).size(13.0).color(p.weak))
                .id_salt(("chained", &path))
                .show(ui, |ui| {
                    for change in report.updates.iter().filter(|c| !c.direct) {
                        change_row(ui, change, false, &mut actions);
                    }
                });
        }
    });
    ui.add_space(12.0);

    // Новые мажорные версии и то, что держат требования.
    let majors: Vec<&Change> = report.majors().collect();
    let held_back: Vec<&Change> = report.held.iter().filter(|c| c.compatible()).collect();
    if !majors.is_empty() || !held_back.is_empty() {
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Rocket, t("Новые мажорные версии"));
            if majors.is_empty() {
                w::note(ui, t("У прямых зависимостей мажорных обновлений нет."));
            } else {
                w::note(ui, t("Сами не обновляются: API мог поменяться. Посмотрите, что нового, и поднимите руками."));
            }
            for change in &majors {
                change_row(ui, change, true, &mut actions);
            }
            if !held_back.is_empty() {
                let title = format!(
                    "{} {}",
                    t("Держат требования других пакетов:"),
                    i18n::count(held_back.len(), ["пакет", "пакета", "пакетов"], ["package", "packages"])
                );
                egui::CollapsingHeader::new(RichText::new(title).size(13.0).color(p.weak))
                    .id_salt(("held", &path))
                    .show(ui, |ui| {
                        for change in &held_back {
                            change_row(ui, change, true, &mut actions);
                        }
                    });
            }
        });
        ui.add_space(12.0);
    }

    // Набор Anvil.
    if let Some(kit) = &report.kit {
        let latest = app.toolchain.as_ref().and_then(|t| t.kit_latest.clone());
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Hammer, t("Набор Anvil"));
            kit_line(ui, kit, latest.as_deref(), &path, &mut actions);
        });
    }
    actions
}

fn advisory_row(ui: &mut Ui, hit: &crate::rustsec::Hit, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let (tone, kind) = match hit.informational.as_deref() {
        None => (Tone::Danger, t("уязвимость")),
        Some("unsound") => (Tone::Danger, t("небезопасно")),
        Some("unmaintained") => (Tone::Warning, t("не поддерживается")),
        Some(_) => (Tone::Neutral, t("заметка")),
    };
    ui.horizontal(|ui| {
        w::badge(ui, kind, tone);
        ui.label(RichText::new(format!("{} {}", hit.package, hit.version)).font(semibold(14.0)).color(p.text));
        if ui.link(RichText::new(&hit.id).size(13.0)).clicked() {
            actions.push(Action::Url(hit.url.clone()));
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        let fixed = if hit.patched.is_empty() {
            t("исправления пока нет").to_owned()
        } else {
            format!("{} {}", t("исправлено:"), hit.patched.join(", "))
        };
        let text = if hit.title.is_empty() { fixed } else { format!("{} · {fixed}", hit.title) };
        ui.add(egui::Label::new(RichText::new(text).size(13.0).color(p.weak)).wrap());
    });
    ui.add_space(6.0);
}

fn change_row(ui: &mut Ui, change: &Change, link: bool, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&change.name).color(p.text));
        w::mono(ui, &format!("{} → {}", change.from, change.to), Some(p.weak));
        if link {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.link(RichText::new(t("что нового")).size(13.0)).clicked() {
                    actions.push(Action::Url(format!("https://crates.io/crates/{}/versions", change.name)));
                }
            });
        }
    });
}

fn kit_line(ui: &mut Ui, kit: &crate::deps::KitUse, latest: Option<&str>, path: &Path, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        let current = kit.tag.clone().unwrap_or_else(|| format!("{} {}", t("коммит"), kit.commit));
        ui.label(RichText::new(&current).font(semibold(14.0)).color(p.text));
        match latest {
            Some(latest) if kit.tag.as_deref() != Some(latest) => {
                w::badge(ui, &format!("{} {latest}", t("есть")), Tone::Accent);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = format!("{} {latest}", t("Поднять до"));
                    if w::button(ui, Kind::Secondary, Some(Icon::ArrowUp), &label).clicked() {
                        actions.push(Action::Ask(Ask::Kit(path.to_path_buf(), latest.to_owned())));
                    }
                });
            }
            Some(_) => {
                w::badge(ui, t("свежий"), Tone::Success);
            }
            None => {}
        }
    });
}

// ─── Окно «Rust и зависимости» ──────────────────────────────────────────────

pub fn overview(app: &mut App, ctx: &egui::Context) {
    let mut open = app.overview_open;
    let mut actions = Vec::new();
    let mut check_toolchain = false;
    chrome::dialog(ctx, "anvil-rust", t("Rust и зависимости"), 640.0, &mut open, |ui| {
        let p = Palette::of(ui);
        w::section_label(ui, t("Тулчейн"));
        ui.add_space(4.0);
        match &app.toolchain {
            Some(tc) => toolchain_rows(ui, tc, &mut actions, &mut check_toolchain),
            None => {
                ui.horizontal(|ui| {
                    w::spinner(ui, 14.0);
                    ui.label(RichText::new(t("Спрашиваю rustup…")).color(p.weak));
                });
            }
        }

        ui.add_space(14.0);
        w::section_label(ui, t("Набор Anvil в программах"));
        ui.add_space(4.0);
        let latest = app.toolchain.as_ref().and_then(|t| t.kit_latest.clone());
        let mut any = false;
        for project in app.visible() {
            let Some(kit) = app.deps.get(&project.path).and_then(|r| r.kit.as_ref()) else { continue };
            any = true;
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(140.0, 20.0), egui::Sense::hover());
                ui.painter().text(
                    rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    project.name(),
                    egui::FontId::proportional(14.0),
                    p.text,
                );
                kit_line(ui, kit, latest.as_deref(), &project.path, &mut actions);
            });
        }
        if !any {
            w::note(ui, t("Ни одна программа пока не берёт набор с GitHub (или зависимости ещё не проверены)."));
        }

        ui.add_space(14.0);
        w::section_label(ui, t("Где разошлись версии"));
        ui.add_space(4.0);
        let reports: Vec<(String, &Report)> =
            app.visible().into_iter().filter_map(|p| app.deps.get(&p.path).map(|r| (p.name(), r))).collect();
        let list = crate::deps::diverged(&reports);
        if list.is_empty() {
            w::note(ui, t("Общие прямые зависимости у программ одних и тех же версий."));
        }
        egui::Grid::new("diverged").num_columns(2).spacing(egui::vec2(18.0, 6.0)).show(ui, |ui| {
            for (name, uses) in &list {
                ui.label(RichText::new(name).color(p.text));
                let text = uses
                    .iter()
                    .map(|(project, version)| format!("{project} {version}"))
                    .collect::<Vec<_>>()
                    .join(" · ");
                w::mono(ui, &text, Some(p.weak));
                ui.end_row();
            }
        });
    });
    app.overview_open = open;
    if check_toolchain {
        app.check_toolchain(true);
    }
    for action in actions {
        run(app, ctx, action);
    }
}

fn toolchain_rows(ui: &mut Ui, tc: &Toolchain, actions: &mut Vec<Action>, check: &mut bool) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Rust {}", tc.current)).font(semibold(15.0)).color(p.text));
        match &tc.latest {
            Some(latest) => {
                w::badge(ui, &format!("{} {latest}", t("есть")), Tone::Accent);
            }
            None if tc.error.is_none() => {
                w::badge(ui, t("свежий"), Tone::Success);
            }
            None => {}
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::icon_button(ui, Icon::Refresh, t("Проверить снова")).clicked() {
                *check = true;
            }
            if tc.latest.is_some() && w::button(ui, Kind::Primary, Some(Icon::Download), t("Обновить Rust")).clicked()
            {
                actions.push(Action::Ask(Ask::Rustup(tc.name.clone())));
            }
        });
    });
    let mut note = format!("{} · {} {}", tc.name, t("проверено"), i18n::ago(tc.checked));
    if let Some(error) = &tc.error {
        note = format!("{note} · {error}");
    }
    w::note(ui, note);
}

// ─── Подтверждения ──────────────────────────────────────────────────────────

pub fn confirm(app: &mut App, ctx: &egui::Context) {
    let Some(ask) = app.deps_ask.clone() else { return };
    let jobs = app.config.build_jobs;
    let (heading, confirm_label, steps): (String, &str, Vec<String>) = match &ask {
        Ask::Update(path) => (
            format!("{} {}?", t("Обновить совместимые зависимости в"), worker::display_name(path)),
            t("Обновить зависимости"),
            vec![
                t("cargo update — только версии в рамках требований Cargo.toml").to_owned(),
                format!("cargo test --workspace{}", if jobs > 0 { format!(" -j {jobs}") } else { String::new() }),
                t("тесты не прошли — Cargo.lock вернётся как был").to_owned(),
                t("изменения останутся в рабочей копии — закоммитьте их, когда посмотрите").to_owned(),
            ],
        ),
        Ask::Kit(path, tag) => (
            format!("{} {} {tag}?", t("Перевести"), worker::display_name(path)),
            t("Перевести"),
            vec![
                format!("{} {tag}", t("Cargo.toml: anvil-ui и anvil-update — тег")),
                format!("{} {tag}", t("workflow выпуска: rust-release.yml@")),
                format!("cargo test --workspace{}", if jobs > 0 { format!(" -j {jobs}") } else { String::new() }),
                t("не собралось или тесты не прошли — все файлы вернутся как были").to_owned(),
            ],
        ),
        Ask::Rustup(name) => (
            t("Обновить Rust?").to_owned(),
            t("Обновить Rust"),
            vec![format!("rustup update {name}"), t("сборки, которые идут в это время, лучше дождаться").to_owned()],
        ),
    };
    let body = |ui: &mut Ui| {
        let p = Palette::of(ui);
        for (i, step) in steps.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).color(p.weak));
                ui.add(egui::Label::new(RichText::new(step).color(p.text)).wrap());
            });
        }
    };
    match w::confirm(ctx, "anvil-deps-confirm", &heading, body, confirm_label, false) {
        Some(true) => {
            app.deps_ask = None;
            app.start_deps(ask);
        }
        Some(false) => app.deps_ask = None,
        None => {}
    }
}
