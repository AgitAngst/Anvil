//! Запуск внешних команд (git, cargo) без мелькающего окна консоли.

use std::path::Path;
use std::process::{Command, Stdio};

/// Команда, которая не открывает окно консоли и никогда не ждёт ввода.
pub fn command(program: &str, dir: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir).stdin(Stdio::null());
    // git не должен спрашивать пароль в невидимой консоли — пусть лучше честно упадёт.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Запустить и вернуть stdout. Ненулевой код выхода — ошибка с текстом stderr.
pub fn output(program: &str, dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = command(program, dir).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().find(|l| !l.trim().is_empty()).unwrap_or("error").trim().to_owned())
    }
}
