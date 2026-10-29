//! Форма установки: значения полей проверяются по виду поля и безопасно становятся аргументами команды.
//!
//! Всё, что попадает в `{args}`, — слово оболочки: безопасное остаётся как есть, остальное в одинарных кавычках
//! ([`crate::shell::word`]). Текст не может начинаться с `-`, чтобы значение не стало ключом команды.

use std::collections::BTreeMap;

use crate::manifest::{Field, FieldKind, Manifest};
use crate::shell;
use crate::template;

/// Самое длинное текстовое значение.
const MAX_TEXT: usize = 256;

/// Какое поле и чем не подошло.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{label}: {reason}")]
pub struct FormError {
    pub key: String,
    pub label: String,
    pub reason: String,
}

/// Привести введённое к виду, в котором оно уйдёт в команду. `Err` — человеческая причина.
pub(crate) fn normalize(field: &Field, value: &str) -> Result<String, String> {
    match field.kind {
        FieldKind::Text | FieldKind::Password => {
            let value = if field.kind == FieldKind::Text { value.trim() } else { value };
            if value.is_empty() {
                return Err("empty".to_owned());
            }
            if value.chars().count() > MAX_TEXT {
                return Err(format!("longer than {MAX_TEXT} characters"));
            }
            if value.chars().any(char::is_control) {
                return Err("control characters are not allowed".to_owned());
            }
            if value.starts_with('-') {
                return Err("must not start with `-`".to_owned());
            }
            Ok(value.to_owned())
        }
        FieldKind::Number => {
            let n: i64 = value.trim().parse().map_err(|_| format!("`{value}` is not a whole number"))?;
            if let Some(min) = field.min
                && n < min
            {
                return Err(format!("less than {min}"));
            }
            if let Some(max) = field.max
                && n > max
            {
                return Err(format!("more than {max}"));
            }
            Ok(n.to_string())
        }
        FieldKind::Port => {
            let n: u16 = value.trim().parse().map_err(|_| format!("`{value}` is not a port (1-65535)"))?;
            if n == 0 {
                return Err("port 0 is not a port (1-65535)".to_owned());
            }
            Ok(n.to_string())
        }
        FieldKind::Bool => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "on" | "yes" | "1" => Ok("true".to_owned()),
            "false" | "off" | "no" | "0" => Ok("false".to_owned()),
            _ => Err("expected true or false".to_owned()),
        },
        FieldKind::Choice => {
            if field.options.iter().any(|o| o == value) {
                Ok(value.to_owned())
            } else {
                Err(format!("not one of: {}", field.options.join(", ")))
            }
        }
    }
}

