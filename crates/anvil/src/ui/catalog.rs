//! «Каталог программ»: все программы подключённых проектов — что установлено, что вышло на GitHub, обновить или поставить
//! разом; список репозиториев для переноса на другой ПК.

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use anvil_update::Version;
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::github::Release;
use crate::i18n::t;

enum Status {
    /// Установлена, на GitHub есть новее.
    Update(Version),
    /// Установлена, новее нет (или выпусков нет).
    Current,
    /// Не установлена, выпуск на GitHub есть.
    Fresh(Version),
    /// Не установлена и выпусков нет — можно только собрать из кода.
    LocalOnly,
}

struct Row {
    project: PathBuf,
    project_name: String,
    bin: String,
    installed: Option<String>,
    release: Option<Release>,
    status: Status,
}

enum Action {
    Install(PathBuf, String, Release),
    Local(PathBuf, String),
    UpdateAll,
    InstallAll,
    Copy,
    Import,
}

fn rows(app: &App) -> Vec<Row> {
    let prerelease = app.config.common.prerelease;
    let mut rows = Vec::new();
    for project in app.visible() {
        let Some(meta) = project.meta() else { continue };
        let remote = app.remotes.get(&project.path);
        for bin in meta.bins.iter().filter(|b| !b.hint.hidden && crate::deck::role(b) != crate::deck::Role::Tool) {
            let installed = app.installs.get(&bin.name).and_then(Option::as_ref);
            let latest = super::install::release_for(remote, &bin.name, prerelease);
            let newer = super::install::newer(installed, latest.as_ref()).cloned();
            let status = match (installed.is_some(), newer, &latest) {
                (true, Some(version), _) => Status::Update(version),
                (true, None, _) => Status::Current,
                (false, _, Some((_, version))) => Status::Fresh(version.clone()),
                (false, _, None) => Status::LocalOnly,
            };
            rows.push(Row {
                project: project.path.clone(),
                project_name: project.name(),
                bin: bin.name.clone(),
                installed: installed.and_then(|i| i.current.clone()),
                release: latest.map(|(r, _)| r.clone()),
                status,
            });
        }
    }
    rows
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let rows = rows(app);
    let updates = rows.iter().filter(|r| matches!(r.status, Status::Update(_))).count();
    let fresh = rows.iter().filter(|r| matches!(r.status, Status::Fresh(_))).count();
    let mut action = None;
    ui.horizontal(|ui| {
        w::title(ui, t("Каталог программ"), 22.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if fresh > 0 {
                let text = format!("{} · {fresh}", t("Поставить все"));
                if w::button(ui, Kind::Secondary, Some(Icon::Download), &text).clicked() {
                    action = Some(Action::InstallAll);
                }
            }
            if updates > 0 {
                let text = format!("{} · {updates}", t("Обновить все"));
                if w::button(ui, Kind::Primary, Some(Icon::Download), &text).clicked() {
                    action = Some(Action::UpdateAll);
                }
            }
        });
    });
    w::note(
        ui,
        t(
            "Программы подключённых проектов: что стоит на этом ПК и что вышло на GitHub. Ставится из выпусков, данные программ не трогаются.",
        ),
    );
    ui.add_space(12.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if rows.is_empty() {
            w::empty_state(
                ui,
                Icon::Package,
                t("Программ нет"),
                t("Подключите проекты на Rust (кнопка + над списком): их программы появятся здесь."),
            );
        }
        for row in &rows {
            row_view(ui, row, &p, &mut action);
        }
        ui.add_space(14.0);
        w::section_label(ui, t("Перенос на другой ПК"));
        ui.add_space(2.0);
        w::note(ui, t("Скопируйте список репозиториев, на новом ПК вставьте его ниже: Anvil склонирует проекты, а программы вы поставите отсюда."));
        ui.add_space(6.0);
        if w::button(ui, Kind::Secondary, Some(Icon::Copy), t("Скопировать список")).clicked() {
            action = Some(Action::Copy);
        }
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::multiline(&mut app.catalog_import)
                .hint_text(t("owner/name, по одному в строке"))
                .desired_rows(4)
                .desired_width(ui.available_width()),
        );
        ui.add_space(6.0);
        ui.add_enabled_ui(!app.catalog_import.trim().is_empty() && app.sources.cloning.is_none(), |ui| {
            if w::button(ui, Kind::Secondary, Some(Icon::Plus), t("Подключить из списка")).clicked() {
                action = Some(Action::Import);
            }
        });
        if app.sources.cloning.is_some() || !app.sources.queue.is_empty() {
            let left = app.sources.queue.len() + usize::from(app.sources.cloning.is_some());
            ui.horizontal(|ui| {
                w::spinner(ui, 14.0);
                w::note(ui, format!("{} {left}", t("Клонирую, осталось:")));
            });
        }
        if let Some(error) = &app.sources.error {
            w::note(ui, error);
        }
    });
    run(app, ui.ctx(), action, rows);
}

