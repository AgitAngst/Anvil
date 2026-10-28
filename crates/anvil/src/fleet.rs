//! Парк серверов и ботов: записи в `anvil.toml` (`[[fleet]]`), проверка «жив ли» и команды обновления и отката.
//!
//! Anvil сам не ходит по SSH и не знает, как устроено обновление каждого сервера: проверка — TCP-порт или HTTP-адрес,
//! обновление и откат — команды, которые владелец сам записал в запись (`update_cmd`, `rollback_cmd`); перед запуском окно
//! показывает команду дословно и спрашивает. Серверы Amber берутся из сводки amber-admin (`amber.rs`) — только чтение.

use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::run;

/// Чем является запись.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    #[default]
    Server,
    Bot,
    Service,
}

/// Запись парка.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub name: String,
    pub group: Group,
    /// Как проверять: `tcp:хост:порт` или `http(s)://адрес` (ответ 2xx — жив; версия — из JSON `version` или первого `X.Y.Z`).
    pub probe: String,
    /// Команда обновления (`cmd /C …`); пусто — кнопки нет.
    pub update_cmd: String,
    /// Команда отката; пусто — кнопки нет.
    pub rollback_cmd: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Tcp(String, u16),
    Http(String),
    None,
}

pub fn parse_probe(text: &str) -> Probe {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("tcp:") {
        if let Some((host, port)) = rest.rsplit_once(':')
            && !host.is_empty()
            && let Ok(port) = port.parse::<u16>()
        {
            return Probe::Tcp(host.to_owned(), port);
        }
        return Probe::None;
    }
    if text.starts_with("http://") || text.starts_with("https://") {
        return Probe::Http(text.to_owned());
    }
    Probe::None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Up,
    Down,
    /// Проверка не задана.
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub state: State,
    pub latency_ms: Option<u32>,
    pub version: Option<String>,
    /// Когда проверяли (секунды UNIX).
    pub checked: i64,
    pub error: Option<String>,
}

const TIMEOUT: Duration = Duration::from_secs(5);

pub fn check(entry: &Entry) -> Status {
    let started = Instant::now();
    let checked = crate::i18n::now();
    let down =
        |error: String| Status { state: State::Down, latency_ms: None, version: None, checked, error: Some(error) };
    let latency = || started.elapsed().as_millis().min(u32::MAX as u128) as u32;
    match parse_probe(&entry.probe) {
        Probe::None => Status { state: State::Unknown, latency_ms: None, version: None, checked, error: None },
        Probe::Tcp(host, port) => {
            let addr = match (host.as_str(), port).to_socket_addrs().ok().and_then(|mut a| a.next()) {
                Some(addr) => addr,
                None => return down(format!("{host}: no address")),
            };
            match TcpStream::connect_timeout(&addr, TIMEOUT) {
                Ok(_) => Status { state: State::Up, latency_ms: Some(latency()), version: None, checked, error: None },
                Err(e) => down(e.to_string()),
            }
        }
        Probe::Http(url) => {
            let client = match reqwest::blocking::Client::builder()
                .user_agent(concat!("anvil/", env!("CARGO_PKG_VERSION")))
                .timeout(TIMEOUT)
                .build()
            {
                Ok(client) => client,
                Err(e) => return down(e.to_string()),
            };
            match client.get(&url).send() {
                Ok(response) if response.status().is_success() => {
                    let mut body = String::new();
                    let _ = response.take(4096).read_to_string(&mut body);
                    Status {
                        state: State::Up,
                        latency_ms: Some(latency()),
                        version: extract_version(&body),
                        checked,
                        error: None,
                    }
                }
                Ok(response) => down(format!("HTTP {}", response.status().as_u16())),
                Err(e) => down(e.without_url().to_string()),
            }
        }
    }
}