impl Manifest {
    /// Проверенные значения полей: введённые, а где не введено — по умолчанию. Нет значения и не обязательно —
    /// поля в ответе нет.
    pub fn form_values(&self, input: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>, FormError> {
        if let Some(key) = input.keys().find(|k| !self.form.iter().any(|f| &f.key == *k)) {
            return Err(FormError { key: key.clone(), label: key.clone(), reason: "no such field".to_owned() });
        }
        let mut out = BTreeMap::new();
        for field in &self.form {
            let fail = |reason: String| FormError { key: field.key.clone(), label: field.label.clone(), reason };
            let given = input.get(&field.key).filter(|v| !v.is_empty());
            match given.or(field.default.as_ref()) {
                Some(value) => {
                    out.insert(field.key.clone(), normalize(field, value).map_err(fail)?);
                }
                None if field.required => return Err(fail("required".to_owned())),
                None => {}
            }
        }
        Ok(out)
    }

    /// Строка для `{args}`: аргументы всех заполненных полей, каждое значение — безопасное слово оболочки.
    pub fn form_args(&self, input: &BTreeMap<String, String>) -> Result<String, FormError> {
        let values = self.form_values(input)?;
        let mut args = Vec::new();
        for field in &self.form {
            let (Some(value), Some(arg)) = (values.get(&field.key), &field.arg) else { continue };
            if field.kind == FieldKind::Bool && value != "true" {
                continue;
            }
            let rendered = template::render(arg, |name| (name == "value").then(|| shell::word(value)))
                .map_err(|reason| FormError { key: field.key.clone(), label: field.label.clone(), reason })?;
            args.push(rendered);
        }
        Ok(args.join(" "))
    }

    /// Порты из полей вида `port` — их центр сверяет с занятыми ([`crate::instance::check_ports`]).
    pub fn form_ports(&self, input: &BTreeMap<String, String>) -> Result<Vec<u16>, FormError> {
        let values = self.form_values(input)?;
        Ok(self
            .form
            .iter()
            .filter(|f| f.kind == FieldKind::Port)
            .filter_map(|f| values.get(&f.key)?.parse().ok())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::EXAMPLE;

    fn manifest() -> Manifest {
        let text = format!(
            "{EXAMPLE}\n[[form]]\nkey = \"name\"\nlabel = \"Имя\"\nrequired = true\narg = \"--name {{value}}\"\n\
             [[form]]\nkey = \"tls\"\nkind = \"bool\"\ndefault = false\narg = \"--tls\"\n\
             [[form]]\nkey = \"mode\"\nkind = \"choice\"\noptions = [\"fast\", \"safe\"]\narg = \"--mode {{value}}\"\n\
             [[form]]\nkey = \"keep\"\nkind = \"number\"\nmin = 1\nmax = 30\narg = \"--keep {{value}}\"\n"
        );
        Manifest::parse(&text).unwrap()
    }

    fn input(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn defaults_fill_in_and_arguments_come_out_in_form_order() {
        let m = manifest();
        assert_eq!(m.form_args(&input(&[("name", "main")])).unwrap(), "--port 8740 --name main");
        let all = input(&[("name", "main"), ("port", "9000"), ("tls", "yes"), ("mode", "safe"), ("keep", "7")]);
        assert_eq!(m.form_args(&all).unwrap(), "--port 9000 --name main --tls --mode safe --keep 7");
    }

    #[test]
    fn a_required_field_is_named_when_missing() {
        let err = manifest().form_args(&input(&[])).unwrap_err();
        assert_eq!((err.key.as_str(), err.reason.as_str()), ("name", "required"));
        assert_eq!(err.to_string(), "Имя: required");
    }

    #[test]
    fn values_are_checked_by_the_kind_of_the_field() {
        let m = manifest();
        for (key, bad) in [
            ("port", "0"),
            ("port", "70000"),
            ("port", "80;reboot"),
            ("keep", "0"),
            ("keep", "31"),
            ("keep", "x"),
            ("mode", "turbo"),
            ("tls", "maybe"),
            ("name", "-rf"),
            ("name", "two\nlines"),
        ] {
            let err = m.form_args(&input(&[("name", "ok"), (key, bad)]));
            let err = if key == "name" { m.form_args(&input(&[(key, bad)])) } else { err };
            assert!(err.is_err(), "{key}={bad:?} must be refused");
        }
        assert!(m.form_args(&input(&[("name", "ok"), ("nope", "1")])).is_err(), "unknown field");
    }

    #[test]
    fn a_value_cannot_break_out_of_its_argument() {
        let m = manifest();
        for hostile in ["a b", "x; rm -rf /", "$(reboot)", "`id`", "it's", "a && b", "a|b", "*", "a#b", "~root"] {
            let args = m.form_args(&input(&[("name", hostile)])).unwrap();
            let expected = format!("--port 8740 --name {}", crate::shell::quote(hostile));
            assert_eq!(args, expected, "{hostile}");
        }
    }

    #[test]
    fn port_fields_are_listed_for_the_busy_check() {
        let m = manifest();
        assert_eq!(m.form_ports(&input(&[("name", "x")])).unwrap(), [8740]);
        assert_eq!(m.form_ports(&input(&[("name", "x"), ("port", "9001")])).unwrap(), [9001]);
    }
}
