//! GitHub: CI и выпуски проектов через REST API, свой поток.
//!
//! Токен — только для чтения, и только до api.github.com. Откуда он берётся, по порядку:
//! свой из хранилища Windows (`keyring`, служба `anvil`), тот же, что у git (`git credential fill`
//! без окон входа), иначе — без токена: видны только публичные репозитории, 60 запросов в час.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use serde::Deserialize;

use crate::run;

const KEYRING_SERVICE: &str = "anvil";
const KEYRING_USER: &str = "github";
const API: &str = "https://api.github.com";
/// Как часто спрашивать GitHub без просьбы.
const POLL_EVERY: Duration = Duration::from_secs(5 * 60);

/// Репозиторий на GitHub: `owner/name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl Repo {
    /// Из адреса `https://github.com/owner/name`.
    pub fn from_url(url: &str) -> Option<Repo> {
        let rest = url.strip_prefix("https://github.com/")?;
        let (owner, name) = rest.split_once('/')?;
        Some(Repo { owner: owner.to_owned(), name: name.to_owned() })
    }
}

/// Что спросить у GitHub о проекте.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub path: PathBuf,
    pub repo: Repo,
    /// Ветка, по которой смотреть CI.
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Queued,
    Running,
    Success,
    Failure,
    Cancelled,
    /// Пропущен, нейтральный итог и прочее без явного «да» или «нет».
    Other,
}

/// Прогон CI.
#[derive(Debug, Clone)]
pub struct CiRun {
    pub id: u64,
    pub workflow: String,
    pub number: u64,
    pub state: RunState,
    pub head_sha: String,
    pub title: String,
    pub url: String,
    pub updated: i64,
    /// Упавшие задачи прогона — только у упавшего последнего.
    pub failed_jobs: Vec<FailedJob>,
}

#[derive(Debug, Clone)]
pub struct FailedJob {
    pub name: String,
    pub url: String,
    /// Упавшие шаги.
    pub steps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub url: String,
    pub published: i64,
    pub prerelease: bool,
    pub draft: bool,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub downloads: u64,
    pub url: String,
}

/// Что известно о проекте на GitHub.
#[derive(Debug, Clone, Default)]
pub struct Remote {
    /// Последние прогоны CI по ветке, свежие первыми.
    pub runs: Vec<CiRun>,
    pub releases: Vec<Release>,
    /// Когда спрашивали (секунды UNIX).
    pub checked: i64,
    pub error: Option<String>,
}

impl Remote {
    /// Последний завершённый или идущий прогон по ветке.
    pub fn latest(&self) -> Option<&CiRun> {
        self.runs.first()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    /// Свой токен из хранилища Windows.
    Keyring,
    /// Тот же, что у git (Git Credential Manager).
    Git,
    None,
}

/// Состояние входа: откуда токен и кто под ним.
#[derive(Debug, Clone)]
pub struct Auth {
    pub source: TokenSource,
    pub login: Option<String>,
    /// Сколько запросов осталось в этом часу.
    pub remaining: Option<u32>,
    pub error: Option<String>,
}

pub enum Cmd {
    /// Список проектов на GitHub изменился.
    Targets(Vec<Target>),
    /// Спросить сейчас, не дожидаясь срока.
    Refresh,
    /// Сохранить свой токен (`None` — забыть) и перечитать всё.
    SetToken(Option<String>),
}

pub enum Event {
    Remote(PathBuf, Box<Remote>),
    Auth(Auth),
    /// Чем кончилось сохранение токена: `Ok` или текст ошибки.
    TokenSaved(Result<(), String>),
}

pub fn spawn(ctx: egui::Context) -> (Sender<Cmd>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("anvil-github".into())
        .spawn(move || Hub::new(ctx, event_tx).run(cmd_rx))
        .expect("spawn github");
    (cmd_tx, event_rx)
}

struct Hub {
    ctx: egui::Context,
    events: Sender<Event>,
    http: Option<reqwest::blocking::Client>,
    token: Option<String>,
    source: TokenSource,
    targets: Vec<Target>,
}

impl Hub {
    fn new(ctx: egui::Context, events: Sender<Event>) -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("anvil/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .ok();
        Self { ctx, events, http, token: None, source: TokenSource::None, targets: Vec::new() }
    }

