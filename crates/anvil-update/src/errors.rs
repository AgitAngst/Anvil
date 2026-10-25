//! Тексты ошибок проверки обновления. Внутри — по-английски (в журнал и для программ, которые показывают
//! строку как есть); `describe` переводит узнаваемые на язык окна.

/// Ошибка «лимит запросов GitHub». `minutes` — через сколько минут лимит сбросится (не меньше 1).
pub fn rate_limit(status: u16, minutes: u64) -> String {
    format!("GitHub: HTTP {status} (rate limit), retry in {} min", minutes.max(1))
}

/// Ошибка GitHub без подробностей.
pub fn http_status(status: u16) -> String {
    format!("GitHub: HTTP {status}")
}

/// Разобрать `rate_limit`: код ответа и минуты.
fn parse_rate_limit(error: &str) -> Option<(u16, u64)> {
    let rest = error.strip_prefix("GitHub: HTTP ")?;
    let (status, rest) = rest.split_once(" (rate limit), retry in ")?;
    let minutes = rest.strip_suffix(" min")?;
    Some((status.parse().ok()?, minutes.parse().ok()?))
}

/// Понятный текст ошибки на языке окна. Незнакомая ошибка возвращается как есть.
pub fn describe(error: &str, ru: bool) -> String {
    if let Some((status, minutes)) = parse_rate_limit(error) {
        return if ru {
            format!("Лимит запросов GitHub (HTTP {status}): повторите примерно через {minutes} мин")
        } else {
            format!("GitHub rate limit (HTTP {status}): try again in about {minutes} min")
        };
    }
    error.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_round_trip_and_translation() {
        let e = rate_limit(403, 23);
        assert_eq!(e, "GitHub: HTTP 403 (rate limit), retry in 23 min");
        assert_eq!(parse_rate_limit(&e), Some((403, 23)));
        assert_eq!(describe(&e, false), "GitHub rate limit (HTTP 403): try again in about 23 min");
        assert_eq!(describe(&e, true), "Лимит запросов GitHub (HTTP 403): повторите примерно через 23 мин");
    }

    #[test]
    fn at_least_one_minute_and_unknown_errors_pass_through() {
        assert_eq!(parse_rate_limit(&rate_limit(429, 0)), Some((429, 1)));
        assert_eq!(describe("GitHub: HTTP 404", true), "GitHub: HTTP 404");
        assert_eq!(describe("timed out", false), "timed out");
        assert_eq!(parse_rate_limit("GitHub: HTTP x (rate limit), retry in y min"), None);
    }
}
