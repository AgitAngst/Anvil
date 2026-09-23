//! Установка обновления: скачать, сверить SHA-256, распаковать, подменить, перезапуститься.
//!
//! Два расклада:
//! - **Портативный** — exe лежит где угодно. Новые файлы кладутся на место старых, а старые
//!   переименовываются в `имя.old-<время>`: Windows не даёт перезаписать запущенный exe, но
//!   разрешает его переименовать. Остатки убирает [`cleanup`] при следующем запуске.
//! - **Установленный Anvil'ом** — `…\<app>\versions\<версия>\` и `…\<app>\current` (junction на
//!   активную версию). Новая версия распаковывается рядом, `current` переключается; прежние
//!   остаются для отката.
//!
//! Что бы ни сломалось по дороге, рабочей остаётся старая версия.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

/// Папка для скачивания и распаковки — рядом с программой, на том же диске: переименования мгновенны.
const STAGING: &str = ".anvil-update";
/// Сколько прежних версий держать в установленном раскладе (для отката).
const KEEP_VERSIONS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    /// exe и его файлы — в этой папке.
    Portable { dir: PathBuf },
    /// `root\versions\<версия>` и `root\current`.
    Managed { root: PathBuf },
}

impl Layout {
    /// Как установлена запущенная программа.
    pub fn detect() -> Result<Layout, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Ok(Self::of(&exe))
    }

    pub fn of(exe: &Path) -> Layout {
        let dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase());
        if name(&dir).as_deref() == Some("current")
            && let Some(root) = dir.parent()
            && root.join("versions").is_dir()
        {
            return Layout::Managed { root: root.to_path_buf() };
        }
        if let Some(versions) = dir.parent()
            && name(versions).as_deref() == Some("versions")
            && let Some(root) = versions.parent()
        {
            return Layout::Managed { root: root.to_path_buf() };
        }
        Layout::Portable { dir }
    }

    fn staging(&self) -> PathBuf {
        match self {
            Layout::Portable { dir } => dir.join(STAGING),
            Layout::Managed { root } => root.join(STAGING),
        }
    }
}

fn stamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Скачать в файл, сообщая ход `(скачано, всего)`.
pub fn download(
    http: &reqwest::blocking::Client,
    url: &str,
    to: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    let mut response = http.get(url).send().map_err(|e| e.without_url().to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let total = response.content_length().unwrap_or(0);
    let mut file = fs::File::create(to).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = response.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress(done, total);
    }
    file.sync_all().map_err(|e| e.to_string())
}

pub fn sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Распаковать архив в пустую папку. Пути вне папки (`..`, абсолютные) отвергаются целиком;
/// единственная общая папка верхнего уровня снимается: `app-1.2.3\app.exe` → `app.exe`.
pub fn extract(zip_path: &Path, into: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.enclosed_name().ok_or_else(|| format!("unsafe path in archive: {}", entry.name()))?;
        entries.push((i, name, entry.is_dir()));
    }
    let prefix = common_top_dir(entries.iter().map(|(_, n, d)| (n.as_path(), *d)));
    fs::create_dir_all(into).map_err(|e| e.to_string())?;
    for (i, name, is_dir) in entries {
        let relative = match &prefix {
            Some(p) => name.strip_prefix(p).unwrap_or(&name).to_path_buf(),
            None => name,
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = into.join(&relative);
        if is_dir {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let mut out = fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Если все файлы лежат в одной папке верхнего уровня — её имя.
fn common_top_dir<'a>(entries: impl Iterator<Item = (&'a Path, bool)>) -> Option<PathBuf> {
    let mut top: Option<PathBuf> = None;
    for (name, is_dir) in entries {
        let mut parts = name.components();
        let first = match parts.next()? {
            Component::Normal(first) => PathBuf::from(first),
            _ => return None,
        };
        // Файл прямо в корне архива — общей папки нет.
        if parts.next().is_none() && !is_dir {
            return None;
        }
        match &top {
            None => top = Some(first),
            Some(t) if *t == first => {}
            Some(_) => return None,
        }
    }
    top
}

/// Все файлы папки, пути относительно неё.
fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in fs::read_dir(dir.join(&rel)).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = rel.join(entry.file_name());
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    Ok(out)
}

