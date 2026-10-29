//! Манифест модуля: что за сервер, откуда его файл, какие у него службы, порты, команды и поля формы.
//!
//! Манифест — данные, а не код: читается только из встроенных модулей и из папки настроек владельца. Команды в нём
//! запускаются на машине от root, поэтому разбор строгий: идентификатор, пути и имена службы проверяются по
//! алфавиту, подстановки — по списку, а недостающее поле называется по имени.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::template::{self, COMMAND_VARS};

/// Файл с контрольными суммами выпуска, если манифест не назвал другой.
pub const DEFAULT_CHECKSUMS: &str = "SHA256SUMS";

/// Архитектура машины: у выпуска для каждой свой файл.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arch {
    X86_64,
    Arm64,
}

impl Arch {
    /// Как архитектура называется в манифесте.
    pub fn key(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Arm64 => "arm64",
        }
    }

    /// По ответу `uname -m` и по ключу манифеста (`aarch64` — то же, что `arm64`).
    pub fn parse(name: &str) -> Option<Arch> {
        match name.trim() {
            "x86_64" | "amd64" => Some(Arch::X86_64),
            "aarch64" | "arm64" => Some(Arch::Arm64),
            _ => None,
        }
    }
}

/// Откуда берётся файл сервера: выпуск GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// `владелец/репозиторий`.
    pub repo: String,
    /// Имя файла выпуска для каждой архитектуры.
    pub assets: BTreeMap<Arch, String>,
    /// Файл с суммами SHA-256 в том же выпуске.
    pub checksums: String,
    /// Начало тега выпусков сервера, когда в репозитории есть и другие выпуски (приложение): берётся новейший
    /// выпуск с таким началом. `None` — последний выпуск репозитория.
    pub tag_prefix: Option<String>,
}

impl Source {
    /// Файл выпуска для архитектуры; `None` — для неё выпуска нет.
    pub fn asset_for(&self, arch: Arch) -> Option<&str> {
        self.assets.get(&arch).map(String::as_str)
    }
}

/// Порты, которые модуль слушает по умолчанию (для брандмауэра и показа).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ports {
    pub tcp: Vec<u16>,
    pub udp: Vec<u16>,
}

/// Команда контракта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandKind {
    Install,
    Status,
    Backup,
    Uninstall,
    Update,
}

impl CommandKind {
    /// Имя ключа в `[commands]`.
    pub fn name(self) -> &'static str {
        match self {
            CommandKind::Install => "install",
            CommandKind::Status => "status",
            CommandKind::Backup => "backup",
            CommandKind::Uninstall => "uninstall",
            CommandKind::Update => "update",
        }
    }

    const ALL: [CommandKind; 5] =
        [CommandKind::Install, CommandKind::Status, CommandKind::Backup, CommandKind::Uninstall, CommandKind::Update];
}

/// Команды контракта: шаблоны со значениями вместо `{bin}`, `{args}`, `{service}`, `{dir}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commands {
    pub install: String,
    pub status: Option<String>,
    pub backup: Option<String>,
    pub uninstall: Option<String>,
    pub update: Option<String>,
}

impl Commands {
    /// Шаблон команды; `None` — модуль её не объявил.
    pub fn get(&self, kind: CommandKind) -> Option<&str> {
        match kind {
            CommandKind::Install => Some(&self.install),
            CommandKind::Status => self.status.as_deref(),
            CommandKind::Backup => self.backup.as_deref(),
            CommandKind::Uninstall => self.uninstall.as_deref(),
            CommandKind::Update => self.update.as_deref(),
        }
    }
}

/// Какого вида поле формы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    /// Не показывается в журнале и в подтверждении; в команде всё равно есть.
    Password,
    Number,
    /// Порт TCP: число 1–65535, которое центр сверяет с занятыми на машине и у других экземпляров.
    Port,
    Bool,
    /// Одно из `options`.
    Choice,
}

