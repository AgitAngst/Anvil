//! Настройки Anvil: `anvil.toml` рядом с exe (портативный режим, если файл там есть)
//! или `%APPDATA%\Anvil\anvil.toml`.

use std::path::{Path, PathBuf};

use anvil_ui::CommonSettings;
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "anvil.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Папки, в которых ищутся проекты: сама папка и её прямые подпапки с `Cargo.toml`.
    pub roots: Vec<PathBuf>,
    /// Проекты, скрытые из списка (полные пути).
    pub hidden: Vec<PathBuf>,
    /// Как часто спрашивать origin, минут; 0 — только по кнопке.
    pub fetch_minutes: u32,
    /// Последний выбранный проект.
    pub selected: Option<PathBuf>,
    pub common: CommonSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            hidden: Vec::new(),
            fetch_minutes: 15,
            selected: None,
            common: CommonSettings::default(),
        }
    }
}

pub fn path() -> PathBuf {
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        let portable = dir.join(FILE_NAME);
        if portable.exists() {
            return portable;
        }
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Anvil").join(FILE_NAME)
}

/// Прочитать настройки. Файла нет — первый запуск: корень угадывается по месту exe.
/// Файл испорчен — настройки по умолчанию и текст ошибки, файл не трогается до первого сохранения.
pub fn load(path: &Path) -> (Config, Option<String>) {
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str(text.trim_start_matches('\u{feff}')) {
            Ok(config) => (config, None),
            Err(e) => (Config::default(), Some(e.to_string())),
        },
        Err(_) => (Config { roots: guess_root().into_iter().collect(), ..Config::default() }, None),
    }
}

pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    let text = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Через временный файл: оборванная запись не портит настройки.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Если exe лежит внутри репозитория Anvil (`…\Anvil\target\release`), проекты — рядом с ним.
fn guess_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|dir| dir.join("crates").join("anvil").join("Cargo.toml").exists())?
        .parent()
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let config = Config {
            roots: vec![PathBuf::from(r"D:\dev_personal")],
            hidden: vec![PathBuf::from(r"D:\dev_personal\old")],
            fetch_minutes: 5,
            selected: Some(PathBuf::from(r"D:\dev_personal\amber")),
            common: CommonSettings::default(),
        };
        let text = toml::to_string_pretty(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let config: Config = toml::from_str("roots = ['C:/src']").unwrap();
        assert_eq!(config.fetch_minutes, 15);
        assert!(config.common.check_updates);
    }
}
