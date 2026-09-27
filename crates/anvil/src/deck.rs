//! Пульт: что можно запустить — программы, службы и боты из проектов на Rust, проекты Godot и
//! Unity. Здесь только модель: какие предметы есть, как их зовут, в какой они группе и в каком
//! порядке. Что сейчас запущено и что запустит Enter — в `ui::deck`.

use std::path::{Path, PathBuf};

use anvil_ui::{Icon, Mark, family};

use crate::config::DeckSettings;
use crate::registry::{Bin, Kind};
use crate::worker::{self, Project};

/// Кем Anvil считает бинарник.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Программа с окном.
    Program,
    /// Долгоживущий консольный процесс: сервер, бот.
    Service,
    /// Консольный инструмент (`uniffi-bindgen`): на Пульт не попадает, виден только в Кузнице.
    Tool,
}

/// Группа Пульта. Порядок групп постоянный.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Programs,
    Services,
    Games,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Programs, Group::Services, Group::Games];
}

/// Предмет Пульта: проект и что в нём запускать.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Постоянный ключ: `<папка проекта в нижнем регистре>|<бинарник>` или `…|engine`.
    pub key: String,
    pub project: PathBuf,
    pub kind: Kind,
    /// Бинарник Rust; у проекта Godot и Unity — `None`.
    pub bin: Option<String>,
    pub group: Group,
    /// Имя на Пульте: «Amber», «Amber Admin», «amber-server», «IQube».
    pub name: String,
    /// Подпись моноширинным рядом с именем: бинарник, «Godot 4.7».
    pub caption: String,
    pub mark: Mark,
    /// Проект не под git.
    pub no_git: bool,
}

impl Item {
    /// Это сам Anvil — «это окно»: запускать его с Пульта незачем.
    pub fn is_self(&self) -> bool {
        self.bin.as_deref() == Some(env!("CARGO_PKG_NAME"))
    }
}

pub fn key(project: &Path, what: &str) -> String {
    format!("{}|{what}", project.to_string_lossy().to_lowercase())
}

/// Роль бинарника: что сказал сам пакет в `[package.metadata.anvil]`, иначе — оконный ли он, иначе —
/// по имени (`server`, `bot`, `daemon` — служба), иначе инструмент.
pub fn role(bin: &Bin) -> Role {
    match bin.hint.role.as_deref().map(str::to_lowercase).as_deref() {
        Some("program") => return Role::Program,
        Some("service") => return Role::Service,
        Some("tool") => return Role::Tool,
        _ => {}
    }
    if bin.gui {
        return Role::Program;
    }
    let name = bin.name.to_lowercase();
    if ["server", "bot", "daemon", "service"].iter().any(|w| name.contains(w)) { Role::Service } else { Role::Tool }
}

/// Значок по имени из `anvil.toml` (`icon = "target"`).
fn icon_named(name: &str) -> Option<Icon> {
    Some(match name.to_lowercase().as_str() {
        "cube" => Icon::Cube,
        "gamepad" => Icon::Gamepad,
        "target" => Icon::Target,
        "layers" => Icon::Layers,
        "tiles" => Icon::Tiles,
        "film" => Icon::Film,
        "image" => Icon::Image,
        "rocket" => Icon::Rocket,
        _ => return None,
    })
}

