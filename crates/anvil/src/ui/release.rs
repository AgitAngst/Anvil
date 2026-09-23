//! Мастер выпуска: проверки → версия → заметки → выпуск → ход. Каждый шаг виден, до
//! подтверждения ничего не меняется, а отправляется наружу только после сборки.

use std::path::{Path, PathBuf};

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone, semibold};
use anvil_update::Version;
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::github::{Repo, RunState};
use crate::i18n::{self, t};
use crate::jobs::{JobId, Step};
use crate::release::{self, Bump, FileEdit};
use crate::tasks::{self, Task};
use crate::worker::Project;

/// Как выпуск попадёт на GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// В проекте есть workflow выпуска: Anvil ставит тег, собирает и публикует CI.
    Ci,
    /// Workflow нет: Anvil собирает, упаковывает и создаёт Release сам.
    Upload,
    /// origin не на GitHub: собрать архивы по соглашению и оставить в папке.
    Local,
}

pub struct Wizard {
    pub project: PathBuf,
    pub step: usize,
    tests: Option<JobId>,
    clippy: Option<JobId>,
    bump: Option<Bump>,
    custom: String,
    notes: String,
    notes_for: String,
    plan: Option<(String, Result<Vec<FileEdit>, String>)>,
    tag_taken: Option<(String, bool)>,
    bins: Vec<(String, bool)>,
    mode: Mode,
    confirm: bool,
    job: Option<JobId>,
    tag: Option<String>,
    out: Option<PathBuf>,
    watching: bool,
}

const STEPS: usize = 5;

fn step_name(i: usize) -> &'static str {
    match i {
        0 => t("Проверки"),
        1 => t("Версия"),
        2 => t("Заметки"),
        3 => t("Выпуск"),
        _ => t("Ход"),
    }
}

impl App {
    /// Открыть мастер для проекта и сразу запустить тесты и clippy.
    pub fn open_release(&mut self, path: &Path) {
        let Some(project) = self.projects.iter().find(|p| p.path == path) else { return };
        let github = project.git().and_then(|g| g.github()).is_some();
        let mode = match (github, release::has_release_workflow(path)) {
            (true, true) => Mode::Ci,
            (true, false) => Mode::Upload,
            (false, _) => Mode::Local,
        };
        let bins = project.meta().map(|m| m.bins.iter().map(|b| (b.name.clone(), true)).collect()).unwrap_or_default();
        self.release = Some(Wizard {
            project: path.to_path_buf(),
            step: 0,
            tests: None,
            clippy: None,
            bump: Some(Bump::Patch),
            custom: String::new(),
            notes: String::new(),
            notes_for: String::new(),
            plan: None,
            tag_taken: None,
            bins,
            mode,
            confirm: false,
            job: None,
            tag: None,
            out: None,
            watching: false,
        });
        self.release_checks();
    }

    fn release_checks(&mut self) {
        let Some(path) = self.release.as_ref().map(|w| w.project.clone()) else { return };
        let tests = self.start_task(&path, Task::Test);
        let clippy = self.start_task(&path, Task::Clippy);
        if let Some(wizard) = &mut self.release {
            wizard.tests = tests;
            wizard.clippy = clippy;
        }
    }

