//! Поиск проектов и сведения о них из `cargo metadata`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::run;

/// Папки проектов: сам корень, если в нём `Cargo.toml`, и его прямые подпапки с `Cargo.toml`.
/// Пути приводятся к каноническому виду, чтобы один проект не попал дважды.
pub fn scan(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in roots {
        if root.join("Cargo.toml").is_file() {
            found.push(root.clone());
            continue;
        }
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
            if !hidden && path.is_dir() && path.join("Cargo.toml").is_file() {
                found.push(path);
            }
        }
    }
    found.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    found.dedup_by_key(|p| p.to_string_lossy().to_lowercase());
    found
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub version: Option<String>,
    pub description: Option<String>,
    pub repository: Option<String>,
    /// Пакетов в workspace; 1 — обычный проект.
    pub packages: usize,
    /// Запускаемые бинарники: имя и пакет.
    pub bins: Vec<Bin>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bin {
    pub name: String,
    pub package: String,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    description: Option<String>,
    repository: Option<String>,
    manifest_path: PathBuf,
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}

/// Сведения о проекте. Без сети: `--no-deps --offline`, cargo не лезет в реестр.
pub fn meta(dir: &Path) -> Result<Meta, String> {
    let json = run::output("cargo", dir, &["metadata", "--no-deps", "--offline", "--format-version", "1"])?;
    let metadata: Metadata = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(summarize(dir, metadata))
}

fn summarize(dir: &Path, metadata: Metadata) -> Meta {
    let packages = metadata.packages;
    // Главный пакет: корневой, а в workspace без корня — названный как папка (`Anvil` → `anvil`).
    // Сведения берутся у него, а чего у него нет — у первого пакета, у кого это заполнено.
    let folder = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let root = packages
        .iter()
        .find(|p| p.manifest_path.parent().is_some_and(|d| same_dir(d, dir)))
        .or_else(|| packages.iter().find(|p| p.name.to_lowercase() == folder));
    let pick = |f: fn(&Package) -> Option<&String>| {
        root.and_then(f).or_else(|| packages.iter().find_map(f)).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
    };
    // Версия: у главного пакета, иначе — та, что встречается чаще (в workspace обычно общая).
    let version = root.map(|p| p.version.clone()).or_else(|| {
        let mut votes: BTreeMap<&str, usize> = BTreeMap::new();
        for p in &packages {
            *votes.entry(p.version.as_str()).or_default() += 1;
        }
        votes.into_iter().max_by_key(|(_, n)| *n).map(|(v, _)| v.to_owned())
    });
    let mut bins: Vec<Bin> = packages
        .iter()
        .flat_map(|p| {
            p.targets
                .iter()
                .filter(|t| t.kind.iter().any(|k| k == "bin"))
                .map(|t| Bin { name: t.name.clone(), package: p.name.clone() })
        })
        .collect();
    bins.sort_by(|a, b| a.name.cmp(&b.name));
    Meta {
        version,
        description: pick(|p| p.description.as_ref()),
        repository: pick(|p| p.repository.as_ref()),
        packages: packages.len(),
        bins,
    }
}

/// Одна ли это папка: cargo и проводник могут по-разному писать регистр и разделители.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

/// Файлы-заметки проекта, которые стоит держать под рукой.
pub const NOTES: [&str; 6] = ["HANDOFF.md", "TODO.md", "SPEC.md", "DESIGN.md", "GUIDE.md", "README.md"];

pub fn notes(dir: &Path) -> Vec<(String, i64)> {
    NOTES
        .iter()
        .filter_map(|name| {
            let modified = std::fs::metadata(dir.join(name)).ok()?.modified().ok()?;
            let secs = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
            Some((name.to_string(), secs))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, dir: &str, version: &str, kinds: &[(&str, &str)]) -> Package {
        Package {
            name: name.into(),
            version: version.into(),
            description: None,
            repository: None,
            manifest_path: PathBuf::from(dir).join("Cargo.toml"),
            targets: kinds.iter().map(|(n, k)| Target { name: (*n).into(), kind: vec![(*k).into()] }).collect(),
        }
    }

    #[test]
    fn virtual_workspace_takes_common_version_and_all_bins() {
        let root = Path::new("/w");
        let meta = summarize(
            root,
            Metadata {
                packages: vec![
                    package("core", "/w/crates/core", "0.3.0", &[("core", "lib")]),
                    package("desktop", "/w/crates/desktop", "0.3.0", &[("amber-desktop", "bin")]),
                    package("server", "/w/crates/server", "0.3.0", &[("amber-server", "bin"), ("smoke", "example")]),
                ],
            },
        );
        assert_eq!(meta.version.as_deref(), Some("0.3.0"));
        assert_eq!(meta.packages, 3);
        let bins: Vec<&str> = meta.bins.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(bins, ["amber-desktop", "amber-server"]);
    }

    #[test]
    fn package_named_like_folder_speaks_for_workspace() {
        let mut kit = package("anvil-ui", "/x/Anvil/crates/anvil-ui", "0.1.0", &[]);
        kit.description = Some("Набор".into());
        let mut app = package("anvil", "/x/Anvil/crates/anvil", "0.1.0", &[("anvil", "bin")]);
        app.description = Some("Командный центр".into());
        let meta = summarize(Path::new("/x/Anvil"), Metadata { packages: vec![kit, app] });
        assert_eq!(meta.description.as_deref(), Some("Командный центр"));
    }

    #[test]
    fn root_package_wins() {
        let mut root_pkg = package("app", "/p", "1.2.0", &[("app", "bin")]);
        root_pkg.description = Some("Главный".into());
        let mut other = package("helper", "/p/helper", "0.1.0", &[]);
        other.description = Some("Помощник".into());
        let meta = summarize(Path::new("/p"), Metadata { packages: vec![other, root_pkg] });
        assert_eq!(meta.version.as_deref(), Some("1.2.0"));
        assert_eq!(meta.description.as_deref(), Some("Главный"));
    }
}
