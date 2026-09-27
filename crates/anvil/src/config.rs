//! Настройки Anvil: `anvil.toml` рядом с exe (портативный режим, если файл там есть)
//! или `%APPDATA%\Anvil\anvil.toml`.

use std::collections::BTreeMap;
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
    /// Сколько задач сборки cargo запускает разом (`-j`); 0 — сколько ядер.
    pub build_jobs: u32,
    /// Уведомление Windows, когда долгая задача кончилась, а окно не в фокусе.
    pub notify: bool,
    /// Уведомление Windows, когда запущенная из Anvil программа упала, а окно не впереди.
    pub notify_crash: bool,
    /// Какие проекты показывать: `rust`; на будущее — `godot`, `unity`, `git` (просто репозиторий).
    pub kinds: Vec<String>,
    /// Настройки проектов; ключ — путь к проекту.
    pub projects: BTreeMap<String, ProjectSettings>,
    /// Пульт: закреплённое, убранное, когда что запускали.
    pub deck: DeckSettings,
    /// Редактор Godot (`Godot_v4.7.1-stable_win64.exe`). Не задан — ищется в `PATH`.
    pub godot: Option<PathBuf>,
    /// Быстрый запуск: сочетание, закрывать ли после запуска, искать ли в другой раскладке.
    pub quick: QuickSettings,
    /// Окно и трей.
    pub window: WindowSettings,
    /// Версия файла настроек: по ней старые файлы один раз дополняются новым (см. [`migrate`]).
    /// В файлах 0.2 поля нет — это версия 0.
    #[serde(default)]
    pub version: u32,
    pub common: CommonSettings,
}

/// Быстрый запуск (§7.5): глобальное сочетание и как он себя ведёт.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickSettings {
    /// «Ctrl+Alt+Space»; пусто — выключено.
    pub hotkey: String,
    /// Закрывать быстрый запуск после запуска.
    pub close_after: bool,
    /// Искать и в другой раскладке: «фь еу» = «am te».
    pub layout: bool,
}

impl Default for QuickSettings {
    fn default() -> Self {
        Self { hotkey: "Ctrl+Alt+Space".to_owned(), close_after: true, layout: true }
    }
}

/// Окно и трей.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    /// Крестик прячет Anvil в трей (выход — из меню трея).
    pub close_to_tray: bool,
    /// Подсказку «Anvil в трее» уже показывали.
    pub tray_hint_shown: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self { close_to_tray: true, tray_hint_shown: false }
    }
}

/// Что Пульт помнит о предметах. Ключ предмета — `<папка проекта в нижнем регистре>|<бинарник>`
/// или `…|engine` у проекта Godot и Unity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeckSettings {
    /// Закреплённые — сверху своей группы, в этом порядке.
    pub pinned: Vec<String>,
    /// Убранные с Пульта (вернуть — в настройках).
    pub removed: Vec<String>,
    /// Когда предмет запускали с Пульта, секунды Unix: по этому сортируется группа.
    pub launched: BTreeMap<String, i64>,
    /// Выбранный профиль предмета (имя); нет — «обычный».
    pub profile: BTreeMap<String, String>,
}

/// Что Anvil помнит о проекте.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    /// Собирать и запускать release, а не debug.
    pub release: bool,
    /// Что запускает главная кнопка: имя бинарника или пресета.
    pub run: Option<String>,
    pub presets: Vec<Preset>,
    /// Откуда брать выпуски (`owner/name`), если не из репозитория проекта. Не задано — как в
    /// workflow выпуска проекта (`repository:`), иначе — сам репозиторий.
    pub releases: Option<String>,
    /// Значок проекта Godot или Unity на Пульте: `cube`, `gamepad`, `target`, `layers`.
    pub icon: Option<String>,
}

/// Профиль запуска (в 0.2 — «пресет»): какой бинарник, откуда и с какими аргументами.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    pub name: String,
    pub bin: String,
    /// Аргументы строкой, как в терминале: `--profile test`. `%VAR%` раскрывается.
    pub args: String,
    /// Откуда запускать: сборка из кода (так было в 0.2) или установленная копия.
    pub source: Source,
    /// Рабочая папка; не задана — корень проекта (из кода) или папка установки.
    pub cwd: Option<PathBuf>,
    /// Переменные окружения сверх унаследованных.
    pub env: BTreeMap<String, String>,
    /// Когда считать запущенным: `port:18731`, `window`, `5s`; пусто — сразу.
    pub ready: String,
}

/// Откуда профиль берёт exe.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// `target\release`: перед запуском `cargo build --release`.
    #[default]
    Code,
    /// `%LOCALAPPDATA%\Programs\<бинарник>\current`.
    Installed,
}