    /// Собрать сценарий выпуска и поставить его в очередь.
    fn release_publish(&mut self, version: &Version, edits: Vec<FileEdit>) {
        let Some(wizard) = &self.release else { return };
        let Some(project) = self.projects.iter().find(|p| p.path == wizard.project) else { return };
        let Some(git) = project.git() else { return };
        let dir = project.path.clone();
        let tag = format!("v{version}");
        let target = project.meta().map(|m| m.target_dir.clone()).unwrap_or_else(|| dir.join("target"));
        let work = target.join("anvil-release");
        let out = work.join(&tag);
        let notes_file = work.join(format!("{tag}.md"));
        let branch = git.branch.clone().unwrap_or_else(|| "HEAD".into());
        let mode = wizard.mode;
        let notes = wizard.notes.clone();
        let bins: Vec<String> = wizard.bins.iter().filter(|(_, on)| *on).map(|(b, _)| b.clone()).collect();
        let packages: Vec<String> = project
            .meta()
            .map(|m| {
                let mut p: Vec<String> =
                    m.bins.iter().filter(|b| bins.contains(&b.name)).map(|b| b.package.clone()).collect();
                p.dedup();
                p
            })
            .unwrap_or_default();
        let repo = git.github().and_then(|u| u.strip_prefix("https://github.com/").map(str::to_owned));

        let s = |x: &str| x.to_owned();
        let mut steps = vec![
            Step::Write(edits.into_iter().map(|e| (e.path, e.text)).collect()),
            // Cargo.lock — за новыми версиями своих крейтов, без сети.
            Step::Run(s("cargo"), vec![s("update"), s("--workspace"), s("--offline")]),
        ];
        if mode != Mode::Ci {
            // Собрать до коммита и тега: сломается сборка — наружу ничего не уйдёт.
            let mut args = vec![s("build"), s("--release")];
            for p in &packages {
                args.extend([s("-p"), p.clone()]);
            }
            for b in &bins {
                args.extend([s("--bin"), b.clone()]);
            }
            steps.push(Step::Run(s("cargo"), args));
            let exes = bins.iter().map(|b| (b.clone(), crate::launch::exe_path(&target, true, b))).collect();
            steps.push(Step::Package { exes, version: version.clone(), out: out.clone() });
        }
        let _ = std::fs::create_dir_all(&work);
        steps.push(Step::Write(vec![(notes_file.clone(), notes.clone())]));
        steps.push(Step::Run(s("git"), vec![s("add"), s("-u")]));
        steps.push(Step::Run(s("git"), vec![s("commit"), s("-m"), format!("Release {tag}")]));
        steps.push(Step::Run(
            s("git"),
            vec![
                s("tag"),
                s("-a"),
                tag.clone(),
                s("--cleanup=verbatim"),
                s("-F"),
                notes_file.to_string_lossy().into_owned(),
            ],
        ));
        if mode != Mode::Local || git.upstream.is_some() {
            steps.push(Step::Run(s("git"), vec![s("push"), s("--atomic"), s("origin"), branch, tag.clone()]));
        }
        if mode == Mode::Upload
            && let Some(repo) = &repo
        {
            steps.push(Step::Publish {
                repo: repo.clone(),
                tag: tag.clone(),
                notes,
                prerelease: version.is_prerelease(),
                out: out.clone(),
            });
        }
        let spec = tasks::script_spec(&dir, format!("{} {tag}", t("выпуск")), steps);
        let id = self.enqueue(spec);
        if let Some(wizard) = &mut self.release {
            wizard.job = Some(id);
            wizard.tag = Some(tag);
            wizard.out = (mode != Mode::Ci).then_some(out);
            wizard.step = 4;
        }
    }
}

