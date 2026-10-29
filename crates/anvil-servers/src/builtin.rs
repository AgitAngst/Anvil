//! Встроенные модули: манифесты, которые идут вместе с программой.
//!
//! Сейчас один — сервер-ретранслятор Ampulse. Его манифест — копия `server/module.toml` из репозитория Ampulse (там же
//! тест сверяет его с самой программой); меняется манифест там — обновить копию здесь. Реестр
//! ([`crate::Registry::load`]) принимает список параметром, поэтому вид и тесты от него не зависят.

/// Встроенные манифесты: `(имя для списка, текст TOML)`.
pub const BUILTIN: &[(&str, &str)] = &[("ampulse", include_str!("builtin/ampulse.toml"))];

#[cfg(test)]
mod tests {
    use super::BUILTIN;
    use crate::Registry;

    #[test]
    fn the_built_in_modules_parse_and_are_visible() {
        let registry = Registry::load_from(BUILTIN, None);
        assert_eq!(registry.broken(true).count(), 0);
        let ampulse = registry.get("ampulse", false).expect("ampulse module");
        assert_eq!(ampulse.manifest.service, "ampulse");
        // Пустая форма: порт по умолчанию, пустой адрес в команду не попадает.
        let args = ampulse.manifest.form_args(&Default::default()).expect("form");
        assert_eq!(args, "--port 8740");
    }
}
