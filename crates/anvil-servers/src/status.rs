//! Состояние модуля из вывода его команды `status --json`.
//!
//! Известные поля — `name`, `version`, `running`, `address`, `fingerprint`; всё остальное (счётчики и прочее)
//! отдаётся как «ещё»: центр показывает его строками. Значения, похожие на секреты (ключ со словом `token`,
//! `password`, `secret`), в «ещё» не попадают.

use serde_json::Value;

use crate::redact;

/// Что сказал сервер о себе.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub name: Option<String>,
    pub version: Option<String>,
    /// `Some(false)` — служба остановлена (карточка покажет «Остановлен» и «Запустить»).
    pub running: Option<bool>,
    pub address: Option<String>,
    pub fingerprint: Option<String>,
    /// Остальные поля по имени: `(ключ, значение строкой)`.
    pub extra: Vec<(String, String)>,
}

/// Чем вывод не годится.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatusError {
    #[error("the status output has no JSON object")]
    NoObject,
    #[error("the status output is not valid JSON: {0}")]
    Invalid(String),
}

const KNOWN: &[&str] = &["name", "version", "running", "address", "fingerprint"];
const SECRET_WORDS: &[&str] = &["token", "password", "secret", "passphrase"];

/// Разобрать вывод `status --json`. Перед объектом и после него могут быть строки журнала.
pub fn parse(output: &str) -> Result<Status, StatusError> {
    let trimmed = output.trim();
    let object = match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Object(map)) => map,
        Ok(_) => return Err(StatusError::NoObject),
        Err(first) => {
            // Строки журнала вокруг объекта: берём от первой `{` до последней `}`.
            let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) else {
                return Err(StatusError::NoObject);
            };
            match serde_json::from_str::<Value>(&trimmed[start..=end]) {
                Ok(Value::Object(map)) => map,
                _ => return Err(StatusError::Invalid(first.to_string())),
            }
        }
    };

    let text = |key: &str| match object.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    let mut status = Status {
        name: text("name"),
        version: text("version"),
        running: object.get("running").and_then(Value::as_bool),
        address: text("address"),
        fingerprint: text("fingerprint"),
        extra: Vec::new(),
    };
    for (key, value) in &object {
        if KNOWN.contains(&key.as_str()) {
            continue;
        }
        let hidden = SECRET_WORDS.iter().any(|w| key.to_ascii_lowercase().contains(w));
        let shown = match value {
            Value::Null => continue,
            _ if hidden => redact::ELLIPSIS.to_owned(),
            Value::String(s) => redact::mask(s),
            other => redact::mask(&other.to_string()),
        };
        status.extra.push((key.clone(), shown));
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_fields_are_taken_and_the_rest_is_extra() {
        let s = parse(
            r#"{"name":"relay","version":"0.3.1","running":true,"address":"203.0.113.7:8740",
                "fingerprint":"ab:cd","clients":12,"uptime":"3d","note":null,"api_token":"SECRET"}"#,
        )
        .unwrap();
        assert_eq!(s.name.as_deref(), Some("relay"));
        assert_eq!(s.version.as_deref(), Some("0.3.1"));
        assert_eq!(s.running, Some(true));
        assert_eq!(s.address.as_deref(), Some("203.0.113.7:8740"));
        assert_eq!(s.fingerprint.as_deref(), Some("ab:cd"));
        assert_eq!(
            s.extra,
            [
                ("api_token".to_owned(), "…".to_owned()),
                ("clients".to_owned(), "12".to_owned()),
                ("uptime".to_owned(), "3d".to_owned())
            ],
            "unknown fields are shown, null skipped, secrets hidden"
        );
    }

    #[test]
    fn a_stopped_service_is_reported() {
        let s = parse(r#"{"running": false}"#).unwrap();
        assert_eq!(s.running, Some(false));
        assert!(s.extra.is_empty());
        assert_eq!(parse("{}").unwrap().running, None, "an unknown state stays unknown");
    }

    #[test]
    fn log_lines_around_the_object_are_ignored() {
        let s = parse("starting up...\n{\n  \"running\": true,\n  \"version\": \"1\"\n}\ndone\n").unwrap();
        assert_eq!((s.running, s.version.as_deref()), (Some(true), Some("1")));
    }

    #[test]
    fn output_without_an_object_is_an_error() {
        assert_eq!(parse(""), Err(StatusError::NoObject));
        assert_eq!(parse("Unit example.service not found."), Err(StatusError::NoObject));
        assert_eq!(parse("[1,2]"), Err(StatusError::NoObject));
        assert!(matches!(parse("{\"running\": tru}"), Err(StatusError::Invalid(_))));
    }
}
