//! Проверка и установка обновлений изнутри программы.
//!
//! ```ignore
//! // При запуске: убрать следы прошлого обновления и завести проверяльщика.
//! anvil_update::cleanup();
//! let updater = anvil_update::Updater::new(
//!     anvil_update::Config::new("tetrachrome", env!("CARGO_PKG_VERSION"), "AgitAngst/Tetrachrome"),
//!     cc.egui_ctx.clone(),
//! );
//!
//! // В каждом кадре: проверка при запуске и раз в сутки, если включено в настройках.
//! updater.auto(&settings.common);
//! anvil_update::ui::banner(ui, &updater, &mut settings.common);
//! ```
//!
//! Выпуски — по соглашению семьи (`docs/RELEASES.md`): тег `vX.Y.Z`, архив
//! `<app>-X.Y.Z-windows-x64.zip` и `SHA256SUMS` в GitHub Release. Без `SHA256SUMS` обновление не
//! ставится: непроверенный файл хуже, чем старая версия.

pub mod errors;
pub mod install;
mod releases;
mod version;

#[cfg(feature = "ui")]
pub mod lang;
#[cfg(feature = "ui")]
pub mod ui;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use install::{Layout, cleanup};
pub use releases::{Asset, Update, asset_name, expected_sum, platform};
pub use version::Version;

/// Как часто проверять, пока программа открыта.
#[cfg(feature = "ui")]
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Что за программа и где её выпуски.
#[derive(Debug, Clone)]
pub struct Config {
    /// Начало имени архива: `lantern-desktop` → `lantern-desktop-0.3.1-windows-x64.zip`.
    pub app: String,
    /// Текущая версия — обычно `env!("CARGO_PKG_VERSION")`.
    pub version: String,
    /// Репозиторий с выпусками: `owner/name`. У закрытого кода — отдельный публичный `…-releases`.
    pub repo: String,
    /// Адрес API; меняется только в проверках.
    pub api: String,
    /// Адрес сайта (запасной путь без API, когда лимит запросов исчерпан); меняется только в проверках.
    pub web: String,
}

impl Config {
    pub fn new(app: &str, version: &str, repo: &str) -> Self {
        Self {
            app: app.into(),
            version: version.into(),
            repo: repo.into(),
            api: "https://api.github.com".into(),
            web: "https://github.com".into(),
        }
    }
}

/// Где сейчас обновление.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// Ещё не проверяли.
    Idle,
    Checking,
    UpToDate,
    Available(Update),
    Downloading {
        update: Update,
        done: u64,
        total: u64,
    },
    /// Новая версия на месте — нужен перезапуск.
    Ready {
        update: Update,
        exe: std::path::PathBuf,
    },
    Failed {
        update: Option<Update>,
        error: String,
    },
}

struct Inner {
    state: State,
    /// Пользователь сказал «Позже» — баннер спрятан до следующей проверки.
    dismissed: bool,
    last_check: Option<Instant>,
    /// Новая версия запущена, эта закрывается: закрытие настоящее, не в трей.
    restarting: bool,
}

/// Проверяльщик обновлений. Дёшево клонируется; вся сеть — в фоновых потоках.
#[derive(Clone)]
pub struct Updater {
    config: Arc<Config>,
    inner: Arc<Mutex<Inner>>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Updater {
    /// `repaint` зовётся, когда состояние поменялось (с egui — `ctx.request_repaint()`).
    pub fn with_repaint(config: Config, repaint: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            config: Arc::new(config),
            inner: Arc::new(Mutex::new(Inner {
                state: State::Idle,
                dismissed: false,
                last_check: None,
                restarting: false,
            })),
            repaint: Arc::new(repaint),
        }
    }

