//! Выпуски на GitHub и выбор того, на который стоит обновиться.

use serde::Deserialize;

use crate::version::Version;

/// Файл выпуска, который можно скачать.
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// Доступное обновление.
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub version: Version,
    pub tag: String,
    /// Заметки к выпуску — тело GitHub Release.
    pub notes: String,
    /// Страница выпуска.
    pub page: String,
    /// Архив для этой системы; `None` — для неё выпуска нет, только страница.
    pub asset: Option<Asset>,
    /// `SHA256SUMS` выпуска.
    pub sums: Option<Asset>,
}

#[derive(Deserialize)]
pub(crate) struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Имя архива по соглашению: `<app>-<версия>-windows-x64.zip`.
pub fn asset_name(app: &str, version: &Version) -> String {
    format!("{app}-{version}-{}.zip", platform())
}

/// Платформа в имени архива. Пока выпуски собираются только под Windows x64.
pub fn platform() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) { "windows-x64" } else { "unsupported" }
}

/// Самый новый выпуск новее текущей версии. Черновики не в счёт, пред-выпуски — только если просили.
pub(crate) fn choose(releases: Vec<ApiRelease>, app: &str, current: &Version, prerelease: bool) -> Option<Update> {
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| Version::parse(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, r)| prerelease || !(r.prerelease || v.is_prerelease()))
        .filter(|(v, _)| v > current)
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(version, r)| {
            let wanted = asset_name(app, &version);
            let find = |name: &str| {
                r.assets.iter().find(|a| a.name == name).map(|a| Asset {
                    name: a.name.clone(),
                    url: a.browser_download_url.clone(),
                    size: a.size,
                })
            };
            Update {
                asset: find(&wanted),
                sums: find("SHA256SUMS"),
                tag: r.tag_name,
                notes: r.body.unwrap_or_default().trim().to_owned(),
                page: r.html_url,
                version,
            }
        })
}

/// Строка `SHA256SUMS` для файла: `<hex>  <имя>` (как у `sha256sum`, допускается `*` перед именем).
pub fn expected_sum(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, draft: bool, assets: &[&str]) -> ApiRelease {
        ApiRelease {
            tag_name: tag.into(),
            body: Some(format!("Что нового в {tag}")),
            html_url: format!("https://github.com/o/r/releases/tag/{tag}"),
            draft,
            prerelease,
            assets: assets
                .iter()
                .map(|n| ApiAsset { name: (*n).into(), browser_download_url: format!("https://dl/{n}"), size: 1 })
                .collect(),
        }
    }

    #[test]
    fn picks_newest_stable_and_its_files() {
        let current = Version::parse("0.3.0").unwrap();
        let zip = asset_name("amber", &Version::parse("0.3.2").unwrap());
        let list = vec![
            release("v0.3.1", false, false, &[]),
            release("v0.3.2", false, false, &[zip.as_str(), "SHA256SUMS"]),
            release("v0.4.0-beta.1", true, false, &[]),
            release("v0.5.0", false, true, &[]),
            release("v0.2.9", false, false, &[]),
        ];
        let update = choose(list, "amber", &current, false).unwrap();
        assert_eq!(update.tag, "v0.3.2");
        assert_eq!(update.sums.unwrap().url, "https://dl/SHA256SUMS");
        if cfg!(all(windows, target_arch = "x86_64")) {
            assert_eq!(update.asset.unwrap().name, "amber-0.3.2-windows-x64.zip");
        }
    }

    #[test]
    fn prereleases_only_on_request() {
        let current = Version::parse("0.3.0").unwrap();
        let list = || vec![release("v0.4.0-beta.1", true, false, &[])];
        assert!(choose(list(), "a", &current, false).is_none());
        assert_eq!(choose(list(), "a", &current, true).unwrap().tag, "v0.4.0-beta.1");
    }

    #[test]
    fn nothing_newer() {
        let current = Version::parse("1.0.0").unwrap();
        assert!(
            choose(
                vec![release("v1.0.0", false, false, &[]), release("v0.9.0", false, false, &[])],
                "a",
                &current,
                true
            )
            .is_none()
        );
    }

    #[test]
    fn sums_file() {
        let sums = "AB12  anvil-0.2.0-windows-x64.zip\ncd34 *other.zip\n";
        assert_eq!(expected_sum(sums, "anvil-0.2.0-windows-x64.zip").as_deref(), Some("ab12"));
        assert_eq!(expected_sum(sums, "other.zip").as_deref(), Some("cd34"));
        assert_eq!(expected_sum(sums, "missing.zip"), None);
    }
}
