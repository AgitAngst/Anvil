//! Установленные программы: `%LOCALAPPDATA%\Programs\<бинарник>\versions\<версия>` и `current`,
//! ярлык в «Пуске». Расклад общий с `anvil-update` — что бы ни поставило версию, Anvil или сама
//! программа, её видно и её можно откатить.
//!
//! Данные программ (`%APPDATA%\…`) здесь не живут и не трогаются никогда.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::run;

/// Установка одного бинарника.
#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    pub root: PathBuf,
    /// Активная версия — куда указывает `current`.
    pub current: Option<String>,
    /// Все версии, свежие первыми: имя папки и время появления.
    pub versions: Vec<(String, i64)>,
}

pub fn programs_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Programs")
}

pub fn root(bin: &str) -> PathBuf {
    programs_dir().join(bin)
}

/// Что установлено для бинарника; `None` — не установлен.
pub fn scan(bin: &str) -> Option<Installed> {
    anvil_update::install::sweep_removed(&programs_dir());
    let root = root(bin);
    if !root.join("versions").is_dir() {
        return None;
    }
    sweep_leftovers(&root);
    let versions = anvil_update::install::versions(&root)
        .into_iter()
        .map(|(name, time)| (name, time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)))
        .collect();
    Some(Installed { current: anvil_update::install::current_version(&root), versions, root })
}

/// Убрать забытые временные папки установки (`.anvil-update`, `.stage-…`, `.anvil-stage`).
/// Только старше десяти минут — свежая может принадлежать идущей прямо сейчас установке.
fn sweep_leftovers(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let temporary = name == ".anvil-update" || name == ".anvil-stage" || name.starts_with(".stage-");
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 600);
        if temporary && old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Лежит ли путь внутри папки установки (без учёта регистра — Windows так и сравнивает).
pub fn inside(path: &Path, root: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let (path, root) = (norm(path), norm(root));
    path.starts_with(&root) && path[root.len()..].starts_with('/')
}

/// Имя для людей: `amber-desktop` → `Amber Desktop`.
pub fn display_name(bin: &str) -> String {
    bin.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Ярлык в «Пуске»: `%APPDATA%\Microsoft\Windows\Start Menu\Programs\<Имя>.lnk`.
pub fn shortcut_path(bin: &str) -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(format!("{}.lnk", display_name(bin)))
}

/// Создать или обновить ярлык на `current\<exe>`: ярлык не устаревает при смене версии.
pub fn create_shortcut(bin: &str) -> Result<PathBuf, String> {
    let lnk = shortcut_path(bin);
    let current = root(bin).join("current");
    let target = current.join(format!("{bin}{}", std::env::consts::EXE_SUFFIX));
    #[cfg(windows)]
    {
        // Пути — через переменные окружения: так их не нужно экранировать внутри скрипта.
        let script = "$s = (New-Object -ComObject WScript.Shell).CreateShortcut($env:ANVIL_LNK); \
                      $s.TargetPath = $env:ANVIL_TARGET; $s.WorkingDirectory = $env:ANVIL_DIR; \
                      $s.Description = $env:ANVIL_DESC; $s.Save()";
        let out = run::command("powershell.exe", Path::new("."))
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("ANVIL_LNK", &lnk)
            .env("ANVIL_TARGET", &target)
            .env("ANVIL_DIR", &current)
            .env("ANVIL_DESC", display_name(bin))
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (&target, run::command);
    }
    Ok(lnk)
}

pub fn remove_shortcut(bin: &str) {
    let _ = std::fs::remove_file(shortcut_path(bin));
}

/// Метка версии локальной сборки: `0.1.0-3f89301`, с `-dirty`, если есть незакоммиченное.
/// Так две сборки одной версии из разных коммитов не затирают друг друга.
pub fn local_label(version: &str, commit: Option<&str>, dirty: bool) -> String {
    let mut label = version.to_owned();
    if let Some(hash) = commit {
        label.push('-');
        label.push_str(hash);
    }
    if dirty {
        label.push_str("-dirty");
    }
    label
}

/// Разложить свежесобранный exe во временную папку рядом с установкой — оттуда она целиком
/// переедет в `versions`, и пустого родителя не останется.
pub fn stage_local(exe: &Path, root: &Path, label: &str) -> Result<PathBuf, String> {
    let staging = root.join(format!(".stage-{label}"));
    anvil_update::install::remove_dir_patiently(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let name = exe.file_name().ok_or("no exe name")?;
    std::fs::copy(exe, staging.join(name)).map_err(|e| format!("{}: {e}", exe.display()))?;
    Ok(staging)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_labels() {
        assert_eq!(display_name("amber-desktop"), "Amber Desktop");
        assert_eq!(display_name("anvil"), "Anvil");
        assert_eq!(local_label("0.1.0", Some("3f89301"), false), "0.1.0-3f89301");
        assert_eq!(local_label("0.1.0", Some("3f89301"), true), "0.1.0-3f89301-dirty");
        assert!(shortcut_path("amber-desktop").ends_with("Amber Desktop.lnk"));
        assert!(inside(Path::new("C:/Users/A/Programs/demo/current/demo.exe"), Path::new("c:/users/a/programs/demo")));
        assert!(!inside(Path::new("C:/Users/A/Programs/demo2/demo.exe"), Path::new("C:/Users/A/Programs/demo")));
    }
}
