//! Готовый вид обновления на `anvil-ui`: баннер под шапкой, окно «Что нового», строка для
//! «О программе». Никаких модальных окон при запуске — только ненавязчивый баннер.

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{CommonSettings, Icon, Kind, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::lang::tr;
use crate::{State, Update, Updater};

fn notes_id() -> egui::Id {
    egui::Id::new("anvil-update-notes")
}

fn mb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_048_576.0)
}

/// Баннер обновления. Ставится в начало основной области. Возвращает `true`, если изменились
/// настройки (пропущенная версия) — программе стоит их сохранить.
pub fn banner(ui: &mut Ui, updater: &Updater, settings: &mut CommonSettings) -> bool {
    let ctx = ui.ctx().clone();
    let current = updater.config().version.clone();
    let mut changed = false;
    let state = updater.state();
    let shown = !updater.dismissed();
    match state {
        State::Available(update) if shown => {
            let title = format!("{} v{}", tr(&ctx, "Вышла"), update.version);
            let text = format!("{} {current}", tr(&ctx, "сейчас"));
            w::banner(ui, Tone::Accent, &title, &text, |ui| {
                if w::button(ui, Kind::Ghost, None, tr(&ctx, "Пропустить")).clicked() {
                    settings.skip_version = Some(update.version.to_string());
                    changed = true;
                    updater.dismiss();
                }
                if w::button(ui, Kind::Ghost, None, tr(&ctx, "Позже")).clicked() {
                    updater.dismiss();
                }
                if w::button(ui, Kind::Ghost, None, tr(&ctx, "Что нового")).clicked() {
                    ctx.data_mut(|d| d.insert_temp(notes_id(), true));
                }
                if update.asset.is_some() {
                    if w::button(ui, Kind::Primary, Some(Icon::Download), tr(&ctx, "Обновить")).clicked() {
                        updater.install();
                    }
                } else if w::button(ui, Kind::Secondary, Some(Icon::Code), tr(&ctx, "Страница выпуска")).clicked()
                {
                    ctx.open_url(egui::OpenUrl::new_tab(&update.page));
                }
            });
            ui.add_space(14.0);
        }
        State::Downloading { update, done, total } => {
            let title = format!("{} v{}", tr(&ctx, "Скачиваю"), update.version);
            let text = format!("{} {} {} {}", mb(done), tr(&ctx, "из"), mb(total), tr(&ctx, "МБ"));
            let fraction = (total > 0).then(|| done as f32 / total as f32);
            w::banner(ui, Tone::Accent, &title, &text, |ui| {
                w::progress(ui, fraction, 180.0);
            });
            ui.add_space(14.0);
        }
        State::Ready { update, .. } if shown => {
            let text = format!("{} v{}", tr(&ctx, "Перезапустите программу, чтобы перейти на"), update.version);
            let mut restart = false;
            w::banner(ui, Tone::Success, tr(&ctx, "Обновление установлено"), &text, |ui| {
                if w::button(ui, Kind::Ghost, None, tr(&ctx, "Позже")).clicked() {
                    updater.dismiss();
                }
                restart = w::button(ui, Kind::Primary, Some(Icon::Refresh), tr(&ctx, "Перезапустить")).clicked();
            });
            if restart {
                match updater.restart() {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => {
                        let text = format!("{}: {e}", tr(&ctx, "Не удалось перезапуститься"));
                        w::note(ui, text);
                    }
                }
            }
            ui.add_space(14.0);
        }
        State::Failed { update: Some(_), error } if shown => {
            w::banner(ui, Tone::Danger, tr(&ctx, "Обновление не удалось"), &error, |ui| {
                if w::button(ui, Kind::Ghost, None, tr(&ctx, "Скрыть")).clicked() {
                    updater.dismiss();
                }
                if w::button(ui, Kind::Secondary, Some(Icon::Refresh), tr(&ctx, "Повторить")).clicked() {
                    updater.install();
                }
            });
            ui.add_space(14.0);
        }
        _ => {}
    }
    notes(&ctx, updater);
    changed
}

/// Окно «Что нового»: заметки к выпуску с GitHub.
fn notes(ctx: &egui::Context, updater: &Updater) {
    let mut open: bool = ctx.data(|d| d.get_temp(notes_id())).unwrap_or(false);
    if !open {
        return;
    }
    let update: Option<Update> = match updater.state() {
        State::Available(u) | State::Ready { update: u, .. } | State::Downloading { update: u, .. } => Some(u),
        State::Failed { update, .. } => update,
        _ => None,
    };
    let Some(update) = update else {
        ctx.data_mut(|d| d.insert_temp(notes_id(), false));
        return;
    };
    let title = format!("{} · v{}", tr(ctx, "Что нового"), update.version);
    chrome::dialog(ctx, "anvil-update-notes-dialog", &title, 560.0, &mut open, |ui| {
        let p = Palette::of(ui);
        if update.notes.is_empty() {
            w::note(ui, tr(ctx, "Заметок к выпуску нет."));
        } else {
            for line in update.notes.lines() {
                // Markdown попроще: заголовки полужирным, списки — точкой, остальное как есть.
                let line = line.trim_end();
                if let Some(head) = line.trim_start_matches('#').strip_prefix(' ').filter(|_| line.starts_with('#')) {
                    ui.add_space(4.0);
                    ui.label(RichText::new(head).font(anvil_ui::semibold(15.0)).color(p.text));
                } else if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
                    ui.label(RichText::new(format!("•  {item}")).color(p.text));
                } else {
                    ui.label(RichText::new(line).color(p.text));
                }
            }
        }
        if update.asset.is_none() {
            ui.add_space(8.0);
            w::note(ui, tr(ctx, "Для этой системы архива нет — только страница выпуска."));
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if w::button(ui, Kind::Secondary, Some(Icon::Code), tr(ctx, "Открыть на GitHub")).clicked() {
                ctx.open_url(egui::OpenUrl::new_tab(&update.page));
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(notes_id(), open));
}

/// Строка для окна «О программе».
pub fn about_status(ctx: &egui::Context, updater: &Updater) -> Option<String> {
    Some(match updater.state() {
        State::Idle => return None,
        State::Checking => tr(ctx, "Проверяю обновления…").to_owned(),
        State::UpToDate => tr(ctx, "Установлена последняя версия").to_owned(),
        State::Available(u) => format!("{} v{}", tr(ctx, "Доступна"), u.version),
        State::Downloading { update, .. } => format!("{} v{}", tr(ctx, "Скачиваю"), update.version),
        State::Ready { update, .. } => format!("{} · v{}", tr(ctx, "Обновление установлено"), update.version),
        State::Failed { error, .. } => format!("{}: {error}", tr(ctx, "Проверка не удалась")),
    })
}
