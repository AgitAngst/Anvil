//! Зависимости проектов и тулчейн: что устарело, что задевает RustSec, какой Rust стоит и на
//! какой версии набора Anvil каждая программа.
//!
//! Всё сетевое и долгое — в своём потоке: `cargo update --dry-run` (cargo сам знает, что обновится
//! в рамках требований, а что держит новая мажорная версия), база RustSec, `rustup check`, теги
//! набора на GitHub. Итоги кешируются в `cache/deps.json`: окно открывается сразу с прошлым знанием,
//! а свежее догружается.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::run;
use crate::rustsec::{self, Hit};

/// Как долго итог проверки проекта считается свежим (если `Cargo.lock` не менялся).
const PROJECT_FRESH: i64 = 12 * 60 * 60;
/// Как долго свежи сведения о тулчейне и тегах набора.
const TOOLCHAIN_FRESH: i64 = 12 * 60 * 60;
/// Где лежит набор: по этому адресу узнаются его git-зависимости и теги.
pub const KIT_REPO: &str = "https://github.com/AgitAngst/Anvil";

/// Изменение версии пакета.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub name: String,
    pub from: String,
    pub to: String,
    /// Прямая зависимость проекта, а не по цепочке.
    pub direct: bool,
}

impl Change {
    /// Совместима ли новая версия со старой по правилам cargo (`^`): 1.2 → 1.9 да, 0.2 → 0.3 нет.
    pub fn compatible(&self) -> bool {
        compatible(&self.from, &self.to)
    }
}

pub fn compatible(from: &str, to: &str) -> bool {
    let (Ok(a), Ok(b)) = (semver::Version::parse(from), semver::Version::parse(to)) else { return false };
    match (a.major, a.minor) {
        (0, 0) => b.major == 0 && b.minor == 0 && a.patch == b.patch,
        (0, minor) => b.major == 0 && b.minor == minor,
        (major, _) => b.major == major,
    }
}

/// На какой версии набора Anvil проект: тег или коммит из `Cargo.lock`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KitUse {
    pub tag: Option<String>,
    pub commit: String,
}

/// Итог проверки одного проекта.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Когда проверяли (секунды Unix).
    pub checked: i64,
    /// Что сделает `cargo update`: совместимые обновления.
    pub updates: Vec<Change>,
    /// Что `cargo update` не тронет: новая мажорная версия или держат требования других пакетов.
    pub held: Vec<Change>,
    /// Прямые зависимости с crates.io и их версии в `Cargo.lock` — для сводки «где разошлись».
    pub direct: BTreeMap<String, String>,
    pub advisories: Vec<Hit>,
    pub kit: Option<KitUse>,
    /// Не удалось спросить crates.io (нет сети и т.п.): уязвимости всё равно сверены.
    pub error: Option<String>,
}

impl Report {
    pub fn vulnerabilities(&self) -> usize {
        self.advisories.iter().filter(|h| h.is_vulnerability()).count()
    }

    /// Новые мажорные версии прямых зависимостей.
    pub fn majors(&self) -> impl Iterator<Item = &Change> {
        self.held.iter().filter(|c| c.direct && !c.compatible())
    }
}

/// Тулчейн Rust.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Toolchain {
    pub checked: i64,
    /// `stable-x86_64-pc-windows-msvc`.
    pub name: String,
    pub current: String,
    /// Есть новее: какая.
    pub latest: Option<String>,
    /// Самый свежий тег набора: `kit-v0.2.0`.
    pub kit_latest: Option<String>,
    pub error: Option<String>,
}

pub enum Cmd {
    /// Проверить проекты. `force` — даже если итог свежий.
    Check { projects: Vec<PathBuf>, force: bool },
    /// Спросить rustup и теги набора. `force` — даже если свежо.
    Toolchain { force: bool },
}

pub enum Event {
    Report(PathBuf, Box<Report>),
    Toolchain(Toolchain),
    /// Сейчас проверяется этот проект; `None` — поток свободен.
    Busy(Option<PathBuf>),
}

