//! Каталог программ: список репозиториев для переноса на другой ПК (выгрузка и разбор). Сами программы ставит
//! `installs`/`app` из выпусков GitHub; здесь только текстовый список.

use crate::github::Repo;
use crate::sources;
use crate::worker::Project;

/// Строка за репозиторием, комментарии начинаются с `#`.
const HEADER: &str = "# Anvil: репозитории моих программ, по одному в строке (owner/name или адрес GitHub)";

/// Репозитории подключённых проектов с origin на GitHub: по алфавиту, по одному на строку.
pub fn export(projects: &[&Project]) -> String {
    let mut slugs: Vec<String> = projects
        .iter()
        .filter_map(|p| p.git()?.github())
        .filter_map(|url| Repo::from_url(&url))
        .map(|r| r.slug())
        .collect();
    slugs.sort_by_key(|s| s.to_lowercase());
    slugs.dedup_by_key(|s| s.to_lowercase());
    let mut text = String::from(HEADER);
    text.push('\n');
    for slug in slugs {
        text.push_str(&slug);
        text.push('\n');
    }
    text
}

/// Разобрать вставленный список: пустые строки и `#` пропускаются, повторы убираются, не-адреса возвращаются отдельно.
pub fn parse(text: &str) -> (Vec<Repo>, Vec<String>) {
    let mut repos: Vec<Repo> = Vec::new();
    let mut bad = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        match sources::parse_address(line) {
            Some(repo) if !repos.iter().any(|r| r.slug().eq_ignore_ascii_case(&repo.slug())) => repos.push(repo),
            Some(_) => {}
            None => bad.push(line.to_owned()),
        }
    }
    (repos, bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parsing() {
        let text =
            "# заголовок\nOwner/Alpha\n\nhttps://github.com/Owner/Beta.git  # комментарий\nowner/alpha\nне адрес\n";
        let (repos, bad) = parse(text);
        let slugs: Vec<String> = repos.iter().map(Repo::slug).collect();
        assert_eq!(slugs, ["Owner/Alpha", "Owner/Beta"]);
        assert_eq!(bad, ["не адрес"]);
        assert_eq!(parse(&export(&[])).0.len(), 0);
        assert!(export(&[]).starts_with('#'));
    }
}
