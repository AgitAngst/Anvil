//! Задачи глазами окна: что пользователь может попросить, как это становится командой cargo,
//! и что делать, если собранный exe сейчас запущен.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::Preset;
use crate::jobs::{Before, Diag, JobId, Line, Outcome, Spec};
use crate::launch::{self, Launch};
use crate::registry::Meta;

/// Что можно попросить у проекта.
#[derive(Debug, Clone, PartialEq)]
pub enum Task {
    Build,
    Test,
    Clippy,
    Fmt,
    Clean,
    /// Собрать бинарник и запустить его.
    Run {
        bin: String,
        args: Vec<String>,
    },
}

impl Task {
    /// Собирает ли задача exe в папку профиля (и может упереться в запущенную программу).
    fn links_into(&self, release: bool) -> Option<bool> {
        match self {
            Task::Build | Task::Run { .. } => Some(release),
            Task::Test => Some(false),
            Task::Clippy | Task::Fmt | Task::Clean => None,
        }
    }
}

/// Задача в окне: что запущено, что вывело, чем кончилось.
pub struct Job {
    pub id: JobId,
    pub project: PathBuf,
    pub spec: Spec,
    pub lines: Vec<Line>,
    pub diags: Vec<Diag>,
    pub units: u32,
    pub started: Option<Instant>,
    pub finished: Option<(Outcome, Duration)>,
    /// Ключ для запоминания числа единиц сборки.
    pub units_key: String,
}

impl Job {
    pub fn running(&self) -> bool {
        self.started.is_some() && self.finished.is_none()
    }

    pub fn queued(&self) -> bool {
        self.started.is_none() && self.finished.is_none()
    }

    pub fn elapsed(&self) -> Duration {
        match (&self.finished, self.started) {
            (Some((_, took)), _) => *took,
            (None, Some(started)) => started.elapsed(),
            (None, None) => Duration::ZERO,
        }
    }

    pub fn errors(&self) -> usize {
        self.diags.iter().filter(|d| d.level == crate::jobs::Level::Error).count()
    }

    pub fn warnings(&self) -> usize {
        self.diags.len() - self.errors()
    }

    /// Доля хода, если число единиц известно по прошлому разу.
    pub fn progress(&self) -> Option<f32> {
        let total = self.spec.expected_units?;
        (total > 0).then(|| (self.units as f32 / total as f32).min(1.0))
    }
}

/// Запущенный exe мешает сборке.
#[derive(Debug, Clone)]
pub struct Locked {
    pub exe: PathBuf,
    pub pids: Vec<u32>,
}

/// Как поступить с занятым exe — выбирает пользователь.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolve {
    /// Переименовать занятый exe: программа доработает, сборка запишет новый.
    MoveAside,
    /// Закрыть программу и собрать.
    Stop,
    /// Собрать в отдельную папку `target/anvil`.
    SeparateDir,
}

/// Собрать команду cargo для задачи.
pub fn spec(project: &Path, meta: Option<&Meta>, task: &Task, release: bool, jobs: u32) -> Spec {
    let workspace = meta.is_some_and(|m| m.packages > 1);
    let mut args: Vec<String> = Vec::new();
    let mut json = true;
    let mut after = None;
    match task {
        Task::Build => {
            args.push("build".into());
            if workspace {
                args.push("--workspace".into());
            }
        }
        Task::Test => {
            args.push("test".into());
            if workspace {
                args.push("--workspace".into());
            }
        }
        Task::Clippy => {
            args.extend(["clippy".into(), "--all-targets".into()]);
            if workspace {
                args.push("--workspace".into());
            }
        }
        Task::Fmt => {
            args.extend(["fmt", "--all", "--", "--check"].map(String::from));
            json = false;
        }
        Task::Clean => {
            args.push("clean".into());
            json = false;
        }
        Task::Run { bin, args: run_args } => {
            args.push("build".into());
            if let Some(package) = meta.and_then(|m| m.bins.iter().find(|b| &b.name == bin)).map(|b| b.package.clone())
            {
                args.extend(["-p".into(), package]);
            }
            args.extend(["--bin".into(), bin.clone()]);
            let target = meta.map(|m| m.target_dir.clone()).unwrap_or_else(|| project.join("target"));
            after = Some(Launch {
                exe: launch::exe_path(&target, release, bin),
                args: run_args.clone(),
                dir: project.to_path_buf(),
            });
        }
    }
    if release && matches!(task, Task::Build | Task::Run { .. }) {
        args.push("--release".into());
    }
    if jobs > 0 && json && !matches!(task, Task::Clean) {
        args.extend(["-j".into(), jobs.to_string()]);
    }
    let title = format!("cargo {}", args.join(" "));
    if json {
        args.push("--message-format=json".into());
    }
    Spec {
        project: project.to_path_buf(),
        title,
        program: "cargo".into(),
        args,
        json,
        expected_units: None,
        before: Vec::new(),
        after,
    }
}

/// Какие собираемые exe этого проекта сейчас запущены из той папки, куда пойдёт сборка.
pub fn locked(meta: Option<&Meta>, task: &Task, release: bool, snapshot: &crate::procs::Snapshot) -> Vec<Locked> {
    let (Some(meta), Some(release)) = (meta, task.links_into(release)) else { return Vec::new() };
    let bins: Vec<&str> = match task {
        Task::Run { bin, .. } => vec![bin.as_str()],
        _ => meta.bins.iter().map(|b| b.name.as_str()).collect(),
    };
    bins.into_iter()
        .filter_map(|bin| {
            let exe = launch::exe_path(&meta.target_dir, release, bin);
            let pids = launch::running_from(snapshot, &exe);
            (!pids.is_empty()).then_some(Locked { exe, pids })
        })
        .collect()
}

