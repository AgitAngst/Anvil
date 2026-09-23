//! Перевод строк Anvil. Ключ — русская строка, перевод — в `lang/en.tsv`.
//!
//! Язык общий с набором (`anvil_ui::lang`): [`set`] меняет оба.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anvil_ui::Lang;

static ENGLISH: AtomicBool = AtomicBool::new(false);

static EN: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| parse(include_str!("../lang/en.tsv")));

fn parse(text: &'static str) -> HashMap<&'static str, &'static str> {
    text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).filter_map(|l| l.split_once('\t')).collect()
}

/// Сменить язык Anvil и набора.
pub fn set(ctx: &eframe::egui::Context, lang: Lang) {
    ENGLISH.store(lang == Lang::En, Ordering::Relaxed);
    anvil_ui::lang::set_language(ctx, lang);
}

fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// Перевести строку. Нет перевода — остаётся русская (тест не даёт этому случиться).
pub fn t(ru: &'static str) -> &'static str {
    if english() { EN.get(ru).copied().unwrap_or(ru) } else { ru }
}

/// Число со словом в нужной форме: «1 файл», «3 файла», «5 файлов» / «1 file», «3 files».
pub fn count(n: usize, ru: [&str; 3], en: [&str; 2]) -> String {
    let word = if english() {
        if n == 1 { en[0] } else { en[1] }
    } else {
        let (n10, n100) = (n % 10, n % 100);
        if n10 == 1 && n100 != 11 {
            ru[0]
        } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
            ru[1]
        } else {
            ru[2]
        }
    };
    format!("{n} {word}")
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// «только что», «5 мин назад», «3 ч назад», «вчера», «4 дня назад», «2 мес. назад».
pub fn ago(timestamp: i64) -> String {
    let secs = (now() - timestamp).max(0);
    let (min, hour, day) = (60, 3600, 86_400);
    if secs < min {
        t("только что").to_owned()
    } else if secs < hour {
        format!("{} {}", secs / min, t("мин назад"))
    } else if secs < day {
        format!("{} {}", secs / hour, t("ч назад"))
    } else if secs < 2 * day {
        t("вчера").to_owned()
    } else if secs < 30 * day {
        format!("{} {}", count((secs / day) as usize, ["день", "дня", "дней"], ["day", "days"]), t("назад"))
    } else if secs < 365 * day {
        format!("{} {}", secs / (30 * day), t("мес. назад"))
    } else {
        format!("{} {}", secs / (365 * day), t("г. назад"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Все строки из `t("…")` в исходниках есть в словаре, и в словаре нет лишних.
    #[test]
    fn dictionary_matches_sources() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut used = std::collections::BTreeSet::new();
        let mut stack = vec![dir];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    // Комментарии не в счёт: там `t("…")` бывает примером.
                    let src: String =
                        text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
                    for (i, _) in src.match_indices("t(\"") {
                        if src[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        let body = &src[i + 3..];
                        used.insert(body[..body.find('"').unwrap()].to_owned());
                    }
                }
            }
        }
        let missing: Vec<_> = used.iter().filter(|k| !EN.contains_key(k.as_str())).collect();
        assert!(missing.is_empty(), "нет перевода в lang/en.tsv: {missing:#?}");
        let stale: Vec<_> = EN.keys().filter(|k| !used.contains(**k)).collect();
        assert!(stale.is_empty(), "лишние строки в lang/en.tsv: {stale:#?}");
    }

    #[test]
    fn russian_plural_forms() {
        ENGLISH.store(false, Ordering::Relaxed);
        let f = |n| count(n, ["файл", "файла", "файлов"], ["file", "files"]);
        assert_eq!(f(1), "1 файл");
        assert_eq!(f(3), "3 файла");
        assert_eq!(f(5), "5 файлов");
        assert_eq!(f(11), "11 файлов");
        assert_eq!(f(22), "22 файла");
    }
}
