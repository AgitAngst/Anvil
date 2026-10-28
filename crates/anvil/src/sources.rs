//! Источники проектов: разбор адреса, сопоставление репозитория аккаунта с папкой, клонирование.
//! Окно «Добавить проект» — `ui/add_project.rs`; список репозиториев аккаунта читает поток GitHub.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use eframe::egui;

use crate::config::{Config, Visibility};
use crate::github::{AccountRepo, Remote, Repo};
use crate::run;
use crate::worker::Project;

/// `https://github.com/o/n(.git)`, `git@github.com:o/n.git`, `github.com/o/n`, `o/n` → репозиторий.
/// Лишние части пути адреса сайта (`/tree/main`) отбрасываются; имена — только из букв, цифр, `-`, `_`, `.`.
pub fn parse_address(text: &str) -> Option<Repo> {
    let text = text.trim();
    let rest = ["https://github.com/", "http://github.com/", "git@github.com:", "ssh://git@github.com/", "github.com/"]
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))
        .unwrap_or(text);
    let mut parts = rest.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let name = parts.next()?.trim_end_matches(".git");
    // Голое `owner/name/x` — не адрес репозитория; хвост допустим только у адреса с сайта.
    let tail_ok = text != rest || parts.next().is_none();
    (tail_ok && safe_name(owner) && safe_name(name)).then(|| Repo { owner: owner.to_owned(), name: name.to_owned() })
}

/// Имя можно ставить в путь: без разделителей, `..` и пробелов.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn clone_url(repo: &Repo) -> String {
    format!("https://github.com/{}/{}.git", repo.owner, repo.name)
}

/// Проект, у которого origin — этот репозиторий.
pub fn local_for<'a>(repo: &Repo, projects: &'a [Project]) -> Option<&'a Project> {
    let wanted = repo.slug().to_lowercase();
    projects.iter().find(|p| {
        p.git()
            .and_then(|g| g.github())
            .and_then(|url| Repo::from_url(&url))
            .is_some_and(|found| found.slug().to_lowercase() == wanted)
    })
}

/// Куда клонировать: заданная папка или первая из папок поиска.
pub fn clone_root(config: &Config) -> Option<PathBuf> {
    config.clone_dir.clone().or_else(|| config.roots.first().cloned())
}

/// Закрытый ли репозиторий проекта: слово владельца важнее ответа GitHub.
pub fn is_private(config: &Config, path: &Path, remote: Option<&Remote>) -> Option<bool> {
    match config.project(path).visibility {
        Some(Visibility::Private) => Some(true),
        Some(Visibility::Public) => Some(false),
        None => remote.and_then(|r| r.private),
    }
}

/// Репозитории аккаунта, которых нет на диске и о которых владельцу ещё не говорили.
pub fn new_repos<'a>(config: &Config, account: &'a [AccountRepo], projects: &[Project]) -> Vec<&'a AccountRepo> {
    let Some(seen) = &config.seen_repos else { return Vec::new() };
    account
        .iter()
        .filter(|r| !r.archived)
        .filter(|r| !seen.contains(&r.repo.slug().to_lowercase()))
        .filter(|r| local_for(&r.repo, projects).is_none())
        .collect()
}

/// Все репозитории аккаунта как «уже видели» (нижний регистр, по алфавиту).
pub fn slugs(account: &[AccountRepo]) -> Vec<String> {
    let mut all: Vec<String> = account.iter().map(|r| r.repo.slug().to_lowercase()).collect();
    all.sort();
    all.dedup();
    all
}

/// Чем кончилось клонирование.
pub struct Cloned {
    pub repo: Repo,
    pub result: Result<PathBuf, String>,
}

/// `git clone` в `root/<имя>` в своём потоке. Папка уже есть — ошибка, ничего не трогаем.
pub fn clone(ctx: egui::Context, tx: Sender<Cloned>, repo: Repo, root: PathBuf) {
    let spawned = std::thread::Builder::new().name("anvil-clone".into()).spawn({
        let (ctx, tx, repo) = (ctx.clone(), tx.clone(), repo.clone());
        move || {
            let result = clone_blocking(&repo, &root);
            let _ = tx.send(Cloned { repo, result });
            ctx.request_repaint();
        }
    });
    if spawned.is_err() {
        let _ = tx.send(Cloned { repo, result: Err("cannot start a thread".to_owned()) });
        ctx.request_repaint();
    }
}

