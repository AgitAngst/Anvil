//! Страж правил в окне: вкладка проекта «Правила» и настройки.

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::guard::{self, Report, Severity};
use crate::i18n::{self, t};
use crate::worker::Project;

pub enum Action {
    Check(PathBuf),
    OpenWords,
}

pub fn run(app: &mut App, action: Action) {
    match action {
        Action::Check(path) => app.check_guard(vec![path]),
        Action::OpenWords => {
            let path = guard::words_path(&app.config_path);
            let dir = path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
            match guard::ensure_words_file(&path) {
                Ok(()) => app.report(crate::open::code(&path, &dir)),
                Err(e) => app.toasts.push(format!("{}: {e}", t("Файл слов не создан")), Tone::Danger),
            }
        }
    }
}

/// Подпись вкладки: число нарушений и предупреждений.
pub fn label(report: Option<&Report>) -> String {
    match report.map(Report::attention) {
        Some(n) if n > 0 => format!("{} · {n}", t("Правила")),
        _ => t("Правила").to_owned(),
    }
}

pub fn tone(severity: Severity) -> Tone {
    match severity {
        Severity::Violation => Tone::Danger,
        Severity::Warning => Tone::Warning,
        Severity::Info => Tone::Neutral,
    }
}

/// Заголовок и пояснение найденного.
fn describe(kind: &guard::Kind) -> (String, String) {
    use guard::Kind as K;
    match kind {
        K::VersionInvalid(v) => (t("Версия не по схеме").to_owned(), format!("{v} — {}", t("нужно X.Y.Z"))),
        K::VersionTooHigh(v) => (t("После 0.9.x идёт 1.0.0").to_owned(), v.clone()),
        K::DocsStale(n) => (
            t("Документация отстала от кода").to_owned(),
            format!(
                "{} {}",
                i18n::count(*n as usize, ["коммит", "коммита", "коммитов"], ["commit", "commits"]),
                t("кода после последней правки docs/")
            ),
        ),
        K::NoDocs => (
            t("Нет docs/README.md").to_owned(),
            t("Проект настроен (есть openspec/), а оглавления документации нет").to_owned(),
        ),
        K::NoClaude => (
            t("Нет CLAUDE.md").to_owned(),
            t("Проект настроен (есть openspec/), а правил проекта для агентов нет").to_owned(),
        ),
        K::PublicWord(word, place) => {
            let place = match place.strip_prefix("commit ") {
                Some(hash) => format!("{} {hash}", t("коммит")),
                None => place.clone(),
            };
            (t("Слово из списка в публичном репозитории").to_owned(), format!("{word} — {place}"))
        }
        K::PublicMore(n) => (t("Ещё места со словами из списка").to_owned(), n.to_string()),
        K::NoWords => (
            t("Список слов пуст").to_owned(),
            t("Репозиторий публичный, а проверять нечем: добавьте слова в файл слов (Настройки → Кузница)").to_owned(),
        ),
        K::VisibilityUnknown => (
            t("Видимость репозитория не известна").to_owned(),
            t("Нужен вход в GitHub или выбор «Публичный» или «Закрытый» в карточке Git").to_owned(),
        ),
        K::BuildOutsideE(path) => (t("Сборка не на диске E:").to_owned(), path.to_string_lossy().into_owned()),
        K::CiTriggers(file, events) => (
            t("CI закрытого репозитория запускается сам").to_owned(),
            format!("{file}: {events} — {}", t("минуты Actions считаны, запуск должен быть по кнопке")),
        ),
    }
}

pub fn tab(app: &App, ui: &mut Ui, project: &Project) -> Vec<Action> {
    let mut actions = Vec::new();
    let p = Palette::of(ui);
    let report = app.guard.get(&project.path);
    let busy = app.guard_busy.contains(&project.path);
    ui.horizontal(|ui| {
        let when = match report {
            Some(r) => format!("{} {}", t("Проверено"), i18n::ago(r.checked)),
            None if busy => t("Проверяю…").to_owned(),
            None => t("Ещё не проверяли").to_owned(),
        };
        ui.label(RichText::new(when).size(13.0).color(p.weak));
        if busy {
            w::spinner(ui, 14.0);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                if w::button(ui, Kind::Secondary, Some(Icon::Refresh), t("Проверить")).clicked() {
                    actions.push(Action::Check(project.path.clone()));
                }
            });
        });
    });
    ui.add_space(8.0);
    let Some(report) = report else { return actions };
    if report.findings.is_empty() {
        w::empty_state(ui, Icon::Check, t("Нарушений нет"), t("Все проверки правил прошли."));
    }
    for finding in &report.findings {
        let (title, detail) = describe(finding);
        ui.horizontal(|ui| {
            w::dot(ui, tone(finding.severity()));
            ui.label(RichText::new(title).size(13.5).color(p.text));
            w::badge(ui, severity_label(finding.severity()), tone(finding.severity()));
        });
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.add(egui::Label::new(RichText::new(detail).size(12.5).color(p.weak)).wrap());
        });
        ui.add_space(6.0);
    }
    ui.add_space(6.0);
    w::note(
        ui,
        t("Слова ищутся в текстовых файлах под git и в последних 300 сообщениях коммитов; картинки смотрите глазами."),
    );
    actions
}

pub fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Violation => t("нарушение"),
        Severity::Warning => t("предупреждение"),
        Severity::Info => t("справка"),
    }
}

/// Раздел настроек «Страж правил» во вкладке «Кузница».
pub fn settings(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Страж правил"));
    ui.add_space(2.0);
    let before = (app.config.guard.enabled, app.config.guard.block_release);
    w::switch(ui, &mut app.config.guard.enabled, t("Проверять правила проектов"));
    w::switch(ui, &mut app.config.guard.block_release, t("Не давать выпускать при нарушениях"));
    let changed = before != (app.config.guard.enabled, app.config.guard.block_release);
    ui.add_space(4.0);
    let words = guard::words_path(&app.config_path);
    ui.horizontal(|ui| {
        w::mono(ui, &words.to_string_lossy(), None);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::button(ui, Kind::Secondary, Some(Icon::File), t("Открыть файл слов")).clicked() {
                run(app, Action::OpenWords);
            }
        });
    });
    w::note(
        ui,
        t("Слова, которых не должно быть в публичных репозиториях. Файл лежит рядом с anvil.toml и в git не попадает."),
    );
    if changed {
        app.save();
        if app.config.guard.enabled {
            let paths = app.visible().iter().map(|p| p.path.clone()).collect();
            app.check_guard(paths);
        }
    }
}
