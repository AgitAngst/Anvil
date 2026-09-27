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

/// Местное время момента `timestamp` (секунды Unix): день, месяц, часы, минуты.
#[cfg(windows)]
fn local(timestamp: i64) -> (u16, u16, u16, u16) {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    let ticks = ((timestamp.max(0) as u64) + 11_644_473_600) * 10_000_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    // SAFETY: обе функции пишут только в переданные структуры; часовой пояс — текущий (null).
    unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut here: SYSTEMTIME = std::mem::zeroed();
        if FileTimeToSystemTime(&ft, &mut utc) == 0
            || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut here) == 0
        {
            return (0, 0, 0, 0);
        }
        (here.wDay, here.wMonth, here.wHour, here.wMinute)
    }
}

/// Вне Windows — по UTC: Anvil там только собирается в CI.
#[cfg(not(windows))]
fn local(timestamp: i64) -> (u16, u16, u16, u16) {
    let secs = timestamp.max(0);
    let days = secs / 86_400;
    let (h, m) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    // Гражданская дата из числа дней (алгоритм Говарда Хиннанта).
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (d as u16, month as u16, h as u16, m as u16)
}

/// «00:28» — местное время, 24 часа.
pub fn clock(timestamp: i64) -> String {
    let (_, _, h, m) = local(timestamp);
    format!("{h:02}:{m:02}")
}

/// «27.09» — местная дата, одинаково на обоих языках.
pub fn date(timestamp: i64) -> String {
    let (d, mo, _, _) = local(timestamp);
    format!("{d:02}.{mo:02}")
}

/// Сколько работает, до минуты: «меньше минуты», «8 мин», «2 ч 14 мин».
pub fn uptime(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 {
        t("меньше минуты").to_owned()
    } else if secs < 3600 {
        format!("{} {}", secs / 60, t("мин"))
    } else {
        format!("{} {} {} {}", secs / 3600, t("ч"), secs % 3600 / 60, t("мин"))
    }
}

/// «есть v0.1.0 на GitHub» / «v0.1.0 on GitHub»: у языков разный порядок слов.
pub fn on_github(version: &str) -> String {
    if english() { format!("{version} on GitHub") } else { format!("есть {version} на GitHub") }
}

/// Время работы в чипе строки состояния: «2:14», «0:08».
pub fn uptime_short(secs: i64) -> String {
    let secs = secs.max(0);
    format!("{}:{:02}", secs / 3600, secs % 3600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_to_the_minute() {
        ENGLISH.store(false, Ordering::Relaxed);
        assert_eq!(uptime(12), "меньше минуты");
        assert_eq!(on_github("v0.1.0"), "есть v0.1.0 на GitHub");
        assert_eq!(uptime(8 * 60 + 30), "8 мин");
        assert_eq!(uptime(2 * 3600 + 14 * 60 + 59), "2 ч 14 мин");
        assert_eq!(uptime_short(2 * 3600 + 14 * 60), "2:14");
        assert_eq!(uptime_short(8 * 60), "0:08");
        // Местное время — двузначные часы и минуты.
        assert_eq!(clock(now()).len(), 5);
        assert_eq!(date(now()).len(), 5);
    }

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
                    // Тесты не в счёт (здесь же ищется сам `t(`), комментарии тоже: там `t("…")` — пример.
                    let text = text.split("#[cfg(test)]").next().unwrap_or_default();
                    let src: String =
                        text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
                    // `t(` и сразу строка — даже если rustfmt перенёс её на следующую строку.
                    for (i, _) in src.match_indices("t(") {
                        if src[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        let Some(body) = src[i + 2..].trim_start().strip_prefix('"') else { continue };
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
