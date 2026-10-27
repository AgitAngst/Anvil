//! Ход работы: планы долгих задач и сессии Claude. Только читает.
//!
//! Планы пишет скилл `progress` в `~/.claude/progress/<проект>.json` (этапы, оценки, состояния). Сессии — файлы
//! `~/.claude/projects/<папка>/<id>.jsonl`: нужны время правки и первая реплика владельца.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

impl StageState {
    fn parse(text: &str) -> StageState {
        match text {
            "running" => StageState::Running,
            "done" => StageState::Done,
            "failed" => StageState::Failed,
            "skipped" => StageState::Skipped,
            _ => StageState::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    pub ru: String,
    pub en: String,
    /// Оценка, минут.
    pub est: f64,
    pub state: StageState,
    pub start: Option<f64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub project: String,
    pub ru: String,
    pub en: String,
    pub stages: Vec<Stage>,
    pub finished: bool,
    /// Когда план создан (секунды UNIX): новый план того же проекта — другая задача.
    pub created: f64,
    /// Когда файл правили в последний раз (секунды UNIX).
    pub updated: f64,
}

impl Plan {
    pub fn title(&self, english: bool) -> &str {
        let text = if english { &self.en } else { &self.ru };
        if text.is_empty() { &self.project } else { text }
    }

    /// Доля сделанного по оценкам этапов; идущий этап считается по времени, но не больше 90 %.
    pub fn fraction(&self, now: f64) -> f32 {
        let total: f64 = self.stages.iter().filter(|s| s.state != StageState::Skipped).map(|s| s.est.max(1.0)).sum();
        if total <= 0.0 {
            return if self.finished { 1.0 } else { 0.0 };
        }
        let done: f64 = self
            .stages
            .iter()
            .map(|s| {
                let est = s.est.max(1.0);
                match s.state {
                    StageState::Done => est,
                    StageState::Running => {
                        let spent = s.start.map_or(0.0, |t| ((now - t) / 60.0).max(0.0));
                        est * (spent / est).min(0.9)
                    }
                    _ => 0.0,
                }
            })
            .sum();
        ((done / total) as f32).clamp(0.0, 1.0)
    }

    /// Сколько минут осталось: оценки того, что не сделано, минус уже потраченное на идущий этап.
    pub fn remaining_minutes(&self, now: f64) -> f64 {
        self.stages
            .iter()
            .map(|s| match s.state {
                StageState::Pending => s.est.max(1.0),
                StageState::Running => {
                    let spent = s.start.map_or(0.0, |t| ((now - t) / 60.0).max(0.0));
                    (s.est.max(1.0) - spent).max(0.0)
                }
                _ => 0.0,
            })
            .sum()
    }

    /// Тот же план: у проекта то же время создания.
    pub fn created_same(&self, other: &Plan) -> bool {
        (self.created - other.created).abs() < 1.0
    }

    pub fn failed(&self) -> bool {
        self.stages.iter().any(|s| s.state == StageState::Failed)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// Папка проекта в `~/.claude/projects`, читаемо.
    pub project: String,
    /// Первая реплика владельца, до одной строки.
    pub title: String,
    /// Когда правили файл (секунды UNIX).
    pub modified: i64,
    /// Правили только что — сессия, похоже, работает.
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub plans: Vec<Plan>,
    pub sessions: Vec<Session>,
    /// Когда прочитали (секунды UNIX).
    pub read: i64,
}

/// Сессия идёт, если файл правили не позже этого.
const ACTIVE_FOR: Duration = Duration::from_secs(120);
/// Сессии старше суток не показываем.
const SESSIONS_FOR: Duration = Duration::from_secs(24 * 3600);
const MAX_SESSIONS: usize = 25;
/// Сколько читать с начала файла сессии в поисках первой реплики.
const HEAD_BYTES: u64 = 128 * 1024;

/// Папка `.claude` владельца: `%USERPROFILE%\.claude`.
pub fn claude_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".claude"))
}

