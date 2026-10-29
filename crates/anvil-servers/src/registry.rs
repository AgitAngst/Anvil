//! Реестр модулей: встроенные манифесты и файлы `modules/*.toml` из папки настроек.
//!
//! Битый манифест реестр не роняет: файл попадает в список «с причиной», остальные модули работают. Скрытые модули
//! (`hidden = true`) отдаются, только когда включён переключатель «Скрытые модули»; то же правило у битых файлов,
//! которые сами себя объявили скрытыми, — чтобы по списку ошибок нельзя было узнать о скрытом модуле.

use std::path::{Path, PathBuf};

use crate::manifest::{Manifest, ManifestError};

/// Подпапка настроек с манифестами владельца.
pub const MODULES_DIR: &str = "modules";

/// Самый большой манифест, который читаем.
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// Откуда модуль.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Встроен в программу; строка — его имя для списка.
    Builtin(String),
    /// Файл в папке настроек.
    File(PathBuf),
}

impl Origin {
    /// Имя для списка: имя встроенного модуля или имя файла.
    pub fn label(&self) -> String {
        match self {
            Origin::Builtin(name) => name.clone(),
            Origin::File(path) => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }
    }
}

/// Годный модуль.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub manifest: Manifest,
    pub origin: Origin,
}

/// Манифест, который не принят, и почему.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken {
    pub origin: Origin,
    /// Причина по-английски, как её назвал разбор (`no install command (commands.install)`).
    pub reason: String,
    /// Файл сам объявил `hidden = true` (читается, даже когда остальное не разобралось).
    pub hidden: bool,
}

/// Все модули, найденные при загрузке.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    modules: Vec<Module>,
    broken: Vec<Broken>,
}

impl Registry {
    /// Собрать реестр: сначала встроенные (`(имя, текст манифеста)`), затем `modules/*.toml` из `config_dir`.
    /// Нет папки — только встроенные. Файлы читаются по имени, по алфавиту; повтор `id` отвергается (встроенный
    /// выигрывает у файла, первый файл — у второго).
    pub fn load(builtin: &[(&str, &str)], config_dir: Option<&Path>) -> Registry {
        Registry::load_from(builtin, config_dir.map(|d| d.join(MODULES_DIR)).as_deref())
    }

    /// То же, но папка манифестов названа прямо.
    pub fn load_from(builtin: &[(&str, &str)], modules_dir: Option<&Path>) -> Registry {
        let mut registry = Registry::default();
        for (name, text) in builtin {
            registry.add(Origin::Builtin((*name).to_owned()), text);
        }
        let Some(dir) = modules_dir else { return registry };
        let Ok(entries) = std::fs::read_dir(dir) else { return registry };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("toml")))
            .collect();
        files.sort();
        for file in files {
            match read_limited(&file) {
                Ok(text) => registry.add(Origin::File(file), &text),
                Err(reason) => registry.broken.push(Broken { origin: Origin::File(file), reason, hidden: false }),
            }
        }
        registry
    }

    fn add(&mut self, origin: Origin, text: &str) {
        match Manifest::parse(text) {
            Ok(manifest) => {
                if let Some(first) = self.modules.iter().find(|m| m.manifest.id == manifest.id) {
                    let reason = format!("duplicate id `{}`: already defined by {}", manifest.id, first.origin.label());
                    self.broken.push(Broken { origin, reason, hidden: manifest.hidden });
                } else {
                    self.modules.push(Module { manifest, origin });
                }
            }
            Err(e) => {
                tracing::warn!("module manifest {} rejected: {e}", origin.label());
                self.broken.push(Broken { origin, reason: e.to_string(), hidden: declares_hidden(text, &e) });
            }
        }
    }

    /// Модули для показа: скрытые — только при `show_hidden`.
    pub fn modules(&self, show_hidden: bool) -> impl Iterator<Item = &Module> {
        self.modules.iter().filter(move |m| show_hidden || !m.manifest.hidden)
    }

    /// Модуль по `id` с тем же правилом скрытых: скрытого при выключенном переключателе «нет».
    pub fn get(&self, id: &str, show_hidden: bool) -> Option<&Module> {
        self.modules(show_hidden).find(|m| m.manifest.id == id)
    }

    /// Непринятые манифесты (для списка в настройках); скрытые — только при `show_hidden`.
    pub fn broken(&self, show_hidden: bool) -> impl Iterator<Item = &Broken> {
        self.broken.iter().filter(move |b| show_hidden || !b.hidden)
    }
}

fn read_limited(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("cannot read: {e}"))?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(format!("file is larger than {} KiB", MAX_MANIFEST_BYTES / 1024));
    }
    std::fs::read_to_string(path).map_err(|e| format!("cannot read: {e}"))
}

