//! Строки для оболочки машины: всё, что попадает в команду из формы или манифеста, проходит здесь.

/// Строка для оболочки: в одинарных кавычках, кавычки внутри экранированы. Подходит для любого значения.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Как [`quote`], но безопасное слово (буквы, цифры и `_@%+=:,./-`) остаётся как есть — команда читается
/// человеком, который подтверждает её дословно.
pub fn word(s: &str) -> String {
    let plain = !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));
    if plain { s.to_owned() } else { quote(s) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_quotes_spaces_and_dollars() {
        assert_eq!(quote("Ане"), "'Ане'");
        assert_eq!(quote("it's $HOME"), r"'it'\''s $HOME'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn safe_words_stay_readable_and_the_rest_is_quoted() {
        assert_eq!(word("8740"), "8740");
        assert_eq!(word("/opt/example/server-1.2"), "/opt/example/server-1.2");
        assert_eq!(word("a b"), "'a b'");
        assert_eq!(word("x;rm -rf /"), "'x;rm -rf /'");
        assert_eq!(word("$(id)"), "'$(id)'");
        assert_eq!(word(""), "''");
    }
}