pub fn read(claude: &Path) -> Snapshot {
    Snapshot {
        plans: read_plans(&claude.join("progress")),
        sessions: read_sessions(&claude.join("projects")),
        read: crate::i18n::now(),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawPlan {
    project: String,
    title: serde_json::Value,
    stages: Vec<RawStage>,
    finished: bool,
    created: f64,
    updated: f64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawStage {
    ru: String,
    en: String,
    est: f64,
    state: String,
    start: Option<f64>,
    note: Option<String>,
}

fn title_of(value: &serde_json::Value, key: &str) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(map) => map.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_owned(),
        _ => String::new(),
    }
}

/// Разобрать файл плана; чужой или испорченный файл — `None`.
pub fn parse_plan(text: &str) -> Option<Plan> {
    let raw: RawPlan = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    if raw.project.is_empty() {
        return None;
    }
    Some(Plan {
        ru: title_of(&raw.title, "ru"),
        en: title_of(&raw.title, "en"),
        project: raw.project,
        finished: raw.finished,
        created: raw.created,
        updated: raw.updated,
        stages: raw
            .stages
            .into_iter()
            .map(|s| Stage {
                ru: s.ru,
                en: s.en,
                est: s.est,
                state: StageState::parse(&s.state),
                start: s.start,
                note: s.note.filter(|n| !n.trim().is_empty()),
            })
            .collect(),
    })
}

fn read_plans(dir: &Path) -> Vec<Plan> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut plans: Vec<Plan> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| parse_plan(&std::fs::read_to_string(p).ok()?))
        .collect();
    // Идущие — сверху, потом по времени правки.
    plans.sort_by(|a, b| a.finished.cmp(&b.finished).then(b.updated.total_cmp(&a.updated)));
    plans
}

