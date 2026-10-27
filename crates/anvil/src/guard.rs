//! Страж правил: проверки правил владельца по проекту. Только читает (git и файлы), ничего не меняет.
//!
//! Правила: версия по схеме X.Y.Z (после 0.9.x — 1.0.0), документация живёт вместе с кодом, у настроенного проекта есть
//! `CLAUDE.md` и `docs/README.md`, в публичном репозитории нет слов из личного списка, сборки лежат на диске E:, у закрытого
//! репозитория CI не запускается на каждый push. Нарушения (`Violation`) могут блокировать выпуск — это настройка;
//! предупреждения (`Warning`) только видны; `Info` — справка.

use std::path::{Path, PathBuf};

use crate::run;

/// Насколько серьёзно.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Violation,
}

/// Что нашли. Текст строится в окне, чтобы он был на языке окна.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Версия не разбирается как X.Y.Z.
    VersionInvalid(String),
    /// 0.10 и дальше: после 0.9.x следующая версия — 1.0.0.
    VersionTooHigh(String),
    /// С последней правки `docs/` в коде прошло столько коммитов.
    DocsStale(u32),
    /// У настроенного проекта (есть `openspec/`) нет `docs/README.md`.
    NoDocs,
    /// У настроенного проекта нет `CLAUDE.md`.
    NoClaude,
    /// Слово из списка в публичном репозитории: слово и место (`файл:строка` или `коммит abc1234`).
    PublicWord(String, String),
    /// Нашлось ещё столько мест, которые не показаны.
    PublicMore(usize),
    /// Репозиторий публичный, а списка слов нет: проверять нечем.
    NoWords,
    /// Закрытый он или публичный — неизвестно, проверка слов пропущена.
    VisibilityUnknown,
    /// Сборка лежит не на диске E:.
    BuildOutsideE(PathBuf),
    /// CI закрытого репозитория запускается сам: файл workflow и события.
    CiTriggers(String, String),
}

