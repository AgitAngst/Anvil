//! Несколько экземпляров одного сервера: у каждого своя служба, пользователь, каталог данных и порты.
//!
//! Имена строятся из манифеста и имени экземпляра: `<служба>-<имя>`. Порты центр не выдумывает молча: занятый
//! порт — отказ с названием порта, свободный подбирается от желаемого.

use std::collections::{BTreeMap, BTreeSet};

use crate::manifest::{Manifest, is_abs_path, is_service, is_user};

/// Как отпросить у машины занятые порты (слушающие TCP и UDP, без заголовка).
pub const LISTEN_COMMAND: &str = "ss -H -ltun";

/// Чем имя экземпляра не годится.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstanceError {
    #[error("this module runs a single instance")]
    NotSupported,
    #[error("instance name: lowercase letters, digits and `-`, up to 20, not starting with `-`")]
    BadName,
    #[error("the name `{0}` is too long for the service, user or directory")]
    TooLong(String),
}

/// Имена, под которыми экземпляр живёт на машине.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Naming {
    /// Имя экземпляра; `None` — основной.
    pub instance: Option<String>,
    pub service: String,
    pub user: String,
    pub data_dir: String,
}

/// Имя экземпляра допустимо.
pub fn is_instance_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 20
        && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Manifest {
    /// Имена службы, пользователя и каталога для экземпляра (или основного, если имя не дано).
    pub fn naming(&self, instance: Option<&str>) -> Result<Naming, InstanceError> {
        let Some(name) = instance else {
            return Ok(Naming {
                instance: None,
                service: self.service.clone(),
                user: self.user.clone(),
                data_dir: self.data_dir.clone(),
            });
        };
        if !self.instances {
            return Err(InstanceError::NotSupported);
        }
        if !is_instance_name(name) {
            return Err(InstanceError::BadName);
        }
        let naming = Naming {
            instance: Some(name.to_owned()),
            service: format!("{}-{name}", self.service),
            user: format!("{}-{name}", self.user),
            data_dir: format!("{}-{name}", self.data_dir),
        };
        if !is_service(&naming.service) || !is_user(&naming.user) || !is_abs_path(&naming.data_dir) {
            return Err(InstanceError::TooLong(name.to_owned()));
        }
        Ok(naming)
    }
}

/// Кто занимает порт.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortHolder {
    /// Порт уже слушают на машине.
    Machine,
    /// Порт у другого экземпляра (его имя; пусто — основной).
    Instance(String),
    /// Порт указан в той же форме дважды.
    Form,
}

/// Порт занят.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct PortBusy {
    pub port: u16,
    pub holder: PortHolder,
}

impl std::fmt::Display for PortBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.holder {
            PortHolder::Machine => write!(f, "port {} is already listened on this machine", self.port),
            PortHolder::Instance(name) if name.is_empty() => {
                write!(f, "port {} is used by the main instance", self.port)
            }
            PortHolder::Instance(name) => write!(f, "port {} is used by instance `{name}`", self.port),
            PortHolder::Form => write!(f, "port {} is given twice", self.port),
        }
    }
}

/// Проверить желаемые порты: ни один не слушают на машине (`listening`, см. [`listening_ports`]) и не держит
/// другой экземпляр (`others`: имя экземпляра → его порты). Ничего не ставится, пока это не прошло.
pub fn check_ports(
    requested: &[u16],
    listening: &BTreeSet<u16>,
    others: &BTreeMap<String, Vec<u16>>,
) -> Result<(), PortBusy> {
    for (i, port) in requested.iter().enumerate() {
        if requested[..i].contains(port) {
            return Err(PortBusy { port: *port, holder: PortHolder::Form });
        }
        if let Some((name, _)) = others.iter().find(|(_, ports)| ports.contains(port)) {
            return Err(PortBusy { port: *port, holder: PortHolder::Instance(name.clone()) });
        }
        if listening.contains(port) {
            return Err(PortBusy { port: *port, holder: PortHolder::Machine });
        }
    }
    Ok(())
}

