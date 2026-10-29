//! SSH до машины: вход, команды, заливка файла.
//!
//! **Ключ машины.** При первом входе центр не знает, кому верить: отпечаток ключа SSH-сервера запоминается, только
//! когда человек его подтвердил ([`probe`] ничего о себе не говорит — ни логина, ни пароля). Дальше вход идёт лишь к
//! машине с тем же ключом: проверка ключа идёт раньше входа, и подменившему машину по пути пароль не достанется.
//! Сменился ключ — [`ConnectError::KeyChanged`] с прежним и новым отпечатком, ни один секрет не отправлен.
//!
//! **Права.** Команды контракта требуют root. Вошли не под root — они идут через `sudo`: с паролем, если входили
//! по паролю (он же пароль `sudo`), иначе `sudo -n`, и если он спросит пароль — честная ошибка.
//!
//! **ssh-agent.** Закрытый ключ может и не покидать агента: центр спрашивает у него открытые ключи и по очереди
//! предлагает их машине, а подписывает агент. В Windows это служба «OpenSSH Authentication Agent» (именованный
//! канал), а если её нет — Pageant.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use russh::ChannelMsg;
use russh::client;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};

use crate::manifest::Arch;
pub use crate::shell::quote;

/// Как входим. Пароли и ключевые фразы живут здесь только на время входа (хранятся — в [`crate::secrets`]).
pub enum Auth {
    Password(String),
    Key {
        path: PathBuf,
        passphrase: Option<String>,
    },
    /// Ключами из ssh-agent: пароль и файл ключа центру не нужны.
    Agent,
    /// Агент на другом конце трубы — чтобы проверить вход, не трогая системный.
    #[cfg(test)]
    TestAgent(tokio::io::DuplexStream),
}

/// Именованный канал службы OpenSSH Authentication Agent в Windows.
#[cfg(windows)]
const OPENSSH_AGENT: &str = r"\\.\pipe\openssh-ssh-agent";

/// Сколько ждать ответа машины при соединении.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Куда входим.
pub struct Target<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    pub auth: Auth,
    /// Отпечаток ключа машины, которому поверили. Без него не входим: сначала [`probe`].
    pub trusted: &'a str,
}

/// Что пошло не так при входе — чтобы сказать человеку точно.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("cannot reach {0}: {1}")]
    Unreachable(String, String),
    /// Ключ машины не тот, которому поверили. Так выглядит и переустановка системы, и подмена по пути.
    #[error("the machine key changed: was {expected}, now {got}")]
    KeyChanged { expected: String, got: String },
    #[error("the server did not accept the login (password or key)")]
    Refused,
    #[error("key problem: {0}")]
    BadKey(String),
    #[error("{0}")]
    Other(String),
}

/// Проверка ключа машины: сверяет с доверенным и запоминает увиденный.
struct Checker {
    trusted: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Checker {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let data = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.key_data().clone(),
            PublicKeyOrCertificate::Certificate(cert) => cert.public_key().clone(),
        };
        let fingerprint = data.fingerprint(HashAlg::Sha256).to_string();
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = Some(fingerprint.clone());
        // Без доверенного ключа — это разведка [`probe`]: входить она не будет.
        Ok(self.trusted.as_ref().is_none_or(|trusted| *trusted == fingerprint))
    }
}

/// Открытое соединение.
pub struct Conn {
    handle: client::Handle<Checker>,
    root: bool,
    /// Пароль для `sudo -S`, если входили по паролю не под root.
    sudo: Option<String>,
    /// Разрешено ли sudo без пароля (правило `NOPASSWD`) — узнаётся один раз ([`Conn::sudo_prefix`]): тогда пароль
    /// не шлём вовсе, иначе sudo его не спросит, и он уйдёт на вход самой команде (`cat` от root напечатал бы его).
    nopasswd: tokio::sync::OnceCell<bool>,
}

