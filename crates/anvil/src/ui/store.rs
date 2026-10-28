//! «Магазин программ»: программы из выпусков репозиториев аккаунта и подключённых проектов — поставить, обновить, запустить, удалить.

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::github::Repo;
use crate::i18n::{self, t};
use crate::store::{Filter, Program, State};

/// Ход установки программы в карточке.
enum Job {
    None,
    Running(Option<f32>),
    Failed,
}

struct Entry {
    program: Program,
    description: Option<String>,
    private: bool,
    tool: bool,
    state: State,
    job: Job,
}

/// Репозиторий без программ в выпусках.
struct Empty {
    repo: Repo,
    description: Option<String>,
    error: Option<String>,
    /// Проект на диске и программа в нём: собрать и поставить из кода.
    local: Option<(PathBuf, String)>,
}

enum Action {
    Install(Program),
    Launch(String),
    Uninstall(String),
    Open(Option<String>),
    UpdateAll(Vec<Program>),
    Refresh,
    Url(String),
    Local(PathBuf, String),
    ToggleEmpty,
}

fn entries(app: &App) -> (Vec<Entry>, Vec<Empty>) {
    let prerelease = app.config.common.prerelease;
    let mut entries = Vec::new();
    let mut empty = Vec::new();
    for info in app.store_repos() {
        let key = info.repo.slug().to_lowercase();
        let programs = match app.store.releases.get(&key) {
            Some(Ok(releases)) => crate::store::programs(&info.repo, releases, prerelease),
            Some(Err(e)) => {
                empty.push(Empty {
                    repo: info.repo,
                    description: info.description,
                    error: Some(e.clone()),
                    local: None,
                });
                continue;
            }
            None => continue,
        };
        if programs.is_empty() {
            let local = info.project.as_ref().and_then(|path| {
                let project = app.projects.iter().find(|p| &p.path == path)?;
                let bin = project.meta()?.bins.iter().find(|b| !b.hint.hidden)?;
                Some((path.clone(), bin.name.clone()))
            });
            empty.push(Empty { repo: info.repo, description: info.description, error: None, local });
            continue;
        }
        for program in programs {
            let installed = app.installs.get(&program.bin).and_then(Option::as_ref).and_then(|i| i.current.as_deref());
            let tool = info
                .project
                .as_ref()
                .and_then(|path| app.projects.iter().find(|p| &p.path == path))
                .and_then(|p| p.meta())
                .and_then(|m| m.bins.iter().find(|b| b.name == program.bin))
                .is_some_and(|b| crate::deck::role(b) == crate::deck::Role::Tool);
            entries.push(Entry {
                state: crate::store::state(installed, &program.version),
                job: job_of(app, &program.bin),
                description: info.description.clone(),
                private: info.private,
                tool,
                program,
            });
        }
    }
    entries.sort_by_key(|e| e.program.bin.to_lowercase());
    (entries, empty)
}

