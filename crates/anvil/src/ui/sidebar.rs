//! Список проектов слева и сводка под ним.

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::i18n::{self, t};
use crate::worker::Project;

/// Точка и бейдж проекта в списке: что в нём требует внимания.
pub fn status(project: &Project) -> (Tone, Option<(String, Tone)>) {
    if project.git.is_err() {
        return (Tone::Danger, Some((t("ошибка git").to_owned(), Tone::Danger)));
    }
    if let Some(Err(_)) = &project.meta {
        return (Tone::Danger, Some((t("ошибка cargo").to_owned(), Tone::Danger)));
    }
    let Some(git) = project.git() else {
        return (Tone::Neutral, Some((t("без git").to_owned(), Tone::Neutral)));
    };
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
            let (tone, trailing) = status(project);
            Row { path: project.path.clone(), name: project.name(), tone, trailing }
        })
        .collect();
    let current = app.current().map(|p| p.path.clone());

    egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(ui.available_height() - 150.0).show(ui, |ui| {
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
    for project in app.visible() {
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
}