/// Применить выбор пользователя к команде.
pub fn resolve(spec: &mut Spec, locked: &[Locked], how: Resolve, meta: Option<&Meta>, release: bool) {
    match how {
        Resolve::MoveAside => spec.before.extend(locked.iter().map(|l| Before::MoveAside(l.exe.clone()))),
        Resolve::Stop => spec.before.extend(locked.iter().flat_map(|l| l.pids.iter().map(|pid| Before::Stop(*pid)))),
        Resolve::SeparateDir => {
            let base = meta.map(|m| m.target_dir.clone()).unwrap_or_else(|| spec.project.join("target"));
            let separate = base.join("anvil");
            // `--target-dir` — до `--message-format`, чтобы заголовок совпадал с командой.
            let at = spec.args.iter().position(|a| a.starts_with("--message-format")).unwrap_or(spec.args.len());
            spec.args.insert(at, separate.to_string_lossy().into_owned());
            spec.args.insert(at, "--target-dir".into());
            spec.title = format!("{} --target-dir {}", spec.title, separate.display());
            if let Some(after) = &mut spec.after
                && let Some(name) = after.exe.file_name()
            {
                after.exe = separate.join(launch::profile_dir(release)).join(name);
            }
        }
    }
}

/// Что запускает главная кнопка: пресет, бинарник по имени или первый бинарник.
pub fn run_target(
    meta: Option<&Meta>,
    presets: &[Preset],
    chosen: Option<&str>,
) -> Option<(String, String, Vec<String>)> {
    let meta = meta?;
    if let Some(name) = chosen {
        if let Some(preset) = presets.iter().find(|p| p.name == name && meta.bins.iter().any(|b| b.name == p.bin)) {
            return Some((preset.name.clone(), preset.bin.clone(), launch::split_args(&preset.args)));
        }
        if meta.bins.iter().any(|b| b.name == name) {
            return Some((name.to_owned(), name.to_owned(), Vec::new()));
        }
    }
    meta.bins.first().map(|b| (b.name.clone(), b.name.clone(), Vec::new()))
}

/// Число единиц сборки прошлых запусков: ключ — проект и команда.
pub type UnitsCache = HashMap<String, u32>;

pub fn units_key(spec: &Spec) -> String {
    format!("{}|{}", spec.project.to_string_lossy().to_lowercase(), spec.args.join(" "))
}

pub fn load_units(path: &Path) -> UnitsCache {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_units(path: &Path, cache: &UnitsCache) {
    if let Ok(text) = serde_json::to_string(cache) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Bin;

    fn meta() -> Meta {
        Meta {
            packages: 3,
            bins: vec![
                Bin { name: "amber-desktop".into(), package: "amber-desktop".into() },
                Bin { name: "amber-server".into(), package: "amber-server".into() },
            ],
            target_dir: PathBuf::from(r"D:\p\target"),
            ..Meta::default()
        }
    }

    #[test]
    fn run_builds_one_bin_and_launches_it() {
        let m = meta();
        let task = Task::Run { bin: "amber-desktop".into(), args: vec!["--profile".into(), "t".into()] };
        let s = spec(Path::new(r"D:\p"), Some(&m), &task, true, 4);
        assert_eq!(s.title, "cargo build -p amber-desktop --bin amber-desktop --release -j 4");
        assert_eq!(s.args.last().map(String::as_str), Some("--message-format=json"));
        let after = s.after.unwrap();
        assert_eq!(after.exe, launch::exe_path(Path::new(r"D:\p\target"), true, "amber-desktop"));
        assert_eq!(after.args, ["--profile", "t"]);
    }

    #[test]
    fn workspace_build_and_fmt() {
        let m = meta();
        assert_eq!(spec(Path::new("p"), Some(&m), &Task::Build, false, 0).title, "cargo build --workspace");
        let fmt = spec(Path::new("p"), Some(&m), &Task::Fmt, false, 4);
        assert_eq!(fmt.title, "cargo fmt --all -- --check");
        assert!(!fmt.json);
    }

    #[test]
    fn separate_dir_moves_launch_too() {
        let m = meta();
        let task = Task::Run { bin: "amber-server".into(), args: Vec::new() };
        let mut s = spec(Path::new(r"D:\p"), Some(&m), &task, false, 0);
        resolve(&mut s, &[], Resolve::SeparateDir, Some(&m), false);
        let at = s.args.iter().position(|a| a == "--target-dir").unwrap();
        // Путь строится через join, как в коде: на Linux `\` — не разделитель.
        let separate = m.target_dir.join("anvil");
        assert_eq!(PathBuf::from(&s.args[at + 1]), separate);
        assert_eq!(s.args.last().map(String::as_str), Some("--message-format=json"));
        assert_eq!(s.after.unwrap().exe, launch::exe_path(&separate, false, "amber-server"));
    }

    #[test]
    fn run_target_prefers_chosen_preset_then_bin() {
        let m = meta();
        let presets =
            vec![Preset { name: "Тест".into(), bin: "amber-desktop".into(), args: "--profile uitest".into() }];
        let (label, bin, args) = run_target(Some(&m), &presets, Some("Тест")).unwrap();
        assert_eq!((label.as_str(), bin.as_str()), ("Тест", "amber-desktop"));
        assert_eq!(args, ["--profile", "uitest"]);
        assert_eq!(run_target(Some(&m), &presets, Some("amber-server")).unwrap().1, "amber-server");
        assert_eq!(run_target(Some(&m), &presets, None).unwrap().1, "amber-desktop");
        assert_eq!(run_target(Some(&m), &presets, Some("нет такого")).unwrap().1, "amber-desktop");
    }
}
