//! Карточка выбранного проекта: заголовок, git, версия, бинарники, вкладки.

use std::path::{Path, PathBuf};

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Palette, Tone, semibold};
use eframe::egui::{self, RichText, Ui};

use crate::app::{App, Tab};
use crate::git::{Change, GitState};
use crate::i18n::{self, t};
use crate::open;
use crate::worker::Project;

/// Что попросили сделать из карточки: выполняется после отрисовки, когда `app` снова свободен.
enum Action {
    Folder,
    Terminal,
    Code(PathBuf),
    Url(String),
    Fetch,
    Hide,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(project) = app.current().cloned() else {
        empty(app, ui);
        return;
    };
    let mut actions = Vec::new();
    header(ui, &project, &mut actions);
    ui.add_space(18.0);
    problems(ui, &project);

    ui.columns(3, |cols| {
        git_card(&mut cols[0], &project);
        version_card(&mut cols[1], &project);
        bins_card(&mut cols[2], app, &project);
    });

    ui.add_space(18.0);
    let changes = project.git().map_or(0, |g| g.changes.len());
    let changes_label = if changes > 0 {
        format!("{} · {changes}", t("Изменения"))
    } else {
        t("Изменения").to_owned()
    };
    let tabs = [t("Коммиты"), changes_label.as_str(), t("Заметки")];
    let mut index = match app.tab {
        Tab::Commits => 0,
        Tab::Changes => 1,
        Tab::Notes => 2,
    };
    w::tabs(ui, &mut index, &tabs);
    app.tab = [Tab::Commits, Tab::Changes, Tab::Notes][index];
    ui.add_space(8.0);
    match app.tab {
        Tab::Commits => commits(ui, &project),
        Tab::Changes => changes_list(ui, &project),
        Tab::Notes => notes(ui, &project, &mut actions),
    }

    for action in actions {
        run(app, ui.ctx(), &project.path, action);
    }
}

fn run(app: &mut App, ctx: &egui::Context, dir: &Path, action: Action) {
    match action {
        Action::Folder => app.report(open::folder(dir)),
        Action::Terminal => app.report(open::terminal(dir)),
        Action::Code(target) => app.report(open::code(&target, dir)),
        Action::Url(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        Action::Fetch => app.fetch(),
        Action::Hide => app.hide(dir),
    }
}

fn empty(app: &mut App, ui: &mut Ui) {
    if app.scanning {
        w::empty_state(ui, Icon::Search, t("Ищу проекты…"), t("Смотрю папки из настроек."));
        return;
    }
    w::card(ui, |ui| {
        w::empty_state(
            ui,
            Icon::Folder,
            t("Проектов не видно"),
            t("Добавьте папку, в которой лежат проекты на Rust: Anvil найдёт в ней всё, где есть Cargo.toml."),
        );
        ui.vertical_centered(|ui| {
            if w::button(ui, anvil_ui::Kind::Primary, Some(Icon::Plus), t("Добавить папку…")).clicked() {
                super::settings::add_root(app);
            }
            ui.add_space(16.0);
        });
    });
}

fn header(ui: &mut Ui, project: &Project, actions: &mut Vec<Action>) {
    let p = Palette::of(ui);
    let meta = project.meta();
    let git = project.git();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(project.name()).font(semibold(24.0)).color(p.text));
                ui.add_space(6.0);
                if let Some(version) = meta.and_then(|m| m.version.as_deref()) {
                    w::badge(ui, &format!("v{version}"), Tone::Neutral);
                }
                if let Some(git) = git {
                    match (&git.last_tag, git.since_tag) {
                        (Some(_), 0) => {}
                        (Some(tag), n) => {
                            let text = format!("{tag} + {}", i18n::count(n as usize, COMMITS_RU, COMMITS_EN));
                            w::badge(ui, &text, Tone::Accent);
                        }
                        (None, _) => {
                            w::badge(ui, t("нет тегов"), Tone::Neutral);
                        }
                    }
                    if git.dirty() {
                        w::badge(ui, t("есть правки"), Tone::Warning);
                    }
                }
            });
            ui.horizontal(|ui| {
                if let Some(description) = meta.and_then(|m| m.description.as_deref()) {
                    w::note(ui, description);
                    w::note(ui, "·");
                }
                w::mono(ui, &project.path.to_string_lossy(), None);
            });
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let more = w::icon_button(ui, Icon::More, t("Ещё"));
            w::menu(&more, 250.0, |ui| more_menu(ui, project, actions));
            if w::icon_button(ui, Icon::Code, t("Открыть в VS Code")).clicked() {
                actions.push(Action::Code(project.path.clone()));
            }
            if w::icon_button(ui, Icon::Terminal, t("Терминал в папке")).clicked() {
                actions.push(Action::Terminal);
            }
            if w::icon_button(ui, Icon::Folder, t("Открыть папку")).clicked() {
                actions.push(Action::Folder);
            }
        });
    });
}