/// Версия из ответа: поле `version` у JSON, иначе первое `X.Y.Z` в начале текста.
pub fn extract_version(body: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(version) = value.get("version").and_then(|v| v.as_str())
    {
        return Some(version.to_owned());
    }
    let head: String = body.chars().take(200).collect();
    let bytes = head.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut dots = 0;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && dots < 2 && i + 1 < bytes.len()))
            {
                if bytes[i] == b'.' {
                    dots += 1;
                }
                i += 1;
            }
            let candidate = head[start..i].trim_end_matches('.');
            if candidate.split('.').count() == 3 && candidate.split('.').all(|p| !p.is_empty()) {
                return Some(candidate.to_owned());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Выполнить команду записи и вернуть код выхода и последние строки вывода.
pub fn run_command(line: &str) -> (i32, String) {
    #[cfg(windows)]
    let output = run::command("cmd", Path::new(".")).args(["/C", line]).output();
    #[cfg(not(windows))]
    let output = run::command("sh", Path::new(".")).args(["-c", line]).output();
    match output {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            let tail: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).rev().take(6).collect();
            (out.status.code().unwrap_or(-1), tail.into_iter().rev().collect::<Vec<_>>().join("\n"))
        }
        Err(e) => (-1, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn probes() {
        assert_eq!(parse_probe("tcp:example.org:443"), Probe::Tcp("example.org".into(), 443));
        assert_eq!(parse_probe(" https://example.org/health "), Probe::Http("https://example.org/health".into()));
        assert_eq!(parse_probe("tcp:nohost"), Probe::None);
        assert_eq!(parse_probe("tcp::80"), Probe::None);
        assert_eq!(parse_probe("ssh:host"), Probe::None);
        assert_eq!(parse_probe(""), Probe::None);
    }

    #[test]
    fn versions() {
        assert_eq!(extract_version(r#"{"status":"ok","version":"0.4.2"}"#).as_deref(), Some("0.4.2"));
        assert_eq!(extract_version("demo-server 0.3.1 (build 12)\n").as_deref(), Some("0.3.1"));
        assert_eq!(extract_version("v1.12.0-rc").as_deref(), Some("1.12.0"));
        assert_eq!(extract_version("no digits here"), None);
        assert_eq!(extract_version("port 8080 only"), None);
    }

    #[test]
    fn tcp_up_and_down() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let entry = Entry { name: "local".into(), probe: format!("tcp:127.0.0.1:{port}"), ..Entry::default() };
        let up = check(&entry);
        assert_eq!(up.state, State::Up);
        assert!(up.latency_ms.is_some());
        drop(listener);
        // Закрытый порт: освобождённый только что бывает занят чужим процессом, поэтому — порт 1, где никто не слушает.
        let entry = Entry { probe: "tcp:127.0.0.1:1".into(), ..entry };
        let down = check(&entry);
        assert_eq!(down.state, State::Down);
        assert!(down.error.is_some());
        assert_eq!(check(&Entry::default()).state, State::Unknown);
    }

    #[test]
    fn commands_report_exit_code_and_tail() {
        #[cfg(windows)]
        let (code, text) = run_command("echo first && echo second && exit 3");
        #[cfg(not(windows))]
        let (code, text) = run_command("echo first; echo second; exit 3");
        assert_eq!(code, 3);
        assert!(text.contains("second"), "{text}");
    }

    #[test]
    fn entries_round_trip_in_toml() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Wrap {
            fleet: Vec<Entry>,
        }
        let wrap = Wrap {
            fleet: vec![Entry { name: "ES-1".into(), group: Group::Bot, probe: "tcp:h:1".into(), ..Entry::default() }],
        };
        let text = toml::to_string_pretty(&wrap).unwrap();
        assert!(text.contains("[[fleet]]") && text.contains("group = \"bot\""));
        assert_eq!(toml::from_str::<Wrap>(&text).unwrap(), wrap);
        // Старые записи без полей берут умолчания.
        let minimal: Wrap = toml::from_str("[[fleet]]\nname = 'x'\n").unwrap();
        assert_eq!(minimal.fleet[0].group, Group::Server);
    }
}