fn read_sessions(dir: &Path) -> Vec<Session> {
    let Ok(folders) = std::fs::read_dir(dir) else { return Vec::new() };
    let now = SystemTime::now();
    let mut files: Vec<(SystemTime, PathBuf, String)> = Vec::new();
    for folder in folders.flatten() {
        let label = folder_label(&folder.file_name().to_string_lossy());
        let Ok(inner) = std::fs::read_dir(folder.path()) else { continue };
        for file in inner.flatten() {
            let path = file.path();
            if !path.extension().is_some_and(|e| e == "jsonl") {
                continue;
            }
            let Ok(modified) = file.metadata().and_then(|m| m.modified()) else { continue };
            if now.duration_since(modified).is_ok_and(|age| age <= SESSIONS_FOR) {
                files.push((modified, path, label.clone()));
            }
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.truncate(MAX_SESSIONS);
    files
        .into_iter()
        .map(|(modified, path, project)| Session {
            title: first_prompt(&path).unwrap_or_default(),
            active: now.duration_since(modified).is_ok_and(|age| age <= ACTIVE_FOR),
            modified: modified.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64),
            project,
        })
        .collect()
}

/// `E--dev-personal-Demo` → `Demo`; `D--dev-personal` → `dev_personal`; рабочее дерево — со словом (worktree).
pub fn folder_label(name: &str) -> String {
    let (base, worktree) = match name.split_once("--claude-worktrees") {
        Some((base, _)) => (base, true),
        None => (name, false),
    };
    let tail = base.rsplit_once("dev-personal").map_or(base, |(_, rest)| rest).trim_matches('-');
    let label = if tail.is_empty() { "dev_personal".to_owned() } else { tail.to_owned() };
    if worktree { format!("{label} (worktree)") } else { label }
}

/// Первая реплика владельца: из начала файла сессии, одной строкой.
fn first_prompt(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = Vec::new();
    std::fs::File::open(path).ok()?.take(HEAD_BYTES).read_to_end(&mut head).ok()?;
    let text = String::from_utf8_lossy(&head);
    text.lines().find_map(prompt_of_line)
}

/// Текст реплики владельца из строки журнала; служебные (в угловых скобках) и пустые пропускаются.
pub fn prompt_of_line(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "user" {
        return None;
    }
    let content = value.get("message")?.get("content")?;
    let text = match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => {
            parts.iter().find_map(|p| p.get("text").and_then(|t| t.as_str()))?.to_owned()
        }
        _ => return None,
    };
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    if line.starts_with('<') {
        return None;
    }
    Some(line.chars().take(90).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = r#"{"project":"demo","title":{"ru":"Задача","en":"Task"},"created":1.0,"finished":false,"updated":100.0,
        "stages":[
          {"key":"a","ru":"Один","en":"One","est":10.0,"state":"done","start":0.0,"note":null},
          {"key":"b","ru":"Два","en":"Two","est":30.0,"state":"running","start":0.0,"note":"в работе"},
          {"key":"c","ru":"Три","en":"Three","est":20.0,"state":"pending","start":null,"note":null},
          {"key":"d","ru":"Четыре","en":"Four","est":50.0,"state":"skipped","start":null,"note":null}],"roadmap":null}"#;

    #[test]
    fn plan_parses_and_counts() {
        let plan = parse_plan(PLAN).unwrap();
        assert_eq!((plan.project.as_str(), plan.title(true), plan.title(false)), ("demo", "Task", "Задача"));
        assert_eq!(plan.stages.len(), 4);
        assert_eq!(plan.stages[1].note.as_deref(), Some("в работе"));
        // 10 мин сделано; идущий этап на 15 минутах из 30 (половина); пропущенный не считается: (10 + 15) / 60.
        let f = plan.fraction(15.0 * 60.0);
        assert!((f - 25.0 / 60.0).abs() < 1e-4, "{f}");
        // Осталось: 15 на идущем + 20 впереди.
        assert!((plan.remaining_minutes(15.0 * 60.0) - 35.0).abs() < 1e-6);
        // Идущий этап не засчитывается целиком, пока не закончен.
        assert!(plan.fraction(10_000.0 * 60.0) < 1.0);
        assert!(!plan.failed());
    }

    #[test]
    fn broken_and_foreign_files_are_skipped() {
        assert!(parse_plan("not json").is_none());
        assert!(parse_plan("{}").is_none());
        assert!(parse_plan(r#"{"project":"x","title":"Просто строка"}"#).is_some_and(|p| p.ru == "Просто строка"));
    }

    #[test]
    fn folder_labels() {
        assert_eq!(folder_label("E--dev-personal-Demo"), "Demo");
        assert_eq!(folder_label("D--dev-personal"), "dev_personal");
        assert_eq!(folder_label("E--dev-personal-Demo--claude-worktrees-calm-fox-1"), "Demo (worktree)");
    }

    #[test]
    fn prompts() {
        let user = r#"{"type":"user","message":{"role":"user","content":"Сделай   этап\nвторая строка"}}"#;
        assert_eq!(prompt_of_line(user).as_deref(), Some("Сделай   этап"));
        let parts = r#"{"type":"user","message":{"content":[{"type":"text","text":"  Привет"}]}}"#;
        assert_eq!(prompt_of_line(parts).as_deref(), Some("Привет"));
        assert_eq!(
            prompt_of_line(r#"{"type":"user","message":{"content":"<system-reminder>x</system-reminder>"}}"#),
            None
        );
        assert_eq!(prompt_of_line(r#"{"type":"assistant","message":{"content":"x"}}"#), None);
        assert_eq!(prompt_of_line("мусор"), None);
    }

    #[test]
    fn reads_a_claude_folder() {
        let dir = std::env::temp_dir().join(format!("anvil-work-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("progress")).unwrap();
        std::fs::create_dir_all(dir.join("projects").join("E--dev-personal-Demo")).unwrap();
        std::fs::write(dir.join("progress").join("demo.json"), PLAN).unwrap();
        std::fs::write(dir.join("progress").join("junk.json"), "x").unwrap();
        std::fs::write(
            dir.join("projects").join("E--dev-personal-Demo").join("s.jsonl"),
            "{\"type\":\"queue-operation\"}\n{\"type\":\"user\",\"message\":{\"content\":\"Первая реплика\"}}\n",
        )
        .unwrap();
        let snap = read(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(snap.plans.len(), 1);
        assert_eq!(snap.sessions.len(), 1);
        assert_eq!((snap.sessions[0].project.as_str(), snap.sessions[0].title.as_str()), ("Demo", "Первая реплика"));
        assert!(snap.sessions[0].active);
    }
}