/// Портативная установка: новые файлы из `staged` — на места старых, старые — в `*.old-<время>`.
/// Не удалось на каком-то файле — всё возвращается как было.
pub fn swap_in(staged: &Path, dir: &Path) -> Result<(), String> {
    let suffix = format!(".old-{}", stamp());
    let mut done: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let result = (|| {
        for rel in files(staged)? {
            let target = dir.join(&rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let backup = if target.exists() {
                let mut name = target.file_name().unwrap_or_default().to_os_string();
                name.push(&suffix);
                let backup = target.with_file_name(name);
                fs::rename(&target, &backup).map_err(|e| format!("{}: {e}", target.display()))?;
                Some(backup)
            } else {
                None
            };
            done.push((target.clone(), backup));
            fs::rename(staged.join(&rel), &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
        Ok(())
    })();
    if result.is_err() {
        // Откат в обратном порядке: убрать новое, вернуть старое.
        for (target, backup) in done.into_iter().rev() {
            if let Some(backup) = backup {
                let _ = fs::remove_file(&target);
                let _ = fs::rename(&backup, &target);
            } else {
                let _ = fs::remove_file(&target);
            }
        }
    }
    result
}

/// Установленный расклад: версия из `staged` → `root\versions\<версия>`, `current` → на неё.
pub fn install_version(staged: &Path, root: &Path, version: &str) -> Result<PathBuf, String> {
    let versions = root.join("versions");
    fs::create_dir_all(&versions).map_err(|e| e.to_string())?;
    let target = versions.join(version);
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    fs::rename(staged, &target).map_err(|e| e.to_string())?;
    switch_current(root, &target)?;
    prune_versions(&versions, &target);
    Ok(target)
}

/// Переключить `root\current` на папку версии: новая ссылка рядом, старая — прочь, новую — на место.
pub fn switch_current(root: &Path, target: &Path) -> Result<(), String> {
    let current = root.join("current");
    let fresh = root.join("current.new");
    let _ = remove_link(&fresh);
    make_link(&fresh, target)?;
    if current.exists() || current.symlink_metadata().is_ok() {
        remove_link(&current).map_err(|e| format!("{}: {e}", current.display()))?;
    }
    fs::rename(&fresh, &current).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn make_link(link: &Path, target: &Path) -> Result<(), String> {
    // Junction не требует прав администратора, в отличие от символьной ссылки.
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()) }
}

#[cfg(not(windows))]
fn make_link(link: &Path, target: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|e| e.to_string())
}

/// Убрать ссылку, не трогая то, на что она указывает.
fn remove_link(link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        fs::remove_dir(link)
    }
    #[cfg(not(windows))]
    {
        fs::remove_file(link)
    }
}

/// Оставить активную и ещё несколько самых свежих версий.
fn prune_versions(versions: &Path, active: &Path) {
    let Ok(entries) = fs::read_dir(versions) else { return };
    let mut dirs: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path() != active)
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.0));
    for (_, dir) in dirs.into_iter().skip(KEEP_VERSIONS - 1) {
        let _ = fs::remove_dir_all(dir);
    }
}

/// Скачать архив выпуска, сверить сумму и поставить. Возвращает exe, который запускать после.
pub fn install(
    http: &reqwest::blocking::Client,
    layout: &Layout,
    asset_url: &str,
    asset_name: &str,
    expected_sha256: &str,
    version: &str,
    progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_name = exe.file_name().ok_or("no exe name")?.to_os_string();
    let staging = layout.staging();
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let zip_path = staging.join(asset_name);
    download(http, asset_url, &zip_path, progress)?;
    let actual = sha256(&zip_path)?;
    if actual != expected_sha256 {
        let _ = fs::remove_dir_all(&staging);
        return Err("the downloaded archive does not match SHA256SUMS — nothing was installed".into());
    }
    let unpacked = staging.join(version);
    extract(&zip_path, &unpacked)?;
    if !unpacked.join(&exe_name).is_file() {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("{} is missing in the archive", exe_name.to_string_lossy()));
    }
    let next = match layout {
        Layout::Portable { dir } => {
            swap_in(&unpacked, dir)?;
            dir.join(&exe_name)
        }
        Layout::Managed { root } => install_version(&unpacked, root, version)?.join(&exe_name),
    };
    let _ = fs::remove_dir_all(&staging);
    Ok(next)
}

