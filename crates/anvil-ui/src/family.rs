//! Знаки программ семьи: цвет — чья программа, значок — что она делает.
//!
//! Одна таблица на всех: по ней Anvil рисует знаки на Пульте, и её же программа берёт для
//! своего окна и exe. Все бинарники Amber — янтарные, различает их значок.

use crate::icons::Icon;
use crate::theme::Accent;

/// Знак предмета: акцент программы (`None` — нейтральный знак) и значок.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    pub accent: Option<Accent>,
    pub icon: Icon,
}

/// Бинарники семьи и их знаки.
pub const MARKS: &[(&str, Mark)] = &[
    ("anvil", Mark { accent: Some(Accent::VIOLET), icon: Icon::Hive }),
    ("amber-desktop", Mark { accent: Some(Accent::AMBER), icon: Icon::Chat }),
    ("amber-admin", Mark { accent: Some(Accent::AMBER), icon: Icon::Server }),
    ("amber-server", Mark { accent: Some(Accent::AMBER), icon: Icon::Broadcast }),
    ("amber-bot", Mark { accent: Some(Accent::AMBER), icon: Icon::Bot }),
    ("tetrachrome", Mark { accent: Some(Accent::TEAL), icon: Icon::Tiles }),
    ("ffmincer", Mark { accent: Some(Accent::ROSE), icon: Icon::Film }),
];

/// Знак бинарника семьи по имени (без `.exe`, без учёта регистра).
pub fn mark_of(binary: &str) -> Option<Mark> {
    let name = binary.strip_suffix(".exe").unwrap_or(binary);
    MARKS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, m)| *m)
}

/// Нейтральный знак — для всего, что не программа на `anvil-ui`: проекты Godot и Unity.
pub const fn neutral(icon: Icon) -> Mark {
    Mark { accent: None, icon }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_binaries_have_their_marks() {
        assert_eq!(mark_of("amber-desktop.exe").map(|m| m.icon), Some(Icon::Chat));
        assert_eq!(mark_of("Anvil").map(|m| m.accent), Some(Some(Accent::VIOLET)));
        assert!(mark_of("uniffi-bindgen").is_none());
    }
}
