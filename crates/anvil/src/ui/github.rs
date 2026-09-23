//! GitHub в окне: CI и выпуски проекта, токен в настройках.

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone, semibold};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::github::{Auth, CiRun, Release, Remote, RunState, TokenSource};
use crate::i18n::{self, t};

pub fn run_tone(state: RunState) -> Tone {
    match state {
        RunState::Success => Tone::Success,
        RunState::Failure => Tone::Danger,
        RunState::Running | RunState::Queued => Tone::Accent,
        RunState::Cancelled | RunState::Other => Tone::Neutral,
    }
}

pub fn run_label(state: RunState) -> &'static str {
    match state {
        RunState::Success => t("прошёл"),
        RunState::Failure => t("упал"),
        RunState::Running => t("идёт"),
        RunState::Queued => t("в очереди"),
        RunState::Cancelled => t("отменён"),
        RunState::Other => t("без итога"),
    }
}

/// Бейдж CI рядом с названием проекта.
pub fn ci_badge(ui: &mut Ui, remote: Option<&Remote>) {
    if let Some(run) = remote.and_then(Remote::latest) {
        w::badge(ui, &format!("CI {}", run_label(run.state)), run_tone(run.state)).on_hover_text(&run.workflow);
    }
}

/// Строка «CI» в карточке Git. Возвращает адрес, если по нему щёлкнули.
pub fn ci_row(ui: &mut Ui, remote: Option<&Remote>, head: Option<&str>) -> Option<String> {
    let run = remote.and_then(Remote::latest)?;
    let mut open = None;
    w::field_row(ui, "CI", |ui| {
        w::dot(ui, run_tone(run.state));
        ui.label(run_label(run.state));
        let r = link(ui, &format!("#{}", run.number));
        if r.clicked() {
            open = Some(run.url.clone());
        }
        w::note(ui, i18n::ago(run.updated));
        if head.is_some_and(|h| h != run.head_sha) {
            w::note(ui, "·")
                .on_hover_text(t("Прогон не для вашего последнего коммита: его ещё не отправили или CI не дошёл"));
            w::note(ui, t("не ваш HEAD"));
        }
    });
    open
}

/// Строка «Выпуск» в карточке Версия.
pub fn release_row(ui: &mut Ui, remote: Option<&Remote>) -> Option<String> {
    let release = remote?.releases.iter().find(|r| !r.draft)?;
    let mut open = None;
    w::field_row(ui, t("Выпуск"), |ui| {
        if link(ui, &release.tag).clicked() {
            open = Some(release.url.clone());
        }
        w::note(ui, i18n::ago(release.published));
    });
    open
}