/// Запустить программу заново с теми же аргументами. Окно текущей закрывает сама программа.
pub fn restart(exe: &Path) -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let dir = std::env::current_dir().unwrap_or_else(|_| exe.parent().unwrap_or(Path::new(".")).to_path_buf());
    std::process::Command::new(exe).args(args).current_dir(dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Убрать следы прошлого обновления: `*.old-<время>` и папку загрузки. Звать при запуске программы.
///
/// Сразу после перезапуска прежний процесс ещё держит свой exe, поэтому уборка идёт в фоне и
/// повторяется несколько секунд; что осталось занятым — уберётся при следующем запуске.
pub fn cleanup() {
    let Ok(layout) = Layout::detect() else { return };
    std::thread::spawn(move || {
        for wait in [0, 1, 2, 4, 8] {
            std::thread::sleep(std::time::Duration::from_secs(wait));
            if sweep(&layout) {
                return;
            }
        }
    });
}

/// Одна попытка уборки. `true` — убрано всё.
fn sweep(layout: &Layout) -> bool {
    let staging = layout.staging();
    let mut clean = fs::remove_dir_all(&staging).is_ok() || !staging.exists();
    if let Layout::Portable { dir } = layout
        && let Ok(list) = files_shallow(dir)
    {
        for path in list.into_iter().filter(|p| is_old_copy(p)) {
            clean &= fs::remove_file(path).is_ok();
        }
    }
    clean
}

/// Файлы папки и её подпапок до второго уровня — dll и ресурсы рядом с exe.
fn files_shallow(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == STAGING) {
                continue;
            }
            if let Ok(inner) = fs::read_dir(&path) {
                out.extend(inner.flatten().map(|e| e.path()).filter(|p| p.is_file()));
            }
        } else {
            out.push(path);
        }
    }
    Ok(out)
}

/// `имя.old-1790176095` — копия, оставленная обновлением.
fn is_old_copy(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.rsplit_once(".old-")
        .is_some_and(|(_, digits)| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("anvil-update-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn layouts() {
        let root = temp("layout");
        fs::create_dir_all(root.join("versions").join("1.0.0")).unwrap();
        assert_eq!(Layout::of(&root.join("current").join("app.exe")), Layout::Managed { root: root.clone() });
        assert_eq!(
            Layout::of(&root.join("versions").join("1.0.0").join("app.exe")),
            Layout::Managed { root: root.clone() }
        );
        assert_eq!(Layout::of(&root.join("app.exe")), Layout::Portable { dir: root.clone() });
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extract_strips_single_top_dir_and_rejects_escape() {
        let dir = temp("extract");
        let zip_path = dir.join("a.zip");
        make_zip(&zip_path, &[("app-1.0.0/app.exe", b"new"), ("app-1.0.0/data/x.txt", b"x")]);
        extract(&zip_path, &dir.join("out")).unwrap();
        assert_eq!(fs::read(dir.join("out").join("app.exe")).unwrap(), b"new");
        assert!(dir.join("out").join("data").join("x.txt").is_file());

        let evil = dir.join("evil.zip");
        make_zip(&evil, &[("../escape.txt", b"x")]);
        assert!(extract(&evil, &dir.join("out2")).is_err());
        assert!(!dir.join("escape.txt").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn swap_keeps_old_copies_and_cleanup_pattern() {
        let dir = temp("swap");
        let app = dir.join("app");
        let staged = dir.join("staged");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(staged.join("data")).unwrap();
        fs::write(app.join("app.exe"), b"old").unwrap();
        fs::write(staged.join("app.exe"), b"new").unwrap();
        fs::write(staged.join("data").join("x.txt"), b"x").unwrap();
        swap_in(&staged, &app).unwrap();
        assert_eq!(fs::read(app.join("app.exe")).unwrap(), b"new");
        let old: Vec<_> = fs::read_dir(&app).unwrap().flatten().map(|e| e.path()).filter(|p| is_old_copy(p)).collect();
        assert_eq!(old.len(), 1);
        assert_eq!(fs::read(&old[0]).unwrap(), b"old");
        assert!(!is_old_copy(Path::new("notes.old-draft")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn managed_install_switches_current() {
        let root = temp("managed");
        for (version, body) in [("1.0.0", b"one"), ("1.1.0", b"two")] {
            let staged = root.join(format!("staged-{version}"));
            fs::create_dir_all(&staged).unwrap();
            fs::write(staged.join("app.exe"), body).unwrap();
            install_version(&staged, &root, version).unwrap();
        }
        assert_eq!(fs::read(root.join("current").join("app.exe")).unwrap(), b"two");
        assert!(root.join("versions").join("1.0.0").join("app.exe").is_file(), "прежняя версия — для отката");
        remove_link(&root.join("current")).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sha256_of_file() {
        let dir = temp("sha");
        fs::write(dir.join("f"), b"abc").unwrap();
        assert_eq!(sha256(&dir.join("f")).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        fs::remove_dir_all(dir).unwrap();
    }
}
