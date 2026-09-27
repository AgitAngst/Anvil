//! Из какого коммита собран exe: `cache\builds.json`. Anvil записывает это, когда сам собирает
//! из кода, — отсюда «сборка 2353af9» на Пульте. Сборка не из Anvil — хеша нет, только дата файла.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Build {
    /// Короткий хеш коммита, из которого собрано.
    pub commit: String,
    /// В дереве были незакоммиченные правки.
    pub dirty: bool,
    /// Когда собрано, секунды Unix.
    pub at: i64,
}

/// Ключ — путь exe в нижнем регистре.
pub type Builds = HashMap<String, Build>;

pub fn key(exe: &Path) -> String {
    exe.to_string_lossy().replace('/', "\\").to_lowercase()
}

pub fn load(path: &Path) -> Builds {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(path: &Path, builds: &Builds) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Через временный файл: оборванная запись не теряет всё записанное.
    if let Ok(text) = serde_json::to_string_pretty(builds) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

/// Что показать в чипе: хеш коммита.
pub fn label(build: &Build) -> String {
    build.commit.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_case_and_slashes() {
        assert_eq!(key(Path::new("D:/x/Target/release/a.exe")), key(Path::new(r"d:\x\target\RELEASE\a.exe")));
        let b = Build { commit: "2353af9".into(), dirty: true, at: 0 };
        assert_eq!(label(&b), "2353af9");
    }
}
