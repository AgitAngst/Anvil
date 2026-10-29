//! Готовая команда для машины: шаблон манифеста + значения формы + имена экземпляра.
//!
//! Строка, которую возвращает [`Manifest::command`], — ровно то, что уйдёт на машину. Её же показывают владельцу
//! дословно. Значения подставляются только как слова оболочки (см. [`crate::shell`]).

use std::collections::BTreeMap;

use crate::form::FormError;
use crate::instance::InstanceError;
use crate::manifest::{CommandKind, Manifest};
use crate::shell;
use crate::template;

/// Что известно вызывающему о запуске.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context<'a> {
    /// Имя экземпляра; `None` — основной. В `{instance}` основной даёт пустую строку `''`.
    pub instance: Option<&'a str>,
    /// Каталог на машине для `{dir}` (например, куда класть резервную копию).
    pub dir: Option<&'a str>,
}

/// Почему команда не собралась.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("the module has no `{0}` command")]
    NoCommand(&'static str),
    #[error("the module has no action number {0}")]
    NoAction(usize),
    #[error(transparent)]
    Instance(#[from] InstanceError),
    #[error(transparent)]
    Form(#[from] FormError),
    #[error("{0}")]
    Template(String),
}

impl Manifest {
    /// Команда контракта с подставленными значениями. `input` — значения формы (нужны установке; для остальных
    /// команд пусто).
    pub fn command(
        &self,
        kind: CommandKind,
        input: &BTreeMap<String, String>,
        ctx: &Context<'_>,
    ) -> Result<String, CommandError> {
        let tpl = self.commands.get(kind).ok_or(CommandError::NoCommand(kind.name()))?;
        let args = self.form_args(input)?;
        self.expand(tpl, args, ctx)
    }

    /// Команда действия № `index` (с нуля). Значений формы у действий нет: `{args}` пуст.
    pub fn action_command(&self, index: usize, ctx: &Context<'_>) -> Result<String, CommandError> {
        let action = self.actions.get(index).ok_or(CommandError::NoAction(index))?;
        self.expand(&action.cmd, String::new(), ctx)
    }

    fn expand(&self, tpl: &str, args: String, ctx: &Context<'_>) -> Result<String, CommandError> {
        let naming = self.naming(ctx.instance)?;
        if ctx.dir.is_some_and(|d| d.contains('\0')) {
            return Err(CommandError::Template("`{dir}` has a NUL byte".to_owned()));
        }
        template::render(tpl, |name| match name {
            "bin" => self.binary.as_deref().map(shell::word),
            "args" => Some(args.clone()),
            "service" => Some(shell::word(&naming.service)),
            "user" => Some(shell::word(&naming.user)),
            "instance" => Some(shell::word(naming.instance.as_deref().unwrap_or(""))),
            "dir" => ctx.dir.map(shell::quote),
            _ => None,
        })
        .map_err(CommandError::Template)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::EXAMPLE;

    fn manifest() -> Manifest {
        Manifest::parse(EXAMPLE).unwrap()
    }

    #[test]
    fn substitutions_make_the_command_verbatim() {
        let m = manifest();
        let none = BTreeMap::new();
        let ctx = Context::default();
        assert_eq!(
            m.command(CommandKind::Install, &none, &ctx).unwrap(),
            "/opt/example/example-server install --port 8740"
        );
        assert_eq!(m.command(CommandKind::Status, &none, &ctx).unwrap(), "/opt/example/example-server status --json");
        assert_eq!(m.action_command(0, &ctx).unwrap(), "systemctl restart example");
    }

    #[test]
    fn the_dir_and_the_instance_are_substituted_safely() {
        let m = manifest();
        let none = BTreeMap::new();
        let ctx = Context { instance: Some("family"), dir: Some("/root/back ups/it's") };
        assert_eq!(
            m.command(CommandKind::Backup, &none, &ctx).unwrap(),
            r"/opt/example/example-server backup --to '/root/back ups/it'\''s'"
        );
        assert_eq!(m.action_command(0, &ctx).unwrap(), "systemctl restart example-family");
        let err = m.command(CommandKind::Backup, &none, &Context::default()).unwrap_err();
        assert!(matches!(err, CommandError::Template(ref s) if s.contains("dir")), "{err}");
    }

    #[test]
    fn a_hostile_form_value_never_reaches_the_command() {
        let m = manifest();
        let hostile = BTreeMap::from([("port".to_owned(), "1; reboot".to_owned())]);
        assert!(m.command(CommandKind::Install, &hostile, &Context::default()).is_err());
    }

    #[test]
    fn a_missing_command_and_a_bad_instance_are_told_apart() {
        let m = manifest();
        let none = BTreeMap::new();
        assert_eq!(m.command(CommandKind::Update, &none, &Context::default()), Err(CommandError::NoCommand("update")));
        assert_eq!(m.action_command(5, &Context::default()), Err(CommandError::NoAction(5)));
        let ctx = Context { instance: Some("Bad Name"), dir: None };
        assert_eq!(m.command(CommandKind::Status, &none, &ctx), Err(CommandError::Instance(InstanceError::BadName)));
    }
}
