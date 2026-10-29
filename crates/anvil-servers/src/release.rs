//! Файл сервера из выпуска GitHub: выбор по архитектуре, сверка по `SHA256SUMS`.
//!
//! Для открытых репозиториев токен не нужен; для закрытых он (с правом чтения Contents) берётся из хранилища
//! секретов ([`crate::secrets`]) и уходит только на адрес API: при переадресации на хранилище файлов `reqwest`
//! заголовок авторизации не переносит, а адрес файла, не начинающийся с адреса API, отвергается.
//!
//! Файл, не сошедшийся с суммой, не отдаётся никогда ([`ReleaseError::ChecksumMismatch`]). Нет суммы в выпуске —
//! [`ReleaseError::Unverified`]: центр предупреждает и просит подтверждения, после него вызов повторяется с
//! `allow_unverified`.

use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::manifest::{Arch, Source};

/// Адрес API GitHub.
pub const GITHUB_API: &str = "https://api.github.com";

/// Самый большой файл, который скачиваем в память.
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

/// Самый большой файл сумм.
const MAX_SUMS_BYTES: u64 = 1024 * 1024;

/// Что может пойти не так.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseError {
    #[error("GitHub did not answer: {0}")]
    Network(String),
    #[error("GitHub did not accept the token: check it in the settings")]
    BadToken,
    #[error("no releases, or no access to the repository {0}")]
    NotFound(String),
    #[error("GitHub refused the request (rate limit or no access): code {0}")]
    Refused(u16),
    #[error("GitHub answered with code {0}")]
    Status(u16),
    #[error("cannot read the GitHub answer: {0}")]
    BadResponse(String),
    #[error("the module has no release file for {0}")]
    NoArch(&'static str),
    #[error("release {tag} has no file {file}")]
    NoAsset { tag: String, file: String },
    #[error("GitHub gave a foreign address for the file: {0}")]
    ForeignUrl(String),
    #[error("the file {file} is too large ({size} bytes)")]
    TooLarge { file: String, size: u64 },
    /// В выпуске нет суммы для файла (нет `SHA256SUMS` или в нём нет строки). Подтверждаемо владельцем.
    #[error("release {tag} cannot be verified: {reason}")]
    Unverified { tag: String, reason: String },
    #[error("checksum mismatch: {file} from release {tag} does not match {sums}")]
    ChecksumMismatch { file: String, tag: String, sums: String },
}

impl ReleaseError {
    /// Можно ли продолжить после подтверждения владельца (`allow_unverified`).
    pub fn is_unverified(&self) -> bool {
        matches!(self, ReleaseError::Unverified { .. })
    }
}

/// Что скачать.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub source: &'a Source,
    pub arch: Arch,
    /// Тег выпуска; `None` — последний.
    pub tag: Option<&'a str>,
    /// Владелец подтвердил установку без сверки суммы.
    pub allow_unverified: bool,
}

/// Скачанный файл.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub tag: String,
    pub file: String,
    pub bytes: Vec<u8>,
    /// Сумма сверена. `false` — только после `allow_unverified`.
    pub verified: bool,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    url: String,
    #[serde(default)]
    size: u64,
}

/// Клиент выпусков GitHub.
pub struct Releases {
    http: reqwest::Client,
    api: String,
    token: Option<String>,
}

impl Releases {
    /// Клиент GitHub; `token` — для закрытых репозиториев (пустой считается отсутствующим).
    pub fn new(token: Option<&str>) -> Result<Releases, ReleaseError> {
        Releases::with_api(GITHUB_API, token)
    }

