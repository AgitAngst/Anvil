//! Выгрузить плитки всех узоров в PNG — для клиентов не на Rust, макетов и
//! страниц. Без зависимостей: PNG собирается здесь же. Сжатие простое —
//! фильтр Sub и повторы байтов (фон плитки ровный), плитка выходит в 10–30 КБ.
//!
//! ```text
//! cargo run -p anvil-wallpaper --example export -- [ПАПКА] [--scale 2] [--bg EEE5D3 --ink 6E5A3A]
//! ```
//!
//! Без `--bg`/`--ink` — по две плитки на узор, цветами фона бесед Amber:
//! `<узор>-light@<N>x.png` и `<узор>-dark@<N>x.png`. С ними — одна плитка
//! `<узор>@<N>x.png` этими цветами. Папка по умолчанию — `target/wallpapers`.

use std::path::PathBuf;

use anvil_wallpaper::{Pattern, tile};

/// Фон бесед и узор у Amber: светлая и тёмная тема (как в `Theme.kt` телефона).
const LIGHT: ([u8; 3], [u8; 3]) = ([0xEE, 0xE5, 0xD3], [0x6E, 0x5A, 0x3A]);
const DARK: ([u8; 3], [u8; 3]) = ([0x12, 0x10, 0x0E], [0xCD, 0xBF, 0xA8]);

fn main() {
    let mut args = std::env::args().skip(1);
    let mut dir = PathBuf::from("target/wallpapers");
    let mut scale = 2.0_f32;
    let (mut bg, mut ink) = (None, None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scale" => {
                scale = args.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| fail("--scale needs a number"))
            }
            "--bg" => bg = Some(hex(&args.next().unwrap_or_default())),
            "--ink" => ink = Some(hex(&args.next().unwrap_or_default())),
            "-h" | "--help" => {
                println!("usage: export [FOLDER] [--scale N] [--bg RRGGBB --ink RRGGBB]");
                return;
            }
            other => dir = PathBuf::from(other),
        }
    }
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| fail(&format!("cannot create {}: {e}", dir.display())));
    let colors: Vec<(String, [u8; 3], [u8; 3])> = match (bg, ink) {
        (Some(bg), Some(ink)) => vec![(String::new(), bg, ink)],
        (None, None) => vec![("-light".into(), LIGHT.0, LIGHT.1), ("-dark".into(), DARK.0, DARK.1)],
        _ => fail("--bg and --ink go together"),
    };
    let mark = if scale.fract() == 0.0 { format!("{}", scale as u32) } else { format!("{scale}") };
    for pattern in Pattern::ALL.into_iter().filter(|p| *p != Pattern::Plain) {
        for (suffix, bg, ink) in &colors {
            let (side, rgba) = tile(pattern, scale, *bg, *ink);
            let path = dir.join(format!("{}{suffix}@{mark}x.png", pattern.code()));
            std::fs::write(&path, png(side, &rgba))
                .unwrap_or_else(|e| fail(&format!("cannot write {}: {e}", path.display())));
            println!("{}", path.display());
        }
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2)
}

fn hex(text: &str) -> [u8; 3] {
    let text = text.trim_start_matches('#');
    let value =
        u32::from_str_radix(text, 16).ok().filter(|_| text.len() == 6).unwrap_or_else(|| fail("colours are RRGGBB"));
    [(value >> 16) as u8, (value >> 8) as u8, value as u8]
}

/// PNG из RGBA построчно: IHDR, IDAT (zlib), IEND.
fn png(side: usize, rgba: &[u8]) -> Vec<u8> {
    // Фильтр Sub: байт минус тот же байт соседнего пикселя слева — ровный фон
    // становится нулями, а их сжимают повторы.
    let mut raw = Vec::with_capacity(rgba.len() + side);
    for row in rgba.chunks(side * 4) {
        raw.push(1);
        raw.extend(row.iter().enumerate().map(|(i, b)| if i < 4 { *b } else { b.wrapping_sub(row[i - 4]) }));
    }
    let mut zlib = vec![0x78, 0x01];
    zlib.extend(deflate(&raw));
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&(side as u32).to_be_bytes());
    header.extend_from_slice(&(side as u32).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 бит, RGBA, без чересстрочности
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Deflate одним блоком с постоянными кодами Хаффмана: байты как есть, а
/// повтор предыдущего байта (3–258 штук) — одной ссылкой на расстояние 1.
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.put(1, 1); // последний блок
    bits.put(1, 2); // постоянные коды
    let mut i = 0;
    while i < data.len() {
        let run = if i > 0 { data[i..].iter().take(258).take_while(|b| **b == data[i - 1]).count() } else { 0 };
        if run >= 3 {
            let (code, base, extra) = LENGTHS.iter().rev().find(|(_, base, _)| *base as usize <= run).copied().unwrap();
            symbol(&mut bits, code);
            bits.put((run - base as usize) as u32, extra);
            bits.code(0, 5); // расстояние 1
            i += run;
        } else {
            symbol(&mut bits, data[i] as u16);
            i += 1;
        }
    }
    symbol(&mut bits, 256);
    bits.finish()
}

/// Коды длин повтора: (код, наименьшая длина, лишних битов).
const LENGTHS: [(u16, u16, u32); 29] = [
    (257, 3, 0),
    (258, 4, 0),
    (259, 5, 0),
    (260, 6, 0),
    (261, 7, 0),
    (262, 8, 0),
    (263, 9, 0),
    (264, 10, 0),
    (265, 11, 1),
    (266, 13, 1),
    (267, 15, 1),
    (268, 17, 1),
    (269, 19, 2),
    (270, 23, 2),
    (271, 27, 2),
    (272, 31, 2),
    (273, 35, 3),
    (274, 43, 3),
    (275, 51, 3),
    (276, 59, 3),
    (277, 67, 4),
    (278, 83, 4),
    (279, 99, 4),
    (280, 115, 4),
    (281, 131, 5),
    (282, 163, 5),
    (283, 195, 5),
    (284, 227, 5),
    (285, 258, 0),
];

/// Символ постоянного кода Хаффмана (RFC 1951, 3.2.6).
fn symbol(bits: &mut Bits, value: u16) {
    let value = value as u32;
    match value {
        0..=143 => bits.code(0x30 + value, 8),
        144..=255 => bits.code(0x190 + value - 144, 9),
        256..=279 => bits.code(value - 256, 7),
        _ => bits.code(0xC0 + value - 280, 8),
    }
}

/// Биты потока deflate: данные — младшим битом вперёд, коды Хаффмана — старшим.
#[derive(Default)]
struct Bits {
    out: Vec<u8>,
    acc: u32,
    count: u32,
}

impl Bits {
    fn put(&mut self, value: u32, count: u32) {
        self.acc |= value << self.count;
        self.count += count;
        while self.count >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.count -= 8;
        }
    }

    fn code(&mut self, code: u32, len: u32) {
        let reversed = (0..len).fold(0, |r, i| (r << 1) | ((code >> i) & 1));
        self.put(reversed, len);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in bytes {
        a = (a + *byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}
