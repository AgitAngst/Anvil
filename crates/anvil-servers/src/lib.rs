//! Ядро управления серверами в Anvil — без графики и без egui.
//!
//! Из чего состоит (OpenSpec `server-modules`):
//!
//! - [`manifest`] — манифест модуля (TOML): идентификатор, источник выпуска по архитектурам, служба, порты, команды
//!   контракта `install | status | backup | uninstall | update`, форма настроек, действия. Неполный манифест
//!   отвергается с названием недостающего поля.
//! - [`registry`] — реестр: встроенные модули ([`builtin`]) и `modules/*.toml` из папки настроек. Битый манифест реестр не роняет —
//!   он попадает в список с причиной. Скрытые модули отдаются только при включённом переключателе.
//! - [`form`], [`command`], [`template`], [`shell`] — готовая команда для машины: значения формы проверяются по виду
//!   поля и подставляются в `{bin}`, `{args}`, `{service}`, `{dir}` только как слова оболочки.
//! - [`instance`] — несколько экземпляров одного сервера: имена службы, пользователя и каталога, проверка портов.
//! - [`ssh`] — SSH: запоминание ключа машины и отказ при его смене (до отправки любых секретов), вход паролем, ключом
//!   или агентом, `sudo`, построчный вывод, заливка файла с ходом.
//! - [`secrets`] — пароли, ключевые фразы и токен выпусков только в хранилище учётных данных системы
//!   (служба `anvil-servers`); [`redact`] прячет токены в выводе и в журнале работы.
//! - [`release`] — файл сервера из выпуска GitHub по архитектуре; сверка с `SHA256SUMS`; несошедшаяся сумма —
//!   отказ, отсутствие суммы — только с согласия владельца. Для открытых репозиториев токен не нужен.
//! - [`status`] — состояние модуля из вывода `status --json`: известные поля и «ещё» для остальных.
//!
//! Вид (экраны, окна, подтверждения) и задачи с ходом работы — в приложении `anvil`, не здесь. Всё, что ядро
//! печатает в журнал и в ошибки, — по-английски; перевод для человека делает вид.

pub mod builtin;
pub mod command;
pub mod form;
pub mod instance;
pub mod manifest;
pub mod redact;
pub mod registry;
pub mod release;
pub mod secrets;
pub mod shell;
pub mod ssh;
pub mod status;
pub mod template;

pub use command::{CommandError, Context};
pub use form::FormError;
pub use instance::{InstanceError, Naming, PortBusy};
pub use manifest::{Arch, CommandKind, Field, FieldKind, Manifest, ManifestError};
pub use registry::{Broken, Module, Origin, Registry};
pub use release::{Fetched, ReleaseError, Releases};
pub use secrets::{MemoryStore, SecretStore, SystemStore};
pub use ssh::{Auth, Conn, ConnectError, Output, Target};
pub use status::Status;
