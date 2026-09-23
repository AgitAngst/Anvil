//! База уязвимостей RustSec — та же, по которой проверяет `cargo audit`.
//!
//! Anvil держит у себя архив репозитория `rustsec/advisory-db` (около мегабайта) и обновляет его
//! раз в сутки с `If-None-Match`: если база не менялась, GitHub отвечает «304» и ничего не качается.
//! Сверка идёт на месте: список зависимостей проектов никуда не отправляется.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const URL: &str = "https://github.com/rustsec/advisory-db/archive/refs/heads/main.zip";

/// Одна запись базы.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub package: String,
    pub title: String,
    pub url: Option<String>,
    /// `unmaintained`, `unsound`, `notice` — предупреждение, а не уязвимость.
    pub informational: Option<String>,
    pub patched: Vec<String>,
    unaffected: Vec<String>,
}

impl Entry {
    /// Задевает ли запись эту версию: не исправлена и не из незатронутых.
    pub fn affects(&self, version: &semver::Version) -> bool {
        let matches = |reqs: &[String]| {
            reqs.iter().filter_map(|r| semver::VersionReq::parse(r).ok()).any(|req| req.matches(version))
        };
        !matches(&self.patched) && !matches(&self.unaffected)
    }
}

/// База целиком: пакет → его записи.
#[derive(Debug, Default)]
pub struct Db {
    pub by_package: HashMap<String, Vec<Entry>>,
}

/// Совпадение: запись RustSec, задевающая версию из `Cargo.lock`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub id: String,
    pub package: String,
    pub version: String,
    pub title: String,
    pub url: String,
    /// Пусто — уязвимость; иначе вид предупреждения: `unmaintained`, `unsound`, `notice`.
    pub informational: Option<String>,
    /// В каких версиях исправлено: `>= 1.2.3`. Пусто — исправления нет.
    pub patched: Vec<String>,
}

impl Hit {
    pub fn is_vulnerability(&self) -> bool {
        self.informational.is_none() || self.informational.as_deref() == Some("unsound")
    }
}

/// Обновить архив базы, если он старше суток. Возвращает, изменилось ли что-нибудь.
pub fn refresh(dir: &Path) -> Result<bool, String> {
    let zip = dir.join("advisory-db.zip");
    let etag_path = dir.join("advisory-db.etag");
    let fresh = std::fs::metadata(&zip)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(24 * 60 * 60));
    if fresh {
        return Ok(false);
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let http = reqwest::blocking::Client::builder()
        .user_agent(concat!("anvil/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = http.get(URL);
    if zip.exists()
        && let Ok(etag) = std::fs::read_to_string(&etag_path)
    {
        request = request.header("If-None-Match", etag.trim());
    }
    let response = request.send().map_err(|e| format!("RustSec: {}", e.without_url()))?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        // Не менялась — отметить, что проверяли: время файла сдвигаем на «сейчас».
        let file = std::fs::File::options().write(true).open(&zip).map_err(|e| e.to_string())?;
        file.set_modified(std::time::SystemTime::now()).map_err(|e| e.to_string())?;
        return Ok(false);
    }
    if !response.status().is_success() {
        return Err(format!("RustSec: HTTP {}", response.status().as_u16()));
    }
    let etag = response.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_owned);
    let bytes = response.bytes().map_err(|e| format!("RustSec: {}", e.without_url()))?;
    // Сначала убедиться, что архив разбирается, и только потом заменить прежний.
    parse(&bytes)?;
    let tmp = zip.with_extension("zip.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &zip).map_err(|e| e.to_string())?;
    if let Some(etag) = etag {
        let _ = std::fs::write(&etag_path, etag);
    }
    Ok(true)
}

/// Прочитать базу из архива в кеше. Архива нет — пустая база.
pub fn load(dir: &Path) -> Result<Db, String> {
    match std::fs::read(dir.join("advisory-db.zip")) {
        Ok(bytes) => parse(&bytes),
        Err(_) => Ok(Db::default()),
    }
}

fn parse(bytes: &[u8]) -> Result<Db, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("RustSec: {e}"))?;
    let mut db = Db::default();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("RustSec: {e}"))?;
        let name = file.name().to_owned();
        if !name.contains("/crates/") || !name.ends_with(".md") {
            continue;
        }
        let mut text = String::new();
        if file.read_to_string(&mut text).is_err() {
            continue;
        }
        if let Some(entry) = parse_entry(&text) {
            db.by_package.entry(entry.package.clone()).or_default().push(entry);
        }
    }
    if db.by_package.is_empty() {
        return Err("RustSec: empty advisory database".into());
    }
    Ok(db)
}

#[derive(Deserialize)]
struct Front {
    advisory: Advisory,
    #[serde(default)]
    versions: Versions,
}

#[derive(Deserialize)]
struct Advisory {
    id: String,
    package: String,
    url: Option<String>,
    informational: Option<String>,
    withdrawn: Option<String>,
}

