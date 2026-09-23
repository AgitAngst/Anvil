//! Выпуск версии: какой будет версия, что поменять в `Cargo.toml`, черновик заметок, упаковка по
//! соглашению (`docs/RELEASES.md`) и выкладка в GitHub Release, если выпуск не собирает CI.

use std::io::Write;
use std::path::{Path, PathBuf};

use anvil_update::Version;
use toml_edit::{DocumentMut, Item, Value};

use crate::run;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

/// Следующая версия: без хвоста пред-выпуска, с нужным разрядом.
pub fn bump(base: &Version, how: Bump) -> Version {
    let (major, minor, patch) = match how {
        Bump::Patch if base.pre.is_some() => (base.major, base.minor, base.patch),
        Bump::Patch => (base.major, base.minor, base.patch + 1),
        Bump::Minor => (base.major, base.minor + 1, 0),
        Bump::Major => (base.major + 1, 0, 0),
    };
    Version { major, minor, patch, pre: None }
}

/// От чего считать: большая из версии в `Cargo.toml` и последнего тега — они бывают разошедшимися.
pub fn base(cargo: Option<&str>, last_tag: Option<&str>) -> Version {
    let parse = |v: Option<&str>| v.and_then(Version::parse);
    match (parse(cargo), parse(last_tag)) {
        (Some(a), Some(b)) => a.max(b),
        (Some(v), None) | (None, Some(v)) => v,
        (None, None) => Version { major: 0, minor: 1, patch: 0, pre: None },
    }
}

/// Правка одного файла: что было, что станет, и как это сказать человеку.
#[derive(Debug, Clone, PartialEq)]
pub struct FileEdit {
    pub path: PathBuf,
    pub text: String,
    /// Что меняется: `workspace.package.version`, `dependencies.anvil-ui`…
    pub what: Vec<String>,
}

/// Что поменять в манифестах проекта, чтобы версия стала `new`: версии пакетов (и общая версия
/// workspace), а также `version` у path-зависимостей между своими крейтами — иначе после
/// `0.1 → 0.2` cargo их не соберёт.
pub fn plan_edits(dir: &Path, old: &str, new: &str) -> Result<Vec<FileEdit>, String> {
    let mut manifests = member_manifests(dir)?;
    let root = dir.join("Cargo.toml");
    if !manifests.iter().any(|m| crate::registry::same_dir(m, &root)) {
        manifests.insert(0, root);
    }
    let mut edits = Vec::new();
    for path in manifests {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some((text, what)) = edit_manifest(&text, old, new)? {
            edits.push(FileEdit { path, text, what });
        }
    }
    Ok(edits)
}

fn member_manifests(dir: &Path) -> Result<Vec<PathBuf>, String> {
    #[derive(serde::Deserialize)]
    struct Metadata {
        packages: Vec<Package>,
    }
    #[derive(serde::Deserialize)]
    struct Package {
        manifest_path: PathBuf,
    }
    let json = run::output("cargo", dir, &["metadata", "--no-deps", "--offline", "--format-version", "1"])?;
    let meta: Metadata = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(meta.packages.into_iter().map(|p| p.manifest_path).collect())
}

/// Поменять версию в одном манифесте. `None` — менять нечего.
fn edit_manifest(text: &str, old: &str, new: &str) -> Result<Option<(String, Vec<String>)>, String> {
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
    let mut what = Vec::new();

    for (section, label) in [("package", "package.version"), ("workspace", "workspace.package.version")] {
        let item = match section {
            "package" => doc.get_mut("package").and_then(|p| p.get_mut("version")),
            _ => doc.get_mut("workspace").and_then(|w| w.get_mut("package")).and_then(|p| p.get_mut("version")),
        };
        if let Some(item) = item
            && set_if(item, old, new)
        {
            what.push(label.to_owned());
        }
    }

    // Зависимости: [dependencies], [dev-…], [build-…], [workspace.dependencies], [target.*.…].
    let mut tables: Vec<Vec<String>> = Vec::new();
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        tables.push(vec![kind.into()]);
        tables.push(vec!["workspace".into(), kind.into()]);
    }
    if let Some(targets) = doc.get("target").and_then(Item::as_table_like) {
        for (target, _) in targets.iter() {
            for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
                tables.push(vec!["target".into(), target.to_owned(), kind.into()]);
            }
        }
    }
    for path in tables {
        let mut item = Some(doc.as_item_mut());
        for key in &path {
            item = item.and_then(|i| i.get_mut(key.as_str()));
        }
        let Some(table) = item.and_then(Item::as_table_like_mut) else { continue };
        for (name, dep) in table.iter_mut() {
            let Some(dep) = dep.as_table_like_mut() else { continue };
            if dep.get("path").is_none() {
                continue;
            }
            if let Some(version) = dep.get_mut("version")
                && set_if(version, old, new)
            {
                what.push(format!("{}.{}", path.join("."), name.get()));
            }
        }
    }
    Ok((!what.is_empty()).then(|| (doc.to_string(), what)))
}