/// Что вышло из команды.
#[derive(Debug, Default, Clone)]
pub struct Output {
    pub code: u32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    /// Причина провала для человека: последняя содержательная строка ошибок.
    pub fn failure(&self) -> String {
        let text = if self.stderr.trim().is_empty() { &self.stdout } else { &self.stderr };
        let line = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        let line = line.strip_prefix("Error: ").unwrap_or(line);
        if line.is_empty() { format!("the command exited with code {}", self.code) } else { line.to_owned() }
    }
}

/// Узнать ключ машины, ничего о себе не сказав: ни логина, ни пароля. Его показывают человеку, и только если он
/// подтвердит — [`connect`].
pub async fn probe(host: &str, port: u16) -> Result<String, ConnectError> {
    let seen = Arc::new(Mutex::new(None));
    let checker = Checker { trusted: None, seen: seen.clone() };
    let address = format!("{host}:{port}");
    let config = Arc::new(client::Config::default());
    let connecting = client::connect(config, (host, port), checker);
    match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Err(_) => Err(ConnectError::Unreachable(address, "no answer in 15 seconds".to_owned())),
        Ok(Err(e)) => Err(ConnectError::Unreachable(address, e.to_string())),
        Ok(Ok(handle)) => {
            let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "en").await;
            seen.lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .ok_or_else(|| ConnectError::Other("the machine did not show a key".to_owned()))
        }
    }
}

/// Войти на машину с уже доверенным ключом.
pub async fn connect(target: Target<'_>) -> Result<Conn, ConnectError> {
    let seen = Arc::new(Mutex::new(None));
    let checker = Checker { trusted: Some(target.trusted.to_owned()), seen: seen.clone() };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(120)),
        keepalive_interval: Some(Duration::from_secs(20)),
        ..Default::default()
    });
    let address = format!("{}:{}", target.host, target.port);
    tracing::debug!("ssh: connecting to {address} as {}", target.user);
    let connecting = client::connect(config, (target.host, target.port), checker);
    let mut handle = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Err(_) => return Err(ConnectError::Unreachable(address, "no answer in 15 seconds".to_owned())),
        Ok(Ok(handle)) => handle,
        Ok(Err(e)) => {
            let got = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(got) = got
                && got != target.trusted
            {
                tracing::warn!("ssh: the key of {address} changed");
                return Err(ConnectError::KeyChanged { expected: target.trusted.to_owned(), got });
            }
            return Err(ConnectError::Unreachable(address, e.to_string()));
        }
    };
    let (accepted, sudo) = match target.auth {
        Auth::Password(password) => {
            let result = handle
                .authenticate_password(target.user, password.clone())
                .await
                .map_err(|e| ConnectError::Other(e.to_string()))?;
            (result.success(), Some(password))
        }
        Auth::Key { path, passphrase } => {
            let key = load_key(&path, passphrase.as_deref())?;
            let hash = handle.best_supported_rsa_hash().await.map_err(|e| ConnectError::Other(e.to_string()))?;
            let result = handle
                .authenticate_publickey(target.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash.flatten()))
                .await
                .map_err(|e| ConnectError::Other(e.to_string()))?;
            (result.success(), None)
        }
        Auth::Agent => (system_agent(&mut handle, target.user).await?, None),
        #[cfg(test)]
        Auth::TestAgent(stream) => (with_agent(&mut handle, target.user, AgentClient::connect(stream)).await?, None),
    };
    if !accepted {
        return Err(ConnectError::Refused);
    }
    let root = target.user == "root";
    Ok(Conn { handle, root, sudo: if root { None } else { sudo }, nopasswd: tokio::sync::OnceCell::new() })
}

/// Войти ключами системного ssh-agent: службы OpenSSH, а нет её — Pageant.
async fn system_agent(handle: &mut client::Handle<Checker>, user: &str) -> Result<bool, ConnectError> {
    let silent = || {
        ConnectError::BadKey(
            "ssh-agent does not answer: start the OpenSSH Authentication Agent service or Pageant".to_owned(),
        )
    };
    #[cfg(windows)]
    {
        if let Ok(agent) = AgentClient::connect_named_pipe(OPENSSH_AGENT).await {
            return with_agent(handle, user, agent).await;
        }
        let agent = AgentClient::connect_pageant().await.map_err(|_| silent())?;
        with_agent(handle, user, agent).await
    }
    #[cfg(not(windows))]
    {
        let agent = AgentClient::connect_env().await.map_err(|_| silent())?;
        with_agent(handle, user, agent).await
    }
}