    /// Клиент с другим адресом API — для GitHub Enterprise и для тестов на подставном сервере.
    pub fn with_api(api: &str, token: Option<&str>) -> Result<Releases, ReleaseError> {
        let http = reqwest::Client::builder()
            .user_agent("anvil-servers")
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| ReleaseError::Network(e.to_string()))?;
        let token = token.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned);
        Ok(Releases { http, api: api.trim_end_matches('/').to_owned(), token })
    }

    /// Скачать файл выпуска под архитектуру и сверить его сумму. `progress` — сколько скачано из скольких.
    pub async fn fetch(
        &self,
        request: Request<'_>,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<Fetched, ReleaseError> {
        let wanted = request.source.asset_for(request.arch).ok_or(ReleaseError::NoArch(request.arch.key()))?.to_owned();
        let release = match (request.tag, &request.source.tag_prefix) {
            (None, Some(prefix)) => self.newest_with_prefix(&request.source.repo, prefix).await?,
            _ => self.release(&request.source.repo, request.tag).await?,
        };
        let tag = release.tag_name.clone();
        let find = |name: &str| release.assets.iter().find(|a| a.name == name);
        let asset = find(&wanted).ok_or_else(|| ReleaseError::NoAsset { tag: tag.clone(), file: wanted.clone() })?;
        let bytes = self.download(asset, MAX_ASSET_BYTES, &mut progress).await?;

        let sums_name = &request.source.checksums;
        let verdict = match find(sums_name) {
            None => Err(format!("the release has no {sums_name}")),
            Some(sums) => {
                let text = self.download(sums, MAX_SUMS_BYTES, &mut |_, _| {}).await?;
                let text = String::from_utf8_lossy(&text);
                match checksum_of(&text, &wanted) {
                    None => Err(format!("{sums_name} has no line for {wanted}")),
                    Some(expected) if expected == sha256_hex(&bytes) => Ok(()),
                    Some(_) => {
                        return Err(ReleaseError::ChecksumMismatch { file: wanted, tag, sums: sums_name.clone() });
                    }
                }
            }
        };
        match verdict {
            Ok(()) => Ok(Fetched { tag, file: wanted, bytes, verified: true }),
            Err(reason) if request.allow_unverified => {
                tracing::warn!("installing {wanted} from {tag} without a checksum: {reason}");
                Ok(Fetched { tag, file: wanted, bytes, verified: false })
            }
            Err(reason) => Err(ReleaseError::Unverified { tag, reason }),
        }
    }

    async fn release(&self, repo: &str, tag: Option<&str>) -> Result<Release, ReleaseError> {
        let path = match tag {
            Some(tag) => format!("releases/tags/{tag}"),
            None => "releases/latest".to_owned(),
        };
        self.get_json(repo, &path).await
    }

    /// Новейший опубликованный выпуск, тег которого начинается с `prefix` (последние 50 выпусков репозитория).
    async fn newest_with_prefix(&self, repo: &str, prefix: &str) -> Result<Release, ReleaseError> {
        let list: Vec<Release> = self.get_json(repo, "releases?per_page=50").await?;
        list.into_iter()
            .find(|r| !r.draft && !r.prerelease && r.tag_name.starts_with(prefix))
            .ok_or_else(|| ReleaseError::NotFound(repo.to_owned()))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, repo: &str, path: &str) -> Result<T, ReleaseError> {
        let response = self
            .authorized(self.http.get(format!("{}/repos/{repo}/{path}", self.api)))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| ReleaseError::Network(e.without_url().to_string()))?;
        match response.status().as_u16() {
            200 => {}
            401 => return Err(ReleaseError::BadToken),
            404 => return Err(ReleaseError::NotFound(repo.to_owned())),
            code @ (403 | 429) => return Err(ReleaseError::Refused(code)),
            code => return Err(ReleaseError::Status(code)),
        }
        response.json().await.map_err(|e| ReleaseError::BadResponse(e.without_url().to_string()))
    }

    /// Токен — только на адреса API.
    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    /// Скачать файл выпуска. Адрес — только под API: туда уходит токен.
    async fn download(
        &self,
        asset: &Asset,
        limit: u64,
        progress: &mut impl FnMut(u64, u64),
    ) -> Result<Vec<u8>, ReleaseError> {
        if !asset.url.starts_with(&format!("{}/", self.api)) {
            return Err(ReleaseError::ForeignUrl(asset.url.clone()));
        }
        if asset.size > limit {
            return Err(ReleaseError::TooLarge { file: asset.name.clone(), size: asset.size });
        }
        let mut response = self
            .authorized(self.http.get(&asset.url))
            .header("Accept", "application/octet-stream")
            .send()
            .await
            .map_err(|e| ReleaseError::Network(e.without_url().to_string()))?;
        match response.status().as_u16() {
            200 => {}
            401 => return Err(ReleaseError::BadToken),
            404 => return Err(ReleaseError::NoAsset { tag: String::new(), file: asset.name.clone() }),
            code @ (403 | 429) => return Err(ReleaseError::Refused(code)),
            code => return Err(ReleaseError::Status(code)),
        }
        let mut bytes = Vec::with_capacity(asset.size.min(limit) as usize);
        while let Some(chunk) =
            response.chunk().await.map_err(|e| ReleaseError::Network(e.without_url().to_string()))?
        {
            bytes.extend_from_slice(&chunk);
            if bytes.len() as u64 > limit {
                return Err(ReleaseError::TooLarge { file: asset.name.clone(), size: bytes.len() as u64 });
            }
            progress(bytes.len() as u64, asset.size.max(bytes.len() as u64));
        }
        Ok(bytes)
    }
}