/// Предметы проектов. `icon_of` — значок проекта из настроек (для Godot и Unity).
pub fn items<'a>(
    projects: impl IntoIterator<Item = &'a Project>,
    icon_of: impl Fn(&Path) -> Option<String>,
) -> Vec<Item> {
    let mut out = Vec::new();
    for project in projects {
        let no_git = matches!(project.git, Ok(None));
        match project.kind {
            Kind::Rust => {
                let Some(meta) = project.meta() else { continue };
                let bins: Vec<(&Bin, Role)> = meta
                    .bins
                    .iter()
                    .filter(|b| !b.hint.hidden)
                    .map(|b| (b, role(b)))
                    .filter(|(_, r)| *r != Role::Tool)
                    .collect();
                let programs: Vec<&Bin> = bins.iter().filter(|(_, r)| *r == Role::Program).map(|(b, _)| *b).collect();
                let main = main_program(&project.path, &programs);
                for (bin, role) in bins {
                    let name = match (&bin.hint.name, role) {
                        (Some(name), _) => name.clone(),
                        (None, Role::Program) if main == Some(bin.name.as_str()) => worker::display_name(&project.path),
                        (None, Role::Program) => crate::installs::display_name(&bin.name),
                        (None, _) => bin.name.clone(),
                    };
                    let mark = family::mark_of(&bin.name).unwrap_or(family::neutral(match role {
                        Role::Service => Icon::Broadcast,
                        _ => Icon::Window,
                    }));
                    out.push(Item {
                        key: key(&project.path, &bin.name),
                        project: project.path.clone(),
                        kind: Kind::Rust,
                        bin: Some(bin.name.clone()),
                        group: if role == Role::Service { Group::Services } else { Group::Programs },
                        name,
                        // У службы имя и есть бинарник — подпись не повторяет его (в П3 там профиль и адрес).
                        caption: if role == Role::Service { String::new() } else { bin.name.clone() },
                        mark,
                        no_git,
                    });
                }
            }
            Kind::Godot | Kind::Unity => {
                // Пока фоновый поток не прочитал проект, версии и git не знаем: строка появится с ними.
                if project.engine.is_none() {
                    continue;
                }
                let engine = if project.kind == Kind::Godot { "Godot" } else { "Unity" };
                let version = project.engine.as_ref().and_then(|e| e.version.clone());
                let default = if project.kind == Kind::Godot { Icon::Cube } else { Icon::Gamepad };
                let icon = icon_of(&project.path).and_then(|n| icon_named(&n)).unwrap_or(default);
                out.push(Item {
                    key: key(&project.path, "engine"),
                    project: project.path.clone(),
                    kind: project.kind,
                    bin: None,
                    group: Group::Games,
                    name: worker::display_name(&project.path),
                    caption: version.map_or_else(|| engine.to_owned(), |v| format!("{engine} {v}")),
                    mark: family::neutral(icon),
                    no_git,
                });
            }
            Kind::Git => {}
        }
    }
    out
}

/// Главная программа проекта берёт имя проекта: единственная оконная, или названная как папка,
/// или `<папка>-desktop` / `-app` / `-gui` (`amber-desktop` → «Amber»).
fn main_program<'a>(project: &Path, programs: &[&'a Bin]) -> Option<&'a str> {
    if let [only] = programs {
        return Some(only.name.as_str());
    }
    let folder = project.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let names = [folder.clone(), format!("{folder}-desktop"), format!("{folder}-app"), format!("{folder}-gui")];
    programs.iter().find(|b| names.iter().any(|n| b.name.eq_ignore_ascii_case(n))).map(|b| b.name.as_str())
}