/// Предложить машине ключи агента по очереди. `false` — ни один не подошёл.
async fn with_agent<S: AgentStream + Unpin + Send>(
    handle: &mut client::Handle<Checker>,
    user: &str,
    mut agent: AgentClient<S>,
) -> Result<bool, ConnectError> {
    let identities =
        agent.request_identities().await.map_err(|e| ConnectError::BadKey(format!("ssh-agent gave no keys: {e}")))?;
    if identities.is_empty() {
        return Err(ConnectError::BadKey("ssh-agent holds no keys: add yours with ssh-add".to_owned()));
    }
    let hash = handle.best_supported_rsa_hash().await.map_err(|e| ConnectError::Other(e.to_string()))?.flatten();
    for identity in identities {
        let key = identity.public_key().into_owned();
        let result = handle
            .authenticate_publickey_with(user, key, hash, &mut agent)
            .await
            .map_err(|e| ConnectError::Other(e.to_string()))?;
        if result.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn load_key(path: &Path, passphrase: Option<&str>) -> Result<russh::keys::PrivateKey, ConnectError> {
    russh::keys::load_secret_key(path, passphrase).map_err(|e| match e {
        russh::keys::Error::KeyIsEncrypted => {
            ConnectError::BadKey("the key is encrypted: a passphrase is needed".to_owned())
        }
        e => ConnectError::BadKey(e.to_string()),
    })
}

impl Conn {
    /// Выполнить команду от своего имени. `stdin` — что подать на вход, `line` — каждая строка вывода, пока
    /// команда идёт (журнал работы прогоняет её через [`crate::redact::mask`]).
    pub async fn run(&self, command: &str, stdin: Option<&[u8]>, mut line: impl FnMut(&str)) -> Result<Output> {
        let mut channel = self.handle.channel_open_session().await.context("cannot open an SSH channel")?;
        channel.exec(true, command).await?;
        if let Some(input) = stdin {
            channel.data_bytes(input.to_vec()).await?;
        }
        channel.eof().await?;
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut pending = Vec::new();
        let mut code = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    out.extend_from_slice(&data);
                    pending.extend_from_slice(&data);
                    flush_lines(&mut pending, &mut line);
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    err.extend_from_slice(&data);
                    pending.extend_from_slice(&data);
                    flush_lines(&mut pending, &mut line);
                }
                ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
                _ => {}
            }
        }
        if !pending.is_empty() {
            line(String::from_utf8_lossy(&pending).trim_end());
        }
        Ok(Output {
            code: code.unwrap_or(255),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    }

    /// Как звать sudo и что дать ему на вход: с паролем — `sudo -S -k`, чтобы sudo читал пароль всегда сам, даже
    /// когда помнит прежний ввод (иначе пароль ушёл бы на вход команде); без пароля по правилу `NOPASSWD` —
    /// `sudo -n` и ничего на вход.
    async fn sudo_prefix(&self) -> (&'static str, Option<String>) {
        let Some(password) = &self.sudo else { return ("sudo -n", None) };
        let nopasswd = *self
            .nopasswd
            .get_or_init(|| async { self.run("sudo -k -n true", None, |_| {}).await.is_ok_and(|o| o.ok()) })
            .await;
        if nopasswd { ("sudo -n", None) } else { ("sudo -S -k -p ''", Some(format!("{password}\n"))) }
    }

    /// Выполнить команду от root: сразу, через `sudo -S` с паролем входа или через `sudo -n`.
    pub async fn root(&self, command: &str, line: impl FnMut(&str)) -> Result<Output> {
        if self.root {
            return self.run(command, None, line).await;
        }
        let wrapped = format!("sh -c {}", quote(command));
        match self.sudo_prefix().await {
            (prefix, Some(input)) => {
                let output = self.run(&format!("{prefix} {wrapped}"), Some(input.as_bytes()), line).await?;
                if output.stderr.contains("incorrect password") || output.stderr.contains("Sorry, try again") {
                    bail!("sudo did not accept the login password: this user has another sudo password");
                }
                Ok(output)
            }
            (prefix, None) => {
                let output = self.run(&format!("{prefix} {wrapped}"), None, line).await?;
                if output.stderr.contains("a password is required") {
                    bail!(
                        "sudo asks for a password, but the login was by key: sign in as root or allow sudo without a password"
                    );
                }
                Ok(output)
            }
        }
    }

    /// Выполнить команду от root и сложить её вывод в файл на этом компьютере — так забирают резервную копию
    /// архивом. `progress` — сколько байт пришло.
    pub async fn root_to_file(&self, command: &str, file: &Path, mut progress: impl FnMut(u64)) -> Result<Output> {
        use std::io::Write as _;
        let (command, input) = if self.root {
            (command.to_owned(), None)
        } else {
            let (prefix, input) = self.sudo_prefix().await;
            (format!("{prefix} sh -c {}", quote(command)), input)
        };
        let mut channel = self.handle.channel_open_session().await.context("cannot open an SSH channel")?;
        channel.exec(true, command).await?;
        if let Some(input) = input {
            channel.data_bytes(input.into_bytes()).await?;
        }
        channel.eof().await?;
        let mut out = std::io::BufWriter::new(std::fs::File::create(file).with_context(|| format!("{file:?}"))?);
        let (mut got, mut err, mut code) = (0u64, Vec::new(), None);
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    out.write_all(&data)?;
                    got += data.len() as u64;
                    progress(got);
                }
                ChannelMsg::ExtendedData { data, .. } => err.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
                _ => {}
            }
        }
        out.flush()?;
        Ok(Output {
            code: code.unwrap_or(255),
            stdout: String::new(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    }

    /// Залить файл по пути (от своего имени). `progress` — сколько байт ушло из скольких. Файл ложится только для
    /// себя (`umask 077`, 0600): в общем каталоге его иначе прочёл бы любой на машине. Кому файл нужен открытым
    /// (бинарник), тот задаёт права сам следующей командой.
    pub async fn upload(&self, bytes: &[u8], path: &str, mut progress: impl FnMut(u64, u64)) -> Result<()> {
        let mut channel = self.handle.channel_open_session().await.context("cannot open an SSH channel")?;
        let command = format!("umask 077 && cat > {}", quote(path));
        channel.exec(true, command).await?;
        let total = bytes.len() as u64;
        let mut sent = 0u64;
        for chunk in bytes.chunks(256 * 1024) {
            channel.data_bytes(chunk.to_vec()).await?;
            sent += chunk.len() as u64;
            progress(sent, total);
        }
        channel.eof().await?;
        let mut code = None;
        while let Some(message) = channel.wait().await {
            if let ChannelMsg::ExitStatus { exit_status } = message {
                code = Some(exit_status);
            }
        }
        match code {
            Some(0) => Ok(()),
            other => Err(anyhow!("the file was not written on the server (exit code {other:?})")),
        }
    }

    /// Архитектура машины по `uname -m`; `None` — такой выпуска не бывает.
    pub async fn arch(&self) -> Result<Option<Arch>> {
        let output = self.run("uname -m", None, |_| {}).await?;
        if !output.ok() {
            bail!("uname -m failed: {}", output.failure());
        }
        Ok(Arch::parse(&output.stdout))
    }

    pub async fn close(self) {
        let _ = self.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await;
    }
}

/// Отдать строки вывода по мере прихода.
fn flush_lines(pending: &mut Vec<u8>, line: &mut impl FnMut(&str)) {
    while let Some(end) = pending.iter().position(|b| *b == b'\n') {
        let text = String::from_utf8_lossy(&pending[..end]).trim_end_matches('\r').to_owned();
        line(&text);
        pending.drain(..=end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_failure_names_the_last_meaningful_line() {
        let output = Output { code: 1, stdout: String::new(), stderr: "warn\nError: name «Nik» is taken\n\n".into() };
        assert_eq!(output.failure(), "name «Nik» is taken");
        let silent = Output { code: 3, ..Default::default() };
        assert!(silent.failure().contains('3'));
    }

    #[test]
    fn lines_come_out_whole_as_they_arrive() {
        let mut got = Vec::new();
        let mut pending = b"one\r\ntw".to_vec();
        flush_lines(&mut pending, &mut |l: &str| got.push(l.to_owned()));
        pending.extend_from_slice(b"o\n");
        flush_lines(&mut pending, &mut |l: &str| got.push(l.to_owned()));
        assert_eq!(got, ["one", "two"]);
        assert!(pending.is_empty());
    }
}

/// Настоящий SSH-сервер в процессе теста: входит `root` и `anya` с паролем (и `root` по ключу из списка),
/// команды понимает несколько — `echo`, `uname -m`, `umask 077 && cat > путь` (заливка — только с закрытыми
/// правами), `sudo …` и `fail`.
#[cfg(test)]
mod live {
    use super::*;
    use russh::server::{self, Msg, Server as _, Session};
    use russh::{Channel, ChannelId};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    type Files = Arc<Mutex<HashMap<String, Vec<u8>>>>;

    const PASSWORD: &str = "секрет";

    #[derive(Clone, Default)]
    struct Fake {
        files: Files,
        /// Сколько раз клиент пытался войти (пароль или ключ).
        attempts: Arc<AtomicUsize>,
        commands: HashMap<ChannelId, String>,
        input: HashMap<ChannelId, Vec<u8>>,
        /// Чей ключ пускать без пароля.
        allowed: Option<russh::keys::PublicKey>,
        /// У пользователя `anya` sudo без пароля (правило `NOPASSWD`).
        nopasswd: bool,
    }

    impl server::Server for Fake {
        type Handler = Self;
        fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
            Self {
                files: self.files.clone(),
                attempts: self.attempts.clone(),
                allowed: self.allowed.clone(),
                nopasswd: self.nopasswd,
                ..Default::default()
            }
        }
    }

    impl server::Handler for Fake {
        type Error = russh::Error;

        async fn auth_password(&mut self, user: &str, password: &str) -> Result<server::Auth, Self::Error> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Ok(if (user == "root" || user == "anya") && password == PASSWORD {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            })
        }

        async fn auth_publickey(
            &mut self,
            user: &str,
            key: &russh::keys::PublicKey,
        ) -> Result<server::Auth, Self::Error> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let known = self.allowed.as_ref().is_some_and(|allowed| allowed.key_data() == key.key_data());
            Ok(if user == "root" && known { server::Auth::Accept } else { server::Auth::reject() })
        }

        async fn channel_open_session(
            &mut self,
            _channel: Channel<Msg>,
            reply: server::ChannelOpenHandle,
            _session: &mut Session,
        ) -> Result<(), Self::Error> {
            reply.accept().await;
            Ok(())
        }

        async fn exec_request(
            &mut self,
            channel: ChannelId,
            data: &[u8],
            session: &mut Session,
        ) -> Result<(), Self::Error> {
            self.commands.insert(channel, String::from_utf8_lossy(data).into_owned());
            session.channel_success(channel)?;
            Ok(())
        }

        async fn data(&mut self, channel: ChannelId, data: &[u8], _session: &mut Session) -> Result<(), Self::Error> {
            self.input.entry(channel).or_default().extend_from_slice(data);
            Ok(())
        }

        async fn channel_eof(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
            let command = self.commands.remove(&channel).unwrap_or_default();
            let input = self.input.remove(&channel).unwrap_or_default();
            let code = if let Some(text) = command.strip_prefix("echo ") {
                session.data(channel, format!("{text}\nвторая строка\n").into_bytes())?;
                0
            } else if command == "uname -m" {
                session.data(channel, b"aarch64\n".to_vec())?;
                0
            } else if let Some(path) = command.strip_prefix("umask 077 && cat > ") {
                self.files.lock().unwrap().insert(path.trim_matches('\'').to_owned(), input);
                0
            } else if command == "sudo -k -n true" {
                if self.nopasswd { 0 } else { 1 }
            } else if let Some(rest) = command.strip_prefix("sudo ") {
                // Как настоящий sudo: с `-S` и без `NOPASSWD` первая строка входа — его пароль, остальное —
                // команде; с `NOPASSWD` пароль он не спрашивает, и весь вход достаётся команде. `cat` от root
                // печатает, что ему досталось.
                let reads_password = rest.starts_with("-S") && !self.nopasswd;
                let given = if reads_password {
                    input.splitn(2, |b| *b == b'\n').nth(1).unwrap_or_default().to_vec()
                } else {
                    input
                };
                if rest.ends_with("sh -c 'cat'") {
                    session.data(channel, given)?;
                }
                0
            } else if command == "fail" {
                session.extended_data(channel, 1, b"warn\nError: all bad\n".to_vec())?;
                3
            } else {
                127
            };
            session.exit_status_request(channel, code)?;
            session.eof(channel)?;
            session.close(channel)?;
            Ok(())
        }
    }

    struct Running {
        port: u16,
        files: Files,
        attempts: Arc<AtomicUsize>,
    }

    fn random_key() -> russh::keys::PrivateKey {
        russh::keys::PrivateKey::random(&mut russh::keys::key::safe_rng(), russh::keys::Algorithm::Ed25519).unwrap()
    }

    async fn start_with(mut fake: Fake) -> Running {
        let config = Arc::new(server::Config {
            keys: vec![random_key()],
            auth_rejection_time: Duration::from_millis(10),
            auth_rejection_time_initial: Some(Duration::from_millis(0)),
            ..Default::default()
        });
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = socket.local_addr().unwrap().port();
        let (files, attempts) = (fake.files.clone(), fake.attempts.clone());
        tokio::spawn(async move {
            let _ = fake.run_on_socket(config, &socket).await;
        });
        Running { port, files, attempts }
    }

    async fn start() -> Running {
        start_with(Fake::default()).await
    }

    fn target<'a>(port: u16, password: &str, trusted: &'a str) -> Target<'a> {
        Target { host: "127.0.0.1", port, user: "root", auth: Auth::Password(password.to_owned()), trusted }
    }

    #[tokio::test]
    async fn the_center_learns_the_key_signs_in_runs_and_uploads() {
        let server = start().await;
        let fingerprint = probe("127.0.0.1", server.port).await.unwrap();
        assert!(fingerprint.starts_with("SHA256:"), "{fingerprint}");
        assert_eq!(server.attempts.load(Ordering::SeqCst), 0, "the probe sends no credentials");

        let conn = connect(target(server.port, PASSWORD, &fingerprint)).await.unwrap();
        let mut lines = Vec::new();
        let output = conn.run("echo привет", None, |l| lines.push(l.to_owned())).await.unwrap();
        assert!(output.ok());
        assert_eq!(lines, ["привет", "вторая строка"], "rows come one by one");

        let failed = conn.run("fail", None, |_| {}).await.unwrap();
        assert_eq!(failed.code, 3);
        assert_eq!(failed.failure(), "all bad");

        assert_eq!(conn.arch().await.unwrap(), Some(Arch::Arm64));

        let bytes: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect();
        let mut seen = 0;
        conn.upload(&bytes, "/tmp/example-server-1", |done, _| seen = done).await.unwrap();
        assert_eq!(seen, bytes.len() as u64);
        assert_eq!(server.files.lock().unwrap().get("/tmp/example-server-1"), Some(&bytes), "the file arrived whole");
        conn.close().await;
    }

    #[tokio::test]
    async fn a_wrong_password_is_refused() {
        let server = start().await;
        let fingerprint = probe("127.0.0.1", server.port).await.unwrap();
        assert!(matches!(connect(target(server.port, "не тот", &fingerprint)).await, Err(ConnectError::Refused)));
    }

    /// Ключ машины не тот, которому поверили: отказ с двумя отпечатками, а пароль до машины не дошёл.
    #[tokio::test]
    async fn a_changed_key_is_refused_before_any_credentials_are_sent() {
        let server = start().await;
        let fingerprint = probe("127.0.0.1", server.port).await.unwrap();
        let stranger = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        match connect(target(server.port, PASSWORD, stranger)).await {
            Err(ConnectError::KeyChanged { expected, got }) => {
                assert_eq!(expected, stranger);
                assert_eq!(got, fingerprint);
                let text = ConnectError::KeyChanged { expected, got }.to_string();
                assert!(text.contains(stranger) && text.contains(&fingerprint), "{text}");
            }
            other => panic!("expected a key change, got {:?}", other.map(|_| ())),
        }
        assert_eq!(server.attempts.load(Ordering::SeqCst), 0, "no password was sent to a machine with another key");
    }

    /// Пароль входа не попадает на вход команде от root: ни когда sudo без пароля (`NOPASSWD` — тогда его не
    /// шлём), ни когда sudo его просит.
    #[tokio::test]
    async fn the_login_password_never_reaches_a_root_command() {
        for nopasswd in [true, false] {
            let server = start_with(Fake { nopasswd, ..Default::default() }).await;
            let fingerprint = probe("127.0.0.1", server.port).await.unwrap();
            let target = Target {
                host: "127.0.0.1",
                port: server.port,
                user: "anya",
                auth: Auth::Password(PASSWORD.into()),
                trusted: &fingerprint,
            };
            let conn = connect(target).await.unwrap();
            let output = conn.root("cat", |_| {}).await.unwrap();
            assert!(
                !output.stdout.contains(PASSWORD),
                "nopasswd={nopasswd}: the password reached the command: {:?}",
                output.stdout
            );
            conn.close().await;
        }
    }

    /// Агент с этими ключами: одной трубой их кладут, как `ssh-add`, другой — входят.
    async fn agent_with(keys: &[&russh::keys::PrivateKey]) -> tokio::io::DuplexStream {
        let (adding, served_adding) = tokio::io::duplex(64 * 1024);
        let (login, served_login) = tokio::io::duplex(64 * 1024);
        let connections = futures::stream::iter([Ok(served_adding), Ok(served_login)]);
        tokio::spawn(russh::keys::agent::server::serve(connections, ()));
        let mut client = AgentClient::connect(adding);
        for key in keys {
            client.add_identity(key, &[]).await.unwrap();
        }
        login
    }

    #[tokio::test]
    async fn the_agent_signs_in_with_whichever_of_its_keys_fits() {
        let (stranger, mine) = (random_key(), random_key());
        let server = start_with(Fake { allowed: Some(mine.public_key().clone()), ..Default::default() }).await;
        let trusted = probe("127.0.0.1", server.port).await.unwrap();
        let signed_in = |agent| Target {
            host: "127.0.0.1",
            port: server.port,
            user: "root",
            auth: Auth::TestAgent(agent),
            trusted: &trusted,
        };

        // Первый ключ агента машине чужой — подходит второй.
        let conn = connect(signed_in(agent_with(&[&stranger, &mine]).await)).await.unwrap();
        assert!(conn.run("echo да", None, |_| {}).await.unwrap().ok());
        conn.close().await;

        assert!(matches!(connect(signed_in(agent_with(&[&stranger]).await)).await, Err(ConnectError::Refused)));
        match connect(signed_in(agent_with(&[]).await)).await {
            Err(ConnectError::BadKey(reason)) => assert!(reason.contains("ssh-add"), "{reason}"),
            other => panic!("expected an empty agent, got {:?}", other.map(|_| ())),
        }
    }

    #[tokio::test]
    async fn nobody_answering_is_unreachable() {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert!(matches!(probe("127.0.0.1", port).await, Err(ConnectError::Unreachable(..))));
    }
}
