//! Проекты Godot и Unity: версия движка, собранная игра, где стоит редактор.
//!
//! Всё читается из файлов проекта, без запуска движка: `project.godot` и `export_presets.cfg`
//! у Godot, `ProjectSettings/ProjectVersion.txt` у Unity.

use std::path::{Path, PathBuf};

use crate::registry::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Godot,
    Unity,
}

/// Что известно о проекте движка.
#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub engine: Engine,
    /// `4.7` у Godot (из `config/features`), `2021.3.45f2` у Unity.
    pub version: Option<String>,
    /// Собранная игра под Windows: у Godot — `export_path` пресета «Windows Desktop».
    pub export: Option<PathBuf>,
    /// Когда игра собрана (время файла), секунды Unix; `None` — файла нет.
    pub exported_at: Option<i64>,
    /// Проект открыт в редакторе: у Unity есть `Temp/UnityLockfile`, пока он открыт.
    pub open: bool,
    /// Редактор Unity нужной версии, если он стоит (у Godot редактор общий — ищет окно).
    pub editor: Option<PathBuf>,
    /// Рендер Godot: `Forward Plus`, `Mobile`, `GL Compatibility`.
    pub render: Option<String>,
    /// Размер собранной игры, байт.
    pub size: Option<u64>,
}

/// Прочитать проект движка. Для Rust и просто git — `None`.
pub fn read(dir: &Path, kind: Kind) -> Option<Info> {
    match kind {
        Kind::Godot => Some(godot(dir)),
        Kind::Unity => Some(unity(dir)),
        Kind::Rust | Kind::Git => None,
    }
}

fn godot(dir: &Path) -> Info {
    let project = std::fs::read_to_string(dir.join("project.godot")).unwrap_or_default();
    let presets = std::fs::read_to_string(dir.join("export_presets.cfg")).unwrap_or_default();
    let export = windows_export_path(&presets).map(|p| dir.join(p));
    let exported_at = export.as_deref().and_then(modified);
    let size = export.as_deref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len());
    let render = project
        .lines()
        .find(|l| l.trim_start().starts_with("config/features="))
        .and_then(|l| l.split('"').skip(1).step_by(2).find(|v| RENDERERS.contains(v)))
        .map(str::to_owned);
    Info {
        engine: Engine::Godot,
        version: godot_version(&project),
        export,
        exported_at,
        open: false,
        editor: None,
        render,
        size,
    }
}

fn unity(dir: &Path) -> Info {
    let text = std::fs::read_to_string(dir.join("ProjectSettings").join("ProjectVersion.txt")).unwrap_or_default();
    let version = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("m_EditorVersion:"))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    let open = dir.join("Temp").join("UnityLockfile").exists();
    let editor = version.as_deref().and_then(unity_editor);
    Info { engine: Engine::Unity, version, export: None, exported_at: None, open, editor, render: None, size: None }
}

/// Версия Godot из `config/features=PackedStringArray("4.7", "Forward Plus")` — первый элемент,
/// похожий на номер версии.
fn godot_version(project: &str) -> Option<String> {
    let line = project.lines().find(|l| l.trim_start().starts_with("config/features="))?;
    line.split('"').skip(1).step_by(2).find(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit())).map(str::to_owned)
}

/// `export_path` первого пресета под Windows (`platform="Windows Desktop"`), путь относительно проекта.
fn windows_export_path(presets: &str) -> Option<String> {
    let mut windows = false;
    for line in presets.lines().map(str::trim) {
        if line.starts_with("[preset.") && !line.contains(".options") {
            windows = false;
        } else if let Some(platform) = line.strip_prefix("platform=") {
            windows = platform.trim_matches('"') == "Windows Desktop";
        } else if windows && let Some(path) = line.strip_prefix("export_path=") {
            let path = path.trim_matches('"').trim_start_matches("res://");
            return (!path.is_empty()).then(|| path.replace('/', std::path::MAIN_SEPARATOR_STR));
        }
    }
    None
}

fn modified(path: &Path) -> Option<i64> {
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    time.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

/// Редактор Unity нужной версии: папка Unity Hub по умолчанию или та, что задана в Hub.
pub fn unity_editor(version: &str) -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from(r"C:\Program Files\Unity\Hub\Editor")];
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let file = PathBuf::from(appdata).join("UnityHub").join("secondaryInstallPath.json");
        if let Ok(text) = std::fs::read_to_string(file) {
            let path = text.trim().trim_matches('"').replace("\\\\", "\\");
            if !path.is_empty() {
                roots.insert(0, PathBuf::from(path));
            }
        }
    }
    roots.into_iter().map(|root| root.join(version).join("Editor").join("Unity.exe")).find(|exe| exe.is_file())
}

