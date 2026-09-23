//! Список проектов слева и сводка под ним.

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::i18n::{self, t};
use crate::worker::Project;

/// Точка и бейдж проекта в списке: что в нём требует внимания.
pub fn status(
    project: &Project,
    remote: Option<&crate::github::Remote>,
    installs: &std::collections::HashMap<String, Option<crate::installs::Installed>>,
    deps: Option<&crate::deps::Report>,
) -> (Tone, Option<(String, Tone)>) {
    if project.git.is_err() {
        return (Tone::Danger, Some((t("ошибка git").to_owned(), Tone::Danger)));
    }
    if let Some(Err(_)) = &project.meta {
        return (Tone::Danger, Some((t("ошибка cargo").to_owned(), Tone::Danger)));
    }
    if super::github::failed(remote) {
        return (Tone::Danger, Some(("CI".to_owned(), Tone::Danger)));
    }
    if deps.is_some_and(|d| d.vulnerabilities() > 0) {
        return (Tone::Danger, Some(("RustSec".to_owned(), Tone::Danger)));
    }
    let Some(git) = project.git() else {
        return (Tone::Neutral, Some((t("без git").to_owned(), Tone::Neutral)));
    };
    // Установленная копия отстала от выпуска на GitHub.
    let newer = project.meta().into_iter().flat_map(|m| m.bins.iter()).find_map(|bin| {
        let installed = installs.get(&bin.name).and_then(Option::as_ref);
        let release = super::install::release_for(remote, &bin.name, false);
        super::install::newer(installed, release.as_ref()).map(|v| format!("⬆ {v}"))
    });
    if let Some(text) = newer {
        return (Tone::Success, Some((text, Tone::Success)));
    }
    if git.dirty() {
        let n = git.changes.len();
        return (Tone::Warning, Some((i18n::count(n, ["файл", "файла", "файлов"], ["file", "files"]), Tone::Warning)));
    }
    if git.behind > 0 {
        return (Tone::Warning, Some((format!("↓{}", git.behind), Tone::Warning)));
    }
    if git.ahead > 0 {
        return (Tone::Accent, Some((format!("↑{}", git.ahead), Tone::Accent)));
    }
    if git.upstream.is_none() {
        return (Tone::Neutral, Some((t("без origin").to_owned(), Tone::Neutral)));
    }
    (Tone::Success, None)
}

/// Строка списка, собранная заранее: во время отрисовки `app` нужен изменяемым.
struct Row {
    path: std::path::PathBuf,
    name: String,
    tone: Tone,
    trailing: Option<(String, Tone)>,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        w::section_label(ui, t("Проекты"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::icon_button(ui, Icon::Plus, t("Добавить папку с проектами")).clicked() {
                super::settings::add_root(app);
            }
        });
    });
    ui.add_space(4.0);

    let visible: Vec<Row> = app
        .visible()
        .into_iter()
        .map(|project| {
            let (tone, trailing) =
                status(project, app.remotes.get(&project.path), &app.installs, app.deps.get(&project.path));
            Row { path: project.path.clone(), name: project.name(), tone, trailing }
        })
        .collect();
    let current = app.current().map(|p| p.path.clone());

    egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(ui.available_height() - 200.0).show(ui, |ui| {
        if visible.is_empty() && !app.scanning {
            ui.add_space(8.0);
            let text = if app.search.trim().is_empty() {
                t("Здесь пока пусто: добавьте папку, в которой лежат проекты.")
            } else {
                t("Ничего не нашлось.")
            };
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                w::note(ui, text);
            });
        }
        for row in &visible {
            let trailing = row.trailing.as_ref().map(|(text, tone)| (text.as_str(), *tone));
            let selected = current.as_ref() == Some(&row.path);
            if w::nav_item(ui, selected, row.tone, &row.name, trailing).clicked() {
                app.select(row.path.clone());
            }
        }
    });

    // Сводка: только то, что требует внимания.
    let mut dirty = 0;
    let mut behind = 0;
    let mut ahead = 0;
    let mut broken = 0;
    let mut ci = 0;
    let mut vulnerable = 0;
    let mut outdated = 0;
    for project in app.visible() {
        ci += usize::from(super::github::failed(app.remotes.get(&project.path)));
        if let Some(report) = app.deps.get(&project.path) {
            vulnerable += usize::from(report.vulnerabilities() > 0);
            outdated += usize::from(report.updates.iter().any(|c| c.direct));
        }
        match project.git() {
            Some(git) => {
                dirty += usize::from(git.dirty());
                behind += usize::from(git.behind > 0);
                ahead += usize::from(git.ahead > 0);
            }
            None => broken += usize::from(project.git.is_err()),
        }
    }
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        w::section_label(ui, t("Состояние"));
    });
    ui.add_space(4.0);
    let projects = |n| i18n::count(n, ["проект", "проекта", "проектов"], ["project", "projects"]);
    let lines = [
        (Tone::Warning, dirty, t("с правками")),
        (Tone::Warning, behind, t("позади origin")),
        (Tone::Accent, ahead, t("с неотправленными коммитами")),
        (Tone::Danger, broken, t("с ошибкой чтения")),
        (Tone::Danger, ci, t("с упавшим CI")),
        (Tone::Danger, vulnerable, t("с уязвимостями")),
        (Tone::Accent, outdated, t("с устаревшими пакетами")),
    ];
    let mut any = false;
    for (tone, n, what) in lines {
        if n > 0 {
            any = true;
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                w::dot(ui, tone);
                ui.label(RichText::new(format!("{} {what}", projects(n))).size(13.0).color(p.weak));
            });
        }
    }
    if !any && !app.scanning {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            w::dot(ui, Tone::Success);
            ui.label(RichText::new(t("Всё закоммичено и отправлено")).size(13.0).color(p.weak));
        });
    }

    // Rust: версия и есть ли новее; щелчок — окно «Rust и зависимости».
    ui.add_space(10.0);
    let (text, trailing) = match &app.toolchain {
        Some(tc) => {
            (format!("Rust {}", tc.current), tc.latest.as_ref().map(|v| (format!("{} {v}", t("есть")), Tone::Accent)))
        }
        None => ("Rust".to_owned(), None),
    };
    let tone = if trailing.is_some() { Tone::Accent } else { Tone::Success };
    let trailing = trailing.as_ref().map(|(text, tone)| (text.as_str(), *tone));
    if w::nav_item(ui, false, tone, &text, trailing).on_hover_text(t("Rust и зависимости")).clicked() {
        app.overview_open = true;
    }
}