pub fn spawn(ctx: egui::Context, cache: PathBuf) -> (Sender<Cmd>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("anvil-deps".into())
        .spawn(move || Hub::new(ctx, event_tx, cache).run(cmd_rx))
        .expect("spawn deps");
    (cmd_tx, event_rx)
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    projects: HashMap<String, Report>,
    toolchain: Option<Toolchain>,
}

struct Hub {
    ctx: egui::Context,
    events: Sender<Event>,
    dir: PathBuf,
    cache: Cache,
    db: Option<rustsec::Db>,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

impl Hub {
    fn new(ctx: egui::Context, events: Sender<Event>, dir: PathBuf) -> Self {
        let cache = std::fs::read_to_string(dir.join("deps.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { ctx, events, dir, cache, db: None }
    }

    fn send(&self, event: Event) {
        let _ = self.events.send(event);
        self.ctx.request_repaint();
    }

    fn save(&self) {
        if let Ok(text) = serde_json::to_string(&self.cache) {
            let _ = std::fs::create_dir_all(&self.dir);
            let tmp = self.dir.join("deps.json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, self.dir.join("deps.json"));
            }
        }
    }

    fn run(mut self, commands: Receiver<Cmd>) {
        // Сначала — то, что знали в прошлый раз: окно не ждёт сети.
        if let Some(toolchain) = &self.cache.toolchain {
            self.send(Event::Toolchain(toolchain.clone()));
        }
        loop {
            let cmd = match commands.recv_timeout(Duration::from_secs(60 * 60)) {
                Ok(cmd) => cmd,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            match cmd {
                Cmd::Check { projects, force } => {
                    for path in &projects {
                        if let Some(report) = self.cache.projects.get(&key(path)) {
                            self.send(Event::Report(path.clone(), Box::new(report.clone())));
                        }
                    }
                    self.refresh_db();
                    for path in projects {
                        if force || self.stale(&path) {
                            self.send(Event::Busy(Some(path.clone())));
                            let report = self.check(&path);
                            self.cache.projects.insert(key(&path), report.clone());
                            self.save();
                            self.send(Event::Report(path, Box::new(report)));
                        }
                    }
                    self.send(Event::Busy(None));
                }
                Cmd::Toolchain { force } => {
                    let fresh = self.cache.toolchain.as_ref().is_some_and(|t| now() - t.checked < TOOLCHAIN_FRESH);
                    if force || !fresh {
                        let toolchain = toolchain();
                        self.cache.toolchain = Some(toolchain.clone());
                        self.save();
                        self.send(Event::Toolchain(toolchain));
                    }
                }
            }
        }
    }

    /// Итог устарел: давно не проверяли или `Cargo.lock` менялся после проверки.
    fn stale(&self, path: &Path) -> bool {
        let Some(report) = self.cache.projects.get(&key(path)) else { return true };
        let lock_changed = std::fs::metadata(path.join("Cargo.lock"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .is_some_and(|t| t.as_secs() as i64 > report.checked);
        lock_changed || now() - report.checked > PROJECT_FRESH
    }

    /// База RustSec: обновить раз в сутки и держать разобранной в памяти.
    fn refresh_db(&mut self) {
        let changed = rustsec::refresh(&self.dir).unwrap_or(false);
        if changed || self.db.is_none() {
            self.db = rustsec::load(&self.dir).ok();
        }
    }

    fn check(&self, dir: &Path) -> Report {
        let mut report = Report { checked: now(), ..Report::default() };
        let locked = rustsec::lock(dir).unwrap_or_default();
        if let Some(db) = &self.db {
            report.advisories = rustsec::check(db, &locked);
        }
        report.kit = kit_use(&locked);
        let direct = direct_names(dir);
        for package in &locked {
            if direct.contains(&package.name) && package.source.as_deref().is_some_and(|s| s.contains("crates.io")) {
                report.direct.insert(package.name.clone(), package.version.clone());
            }
        }
        match dry_run(dir) {
            Ok(text) => {
                let (updates, held) = parse_dry_run(&text, &direct);
                report.updates = updates;
                report.held = held;
            }
            Err(e) => report.error = Some(e),
        }
        report
    }
}

/// `cargo update --dry-run --verbose`: что обновилось бы и что осталось позади. Пишет в stderr.
fn dry_run(dir: &Path) -> Result<String, String> {
    let out = run::command("cargo", dir)
        .args(["update", "--dry-run", "--verbose"])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .map_err(|e| format!("cargo: {e}"))?;
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    if out.status.success() {
        Ok(text)
    } else {
        let error = text.lines().find(|l| l.trim_start().starts_with("error")).unwrap_or("cargo update failed");
        Err(error.trim().to_owned())
    }
}

/// Разобрать вывод: `Updating a v1 -> v2` — совместимое обновление, `Unchanged a v1 (available: v2)`
/// или `(latest: v2)` — осталось позади.
pub fn parse_dry_run(text: &str, direct: &BTreeSet<String>) -> (Vec<Change>, Vec<Change>) {
    let (mut updates, mut held) = (Vec::new(), Vec::new());
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("Updating") | Some("Downgrading") => {
                let (Some(name), Some(from), Some("->"), Some(to)) =
                    (words.next(), words.next(), words.next(), words.next())
                else {
                    continue;
                };
                if !from.starts_with('v') {
                    continue; // «Updating crates.io index», «Updating git repository …»
                }
                updates.push(Change {
                    name: name.to_owned(),
                    from: from.trim_start_matches('v').to_owned(),
                    to: to.trim_start_matches('v').to_owned(),
                    direct: direct.contains(name),
                });
            }
            Some("Unchanged") => {
                let (Some(name), Some(from)) = (words.next(), words.next()) else { continue };
                let Some(to) = line.split(':').nth(1).map(|s| s.trim().trim_end_matches(')').trim_start_matches('v'))
                else {
                    continue;
                };
                held.push(Change {
                    name: name.to_owned(),
                    from: from.trim_start_matches('v').to_owned(),
                    to: to.to_owned(),
                    direct: direct.contains(name),
                });
            }
            _ => {}
        }
    }
    // Прямые — первыми, внутри — по имени.
    for list in [&mut updates, &mut held] {
        list.sort_by(|a, b| b.direct.cmp(&a.direct).then(a.name.cmp(&b.name)));
    }
    (updates, held)
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<MetaPackage>,
}

#[derive(Deserialize)]
struct MetaPackage {
    #[serde(default)]
    dependencies: Vec<MetaDep>,
}

#[derive(Deserialize)]
struct MetaDep {
    name: String,
    source: Option<String>,
}

/// Имена прямых зависимостей с crates.io у всех пакетов проекта (без своих по пути и git).
fn direct_names(dir: &Path) -> BTreeSet<String> {
    let json = run::output("cargo", dir, &["metadata", "--no-deps", "--offline", "--format-version", "1"]);
    let Ok(metadata) = json.and_then(|j| serde_json::from_str::<Metadata>(&j).map_err(|e| e.to_string())) else {
        return BTreeSet::new();
    };
    metadata
        .packages
        .into_iter()
        .flat_map(|p| p.dependencies)
        .filter(|d| d.source.as_deref().is_some_and(|s| s.contains("crates.io")))
        .map(|d| d.name)
        .collect()
}

/// Набор Anvil в `Cargo.lock`: `git+https://github.com/AgitAngst/Anvil?tag=kit-v0.2.0#ebb7f92…`.
fn kit_use(locked: &[rustsec::Locked]) -> Option<KitUse> {
    let source = locked.iter().find(|p| p.name == "anvil-ui")?.source.as_deref()?;
    let rest = source.strip_prefix("git+")?;
    if !rest.starts_with(KIT_REPO) {
        return None;
    }
    let (url, commit) = rest.split_once('#')?;
    let tag = url.split_once("tag=").map(|(_, t)| t.split('&').next().unwrap_or(t).to_owned());
    Some(KitUse { tag, commit: commit.chars().take(7).collect() })
}

/// `rustup check` и свежий тег набора.
fn toolchain() -> Toolchain {
    let mut toolchain = Toolchain { checked: now(), ..Toolchain::default() };
    let home = std::env::temp_dir();
    let active = run::output("rustup", &home, &["show", "active-toolchain"]).unwrap_or_default();
    let active = active.split_whitespace().next().unwrap_or("").to_owned();
    match run::output("rustup", &home, &["check"]) {
        Ok(text) => {
            if let Some((name, current, latest)) = parse_rustup_check(&text, &active) {
                toolchain.name = name;
                toolchain.current = current;
                toolchain.latest = latest;
            }
        }
        Err(e) => toolchain.error = Some(e),
    }
    if toolchain.current.is_empty() {
        // Без сети rustup check молчит — версия хотя бы из rustc.
        let rustc = run::output("rustc", &home, &["-V"]).unwrap_or_default();
        toolchain.current = rustc.split_whitespace().nth(1).unwrap_or("").to_owned();
        toolchain.name = active;
    }
    toolchain.kit_latest = kit_latest();
    toolchain
}

/// Строка тулчейна из `rustup check`: `stable-… - up to date: 1.98.1 (…)` или
/// `stable-… - update available: 1.98.1 (…) -> 1.99.0 (…)`. Берётся активный, иначе первый.
pub fn parse_rustup_check(text: &str, active: &str) -> Option<(String, String, Option<String>)> {
    let lines: Vec<(&str, &str)> = text
        .lines()
        .filter_map(|l| l.split_once(" - "))
        .filter(|(name, _)| !name.trim().starts_with("rustup"))
        .collect();
    let (name, status) = lines.iter().find(|(name, _)| name.trim() == active).or_else(|| lines.first())?;
    let (kind, versions) = status.split_once(':')?;
    let version = |s: &str| s.split_whitespace().next().unwrap_or("").to_owned();
    if kind.to_lowercase().contains("update available") {
        let (from, to) = versions.split_once("->")?;
        Some((name.trim().to_owned(), version(from), Some(version(to))))
    } else {
        Some((name.trim().to_owned(), version(versions), None))
    }
}

/// Самый свежий тег `kit-vX.Y.Z` на GitHub.
fn kit_latest() -> Option<String> {
    let out = run::output("git", &std::env::temp_dir(), &["ls-remote", "--tags", KIT_REPO, "kit-v*"]).ok()?;
    latest_kit_tag(&out)
}

pub fn latest_kit_tag(ls_remote: &str) -> Option<String> {
    ls_remote
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .filter_map(|r| r.strip_prefix("refs/tags/"))
        .filter(|t| !t.ends_with("^{}"))
        .filter_map(|t| Some((anvil_update::Version::parse(t.strip_prefix("kit-")?)?, t.to_owned())))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, t)| t)
}

/// Где версии прямых зависимостей разошлись между проектами: пакет → (проект, версия).
pub fn diverged(reports: &[(String, &Report)]) -> Vec<(String, Vec<(String, String)>)> {
    let mut by_name: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
    for (project, report) in reports {
        for (name, version) in &report.direct {
            by_name.entry(name).or_default().push((project.clone(), version.clone()));
        }
    }
    by_name
        .into_iter()
        .filter(|(_, uses)| uses.len() > 1 && uses.iter().any(|(_, v)| *v != uses[0].1))
        .map(|(name, uses)| (name.to_owned(), uses))
        .collect()
}

// ─── Поднять набор ──────────────────────────────────────────────────────────

/// Правки, переводящие проект на тег набора `tag`: git-зависимости `anvil-ui`/`anvil-update` во
/// всех манифестах и `uses: …/rust-release.yml@…` в workflow. Оформление файлов сохраняется.
pub fn kit_edits(dir: &Path, tag: &str) -> Result<Vec<(PathBuf, String)>, String> {
    let mut edits = Vec::new();
    for manifest in manifests(dir)? {
        let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
        let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{}: {e}", manifest.display()))?;
        if retag(doc.as_item_mut(), tag) {
            edits.push((manifest, doc.to_string()));
        }
    }
    let workflows = dir.join(".github").join("workflows");
    for entry in std::fs::read_dir(&workflows).into_iter().flatten().flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "yml" || e == "yaml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let marker = "AgitAngst/Anvil/.github/workflows/rust-release.yml@";
        if !text.contains(marker) {
            continue;
        }
        let updated: String = text
            .split_inclusive('\n')
            .map(|line| match line.find(marker) {
                Some(at) => {
                    let end = line[at + marker.len()..]
                        .find(|c: char| c.is_whitespace())
                        .map_or(line.len(), |n| at + marker.len() + n);
                    format!("{}{tag}{}", &line[..at + marker.len()], &line[end..])
                }
                None => line.to_owned(),
            })
            .collect();
        if updated != text {
            edits.push((path, updated));
        }
    }
    if edits.is_empty() {
        return Err("anvil-ui / anvil-update are not git dependencies here".into());
    }
    Ok(edits)
}

fn manifests(dir: &Path) -> Result<Vec<PathBuf>, String> {
    #[derive(Deserialize)]
    struct Meta {
        packages: Vec<Pkg>,
    }
    #[derive(Deserialize)]
    struct Pkg {
        manifest_path: PathBuf,
    }
    let json = run::output("cargo", dir, &["metadata", "--no-deps", "--offline", "--format-version", "1"])?;
    let meta: Meta = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let mut list: Vec<PathBuf> = meta.packages.into_iter().map(|p| p.manifest_path).collect();
    let root = dir.join("Cargo.toml");
    if !list.iter().any(|p| crate::registry::same_dir(p, &root)) {
        list.push(root);
    }
    Ok(list)
}

/// Во всех таблицах зависимостей документа поставить `tag` у git-зависимостей набора.
fn retag(item: &mut toml_edit::Item, tag: &str) -> bool {
    let mut changed = false;
    if let Some(table) = item.as_table_like_mut() {
        for (key, value) in table.iter_mut() {
            let name = key.get();
            if (name == "anvil-ui" || name == "anvil-update")
                && let Some(dep) = value.as_table_like_mut()
                && dep.get("git").and_then(|g| g.as_str()).is_some_and(|g| g.trim_end_matches(".git") == KIT_REPO)
            {
                let already = dep.get("tag").and_then(|t| t.as_str()) == Some(tag);
                if !already || dep.contains_key("rev") || dep.contains_key("branch") {
                    dep.remove("rev");
                    dep.remove("branch");
                    dep.insert("tag", toml_edit::value(tag));
                    changed = true;
                }
            } else {
                changed |= retag(value, tag);
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRY_RUN: &str = "    Updating crates.io index
     Locking 3 packages to latest compatible versions
    Updating cc v1.4.6 -> v1.4.7
    Updating serde v1.0.200 -> v1.0.210
   Unchanged toml v0.8.2 (available: v0.8.23)
   Unchanged windows-core v0.62.2 (available: v0.100.0)
   Unchanged eframe v0.36.2 (latest: v0.37.0)
      Adding synstructure v0.14.0
warning: not updating lockfile due to dry run";

    #[test]
    fn dry_run_splits_updates_and_held() {
        let direct: BTreeSet<String> = ["serde", "eframe"].iter().map(|s| s.to_string()).collect();
        let (updates, held) = parse_dry_run(DRY_RUN, &direct);
        assert_eq!(updates.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["serde", "cc"]);
        assert_eq!((updates[0].from.as_str(), updates[0].to.as_str()), ("1.0.200", "1.0.210"));
        assert_eq!(held.len(), 3);
        assert_eq!(held[0].name, "eframe");
        assert_eq!(held[0].to, "0.37.0");
        let report = Report { held, ..Report::default() };
        assert_eq!(report.majors().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["eframe"]);
    }

    #[test]
    fn compatibility_follows_caret_rules() {
        assert!(compatible("1.2.3", "1.9.0"));
        assert!(!compatible("1.2.3", "2.0.0"));
        assert!(compatible("0.8.2", "0.8.23"));
        assert!(!compatible("0.36.2", "0.37.0"));
        assert!(!compatible("0.0.3", "0.0.4"));
    }

    #[test]
    fn rustup_check_lines() {
        let text = "stable-x86_64-pc-windows-msvc - update available: 1.98.1 (48a229cea 2026-09-01) -> 1.99.0 (aa 2026-10-15)\n\
                    rustup - up to date : 1.29.1\n";
        let (name, current, latest) = parse_rustup_check(text, "stable-x86_64-pc-windows-msvc").unwrap();
        assert_eq!(name, "stable-x86_64-pc-windows-msvc");
        assert_eq!(current, "1.98.1");
        assert_eq!(latest.as_deref(), Some("1.99.0"));
        let up = parse_rustup_check("stable-x86_64-pc-windows-msvc - up to date: 1.98.1 (x)\n", "").unwrap();
        assert_eq!((up.1.as_str(), up.2), ("1.98.1", None));
    }

    #[test]
    fn kit_tags_and_lock_source() {
        let ls = "690943fa\trefs/tags/kit-v0.1.0\n3f89301b\trefs/tags/kit-v0.1.0^{}\nc04fc9e9\trefs/tags/kit-v0.2.0\n\
                  ebb7f92c\trefs/tags/kit-v0.2.0^{}\n";
        assert_eq!(latest_kit_tag(ls).as_deref(), Some("kit-v0.2.0"));
        let locked = [rustsec::Locked {
            name: "anvil-ui".into(),
            version: "0.1.0".into(),
            source: Some("git+https://github.com/AgitAngst/Anvil?tag=kit-v0.2.0#ebb7f92cb65da53b".into()),
        }];
        assert_eq!(kit_use(&locked), Some(KitUse { tag: Some("kit-v0.2.0".into()), commit: "ebb7f92".into() }));
    }

    #[test]
    fn retag_rewrites_only_kit_git_dependencies() {
        let text = "[dependencies]\n# набор\nanvil-ui = { git = \"https://github.com/AgitAngst/Anvil\", rev = \"ebb7f92\", features = [\"serde\"] }\n\
                    serde = \"1\"\n\n[workspace.dependencies]\nanvil-update = { git = \"https://github.com/AgitAngst/Anvil\", tag = \"kit-v0.1.0\" }\n\
                    anvil-ui = { path = \"../anvil-ui\" }\n";
        let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert!(retag(doc.as_item_mut(), "kit-v0.3.0"));
        let out = doc.to_string();
        assert!(out.contains("# набор"), "комментарии остаются");
        assert!(out.contains("anvil-ui = { git = \"https://github.com/AgitAngst/Anvil\", features = [\"serde\"] , tag = \"kit-v0.3.0\" }")
            || out.contains("tag = \"kit-v0.3.0\""));
        assert!(!out.contains("rev ="));
        assert!(out.contains("anvil-ui = { path = \"../anvil-ui\" }"), "путь не трогаем");
        assert_eq!(out.matches("kit-v0.3.0").count(), 2);
    }

    #[test]
    fn diverged_lists_only_differing_versions() {
        let a = Report {
            direct: [("egui".into(), "0.36.2".into()), ("serde".into(), "1.0.1".into())].into(),
            ..Report::default()
        };
        let b = Report {
            direct: [("egui".into(), "0.27.0".into()), ("serde".into(), "1.0.1".into())].into(),
            ..Report::default()
        };
        let list = diverged(&[("A".into(), &a), ("B".into(), &b)]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "egui");
    }
}
