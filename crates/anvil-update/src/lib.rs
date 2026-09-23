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
    /// Начало имени архива: `amber-desktop` → `amber-desktop-0.3.1-windows-x64.zip`.
    pub app: String,
    /// Текущая версия — обычно `env!("CARGO_PKG_VERSION")`.
    pub version: String,
    /// Репозиторий с выпусками: `owner/name`. У закрытого кода — отдельный публичный `…-releases`.
    pub repo: String,
    /// Адрес API; меняется только в проверках.
    pub api: String,
}

impl Config {
    pub fn new(app: &str, version: &str, repo: &str) -> Self {
        Self { app: app.into(), version: version.into(), repo: repo.into(), api: "https://api.github.com".into() }
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
            inner: Arc::new(Mutex::new(Inner { state: State::Idle, dismissed: false, last_check: None })),
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
        if !response.status().is_success() {
            return Err(format!("GitHub: HTTP {}", response.status().as_u16()));
        }
        let list = response.json().map_err(|e| e.without_url().to_string())?;
        Ok(releases::choose(list, &self.config.app, &current, prerelease))
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
        match self.state() {
            State::Ready { exe, .. } => install::restart(&exe),
            _ => Err("nothing to restart into".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