impl Preset {
    /// Порт из условия готовности `port:18731` — для чипа «amber-server :18731».
    pub fn port(&self) -> Option<u16> {
        self.ready.strip_prefix("port:")?.trim().parse().ok()
    }
}

impl Config {
    fn key(path: &Path) -> String {
        path.to_string_lossy().to_lowercase()
    }

    pub fn project(&self, path: &Path) -> ProjectSettings {
        self.projects.get(&Self::key(path)).cloned().unwrap_or_default()
    }

    pub fn project_mut(&mut self, path: &Path) -> &mut ProjectSettings {
        self.projects.entry(Self::key(path)).or_default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            hidden: Vec::new(),
            fetch_minutes: 15,
            selected: None,
            build_jobs: 0,
            notify: true,
            notify_crash: true,
            kinds: vec!["rust".to_owned(), "godot".to_owned(), "unity".to_owned()],
            projects: BTreeMap::new(),
            deck: DeckSettings::default(),
            godot: None,
            quick: QuickSettings::default(),
            window: WindowSettings::default(),
            version: VERSION,
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

/// Текущая версия файла настроек.
pub const VERSION: u32 = 1;

/// Прочитать настройки. Файла нет — первый запуск: корень угадывается по месту exe.
/// Файл испорчен — настройки по умолчанию и текст ошибки, файл не трогается до первого сохранения.
/// Второе в ответе — старый файл дополнен и его стоит сохранить.
pub fn load(path: &Path) -> (Config, Option<String>, bool) {
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<Config>(text.trim_start_matches('\u{feff}')) {
            Ok(mut config) => {
                let migrated = migrate(&mut config);
                (config, None, migrated)
            }
            Err(e) => (Config::default(), Some(e.to_string()), false),
        },
        Err(_) => (Config { roots: guess_root().into_iter().collect(), ..Config::default() }, None, false),
    }
}

/// Дополнить файл старой версии. `true` — что-то поменялось.
///
/// 0 → 1 (Anvil 0.3, Пульт): проекты Godot и Unity входят в библиотеку — их виды включаются, если
/// выключены (в 0.2 по умолчанию был только `rust`).
fn migrate(config: &mut Config) -> bool {
    if config.version >= VERSION {
        return false;
    }
    for kind in ["godot", "unity"] {
        if !config.kinds.iter().any(|k| k.eq_ignore_ascii_case(kind)) {
            config.kinds.push(kind.to_owned());
        }
    }
    config.version = VERSION;
    true
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
        let mut config = Config {
            roots: vec![PathBuf::from(r"D:\dev_personal")],
            hidden: vec![PathBuf::from(r"D:\dev_personal\old")],
            fetch_minutes: 5,
            selected: Some(PathBuf::from(r"D:\dev_personal\amber")),
            build_jobs: 4,
            ..Config::default()
        };
        let amber = config.project_mut(Path::new(r"D:\dev_personal\amber"));
        amber.release = true;
        amber.presets.push(Preset {
            name: "Тест".into(),
            bin: "amber-desktop".into(),
            args: "--profile t".into(),
            ..Preset::default()
        });
        assert_eq!(config.project(Path::new(r"D:\DEV_PERSONAL\Amber")).presets.len(), 1);
        let text = toml::to_string_pretty(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    #[test]
    fn old_files_get_godot_and_unity_once() {
        let mut old: Config = toml::from_str(
            "roots = ['C:/src']
kinds = ['rust']",
        )
        .unwrap();
        assert_eq!(old.version, 0);
        assert!(migrate(&mut old));
        assert_eq!(old.kinds, ["rust", "godot", "unity"]);
        assert_eq!(old.version, VERSION);
        // Второй раз ничего не меняется, и пользователь может снова выключить виды.
        old.kinds.truncate(1);
        assert!(!migrate(&mut old));
        assert_eq!(old.kinds, ["rust"]);
        let fresh: Config = toml::from_str(&toml::to_string_pretty(&Config::default()).unwrap()).unwrap();
        assert_eq!(fresh.version, VERSION);
    }

    #[test]
    fn old_presets_become_profiles_from_code() {
        let text = "[projects.'d:\\x']\npresets = [{ name = 'test', bin = 'amber-server', args = '--addr 1', ready = 'port:18731' }]";
        let config: Config = toml::from_str(text).unwrap();
        let preset = &config.project(Path::new(r"D:\x")).presets[0];
        assert_eq!(preset.source, Source::Code);
        assert_eq!(preset.port(), Some(18731));
    }

    #[test]
    fn missing_fields_take_defaults() {
        let config: Config = toml::from_str("roots = ['C:/src']").unwrap();
        assert_eq!(config.fetch_minutes, 15);
        assert!(config.common.check_updates);
    }
}
