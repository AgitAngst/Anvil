//! Скрытие секретов в выводе команд и в журнале работы.
//!
//! Журнал и окна задач не должны хранить токены: `token=…` в приглашениях, `"token": "…"` в JSON, заголовок
//! `Bearer …` и токены GitHub по виду. Значение заменяется многоточием `…`. Секреты, которые центр знает сам
//! (пароль входа, токен выпусков), прячет [`mask_known`] — по точному совпадению.

/// Чем заменяется значение.
pub const ELLIPSIS: &str = "…";

/// Ключи, после которых идёт секрет: `ключ=значение` и `"ключ": "значение"`.
const KEYS: &[&str] = &["token"];

/// Префиксы токенов GitHub: узнаются по виду, даже без ключа рядом.
const GITHUB_PREFIXES: &[&str] = &["github_pat_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_"];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_.~+/=".contains(c)
}

/// Строка без токенов: `token=…`, `"token": "…"`, `Bearer …`, `ghp_…`.
pub fn mask(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(hit) = find_secret(rest) {
        out.push_str(&rest[..hit.keep]);
        out.push_str(ELLIPSIS);
        rest = &rest[hit.keep + hit.skip..];
    }
    out.push_str(rest);
    out
}

/// Найденный секрет: оставить `keep` байт слева, затем пропустить `skip` байт (само значение).
struct Hit {
    keep: usize,
    skip: usize,
}

/// Самое раннее вхождение секрета в `rest`.
fn find_secret(rest: &str) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    let mut consider = |hit: Option<Hit>| {
        if let Some(hit) = hit
            && best.as_ref().is_none_or(|b| hit.keep < b.keep)
        {
            best = Some(hit);
        }
    };
    let lower = rest.to_ascii_lowercase();

    for key in KEYS {
        // `token=значение` и `token: значение` (в том числе внутри `access_token=`).
        let mut from = 0;
        while let Some(at) = lower[from..].find(key) {
            let start = from + at;
            let after = start + key.len();
            let tail = &rest[after..];
            // JSON: `token": "значение"` или `token" : "значение"`.
            let quoted = tail.strip_prefix('"').unwrap_or(tail);
            let colon_ws = quoted.trim_start();
            if let Some(value_part) = colon_ws.strip_prefix(':') {
                let ws = value_part.len() - value_part.trim_start().len();
                let value = value_part.trim_start();
                let lead = (tail.len() - quoted.len()) + (quoted.len() - colon_ws.len()) + 1 + ws;
                if let Some(inner) = value.strip_prefix('"') {
                    let end = inner.find('"').unwrap_or(inner.len());
                    // Кавычка остаётся, значение до закрывающей — нет.
                    consider(Some(Hit { keep: after + lead + 1, skip: end }));
                    break;
                }
            }
            if let Some(value) = tail.strip_prefix('=') {
                let end = value.find(|c: char| "&#\"' \t\r\n".contains(c)).unwrap_or(value.len());
                if end > 0 {
                    consider(Some(Hit { keep: after + 1, skip: end }));
                    break;
                }
            }
            from = after;
        }
    }

    // `Bearer значение`, регистр любой.
    if let Some(at) = lower.find("bearer ") {
        let after = at + "bearer ".len();
        let end = rest[after..].find(|c: char| !is_token_char(c)).unwrap_or(rest.len() - after);
        if end > 0 {
            consider(Some(Hit { keep: after, skip: end }));
        }
    }

    // Токены GitHub по виду.
    for prefix in GITHUB_PREFIXES {
        let mut from = 0;
        while let Some(at) = rest[from..].find(prefix) {
            let start = from + at;
            let after = start + prefix.len();
            let body =
                rest[after..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len() - after);
            if body >= 16 {
                consider(Some(Hit { keep: start, skip: prefix.len() + body }));
                break;
            }
            from = after;
        }
    }
    best
}

/// Заменить в тексте известные секреты (пароль входа, токен) многоточием. Пустые и слишком короткие (меньше
/// 4 символов) не трогаются: иначе пароль «1» съел бы половину журнала.
pub fn mask_known(text: &str, secrets: &[&str]) -> String {
    let mut out = text.to_owned();
    for secret in secrets {
        if secret.chars().count() >= 4 {
            out = out.replace(secret, ELLIPSIS);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_in_a_query_is_replaced_by_an_ellipsis() {
        assert_eq!(
            mask("  Аня: app://203.0.113.7:8730?pin=ab12&token=SECRETabc"),
            "  Аня: app://203.0.113.7:8730?pin=ab12&token=…"
        );
        assert_eq!(mask("token=abc"), "token=…");
        assert_eq!(mask("export ACCESS_TOKEN=abc123 && run"), "export ACCESS_TOKEN=… && run");
        assert_eq!(mask("a token=x#frag"), "a token=…#frag");
    }

    #[test]
    fn a_token_in_json_is_replaced_and_the_quotes_stay() {
        assert_eq!(
            mask(r#"{"name":"Бот","token": "SECRET","invitation":"app://h?pin=1&token=SECRET2"}"#),
            r#"{"name":"Бот","token": "…","invitation":"app://h?pin=1&token=…"}"#
        );
        assert_eq!(mask(r#"{"token":"abc"}"#), r#"{"token":"…"}"#);
        assert_eq!(mask(r#"{"token" : "abc", "x": 1}"#), r#"{"token" : "…", "x": 1}"#);
        assert_eq!(mask(r#"{"access_token":"abc"}"#), r#"{"access_token":"…"}"#);
    }

    #[test]
    fn a_bearer_header_is_hidden() {
        assert_eq!(mask("Authorization: Bearer abc.def-123"), "Authorization: Bearer …");
        assert_eq!(mask("authorization: bearer abc"), "authorization: bearer …");
        assert_eq!(mask("curl -H 'Authorization: Bearer abc' url"), "curl -H 'Authorization: Bearer …' url");
    }

    #[test]
    fn github_tokens_are_hidden_by_their_look() {
        assert_eq!(mask("using ghp_abcdefghijklmnopqrstuvwxyz0123456789 now"), "using … now");
        assert_eq!(mask("github_pat_11ABCDEFG0abcdefghijkl_mnopqrstuvwxyz"), "…");
        assert_eq!(mask("ghp_short is not a token"), "ghp_short is not a token");
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        for text in ["", "ничего секретного", "tokens are fine", "token", "token = 5", "the token: is empty", "a.b: c"]
        {
            let _ = mask(text);
        }
        assert_eq!(mask("ничего секретного"), "ничего секретного");
        assert_eq!(mask("tokens are fine"), "tokens are fine");
        assert_eq!(mask("token="), "token=");
    }

    #[test]
    fn several_secrets_in_one_line() {
        assert_eq!(mask("a token=1 b token=2 c"), "a token=… b token=… c");
        assert_eq!(mask("token=1\ntoken=2"), "token=…\ntoken=…");
    }

    #[test]
    fn known_secrets_are_removed_exactly() {
        assert_eq!(mask_known("password hunter22 here, hunter22 again", &["hunter22"]), "password … here, … again");
        assert_eq!(mask_known("a 1 b", &["1", ""]), "a 1 b", "too short to be safe to cut");
    }
}