/// Рендеры Godot 4, как их пишет `config/features` (там же версия и, например, `C#`).
const RENDERERS: [&str; 3] = ["Forward Plus", "Mobile", "GL Compatibility"];

/// Версия редактора Godot по имени файла: `Godot_v4.7.1-stable_win64.exe` → `4.7.1`.
pub fn godot_editor_version(exe: &Path) -> Option<String> {
    let name = exe.file_name()?.to_string_lossy().to_lowercase();
    let rest = name.split("_v").nth(1)?;
    let version: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    (!version.is_empty()).then_some(version)
}

/// Папка шаблонов экспорта для этого редактора: `Godot_v4.7.1-stable_win64.exe` → `4.7.1.stable`,
/// `Godot_v4.5-beta2_mono_win64.exe` → `4.5.beta2.mono`. Не разобрать имя — `None`.
pub fn godot_templates_name(exe: &Path) -> Option<String> {
    let name = exe.file_name()?.to_string_lossy().to_lowercase();
    let rest = name.split("_v").nth(1)?;
    let (version, tail) = rest.split_once('-')?;
    let status: String = tail.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    let valid = !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.') && !status.is_empty();
    let mono = if tail.contains("mono") { ".mono" } else { "" };
    valid.then(|| format!("{version}.{status}{mono}"))
}

/// Стоят ли шаблоны экспорта для этого редактора: `%APPDATA%\Godot\export_templates\<папка>`, а у
/// переносного редактора (рядом `_sc_`) — `editor_data\export_templates\<папка>` возле exe.
pub fn godot_templates(exe: &Path, name: &str) -> bool {
    let beside = exe.parent().map(|d| d.join("editor_data").join("export_templates").join(name));
    let appdata =
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Godot").join("export_templates").join(name));
    beside.into_iter().chain(appdata).any(|d| d.is_dir())
}

/// Редактор Godot: путь из `anvil.toml` (`godot = "…"`), иначе первый `godot*.exe` в `PATH`
/// (не консольный вариант).
pub fn godot_editor(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = configured {
        return path.is_file().then(|| path.to_path_buf());
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|dir| {
        let entries = std::fs::read_dir(dir).ok()?;
        entries.flatten().map(|e| e.path()).find(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            name.starts_with("godot") && name.ends_with(".exe") && !name.contains("console")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn godot_version_and_export_path() {
        let project = "config/name=\"IQube\"\nconfig/features=PackedStringArray(\"4.7\", \"Forward Plus\")\n";
        assert_eq!(godot_version(project).as_deref(), Some("4.7"));
        assert_eq!(godot_version("config/name=\"x\"\n"), None);
        let presets = "[preset.0]\n\nname=\"Web\"\nplatform=\"Web\"\nexport_path=\"web/index.html\"\n\n\
            [preset.0.options]\n\n[preset.1]\n\nname=\"Windows Desktop\"\nplatform=\"Windows Desktop\"\n\
            export_path=\"build/IQube.exe\"\n";
        let path = windows_export_path(presets).unwrap();
        assert!(path.starts_with("build") && path.ends_with("IQube.exe"));
        assert_eq!(windows_export_path("[preset.0]\nplatform=\"Web\"\nexport_path=\"a.html\"\n"), None);
        assert_eq!(godot_editor_version(Path::new("Godot_v4.7.1-stable_win64.exe")).as_deref(), Some("4.7.1"));
        let name = |f: &str| godot_templates_name(Path::new(f));
        assert_eq!(name("Godot_v4.7.1-stable_win64.exe").as_deref(), Some("4.7.1.stable"));
        assert_eq!(name("Godot_v4.5-beta2_mono_win64.exe").as_deref(), Some("4.5.beta2.mono"));
        assert_eq!(name("godot.exe"), None);
    }

    #[test]
    fn unity_version_from_project_settings() {
        let dir = std::env::temp_dir().join(format!("anvil-engines-unity-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("ProjectSettings")).unwrap();
        std::fs::write(
            dir.join("ProjectSettings").join("ProjectVersion.txt"),
            "m_EditorVersion: 2021.3.45f2\nm_EditorVersionWithRevision: 2021.3.45f2 (abc)\n",
        )
        .unwrap();
        let info = read(&dir, Kind::Unity).unwrap();
        assert_eq!(info.version.as_deref(), Some("2021.3.45f2"));
        assert_eq!(info.engine, Engine::Unity);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