#[derive(Deserialize, Default)]
struct Versions {
    #[serde(default)]
    patched: Vec<String>,
    #[serde(default)]
    unaffected: Vec<String>,
}

/// Запись — markdown: блок ```toml``` с полями и заголовок `# …` после него.
fn parse_entry(text: &str) -> Option<Entry> {
    let text = text.replace("\r\n", "\n");
    let body = text.strip_prefix("```toml\n")?;
    let end = body.find("\n```")?;
    let front: Front = toml::from_str(&body[..end]).ok()?;
    if front.advisory.withdrawn.is_some() {
        return None;
    }
    let title =
        body[end + 4..].lines().find_map(|l| l.strip_prefix("# ")).map(|t| t.trim().to_owned()).unwrap_or_default();
    Some(Entry {
        id: front.advisory.id,
        package: front.advisory.package,
        title,
        url: front.advisory.url,
        informational: front.advisory.informational,
        patched: front.versions.patched,
        unaffected: front.versions.unaffected,
    })
}

/// Пакет из `Cargo.lock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    pub name: String,
    pub version: String,
    /// `registry+…` — crates.io, `git+…` — git-зависимость, пусто — свой пакет по пути.
    pub source: Option<String>,
}

#[derive(Deserialize)]
struct LockFile {
    #[serde(default)]
    package: Vec<LockPackage>,
}

#[derive(Deserialize)]
struct LockPackage {
    name: String,
    version: String,
    source: Option<String>,
}

/// Пакеты из `Cargo.lock` проекта. Файла нет (библиотека) — пустой список.
pub fn lock(dir: &Path) -> Result<Vec<Locked>, String> {
    let path = dir.join("Cargo.lock");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Cargo.lock: {e}")),
    };
    let file: LockFile = toml::from_str(&text).map_err(|e| format!("Cargo.lock: {e}"))?;
    Ok(file.package.into_iter().map(|p| Locked { name: p.name, version: p.version, source: p.source }).collect())
}

/// Что из `Cargo.lock` задевают записи базы. Сверяются только пакеты с crates.io.
pub fn check(db: &Db, locked: &[Locked]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for package in locked.iter().filter(|p| p.source.as_deref().is_some_and(|s| s.contains("crates.io"))) {
        let Some(entries) = db.by_package.get(&package.name) else { continue };
        let Ok(version) = semver::Version::parse(&package.version) else { continue };
        for entry in entries.iter().filter(|e| e.affects(&version)) {
            hits.push(Hit {
                id: entry.id.clone(),
                package: package.name.clone(),
                version: package.version.clone(),
                title: entry.title.clone(),
                url: entry.url.clone().unwrap_or_else(|| format!("https://rustsec.org/advisories/{}.html", entry.id)),
                informational: entry.informational.clone(),
                patched: entry.patched.clone(),
            });
        }
    }
    // Уязвимости — первыми, внутри — свежие сверху.
    hits.sort_by(|a, b| b.is_vulnerability().cmp(&a.is_vulnerability()).then(b.id.cmp(&a.id)));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "```toml\n[advisory]\nid = \"RUSTSEC-2020-0105\"\npackage = \"abi_stable\"\ndate = \"2020-12-21\"\n\
        url = \"https://example.org/44\"\n\n[versions]\npatched = [\">= 0.9.1\"]\nunaffected = [\"< 0.4.0\"]\n```\n\n\
        # Update unsound DrainFilter\n\nText.\n";

    #[test]
    fn entry_reads_and_matches_versions() {
        let entry = parse_entry(ENTRY).unwrap();
        assert_eq!(entry.id, "RUSTSEC-2020-0105");
        assert_eq!(entry.title, "Update unsound DrainFilter");
        let v = |s| semver::Version::parse(s).unwrap();
        assert!(entry.affects(&v("0.8.0")));
        assert!(!entry.affects(&v("0.9.1")), "исправленная");
        assert!(!entry.affects(&v("0.3.9")), "незатронутая");
    }

    #[test]
    fn withdrawn_entries_are_skipped_and_crlf_is_fine() {
        let withdrawn = ENTRY.replace("date = ", "withdrawn = \"2021-01-01\"\ndate = ");
        assert!(parse_entry(&withdrawn).is_none());
        assert!(parse_entry(&ENTRY.replace('\n', "\r\n")).is_some());
    }

    #[test]
    fn only_crates_io_packages_are_checked() {
        let mut db = Db::default();
        db.by_package.insert("abi_stable".into(), vec![parse_entry(ENTRY).unwrap()]);
        let registry = Some("registry+https://github.com/rust-lang/crates.io-index".to_owned());
        let locked = [
            Locked { name: "abi_stable".into(), version: "0.8.0".into(), source: registry.clone() },
            Locked { name: "abi_stable".into(), version: "0.8.0".into(), source: None },
            Locked { name: "abi_stable".into(), version: "0.9.2".into(), source: registry },
        ];
        let hits = check(&db, &locked);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].version, "0.8.0");
        assert!(hits[0].is_vulnerability());
    }
}
