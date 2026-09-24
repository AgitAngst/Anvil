//! Коротко о серверах Amber: сводка, которую amber-admin пишет после каждой проверки
//! (`%APPDATA%\amber\Amber\config\admin-servers.json`). Anvil её только читает — ни адресов,
//! ни ключей, ни своих запросов к серверам; подробности — в самом amber-admin.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use serde::Deserialize;

/// Бинарник, у проекта которого показывается строка.
pub const ADMIN: &str = "amber-admin";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Summary {
    /// Когда amber-admin проверял, секунды Unix.
    pub checked_at: i64,
    #[serde(default)]
    pub servers: Vec<Server>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Server {
    pub name: String,
    /// `up`, `down`, `bare`, `unreachable`, `unknown`.
    pub health: String,
    pub version: Option<String>,
    pub members: Option<usize>,
    pub online: Option<usize>,
}

pub fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|dir| PathBuf::from(dir).join("amber").join("Amber").join("config").join("admin-servers.json"))
}

/// Следит за файлом сводки: перечитывает, только когда он поменялся, и смотрит не чаще раза в 3 с.
#[derive(Default)]
pub struct Watch {
    looked: Option<Instant>,
    modified: Option<SystemTime>,
    /// `None` — сводки нет (amber-admin ещё не проверял серверы на этой машине).
    pub summary: Option<Result<Summary, String>>,
}

impl Watch {
    pub fn get(&mut self) -> Option<&Result<Summary, String>> {
        if self.looked.is_none_or(|t| t.elapsed() >= Duration::from_secs(3)) {
            self.looked = Some(Instant::now());
            let file = path();
            let modified = file.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
            if modified != self.modified {
                self.modified = modified;
                self.summary = file.filter(|_| modified.is_some()).map(|p| read(&p));
            }
        }
        self.summary.as_ref()
    }
}

fn read(path: &std::path::Path) -> Result<Summary, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<Summary, String> {
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_admin_writes() {
        let text = r#"{
  "checked_at": 1790000000,
  "servers": [
    { "name": "home", "health": "up", "version": "0.3.0", "members": 5, "online": 2 },
    { "name": "spare", "health": "unreachable", "version": null, "members": null, "online": null }
  ]
}"#;
        let summary = parse(text).unwrap();
        assert_eq!(summary.checked_at, 1_790_000_000);
        assert_eq!(summary.servers.len(), 2);
        assert_eq!(summary.servers[0].online, Some(2));
        assert_eq!(summary.servers[1].health, "unreachable");
        assert!(parse("{").is_err());
    }
}
