//! Строки баннера обновления на языке программы (`anvil_ui::lang`). Ключ — русская строка.

use eframe::egui;

pub fn tr(ctx: &egui::Context, ru: &'static str) -> &'static str {
    match anvil_ui::lang::language(ctx) {
        anvil_ui::Lang::Ru => ru,
        anvil_ui::Lang::En => DICTIONARY.iter().find(|(key, _)| *key == ru).map_or(ru, |(_, en)| en),
    }
}

const DICTIONARY: &[(&str, &str)] = &[
    ("Вышла", "Available:"),
    ("сейчас", "you have"),
    ("Обновить", "Update"),
    ("Что нового", "What's new"),
    ("Позже", "Later"),
    ("Пропустить", "Skip this version"),
    ("Страница выпуска", "Release page"),
    ("Скачиваю", "Downloading"),
    ("из", "of"),
    ("Обновление установлено", "Update installed"),
    ("Перезапустите программу, чтобы перейти на", "Restart the app to switch to"),
    ("Перезапустить", "Restart"),
    ("Обновление не удалось", "Update failed"),
    ("Повторить", "Retry"),
    ("Скрыть", "Hide"),
    ("Не удалось перезапуститься", "Could not restart"),
    ("Проверяю обновления…", "Checking for updates…"),
    ("Установлена последняя версия", "You have the latest version"),
    ("Доступна", "Available:"),
    ("Проверка не удалась", "Check failed"),
    ("Заметок к выпуску нет.", "The release has no notes."),
    ("Открыть на GitHub", "Open on GitHub"),
    (
        "Для этой системы архива нет — только страница выпуска.",
        "There is no archive for this system — only the release page.",
    ),
    ("МБ", "MB"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Каждая строка из `tr(ctx, "…")` в ui.rs есть в словаре.
    #[test]
    fn every_string_has_english() {
        let src = include_str!("ui.rs");
        let mut missing = Vec::new();
        for (i, _) in src.match_indices("tr(") {
            if src[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = &src[i + 3..];
            let Some(comma) = rest.find(',') else { continue };
            let Some(body) = rest[comma + 1..].trim_start().strip_prefix('"') else { continue };
            let key = &body[..body.find('"').unwrap()];
            if !DICTIONARY.iter().any(|(k, _)| *k == key) {
                missing.push(key.to_owned());
            }
        }
        assert!(missing.is_empty(), "нет перевода: {missing:?}");
    }
}
