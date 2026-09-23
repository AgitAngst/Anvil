//! Окна-вопросы: занятый exe, остановка программы, очистка сборки.

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::i18n::t;
use crate::tasks::{Resolve, Task};
use crate::worker;

pub fn show(app: &mut App, ctx: &egui::Context) {
    locked(app, ctx);
    stop(app, ctx);
    clean(app, ctx);
    uninstall(app, ctx);
}

fn uninstall(app: &mut App, ctx: &egui::Context) {
    let Some(bin) = app.uninstall_confirm.clone() else { return };
    let root = crate::installs::root(&bin);
    let body = |ui: &mut egui::Ui| {
        let p = Palette::of(ui);
        w::note(ui, t("Удалятся все установленные версии и ярлык в «Пуске». Данные программы в %APPDATA% останутся."));
        ui.add_space(4.0);
        ui.label(RichText::new(root.display().to_string()).font(egui::FontId::monospace(12.5)).color(p.text));
    };
    let heading = format!("{} {}?", t("Удалить установку"), crate::installs::display_name(&bin));
    match w::confirm(ctx, "anvil-uninstall", &heading, body, t("Удалить"), true) {
        Some(true) => {
            app.uninstall_confirm = None;
            let path = app.current().map(|p| p.path.clone());
            if let Some(path) = path {
                app.uninstall(&path, &bin);
            }
        }
        Some(false) => app.uninstall_confirm = None,
        None => {}
    }
}

/// Сборке мешает запущенная программа — три честных способа и отмена.
fn locked(app: &mut App, ctx: &egui::Context) {
    let Some((spec, locked, task)) = &app.locked else { return };
    let name = worker::display_name(&spec.project);
    let exes: Vec<String> = locked
        .iter()
        .map(|l| {
            let file = l.exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let pids: Vec<String> = l.pids.iter().map(u32::to_string).collect();
            format!("{file} · PID {}", pids.join(", "))
        })
        .collect();
    let running_after = matches!(task, Task::Run { .. });
    let mut open = true;
    let mut choice = None;
    let mut cancel = false;
    chrome::dialog(ctx, "anvil-locked", t("Программа сейчас запущена"), 520.0, &mut open, |ui| {
        let p = Palette::of(ui);
        w::note(
            ui,
            format!("{name}: {}", t("сборка должна перезаписать exe, а Windows не даёт трогать запущенный файл.")),
        );
        ui.add_space(6.0);
        for exe in &exes {
            ui.horizontal(|ui| {
                w::dot(ui, Tone::Success);
                w::mono(ui, exe, Some(p.text));
            });
        }
        ui.add_space(10.0);
        let option = |ui: &mut egui::Ui, icon: Icon, title: &str, text: &str| {
            let r = w::button(ui, Kind::Secondary, Some(icon), title);
            w::note(ui, text);
            ui.add_space(8.0);
            r.clicked()
        };
        if option(
            ui,
            Icon::ArrowRight,
            t("Отодвинуть exe и собрать"),
            t("Запущенный файл переименуется рядом, программа доработает как есть, новая сборка ляжет на своё место."),
        ) {
            choice = Some(Resolve::MoveAside);
        }
        let stop_text = if running_after {
            t(
                "Программа получит команду закрыться, как от крестика; через 5 секунд — принудительно. Потом сборка и запуск новой.",
            )
        } else {
            t(
                "Программа получит команду закрыться, как от крестика; через 5 секунд — принудительно. Несохранённое в ней может пропасть.",
            )
        };
        if option(ui, Icon::Stop, t("Закрыть программу и собрать"), stop_text) {
            choice = Some(Resolve::Stop);
        }
        if option(
            ui,
            Icon::Folder,
            t("Собрать в отдельную папку"),
            t(
                "Сборка пойдёт в папку anvil внутри target: запущенная программа не мешает, но собираться придётся с нуля.",
            ),
        ) {
            choice = Some(Resolve::SeparateDir);
        }
        // В строке: иначе раскладка справа налево растягивается на всю высоту диалога.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                cancel = w::button(ui, Kind::Ghost, None, t("Отмена")).clicked();
            });
        });
    });
    if choice.is_some() || !open || cancel {
        app.resolve_locked(choice);
    }
}

fn stop(app: &mut App, ctx: &egui::Context) {
    let Some((name, pid, dir)) = app.stop_confirm.clone() else { return };
    let body = |ui: &mut egui::Ui| {
        w::note(
            ui,
            t(
                "Программа получит команду закрыться, как от крестика окна. Если за 5 секунд не закроется — будет остановлена принудительно, несохранённое в ней пропадёт.",
            ),
        );
        ui.add_space(4.0);
        w::mono(ui, &format!("PID {pid}"), None);
    };
    let heading = format!("{} {name}?", t("Остановить"));
    match w::confirm(ctx, "anvil-stop", &heading, body, t("Остановить"), true) {
        Some(true) => {
            app.stop_confirm = None;
            app.stop_program(name, pid, dir);
        }
        Some(false) => app.stop_confirm = None,
        None => {}
    }
}

fn clean(app: &mut App, ctx: &egui::Context) {
    let Some(path) = app.clean_confirm.clone() else { return };
    let target = app
        .projects
        .iter()
        .find(|p| p.path == path)
        .and_then(|p| p.meta())
        .map(|m| m.target_dir.display().to_string())
        .unwrap_or_else(|| "target".into());
    let body = |ui: &mut egui::Ui| {
        let p = Palette::of(ui);
        w::note(ui, t("cargo clean удалит папку сборки целиком. Следующая сборка пойдёт с нуля и будет долгой."));
        ui.add_space(4.0);
        ui.label(RichText::new(&target).font(egui::FontId::monospace(12.5)).color(p.text));
    };
    let heading = format!("{} {}?", t("Очистить сборку"), worker::display_name(&path));
    match w::confirm(ctx, "anvil-clean", &heading, body, t("Очистить"), true) {
        Some(true) => {
            app.clean_confirm = None;
            app.start_task(&path, Task::Clean);
        }
        Some(false) => app.clean_confirm = None,
        None => {}
    }
}