/// Состояние задачи для строки проверки.
fn job_check(app: &App, id: Option<JobId>) -> (Tone, String) {
    let Some(job) = id.and_then(|id| app.jobs.iter().find(|j| j.id == id)) else {
        return (Tone::Neutral, t("ждёт решения о занятом exe").to_owned());
    };
    match &job.finished {
        None => (Tone::Accent, t("идёт…").to_owned()),
        Some((o, _)) if o.ok => match o.tests {
            Some((passed, _)) => (Tone::Success, format!("{} {passed}", t("прошло тестов:"))),
            None if job.warnings() > 0 => (
                Tone::Success,
                i18n::count(
                    job.warnings(),
                    ["предупреждение", "предупреждения", "предупреждений"],
                    ["warning", "warnings"],
                ),
            ),
            None => (Tone::Success, t("чисто").to_owned()),
        },
        Some(_) => (Tone::Danger, t("не прошло — подробности в логе").to_owned()),
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(path) = app.release.as_ref().map(|w| w.project.clone()) else { return };
    let Some(project) = app.projects.iter().find(|p| p.path == path).cloned() else {
        app.release = None;
        return;
    };
    let mut open = true;
    let title = format!("{} · {}", t("Выпуск"), project.name());
    let mut action = None;
    chrome::dialog(ctx, "anvil-release", &title, 720.0, &mut open, |ui| {
        action = body(app, ui, &project);
    });
    if let Some(action) = action {
        handle(app, action);
    }
    confirm(app, ctx, &project);
    if !open {
        app.release = None;
    }
}

enum Act {
    Next,
    Back,
    Recheck,
    Fetch,
    Publish,
    Log(JobId),
    Folder(PathBuf),
    Close,
}

fn handle(app: &mut App, act: Act) {
    match act {
        Act::Next => {
            if let Some(w) = &mut app.release {
                w.step = (w.step + 1).min(3);
            }
        }
        Act::Back => {
            if let Some(w) = &mut app.release {
                w.step = w.step.saturating_sub(1);
            }
        }
        Act::Recheck => app.release_checks(),
        Act::Fetch => app.fetch(),
        Act::Publish => {
            if let Some(w) = &mut app.release {
                w.confirm = true;
            }
        }
        Act::Log(id) => {
            app.log_open = true;
            app.log_job = Some(id);
        }
        Act::Folder(dir) => app.report(crate::open::folder(&dir)),
        Act::Close => app.release = None,
    }
}

fn body(app: &mut App, ui: &mut Ui, project: &Project) -> Option<Act> {
    let step = app.release.as_ref()?.step;
    steps_header(ui, step);
    ui.add_space(12.0);
    match step {
        0 => checks(app, ui, project),
        1 => version_step(app, ui, project),
        2 => notes_step(app, ui, project),
        3 => publish_step(app, ui, project),
        _ => progress_step(app, ui, project),
    }
}

fn steps_header(ui: &mut Ui, current: usize) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        for i in 0..STEPS {
            let (tone, color) = match i.cmp(&current) {
                std::cmp::Ordering::Less => (Tone::Success, p.weak),
                std::cmp::Ordering::Equal => (Tone::Accent, p.text),
                std::cmp::Ordering::Greater => (Tone::Neutral, p.faint),
            };
            w::badge(ui, &(i + 1).to_string(), tone);
            let font = if i == current { semibold(13.5) } else { egui::FontId::proportional(13.5) };
            ui.label(RichText::new(step_name(i)).font(font).color(color));
            if i + 1 < STEPS {
                ui.label(RichText::new("›").color(p.faint));
            }
        }
    });
}

fn check_row(ui: &mut Ui, tone: Tone, title: &str, detail: &str) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        let icon = match tone {
            Tone::Success => Icon::Check,
            Tone::Danger | Tone::Warning => Icon::Warning,
            _ => Icon::Clock,
        };
        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        anvil_ui::icons::paint(ui.painter(), rect, icon, tone.color(&p));
        ui.label(RichText::new(title).color(p.text));
        // Пути и ссылки бывают длинными — обрезать, а не раздвигать окно.
        let detail_text = RichText::new(detail).size(13.0).color(p.weak);
        ui.add(egui::Label::new(detail_text).truncate()).on_hover_text(detail);
    });
}

fn footer(ui: &mut Ui, back: bool, next: Option<(&str, bool)>) -> Option<Act> {
    let mut act = None;
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some((label, enabled)) = next {
                ui.add_enabled_ui(enabled, |ui| {
                    if w::button(ui, Kind::Primary, None, label).clicked() {
                        act = Some(Act::Next);
                    }
                });
            }
            if back && w::button(ui, Kind::Ghost, None, t("Назад")).clicked() {
                act = Some(Act::Back);
            }
        });
    });
    act
}