/// Если значение — строка `old` (или `=old`, `^old`), заменить на `new`, сохранив оформление.
fn set_if(item: &mut Item, old: &str, new: &str) -> bool {
    let Some(value) = item.as_value_mut() else { return false };
    let Some(current) = value.as_str() else { return false };
    let prefix = current.strip_suffix(old).filter(|p| p.is_empty() || *p == "=" || *p == "^");
    let Some(prefix) = prefix else { return false };
    let decor = value.decor().clone();
    *value = Value::from(format!("{prefix}{new}"));
    *value.decor_mut() = decor;
    true
}

/// Черновик заметок: заголовок и список коммитов после прошлого тега (без служебных «Release …»).
pub fn draft_notes(dir: &Path, last_tag: Option<&str>, version: &Version) -> String {
    let range = last_tag.map(|t| format!("{t}..HEAD"));
    let mut args = vec!["log", "--no-merges", "--format=%s"];
    if let Some(range) = &range {
        args.push(range);
    }
    let subjects = run::output("git", dir, &args).unwrap_or_default();
    let mut notes = format!("## v{version}\n\n");
    for subject in subjects.lines().filter(|s| !s.trim().is_empty() && !s.starts_with("Release v")) {
        notes.push_str(&format!("- {}\n", subject.trim()));
    }
    notes
}

/// Есть ли в проекте workflow, который собирает выпуск по тегу (тогда выпуск делает CI).
pub fn has_release_workflow(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir.join(".github").join("workflows")) else { return false };
    entries.flatten().any(|e| {
        let text = std::fs::read_to_string(e.path()).unwrap_or_default();
        text.contains("tags:") && (text.contains("rust-release.yml") || text.contains("gh release create"))
    })
}

pub fn tag_exists(dir: &Path, tag: &str) -> bool {
    run::output("git", dir, &["rev-parse", "-q", "--verify", &format!("refs/tags/{tag}")]).is_ok()
}

/// Упаковать по соглашению: `<bin>-X.Y.Z-windows-x64.zip` на каждый бинарник и общий `SHA256SUMS`.
pub fn package(exes: &[(String, PathBuf)], version: &Version, out: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    let mut sums = String::new();
    for (bin, exe) in exes {
        let name = anvil_update::asset_name(bin, version);
        let path = out.join(&name);
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).map_err(|e| e.to_string())?);
        let file_name = exe.file_name().ok_or("no exe name")?.to_string_lossy().into_owned();
        zip.start_file(file_name, zip::write::SimpleFileOptions::default()).map_err(|e| e.to_string())?;
        let data = std::fs::read(exe).map_err(|e| format!("{}: {e}", exe.display()))?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?;
        sums.push_str(&format!("{}  {name}\n", anvil_update::install::sha256(&path)?));
        files.push(path);
    }
    let sums_path = out.join("SHA256SUMS");
    std::fs::write(&sums_path, sums).map_err(|e| e.to_string())?;
    files.push(sums_path);
    Ok(files)
}

/// Адрес API GitHub. Переменная `ANVIL_GITHUB_API` — только для проверок на подставном сервере.
fn api() -> String {
    std::env::var("ANVIL_GITHUB_API").unwrap_or_else(|_| "https://api.github.com".into())
}