/// Объявил ли файл `hidden = true`, даже если он не прошёл проверку.
fn declares_hidden(text: &str, error: &ManifestError) -> bool {
    if matches!(error, ManifestError::Syntax(_)) {
        // Не TOML — поле не прочесть. Строка `hidden = true` в тексте — достаточный признак.
        return text.lines().any(|l| {
            let l: String = l.chars().filter(|c| !c.is_whitespace()).collect();
            l == "hidden=true"
        });
    }
    toml::from_str::<toml::Table>(text)
        .ok()
        .and_then(|t| t.get("hidden").and_then(toml::Value::as_bool))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::EXAMPLE;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("anvil-servers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(MODULES_DIR)).unwrap();
        dir
    }

    fn hidden_module(id: &str) -> String {
        format!(
            "id = \"{id}\"\nhidden = true\n[source]\nrepo = \"octo/{id}\"\nassets = {{ x86_64 = \"{id}\" }}\n\
             [commands]\ninstall = \"true\"\n"
        )
    }

    #[test]
    fn builtin_and_file_modules_are_listed_in_order() {
        let dir = temp_dir("order");
        std::fs::write(dir.join(MODULES_DIR).join("b.toml"), hidden_module("beta").replace("hidden = true", ""))
            .unwrap();
        std::fs::write(dir.join(MODULES_DIR).join("a.toml"), EXAMPLE.replace("id = \"example\"", "id = \"alpha\""))
            .unwrap();
        std::fs::write(dir.join(MODULES_DIR).join("notes.txt"), "not a manifest").unwrap();
        let registry = Registry::load(&[("example", EXAMPLE)], Some(&dir));
        let ids: Vec<_> = registry.modules(false).map(|m| m.manifest.id.as_str()).collect();
        assert_eq!(ids, ["example", "alpha", "beta"]);
        assert_eq!(registry.get("alpha", false).unwrap().origin.label(), "a.toml");
        assert_eq!(registry.broken(true).count(), 0, "a stray .txt is not a manifest");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_manifest_is_listed_with_its_reason_and_does_not_stop_the_rest() {
        let dir = temp_dir("broken");
        let no_install = EXAMPLE.replace("install = \"{bin} install {args}\"\n", "").replace("\"example\"", "\"lame\"");
        std::fs::write(dir.join(MODULES_DIR).join("lame.toml"), no_install).unwrap();
        std::fs::write(dir.join(MODULES_DIR).join("junk.toml"), "id = = =").unwrap();
        std::fs::write(dir.join(MODULES_DIR).join("good.toml"), EXAMPLE.replace("\"example\"", "\"good\"")).unwrap();
        let registry = Registry::load(&[], Some(&dir));
        assert_eq!(registry.modules(false).map(|m| m.manifest.id.as_str()).collect::<Vec<_>>(), ["good"]);
        let broken: Vec<_> = registry.broken(false).map(|b| (b.origin.label(), b.reason.clone())).collect();
        assert_eq!(broken.len(), 2);
        let lame = broken.iter().find(|(n, _)| n == "lame.toml").unwrap();
        assert!(lame.1.starts_with("no install command"), "{}", lame.1);
        assert!(registry.get("lame", true).is_none(), "an incomplete module is not offered");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hidden_modules_appear_only_with_the_switch() {
        let dir = temp_dir("hidden");
        std::fs::write(dir.join(MODULES_DIR).join("h.toml"), hidden_module("secretive")).unwrap();
        let registry = Registry::load(&[("example", EXAMPLE)], Some(&dir));
        assert_eq!(registry.modules(false).map(|m| m.manifest.id.as_str()).collect::<Vec<_>>(), ["example"]);
        assert_eq!(registry.modules(true).count(), 2);
        assert!(registry.get("secretive", false).is_none(), "absent from the install choice");
        assert!(registry.get("secretive", true).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_hidden_file_stays_hidden_too() {
        let dir = temp_dir("hidden-broken");
        let broken = hidden_module("ghost").replace("install = \"true\"", "");
        std::fs::write(dir.join(MODULES_DIR).join("ghost.toml"), broken).unwrap();
        std::fs::write(dir.join(MODULES_DIR).join("syntax.toml"), "hidden = true\nid = = =").unwrap();
        let registry = Registry::load(&[], Some(&dir));
        assert_eq!(registry.broken(false).count(), 0);
        assert_eq!(registry.broken(true).count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_duplicate_id_is_refused_and_the_builtin_wins() {
        let dir = temp_dir("dup");
        std::fs::write(dir.join(MODULES_DIR).join("again.toml"), EXAMPLE).unwrap();
        let registry = Registry::load(&[("example", EXAMPLE)], Some(&dir));
        assert_eq!(registry.modules(false).count(), 1);
        assert_eq!(registry.modules(false).next().unwrap().origin, Origin::Builtin("example".into()));
        let reason = registry.broken(false).next().unwrap().reason.clone();
        assert!(reason.contains("duplicate id"), "{reason}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_folder_means_only_builtin_modules() {
        let registry = Registry::load(&[("example", EXAMPLE)], Some(Path::new("Z:/definitely/not/here")));
        assert_eq!(registry.modules(false).count(), 1);
        assert_eq!(Registry::load(&[], None).modules(true).count(), 0);
    }

    #[test]
    fn a_builtin_manifest_that_does_not_parse_is_listed_as_broken() {
        let registry = Registry::load(&[("oops", "id = 1")], None);
        assert_eq!(registry.modules(true).count(), 0);
        assert_eq!(registry.broken(false).next().unwrap().origin, Origin::Builtin("oops".into()));
    }
}
