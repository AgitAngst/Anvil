//! Магазин программ: программы, найденные в выпусках репозиториев. Выпуски читает поток GitHub (`Cmd::StoreScan`);
//! здесь — разбор архивов выпуска по соглашению `<программа>-X.Y.Z-<платформа>.zip` + `SHA256SUMS` и состояние программы.

use std::collections::{HashMap, HashSet};

use anvil_update::Version;

use crate::github::{Release, Repo};
use crate::sources::safe_name;

/// Программа из выпуска.
#[derive(Debug, Clone)]
pub struct Program {
    pub repo: Repo,
    pub bin: String,
    pub version: Version,
    /// Выпуск, из которого ставится.
    pub release: Release,
    /// Размер архива, байт.
    pub size: u64,
}

impl Program {
    /// Ключ программы в окне.
    pub fn key(&self) -> String {
        format!("{}|{}", self.repo.slug().to_lowercase(), self.bin)
    }
}

/// Имя программы из имени архива выпуска `version`: `lantern-desktop-0.3.1-windows-x64.zip` → `lantern-desktop`.
/// Имя идёт в путь установки, поэтому только безопасные.
pub fn bin_of(asset: &str, version: &Version) -> Option<String> {
    let suffix = anvil_update::asset_name("", version);
    let bin = asset.strip_suffix(suffix.as_str())?;
    safe_name(bin).then(|| bin.to_owned())
}

/// Программы репозитория: по каждому имени — самый новый выпуск с архивом и `SHA256SUMS`. Пред-выпуски — только если разрешены.
pub fn programs(repo: &Repo, releases: &[Release], prerelease: bool) -> Vec<Program> {
    let mut candidates: Vec<(&Release, Version)> = releases
        .iter()
        .filter(|r| !r.draft && (prerelease || !r.prerelease))
        .filter(|r| r.assets.iter().any(|a| a.name == "SHA256SUMS"))
        .filter_map(|r| Version::parse(&r.tag).map(|v| (r, v)))
        .collect();
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (release, version) in candidates {
        for asset in &release.assets {
            let Some(bin) = bin_of(&asset.name, &version) else { continue };
            if seen.insert(bin.clone()) {
                out.push(Program {
                    repo: repo.clone(),
                    bin,
                    version: version.clone(),
                    release: release.clone(),
                    size: asset.size,
                });
            }
        }
    }
    out.sort_by(|a, b| a.bin.cmp(&b.bin));
    out
}

/// Состояние программы на этом ПК.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// Не установлена.
    Fresh,
    /// Установлена, в выпуске новее: установленная метка.
    Update(String),
    /// Установлена, новее нет.
    Current(String),
}

/// Установленная метка (`0.4.1` или `0.1.0-3f89301` у локальной сборки) против версии выпуска; хвост сборки не в счёт.
pub fn state(installed: Option<&str>, latest: &Version) -> State {
    let Some(label) = installed else { return State::Fresh };
    let core = |v: &Version| Version { pre: None, ..v.clone() };
    match Version::parse(label) {
        Some(current) if core(latest) > core(&current) => State::Update(label.to_owned()),
        _ => State::Current(label.to_owned()),
    }
}

/// Какие карточки показывать.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Installed,
    Updates,
}

/// Состояние окна магазина.
#[derive(Default)]
pub struct Store {
    /// Выпуски репозиториев (ключ — `owner/name` в нижнем регистре) или причина, почему не прочитались.
    pub releases: HashMap<String, Result<Vec<Release>, String>>,
    /// Идёт чтение выпусков.
    pub loading: bool,
    /// Когда прочитали (секунды UNIX); 0 — ни разу.
    pub read: i64,
    pub filter: Filter,
    pub query: String,
    /// Открытая программа (`Program::key`).
    pub open: Option<String>,
    /// Показать репозитории без программ.
    pub show_empty: bool,
    /// Задача установки по имени программы: ход и ошибка — в карточке.
    pub jobs: HashMap<String, crate::jobs::JobId>,
}

/// Репозиторий для магазина: из аккаунта или подключённого проекта.
#[derive(Debug, Clone)]
pub struct RepoInfo {
    pub repo: Repo,
    pub description: Option<String>,
    pub private: bool,
    /// Проект на диске с таким origin.
    pub project: Option<std::path::PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Asset;

    fn asset(name: String) -> Asset {
        Asset { name, size: 1000, downloads: 0, url: "u".into(), api_url: "a".into() }
    }

    fn release(tag: &str, bins: &[&str], sums: bool, draft: bool, pre: bool) -> Release {
        let version = Version::parse(tag).unwrap();
        let mut assets: Vec<Asset> = bins.iter().map(|b| asset(anvil_update::asset_name(b, &version))).collect();
        if sums {
            assets.push(asset("SHA256SUMS".into()));
        }
        Release { tag: tag.into(), name: tag.into(), url: "r".into(), published: 0, prerelease: pre, draft, assets }
    }

    fn repo() -> Repo {
        Repo { owner: "o".into(), name: "n".into() }
    }

    #[test]
    fn names_from_archives() {
        let v = Version::parse("0.3.1").unwrap();
        let ok = anvil_update::asset_name("lantern-desktop", &v);
        assert_eq!(bin_of(&ok, &v).as_deref(), Some("lantern-desktop"));
        assert_eq!(bin_of("SHA256SUMS", &v), None);
        assert_eq!(bin_of(&anvil_update::asset_name("a b", &v), &v), None);
        assert_eq!(bin_of(&anvil_update::asset_name("..", &v), &v), None);
        // Версия в имени должна быть версией выпуска.
        assert_eq!(bin_of(&anvil_update::asset_name("x", &Version::parse("0.3.0").unwrap()), &v), None);
    }

    #[test]
    fn latest_release_per_program() {
        let releases = vec![
            release("v1.0.0", &["tool"], true, false, false),
            release("v1.1.0", &["tool", "other"], true, false, false),
            release("v1.2.0-beta.1", &["tool"], true, false, true),
            release("v2.0.0", &["draft-only"], true, true, false),
            release("v1.3.0", &["nosums"], false, false, false),
        ];
        let list = programs(&repo(), &releases, false);
        let names: Vec<(&str, String)> = list.iter().map(|p| (p.bin.as_str(), p.version.to_string())).collect();
        assert_eq!(names, [("other", "1.1.0".to_owned()), ("tool", "1.1.0".to_owned())]);
        // С пред-выпусками — свежий beta у `tool`.
        let with_pre = programs(&repo(), &releases, true);
        assert_eq!(with_pre.iter().find(|p| p.bin == "tool").unwrap().version.to_string(), "1.2.0-beta.1");
        assert!(programs(&repo(), &[], false).is_empty());
    }

    #[test]
    fn states() {
        let v = |s: &str| Version::parse(s).unwrap();
        assert_eq!(state(None, &v("1.0.0")), State::Fresh);
        assert_eq!(state(Some("0.9.0"), &v("1.0.0")), State::Update("0.9.0".into()));
        assert_eq!(state(Some("1.0.0"), &v("1.0.0")), State::Current("1.0.0".into()));
        // Локальная сборка того же выпуска — не отстала.
        assert_eq!(state(Some("1.0.0-3f89301"), &v("1.0.0")), State::Current("1.0.0-3f89301".into()));
        assert_eq!(state(Some("1.1.0"), &v("1.0.0")), State::Current("1.1.0".into()));
        let r = release("v1.0.0", &["tool"], true, false, false);
        assert_eq!(programs(&repo(), &[r], false)[0].key(), "o/n|tool");
    }
}