fn row_view(ui: &mut Ui, row: &Row, p: &Palette, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::installs::display_name(&row.bin)).size(13.5).color(p.text));
        w::note(ui, &row.project_name);
        if let Some(installed) = &row.installed {
            w::mono(ui, installed, Some(p.weak));
        }
        match &row.status {
            Status::Update(version) => {
                w::note(ui, format!("→ {version}"));
                w::badge(ui, t("есть новая"), Tone::Success);
            }
            Status::Current => {
                w::badge(ui, t("актуальна"), Tone::Neutral);
            }
            Status::Fresh(version) => {
                w::note(ui, version.to_string());
                w::badge(ui, t("не установлена"), Tone::Accent);
            }
            Status::LocalOnly => {
                w::badge(ui, t("нет выпусков"), Tone::Neutral);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match (&row.status, &row.release) {
            (Status::Update(_), Some(release)) => {
                if w::button(ui, Kind::Secondary, Some(Icon::Download), t("Обновить")).clicked() {
                    *action = Some(Action::Install(row.project.clone(), row.bin.clone(), release.clone()));
                }
            }
            (Status::Fresh(_), Some(release)) => {
                if w::button(ui, Kind::Secondary, Some(Icon::Download), t("Поставить")).clicked() {
                    *action = Some(Action::Install(row.project.clone(), row.bin.clone(), release.clone()));
                }
            }
            // Кнопка рисуется в условии: нажата — действие, нет — ничего (общая ветка `_` ниже).
            (Status::LocalOnly, _)
                if w::button(ui, Kind::Ghost, Some(Icon::Hammer), t("Собрать и поставить")).clicked() =>
            {
                *action = Some(Action::Local(row.project.clone(), row.bin.clone()));
            }
            _ => {}
        });
    });
    ui.add_space(4.0);
}

fn run(app: &mut App, ctx: &egui::Context, action: Option<Action>, rows: Vec<Row>) {
    match action {
        Some(Action::Install(path, bin, release)) => {
            app.install_release(&path, &bin, &release);
        }
        Some(Action::Local(path, bin)) => app.install_local(&path, &bin),
        Some(Action::UpdateAll) => install_all(app, rows, |s| matches!(s, Status::Update(_))),
        Some(Action::InstallAll) => install_all(app, rows, |s| matches!(s, Status::Fresh(_))),
        Some(Action::Copy) => {
            let visible = app.visible();
            ctx.copy_text(crate::catalog::export(&visible));
            app.toasts.push(t("Список скопирован"), Tone::Success);
        }
        Some(Action::Import) => {
            let (repos, bad) = crate::catalog::parse(&app.catalog_import);
            if !bad.is_empty() {
                app.sources.error = Some(format!("{}: {}", t("Не адреса"), bad.join(", ")));
            }
            app.queue_clones(repos);
            app.catalog_import.clear();
        }
        None => {}
    }
}

fn install_all(app: &mut App, rows: Vec<Row>, wanted: impl Fn(&Status) -> bool) {
    for row in rows.into_iter().filter(|r| wanted(&r.status)) {
        if let Some(release) = row.release {
            app.install_release(&row.project, &row.bin, &release);
        }
    }
}