/// Поле окна установки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub default: Option<String>,
    /// Что добавить в `{args}`; `{value}` — значение поля. У `Bool` аргумент идёт, только когда включено.
    pub arg: Option<String>,
    pub required: bool,
    pub options: Vec<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
}

/// Кнопка «Обслуживания»: команда сверх контракта.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    pub cmd: String,
    /// Спрашивать ли подтверждение у владельца. По умолчанию да; команды сверх контракта показываются дословно.
    pub confirm: bool,
}

/// Описание модуля.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    /// Показывать только при включённом переключателе «Скрытые модули».
    pub hidden: bool,
    /// Можно поставить несколько экземпляров одного сервера.
    pub instances: bool,
    /// Имя службы systemd (по умолчанию — `id`).
    pub service: String,
    /// Абсолютный путь к бинарнику на машине.
    pub binary: Option<String>,
    /// Пользователь службы (по умолчанию — имя службы).
    pub user: String,
    /// Каталог данных (по умолчанию `/var/lib/<служба>`).
    pub data_dir: String,
    pub source: Source,
    pub ports: Ports,
    pub commands: Commands,
    pub form: Vec<Field>,
    pub actions: Vec<Action>,
}

/// Чем манифест не годится.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// Не TOML (или не тот тип у поля).
    #[error("not valid TOML: {0}")]
    Syntax(String),
    /// Нет обязательного поля: `what` — по-человечески, `field` — его путь в файле.
    #[error("no {what} ({field})")]
    Missing { what: &'static str, field: &'static str },
    /// Поле есть, но значение недопустимо.
    #[error("{field}: {reason}")]
    Invalid { field: String, reason: String },
}

impl ManifestError {
    fn missing(what: &'static str, field: &'static str) -> Self {
        ManifestError::Missing { what, field }
    }

    fn invalid(field: impl Into<String>, reason: impl Into<String>) -> Self {
        ManifestError::Invalid { field: field.into(), reason: reason.into() }
    }
}

// --- Сырой вид файла: всё необязательное, чтобы недостающее называть самим, а не словами serde. ---

#[derive(Deserialize, Default)]
struct Raw {
    id: Option<String>,
    name: Option<String>,
    icon: Option<String>,
    hidden: Option<bool>,
    instances: Option<bool>,
    service: Option<String>,
    binary: Option<String>,
    user: Option<String>,
    data_dir: Option<String>,
    source: Option<RawSource>,
    ports: Option<RawPorts>,
    commands: Option<RawCommands>,
    #[serde(default)]
    form: Vec<RawField>,
    #[serde(default)]
    action: Vec<RawAction>,
}

#[derive(Deserialize, Default)]
struct RawSource {
    repo: Option<String>,
    #[serde(default)]
    assets: BTreeMap<String, String>,
    checksums: Option<String>,
    tag_prefix: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawPorts {
    #[serde(default)]
    tcp: Vec<u16>,
    #[serde(default)]
    udp: Vec<u16>,
}

#[derive(Deserialize, Default)]
struct RawCommands {
    install: Option<String>,
    status: Option<String>,
    backup: Option<String>,
    uninstall: Option<String>,
    update: Option<String>,
}

#[derive(Deserialize)]
struct RawField {
    key: Option<String>,
    label: Option<String>,
    kind: Option<String>,
    default: Option<toml::Value>,
    arg: Option<String>,
    required: Option<bool>,
    #[serde(default)]
    options: Vec<String>,
    min: Option<i64>,
    max: Option<i64>,
}

#[derive(Deserialize)]
struct RawAction {
    label: Option<String>,
    cmd: Option<String>,
    confirm: Option<bool>,
}

// --- Допустимые алфавиты ---

fn is_id(s: &str) -> bool {
    let mut chars = s.chars();
    s.len() <= 40
        && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

pub(crate) fn is_service(s: &str) -> bool {
    let mut chars = s.chars();
    s.len() <= 64
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || "_.@-".contains(c))
}

pub(crate) fn is_user(s: &str) -> bool {
    let mut chars = s.chars();
    s.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Абсолютный путь без `..` и без символов, которым нужны кавычки.
pub(crate) fn is_abs_path(s: &str) -> bool {
    s.starts_with('/')
        && s.len() <= 200
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c))
        && !s.split('/').any(|part| part == "..")
}