fn more_menu(ui: &mut Ui, project: &Project, actions: &mut Vec<Action>) {
    if let Some(url) = project.git().and_then(GitState::github) {
        if w::menu_item(ui, Some(Icon::Code), t("Репозиторий на GitHub"), None).clicked() {
            actions.push(Action::Url(url.clone()));
        }
        if w::menu_item(ui, Some(Icon::Play), "Actions", None).clicked() {
            actions.push(Action::Url(format!("{url}/actions")));
        }
        if w::menu_item(ui, Some(Icon::Package), "Releases", None).clicked() {
            actions.push(Action::Url(format!("{url}/releases")));
        }
        w::menu_separator(ui);
    }
    let has_origin = project.git().is_some_and(|g| g.remote.is_some());
    ui.add_enabled_ui(has_origin, |ui| {
        if w::menu_item(ui, Some(Icon::Download), t("Спросить origin"), Some("F5")).clicked() {
            actions.push(Action::Fetch);
        }
    });
    w::menu_separator(ui);
    if w::menu_item(ui, Some(Icon::Close), t("Скрыть из списка"), None).clicked() {
        actions.push(Action::Hide);
    }
}

/// Ошибки чтения — баннером над карточками: без них состояние было бы неполным молча.
fn problems(ui: &mut Ui, project: &Project) {
    let mut shown = false;
    if let Err(e) = &project.git {
        w::banner(ui, Tone::Danger, t("git не читается"), e, |_| {});
        shown = true;
    }
    if let Some(Err(e)) = &project.meta {
        if shown {
            ui.add_space(6.0);
        }
        w::banner(ui, Tone::Danger, t("cargo metadata не удался"), e, |_| {});
        shown = true;
    }
    if let Some(git) = project.git()
        && git.behind > 0
    {
        let text = format!(
            "{} {}",
            i18n::count(git.behind as usize, COMMITS_RU, COMMITS_EN),
            t("ещё не у вас — нужен git pull")
        );
        w::banner(ui, Tone::Warning, t("На origin есть новое"), &text, |_| {});
        shown = true;
    }
    if shown {
        ui.add_space(14.0);
    }
}

const COMMITS_RU: [&str; 3] = ["коммит", "коммита", "коммитов"];
const COMMITS_EN: [&str; 2] = ["commit", "commits"];