fn job_of(app: &App, bin: &str) -> Job {
    let Some(job) = app.store.jobs.get(bin).and_then(|id| app.jobs.iter().find(|j| j.id == *id)) else {
        return Job::None;
    };
    match &job.finished {
        None => Job::Running(job.progress()),
        Some((outcome, _)) if !outcome.ok => Job::Failed,
        Some(_) => Job::None,
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let (all, empty) = entries(app);
    let updates: Vec<Program> = all
        .iter()
        .filter(|e| matches!(e.state, State::Update(_)) && matches!(e.job, Job::None))
        .map(|e| e.program.clone())
        .collect();
    let mut action = None;
    ui.horizontal(|ui| {
        w::title(ui, t("Магазин программ"), 22.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !updates.is_empty() {
                let text = format!("{} · {}", t("Обновить все"), updates.len());
                if w::button(ui, Kind::Primary, Some(Icon::Download), &text).clicked() {
                    action = Some(Action::UpdateAll(updates.clone()));
                }
            }
            ui.add_enabled_ui(!app.store.loading, |ui| {
                if w::icon_button(ui, Icon::Refresh, t("Перечитать выпуски")).clicked() {
                    action = Some(Action::Refresh);
                }
            });
            if app.store.loading {
                w::spinner(ui, 14.0);
            }
        });
    });
    let login = app.gh_auth.as_ref().and_then(|a| a.login.clone());
    let read = if app.store.read > 0 {
        format!(" · {} {}", t("выпуски прочитаны"), i18n::ago(app.store.read))
    } else {
        String::new()
    };
    w::note(
        ui,
        format!(
            "{}{read}",
            login.map_or_else(|| t("Подключённые проекты").to_owned(), |l| format!("{} {l}", t("Репозитории")))
        ),
    );
    if login_missing(app) {
        w::note(
            ui,
            t("Войдите в GitHub (токен — в Настройки → Кузница): без входа видны только подключённые проекты."),
        );
    }
    ui.add_space(10.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if let Some(key) = app.store.open.clone()
            && let Some(entry) = all.iter().find(|e| e.program.key() == key)
        {
            detail(ui, entry, &p, &mut action);
            return;
        }
        app.store.open = None;
        ui.horizontal(|ui| {
            w::search_field(ui, &mut app.store.query, t("Найти программу"), None, 260.0);
            w::segmented(
                ui,
                &mut app.store.filter,
                &[
                    (Filter::All, None, t("Все")),
                    (Filter::Installed, None, t("Установленные")),
                    (Filter::Updates, None, t("Обновления")),
                ],
            );
        });
        ui.add_space(8.0);
        let needle = app.store.query.trim().to_lowercase();
        let shown: Vec<&Entry> = all
            .iter()
            .filter(|e| match app.store.filter {
                Filter::All => true,
                Filter::Installed => !matches!(e.state, State::Fresh),
                Filter::Updates => matches!(e.state, State::Update(_)),
            })
            .filter(|e| needle.is_empty() || e.program.bin.to_lowercase().contains(&needle))
            .collect();
        if shown.is_empty() {
            if app.store.loading && all.is_empty() {
                ui.horizontal(|ui| {
                    w::spinner(ui, 14.0);
                    w::note(ui, t("Читаю выпуски…"));
                });
            } else {
                w::empty_state(
                    ui,
                    Icon::Package,
                    t("Программ нет"),
                    t("Здесь появятся программы из выпусков ваших репозиториев: архив по соглашению и SHA256SUMS."),
                );
            }
        }
        for pair in shown.chunks(2) {
            ui.columns(2, |cols| {
                for (col, entry) in cols.iter_mut().zip(pair) {
                    card(col, entry, &p, &mut action);
                }
            });
            ui.add_space(8.0);
        }
        if !empty.is_empty() {
            ui.add_space(6.0);
            let label = format!(
                "{} · {}",
                if app.store.show_empty {
                    t("Скрыть без выпусков")
                } else {
                    t("Показать без выпусков")
                },
                empty.len()
            );
            if w::button(ui, Kind::Ghost, None, &label).clicked() {
                action = Some(Action::ToggleEmpty);
            }
            if app.store.show_empty {
                ui.add_space(4.0);
                for repo in &empty {
                    empty_row(ui, repo, &p, &mut action);
                }
            }
        }
    });
    run(app, ui.ctx(), action);
}

fn login_missing(app: &App) -> bool {
    !matches!(&app.sources.account, Some(Ok(_))) && !app.sources.loading
}

fn state_badge(ui: &mut Ui, entry: &Entry) {
    match &entry.state {
        State::Update(label) => w::badge(ui, &format!("{label} → {}", entry.program.version), Tone::Warning),
        State::Current(label) => w::badge(ui, &format!("{} {label}", t("Установлена")), Tone::Success),
        State::Fresh => w::badge(ui, &entry.program.version.to_string(), Tone::Neutral),
    };
}

/// Главное действие карточки: ход установки, повтор, обновить, запустить или поставить.
fn main_action(ui: &mut Ui, entry: &Entry, action: &mut Option<Action>) {
    match entry.job {
        Job::Running(fraction) => {
            w::progress(ui, fraction, 140.0);
            w::note(ui, fraction.map_or_else(|| t("Ставлю…").to_owned(), |f| format!("{:.0} %", f * 100.0)));
            return;
        }
        Job::Failed => {
            if w::button(ui, Kind::Secondary, Some(Icon::Refresh), t("Повторить")).clicked() {
                *action = Some(Action::Install(entry.program.clone()));
            }
            return;
        }
        Job::None => {}
    }
    match entry.state {
        State::Update(_) => {
            if w::button(ui, Kind::Secondary, Some(Icon::Download), t("Обновить")).clicked() {
                *action = Some(Action::Install(entry.program.clone()));
            }
        }
        State::Current(_) => {
            if w::button(ui, Kind::Ghost, Some(Icon::Play), t("Запустить")).clicked() {
                *action = Some(Action::Launch(entry.program.bin.clone()));
            }
        }
        State::Fresh => {
            if w::button(ui, Kind::Secondary, Some(Icon::Download), t("Установить")).clicked() {
                *action = Some(Action::Install(entry.program.clone()));
            }
        }
    }
}

/// Имя программы: совпадает с репозиторием — как у репозитория (`FFMincer`), иначе по правилу Пульта.
fn title(entry: &Entry) -> String {
    if entry.program.repo.name.eq_ignore_ascii_case(&entry.program.bin) {
        entry.program.repo.name.clone()
    } else {
        crate::installs::display_name(&entry.program.bin)
    }
}