    #[cfg(feature = "ui")]
    pub fn new(config: Config, ctx: eframe::egui::Context) -> Self {
        Self::with_repaint(config, move || ctx.request_repaint())
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn state(&self) -> State {
        self.lock().state.clone()
    }

    pub fn dismissed(&self) -> bool {
        self.lock().dismissed
    }

    /// Новая версия уже запущена: программа закрывается по-настоящему (не прячется в трей).
    pub fn restarting(&self) -> bool {
        self.lock().restarting
    }

    /// «Позже»: спрятать баннер до следующей проверки.
    pub fn dismiss(&self) {
        self.lock().dismissed = true;
        (self.repaint)();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set(&self, state: State) {
        {
            let mut inner = self.lock();
            inner.state = state;
            inner.dismissed = false;
        }
        (self.repaint)();
    }

    fn busy(&self) -> bool {
        matches!(self.lock().state, State::Checking | State::Downloading { .. } | State::Ready { .. })
    }

    /// Проверка при запуске и раз в сутки — если она включена. Звать в каждом кадре: дёшево.
    #[cfg(feature = "ui")]
    pub fn auto(&self, settings: &anvil_ui::CommonSettings) {
        if !settings.check_updates || self.busy() {
            return;
        }
        let due = self.lock().last_check.is_none_or(|t| t.elapsed() >= EVERY);
        if due {
            self.check(settings.prerelease, settings.skip_version.as_deref(), false);
        }
    }

    /// Спросить GitHub. `skip` — версия, которую пользователь пропустил: при авто-проверке она не
    /// предлагается; `manual` — проверка по кнопке, пропущенную версию показать всё равно.
    pub fn check(&self, prerelease: bool, skip: Option<&str>, manual: bool) {
        if self.busy() {
            return;
        }
        self.lock().last_check = Some(Instant::now());
        self.set(State::Checking);
        let this = self.clone();
        let skip = skip.map(str::to_owned);
        std::thread::spawn(move || {
            let state = match this.fetch(prerelease) {
                Ok(Some(update)) if !manual && skip.as_deref() == Some(update.version.to_string().as_str()) => {
                    State::UpToDate
                }
                Ok(Some(update)) => State::Available(update),
                Ok(None) => State::UpToDate,
                Err(error) => State::Failed { update: None, error },
            };
            this.set(state);
        });
    }

    fn http() -> Result<reqwest::blocking::Client, String> {
        reqwest::blocking::Client::builder()
            .user_agent(concat!("anvil-update/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())
    }

    fn fetch(&self, prerelease: bool) -> Result<Option<Update>, String> {
        let current = Version::parse(&self.config.version).ok_or("bad current version")?;
        let url = format!("{}/repos/{}/releases?per_page=20", self.config.api, self.config.repo);
        let response = Self::http()?
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .map_err(|e| e.without_url().to_string())?;
        let code = response.status().as_u16();
        if code == 403 || code == 429 {
            // Анонимному запросу к API — 60 в час на адрес: лимит съедают соседи по сети, VPN, другие программы.
            // Последний выпуск можно узнать и без API, поэтому сначала пробуем так, а ошибка — только если не вышло.
            let minutes = limit_minutes(response.headers(), unix_now());
            return self.fetch_without_api(&current).map_err(|_| match minutes {
                Some(m) => errors::rate_limit(code, m),
                None => errors::http_status(code),
            });
        }
        if !response.status().is_success() {
            return Err(errors::http_status(code));
        }
        let list = response.json().map_err(|e| e.without_url().to_string())?;
        Ok(releases::choose(list, &self.config.app, &current, prerelease))
    }

    /// Последний выпуск без API: `<сайт>/<репозиторий>/releases/latest` отвечает переадресацией на страницу
    /// тега, а архив и `SHA256SUMS` лежат по известным адресам. Видит только последний *обычный* выпуск
    /// (пред-выпуски GitHub в `latest` не кладёт) и не знает заметок к выпуску.
    fn fetch_without_api(&self, current: &Version) -> Result<Option<Update>, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("anvil-update/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        let url = format!("{}/{}/releases/latest", self.config.web, self.config.repo);
        let response = client.get(url).send().map_err(|e| e.without_url().to_string())?;
        if !response.status().is_redirection() {
            return Err(errors::http_status(response.status().as_u16()));
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or("no Location in the redirect")?;
        let tag = latest_tag(location).ok_or("no release tag in the redirect")?;
        let version = Version::parse(tag).ok_or("bad release tag")?;
        if version <= *current {
            return Ok(None);
        }
        Ok(Some(releases::from_tag(&self.config.app, &self.config.web, &self.config.repo, tag, version)))
    }

    /// Скачать и поставить доступное обновление (в фоне).
    pub fn install(&self) {
        let update = match self.state() {
            State::Available(update) | State::Failed { update: Some(update), .. } => update,
            _ => return,
        };
        let (Some(asset), Some(sums)) = (update.asset.clone(), update.sums.clone()) else {
            self.set(State::Failed { update: Some(update), error: "no archive or SHA256SUMS in the release".into() });
            return;
        };
        self.set(State::Downloading { update: update.clone(), done: 0, total: asset.size });
        let this = self.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let http = Self::http()?;
                let sums_text = install::fetch_text(&http, install::Source::public(&sums.url))?;
                let expected = releases::expected_sum(&sums_text, &asset.name)
                    .ok_or_else(|| format!("{} is not listed in SHA256SUMS", asset.name))?;
                let layout = Layout::detect()?;
                let mut last = Instant::now();
                let progress = |done: u64, total: u64| {
                    // Не чаще десяти раз в секунду: иначе окно перерисовывается впустую.
                    if last.elapsed() >= Duration::from_millis(100) || done == total {
                        last = Instant::now();
                        this.lock().state =
                            State::Downloading { update: update.clone(), done, total: total.max(asset.size) };
                        (this.repaint)();
                    }
                };
                let version = update.version.to_string();
                install::install(&http, &layout, &asset.url, &asset.name, &expected, &version, progress)
            })();
            this.set(match result {
                Ok(exe) => State::Ready { update, exe },
                Err(error) => State::Failed { update: Some(update), error },
            });
        });
    }

    /// Запустить новую версию. Окно текущей программа закрывает сама, сразу после этого вызова.
    pub fn restart(&self) -> Result<(), String> {
        let result = match self.state() {
            State::Ready { exe, .. } => install::restart(&exe),
            _ => Err("nothing to restart into".into()),
        };
        if result.is_ok() {
            self.lock().restarting = true;
        }
        result
    }
}

/// Тег из адреса переадресации `…/releases/tag/<тег>`.
fn latest_tag(location: &str) -> Option<&str> {
    let tag = location.rsplit_once("/releases/tag/")?.1;
    let tag = tag.split(['?', '#']).next()?.trim_end_matches('/');
    (!tag.is_empty()).then_some(tag)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Через сколько минут снимется лимит по заголовкам ответа GitHub; `None` — это не лимит.
fn limit_minutes(headers: &reqwest::header::HeaderMap, now: u64) -> Option<u64> {
    let number = |name: &str| headers.get(name)?.to_str().ok()?.trim().parse::<u64>().ok();
    let seconds = if let Some(retry) = number("retry-after") {
        retry
    } else if number("x-ratelimit-remaining") == Some(0) {
        number("x-ratelimit-reset")?.saturating_sub(now)
    } else {
        return None;
    };
    Some(seconds.div_ceil(60).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn tag_from_redirect() {
        assert_eq!(latest_tag("https://github.com/o/r/releases/tag/v1.3.5"), Some("v1.3.5"));
        assert_eq!(latest_tag("/o/r/releases/tag/v1.3.5/"), Some("v1.3.5"));
        assert_eq!(latest_tag("https://github.com/o/r/releases/tag/v2.0.0?x=1"), Some("v2.0.0"));
        assert_eq!(latest_tag("https://github.com/o/r/releases"), None);
        assert_eq!(latest_tag("https://github.com/o/r/releases/tag/"), None);
    }

    #[test]
    fn limit_minutes_from_headers() {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut h = HeaderMap::new();
        assert_eq!(limit_minutes(&h, 1000), None);
        h.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        h.insert("x-ratelimit-reset", HeaderValue::from_static("1000"));
        assert_eq!(limit_minutes(&h, 1000), Some(1));
        h.insert("x-ratelimit-reset", HeaderValue::from_static("2400"));
        assert_eq!(limit_minutes(&h, 1000), Some(24));
        h.insert("x-ratelimit-remaining", HeaderValue::from_static("5"));
        assert_eq!(limit_minutes(&h, 1000), None);
        h.insert("retry-after", HeaderValue::from_static("90"));
        assert_eq!(limit_minutes(&h, 1000), Some(2));
    }

    /// Крошечный HTTP-сервер: на каждый запрос отвечает по `route(путь)` и закрывает соединение.
    fn serve(route: impl Fn(&str, &str) -> String + Send + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let host = base.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
                let _ = stream.write_all(route(&path, &host).as_bytes());
            }
        });
        base
    }