fn link(ui: &mut Ui, text: &str) -> egui::Response {
    let p = Palette::of(ui);
    ui.add(
        egui::Label::new(RichText::new(text).font(egui::FontId::monospace(12.5)).color(p.accent_text))
            .sense(egui::Sense::click()),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Баннер над карточками, если последний прогон упал.
pub fn failure_banner(ui: &mut Ui, remote: Option<&Remote>) -> Option<String> {
    let run = remote.and_then(Remote::latest).filter(|r| r.state == RunState::Failure)?;
    let what = run
        .failed_jobs
        .iter()
        .map(|j| if j.steps.is_empty() { j.name.clone() } else { format!("{}: {}", j.name, j.steps.join(", ")) })
        .collect::<Vec<_>>()
        .join(" · ");
    let title = format!("{} · {} #{}", t("CI упал"), run.workflow, run.number);
    let mut open = None;
    w::banner(ui, Tone::Danger, &title, &what, |ui| {
        let url = run.failed_jobs.first().map(|j| j.url.clone()).unwrap_or_else(|| run.url.clone());
        if w::button(ui, Kind::Ghost, Some(Icon::Terminal), t("Открыть лог")).clicked() {
            open = Some(url);
        }
    });
    ui.add_space(14.0);
    open
}

/// Нет данных — почему: не на GitHub, ещё не спрашивали, ошибка.
fn missing(ui: &mut Ui, remote: Option<&Remote>, on_github: bool) -> bool {
    if !on_github {
        w::card(ui, |ui| {
            w::empty_state(ui, Icon::Code, t("Проект не на GitHub"), t("origin ведёт не на github.com или его нет."))
        });
        return true;
    }
    let Some(remote) = remote else {
        w::card(ui, |ui| {
            w::empty_state(ui, Icon::Refresh, t("Спрашиваю GitHub…"), t("Ответ придёт через пару секунд."))
        });
        return true;
    };
    if let Some(error) = &remote.error {
        w::banner(ui, Tone::Warning, t("GitHub не ответил"), error, |_| {});
        ui.add_space(8.0);
    }
    false
}

/// Вкладка «CI»: последние прогоны по ветке.
pub fn ci_tab(ui: &mut Ui, remote: Option<&Remote>, on_github: bool, head: Option<&str>) -> Option<String> {
    if missing(ui, remote, on_github) {
        return None;
    }
    let runs = remote.map(|r| r.runs.as_slice()).unwrap_or_default();
    if runs.is_empty() {
        w::card(ui, |ui| {
            w::empty_state(
                ui,
                Icon::Play,
                t("Прогонов нет"),
                t("В репозитории нет GitHub Actions или они ещё ни разу не запускались."),
            )
        });
        return None;
    }
    let p = Palette::of(ui);
    if let Some(remote) = remote {
        w::note(ui, format!("{} {}", t("GitHub спрошен"), i18n::ago(remote.checked)));
        ui.add_space(4.0);
    }
    let mut open = None;
    w::card(ui, |ui| {
        for (i, run) in runs.iter().enumerate() {
            if i > 0 {
                w::divider(ui);
            }
            ui.horizontal(|ui| {
                ui.set_min_height(32.0);
                w::badge(ui, run_label(run.state), run_tone(run.state));
                ui.label(RichText::new(&run.workflow).font(semibold(13.5)).color(p.text));
                w::mono(ui, &format!("#{}", run.number), None);
                let width = (ui.available_width() - 260.0).max(60.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add(egui::Label::new(RichText::new(&run.title).color(p.weak)).truncate());
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::button(ui, Kind::Ghost, Some(Icon::Code), t("Открыть")).clicked() {
                        open = Some(run.url.clone());
                    }
                    w::note(ui, i18n::ago(run.updated));
                    if head == Some(run.head_sha.as_str()) {
                        w::badge(ui, "HEAD", Tone::Accent).on_hover_text(t("Прогон для вашего последнего коммита"));
                    }
                    w::mono(ui, run.head_sha.get(..7).unwrap_or(&run.head_sha), None);
                });
            });
            for job in &run.failed_jobs {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    w::dot(ui, Tone::Danger);
                    ui.label(RichText::new(&job.name).color(p.text));
                    if !job.steps.is_empty() {
                        w::note(ui, format!("{}: {}", t("шаг"), job.steps.join(", ")));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::button(ui, Kind::Ghost, Some(Icon::Terminal), t("Лог")).clicked() {
                            open = Some(job.url.clone());
                        }
                    });
                });
            }
        }
    });
    open
}

/// Вкладка «Выпуски»: последние выпуски и их файлы.
pub fn releases_tab(ui: &mut Ui, remote: Option<&Remote>, on_github: bool) -> Option<String> {
    if missing(ui, remote, on_github) {
        return None;
    }
    let releases = remote.map(|r| r.releases.as_slice()).unwrap_or_default();
    if releases.is_empty() {
        w::card(ui, |ui| {
            w::empty_state(
                ui,
                Icon::Rocket,
                t("Выпусков нет"),
                t("Выпуски появятся, когда на GitHub опубликуют первый Release."),
            )
        });
        return None;
    }
    let mut open = None;
    w::card(ui, |ui| {
        for (i, release) in releases.iter().enumerate() {
            if i > 0 {
                w::divider(ui);
            }
            if let Some(url) = release_block(ui, release, i == 0) {
                open = Some(url);
            }
        }
    });
    open
}