fn card(ui: &mut Ui, entry: &Entry, p: &Palette, action: &mut Option<Action>) {
    w::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            w::item_mark(ui, None, Icon::Package, 34.0);
            let name = ui
                .add(
                    egui::Label::new(RichText::new(title(entry)).font(anvil_ui::semibold(14.5)).color(p.text))
                        .sense(egui::Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(t("Сведения"));
            if name.clicked() {
                *action = Some(Action::Open(Some(entry.program.key())));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                state_badge(ui, entry);
                if entry.tool {
                    w::badge(ui, t("инструмент"), Tone::Neutral);
                }
            });
        });
        let text = entry.description.as_deref().unwrap_or("");
        ui.add(
            egui::Label::new(
                RichText::new(if text.is_empty() { entry.program.repo.slug() } else { text.to_owned() })
                    .size(12.5)
                    .color(p.weak),
            )
            .truncate(),
        );
        if matches!(entry.job, Job::Failed) {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                anvil_ui::icons::paint(ui.painter(), rect, Icon::Warning, p.danger);
                ui.label(RichText::new(t("Не поставилось: смотрите журнал задач")).size(12.5).color(p.danger));
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| main_action(ui, entry, action));
    });
}

fn detail(ui: &mut Ui, entry: &Entry, p: &Palette, action: &mut Option<Action>) {
    if w::button(ui, Kind::Ghost, Some(Icon::ArrowLeft), t("К списку")).clicked() {
        *action = Some(Action::Open(None));
    }
    ui.add_space(8.0);
    w::card(ui, |ui| {
        ui.horizontal(|ui| {
            w::item_mark(ui, None, Icon::Package, 44.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title(entry)).font(anvil_ui::semibold(17.0)).color(p.text));
                w::note(ui, entry.description.clone().unwrap_or_else(|| entry.program.repo.slug()));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| main_action(ui, entry, action));
        });
        ui.add_space(10.0);
        let installed = match &entry.state {
            State::Fresh => t("не установлена").to_owned(),
            State::Update(label) | State::Current(label) => label.clone(),
        };
        w::field_row(ui, t("Установлена"), |ui| {
            w::mono(ui, &installed, Some(p.text));
        });
        w::field_row(ui, t("Последний выпуск"), |ui| {
            w::mono(ui, &entry.program.version.to_string(), Some(p.accent_text));
            if entry.program.release.published > 0 {
                w::note(ui, i18n::ago(entry.program.release.published));
            }
        });
        w::field_row(ui, t("Размер"), |ui| {
            w::note(ui, super::resources::bytes_text(entry.program.size));
        });
        w::field_row(ui, t("Репозиторий"), |ui| {
            w::mono(ui, &entry.program.repo.slug(), Some(p.text));
            if entry.private {
                w::badge(ui, t("закрытый"), Tone::Neutral);
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            anvil_ui::icons::paint(ui.painter(), rect, Icon::Check, p.accent_text);
            w::note(ui, t("Контрольная сумма SHA-256 сверяется с SHA256SUMS выпуска перед установкой."));
        });
        w::note(ui, t("Данные программы при удалении не трогаются."));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if w::button(ui, Kind::Ghost, Some(Icon::File), t("Заметки выпуска")).clicked() {
                *action = Some(Action::Url(entry.program.release.url.clone()));
            }
            if w::button(ui, Kind::Ghost, Some(Icon::External), t("Страница на GitHub")).clicked() {
                *action = Some(Action::Url(format!("https://github.com/{}", entry.program.repo.slug())));
            }
            if !matches!(entry.state, State::Fresh) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::button(ui, Kind::Danger, Some(Icon::Trash), t("Удалить")).clicked() {
                        *action = Some(Action::Uninstall(entry.program.bin.clone()));
                    }
                });
            }
        });
    });
}

fn empty_row(ui: &mut Ui, entry: &Empty, p: &Palette, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(entry.repo.slug()).size(13.0).color(p.text));
        if let Some(error) = &entry.error {
            w::badge(ui, error, Tone::Danger);
        } else {
            w::badge(ui, t("нет выпусков"), Tone::Neutral);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some((path, bin)) = &entry.local
                && w::button(ui, Kind::Ghost, Some(Icon::Hammer), t("Собрать и поставить")).clicked()
            {
                *action = Some(Action::Local(path.clone(), bin.clone()));
            }
            if let Some(description) = &entry.description {
                ui.add(egui::Label::new(RichText::new(description).size(12.0).color(p.weak)).truncate());
            }
        });
    });
}

fn run(app: &mut App, ctx: &egui::Context, action: Option<Action>) {
    match action {
        Some(Action::Install(program)) => app.store_install(&program),
        Some(Action::UpdateAll(programs)) => {
            for program in &programs {
                app.store_install(program);
            }
        }
        Some(Action::Launch(bin)) => {
            app.launch_installed(&bin);
        }
        Some(Action::Uninstall(bin)) => app.uninstall_confirm = Some(bin),
        Some(Action::Open(key)) => app.store.open = key,
        Some(Action::Refresh) => app.refresh_store(),
        Some(Action::Url(url)) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        Some(Action::Local(path, bin)) => app.install_local(&path, &bin),
        Some(Action::ToggleEmpty) => app.store.show_empty = !app.store.show_empty,
        None => {}
    }
}