    fn updater(base: &str, version: &str) -> Updater {
        let mut config = Config::new("kamerton", version, "o/r");
        config.api = base.to_owned();
        config.web = base.to_owned();
        Updater::with_repaint(config, || {})
    }

    #[test]
    fn rate_limited_api_falls_back_to_the_site() {
        let base = serve(|path, host| {
            if path.starts_with("/repos/") {
                "HTTP/1.1 403 Forbidden\r\nx-ratelimit-remaining: 0\r\nx-ratelimit-reset: 99999999999\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into()
            } else if path == "/o/r/releases/latest" {
                format!(
                    "HTTP/1.1 302 Found\r\nlocation: {host}/o/r/releases/tag/v1.4.0\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                )
            } else {
                "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into()
            }
        });
        let update = updater(&base, "1.3.0").fetch(false).unwrap().expect("a newer release");
        assert_eq!(update.tag, "v1.4.0");
        assert_eq!(update.version.to_string(), "1.4.0");
        let asset = update.asset.unwrap();
        assert_eq!(asset.name, asset_name("kamerton", &update.version));
        assert_eq!(asset.url, format!("{base}/o/r/releases/download/v1.4.0/{}", asset.name));
        assert_eq!(update.sums.unwrap().url, format!("{base}/o/r/releases/download/v1.4.0/SHA256SUMS"));
        // Версия уже стоит — обновления нет.
        assert!(updater(&base, "1.4.0").fetch(false).unwrap().is_none());
    }

    #[test]
    fn rate_limit_without_a_fallback_gives_a_clear_error() {
        let base = serve(|path, _| {
            if path.starts_with("/repos/") {
                "HTTP/1.1 403 Forbidden\r\nx-ratelimit-remaining: 0\r\nx-ratelimit-reset: 99999999999\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into()
            } else {
                "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into()
            }
        });
        let error = updater(&base, "1.3.0").fetch(false).unwrap_err();
        assert!(error.starts_with("GitHub: HTTP 403 (rate limit), retry in "), "{error}");
        // Не лимит (403 без заголовков) — просто код.
        let base = serve(|_, _| "HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into());
        assert_eq!(updater(&base, "1.3.0").fetch(false).unwrap_err(), "GitHub: HTTP 403");
    }

    /// По сети: запасной путь на настоящем GitHub (`cargo test -p anvil-update -- --ignored`).
    #[test]
    #[ignore = "needs the network"]
    fn fallback_on_the_real_site() {
        let updater = Updater::with_repaint(Config::new("kamerton", "1.0.0", "AgitAngst/Kamerton"), || {});
        let update = updater.fetch_without_api(&Version::parse("1.0.0").unwrap()).unwrap().expect("newer release");
        eprintln!("latest: {} {}", update.tag, update.asset.unwrap().url);
        assert!(update.version > Version::parse("1.3.0").unwrap());
    }

    #[test]
    fn busy_states_block_new_checks() {
        let updater = Updater::with_repaint(Config::new("a", "0.1.0", "o/r"), || {});
        assert!(!updater.busy());
        updater.set(State::Checking);
        assert!(updater.busy());
        updater.set(State::UpToDate);
        assert!(!updater.busy());
    }

    #[test]
    fn dismiss_resets_on_new_state() {
        let updater = Updater::with_repaint(Config::new("a", "0.1.0", "o/r"), || {});
        updater.dismiss();
        assert!(updater.dismissed());
        updater.set(State::UpToDate);
        assert!(!updater.dismissed());
    }
}
