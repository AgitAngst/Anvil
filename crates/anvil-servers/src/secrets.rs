//! Секреты — пароли входа, ключевые фразы и токен выпусков — только в хранилище учётных данных системы.
//!
//! В файл настроек и в журнал работы они не попадают ни при каких обстоятельствах. Служба хранилища —
//! [`SERVICE`]; ключи записей: `ssh:<id машины>` и `github`. Хранилище спрятано за [`SecretStore`], чтобы
//! программа и тесты не зависели от настоящего: в тестах — [`MemoryStore`].

use std::collections::HashMap;
use std::sync::Mutex;

/// Имя службы записей в хранилище учётных данных.
pub const SERVICE: &str = "anvil-servers";

/// Запись об ошибке хранилища: причина без самого секрета.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("credential store: {0}")]
    Store(String),
    #[error("the credential store is not available on this system")]
    Unavailable,
}

/// Хранилище секретов: ключ → значение.
pub trait SecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    /// Пустое значение — то же, что забыть запись.
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn forget(&self, key: &str) -> Result<(), SecretError>;

    /// Пароль SSH или ключевая фраза машины.
    fn machine(&self, id: &str) -> Result<Option<String>, SecretError> {
        self.get(&machine_key(id))
    }

    fn set_machine(&self, id: &str, secret: &str) -> Result<(), SecretError> {
        self.set(&machine_key(id), secret)
    }

    fn forget_machine(&self, id: &str) -> Result<(), SecretError> {
        self.forget(&machine_key(id))
    }

    /// Токен GitHub для закрытых репозиториев; для открытых не нужен.
    fn github(&self) -> Result<Option<String>, SecretError> {
        self.get(GITHUB_KEY)
    }

    fn set_github(&self, token: &str) -> Result<(), SecretError> {
        self.set(GITHUB_KEY, token)
    }
}

const GITHUB_KEY: &str = "github";

/// Ключ записи пароля или ключевой фразы машины.
pub fn machine_key(id: &str) -> String {
    format!("ssh:{id}")
}

/// Хранилище только в памяти — для тестов и для режима без хранилища.
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self.entries.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        if value.is_empty() {
            return self.forget(key);
        }
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn forget(&self, key: &str) -> Result<(), SecretError> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
        Ok(())
    }
}

/// Хранилище учётных данных системы (в Windows — Credential Manager). На других системах пока недоступно:
/// там ядро работает, а секреты программа держит в памяти на время сеанса.
pub struct SystemStore {
    service: String,
}

impl Default for SystemStore {
    fn default() -> Self {
        SystemStore { service: SERVICE.to_owned() }
    }
}

impl SystemStore {
    /// Хранилище с другим именем службы — для проверки на живой системе, чтобы не трогать настоящие записи.
    pub fn with_service(service: &str) -> SystemStore {
        SystemStore { service: service.to_owned() }
    }
}

#[cfg(windows)]
impl SystemStore {
    fn entry(&self, key: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, key).map_err(|e| SecretError::Store(e.to_string()))
    }
}

#[cfg(windows)]
impl SecretStore for SystemStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Store(e.to_string())),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        if value.is_empty() {
            return self.forget(key);
        }
        self.entry(key)?.set_password(value).map_err(|e| SecretError::Store(e.to_string()))
    }

    fn forget(&self, key: &str) -> Result<(), SecretError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Store(e.to_string())),
        }
    }
}

#[cfg(not(windows))]
impl SecretStore for SystemStore {
    fn get(&self, _key: &str) -> Result<Option<String>, SecretError> {
        let _ = &self.service;
        Err(SecretError::Unavailable)
    }

    fn set(&self, _key: &str, _value: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }

    fn forget(&self, _key: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_service_and_the_keys_are_fixed() {
        assert_eq!(SERVICE, "anvil-servers");
        assert_eq!(machine_key("m1"), "ssh:m1");
        assert_eq!(SystemStore::default().service, "anvil-servers");
    }

    #[test]
    fn secrets_round_trip_and_an_empty_value_forgets() {
        let store = MemoryStore::default();
        assert_eq!(store.machine("m1").unwrap(), None);
        store.set_machine("m1", "пароль").unwrap();
        store.set_github("ghp_token").unwrap();
        assert_eq!(store.machine("m1").unwrap().as_deref(), Some("пароль"));
        assert_eq!(store.github().unwrap().as_deref(), Some("ghp_token"));
        assert_eq!(store.machine("m2").unwrap(), None, "machines do not share secrets");
        store.set_machine("m1", "").unwrap();
        assert_eq!(store.machine("m1").unwrap(), None);
        store.forget_machine("m1").unwrap();
        store.forget_machine("never-was").unwrap();
    }

    /// Настоящее хранилище Windows: запись, чтение, забывание. Службе дано другое имя, настоящие записи не
    /// затрагиваются. Запускается вручную: `cargo test -p anvil-servers -- --ignored system_store`.
    #[cfg(windows)]
    #[test]
    #[ignore = "writes to the real Windows credential store"]
    fn system_store_round_trip() {
        let store = SystemStore::with_service("anvil-servers-test");
        let key = format!("probe-{}", std::process::id());
        store.set(&key, "value").unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some("value"));
        store.forget(&key).unwrap();
        assert_eq!(store.get(&key).unwrap(), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn without_a_system_store_the_error_is_honest() {
        assert!(matches!(SystemStore::default().get("x"), Err(SecretError::Unavailable)));
    }
}
