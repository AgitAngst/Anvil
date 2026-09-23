//! Состояние репозитория — через `git` из командной строки, чтобы всё было так же, как в терминале.

use std::path::Path;

use crate::run;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GitState {
    /// Ветка; `None` — отсоединённый HEAD.
    pub branch: Option<String>,
    /// Ветка на origin, за которой следим: `origin/main`.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changes: Vec<Change>,
    pub last_tag: Option<String>,
    /// Коммитов после последнего тега.
    pub since_tag: u32,
    pub commits: Vec<Commit>,
    pub remote: Option<String>,
    /// Когда последний раз спрашивали origin (время `FETCH_HEAD`), секунды UNIX.
    pub fetched_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// Буква состояния: `M`, `A`, `D`, `R`, `?`, `U`.
    pub kind: char,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub author: String,
    pub time: i64,
}

impl GitState {
    pub fn dirty(&self) -> bool {
        !self.changes.is_empty()
    }

    pub fn last_commit_time(&self) -> i64 {
        self.commits.first().map_or(0, |c| c.time)
    }

    /// Адрес на GitHub, если origin там: `https://github.com/owner/repo`.
    pub fn github(&self) -> Option<String> {
        github_url(self.remote.as_deref()?)
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut all = vec!["-c", "core.quotepath=false", "-c", "color.ui=never"];
    all.extend_from_slice(args);
    run::output("git", dir, &all)
}

/// Прочитать состояние. `Ok(None)` — папка не под git.
pub fn read(dir: &Path) -> Result<Option<GitState>, String> {
    if !dir.join(".git").exists() {
        return Ok(None);
    }
    let status = git(dir, &["status", "--porcelain=v2", "--branch"])?;
    let mut state = parse_status(&status);

    state.commits =
        git(dir, &["log", "-40", "--format=%h%x1f%s%x1f%an%x1f%ct"]).map(|s| parse_log(&s)).unwrap_or_default();
    state.last_tag = git(dir, &["describe", "--tags", "--abbrev=0"]).ok().map(|s| s.trim().to_owned());
    if let Some(tag) = &state.last_tag {
        let range = format!("{tag}..HEAD");
        state.since_tag =
            git(dir, &["rev-list", "--count", &range]).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    }
    state.remote = git(dir, &["remote", "get-url", "origin"]).ok().map(|s| s.trim().to_owned());
    state.fetched_at = std::fs::metadata(dir.join(".git").join("FETCH_HEAD"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    Ok(Some(state))
}

/// Спросить origin о новых коммитах. Ничего не меняет в рабочей копии.
pub fn fetch(dir: &Path) -> Result<(), String> {
    git(dir, &["fetch", "--quiet", "--prune", "origin"]).map(|_| ())
}

fn parse_status(text: &str) -> GitState {
    let mut state = GitState::default();
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            state.branch = (head != "(detached)").then(|| head.to_owned());
        } else if let Some(up) = line.strip_prefix("# branch.upstream ") {
            state.upstream = Some(up.to_owned());
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            for part in ab.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    state.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    state.behind = n.parse().unwrap_or(0);
                }
            }
        } else if let Some(change) = parse_change(line) {
            state.changes.push(change);
        }
    }
    state
}

fn parse_change(line: &str) -> Option<Change> {
    let (tag, rest) = line.split_once(' ')?;
    match tag {
        "?" => Some(Change { kind: '?', path: rest.to_owned() }),
        // «1 XY sub mH mI mW hH hI путь»
        "1" => {
            let fields: Vec<&str> = rest.splitn(8, ' ').collect();
            Some(Change { kind: xy_kind(fields.first()?), path: fields.get(7)?.to_string() })
        }
        // «2 XY sub mH mI mW hH hI Xscore путь\tстарый_путь»
        "2" => {
            let fields: Vec<&str> = rest.splitn(9, ' ').collect();
            let path = fields.get(8)?.split('\t').next()?;
            Some(Change { kind: 'R', path: path.to_owned() })
        }
        // «u XY sub m1 m2 m3 mW h1 h2 h3 путь»
        "u" => rest.splitn(10, ' ').nth(9).map(|p| Change { kind: 'U', path: p.to_owned() }),
        _ => None,
    }
}

/// Из пары «индекс / рабочая копия» — одна буква: что важнее, то и видно.
fn xy_kind(xy: &str) -> char {
    let mut chars = xy.chars();
    let (x, y) = (chars.next().unwrap_or('.'), chars.next().unwrap_or('.'));
    for c in [x, y] {
        if c == 'D' {
            return 'D';
        }
    }
    for c in [x, y] {
        if c == 'A' {
            return 'A';
        }
    }
    'M'
}

fn parse_log(text: &str) -> Vec<Commit> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split('\x1f');
            Some(Commit {
                hash: f.next()?.to_owned(),
                subject: f.next()?.to_owned(),
                author: f.next()?.to_owned(),
                time: f.next()?.trim().parse().ok()?,
            })
        })
        .collect()
}

/// `git@github.com:owner/repo.git`, `https://github.com/owner/repo(.git)` → `https://github.com/owner/repo`.
pub fn github_url(remote: &str) -> Option<String> {
    let rest = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .or_else(|| remote.split_once("@github.com/").map(|(_, r)| r))?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    (rest.split('/').count() == 2).then(|| format!("https://github.com/{rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_v2() {
        let text = "\
# branch.oid 451da94
# branch.head main
# branch.upstream origin/main
# branch.ab +2 -1
1 .M N... 100644 100644 100644 aaa bbb src/main.rs
1 A. N... 000000 100644 100644 000 ccc src/новый файл.rs
1 .D N... 100644 100644 000000 ddd ddd old.rs
2 R. N... 100644 100644 100644 eee eee R100 src/new.rs\tsrc/old.rs
? notes.txt
";
        let s = parse_status(text);
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        let kinds: Vec<(char, &str)> = s.changes.iter().map(|c| (c.kind, c.path.as_str())).collect();
        assert_eq!(
            kinds,
            [
                ('M', "src/main.rs"),
                ('A', "src/новый файл.rs"),
                ('D', "old.rs"),
                ('R', "src/new.rs"),
                ('?', "notes.txt")
            ]
        );
    }

    #[test]
    fn detached_head_and_no_upstream() {
        let s = parse_status("# branch.oid abc\n# branch.head (detached)\n");
        assert_eq!(s.branch, None);
        assert_eq!(s.upstream, None);
        assert!(!s.dirty());
    }

    #[test]
    fn log_lines() {
        let log = parse_log("451da94\x1fTODO: раздел\x1fNikolay\x1f1790000000\n");
        assert_eq!(log[0].subject, "TODO: раздел");
        assert_eq!(log[0].time, 1_790_000_000);
    }

    #[test]
    fn github_remotes() {
        let want = Some("https://github.com/AgitAngst/Anvil".to_owned());
        assert_eq!(github_url("https://github.com/AgitAngst/Anvil.git"), want);
        assert_eq!(github_url("git@github.com:AgitAngst/Anvil.git"), want);
        assert_eq!(github_url("https://token@github.com/AgitAngst/Anvil"), want);
        assert_eq!(github_url("https://gitlab.com/a/b"), None);
    }
}
