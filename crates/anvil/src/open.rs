//! Быстрые переходы: папка в проводнике, терминал в папке, VS Code.
//! Всё запускается отдельно от Anvil и не ждётся.

use std::path::Path;

use crate::run;

/// Открыть папку в проводнике.
pub fn folder(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let result = run::command("explorer.exe", path).arg(path).spawn();
    #[cfg(not(windows))]
    let result = run::command("xdg-open", path).arg(path).spawn();
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Терминал в папке: Windows Terminal, если он есть, иначе обычная консоль.
pub fn terminal(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        if run::command("wt.exe", path).arg("-d").arg(path).spawn().is_ok() {
            return Ok(());
        }
        // `start` открывает консоли собственное окно, а сам cmd окна не показывает.
        run::command("cmd.exe", path)
            .args(["/C", "start", "", "cmd.exe"])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        run::command("x-terminal-emulator", path).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Открыть папку или файл в VS Code (`code` из PATH).
pub fn code(target: &Path, dir: &Path) -> Result<(), String> {
    // На Windows `code` — это code.cmd, поэтому через cmd.
    #[cfg(windows)]
    let result = run::command("cmd.exe", dir).arg("/C").arg("code").arg(target).spawn();
    #[cfg(not(windows))]
    let result = run::command("code", dir).arg(target).spawn();
    result.map(|_| ()).map_err(|e| e.to_string())
}