fn is_repo(s: &str) -> bool {
    let part = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
    matches!(s.split_once('/'), Some((owner, name)) if part(owner) && part(name))
}

fn is_file_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 120
        && !s.starts_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "_.-+".contains(c))
}

fn is_key(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn text_of(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

impl Manifest {
    /// Разобрать и проверить манифест.
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let raw: Raw = toml::from_str(text).map_err(|e| {
            let line = e.span().map(|s| text[..s.start.min(text.len())].matches('\n').count() + 1);
            match line {
                Some(line) => ManifestError::Syntax(format!("line {line}: {}", e.message())),
                None => ManifestError::Syntax(e.message().to_owned()),
            }
        })?;

        let id = non_empty(raw.id).ok_or(ManifestError::missing("id", "id"))?;
        if !is_id(&id) {
            return Err(ManifestError::invalid("id", "lowercase letters, digits, `-` and `_`, up to 40"));
        }

        let source = parse_source(raw.source)?;

        let commands = raw.commands.unwrap_or_default();
        let install =
            non_empty(commands.install).ok_or(ManifestError::missing("install command", "commands.install"))?;
        let commands = Commands {
            install,
            status: non_empty(commands.status),
            backup: non_empty(commands.backup),
            uninstall: non_empty(commands.uninstall),
            update: non_empty(commands.update),
        };

        let service = raw.service.unwrap_or_else(|| id.clone());
        if !is_service(&service) {
            return Err(ManifestError::invalid("service", "letters, digits and `_.@-`, up to 64"));
        }
        let user = raw.user.unwrap_or_else(|| service.clone());
        if !is_user(&user) {
            return Err(ManifestError::invalid("user", "a Linux user name: lowercase letters, digits, `_`, `-`"));
        }
        let data_dir = raw.data_dir.unwrap_or_else(|| format!("/var/lib/{service}"));
        if !is_abs_path(&data_dir) {
            return Err(ManifestError::invalid("data_dir", "an absolute path of letters, digits and `_./-`"));
        }
        if let Some(binary) = &raw.binary
            && !is_abs_path(binary)
        {
            return Err(ManifestError::invalid("binary", "an absolute path of letters, digits and `_./-`"));
        }

        let mut ports = Ports::default();
        if let Some(p) = raw.ports {
            if p.tcp.iter().chain(&p.udp).any(|port| *port == 0) {
                return Err(ManifestError::invalid("ports", "port 0 is not a port"));
            }
            ports = Ports { tcp: p.tcp, udp: p.udp };
        }

        let form = raw.form.into_iter().enumerate().map(|(i, f)| parse_field(i, f)).collect::<Result<Vec<_>, _>>()?;
        for (i, field) in form.iter().enumerate() {
            if form[..i].iter().any(|other| other.key == field.key) {
                return Err(ManifestError::invalid("form", format!("the key `{}` is used twice", field.key)));
            }
        }

        let actions = raw
            .action
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                let (Some(label), Some(cmd)) = (non_empty(a.label), non_empty(a.cmd)) else {
                    return Err(ManifestError::invalid(format!("action[{}]", i + 1), "needs `label` and `cmd`"));
                };
                Ok(Action { label, cmd, confirm: a.confirm.unwrap_or(true) })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let manifest = Manifest {
            name: non_empty(raw.name).unwrap_or_else(|| id.clone()),
            icon: non_empty(raw.icon),
            hidden: raw.hidden.unwrap_or(false),
            instances: raw.instances.unwrap_or(false),
            binary: raw.binary,
            id,
            service,
            user,
            data_dir,
            source,
            ports,
            commands,
            form,
            actions,
        };
        manifest.check_templates()?;
        Ok(manifest)
    }

    /// Все шаблоны манифеста должны разбираться, знать свои подстановки, а `{bin}` — иметь `binary`.
    fn check_templates(&self) -> Result<(), ManifestError> {
        let mut templates: Vec<(String, &str)> = Vec::new();
        for kind in CommandKind::ALL {
            if let Some(t) = self.commands.get(kind) {
                templates.push((format!("commands.{}", kind.name()), t));
            }
        }
        for (i, a) in self.actions.iter().enumerate() {
            templates.push((format!("action[{}].cmd", i + 1), &a.cmd));
        }
        for (field, t) in templates {
            let used = template::check(t, COMMAND_VARS).map_err(|e| ManifestError::invalid(field.clone(), e))?;
            if used.iter().any(|u| u == "bin") && self.binary.is_none() {
                return Err(ManifestError::invalid(field, "uses `{bin}`, but the manifest has no `binary`"));
            }
        }
        for f in &self.form {
            if let Some(arg) = &f.arg {
                template::check(arg, &["value"])
                    .map_err(|e| ManifestError::invalid(format!("form `{}` arg", f.key), e))?;
            }
        }
        Ok(())
    }

    /// Есть ли выпуск для архитектуры машины.
    pub fn supports(&self, arch: Arch) -> bool {
        self.source.assets.contains_key(&arch)
    }
}

fn parse_source(raw: Option<RawSource>) -> Result<Source, ManifestError> {
    let raw = raw.ok_or(ManifestError::missing("release source", "source.repo"))?;
    let repo = non_empty(raw.repo).ok_or(ManifestError::missing("release source", "source.repo"))?;
    if !is_repo(&repo) {
        return Err(ManifestError::invalid("source.repo", "expected `owner/name`"));
    }
    if raw.assets.is_empty() {
        return Err(ManifestError::missing("release file", "source.assets"));
    }
    let mut assets = BTreeMap::new();
    for (arch, file) in raw.assets {
        let arch = Arch::parse(&arch).ok_or_else(|| {
            ManifestError::invalid("source.assets", format!("unknown architecture `{arch}` (x86_64, arm64)"))
        })?;
        if !is_file_name(&file) {
            return Err(ManifestError::invalid("source.assets", format!("`{file}` is not a plain file name")));
        }
        if assets.insert(arch, file).is_some() {
            return Err(ManifestError::invalid("source.assets", format!("{} is given twice", arch.key())));
        }
    }
    let checksums = raw.checksums.unwrap_or_else(|| DEFAULT_CHECKSUMS.to_owned());
    if !is_file_name(&checksums) {
        return Err(ManifestError::invalid("source.checksums", "not a plain file name"));
    }
    let tag_prefix = match raw.tag_prefix {
        None => None,
        Some(prefix)
            if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) =>
        {
            Some(prefix)
        }
        Some(_) => return Err(ManifestError::invalid("source.tag_prefix", "letters, digits, `.`, `_` and `-` only")),
    };
    Ok(Source { repo, assets, checksums, tag_prefix })
}