/// Свободный порт, начиная с желаемого: не слушается на машине и не в `taken`.
pub fn pick_free(wanted: u16, listening: &BTreeSet<u16>, taken: &BTreeSet<u16>) -> Option<u16> {
    (wanted.max(1)..=u16::MAX).find(|p| !listening.contains(p) && !taken.contains(p))
}

/// Порты из вывода [`LISTEN_COMMAND`] (`ss`): `0.0.0.0:22`, `[::]:22`, `*:80`, `127.0.0.53%lo:53`.
pub fn listening_ports(ss_output: &str) -> BTreeSet<u16> {
    ss_output
        .lines()
        .filter_map(|line| {
            // Локальный адрес — первое слово с `:` и числом после него (состояние `LISTEN` двоеточия не содержит).
            line.split_whitespace().find_map(|word| word.rsplit_once(':')?.1.parse::<u16>().ok())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::EXAMPLE;

    #[test]
    fn each_instance_gets_its_own_names() {
        let m = Manifest::parse(EXAMPLE).unwrap();
        let main = m.naming(None).unwrap();
        assert_eq!(
            (main.service.as_str(), main.user.as_str(), main.data_dir.as_str()),
            ("example", "example", "/var/lib/example")
        );
        let family = m.naming(Some("family")).unwrap();
        assert_eq!(family.instance.as_deref(), Some("family"));
        assert_eq!(
            (family.service.as_str(), family.user.as_str(), family.data_dir.as_str()),
            ("example-family", "example-family", "/var/lib/example-family")
        );
    }

    #[test]
    fn instance_names_are_checked() {
        let m = Manifest::parse(EXAMPLE).unwrap();
        for bad in ["", "A", "-x", "a b", "a;b", "../x", "a.b", "abcdefghijklmnopqrstuvwxyz"] {
            assert!(m.naming(Some(bad)).is_err(), "{bad:?}");
        }
        let single = Manifest::parse(&EXAMPLE.replace("instances = true", "instances = false")).unwrap();
        assert_eq!(single.naming(Some("x")), Err(InstanceError::NotSupported));
        assert!(single.naming(None).is_ok());
    }

    const SS: &str = "\
tcp   LISTEN 0      4096         0.0.0.0:22        0.0.0.0:*
tcp   LISTEN 0      4096            [::]:22           [::]:*
udp   UNCONN 0      0       127.0.0.53%lo:53        0.0.0.0:*
tcp   LISTEN 0      511                *:8740            *:*
udp   UNCONN 0      0                  *:3478            *:*
";

    #[test]
    fn listening_ports_are_read_from_ss() {
        assert_eq!(listening_ports(SS), BTreeSet::from([22, 53, 3478, 8740]));
        assert!(listening_ports("State Recv-Q Send-Q Local Address:Port Peer Address:Port\n").is_empty());
        assert!(listening_ports("").is_empty());
    }

    #[test]
    fn a_busy_port_is_named_and_a_free_one_passes() {
        let listening = listening_ports(SS);
        let others = BTreeMap::from([("family".to_owned(), vec![9001, 9002])]);
        let busy = check_ports(&[8740], &listening, &others).unwrap_err();
        assert_eq!(busy, PortBusy { port: 8740, holder: PortHolder::Machine });
        assert!(busy.to_string().contains("8740"));
        let taken = check_ports(&[9002], &listening, &others).unwrap_err();
        assert_eq!(taken.holder, PortHolder::Instance("family".into()));
        assert!(taken.to_string().contains("family"));
        assert_eq!(check_ports(&[7000, 7000], &listening, &others).unwrap_err().holder, PortHolder::Form);
        assert!(check_ports(&[7000, 7001], &listening, &others).is_ok());
    }

    #[test]
    fn a_free_port_is_picked_from_the_wanted_one() {
        let listening = BTreeSet::from([8740, 8741]);
        let taken = BTreeSet::from([8742]);
        assert_eq!(pick_free(8740, &listening, &taken), Some(8743));
        assert_eq!(pick_free(9000, &listening, &taken), Some(9000));
        assert_eq!(pick_free(u16::MAX, &BTreeSet::from([u16::MAX]), &BTreeSet::new()), None);
    }
}
