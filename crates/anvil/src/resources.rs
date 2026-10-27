//! Ресурсы ПК: место на дисках, размер сборок (`target`), SMART дисков. Только читает.

use std::path::{Path, PathBuf};

#[cfg(windows)]
use crate::run;

#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    pub letter: char,
    pub total: u64,
    pub free: u64,
}

/// Что Windows знает о физическом диске. Пороги не заданы — только показ.
#[derive(Debug, Clone, PartialEq)]
pub struct Smart {
    pub name: String,
    pub media: String,
    pub health: String,
    pub size: Option<u64>,
    /// Износ, процентов.
    pub wear: Option<u32>,
    /// Температура, °C.
    pub temp: Option<u32>,
    /// Часов работы.
    pub hours: Option<u64>,
    /// Ошибки чтения за всё время.
    pub errors: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Build {
    /// Папка проекта.
    pub project: PathBuf,
    pub target: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub disks: Vec<Disk>,
    pub smart: Result<Vec<Smart>, String>,
    /// По убыванию размера.
    pub builds: Vec<Build>,
    /// Когда прочитали (секунды UNIX).
    pub read: i64,
}

/// Что прочитать: буквы дисков и пары «проект → папка сборки».
#[derive(Debug, Clone, Default)]
pub struct Input {
    pub letters: Vec<char>,
    pub builds: Vec<(PathBuf, PathBuf)>,
}

pub fn read(input: &Input) -> Snapshot {
    let mut disks: Vec<Disk> = input
        .letters
        .iter()
        .filter_map(|l| disk_space(*l).map(|(total, free)| Disk { letter: l.to_ascii_uppercase(), total, free }))
        .collect();
    disks.sort_by_key(|d| d.letter);
    disks.dedup_by_key(|d| d.letter);
    let mut builds: Vec<Build> = input
        .builds
        .iter()
        .map(|(project, target)| Build { project: project.clone(), target: target.clone(), bytes: dir_size(target) })
        .collect();
    builds.sort_by_key(|b| std::cmp::Reverse(b.bytes));
    Snapshot { disks, smart: read_smart(), builds, read: crate::i18n::now() }
}

/// Буква диска пути (`E:\…` → `E`).
pub fn drive_letter(path: &Path) -> Option<char> {
    let text = path.to_string_lossy();
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => Some(letter.to_ascii_uppercase()),
        _ => None,
    }
}

/// Всего и свободно байт на диске.
#[cfg(windows)]
fn disk_space(letter: char) -> Option<(u64, u64)> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let root: Vec<u16> = format!("{}:\\", letter.to_ascii_uppercase()).encode_utf16().chain(Some(0)).collect();
    let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: `root` — строка с нулём на конце, три указателя — на живые локальные переменные.
    let ok = unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut available, &mut total, &mut free) };
    (ok != 0 && total > 0).then_some((total, available))
}

#[cfg(not(windows))]
fn disk_space(_: char) -> Option<(u64, u64)> {
    None
}

/// Размер папки со всем внутри; ссылки не разворачиваются.
pub fn dir_size(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

#[cfg(windows)]
fn read_smart() -> Result<Vec<Smart>, String> {
    let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ErrorActionPreference='SilentlyContinue'; \
        Get-PhysicalDisk | ForEach-Object { $c = $_ | Get-StorageReliabilityCounter; \
        [pscustomobject]@{ Name=$_.FriendlyName; Media=[string]$_.MediaType; Health=[string]$_.HealthStatus; Size=$_.Size; \
        Wear=$c.Wear; Temp=$c.Temperature; Hours=$c.PowerOnHours; Errors=$c.ReadErrorsTotal } } | ConvertTo-Json -Compress";
    let out = run::output("powershell", Path::new("."), &["-NoProfile", "-NonInteractive", "-Command", script])?;
    parse_smart(&out)
}

#[cfg(not(windows))]
fn read_smart() -> Result<Vec<Smart>, String> {
    Err("SMART is read on Windows only".to_owned())
}

/// Ответ `ConvertTo-Json`: один объект или массив; пустое — «нет данных».
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_smart(json: &str) -> Result<Vec<Smart>, String> {
    let text = json.trim().trim_start_matches('\u{feff}');
    if text.is_empty() {
        return Err("no data".to_owned());
    }
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let items = match value {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    };
    let text_of = |v: &serde_json::Value, key: &str| v.get(key).and_then(|x| x.as_str()).unwrap_or_default().to_owned();
    let number = |v: &serde_json::Value, key: &str| v.get(key).and_then(|x| x.as_f64()).map(|n| n.max(0.0) as u64);
    Ok(items
        .iter()
        .map(|v| Smart {
            name: text_of(v, "Name"),
            media: text_of(v, "Media"),
            health: text_of(v, "Health"),
            size: number(v, "Size"),
            wear: number(v, "Wear").map(|n| n as u32),
            temp: number(v, "Temp").map(|n| n as u32),
            hours: number(v, "Hours"),
            errors: number(v, "Errors"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_letters() {
        assert_eq!(drive_letter(Path::new(r"e:\dev\target")), Some('E'));
        assert_eq!(drive_letter(Path::new("/home/x")), None);
    }

    #[test]
    fn smart_one_object_and_array() {
        let one = r#"{"Name":"Disk A","Media":"SSD","Health":"Healthy","Size":1000204886016,"Wear":3,"Temp":41,"Hours":1200,"Errors":0}"#;
        let list = parse_smart(one).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            (list[0].name.as_str(), list[0].wear, list[0].temp, list[0].hours),
            ("Disk A", Some(3), Some(41), Some(1200))
        );
        let two = r#"[{"Name":"A","Media":"HDD","Health":"Healthy","Size":2,"Wear":null,"Temp":null,"Hours":null,"Errors":null},{"Name":"B"}]"#;
        let list = parse_smart(two).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!((list[0].wear, list[0].temp, list[1].media.as_str()), (None, None, ""));
        assert!(parse_smart("  ").is_err());
        assert!(parse_smart("не json").is_err());
    }

    #[test]
    fn sizes_of_folders() {
        let dir = std::env::temp_dir().join(format!("anvil-res-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a").join("b")).unwrap();
        std::fs::write(dir.join("one.bin"), vec![0u8; 1000]).unwrap();
        std::fs::write(dir.join("a").join("b").join("two.bin"), vec![0u8; 234]).unwrap();
        assert_eq!(dir_size(&dir), 1234);
        assert_eq!(dir_size(&dir.join("missing")), 0);
        let snapshot = read(&Input { letters: vec![], builds: vec![(PathBuf::from("p"), dir.clone())] });
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(snapshot.builds[0].bytes, 1234);
    }

    #[cfg(windows)]
    #[test]
    fn the_system_drive_has_space() {
        let letter = std::env::var("SystemDrive").ok().and_then(|d| d.chars().next()).unwrap_or('C');
        let (total, free) = disk_space(letter).expect("system drive");
        assert!(total > 0 && free <= total);
        assert!(disk_space('#').is_none());
    }
}