fn checks(app: &App, ui: &mut Ui, project: &Project) -> Option<Act> {
    let w = app.release.as_ref()?;
    let git = project.git();
    let mut act = None;
    let mut all = true;

    let (tone, detail) = match git {
        Some(g) if !g.dirty() => (Tone::Success, t("незакоммиченного нет").to_owned()),
        Some(g) => (
            Tone::Danger,
            format!(
                "{} — {}",
                i18n::count(g.changes.len(), ["изменение", "изменения", "изменений"], ["change", "changes"]),
                t("закоммитьте или уберите")
            ),
        ),
        None => (Tone::Danger, t("папка не под git").to_owned()),
    };
    all &= tone == Tone::Success;
    check_row(ui, tone, t("Рабочая копия чистая"), &detail);

    let (tone, detail) = match git {
        Some(g) if g.upstream.is_none() && w.mode != Mode::Local => {
            (Tone::Danger, t("ветка не связана с origin").to_owned())
        }
        Some(g) if g.behind > 0 => (Tone::Danger, format!("{} {} — {}", t("позади на"), g.behind, t("нужен git pull"))),
        Some(g) if g.ahead > 0 => (Tone::Success, format!("{} {}", t("вместе с тегом уйдут ещё коммитов:"), g.ahead)),
        Some(_) => (Tone::Success, t("совпадает").to_owned()),
        None => (Tone::Danger, "—".to_owned()),
    };
    all &= tone == Tone::Success;
    ui.horizontal(|ui| {
        check_row(ui, tone, t("Ветка совпадает с origin"), &detail);
        if let Some(when) = git.and_then(|g| g.fetched_at) {
            w::note(ui, format!("· {} {}", t("проверено"), i18n::ago(when)));
        }
        if w::button(ui, Kind::Ghost, Some(Icon::Download), t("Спросить origin")).clicked() {
            act = Some(Act::Fetch);
        }
    });

    for (title, id) in [(t("Тесты"), w.tests), ("Clippy", w.clippy)] {
        let (tone, detail) = job_check(app, id);
        all &= tone == Tone::Success;
        ui.horizontal(|ui| {
            check_row(ui, tone, title, &detail);
            if let Some(id) = id
                && w::button(ui, Kind::Ghost, Some(Icon::Terminal), t("Лог")).clicked()
            {
                act = Some(Act::Log(id));
            }
        });
    }

    if w.mode == Mode::Upload {
        let ok =
            app.gh_auth.as_ref().is_some_and(|a| a.source != crate::github::TokenSource::None && a.error.is_none());
        let detail = if ok {
            t("выпуск создаст Anvil")
        } else {
            t("без токена GitHub выпуск не создать")
        };
        all &= ok;
        check_row(ui, if ok { Tone::Success } else { Tone::Danger }, t("Токен GitHub"), detail);
    }

    ui.add_space(8.0);
    w::note(ui, mode_text(w.mode));
    ui.horizontal(|ui| {
        if w::button(ui, Kind::Ghost, Some(Icon::Refresh), t("Проверить заново")).clicked() {
            act = Some(Act::Recheck);
        }
    });
    footer(ui, false, Some((t("Далее"), all))).or(act)
}

fn mode_text(mode: Mode) -> &'static str {
    match mode {
        Mode::Ci => t("В проекте есть workflow выпуска: Anvil поставит тег, а соберёт и опубликует CI."),
        Mode::Upload => t("Workflow выпуска нет: Anvil сам соберёт, упакует по соглашению и создаст GitHub Release."),
        Mode::Local => t("origin не на GitHub: архивы по соглашению останутся в папке сборки."),
    }
}

/// Какая будет версия и можно ли её выпустить.
fn target_version(w: &Wizard, base: &Version) -> Result<Version, String> {
    let version = match w.bump {
        Some(b) => release::bump(base, b),
        None => Version::parse(&w.custom).ok_or_else(|| t("версия — три числа: 1.2.3 или 1.2.3-beta.1").to_owned())?,
    };
    if version <= *base {
        return Err(format!("{} v{base}", t("версия должна быть больше")));
    }
    Ok(version)
}