    fn send(&self, event: Event) {
        let _ = self.events.send(event);
        self.ctx.request_repaint();
    }

    fn run(mut self, commands: Receiver<Cmd>) {
        self.load_token();
        let mut last = Instant::now() - POLL_EVERY;
        loop {
            let wait = POLL_EVERY.saturating_sub(last.elapsed());
            match commands.recv_timeout(wait) {
                Ok(Cmd::Targets(targets)) => {
                    let new: Vec<Target> = targets.iter().filter(|t| !self.targets.contains(t)).cloned().collect();
                    self.targets = targets;
                    // Новые проекты — сразу, остальные — в свой срок.
                    for target in &new {
                        self.poll(target);
                    }
                    continue;
                }
                Ok(Cmd::Refresh) => {}
                Ok(Cmd::SetToken(token)) => {
                    let result = store_token(token.as_deref());
                    self.send(Event::TokenSaved(result));
                    self.load_token();
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            for target in self.targets.clone() {
                self.poll(&target);
            }
            last = Instant::now();
        }
    }

    fn load_token(&mut self) {
        (self.token, self.source) = find_token();
        let mut auth = Auth { source: self.source, login: None, remaining: None, error: None };
        if self.token.is_some() {
            match self.get::<User>("/user") {
                Ok((user, remaining)) => {
                    auth.login = Some(user.login);
                    auth.remaining = remaining;
                }
                Err(e) => auth.error = Some(e),
            }
        } else {
            auth.remaining = self.get::<RateLimit>("/rate_limit").ok().map(|(r, _)| r.rate.remaining);
        }
        self.send(Event::Auth(auth));
    }

    fn poll(&self, target: &Target) {
        let remote = self.remote(target);
        self.send(Event::Remote(target.path.clone(), Box::new(remote)));
    }

    fn remote(&self, target: &Target) -> Remote {
        let repo = format!("/repos/{}/{}", target.repo.owner, target.repo.name);
        let mut remote = Remote { checked: crate::i18n::now(), ..Remote::default() };
        let branch = target.branch.as_deref().map(|b| format!("&branch={}", encode(b))).unwrap_or_default();
        match self.get::<Runs>(&format!("{repo}/actions/runs?per_page=10{branch}")) {
            Ok((runs, _)) => remote.runs = runs.workflow_runs.into_iter().map(CiRun::from).collect(),
            Err(e) => remote.error = Some(e),
        }
        // У упавшего последнего прогона — какие задачи и шаги упали.
        if let Some(run) = remote.runs.first_mut()
            && run.state == RunState::Failure
            && let Ok((jobs, _)) = self.get::<Jobs>(&format!("{repo}/actions/runs/{}/jobs?per_page=50", run.id))
        {
            run.failed_jobs = jobs
                .jobs
                .into_iter()
                .filter(|j| j.conclusion.as_deref() == Some("failure"))
                .map(|j| FailedJob {
                    name: j.name,
                    url: j.html_url,
                    steps: j
                        .steps
                        .into_iter()
                        .filter(|s| s.conclusion.as_deref() == Some("failure"))
                        .map(|s| s.name)
                        .collect(),
                })
                .collect();
        }
        match self.get::<Vec<ApiRelease>>(&format!("{repo}/releases?per_page=10")) {
            Ok((releases, _)) => remote.releases = releases.into_iter().map(Release::from).collect(),
            Err(e) if remote.error.is_none() => remote.error = Some(e),
            Err(_) => {}
        }
        remote
    }

    /// GET к API. Вторым — сколько запросов осталось в этом часу.
    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<(T, Option<u32>), String> {
        let http = self.http.as_ref().ok_or("HTTP client is unavailable")?;
        let mut request = http
            .get(format!("{API}{path}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|e| e.without_url().to_string())?;
        let status = response.status();
        let remaining =
            response.headers().get("x-ratelimit-remaining").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
        if status.is_success() {
            return response.json::<T>().map(|v| (v, remaining)).map_err(|e| e.without_url().to_string());
        }
        Err(match status.as_u16() {
            401 => "401: token rejected".to_owned(),
            403 | 429 if remaining == Some(0) => "rate limit".to_owned(),
            404 => "404: not visible (private repository without a token?)".to_owned(),
            code => format!("HTTP {code}"),
        })
    }
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ─── Токен ─────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn keyring_entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

#[cfg(windows)]
fn own_token() -> Option<String> {
    keyring_entry()?.get_password().ok().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

#[cfg(not(windows))]
fn own_token() -> Option<String> {
    let _ = (KEYRING_SERVICE, KEYRING_USER);
    None
}

fn find_token() -> (Option<String>, TokenSource) {
    if let Some(token) = own_token() {
        return (Some(token), TokenSource::Keyring);
    }
    if let Some(token) = git_credential() {
        return (Some(token), TokenSource::Git);
    }
    (None, TokenSource::None)
}

#[cfg(not(windows))]
fn store_token(_: Option<&str>) -> Result<(), String> {
    Err("own token storage is available on Windows only".into())
}

#[cfg(windows)]
fn store_token(token: Option<&str>) -> Result<(), String> {
    let entry = keyring_entry().ok_or("keyring is unavailable")?;
    match token.map(str::trim).filter(|t| !t.is_empty()) {
        Some(token) => entry.set_password(token).map_err(|e| e.to_string()),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}

/// Токен, который git хранит для github.com. Никаких окон входа: нет — значит нет.
fn git_credential() -> Option<String> {
    let mut child = run::command("git", Path::new("."))
        .args(["credential", "fill"])
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(b"protocol=https\nhost=github.com\n\n").ok()?;
    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("password="))
        .map(str::to_owned)
        .filter(|t| !t.is_empty())
}

// ─── Ответы API ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct User {
    login: String,
}

#[derive(Deserialize)]
struct RateLimit {
    rate: Rate,
}

#[derive(Deserialize)]
struct Rate {
    remaining: u32,
}

#[derive(Deserialize)]
struct Runs {
    workflow_runs: Vec<ApiRun>,
}

#[derive(Deserialize)]
struct ApiRun {
    id: u64,
    name: Option<String>,
    run_number: u64,
    status: Option<String>,
    conclusion: Option<String>,
    head_sha: String,
    display_title: Option<String>,
    html_url: String,
    updated_at: String,
}

impl From<ApiRun> for CiRun {
    fn from(r: ApiRun) -> Self {
        CiRun {
            id: r.id,
            workflow: r.name.unwrap_or_default(),
            number: r.run_number,
            state: run_state(r.status.as_deref(), r.conclusion.as_deref()),
            head_sha: r.head_sha,
            title: r.display_title.unwrap_or_default(),
            url: r.html_url,
            updated: parse_time(&r.updated_at).unwrap_or(0),
            failed_jobs: Vec::new(),
        }
    }
}

fn run_state(status: Option<&str>, conclusion: Option<&str>) -> RunState {
    match (status, conclusion) {
        (Some("completed"), Some("success")) => RunState::Success,
        (Some("completed"), Some("failure" | "timed_out" | "startup_failure")) => RunState::Failure,
        (Some("completed"), Some("cancelled")) => RunState::Cancelled,
        (Some("completed"), _) => RunState::Other,
        (Some("in_progress"), _) => RunState::Running,
        (Some(_), _) => RunState::Queued,
        (None, _) => RunState::Other,
    }
}

#[derive(Deserialize)]
struct Jobs {
    jobs: Vec<ApiJob>,
}

#[derive(Deserialize)]
struct ApiJob {
    name: String,
    conclusion: Option<String>,
    html_url: String,
    #[serde(default)]
    steps: Vec<ApiStep>,
}

#[derive(Deserialize)]
struct ApiStep {
    name: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: Option<String>,
    created_at: String,
    prerelease: bool,
    draft: bool,
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    size: u64,
    download_count: u64,
    browser_download_url: String,
}

impl From<ApiRelease> for Release {
    fn from(r: ApiRelease) -> Self {
        Release {
            name: r.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            tag: r.tag_name,
            url: r.html_url,
            published: parse_time(r.published_at.as_deref().unwrap_or(&r.created_at)).unwrap_or(0),
            prerelease: r.prerelease,
            draft: r.draft,
            assets: r
                .assets
                .into_iter()
                .map(|a| Asset { name: a.name, size: a.size, downloads: a.download_count, url: a.browser_download_url })
                .collect(),
        }
    }
}

/// `2026-09-23T15:04:05Z` → секунды UNIX.
pub fn parse_time(text: &str) -> Option<i64> {
    let (date, time) = text.trim_end_matches('Z').split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|p| p.split('.').next().and_then(|p| p.parse::<i64>().ok()));
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next()??);
    // Дни от 1970-01-01 по григорианскому календарю (алгоритм Хиннанта).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// Цели для GitHub из проектов: у кого origin на GitHub.
pub fn targets(projects: &[crate::worker::Project]) -> Vec<Target> {
    projects
        .iter()
        .filter_map(|p| {
            let git = p.git()?;
            Some(Target { path: p.path.clone(), repo: Repo::from_url(&git.github()?)?, branch: git.branch.clone() })
        })
        .collect()
}

pub type Remotes = HashMap<PathBuf, Remote>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_to_unix() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-09-23T15:04:05Z"), Some(1_790_175_845));
        assert_eq!(parse_time("2000-02-29T12:00:00.123Z"), Some(951_825_600));
        assert_eq!(parse_time("вчера"), None);
    }

    #[test]
    fn run_states() {
        assert_eq!(run_state(Some("completed"), Some("success")), RunState::Success);
        assert_eq!(run_state(Some("completed"), Some("failure")), RunState::Failure);
        assert_eq!(run_state(Some("in_progress"), None), RunState::Running);
        assert_eq!(run_state(Some("queued"), None), RunState::Queued);
        assert_eq!(run_state(Some("completed"), Some("skipped")), RunState::Other);
    }

    #[test]
    fn runs_and_releases_parse() {
        let runs: Runs = serde_json::from_str(r#"{"total_count":1,"workflow_runs":[{"id":7,"name":"CI","run_number":12,
            "status":"completed","conclusion":"failure","event":"push","head_branch":"main","head_sha":"abc",
            "display_title":"A1","html_url":"https://github.com/o/r/actions/runs/7","updated_at":"2026-09-23T15:04:05Z"}]}"#)
        .unwrap();
        let run = CiRun::from(runs.workflow_runs.into_iter().next().unwrap());
        assert_eq!((run.number, run.state, run.updated), (12, RunState::Failure, 1_790_175_845));

        let releases: Vec<ApiRelease> = serde_json::from_str(
            r#"[{"tag_name":"v0.3.0","name":"","html_url":"u",
            "published_at":"2026-09-23T15:04:05Z","created_at":"2026-09-23T15:00:00Z","prerelease":false,"draft":false,
            "assets":[{"name":"amber-server","size":10485760,"download_count":3,"browser_download_url":"d"}]}]"#,
        )
        .unwrap();
        let release = Release::from(releases.into_iter().next().unwrap());
        assert_eq!(release.name, "v0.3.0");
        assert_eq!(release.assets[0].size, 10_485_760);
    }

    #[test]
    fn repo_from_url_and_encode() {
        let repo = Repo::from_url("https://github.com/AgitAngst/Anvil").unwrap();
        assert_eq!((repo.owner.as_str(), repo.name.as_str()), ("AgitAngst", "Anvil"));
        assert_eq!(
            encode("feature/новая ветка"),
            "feature%2F%D0%BD%D0%BE%D0%B2%D0%B0%D1%8F%20%D0%B2%D0%B5%D1%82%D0%BA%D0%B0"
        );
    }
}