fn release_block(ui: &mut Ui, release: &Release, latest: bool) -> Option<String> {
    let p = Palette::of(ui);
    let mut open = None;
    ui.horizontal(|ui| {
        ui.set_min_height(32.0);
        w::badge(ui, &release.tag, if latest { Tone::Accent } else { Tone::Neutral });
        if release.name != release.tag {
            ui.label(RichText::new(&release.name).font(semibold(13.5)).color(p.text));
        }
        if release.draft {
            w::badge(ui, t("черновик"), Tone::Warning);
        }
        if release.prerelease {
            w::badge(ui, t("пред-выпуск"), Tone::Warning);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::button(ui, Kind::Ghost, Some(Icon::Code), t("Открыть")).clicked() {
                open = Some(release.url.clone());
            }
            w::note(ui, i18n::ago(release.published));
        });
    });
    for asset in &release.assets {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            anvil_ui::icons::paint(ui.painter(), rect, Icon::File, p.weak);
            w::mono(ui, &asset.name, Some(p.text));
            let downloads = i18n::count(
                asset.downloads as usize,
                ["скачивание", "скачивания", "скачиваний"],
                ["download", "downloads"],
            );
            w::note(ui, format!("{} · {downloads}", size(asset.size)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::icon_button(ui, Icon::Download, t("Скачать в браузере")).clicked() {
                    open = Some(asset.url.clone());
                }
            });
        });
    }
    open
}

fn size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1.0 { format!("{mb:.1} {}", t("МБ")) } else { format!("{} {}", bytes.div_ceil(1024), t("КБ")) }
}

/// Раздел настроек: откуда токен, кто под ним, свой токен.
pub fn settings(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    w::section_label(ui, "GitHub");
    ui.add_space(2.0);
    match &app.gh_auth {
        None => {
            ui.horizontal(|ui| {
                w::spinner(ui, 14.0);
                w::note(ui, t("Проверяю доступ к GitHub…"));
            });
        }
        Some(auth) => auth_line(ui, auth),
    }
    ui.add_space(6.0);
    let own = app.gh_auth.as_ref().is_some_and(|a| a.source == TokenSource::Keyring);
    ui.horizontal(|ui| {
        // Место под «Сохранить» и, если свой токен уже есть, «Забыть».
        let buttons = if own { 190.0 } else { 110.0 };
        let edit = egui::TextEdit::singleline(&mut app.token_input)
            .password(true)
            .hint_text(t("свой токен: github_pat_…"))
            .desired_width(ui.available_width() - buttons);
        ui.add(edit);
        let has_input = !app.token_input.trim().is_empty();
        ui.add_enabled_ui(has_input, |ui| {
            if w::button(ui, Kind::Secondary, Some(Icon::Lock), t("Сохранить")).clicked() {
                let token = std::mem::take(&mut app.token_input);
                app.set_token(Some(token));
            }
        });
        if app.gh_auth.as_ref().is_some_and(|a| a.source == TokenSource::Keyring)
            && w::button(ui, Kind::Ghost, None, t("Забыть")).clicked()
        {
            app.set_token(None);
        }
    });
    ui.label(
        RichText::new(t("Хватит токена только на чтение (Contents, Actions, Metadata). Он хранится в хранилище учётных данных Windows и уходит только на api.github.com."))
            .size(13.0)
            .color(p.weak),
    );
}

fn auth_line(ui: &mut Ui, auth: &Auth) {
    let p = Palette::of(ui);
    ui.horizontal_wrapped(|ui| {
        let (tone, text) = match (&auth.error, auth.source) {
            (Some(e), _) => (Tone::Danger, format!("{}: {e}", t("Токен не подошёл"))),
            (None, TokenSource::None) => {
                (Tone::Warning, t("Без токена: видны только публичные репозитории, 60 запросов в час").to_owned())
            }
            (None, source) => {
                let from = if source == TokenSource::Keyring { t("свой токен") } else { t("токен git") };
                let who = auth.login.as_deref().unwrap_or("?");
                (Tone::Success, format!("{from} · {} {who}", t("вход как")))
            }
        };
        w::dot(ui, tone);
        ui.label(RichText::new(text).color(p.text));
        if let Some(left) = auth.remaining {
            w::note(ui, format!("· {} {left}", t("запросов осталось")));
        }
    });
}

/// Нужно ли показать CI как упавший в списке проектов.
pub fn failed(remote: Option<&Remote>) -> bool {
    remote.and_then(Remote::latest).is_some_and(|r: &CiRun| r.state == RunState::Failure)
}