fn version_step(app: &mut App, ui: &mut Ui, project: &Project) -> Option<Act> {
    let p = Palette::of(ui);
    let cargo = project.meta().and_then(|m| m.version.clone());
    let tag = project.git().and_then(|g| g.last_tag.clone());
    let base = release::base(cargo.as_deref(), tag.as_deref());
    let w = app.release.as_mut()?;

    ui.horizontal(|ui| {
        w::note(ui, format!("Cargo.toml: {}", cargo.as_deref().unwrap_or("—")));
        w::note(ui, format!("· {}: {}", t("последний тег"), tag.as_deref().unwrap_or("—")));
    });
    ui.add_space(6.0);
    let options: Vec<(Option<Bump>, Option<Icon>, String)> = vec![
        (Some(Bump::Patch), None, format!("Patch · {}", release::bump(&base, Bump::Patch))),
        (Some(Bump::Minor), None, format!("Minor · {}", release::bump(&base, Bump::Minor))),
        (Some(Bump::Major), None, format!("Major · {}", release::bump(&base, Bump::Major))),
        (None, None, t("Своя").to_owned()),
    ];
    let refs: Vec<(Option<Bump>, Option<Icon>, &str)> = options.iter().map(|(b, i, s)| (*b, *i, s.as_str())).collect();
    w::segmented(ui, &mut w.bump, &refs);
    if w.bump.is_none() {
        ui.add_space(4.0);
        ui.add(egui::TextEdit::singleline(&mut w.custom).hint_text("1.2.3").desired_width(160.0));
    }
    ui.add_space(10.0);

    let version = target_version(w, &base);
    let mut ok = false;
    match &version {
        Ok(v) => {
            let key = v.to_string();
            ui.label(RichText::new(format!("v{key}")).font(semibold(22.0)).color(p.text));
            if w.tag_taken.as_ref().is_none_or(|(k, _)| *k != key) {
                w.tag_taken = Some((key.clone(), release::tag_exists(&project.path, &format!("v{key}"))));
            }
            if w.plan.as_ref().is_none_or(|(k, _)| *k != key) {
                let old = cargo.clone().unwrap_or_default();
                w.plan = Some((key.clone(), release::plan_edits(&project.path, &old, &key)));
            }
            let taken = w.tag_taken.as_ref().is_some_and(|(_, t)| *t);
            if taken {
                w::banner(ui, Tone::Danger, t("Тег уже есть"), &format!("v{key}"), |_| {});
            }
            match w.plan.as_ref().map(|(_, plan)| plan) {
                Some(Ok(edits)) if !edits.is_empty() => {
                    w::section_label(ui, t("Что поменяется"));
                    for edit in edits {
                        let file = edit.path.strip_prefix(&project.path).unwrap_or(&edit.path);
                        ui.horizontal_wrapped(|ui| {
                            w::mono(ui, &file.display().to_string(), Some(p.text));
                            w::note(ui, edit.what.join(", "));
                        });
                    }
                    w::mono(ui, "Cargo.lock", Some(p.text));
                    ok = !taken;
                }
                Some(Ok(_)) => {
                    w::banner(
                        ui,
                        Tone::Danger,
                        t("Версия не найдена в Cargo.toml"),
                        cargo.as_deref().unwrap_or("—"),
                        |_| {},
                    );
                }
                Some(Err(e)) => w::banner(ui, Tone::Danger, t("Cargo.toml не прочитан"), e, |_| {}),
                None => {}
            }
        }
        Err(e) => w::banner(ui, Tone::Warning, t("Такую версию не выпустить"), e, |_| {}),
    }
    footer(ui, true, Some((t("Далее"), ok)))
}

fn notes_step(app: &mut App, ui: &mut Ui, project: &Project) -> Option<Act> {
    let cargo = project.meta().and_then(|m| m.version.clone());
    let tag = project.git().and_then(|g| g.last_tag.clone());
    let base = release::base(cargo.as_deref(), tag.as_deref());
    let w = app.release.as_mut()?;
    let version = target_version(w, &base).ok()?;
    if w.notes_for != version.to_string() {
        w.notes = release::draft_notes(&project.path, tag.as_deref(), &version);
        w.notes_for = version.to_string();
    }
    w::note(ui, t("Черновик — из коммитов после прошлого тега. Это тело GitHub Release и «Что нового» в программах."));
    ui.add_space(6.0);
    ui.add(
        egui::TextEdit::multiline(&mut w.notes)
            .font(egui::TextStyle::Monospace)
            .desired_rows(14)
            .desired_width(f32::INFINITY),
    );
    footer(ui, true, Some((t("Далее"), !w.notes.trim().is_empty())))
}