/// Создать GitHub Release и залить файлы. Возвращает адрес страницы выпуска.
pub fn publish(repo: &str, tag: &str, notes: &str, prerelease: bool, files: &[PathBuf]) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Created {
        html_url: String,
        upload_url: String,
    }
    let (token, _) = crate::github::find_token();
    let token = token.ok_or("no GitHub token: a release cannot be created")?;
    let http = reqwest::blocking::Client::builder()
        .user_agent(concat!("anvil/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let body = serde_json::json!({
        "tag_name": tag, "name": tag, "body": notes, "prerelease": prerelease, "draft": false,
    });
    let response = http
        .post(format!("{}/repos/{repo}/releases", api()))
        .bearer_auth(&token)
        .header("Accept", "application/vnd.github+json")
        .json(&body)
        .send()
        .map_err(|e| e.without_url().to_string())?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().unwrap_or_default();
        return Err(format!("create release: HTTP {status} {}", text.chars().take(200).collect::<String>()));
    }
    let created: Created = response.json().map_err(|e| e.without_url().to_string())?;
    // `upload_url` — шаблон `…/assets{?name,label}`.
    let upload = created.upload_url.split('{').next().unwrap_or(&created.upload_url).to_owned();
    for file in files {
        let name = file.file_name().ok_or("no file name")?.to_string_lossy().into_owned();
        let kind = if name.ends_with(".zip") { "application/zip" } else { "text/plain" };
        let data = std::fs::read(file).map_err(|e| e.to_string())?;
        let response = http
            .post(format!("{upload}?name={name}"))
            .bearer_auth(&token)
            .header("Content-Type", kind)
            .body(data)
            .send()
            .map_err(|e| e.without_url().to_string())?;
        if !response.status().is_success() {
            return Err(format!("upload {name}: HTTP {}", response.status().as_u16()));
        }
    }
    Ok(created.html_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn bumps_and_base() {
        assert_eq!(bump(&v("0.3.1"), Bump::Patch).to_string(), "0.3.2");
        assert_eq!(bump(&v("0.3.1"), Bump::Minor).to_string(), "0.4.0");
        assert_eq!(bump(&v("0.3.1"), Bump::Major).to_string(), "1.0.0");
        assert_eq!(bump(&v("1.0.0-rc.1"), Bump::Patch).to_string(), "1.0.0");
        // Amber: в Cargo.toml 0.1.0, а тег уже v0.3.0 — считать от тега.
        assert_eq!(base(Some("0.1.0"), Some("v0.3.0")).to_string(), "0.3.0");
        assert_eq!(base(Some("0.2.0"), None).to_string(), "0.2.0");
    }

    #[test]
    fn manifest_versions_and_path_dependencies() {
        let text = r#"[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.1.0"   # общая версия
edition = "2024"

[package]
name = "app"
version.workspace = true

[dependencies]
core = { version = "0.1.0", path = "crates/core" }
serde = "0.1.0"
other = { version = "0.1.0" }

[target.'cfg(windows)'.dependencies]
winhelp = { path = "crates/winhelp", version = "=0.1.0" }
"#;
        let (out, what) = edit_manifest(text, "0.1.0", "0.2.0").unwrap().unwrap();
        assert!(out.contains(r#"version = "0.2.0"   # общая версия"#), "оформление и комментарий на месте:\n{out}");
        assert!(out.contains(r#"core = { version = "0.2.0", path = "crates/core" }"#));
        assert!(out.contains(r#"serde = "0.1.0""#), "внешние зависимости не трогаются");
        assert!(out.contains(r#"other = { version = "0.1.0" }"#), "без path — не своя");
        assert!(out.contains(r#"version = "=0.2.0""#));
        assert!(out.contains("version.workspace = true"));
        assert_eq!(
            what,
            ["workspace.package.version", "dependencies.core", "target.cfg(windows).dependencies.winhelp"]
        );
        assert!(edit_manifest("[package]\nname = \"x\"\nversion = \"9.9.9\"\n", "0.1.0", "0.2.0").unwrap().is_none());
    }

    #[test]
    fn package_by_convention() {
        let dir = std::env::temp_dir().join(format!("anvil-release-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("demo.exe");
        std::fs::write(&exe, b"binary").unwrap();
        let files = package(&[("demo".into(), exe)], &v("0.2.1"), &dir.join("out")).unwrap();
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, [anvil_update::asset_name("demo", &v("0.2.1")), "SHA256SUMS".into()]);
        let sums = std::fs::read_to_string(&files[1]).unwrap();
        assert_eq!(
            anvil_update::expected_sum(&sums, &names[0]),
            Some(anvil_update::install::sha256(&files[0]).unwrap())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