fn parse_field(index: usize, raw: RawField) -> Result<Field, ManifestError> {
    let place = format!("form[{}]", index + 1);
    let key = raw.key.filter(|s| !s.is_empty()).ok_or_else(|| ManifestError::missing("form field key", "form.key"))?;
    if !is_key(&key) {
        return Err(ManifestError::invalid(&place, format!("key `{key}`: letters, digits, `_`, `-`")));
    }
    let kind = match raw.kind.as_deref().unwrap_or("text") {
        "text" => FieldKind::Text,
        "password" => FieldKind::Password,
        "number" => FieldKind::Number,
        "port" => FieldKind::Port,
        "bool" => FieldKind::Bool,
        "choice" => FieldKind::Choice,
        other => return Err(ManifestError::invalid(&place, format!("unknown kind `{other}`"))),
    };
    if kind == FieldKind::Choice && raw.options.is_empty() {
        return Err(ManifestError::invalid(&place, "a `choice` field needs `options`"));
    }
    let default = match &raw.default {
        None => None,
        Some(value) => Some(
            text_of(value)
                .ok_or_else(|| ManifestError::invalid(&place, "`default` must be a string, a number or a boolean"))?,
        )
        .filter(|d| !d.is_empty()),
    };
    let field = Field {
        label: non_empty(raw.label).unwrap_or_else(|| key.clone()),
        key,
        kind,
        default,
        arg: raw.arg,
        required: raw.required.unwrap_or(false),
        options: raw.options,
        min: raw.min,
        max: raw.max,
    };
    // Значение по умолчанию проверяется так же, как введённое: сломанное видно сразу, а не при установке.
    if let Some(default) = &field.default {
        crate::form::normalize(&field, default).map_err(|e| ManifestError::invalid(&place, format!("default: {e}")))?;
    }
    Ok(field)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Выдуманный модуль для тестов.
    pub(crate) const EXAMPLE: &str = r#"
id = "example"
name = "Пример"
instances = true
service = "example"
binary = "/opt/example/example-server"

[source]
repo = "octo/example"
assets = { x86_64 = "example-server", aarch64 = "example-server-arm64" }

[ports]
tcp = [8740]

[commands]
install = "{bin} install {args}"
status = "{bin} status --json"
backup = "{bin} backup --to {dir}"
uninstall = "{bin} uninstall"

[[form]]
key = "port"
label = "Порт"
kind = "port"
default = 8740
arg = "--port {value}"

[[action]]
label = "Перезапустить"
cmd = "systemctl restart {service}"
"#;

    #[test]
    fn a_tag_prefix_is_read_and_checked() {
        let with = |prefix: &str| {
            let line = format!("tag_prefix = \"{prefix}\"\n\n[ports]");
            EXAMPLE.replace("[ports]", &line)
        };
        assert_eq!(Manifest::parse(&with("server-v")).unwrap().source.tag_prefix.as_deref(), Some("server-v"));
        for bad in ["", "a b", "x/y"] {
            let err = Manifest::parse(&with(bad)).unwrap_err().to_string();
            assert!(err.contains("source.tag_prefix"), "{bad:?}: {err}");
        }
    }

    #[test]
    fn the_example_manifest_reads_whole() {
        let m = Manifest::parse(EXAMPLE).unwrap();
        assert_eq!((m.id.as_str(), m.name.as_str(), m.hidden, m.instances), ("example", "Пример", false, true));
        assert_eq!(m.service, "example");
        assert_eq!(
            (m.user.as_str(), m.data_dir.as_str()),
            ("example", "/var/lib/example"),
            "defaults follow the service"
        );
        assert_eq!(m.source.repo, "octo/example");
        assert_eq!(m.source.asset_for(Arch::X86_64), Some("example-server"));
        assert_eq!(m.source.asset_for(Arch::Arm64), Some("example-server-arm64"), "`aarch64` is `arm64`");
        assert_eq!(m.source.checksums, "SHA256SUMS");
        assert_eq!(m.source.tag_prefix, None, "the latest release unless a prefix is given");
        assert_eq!(m.ports.tcp, [8740]);
        assert_eq!(m.commands.get(CommandKind::Status), Some("{bin} status --json"));
        assert_eq!(m.commands.get(CommandKind::Update), None);
        assert_eq!(m.form.len(), 1);
        assert_eq!((m.form[0].kind, m.form[0].default.as_deref()), (FieldKind::Port, Some("8740")));
        assert_eq!(m.actions.len(), 1);
        assert!(m.actions[0].confirm, "an action asks by default");
    }

    #[test]
    fn a_missing_field_is_named() {
        let no_install = EXAMPLE.replace("install = \"{bin} install {args}\"\n", "");
        let err = Manifest::parse(&no_install).unwrap_err();
        assert_eq!(err, ManifestError::Missing { what: "install command", field: "commands.install" });
        assert!(err.to_string().starts_with("no install command"), "{err}");

        let no_id = EXAMPLE.replace("id = \"example\"\n", "");
        assert!(Manifest::parse(&no_id).unwrap_err().to_string().starts_with("no id"));

        let no_source = EXAMPLE.replace("repo = \"octo/example\"\n", "");
        assert!(Manifest::parse(&no_source).unwrap_err().to_string().starts_with("no release source"));

        let no_files =
            EXAMPLE.replace("assets = { x86_64 = \"example-server\", aarch64 = \"example-server-arm64\" }\n", "");
        assert!(Manifest::parse(&no_files).unwrap_err().to_string().starts_with("no release file"));

        assert!(Manifest::parse("").unwrap_err().to_string().starts_with("no id"));
    }

    #[test]
    fn broken_toml_says_which_line() {
        let err = Manifest::parse("id = \"x\"\nname = = 3").unwrap_err();
        assert!(matches!(&err, ManifestError::Syntax(s) if s.starts_with("line 2")), "{err}");
        let err = Manifest::parse("id = 5").unwrap_err();
        assert!(matches!(err, ManifestError::Syntax(_)), "a wrong type is a syntax error: {err}");
    }

    #[test]
    fn dangerous_names_do_not_get_through() {
        for (from, to) in [
            ("id = \"example\"", "id = \"Ex ample\""),
            ("service = \"example\"", "service = \"a;rm -rf /\""),
            ("binary = \"/opt/example/example-server\"", "binary = \"/opt/../etc/x\""),
            ("binary = \"/opt/example/example-server\"", "binary = \"example-server\""),
            ("repo = \"octo/example\"", "repo = \"octo\""),
            ("x86_64 = \"example-server\"", "x86_64 = \"../example-server\""),
            ("x86_64 = \"example-server\"", "riscv = \"example-server\""),
            ("tcp = [8740]", "tcp = [0]"),
        ] {
            assert!(Manifest::parse(&EXAMPLE.replace(from, to)).is_err(), "{to}");
        }
    }

    #[test]
    fn placeholders_are_checked_when_the_manifest_is_read() {
        let bad = EXAMPLE.replace("systemctl restart {service}", "systemctl restart {servise}");
        assert!(Manifest::parse(&bad).unwrap_err().to_string().contains("servise"));
        let no_bin = EXAMPLE.replace("binary = \"/opt/example/example-server\"\n", "");
        assert!(Manifest::parse(&no_bin).unwrap_err().to_string().contains("binary"));
        let bad_arg = EXAMPLE.replace("--port {value}", "--port {val}");
        assert!(Manifest::parse(&bad_arg).is_err());
    }

    #[test]
    fn form_mistakes_are_caught_at_load() {
        let twice = format!("{EXAMPLE}\n[[form]]\nkey = \"port\"\n");
        assert!(Manifest::parse(&twice).unwrap_err().to_string().contains("twice"));
        let kind = EXAMPLE.replace("kind = \"port\"", "kind = \"colour\"");
        assert!(Manifest::parse(&kind).is_err());
        let default = EXAMPLE.replace("default = 8740", "default = 99999");
        assert!(Manifest::parse(&default).unwrap_err().to_string().contains("default"));
        let choice = format!("{EXAMPLE}\n[[form]]\nkey = \"mode\"\nkind = \"choice\"\n");
        assert!(Manifest::parse(&choice).is_err(), "a choice needs options");
    }

    #[test]
    fn hidden_and_the_defaults() {
        let minimal = r#"
id = "tiny"
hidden = true
[source]
repo = "octo/tiny"
assets = { x86_64 = "tiny" }
[commands]
install = "true"
"#;
        let m = Manifest::parse(minimal).unwrap();
        assert!(m.hidden && !m.instances);
        assert_eq!((m.name.as_str(), m.service.as_str()), ("tiny", "tiny"));
        assert!(m.supports(Arch::X86_64) && !m.supports(Arch::Arm64));
    }

    #[test]
    fn architectures_are_read_from_uname() {
        assert_eq!(Arch::parse("x86_64\n"), Some(Arch::X86_64));
        assert_eq!(Arch::parse("aarch64"), Some(Arch::Arm64));
        assert_eq!(Arch::parse("armv7l"), None);
    }
}
