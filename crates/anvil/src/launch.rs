//! Запуск собранных программ, их остановка и занятые exe.
//!
//! Запущенный exe Windows не даёт перезаписать, но разрешает переименовать. Поэтому сборка,
//! которой мешает работающая программа, может отодвинуть старый exe (`name.exe` →
//! `name.exe.old-<время>`), а программа доработает из переименованного файла.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::procs::{self, Snapshot};
use crate::run;

/// Что запустить после сборки.
#[derive(Debug, Clone)]
pub struct Launch {
    pub exe: PathBuf,
    pub args: Vec<String>,
    /// Рабочая папка — корень проекта.
    pub dir: PathBuf,
}

pub const EXE_SUFFIX: &str = std::env::consts::EXE_SUFFIX;

/// Где cargo положит бинарник: `<target>/<debug|release>/<name>.exe`.
pub fn exe_path(target_dir: &Path, release: bool, bin: &str) -> PathBuf {
    target_dir.join(profile_dir(release)).join(format!("{bin}{EXE_SUFFIX}"))
}

pub fn profile_dir(release: bool) -> &'static str {
    if release { "release" } else { "debug" }
}

/// Запустить программу отдельно от Anvil. Консольная получает своё окно, оконная — просто стартует.
pub fn start(launch: &Launch) -> Result<u32, String> {
    let mut cmd = Command::new(&launch.exe);
    cmd.args(&launch.args).current_dir(&launch.dir).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NEW_CONSOLE | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn().map(|child| child.id()).map_err(|e| e.to_string())
}

/// Попросить программу закрыться (как крестиком окна); не закрылась за 5 с — остановить.
pub fn stop(pid: u32, dir: &Path) -> Result<(), String> {
    let pid_text = pid.to_string();
    #[cfg(windows)]
    {
        let _ = run::command("taskkill", dir).args(["/PID", &pid_text]).output();
        if procs::wait_exit(pid, Duration::from_secs(5)) {
            return Ok(());
        }
        run::output("taskkill", dir, &["/F", "/PID", &pid_text]).map(|_| ())?;
        if procs::wait_exit(pid, Duration::from_secs(3)) { Ok(()) } else { Err(format!("PID {pid}: still running")) }
    }
    #[cfg(not(windows))]
    {
        let _ = run::command("kill", dir).arg(&pid_text).output();
        if procs::wait_exit(pid, Duration::from_secs(5)) {
            return Ok(());
        }
        run::output("kill", dir, &["-9", &pid_text]).map(|_| ())
    }
}

/// Отодвинуть занятый exe: переименовать рядом, чтобы сборка записала новый.
pub fn move_aside(exe: &Path) -> Result<PathBuf, String> {
    let stamp = crate::i18n::now();
    let name = exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let to = exe.with_file_name(format!("{name}.old-{stamp}"));
    std::fs::rename(exe, &to).map_err(|e| e.to_string())?;
    Ok(to)
}

/// Убрать отодвинутые exe, которые больше никто не держит. Занятые остаются до следующего раза.
pub fn clean_moved(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains(&format!("{EXE_SUFFIX}.old-")) || (EXE_SUFFIX.is_empty() && name.contains(".old-")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Запущенные экземпляры этого exe: процессы с тем же именем, чей файл лежит по этому пути.
pub fn running_from(snapshot: &Snapshot, exe: &Path) -> Vec<u32> {
    let Some(stem) = exe.file_stem() else { return Vec::new() };
    let key = procs::key(&stem.to_string_lossy());
    snapshot
        .get(&key)
        .map(|list| {
            list.iter()
                .filter(|r| r.path.as_deref().is_some_and(|p| crate::registry::same_dir(p, exe)))
                .map(|r| r.pid)
                .collect()
        })
        .unwrap_or_default()
}

/// Аргументы строкой → список: пробелы делят, кавычки склеивают (`--profile "my test"`).
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut has = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has {
                    args.push(std::mem::take(&mut current));
                    has = false;
                }
            }
            c => {
                current.push(c);
                has = true;
            }
        }
    }
    if has {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_split_on_spaces_and_keep_quotes_together() {
        assert_eq!(split_args(r#"--profile "my test"  -v"#), ["--profile", "my test", "-v"]);
        assert_eq!(split_args(r#"--name """#), ["--name", ""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn moved_exe_is_renamed_and_cleaned() {
        let dir = std::env::temp_dir().join(format!("anvil-launch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(format!("app{EXE_SUFFIX}"));
        std::fs::write(&exe, b"old").unwrap();
        let moved = move_aside(&exe).unwrap();
        assert!(!exe.exists() && moved.exists());
        clean_moved(&dir);
        assert!(!moved.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