fn git_card(ui: &mut Ui, project: &Project) {
    let p = Palette::of(ui);
    w::card(ui, |ui| {
        w::card_title(ui, Icon::Branch, "Git");
        let Some(git) = project.git() else {
            w::note(
                ui,
                if project.git.is_err() {
                    t("Состояние не прочитано.")
                } else {
                    t("Папка не под git.")
                },
            );
            return;
        };
        w::field_row(ui, t("Ветка"), |ui| match &git.branch {
            Some(branch) => {
                w::mono(ui, branch, Some(p.text));
            }
            None => {
                w::badge(ui, t("отсоединённый HEAD"), Tone::Warning);
            }
        });
        w::field_row(ui, t("Дерево"), |ui| {
            if git.dirty() {
                w::dot(ui, Tone::Warning);
                ui.label(i18n::count(
                    git.changes.len(),
                    ["изменение", "изменения", "изменений"],
                    ["change", "changes"],
                ));
            } else {
                w::dot(ui, Tone::Success);
                ui.label(t("чисто"));
            }
        });
        w::field_row(ui, "origin", |ui| match &git.upstream {
            Some(_) if git.ahead == 0 && git.behind == 0 => {
                w::dot(ui, Tone::Success);
                ui.label(t("совпадает"));
            }
            Some(_) => {
                ui.label(format!("↑{} ↓{}", git.ahead, git.behind));
            }
            None => {
                w::note(ui, t("ветка не связана с origin"));
            }
        });
        w::field_row(ui, t("Проверено"), |ui| {
            w::note(ui, git.fetched_at.map(i18n::ago).unwrap_or_else(|| t("ни разу").to_owned()));
        });
        if let Some(commit) = git.commits.first() {
            w::field_row(ui, t("Последний"), |ui| {
                w::mono(ui, &commit.hash, Some(p.accent_text));
                w::note(ui, i18n::ago(commit.time));
            });
        }
    });
}

fn version_card(ui: &mut Ui, project: &Project) {
    let p = Palette::of(ui);
    w::card(ui, |ui| {
        w::card_title(ui, Icon::Package, t("Версия"));
        let meta = match &project.meta {
            None => {
                ui.horizontal(|ui| {
                    w::spinner(ui, 14.0);
                    w::note(ui, t("Читаю Cargo.toml…"));
                });
                return;
            }
            Some(Err(_)) => {
                w::note(ui, t("Cargo.toml не прочитан."));
                return;
            }
            Some(Ok(meta)) => meta,
        };
        w::field_row(ui, "Cargo", |ui| {
            w::mono(ui, meta.version.as_deref().unwrap_or("—"), Some(p.text));
        });
        if let Some(git) = project.git() {
            w::field_row(ui, t("Тег"), |ui| match &git.last_tag {
                Some(tag) => {
                    w::mono(ui, tag, Some(p.text));
                    if git.since_tag > 0 {
                        w::note(ui, format!("+{}", i18n::count(git.since_tag as usize, COMMITS_RU, COMMITS_EN)));
                    }
                }
                None => {
                    w::note(ui, t("нет"));
                }
            });
        }
        if meta.packages > 1 {
            w::field_row(ui, t("Крейтов"), |ui| {
                ui.label(meta.packages.to_string());
            });
        }
        let link = project.git().and_then(GitState::github).or_else(|| meta.repository.clone());
        if let Some(url) = link {
            w::field_row(ui, t("Код"), |ui| {
                let short = url.trim_start_matches("https://").trim_start_matches("github.com/");
                ui.hyperlink_to(RichText::new(short).size(13.0), url.as_str());
            });
        }
    });
}

fn bins_card(ui: &mut Ui, app: &App, project: &Project) {
    let p = Palette::of(ui);
    w::card(ui, |ui| {
        w::card_title(ui, Icon::Terminal, t("Бинарники"));
        let Some(meta) = project.meta() else {
            w::note(ui, "—");
            return;
        };
        if meta.bins.is_empty() {
            w::note(ui, t("Библиотека: запускать нечего."));
            return;
        }
        for bin in &meta.bins {
            let running = app.running(&bin.name);
            ui.horizontal(|ui| {
                w::dot(ui, if running.is_empty() { Tone::Neutral } else { Tone::Success });
                let label = w::mono(ui, &bin.name, Some(p.text));
                if bin.package != bin.name {
                    label.on_hover_text(format!("{}: {}", t("пакет"), bin.package));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(first) = running.first() {
                        let where_ = first.path.as_ref().map(|path| place(path, &project.path)).unwrap_or_default();
                        let text = if running.len() > 1 {
                            format!("{}: {}", t("запущено экземпляров"), running.len())
                        } else {
                            format!("{} · PID {}", t("запущен"), first.pid)
                        };
                        let r = ui.label(RichText::new(text).size(13.0).color(p.success));
                        if !where_.is_empty() {
                            r.on_hover_text(where_);
                        }
                    }
                });
            });
        }
    });
}

