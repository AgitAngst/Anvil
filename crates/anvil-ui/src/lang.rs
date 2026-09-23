//! Язык собственных строк набора: кнопки диалогов, окна «О программе» и «Настройки».
//!
//! Строки программы переводит сама программа. Набор только знает, какой язык выбран
//! ([`set_language`]), и переводит своё: ключ — русская строка, как в Amber.

use eframe::egui;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Lang {
    #[default]
    En,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Ru];

    /// Название языка на нём самом — так его узнают, даже не понимая текущего.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
        }
    }
}

fn lang_id() -> egui::Id {
    egui::Id::new("anvil-ui-lang")
}

/// Сообщить набору язык программы.
pub fn set_language(ctx: &egui::Context, lang: Lang) {
    ctx.data_mut(|d| d.insert_temp(lang_id(), lang));
}

/// Текущий язык; пока программа не сказала — язык системы.
pub fn language(ctx: &egui::Context) -> Lang {
    ctx.data(|d| d.get_temp(lang_id())).unwrap_or_else(system_language)
}

/// Язык интерфейса системы: русский, если система по-русски, иначе английский.
pub fn system_language() -> Lang {
    #[cfg(windows)]
    {
        // Младшие 10 бит LANGID — основной язык; 0x19 — русский.
        let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        if id & 0x3FF == 0x19 { Lang::Ru } else { Lang::En }
    }
    #[cfg(not(windows))]
    {
        let vars = ["LC_ALL", "LC_MESSAGES", "LANG"];
        let russian = vars.iter().filter_map(|v| std::env::var(v).ok()).find(|v| !v.is_empty());
        if russian.is_some_and(|v| v.starts_with("ru")) { Lang::Ru } else { Lang::En }
    }
}

/// Перевести строку набора на текущий язык.
pub fn tr(ctx: &egui::Context, ru: &'static str) -> &'static str {
    match language(ctx) {
        Lang::Ru => ru,
        Lang::En => english(ru).unwrap_or(ru),
    }
}

fn english(ru: &str) -> Option<&'static str> {
    DICTIONARY.iter().find(|(key, _)| *key == ru).map(|(_, en)| *en)
}

const DICTIONARY: &[(&str, &str)] = &[
    ("Отмена", "Cancel"),
    ("Закрыть", "Close"),
    ("Готово", "Done"),
    ("О программе", "About"),
    ("Настройки", "Settings"),
    ("Версия", "Version"),
    ("Исходный код", "Source code"),
    ("Проверить обновления", "Check for updates"),
    ("Оформление", "Appearance"),
    ("Тема", "Theme"),
    ("Система", "System"),
    ("Светлая", "Light"),
    ("Тёмная", "Dark"),
    ("Язык", "Language"),
    ("Обновления", "Updates"),
    ("Проверять при запуске", "Check on startup"),
    ("Предлагать пред-выпуски", "Offer pre-releases"),
    (
        "Программа сама узнаёт о новых версиях на GitHub. Больше ничего наружу не отправляет.",
        "The app checks GitHub for new versions. Nothing else is sent anywhere.",
    ),
    ("Часть семьи Anvil", "Part of the Anvil family"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Каждая строка, которую набор переводит, есть в словаре.
    #[test]
    fn every_kit_string_has_english() {
        let src = [include_str!("chrome.rs"), include_str!("widgets.rs")].concat();
        let mut missing = Vec::new();
        for (i, _) in src.match_indices("tr(") {
            // Именно вызов tr, а не хвост attr( или str(.
            if src[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = &src[i + 3..];
            let Some(start) = rest.find('"') else { continue };
            // Только прямые литералы: tr(ctx, "…").
            if rest[..start].contains(')') || rest[..start].len() > 24 {
                continue;
            }
            let body = &rest[start + 1..];
            let end = body.find('"').unwrap();
            let key = &body[..end];
            if english(key).is_none() {
                missing.push(key.to_owned());
            }
        }
        assert!(missing.is_empty(), "нет перевода: {missing:?}");
    }

    #[test]
    fn untranslated_falls_back_to_russian() {
        let ctx = egui::Context::default();
        set_language(&ctx, Lang::En);
        assert_eq!(tr(&ctx, "Отмена"), "Cancel");
        assert_eq!(tr(&ctx, "нет такой строки"), "нет такой строки");
        set_language(&ctx, Lang::Ru);
        assert_eq!(tr(&ctx, "Отмена"), "Отмена");
    }
}