/// Что именно произойдёт — одинаково на шаге и в окне подтверждения.
fn plan_lines(w: &Wizard, project: &Project, version: &Version) -> Vec<String> {
    let tag = format!("v{version}");
    let branch = project.git().and_then(|g| g.branch.clone()).unwrap_or_else(|| "HEAD".into());
    let mut lines =
        vec![format!("{} → {version}", t("Cargo.toml: версия")), "cargo update --workspace --offline".into()];
    if w.mode != Mode::Ci {
        let bins: Vec<&str> = w.bins.iter().filter(|(_, on)| *on).map(|(b, _)| b.as_str()).collect();
        lines.push(format!("cargo build --release --bin {}", bins.join(" --bin ")));
        lines.push(format!(
            "{} + SHA256SUMS",
            bins.iter().map(|b| anvil_update::asset_name(b, version)).collect::<Vec<_>>().join(", ")
        ));
    }
    lines.push(format!("git commit -m \"Release {tag}\""));
    lines.push(format!("git tag -a {tag}"));
    if w.mode != Mode::Local || project.git().is_some_and(|g| g.upstream.is_some()) {
        lines.push(format!("git push --atomic origin {branch} {tag}"));
    }
    match w.mode {
        Mode::Ci => lines.push(t("CI соберёт и опубликует выпуск").into()),
        Mode::Upload => lines.push(t("GitHub Release с архивами и SHA256SUMS").into()),
        Mode::Local => {}
    }
    lines
}

fn publish_step(app: &mut App, ui: &mut Ui, project: &Project) -> Option<Act> {
    let p = Palette::of(ui);
    let cargo = project.meta().and_then(|m| m.version.clone());
    let tag = project.git().and_then(|g| g.last_tag.clone());
    let base = release::base(cargo.as_deref(), tag.as_deref());
    let w = app.release.as_mut()?;
    let version = target_version(w, &base).ok()?;
    w::note(ui, mode_text(w.mode));
    ui.add_space(6.0);
    if w.mode != Mode::Ci {
        w::section_label(ui, t("Что упаковать"));
        for (bin, on) in &mut w.bins {
            w::switch(ui, on, bin);
        }
        ui.add_space(6.0);
    }
    w::section_label(ui, t("По порядку"));
    for line in plan_lines(w, project, &version) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            anvil_ui::icons::paint(ui.painter(), rect, Icon::ArrowRight, p.faint);
            w::mono(ui, &line, Some(p.text));
        });
    }
    ui.add_space(6.0);
    w::note(
        ui,
        t(
            "Сборка — до коммита и тега: если она не пройдёт, наружу ничего не уйдёт, а правки Cargo.toml останутся в рабочей копии.",
        ),
    );
    let any = w.mode == Mode::Ci || w.bins.iter().any(|(_, on)| *on);
    let mut act = footer(ui, true, None);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(any, |ui| {
                let text = format!("{} v{version}…", t("Выпустить"));
                if w::button(ui, Kind::Primary, Some(Icon::Rocket), &text).clicked() {
                    act = Some(Act::Publish);
                }
            });
        });
    });
    act
}

fn confirm(app: &mut App, ctx: &egui::Context, project: &Project) {
    let Some(w) = &app.release else { return };
    if !w.confirm {
        return;
    }
    let cargo = project.meta().and_then(|m| m.version.clone());
    let tag = project.git().and_then(|g| g.last_tag.clone());
    let Ok(version) = target_version(w, &release::base(cargo.as_deref(), tag.as_deref())) else { return };
    let lines = plan_lines(w, project, &version);
    let body = |ui: &mut Ui| {
        let p = Palette::of(ui);
        for line in &lines {
            w::mono(ui, line, Some(p.text));
        }
        ui.add_space(8.0);
        w::banner(ui, Tone::Warning, t("Push не отменить."), t("Тег и выпуск увидят все."), |_| {});
    };
    let heading = format!("{} {} v{version}?", t("Выпустить"), project.name());
    match w::confirm(ctx, "anvil-release-confirm", &heading, body, t("Выпустить"), false) {
        Some(true) => {
            let edits =
                app.release.as_ref().and_then(|w| w.plan.clone()).and_then(|(_, plan)| plan.ok()).unwrap_or_default();
            if let Some(w) = &mut app.release {
                w.confirm = false;
            }
            app.release_publish(&version, edits);
        }
        Some(false) => {
            if let Some(w) = &mut app.release {
                w.confirm = false;
            }
        }
        None => {}
    }
}