impl Kind {
    pub fn severity(&self) -> Severity {
        match self {
            Kind::VersionInvalid(_) | Kind::VersionTooHigh(_) | Kind::PublicWord(..) | Kind::PublicMore(_) => {
                Severity::Violation
            }
            Kind::DocsStale(_)
            | Kind::NoDocs
            | Kind::NoClaude
            | Kind::NoWords
            | Kind::BuildOutsideE(_)
            | Kind::CiTriggers(..) => Severity::Warning,
            Kind::VisibilityUnknown => Severity::Info,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// Когда проверяли (секунды UNIX).
    pub checked: i64,
    pub findings: Vec<Kind>,
}

impl Report {
    pub fn count(&self, severity: Severity) -> usize {
        self.findings.iter().filter(|k| k.severity() == severity).count()
    }

    /// Нарушения, из-за которых можно блокировать выпуск.
    pub fn violations(&self) -> usize {
        self.count(Severity::Violation)
    }

    /// Нарушения и предупреждения — то, что видно числом на вкладке.
    pub fn attention(&self) -> usize {
        self.violations() + self.count(Severity::Warning)
    }
}

/// Всё, что нужно проверке; окно собирает это заранее.
#[derive(Debug, Clone)]
pub struct Input {
    pub path: PathBuf,
    pub version: Option<String>,
    /// Закрытый ли репозиторий (`None` — неизвестно).
    pub private: Option<bool>,
    pub target_dir: Option<PathBuf>,
    pub words: Vec<String>,
}

/// Сколько коммитов кода без правки `docs/` терпим.
const DOCS_SLACK: u32 = 15;
/// Не читать файлы больше этого при поиске слов.
const MAX_FILE: u64 = 512 * 1024;
/// Сколько найденных мест показывать.
const MAX_SHOWN: usize = 30;

pub fn check(input: &Input) -> Report {
    let mut findings = Vec::new();
    if let Some(version) = &input.version {
        findings.extend(version_finding(version));
    }
    let dir = &input.path;
    if dir.join("openspec").is_dir() {
        if !dir.join("CLAUDE.md").is_file() {
            findings.push(Kind::NoClaude);
        }
        if !dir.join("docs").join("README.md").is_file() {
            findings.push(Kind::NoDocs);
        }
    }
    if dir.join(".git").exists() {
        findings.extend(docs_stale(dir));
        match input.private {
            Some(false) if input.words.is_empty() => findings.push(Kind::NoWords),
            Some(false) => findings.extend(public_words(dir, &input.words)),
            Some(true) => findings.extend(ci_triggers(dir)),
            None => findings.push(Kind::VisibilityUnknown),
        }
    }
    if let Some(target) = &input.target_dir
        && !on_drive_e(target)
    {
        findings.push(Kind::BuildOutsideE(target.clone()));
    }
    findings.sort_by_key(|k| std::cmp::Reverse(k.severity()));
    Report { checked: crate::i18n::now(), findings }
}

/// Схема X.Y.Z: версия разбирается, и после 0.9.x не бывает 0.10.
pub fn version_finding(version: &str) -> Option<Kind> {
    let core = version.split(['+', '-']).next().unwrap_or(version);
    let parts: Vec<&str> = core.split('.').collect();
    let numbers: Option<Vec<u64>> = parts.iter().map(|p| p.parse().ok()).collect();
    match numbers.as_deref() {
        Some([major, minor, _]) if *major == 0 && *minor >= 10 => Some(Kind::VersionTooHigh(version.to_owned())),
        Some([_, _, _]) => None,
        _ => Some(Kind::VersionInvalid(version.to_owned())),
    }
}

/// Диск E: (сборки только там). Пути без буквы диска — не наше дело.
pub fn on_drive_e(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) => letter.eq_ignore_ascii_case(&'e'),
        _ => true,
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut all = vec!["-c", "core.quotepath=false"];
    all.extend_from_slice(args);
    run::output("git", dir, &all)
}

/// Сколько коммитов кода было после последней правки `docs/`; предупреждать, когда больше [`DOCS_SLACK`].
fn docs_stale(dir: &Path) -> Option<Kind> {
    if !dir.join("docs").is_dir() {
        return None;
    }
    let last = git(dir, &["log", "-1", "--format=%H", "--", "docs"]).ok()?;
    let last = last.trim();
    if last.is_empty() {
        return None;
    }
    let range = format!("{last}..HEAD");
    let count = git(dir, &["rev-list", "--count", &range, "--", ".", ":(exclude)docs", ":(exclude)*.md"]).ok()?;
    let count: u32 = count.trim().parse().ok()?;
    (count > DOCS_SLACK).then_some(Kind::DocsStale(count))
}

/// Слова из списка в отслеживаемых текстовых файлах и в сообщениях последних коммитов.
fn public_words(dir: &Path, words: &[String]) -> Vec<Kind> {
    let words: Vec<String> = words.iter().map(|w| w.to_lowercase()).filter(|w| !w.is_empty()).collect();
    let mut found: Vec<(String, String)> = Vec::new();
    if let Ok(list) = git(dir, &["ls-files"]) {
        for rel in list.lines().filter(|l| !l.is_empty()) {
            let path = dir.join(rel);
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if !meta.is_file() || meta.len() > MAX_FILE || rel.ends_with("Cargo.lock") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if bytes.iter().take(8192).any(|b| *b == 0) {
                continue;
            }
            found.extend(scan_text(rel, &String::from_utf8_lossy(&bytes), &words));
        }
    }
    if let Ok(log) = git(dir, &["log", "-n", "300", "--format=%h%x1f%s%n%b%x1e"]) {
        for entry in log.split('\x1e') {
            let Some((hash, message)) = entry.trim().split_once('\x1f') else { continue };
            let lower = message.to_lowercase();
            for word in &words {
                if lower.contains(word.as_str()) {
                    found.push((word.clone(), format!("commit {hash}")));
                }
            }
        }
    }
    let total = found.len();
    let mut kinds: Vec<Kind> = found.into_iter().take(MAX_SHOWN).map(|(w, place)| Kind::PublicWord(w, place)).collect();
    if total > MAX_SHOWN {
        kinds.push(Kind::PublicMore(total - MAX_SHOWN));
    }
    kinds
}

/// Первое место каждого слова в тексте файла: `(слово, "файл:строка")`.
pub fn scan_text(name: &str, text: &str, words: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for word in words {
        let lower_word = word.to_lowercase();
        if let Some(line) = text.lines().position(|l| l.to_lowercase().contains(&lower_word)) {
            out.push((word.clone(), format!("{name}:{}", line + 1)));
        }
    }
    out
}

/// У закрытого репозитория workflow запускается сам (`push`, `pull_request`) — минуты считаны.
fn ci_triggers(dir: &Path) -> Vec<Kind> {
    let Ok(entries) = std::fs::read_dir(dir.join(".github").join("workflows")) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "yml" || e == "yaml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let events = auto_events(&text);
        if !events.is_empty() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            out.push(Kind::CiTriggers(name, events.join(", ")));
        }
    }
    out
}

/// События `on:`, которые запускают CI без кнопки: `push`, `pull_request` (в любой записи workflow).
pub fn auto_events(workflow: &str) -> Vec<String> {
    let mut events = Vec::new();
    let mut in_on = false;
    for raw in workflow.lines() {
        let line = raw.split('#').next().unwrap_or("");
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            in_on = trimmed.starts_with("on:") || trimmed.starts_with("'on':") || trimmed.starts_with("\"on\":");
            // Короткая запись: `on: [push, pull_request]` или `on: push`.
            if in_on && let Some((_, rest)) = trimmed.split_once(':') {
                for name in ["push", "pull_request"] {
                    if rest.split(|c: char| !c.is_alphanumeric() && c != '_').any(|w| w == name) {
                        events.push(name.to_owned());
                    }
                }
            }
            continue;
        }
        if in_on {
            for name in ["push", "pull_request"] {
                let key = trimmed.trim_start_matches("- ").trim_end_matches(':');
                if key == name && !events.iter().any(|e| e == name) {
                    events.push(name.to_owned());
                }
            }
        }
    }
    events
}