/// Откуда запущен exe — коротко, если он внутри проекта: `target\release\amber-desktop.exe`.
fn place(exe: &Path, project: &Path) -> String {
    let text = exe.to_string_lossy();
    let root = project.to_string_lossy();
    if text.to_lowercase().starts_with(&root.to_lowercase()) {
        text[root.len()..].trim_start_matches(['\\', '/']).to_owned()
    } else {
        text.into_owned()
    }
}

fn commits(ui: &mut Ui, project: &Project) {
    let p = Palette::of(ui);
    let Some(git) = project.git().filter(|g| !g.commits.is_empty()) else {
        w::card(ui, |ui| {
            w::empty_state(ui, Icon::Branch, t("Коммитов нет"), t("История появится после первого коммита."))
        });
        return;
    };
    w::card(ui, |ui| {
        for (i, commit) in git.commits.iter().enumerate() {
            if i > 0 {
                w::divider(ui);
            }
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                w::mono(ui, &commit.hash, Some(p.accent_text));
                ui.add_space(6.0);
                let width = (ui.available_width() - 150.0).max(80.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| ui.add(egui::Label::new(&commit.subject).truncate()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    w::note(ui, i18n::ago(commit.time)).on_hover_text(&commit.author);
                });
            });
        }
    });
}

fn changes_list(ui: &mut Ui, project: &Project) {
    let Some(git) = project.git().filter(|g| g.dirty()) else {
        w::card(ui, |ui| w::empty_state(ui, Icon::Check, t("Всё закоммичено"), t("В рабочей копии нет изменений.")));
        return;
    };
    let p = Palette::of(ui);
    w::card(ui, |ui| {
        for (i, Change { kind, path }) in git.changes.iter().enumerate() {
            if i > 0 {
                w::divider(ui);
            }
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                let (label, tone) = match kind {
                    'A' => ("A", Tone::Success),
                    'D' => ("D", Tone::Danger),
                    'R' => ("R", Tone::Accent),
                    '?' => ("?", Tone::Neutral),
                    'U' => ("U", Tone::Danger),
                    _ => ("M", Tone::Warning),
                };
                // Ячейка одной ширины: пути в столбик, какой бы ни была буква.
                ui.allocate_ui_with_layout(
                    egui::vec2(24.0, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(24.0);
                        w::badge(ui, label, tone).on_hover_text(change_hint(*kind));
                    },
                );
                ui.add_space(4.0);
                w::mono(ui, path, Some(p.text));
            });
        }
    });
}

fn change_hint(kind: char) -> &'static str {
    match kind {
        'A' => t("добавлен"),
        'D' => t("удалён"),
        'R' => t("переименован"),
        '?' => t("не отслеживается"),
        'U' => t("конфликт слияния"),
        _ => t("изменён"),
    }
}

fn notes(ui: &mut Ui, project: &Project, actions: &mut Vec<Action>) {
    if project.notes.is_empty() {
        w::card(ui, |ui| {
            w::empty_state(
                ui,
                Icon::Pencil,
                t("Заметок нет"),
                t("Здесь появятся HANDOFF.md, TODO.md, SPEC.md и README.md проекта."),
            )
        });
        return;
    }
    let p = Palette::of(ui);
    w::card(ui, |ui| {
        for (i, (name, modified)) in project.notes.iter().enumerate() {
            if i > 0 {
                w::divider(ui);
            }
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                anvil_ui::icons::paint(ui.painter(), rect, Icon::File, p.weak);
                w::mono(ui, name, Some(p.text));
                w::note(ui, format!("{} {}", t("правка"), i18n::ago(*modified)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::button(ui, anvil_ui::Kind::Ghost, Some(Icon::Code), t("Открыть")).clicked() {
                        actions.push(Action::Code(project.path.join(name)));
                    }
                });
            });
        }
    });
}