fn clone_blocking(repo: &Repo, root: &Path) -> Result<PathBuf, String> {
    if !safe_name(&repo.name) || !safe_name(&repo.owner) {
        return Err("bad repository name".to_owned());
    }
    let dest = root.join(&repo.name);
    if dest.exists() {
        return Err(format!("{} exists", dest.display()));
    }
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let dest_arg = dest.to_string_lossy().into_owned();
    run::output("git", root, &["clone", "--", &clone_url(repo), &dest_arg])?;
    Ok(dest)
}

/// Состояние окна «Добавить проект».
#[derive(Default)]
pub struct State {
    pub open: bool,
    /// 0 — «С GitHub», 1 — «Папка», 2 — «Адрес».
    pub tab: usize,
    pub address: String,
    pub filter: String,
    /// Репозитории аккаунта или причина, почему списка нет; `None` — ещё не спрашивали.
    pub account: Option<Result<Vec<AccountRepo>, String>>,
    pub loading: bool,
    /// Что клонируется сейчас.
    pub cloning: Option<Repo>,
    /// Ошибка последнего клонирования.
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(owner: &str, name: &str) -> Repo {
        Repo { owner: owner.to_owned(), name: name.to_owned() }
    }

    #[test]
    fn addresses() {
        let want = Some(repo("Owner", "Demo"));
        for text in [
            "Owner/Demo",
            " https://github.com/Owner/Demo ",
            "https://github.com/Owner/Demo.git",
            "https://github.com/Owner/Demo/tree/main",
            "git@github.com:Owner/Demo.git",
            "github.com/Owner/Demo",
        ] {
            assert_eq!(parse_address(text), want, "{text}");
        }
        for text in ["", "Demo", "a/b/c", "a/../b", "a/b c", "https://example.com/a/b", "https://github.com/a", "../x"]
        {
            assert_eq!(parse_address(text), None, "{text}");
        }
    }

    #[test]
    fn names_are_path_safe() {
        assert!(safe_name("demo-releases") && safe_name("a_b.c"));
        assert!(!safe_name("..") && !safe_name("") && !safe_name("a/b") && !safe_name("a\\b") && !safe_name("a b"));
        assert_eq!(clone_url(&repo("o", "n")), "https://github.com/o/n.git");
    }

    #[test]
    fn clone_refuses_bad_names_and_existing_folders() {
        let dir = std::env::temp_dir().join(format!("anvil-clone-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("taken")).unwrap();
        assert!(clone_blocking(&repo("o", "taken"), &dir).unwrap_err().contains("exists"));
        assert!(clone_blocking(&repo("o", ".."), &dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manual_visibility_wins() {
        let mut config = Config::default();
        let path = Path::new(r"E:\x");
        let remote = Remote { private: Some(true), ..Remote::default() };
        assert_eq!(is_private(&config, path, Some(&remote)), Some(true));
        assert_eq!(is_private(&config, path, None), None);
        config.project_mut(path).visibility = Some(Visibility::Public);
        assert_eq!(is_private(&config, path, Some(&remote)), Some(false));
    }

    #[test]
    fn new_repos_skip_seen_and_local() {
        let account = |slug: &str, archived| AccountRepo {
            repo: Repo::from_slug(slug).unwrap(),
            private: false,
            description: None,
            fork: false,
            archived,
        };
        let list = vec![account("o/old", false), account("o/fresh", false), account("o/dead", true)];
        let mut config = Config::default();
        // Точки отсчёта ещё нет — ничего не «новое».
        assert!(new_repos(&config, &list, &[]).is_empty());
        config.seen_repos = Some(slugs(&list[..1]));
        let fresh = new_repos(&config, &list, &[]);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].repo.slug(), "o/fresh");
        assert_eq!(slugs(&list), ["o/dead", "o/fresh", "o/old"]);
    }
}