// ─── Список слов ────────────────────────────────────────────────────────────

const WORDS_FILE: &str = "guard-words.txt";

/// Файл слов лежит рядом с `anvil.toml` и в git не попадает.
pub fn words_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name(WORDS_FILE)
}

/// Слова по одному в строке; `#` — комментарий, пустые строки пропускаются.
pub fn load_words(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).map(|t| parse_words(&t)).unwrap_or_default()
}

pub fn parse_words(text: &str) -> Vec<String> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Создать файл слов с пояснением, если его нет.
pub fn ensure_words_file(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        path,
        "# Слова, которых не должно быть в публичных репозиториях, по одному в строке (регистр не важен).\n\
         # Файл лежит рядом с anvil.toml и в git не попадает.\n",
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_scheme() {
        assert_eq!(version_finding("0.9.4"), None);
        assert_eq!(version_finding("1.0.0"), None);
        assert_eq!(version_finding("0.7.34+525"), None);
        assert_eq!(version_finding("0.10.0"), Some(Kind::VersionTooHigh("0.10.0".into())));
        assert_eq!(version_finding("1.12.0"), None);
        assert_eq!(version_finding("2"), Some(Kind::VersionInvalid("2".into())));
        assert_eq!(version_finding("a.b.c"), Some(Kind::VersionInvalid("a.b.c".into())));
    }

    #[test]
    fn drive_e_only() {
        assert!(on_drive_e(Path::new(r"E:\dev\target")));
        assert!(on_drive_e(Path::new(r"e:/dev/target")));
        assert!(!on_drive_e(Path::new(r"D:\dev\target")));
        assert!(on_drive_e(Path::new("/home/user/target")));
    }

    #[test]
    fn words_in_text() {
        let words = vec!["Secret".to_owned(), "other".to_owned()];
        let hits = scan_text("a.md", "one\ntwo SECRET three\n", &words);
        assert_eq!(hits, vec![("Secret".to_owned(), "a.md:2".to_owned())]);
        assert!(scan_text("a.md", "nothing", &words).is_empty());
    }

    #[test]
    fn words_file_parsing() {
        assert_eq!(parse_words("\u{feff}# комментарий\nalpha\n\n  beta  # хвост\n"), vec!["alpha", "beta"]);
        assert!(parse_words("# только комментарий\n").is_empty());
    }

    #[test]
    fn ci_events() {
        let by_button = "name: CI\non:\n  workflow_dispatch:\n  push:\n    tags: ['v*']\njobs: {}\n";
        assert_eq!(auto_events(by_button), vec!["push"]);
        assert_eq!(auto_events("on: [push, pull_request]\n"), vec!["push", "pull_request"]);
        assert_eq!(auto_events("on: workflow_dispatch\n"), Vec::<String>::new());
        assert_eq!(
            auto_events("on:\n  pull_request:\n    branches: [main]\n  workflow_dispatch:\n"),
            vec!["pull_request"]
        );
        // `push` в заданиях, а не в `on:`, не считается.
        assert!(auto_events("on:\n  workflow_dispatch:\njobs:\n  push:\n    runs-on: x\n").is_empty());
    }

    #[test]
    fn report_counts_and_order() {
        let dir = std::env::temp_dir().join(format!("anvil-guard-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("openspec")).unwrap();
        let input = Input {
            path: dir.clone(),
            version: Some("0.10.1".into()),
            private: None,
            target_dir: Some(PathBuf::from(r"D:\x\target")),
            words: vec![],
        };
        let report = check(&input);
        let _ = std::fs::remove_dir_all(&dir);
        // Нарушение — первым; настроенный проект без CLAUDE.md и docs/README.md; сборка вне E:.
        assert_eq!(report.findings[0], Kind::VersionTooHigh("0.10.1".into()));
        assert!(report.findings.contains(&Kind::NoClaude) && report.findings.contains(&Kind::NoDocs));
        assert!(report.findings.contains(&Kind::BuildOutsideE(PathBuf::from(r"D:\x\target"))));
        assert_eq!((report.violations(), report.attention()), (1, 4));
    }

    #[test]
    fn public_scan_finds_words_in_files_and_commits() {
        let dir = std::env::temp_dir().join(format!("anvil-guard-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sh = |args: &[&str]| {
            let ok = run::command("git", &dir).args(args).output().map(|o| o.status.success()).unwrap_or(false);
            assert!(ok, "git {args:?}");
        };
        sh(&["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "hello\nthe Hidden word\n").unwrap();
        sh(&["add", "-A"]);
        sh(&["-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-q", "-m", "mentions hidden thing"]);
        let found = public_words(&dir, &["hidden".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(found.contains(&Kind::PublicWord("hidden".into(), "a.txt:2".into())));
        assert!(
            found
                .iter()
                .any(|k| matches!(k, Kind::PublicWord(w, place) if w == "hidden" && place.starts_with("commit ")))
        );
    }
}