/// Сумма файла из строк `sha256sum`: «<64 hex>  <имя>» (у двоичного режима перед именем `*`).
pub fn checksum_of(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (sum, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim_start().trim_start_matches('*').trim_end();
        (name == file && sum.len() == 64 && sum.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| sum.to_ascii_lowercase())
    })
}

/// SHA-256 в шестнадцатеричном виде.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Что подставной сервер увидел: путь и заголовок авторизации.
    type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

    /// Подставной HTTP-сервер в процессе: отвечает по таблице `путь → (код, тело)`. Наружу не ходит.
    async fn serve(routes: impl FnOnce(&str) -> HashMap<String, (u16, Vec<u8>)>) -> (String, Seen) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes = Arc::new(routes(&base));
        let seen: Seen = Arc::default();
        let log = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else { return };
                let (routes, log) = (routes.clone(), log.clone());
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&buf).into_owned();
                    let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
                    let auth = head.lines().find_map(|l| {
                        l.to_ascii_lowercase().starts_with("authorization:").then(|| l[14..].trim().to_owned())
                    });
                    log.lock().unwrap().push((path.clone(), auth));
                    let (code, body) = routes.get(&path).cloned().unwrap_or((404, b"not found".to_vec()));
                    let reply = format!(
                        "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(reply.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        (base, seen)
    }

    const BINARY_X86: &[u8] = b"\x7fELF fake x86 server";
    const BINARY_ARM: &[u8] = b"\x7fELF fake arm server";

    fn source() -> Source {
        match crate::manifest::Manifest::parse(crate::manifest::tests::EXAMPLE) {
            Ok(m) => m.source,
            Err(e) => panic!("{e}"),
        }
    }

    /// Выпуск `v1.2.0` с двумя файлами; `sums` — тело `SHA256SUMS` (`None` — файла сумм в выпуске нет).
    fn release_routes(base: &str, sums: Option<String>) -> HashMap<String, (u16, Vec<u8>)> {
        let mut assets = vec![
            format!(r#"{{"name":"example-server","url":"{base}/assets/1","size":{}}}"#, BINARY_X86.len()),
            format!(r#"{{"name":"example-server-arm64","url":"{base}/assets/2","size":{}}}"#, BINARY_ARM.len()),
        ];
        let mut routes = HashMap::new();
        routes.insert("/assets/1".to_owned(), (200, BINARY_X86.to_vec()));
        routes.insert("/assets/2".to_owned(), (200, BINARY_ARM.to_vec()));
        if let Some(sums) = sums {
            assets.push(format!(r#"{{"name":"SHA256SUMS","url":"{base}/assets/3","size":{}}}"#, sums.len()));
            routes.insert("/assets/3".to_owned(), (200, sums.into_bytes()));
        }
        let json = format!(r#"{{"tag_name":"v1.2.0","assets":[{}]}}"#, assets.join(","));
        routes.insert("/repos/octo/example/releases/latest".to_owned(), (200, json.clone().into_bytes()));
        routes.insert("/repos/octo/example/releases/tags/v1.2.0".to_owned(), (200, json.into_bytes()));
        routes
    }

    fn good_sums() -> String {
        format!("{}  example-server\n{} *example-server-arm64\n", sha256_hex(BINARY_X86), sha256_hex(BINARY_ARM))
    }

    fn request(source: &Source, arch: Arch) -> Request<'_> {
        Request { source, arch, tag: None, allow_unverified: false }
    }

    #[tokio::test]
    async fn the_file_follows_the_architecture_and_its_sum_is_checked() {
        let (base, _) = serve(|b| release_routes(b, Some(good_sums()))).await;
        let source = source();
        let releases = Releases::with_api(&base, None).unwrap();

        let mut last = (0, 0);
        let x86 = releases.fetch(request(&source, Arch::X86_64), |done, total| last = (done, total)).await.unwrap();
        assert_eq!((x86.tag.as_str(), x86.file.as_str(), x86.verified), ("v1.2.0", "example-server", true));
        assert_eq!(x86.bytes, BINARY_X86);
        assert_eq!(last, (BINARY_X86.len() as u64, BINARY_X86.len() as u64), "progress ends at the whole file");

        let arm = releases.fetch(request(&source, Arch::Arm64), |_, _| {}).await.unwrap();
        assert_eq!((arm.file.as_str(), arm.bytes.as_slice()), ("example-server-arm64", BINARY_ARM));

        let tagged = Request { tag: Some("v1.2.0"), ..request(&source, Arch::X86_64) };
        assert_eq!(releases.fetch(tagged, |_, _| {}).await.unwrap().tag, "v1.2.0");
    }

    /// Спецификация: сумма не сошлась — ничего не отдаётся, задача падает с «checksum mismatch».
    #[tokio::test]
    async fn a_checksum_mismatch_hands_out_nothing() {
        let bad = format!("{}  example-server\n", sha256_hex(b"something else"));
        let (base, _) = serve(|b| release_routes(b, Some(bad))).await;
        let source = source();
        let releases = Releases::with_api(&base, None).unwrap();
        let err = releases.fetch(request(&source, Arch::X86_64), |_, _| {}).await.unwrap_err();
        assert!(matches!(err, ReleaseError::ChecksumMismatch { .. }), "{err}");
        assert!(err.to_string().contains("checksum mismatch"), "{err}");
        // Согласие владельца на непроверенное не отменяет несошедшуюся сумму.
        let allowed = Request { allow_unverified: true, ..request(&source, Arch::X86_64) };
        assert!(matches!(releases.fetch(allowed, |_, _| {}).await, Err(ReleaseError::ChecksumMismatch { .. })));
    }

    #[tokio::test]
    async fn a_release_without_sums_needs_the_owner_to_agree() {
        let source = source();
        for sums in [None, Some(format!("{}  other-file\n", sha256_hex(b"x")))] {
            let (base, _) = serve(|b| release_routes(b, sums)).await;
            let releases = Releases::with_api(&base, None).unwrap();
            let err = releases.fetch(request(&source, Arch::X86_64), |_, _| {}).await.unwrap_err();
            assert!(err.is_unverified(), "{err}");

            let allowed = Request { allow_unverified: true, ..request(&source, Arch::X86_64) };
            let fetched = releases.fetch(allowed, |_, _| {}).await.unwrap();
            assert!(!fetched.verified);
            assert_eq!(fetched.bytes, BINARY_X86);
        }
    }

    #[tokio::test]
    async fn a_public_repository_needs_no_token_and_a_private_one_gets_it_on_the_api_only() {
        let source = source();
        let (base, seen) = serve(|b| release_routes(b, Some(good_sums()))).await;
        Releases::with_api(&base, None).unwrap().fetch(request(&source, Arch::X86_64), |_, _| {}).await.unwrap();
        assert!(seen.lock().unwrap().iter().all(|(_, auth)| auth.is_none()), "no token — no Authorization header");

        let (base, seen) = serve(|b| release_routes(b, Some(good_sums()))).await;
        Releases::with_api(&base, Some("  tok123 "))
            .unwrap()
            .fetch(request(&source, Arch::X86_64), |_, _| {})
            .await
            .unwrap();
        {
            let seen = seen.lock().unwrap();
            assert!(seen.len() >= 3);
            assert!(seen.iter().all(|(_, auth)| auth.as_deref() == Some("Bearer tok123")), "{seen:?}");
        }

        let (base, seen) = serve(|b| release_routes(b, Some(good_sums()))).await;
        Releases::with_api(&base, Some("")).unwrap().fetch(request(&source, Arch::X86_64), |_, _| {}).await.unwrap();
        assert!(seen.lock().unwrap().iter().all(|(_, auth)| auth.is_none()), "an empty token is no token");
    }

    #[tokio::test]
    async fn a_tag_prefix_picks_the_newest_published_release_of_that_kind() {
        let mut source = source();
        source.tag_prefix = Some("server-v".into());
        let (base, seen) = serve(|b| {
            let mut routes = release_routes(b, Some(good_sums()));
            let body = String::from_utf8(routes["/repos/octo/example/releases/latest"].1.clone()).unwrap();
            let server = body.replace("v1.2.0", "server-v0.3.0");
            let old = body.replace("v1.2.0", "server-v0.2.0");
            let list = format!(
                r#"[{{"tag_name":"v0.9.2","assets":[]}},{{"tag_name":"server-v0.4.0-rc.1","prerelease":true,"assets":[]}},{{"tag_name":"server-v0.3.5","draft":true,"assets":[]}},{server},{old}]"#
            );
            routes.insert("/repos/octo/example/releases?per_page=50".to_owned(), (200, list.into_bytes()));
            routes
        })
        .await;
        let releases = Releases::with_api(&base, None).unwrap();
        let got = releases.fetch(request(&source, Arch::X86_64), |_, _| {}).await.unwrap();
        assert_eq!((got.tag.as_str(), got.verified), ("server-v0.3.0", true));
        assert!(seen.lock().unwrap().iter().all(|(path, _)| !path.ends_with("/releases/latest")));

        source.tag_prefix = Some("tool-v".into());
        let err = releases.fetch(request(&source, Arch::X86_64), |_, _| {}).await.unwrap_err();
        assert_eq!(err, ReleaseError::NotFound("octo/example".into()));
    }

    #[tokio::test]
    async fn missing_pieces_are_named() {
        let source = source();
        let (base, _) = serve(|b| release_routes(b, Some(good_sums()))).await;
        let releases = Releases::with_api(&base, None).unwrap();

        let mut only_x86 = source.clone();
        only_x86.assets.remove(&Arch::Arm64);
        let err = releases.fetch(request(&only_x86, Arch::Arm64), |_, _| {}).await.unwrap_err();
        assert_eq!(err, ReleaseError::NoArch("arm64"));

        let mut renamed = source.clone();
        renamed.assets.insert(Arch::X86_64, "no-such-file".into());
        let err = releases.fetch(request(&renamed, Arch::X86_64), |_, _| {}).await.unwrap_err();
        assert_eq!(err, ReleaseError::NoAsset { tag: "v1.2.0".into(), file: "no-such-file".into() });

        let mut other_repo = source.clone();
        other_repo.repo = "octo/missing".into();
        let err = releases.fetch(request(&other_repo, Arch::X86_64), |_, _| {}).await.unwrap_err();
        assert_eq!(err, ReleaseError::NotFound("octo/missing".into()));
    }

    #[tokio::test]
    async fn http_errors_are_told_apart() {
        let source = source();
        for (code, expected) in [
            (401, ReleaseError::BadToken),
            (403, ReleaseError::Refused(403)),
            (429, ReleaseError::Refused(429)),
            (500, ReleaseError::Status(500)),
        ] {
            let (base, _) =
                serve(|_| HashMap::from([("/repos/octo/example/releases/latest".to_owned(), (code, vec![]))])).await;
            let err = Releases::with_api(&base, None).unwrap().fetch(request(&source, Arch::X86_64), |_, _| {}).await;
            assert_eq!(err.unwrap_err(), expected);
        }
        let (base, _) =
            serve(|_| HashMap::from([("/repos/octo/example/releases/latest".to_owned(), (200, b"not json".to_vec()))]))
                .await;
        let err = Releases::with_api(&base, None).unwrap().fetch(request(&source, Arch::X86_64), |_, _| {}).await;
        assert!(matches!(err, Err(ReleaseError::BadResponse(_))));

        let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let err = Releases::with_api(&format!("http://127.0.0.1:{dead}"), None).unwrap();
        assert!(matches!(err.fetch(request(&source, Arch::X86_64), |_, _| {}).await, Err(ReleaseError::Network(_))));
    }

    /// Токен не уйдёт по адресу, который не под API.
    #[tokio::test]
    async fn a_foreign_file_address_is_refused_before_any_request() {
        let source = source();
        let (base, seen) = serve(|b| {
            let json = format!(
                r#"{{"tag_name":"v9","assets":[{{"name":"example-server","url":"http://evil.example/x","size":1}},
                    {{"name":"SHA256SUMS","url":"{b}/assets/3","size":1}}]}}"#
            );
            HashMap::from([("/repos/octo/example/releases/latest".to_owned(), (200, json.into_bytes()))])
        })
        .await;
        let releases = Releases::with_api(&base, Some("tok")).unwrap();
        let err = releases.fetch(request(&source, Arch::X86_64), |_, _| {}).await.unwrap_err();
        assert_eq!(err, ReleaseError::ForeignUrl("http://evil.example/x".into()));
        assert_eq!(seen.lock().unwrap().len(), 1, "only the release listing was requested");
    }

    #[test]
    fn the_sum_of_a_file_is_found_in_sha256sums() {
        let sums = format!("{}  example-server\n{} *example-server-arm64\n", "ab".repeat(32), "CD".repeat(32));
        assert_eq!(checksum_of(&sums, "example-server").as_deref(), Some("ab".repeat(32).as_str()));
        assert_eq!(checksum_of(&sums, "example-server-arm64").as_deref(), Some("cd".repeat(32).as_str()));
        assert_eq!(checksum_of(&sums, "other"), None);
        assert_eq!(checksum_of("short  example-server", "example-server"), None);
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