/// Порядок строк: по группам; внутри — закреплённые в порядке закрепления, потом недавно
/// запущенные, потом по имени. Сам Anvil — последний среди программ. Убранные не показываются.
pub fn order(items: &[Item], deck: &DeckSettings) -> Vec<String> {
    let mut list: Vec<&Item> = items.iter().filter(|i| !deck.removed.contains(&i.key)).collect();
    list.sort_by_key(|i| {
        let pin = deck.pinned.iter().position(|k| *k == i.key).unwrap_or(usize::MAX);
        let launched = deck.launched.get(&i.key).copied().unwrap_or(0);
        (i.group, i.is_self(), pin, std::cmp::Reverse(launched), i.name.to_lowercase())
    });
    list.into_iter().map(|i| i.key.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{BinHint, Meta};

    fn bin(name: &str, gui: bool) -> Bin {
        Bin { name: name.into(), package: name.into(), gui, hint: BinHint::default() }
    }

    fn rust(path: &str, bins: Vec<Bin>) -> Project {
        Project {
            path: PathBuf::from(path),
            kind: Kind::Rust,
            meta: Some(Ok(Meta { bins, ..Meta::default() })),
            git: Ok(None),
            notes: Vec::new(),
            engine: None,
        }
    }

    #[test]
    fn roles_come_from_hint_then_window_then_name() {
        assert_eq!(role(&bin("amber-desktop", true)), Role::Program);
        assert_eq!(role(&bin("amber-server", false)), Role::Service);
        assert_eq!(role(&bin("amber-bot", false)), Role::Service);
        assert_eq!(role(&bin("uniffi-bindgen", false)), Role::Tool);
        let mut hinted = bin("uniffi-bindgen", false);
        hinted.hint.role = Some("program".into());
        assert_eq!(role(&hinted), Role::Program);
    }

    #[test]
    fn amber_becomes_four_items_with_family_names() {
        let amber = rust(
            r"D:\dev\amber",
            vec![
                bin("amber-admin", true),
                bin("amber-bot", false),
                bin("amber-desktop", true),
                bin("amber-server", false),
                bin("uniffi-bindgen", false),
            ],
        );
        let items = items([&amber], |_| None);
        let names: Vec<(&str, Group)> = items.iter().map(|i| (i.name.as_str(), i.group)).collect();
        assert_eq!(
            names,
            [
                ("Amber Admin", Group::Programs),
                ("amber-bot", Group::Services),
                ("Amber", Group::Programs),
                ("amber-server", Group::Services),
            ]
        );
        assert_eq!(items[2].mark.icon, Icon::Chat);
        assert_eq!(items[2].key, r"d:\dev\amber|amber-desktop");
    }

    #[test]
    fn engines_get_neutral_marks_and_versions() {
        let mut godot = rust(r"D:\dev\IQube", Vec::new());
        godot.kind = Kind::Godot;
        godot.meta = None;
        godot.engine = Some(crate::engines::Info {
            engine: crate::engines::Engine::Godot,
            version: Some("4.7".into()),
            export: None,
            exported_at: None,
            open: false,
            editor: None,
        });
        let mut unity = godot.clone();
        unity.path = PathBuf::from(r"D:\dev\Claude_Sandbox");
        unity.kind = Kind::Unity;
        unity.engine.as_mut().unwrap().version = None;
        let items = items([&godot, &unity], |p| p.ends_with("Claude_Sandbox").then(|| "target".to_owned()));
        assert_eq!(items[0].caption, "Godot 4.7");
        assert_eq!(items[0].mark, family::neutral(Icon::Cube));
        assert_eq!(items[1].caption, "Unity");
        assert_eq!(items[1].mark.icon, Icon::Target);
        assert!(items.iter().all(|i| i.group == Group::Games && i.no_git));
    }

    #[test]
    fn pinned_first_then_recent_and_anvil_last() {
        let anvil = rust(r"D:\dev\Anvil", vec![bin("anvil", true)]);
        let tools = rust(r"D:\dev\Tools", vec![bin("alpha", true), bin("beta", true), bin("gamma", true)]);
        let items = items([&anvil, &tools], |_| None);
        let mut deck = DeckSettings::default();
        deck.pinned.push(key(Path::new(r"D:\dev\Tools"), "gamma"));
        deck.launched.insert(key(Path::new(r"D:\dev\Tools"), "beta"), 100);
        deck.launched.insert(key(Path::new(r"D:\dev\Anvil"), "anvil"), 999);
        deck.removed.push(key(Path::new(r"D:\dev\Tools"), "alpha"));
        let order = order(&items, &deck);
        let names: Vec<&str> = order.iter().map(|k| k.rsplit('|').next().unwrap()).collect();
        assert_eq!(names, ["gamma", "beta", "anvil"]);
    }
}
