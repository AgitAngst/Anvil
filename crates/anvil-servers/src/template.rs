//! Подстановки в командах манифеста: `{bin}`, `{args}`, `{service}`, `{dir}`, `{instance}`, `{user}`.
//!
//! `{{` и `}}` дают сами скобки (для `${{HOME}}` и `awk '{{print $1}}'`). Незнакомое имя — ошибка при
//! разборе манифеста, а не пустая строка на машине под root.

/// Имена, которые понимают команды и действия.
pub const COMMAND_VARS: &[&str] = &["bin", "args", "service", "dir", "instance", "user"];

/// Часть шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Var(String),
}

/// Разобрать шаблон на текст и подстановки.
pub fn parse(template: &str) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) if c.is_ascii_lowercase() || c == '_' => name.push(c),
                        Some(c) => return Err(format!("unexpected `{c}` inside a placeholder")),
                        None => return Err("a placeholder is not closed".to_owned()),
                    }
                }
                if name.is_empty() {
                    return Err("an empty placeholder `{}`".to_owned());
                }
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Var(name));
            }
            '}' => return Err("a stray `}`".to_owned()),
            c => text.push(c),
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

/// Проверить, что в шаблоне только допустимые подстановки. Возвращает те, что в нём есть.
pub fn check(template: &str, allowed: &[&str]) -> Result<Vec<String>, String> {
    let mut used = Vec::new();
    for piece in parse(template)? {
        if let Piece::Var(name) = piece {
            if !allowed.contains(&name.as_str()) {
                return Err(format!("unknown placeholder `{{{name}}}`"));
            }
            used.push(name);
        }
    }
    Ok(used)
}

/// Собрать строку; `value` отдаёт готовый к вставке текст подстановки (уже в кавычках) или `None`.
pub fn render(template: &str, mut value: impl FnMut(&str) -> Option<String>) -> Result<String, String> {
    let mut out = String::new();
    for piece in parse(template)? {
        match piece {
            Piece::Text(t) => out.push_str(&t),
            Piece::Var(name) => match value(&name) {
                Some(v) => out.push_str(&v),
                None => return Err(format!("`{{{name}}}` has no value here")),
            },
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_found_and_braces_escape() {
        assert_eq!(check("{bin} install {args}", COMMAND_VARS).unwrap(), ["bin", "args"]);
        assert_eq!(render("echo ${{HOME}} {bin}", |n| (n == "bin").then(|| "x".to_owned())).unwrap(), "echo ${HOME} x");
    }

    #[test]
    fn bad_templates_are_refused() {
        assert!(check("{nope}", COMMAND_VARS).unwrap_err().contains("nope"));
        assert!(parse("{bin").is_err());
        assert!(parse("a } b").is_err());
        assert!(parse("{}").is_err());
        assert!(parse("{b in}").is_err());
        assert!(render("{dir}", |_| None).is_err());
    }
}
