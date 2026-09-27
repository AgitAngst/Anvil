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

/// Сегодня — время, раньше — дата: «00:28», «26.09».
pub fn when(timestamp: i64) -> String {
    let (today, then) = (local(now()), local(timestamp));
    let same_day = (today.0, today.1) == (then.0, then.1) && now() - timestamp < 86_400;
    if same_day { clock(timestamp) } else { date(timestamp) }
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

/// Сколько длилось: «12 с», «38 мин», «2 ч 14 мин» — для законченного запуска.
pub fn span(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 { format!("{secs} {}", t("с")) } else { uptime(secs) }
}

/// «Amber · test упал» / «Amber · test crashed»; пустое `who` — одно слово. «упал» у CI переводится
/// иначе («failed»), поэтому здесь своя пара.
pub fn crashed(who: &str) -> String {
    let word = if english() { "crashed" } else { "упал" };
    if who.is_empty() { word.to_owned() } else { format!("{who} {word}") }
}

/// «через 12 с после запуска» / «12 s after launch».
pub fn after_launch(secs: i64) -> String {
    if english() {
        format!("{} after launch", span(secs))
    } else {
        format!("через {} после запуска", span(secs))
    }
}

/// Первый пункт подтверждения остановки: что именно будет сделано.
pub fn stop_line(service: bool, name: &str, pid: u32) -> String {
    match (english(), service) {
        (false, true) => format!("Пошлю Ctrl+Break службе {name}, PID {pid}: она закроется сама"),
        (false, false) => format!("Закрою окно {name}, PID {pid}, как крестиком"),
        (true, true) => format!("I'll send Ctrl+Break to {name}, PID {pid}; it should shut down on its own"),
        (true, false) => format!("I'll close {name}, PID {pid}, like the close button"),
    }
}

/// «Недавно» о работающем запуске: «запущен из сборки 2353af9», «запущен · установлена 0.4.0».
/// `source` — как в истории: русское слово и значение.
pub fn started(source: &str) -> String {
    let (word, rest) = source.split_once(' ').unwrap_or((source, ""));
    match (english(), word) {
        (false, "сборка") => format!("запущен из сборки {rest}").trim_end().to_owned(),
        (true, "сборка") => format!("started from build {rest}").trim_end().to_owned(),
        (false, _) => format!("запущен · {} {rest}", source_word(word)).trim_end().to_owned(),
        (true, _) => format!("started · {} {rest}", source_word(word)).trim_end().to_owned(),
    }
}

/// Слово источника из истории запусков. Оно хранится по-русски («сборка 2353af9»), чтобы история
/// не зависела от языка, а на экране переводится.
pub fn source_word(word: &str) -> String {
    match word {
        "сборка" => t("сборка").to_owned(),
        "установлена" => t("установлена").to_owned(),
        "экспорт" => t("экспорт").to_owned(),
        "исходники" => t("исходники").to_owned(),
        other => other.to_owned(),
    }
}

/// «есть v0.1.0 на GitHub» / «v0.1.0 on GitHub»: у языков разный порядок слов.
pub fn on_github(version: &str) -> String {
    if english() { format!("{version} on GitHub") } else { format!("есть {version} на GitHub") }
}

/// «Из проекта Amber» / «From the Amber project».
pub fn from_project(name: &str) -> String {
    if english() { format!("From the {name} project") } else { format!("Из проекта {name}") }
}

/// «в 00:25» / «at 00:25».
pub fn at(timestamp: i64) -> String {
    if english() { format!("at {}", clock(timestamp)) } else { format!("в {}", clock(timestamp)) }
}

/// «26.09 в 21:01» / «26.09 at 21:01».
pub fn date_at(timestamp: i64) -> String {
    format!("{} {}", date(timestamp), at(timestamp))
}

/// «с 00:26» / «since 00:26».
pub fn since(timestamp: i64) -> String {
    if english() { format!("since {}", clock(timestamp)) } else { format!("с {}", clock(timestamp)) }
}

/// «упал в 00:28 через 12 с» / «crashed at 00:28 after 12 s»; не сегодня — с датой:
/// «упал 20.09 в 00:28 через 12 с».
pub fn crashed_at(timestamp: i64, took: i64) -> String {
    let today = when(timestamp) == clock(timestamp);
    match (english(), today) {
        (true, true) => format!("crashed at {} after {}", clock(timestamp), span(took)),
        (true, false) => format!("crashed on {} after {}", date_at(timestamp), span(took)),
        (false, true) => format!("упал в {} через {}", clock(timestamp), span(took)),
        (false, false) => format!("упал {} через {}", date_at(timestamp), span(took)),
    }
}

/// «упал в 00:28» / «crashed at 00:28»; не сегодня — «упал 20.09 в 00:28».
pub fn crashed_short(timestamp: i64) -> String {
    let today = when(timestamp) == clock(timestamp);
    match (english(), today) {
        (true, true) => format!("crashed at {}", clock(timestamp)),
        (true, false) => format!("crashed on {}", date_at(timestamp)),
        (false, true) => format!("упал в {}", clock(timestamp)),
        (false, false) => format!("упал {}", date_at(timestamp)),
    }
}

/// «amber-desktop из кода уже запущен (test)» / «amber-desktop from code is already running (test)».
pub fn already_running(bin: &str, profile: &str) -> String {
    let tail = if profile.is_empty() { String::new() } else { format!(" ({profile})") };
    if english() {
        format!("{bin} from code is already running{tail}")
    } else {
        format!("{bin} из кода уже запущен{tail}")
    }
}

/// Окно занятого exe: что будет при «Закрыть программу и собрать».
pub fn close_then_force(secs: u64, launch_after: bool) -> String {
    match (english(), launch_after) {
        (false, true) => format!(
            "Программа получит команду закрыться, как от крестика; через {secs} с — принудительно. Потом сборка и запуск новой."
        ),
        (false, false) => format!(
            "Программа получит команду закрыться, как от крестика; через {secs} с — принудительно. Несохранённое в ней может пропасть."
        ),
        (true, true) => format!(
            "The program is asked to close, as if its close button was pressed; after {secs} s it is stopped forcibly. Then the new build is made and started."
        ),
        (true, false) => format!(
            "The program is asked to close, as if its close button was pressed; after {secs} s it is stopped forcibly. Unsaved work in it may be lost."
        ),
    }
}

/// «Если не закроется за 5 с — спрошу…» / «If it doesn't close in 5 s…».
pub fn ask_before_force(secs: u64) -> String {
    if english() {
        format!("If it doesn't close in {secs} s, I'll ask before forcing it")
    } else {
        format!("Если не закроется за {secs} с — спрошу, остановить ли принудительно")
    }
}

/// «не закрылась за 5 с» — запись в журнале задачи.
pub fn not_closed_in(secs: u64) -> String {
    if english() { format!("didn't close in {secs} s") } else { format!("не закрылась за {secs} с") }
}

/// «не закрылся за 5 с. Остановить принудительно?».
pub fn force_question(secs: u64) -> String {
    if english() {
        format!("didn't close in {secs} s. Force stop?")
    } else {
        format!("не закрылся за {secs} с. Остановить принудительно?")
    }
}

/// «ещё 22 — в Кузнице» / «22 more in the Forge».
pub fn more_in_forge(n: usize) -> String {
    if english() { format!("{n} more in the Forge") } else { format!("ещё {n} — в Кузнице") }
}

/// Время работы в чипе строки состояния: «2:14», «0:08».
pub fn uptime_short(secs: i64) -> String {
    let secs = secs.max(0);
    format!("{}:{:02}", secs / 3600, secs % 3600 / 60)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Для тестов: говорить по-русски.
    pub fn russian() {
        ENGLISH.store(false, Ordering::Relaxed);
    }

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
