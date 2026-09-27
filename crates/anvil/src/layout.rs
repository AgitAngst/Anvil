//! Запрос, набранный не в той раскладке: «фь еу» вместо «am te» (и наоборот). Поиск пробует обе
//! строки и берёт лучшее совпадение.

/// Клавиши английской раскладки и русские буквы на тех же клавишах (ЙЦУКЕН, Windows).
const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";

/// Тот же запрос, набранный в другой раскладке: русские буквы — латинскими клавишами и наоборот.
/// Остальное (цифры, пробелы, знаки вне раскладки) не меняется.
pub fn swapped(query: &str) -> String {
    query
        .to_lowercase()
        .chars()
        .map(|c| {
            if let Some(i) = RU.chars().position(|r| r == c) {
                EN.chars().nth(i).unwrap_or(c)
            } else if let Some(i) = EN.chars().position(|e| e == c) {
                RU.chars().nth(i).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_keys_become_latin_and_back() {
        assert_eq!(EN.chars().count(), RU.chars().count());
        assert_eq!(swapped("фь еу"), "am te");
        assert_eq!(swapped("Фьиук"), "amber");
        assert_eq!(swapped("am te"), "фь еу");
        assert_eq!(swapped("repyb"), "кузни");
        assert_eq!(swapped("amber-server 18731"), "фьиук-ыукмук 18731");
    }
}