fn progress_step(app: &mut App, ui: &mut Ui, project: &Project) -> Option<Act> {
    let p = Palette::of(ui);
    let w = app.release.as_ref()?;
    let (job_id, tag, mode, out) = (w.job?, w.tag.clone()?, w.mode, w.out.clone());
    let job = app.jobs.iter().find(|j| j.id == job_id)?;
    let mut act = None;
    let finished = job.finished.as_ref().map(|(o, _)| o.clone());
    ui.horizontal(|ui| {
        match &finished {
            None => {
                w::spinner(ui, 16.0);
                ui.label(RichText::new(format!("{} {tag}…", t("Выпускаю"))).color(p.text));
                w::progress(ui, job.progress(), 180.0);
            }
            Some(o) if o.ok => check_row(ui, Tone::Success, &format!("{tag} {}", t("выпущен")), ""),
            Some(_) => check_row(ui, Tone::Danger, t("Выпуск не удался"), t("подробности в логе")),
        }
        if w::button(ui, Kind::Ghost, Some(Icon::Terminal), t("Лог")).clicked() {
            act = Some(Act::Log(job_id));
        }
    });
    if let Some(line) = job.lines.iter().rev().find(|l| l.kind == crate::jobs::LineKind::Note) {
        w::mono(ui, &line.text, None);
    }

    if let Some(o) = finished.filter(|o| o.ok) {
        ui.add_space(10.0);
        match mode {
            Mode::Ci => {
                let started = app.release.as_ref().is_some_and(|w| w.watching);
                if !started && let Some(repo) = project.git().and_then(|g| g.github()).and_then(|u| Repo::from_url(&u))
                {
                    app.watch_release(&project.path, repo, tag.clone());
                    if let Some(w) = &mut app.release {
                        w.watching = true;
                    }
                }
                match app.watches.get(&project.path).filter(|w| w.tag == tag) {
                    None => {
                        ui.horizontal(|ui| {
                            w::spinner(ui, 14.0);
                            w::note(ui, t("Жду, пока GitHub запустит сборку…"));
                        });
                    }
                    Some(watch) => {
                        for run in &watch.runs {
                            ui.horizontal(|ui| {
                                w::badge(ui, super::github::run_label(run.state), super::github::run_tone(run.state));
                                ui.label(RichText::new(&run.workflow).color(p.text));
                                if w::button(ui, Kind::Ghost, Some(Icon::Code), t("Открыть")).clicked() {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(&run.url));
                                }
                            });
                        }
                        match &watch.release {
                            Some(release) if !release.assets.is_empty() => {
                                check_row(
                                    ui,
                                    Tone::Success,
                                    t("Выпуск опубликован"),
                                    &release.assets.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
                                );
                                if w::button(ui, Kind::Secondary, Some(Icon::Code), t("Открыть на GitHub")).clicked()
                                {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(&release.url));
                                }
                            }
                            _ if watch.runs.first().is_some_and(|r| r.state == RunState::Failure) => {
                                check_row(
                                    ui,
                                    Tone::Danger,
                                    t("Сборка выпуска упала"),
                                    t("тег уже на GitHub — поправьте и перезапустите workflow"),
                                );
                            }
                            _ if watch.done => {
                                check_row(ui, Tone::Warning, t("Выпуск так и не появился"), t("загляните в Actions"))
                            }
                            _ => {
                                ui.horizontal(|ui| {
                                    w::spinner(ui, 14.0);
                                    w::note(ui, t("CI собирает выпуск…"));
                                });
                            }
                        }
                    }
                }
            }
            Mode::Upload => {
                if let Some(url) = &o.published {
                    ui.horizontal(|ui| {
                        check_row(ui, Tone::Success, t("Выпуск опубликован"), url);
                        if w::button(ui, Kind::Secondary, Some(Icon::Code), t("Открыть на GitHub")).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    });
                }
            }
            Mode::Local => {
                if let Some(out) = out {
                    ui.horizontal(|ui| {
                        check_row(ui, Tone::Success, t("Архивы готовы"), &out.display().to_string());
                        if w::icon_button(ui, Icon::Folder, t("Открыть папку")).clicked() {
                            act = Some(Act::Folder(out.clone()));
                        }
                    });
                }
            }
        }
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::button(ui, Kind::Secondary, None, t("Закрыть")).clicked() {
                act = Some(Act::Close);
            }
        });
    });
    act
}
