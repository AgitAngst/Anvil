//! Узоры на фоне бесед — как обои у Telegram, только свои и неброские. Одни на
//! все программы и клиенты: плитку рисует этот крейт, клиент только повторяет её.
//! Зависимостей нет — его берут и ядро телефона (через UniFFI), и egui-программы
//! (`anvil_ui::wallpaper` делает из плитки текстуру). Правила и как взять узор в
//! свой клиент — docs/WALLPAPERS.md.
//!
//! Плитка рисуется один раз, в памяти, из простых фигур. Фигуры считаются через
//! расстояние до края, поэтому края гладкие, а у краёв плитки фигуры
//! заворачиваются на другую сторону — швов нет. Плитка непрозрачная: в неё сразу
//! запечён и фон, и узор поверх него. Полупрозрачную отрисовщик egui смешивал
//! по-своему, и узор с малой прозрачностью пропадал совсем.
//!
//! Первые пять узоров перенесены из `amber_core::wallpaper` бит в бит (тест
//! `the_first_five_are_the_same_as_in_amber_bit_for_bit`): у кого фон уже выбран,
//! ничего не сдвинется.

#![forbid(unsafe_code)]

/// Сторона плитки в точках экрана (в dp на телефоне).
pub const TILE: f32 = 192.0;

/// Насколько узор проступает сквозь фон в самых плотных местах.
pub const STRENGTH: f32 = 0.13;

/// Какой узор. «Янтарь» — тот, что был у десктопа Amber с самого начала.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pattern {
    /// Колечки, точки, искорки и сердечки.
    Amber,
    /// Звёзды, месяцы и искорки.
    Stars,
    /// Треугольники, ромбы, крестики и кольца.
    Geometry,
    /// Листья и цветы.
    Garden,
    /// Волны и пузырьки.
    Waves,
    /// Шестиугольники ячеек — контуром и залитые.
    Honeycomb,
    /// Снежинки, снежинки поменьше и крупа.
    Snow,
    /// Планеты с кольцами, звёзды и искорки.
    Space,
    /// Кошачьи лапки и маленькие сердечки.
    Paws,
    /// Полоски, квадратики, завитки и кружки вразброс.
    Confetti,
    /// Ровная сетка точек, как бумага «в точку».
    Dots,
    /// Без узора — только фон.
    Plain,
}

impl Pattern {
    /// Все узоры в том порядке, в каком их показывают в выборе; «Без узора» — последним.
    pub const ALL: [Pattern; 12] = [
        Pattern::Amber,
        Pattern::Stars,
        Pattern::Geometry,
        Pattern::Garden,
        Pattern::Waves,
        Pattern::Honeycomb,
        Pattern::Snow,
        Pattern::Space,
        Pattern::Paws,
        Pattern::Confetti,
        Pattern::Dots,
        Pattern::Plain,
    ];

    /// Как узор хранится в настройках.
    pub fn code(self) -> &'static str {
        match self {
            Pattern::Amber => "amber",
            Pattern::Stars => "stars",
            Pattern::Geometry => "geometry",
            Pattern::Garden => "garden",
            Pattern::Waves => "waves",
            Pattern::Honeycomb => "honeycomb",
            Pattern::Snow => "snow",
            Pattern::Space => "space",
            Pattern::Paws => "paws",
            Pattern::Confetti => "confetti",
            Pattern::Dots => "dots",
            Pattern::Plain => "plain",
        }
    }

    /// Незнакомое имя — «Янтарь»: узор из более новой версии не ломает фон.
    pub fn from_code(code: &str) -> Pattern {
        Pattern::ALL.into_iter().find(|p| p.code() == code).unwrap_or(Pattern::Amber)
    }

    /// Название для людей по-русски — клиенту без своего словаря не нужно его выдумывать.
    pub fn title_ru(self) -> &'static str {
        match self {
            Pattern::Amber => "Янтарь",
            Pattern::Stars => "Звёзды",
            Pattern::Geometry => "Геометрия",
            Pattern::Garden => "Сад",
            Pattern::Waves => "Волны",
            Pattern::Honeycomb => "Соты",
            Pattern::Snow => "Снег",
            Pattern::Space => "Космос",
            Pattern::Paws => "Лапки",
            Pattern::Confetti => "Конфетти",
            Pattern::Dots => "В точку",
            Pattern::Plain => "Без узора",
        }
    }

    /// Название для людей по-английски.
    pub fn title_en(self) -> &'static str {
        match self {
            Pattern::Amber => "Amber",
            Pattern::Stars => "Stars",
            Pattern::Geometry => "Geometry",
            Pattern::Garden => "Garden",
            Pattern::Waves => "Waves",
            Pattern::Honeycomb => "Honeycomb",
            Pattern::Snow => "Snow",
            Pattern::Space => "Space",
            Pattern::Paws => "Paws",
            Pattern::Confetti => "Confetti",
            Pattern::Dots => "Dotted",
            Pattern::Plain => "Plain",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Shape {
    Ring { r: f32, width: f32 },
    Dot { r: f32 },
    Spark { r: f32 },
    Heart { r: f32 },
    Star { r: f32 },
    Moon { r: f32 },
    Triangle { r: f32, width: f32 },
    Diamond { r: f32, width: f32 },
    Plus { r: f32, width: f32 },
    Leaf { r: f32 },
    Flower { r: f32 },
    Wave { half: f32, amp: f32, width: f32 },
    Hex { r: f32, width: f32 },
    HexFill { r: f32 },
    Flake { r: f32, width: f32 },
    Planet { r: f32, width: f32 },
    Paw { r: f32 },
    Strip { w: f32, h: f32 },
    HeartExact { r: f32 },
}

/// Фигура на плитке: где, какая и насколько повёрнута (радианы).
#[derive(Debug, Clone, Copy)]
struct Placed {
    center: (f32, f32),
    shape: Shape,
    turn: f32,
}

/// Плитка `side × side` пикселей, RGBA построчно: фон `bg` и узор цветом `ink`
/// поверх. `scale` — пикселей на точку: у телефона это плотность экрана. Одна и
/// та же при каждом запуске: фигуры расставлены не случайно, а по счётчику.
pub fn tile(pattern: Pattern, scale: f32, bg: [u8; 3], ink: [u8; 3]) -> (usize, Vec<u8>) {
    let scale = scale.clamp(0.5, 8.0);
    let side = (TILE * scale).round() as usize;
    let shapes = layout(pattern);
    let mut rgba = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        for x in 0..side {
            let point = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
            let covered = shapes
                .iter()
                .map(|placed| coverage(placed, wrapped(point, placed.center), scale))
                .fold(0.0_f32, f32::max);
            let [r, g, b] = mix(bg, ink, covered * STRENGTH);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    (side, rgba)
}

/// Цвет между `bg` и `ink` — в гамме, как `Color32::lerp_to_gamma` у egui.
pub fn mix(bg: [u8; 3], ink: [u8; 3], t: f32) -> [u8; 3] {
    let one = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t + 0.5).floor().clamp(0.0, 255.0) as u8;
    [one(bg[0], ink[0]), one(bg[1], ink[1]), one(bg[2], ink[2])]
}

/// Ровно выверенный генератор без зависимостей: одна и та же последовательность
/// на любой машине.
struct Counter(u32);

impl Counter {
    fn next(&mut self) -> f32 {
        let n = &mut self.0;
        *n ^= *n << 13;
        *n ^= *n >> 17;
        *n ^= *n << 5;
        (*n % 1000) as f32 / 1000.0
    }
}

/// Фигуры плитки. Решётка 4×4 со сдвигом каждой второй строки и небольшим
/// разбросом по счётчику — глазу не за что зацепиться, а пустых мест нет.
/// «В точку» — своя ровная сетка.
fn layout(pattern: Pattern) -> Vec<Placed> {
    if pattern == Pattern::Plain {
        return Vec::new();
    }
    if pattern == Pattern::Dots {
        return dotted();
    }
    let seed = match pattern {
        // Тот же счётчик, что у десктопа Amber с первого дня: узор не сдвинулся.
        Pattern::Amber => 0x2545_f491,
        Pattern::Stars => 0x1b87_3593,
        Pattern::Geometry => 0x68e3_1da4,
        Pattern::Garden => 0x0b52_9d3c,
        Pattern::Waves => 0x7f4a_7c15,
        Pattern::Honeycomb => 0x3c6e_f372,
        Pattern::Snow => 0x5be0_cd19,
        Pattern::Space => 0x510e_527f,
        Pattern::Paws => 0x9b05_688c,
        Pattern::Confetti => 0x1f83_d9ab,
        Pattern::Dots | Pattern::Plain => unreachable!(),
    };
    let mut n = Counter(seed);
    let cell = TILE / 4.0;
    let mut out = Vec::new();
    for row in 0..4 {
        for col in 0..4 {
            let shift = if row % 2 == 1 { cell / 2.0 } else { 0.0 };
            let x = col as f32 * cell + shift + (n.next() - 0.5) * cell * 0.35;
            let y = row as f32 * cell + (n.next() - 0.5) * cell * 0.35;
            let k = (row * 4 + col) % 5;
            let shape = match pattern {
                Pattern::Amber => match k {
                    0 => Shape::Ring { r: 6.0 + n.next() * 3.0, width: 1.6 },
                    1 => Shape::Dot { r: 2.2 + n.next() * 1.2 },
                    2 => Shape::Spark { r: 5.5 + n.next() * 2.5 },
                    3 => Shape::Heart { r: 6.0 + n.next() * 2.0 },
                    _ => Shape::Dot { r: 1.6 },
                },
                Pattern::Stars => match k {
                    0 => Shape::Star { r: 6.5 + n.next() * 2.5 },
                    1 => Shape::Dot { r: 1.4 + n.next() * 0.8 },
                    2 => Shape::Spark { r: 5.0 + n.next() * 2.5 },
                    3 => Shape::Moon { r: 6.0 + n.next() * 1.5 },
                    _ => Shape::Dot { r: 2.2 },
                },
                Pattern::Geometry => match k {
                    0 => Shape::Triangle { r: 5.5 + n.next() * 2.0, width: 1.5 },
                    1 => Shape::Dot { r: 1.8 + n.next() * 0.8 },
                    2 => Shape::Diamond { r: 6.0 + n.next() * 2.0, width: 1.5 },
                    3 => Shape::Plus { r: 5.0 + n.next() * 1.5, width: 1.8 },
                    _ => Shape::Ring { r: 4.0 + n.next() * 2.0, width: 1.5 },
                },
                Pattern::Garden => match k {
                    0 => Shape::Leaf { r: 11.0 + n.next() * 3.0 },
                    1 => Shape::Dot { r: 2.0 + n.next() * 1.0 },
                    2 => Shape::Flower { r: 6.5 + n.next() * 1.5 },
                    3 => Shape::Leaf { r: 9.0 + n.next() * 2.0 },
                    _ => Shape::Dot { r: 1.5 },
                },
                Pattern::Waves => match k {
                    0 => Shape::Wave { half: 9.0 + n.next() * 2.0, amp: 2.2, width: 1.6 },
                    1 => Shape::Ring { r: 2.8 + n.next() * 1.5, width: 1.3 },
                    2 => Shape::Wave { half: 7.0 + n.next() * 2.0, amp: 1.8, width: 1.5 },
                    3 => Shape::Dot { r: 2.0 + n.next() * 1.0 },
                    _ => Shape::Ring { r: 5.0 + n.next() * 1.5, width: 1.4 },
                },
                Pattern::Honeycomb => match k {
                    0 => Shape::Hex { r: 7.0 + n.next() * 2.5, width: 1.5 },
                    1 => Shape::Dot { r: 1.6 + n.next() * 0.8 },
                    2 => Shape::HexFill { r: 3.2 + n.next() * 1.2 },
                    3 => Shape::Hex { r: 4.5 + n.next() * 1.5, width: 1.4 },
                    _ => Shape::Dot { r: 1.4 },
                },
                Pattern::Snow => match k {
                    0 => Shape::Flake { r: 7.5 + n.next() * 2.5, width: 1.4 },
                    1 => Shape::Dot { r: 1.5 + n.next() * 0.9 },
                    2 => Shape::Flake { r: 5.0 + n.next() * 1.5, width: 1.3 },
                    3 => Shape::Dot { r: 2.4 + n.next() * 0.8 },
                    _ => Shape::Spark { r: 3.5 + n.next() * 1.5 },
                },
                Pattern::Space => match k {
                    0 => Shape::Planet { r: 8.0 + n.next() * 2.0, width: 1.3 },
                    1 => Shape::Dot { r: 1.2 + n.next() * 0.8 },
                    2 => Shape::Star { r: 5.0 + n.next() * 1.5 },
                    3 => Shape::Ring { r: 2.6 + n.next() * 1.2, width: 1.2 },
                    _ => Shape::Spark { r: 4.0 + n.next() * 1.5 },
                },
                Pattern::Paws => match k {
                    0 => Shape::Paw { r: 9.5 + n.next() * 2.0 },
                    1 => Shape::Dot { r: 1.6 + n.next() * 0.8 },
                    2 => Shape::Paw { r: 7.0 + n.next() * 1.5 },
                    3 => Shape::HeartExact { r: 5.5 + n.next() * 1.5 },
                    _ => Shape::Dot { r: 2.2 },
                },
                Pattern::Confetti => match k {
                    0 => Shape::Strip { w: 4.2 + n.next() * 1.5, h: 1.3 },
                    1 => Shape::Dot { r: 1.7 + n.next() * 0.9 },
                    2 => Shape::Wave { half: 5.5 + n.next() * 1.5, amp: 2.0, width: 1.5 },
                    3 => Shape::Strip { w: 2.3, h: 2.3 },
                    _ => Shape::Ring { r: 2.8 + n.next() * 1.0, width: 1.3 },
                },
                Pattern::Dots | Pattern::Plain => unreachable!(),
            };
            // Поворот — у фигур, которым он к лицу; у «Янтаря» — никакого, как было.
            // Новые узоры — своими правилами и раньше общих: у первых пяти счётчик
            // тратится как прежде, и узор не сдвигается.
            let turn = match (pattern, shape) {
                (Pattern::Amber, _) => 0.0,
                (Pattern::Honeycomb, _) => 0.0,
                (Pattern::Snow, Shape::Flake { .. }) => (n.next() - 0.5) * 0.9,
                (Pattern::Space, Shape::Planet { .. }) => (n.next() - 0.5) * 1.2,
                (Pattern::Paws, Shape::Paw { .. }) => (n.next() - 0.5) * 1.4,
                (Pattern::Confetti, Shape::Strip { .. } | Shape::Wave { .. }) => {
                    (n.next() - 0.5) * std::f32::consts::TAU
                }
                (Pattern::Snow | Pattern::Space | Pattern::Paws | Pattern::Confetti, Shape::Star { .. }) => {
                    (n.next() - 0.5) * 0.9
                }
                (Pattern::Snow | Pattern::Space | Pattern::Paws | Pattern::Confetti, _) => 0.0,
                (_, Shape::Leaf { .. } | Shape::Triangle { .. } | Shape::Moon { .. }) => {
                    (n.next() - 0.5) * std::f32::consts::TAU
                }
                (_, Shape::Wave { .. } | Shape::Star { .. } | Shape::Plus { .. }) => (n.next() - 0.5) * 0.9,
                _ => 0.0,
            };
            out.push(Placed { center: (x.rem_euclid(TILE), y.rem_euclid(TILE)), shape, turn });
        }
    }
    out
}

/// «В точку»: ровная сетка 12×12 — шаг 16 точек, у плитки сетка сходится без шва.
fn dotted() -> Vec<Placed> {
    let step = TILE / 12.0;
    (0..12)
        .flat_map(|row| (0..12).map(move |col| (row, col)))
        .map(|(row, col)| Placed {
            center: (col as f32 * step + step / 2.0, row as f32 * step + step / 2.0),
            shape: Shape::Dot { r: 1.25 },
            turn: 0.0,
        })
        .collect()
}

/// Смещение точки от центра фигуры по кратчайшему пути на «торе» плитки.
fn wrapped(point: (f32, f32), center: (f32, f32)) -> (f32, f32) {
    let fold = |d: f32| {
        let d = d.rem_euclid(TILE);
        if d > TILE / 2.0 { d - TILE } else { d }
    };
    (fold(point.0 - center.0), fold(point.1 - center.1))
}

fn len(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// Прямоугольник с полуразмерами `w × h`: расстояние до края.
fn boxed(x: f32, y: f32, w: f32, h: f32) -> f32 {
    let (qx, qy) = (x.abs() - w, y.abs() - h);
    len(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0)
}

/// Отрезок от `a` до `b`: расстояние до его середины линии.
fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (px, py) = (x - a.0, y - a.1);
    let (bx, by) = (b.0 - a.0, b.1 - a.1);
    let h = ((px * bx + py * by) / (bx * bx + by * by)).clamp(0.0, 1.0);
    len(px - bx * h, py - by * h)
}

/// Насколько пиксель со смещением `d` от центра закрыт фигурой: 0…1. Край
/// размыт на один пиксель — `scale` пикселей на точку.
fn coverage(placed: &Placed, d: (f32, f32), scale: f32) -> f32 {
    let (s, c) = placed.turn.sin_cos();
    let (x, y) = (d.0 * c + d.1 * s, -d.0 * s + d.1 * c);
    let distance = match placed.shape {
        Shape::Ring { r, width } => (len(x, y) - r).abs() - width / 2.0,
        Shape::Dot { r } => len(x, y) - r,
        // Искорка — два узких ромба крест-накрест.
        Shape::Spark { r } => {
            let thin = r * 0.22;
            let a = x.abs() / thin + y.abs() / r - 1.0;
            let b = x.abs() / r + y.abs() / thin - 1.0;
            a.min(b) * thin
        }
        // Сердечко по неявной кривой (x² + y² − 1)³ − x²y³ ≤ 0; расстояние
        // приближённое, но для фигурки в дюжину точек этого хватает.
        Shape::Heart { r } => {
            let (u, v) = (x / r, -y / r + 0.25);
            let f = (u * u + v * v - 1.0).powi(3) - u * u * v.powi(3);
            f.clamp(-1.0, 1.0) * r * 0.35
        }
        // Пятиконечная звезда (Иниго Килес, sdStar5), остриём вверх.
        Shape::Star { r } => {
            let (k1x, k1y) = (0.809_017_f32, -0.587_785_f32);
            let (k2x, k2y) = (-k1x, k1y);
            let (mut px, mut py) = (x.abs(), -y);
            let d1 = 2.0 * (k1x * px + k1y * py).max(0.0);
            px -= d1 * k1x;
            py -= d1 * k1y;
            let d2 = 2.0 * (k2x * px + k2y * py).max(0.0);
            px -= d2 * k2x;
            py -= d2 * k2y;
            px = px.abs();
            py -= r;
            let rf = 0.45;
            let (bax, bay) = (rf * -k1y, rf * k1x - 1.0);
            let h = ((px * bax + py * bay) / (bax * bax + bay * bay)).clamp(0.0, r);
            len(px - bax * h, py - bay * h) * (py * bax - px * bay).signum()
        }
        // Месяц — круг без сдвинутого круга.
        Shape::Moon { r } => (len(x, y) - r).max(-(len(x - r * 0.45, y + r * 0.2) - r * 0.82)),
        // Равносторонний треугольник контуром (Иниго Килес, sdEquilateralTriangle).
        Shape::Triangle { r, width } => {
            let k = 3.0_f32.sqrt();
            let (mut px, mut py) = (x.abs() - r, -y + r / k);
            if px + k * py > 0.0 {
                (px, py) = ((px - k * py) / 2.0, (-k * px - py) / 2.0);
            }
            px -= px.clamp(-2.0 * r, 0.0);
            (-len(px, py) * py.signum()).abs() - width / 2.0
        }
        Shape::Diamond { r, width } => ((x.abs() + y.abs() - r) * std::f32::consts::FRAC_1_SQRT_2).abs() - width / 2.0,
        Shape::Plus { r, width } => boxed(x, y, r, width / 2.0).min(boxed(x, y, width / 2.0, r)),
        // Лист — пересечение двух кругов, узкое и острое, с прожилкой вдоль.
        Shape::Leaf { r } => {
            let c = r * 0.76;
            let blade = (len(x - c, y) - r).max(len(x + c, y) - r);
            let length = (r * r - c * c).sqrt();
            let rib = if y.abs() < length * 0.6 { 0.35 - x.abs() } else { f32::MIN };
            blade.max(rib)
        }
        // Цветок — пять лепестков вокруг середины.
        Shape::Flower { r } => (0..5)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 5.0 - std::f32::consts::FRAC_PI_2;
                len(x - a.cos() * r * 0.55, y - a.sin() * r * 0.55) - r * 0.36
            })
            .fold(len(x, y) - r * 0.25, f32::min),
        // Волна — отрезок синусоиды; расстояние по вертикали, поправленное на наклон.
        Shape::Wave { half, amp, width } => {
            let k = std::f32::consts::PI / half;
            let at = x.clamp(-half, half);
            let curve = amp * (k * at).sin();
            let slope = amp * k * (k * at).cos();
            let beyond = x.abs() - half;
            let d =
                if beyond > 0.0 { len(beyond, y - curve) } else { (y - curve).abs() / (1.0 + slope * slope).sqrt() };
            d - width / 2.0
        }
        // Шестиугольник (Иниго Килес, sdHexagon), `r` — до середины стороны.
        Shape::Hex { r, width } => hexagon(x, y, r).abs() - width / 2.0,
        Shape::HexFill { r } => hexagon(x, y, r),
        // Снежинка — шесть лучей, у каждого «ёлочка» из двух веточек. Точку
        // поворачиваем в сектор своего луча — считать один луч вместо шести.
        Shape::Flake { r, width } => {
            let sector = std::f32::consts::PI / 3.0;
            let angle = y.atan2(x);
            let local = angle - (angle / sector).round() * sector;
            let reach = len(x, y);
            let (ax, ay) = (reach * local.cos(), (reach * local.sin()).abs());
            let arm = segment(ax, ay, (0.0, 0.0), (r, 0.0));
            let (bx, by) = (r * 0.55, 0.0);
            let branch = segment(ax, ay, (bx, by), (bx + r * 0.3 * 0.77, by + r * 0.3 * 0.64));
            arm.min(branch) - width / 2.0
        }
        // Планета — шарик с наклонённым кольцом (приближённое расстояние до эллипса).
        Shape::Planet { r, width } => {
            let body = len(x, y) - r * 0.5;
            let q = 0.32;
            let f = len(x, y / q).max(1e-3);
            let slope = len(x, y / (q * q)) / f;
            let ring = ((f - r) / slope.max(1e-3)).abs() - width / 2.0;
            body.min(ring)
        }
        // Лапка — подушечка-овал и четыре пальчика дугой над ней, с просветами.
        Shape::Paw { r } => {
            let (a, b) = (r * 0.4, r * 0.33);
            let pad = (len(x / a, (y - r * 0.28) / b) - 1.0) * b;
            [(-0.55, -0.12), (-0.2, -0.38), (0.2, -0.38), (0.55, -0.12)]
                .into_iter()
                .map(|(tx, ty)| len(x - tx * r, y - ty * r) - r * 0.17)
                .fold(pad, f32::min)
        }
        // Полоска конфетти — прямоугольник со скруглёнными углами.
        Shape::Strip { w, h } => boxed(x, y, w, h) - 0.4,
        // Сердечко с точным расстоянием (Иниго Килес, sdHeart) — края чёткие и в
        // мелком размере. Старое `Heart` не трогаем: на нём держится «Янтарь».
        Shape::HeartExact { r } => {
            let s = r;
            let (px, py) = ((x / s).abs(), -y / s + 0.55);
            let d = if px + py > 1.0 {
                len(px - 0.25, py - 0.75) - std::f32::consts::SQRT_2 / 4.0
            } else {
                let m = 0.5 * (px + py).max(0.0);
                len(px, py - 1.0).min(len(px - m, py - m)) * (px - py).signum()
            };
            d * s
        }
    };
    (0.5 - distance * scale).clamp(0.0, 1.0)
}

/// Шестиугольник: расстояние до края (внутри — меньше нуля).
fn hexagon(x: f32, y: f32, r: f32) -> f32 {
    let (kx, ky, kz) = (-0.866_025_4_f32, 0.5_f32, 0.577_350_3_f32);
    let (mut px, mut py) = (x.abs(), y.abs());
    let d = 2.0 * (kx * px + ky * py).min(0.0);
    px -= d * kx;
    py -= d * ky;
    px -= px.clamp(-kz * r, kz * r);
    py -= r;
    len(px, py) * py.signum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: [u8; 3] = [14, 22, 33];
    const INK: [u8; 3] = [159, 184, 208];

    #[test]
    fn every_pattern_is_sparse_the_same_every_time_and_never_denser_than_set() {
        for pattern in Pattern::ALL {
            let (side, first) = tile(pattern, 1.0, BG, INK);
            assert_eq!(side, TILE as usize);
            assert_eq!(first.len(), side * side * 4);
            assert!(first.chunks(4).all(|p| p[3] == 255), "{pattern:?}: the tile is opaque");
            let inked = first.chunks(4).filter(|p| p[..3] != BG).count();
            let share = inked as f32 / (side * side) as f32;
            if pattern == Pattern::Plain {
                assert_eq!(inked, 0, "no pattern at all");
            } else {
                assert!(
                    share > 0.01 && share < 0.15,
                    "{pattern:?}: there must be a pattern, but sparse: {share} painted"
                );
            }
            assert_eq!(tile(pattern, 1.0, BG, INK).1, first, "{pattern:?}: the tile does not change between runs");
            let brightest = first.chunks(4).map(|p| p[0]).max().unwrap();
            assert!(brightest <= mix(BG, INK, STRENGTH)[0], "{pattern:?}: the pattern is never denser than set");
        }
    }

    /// Хэши плиток первых пяти узоров так, как их рисовал `amber_core::wallpaper`
    /// до переезда сюда (снято 30.09.2026 на Windows). Синус и корень у разных
    /// libm расходятся в последнем бите — поэтому сверка только под Windows, где
    /// живут программы семьи; на других системах узор тот же с точностью до пикселя.
    #[cfg(windows)]
    #[test]
    fn the_first_five_are_the_same_as_in_amber_bit_for_bit() {
        fn fnv(bytes: &[u8]) -> u64 {
            bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3))
        }
        let cases: [(f32, [u8; 3], [u8; 3]); 3] = [
            (1.0, [14, 22, 33], [159, 184, 208]),
            (2.75, [0xEE, 0xE5, 0xD3], [0x6E, 0x5A, 0x3A]),
            (1.5, [0x12, 0x10, 0x0E], [0xCD, 0xBF, 0xA8]),
        ];
        let known: [(Pattern, [u64; 3]); 6] = [
            (Pattern::Amber, [0xc997a53151c5edec, 0x763f476bb5b1007e, 0x10d4f043ccc16361]),
            (Pattern::Stars, [0x15b2db709711f936, 0xdbe150369e037ce2, 0x7b6779f8a3fb8e02]),
            (Pattern::Geometry, [0xbcd41326f7b0a2aa, 0xf86f5ee44db8335b, 0x92724abe7adb08eb]),
            (Pattern::Garden, [0x0f776e865e406c83, 0x81f52f94f0f719ac, 0x675775996e8de08d]),
            (Pattern::Waves, [0xf10ac8a5309bd7b5, 0xf46c7778fef2ced0, 0x1151c0aaaa00188b]),
            (Pattern::Plain, [0xb01498cbc7322325, 0xd4a484b81939a125, 0x1aaa088208076325]),
        ];
        for (pattern, hashes) in known {
            for ((scale, bg, ink), want) in cases.iter().zip(hashes) {
                assert_eq!(fnv(&tile(pattern, *scale, *bg, *ink).1), want, "{pattern:?} at {scale}: moved bit for bit");
            }
        }
    }

    #[test]
    fn a_denser_screen_gets_a_bigger_tile_of_the_same_picture() {
        for pattern in [Pattern::Stars, Pattern::Snow, Pattern::Dots] {
            let (side, pixels) = tile(pattern, 3.0, BG, INK);
            assert_eq!(side, 576);
            let inked = pixels.chunks(4).filter(|p| p[..3] != BG).count() as f32 / (side * side) as f32;
            let (_, small) = tile(pattern, 1.0, BG, INK);
            let small = small.chunks(4).filter(|p| p[..3] != BG).count() as f32 / (TILE * TILE);
            assert!((inked - small).abs() < 0.03, "{pattern:?}: the same share painted: {inked} vs {small}");
        }
    }

    #[test]
    fn names_survive_an_unknown_one_is_amber_and_every_pattern_has_titles() {
        for pattern in Pattern::ALL {
            assert_eq!(Pattern::from_code(pattern.code()), pattern);
            assert!(!pattern.title_ru().is_empty() && !pattern.title_en().is_empty(), "{pattern:?}");
        }
        assert_eq!(Pattern::from_code("из будущего"), Pattern::Amber);
        let codes: std::collections::HashSet<_> = Pattern::ALL.iter().map(|p| p.code()).collect();
        assert_eq!(codes.len(), Pattern::ALL.len(), "codes are unique");
        assert_eq!(Pattern::ALL.last(), Some(&Pattern::Plain), "“Plain” is the last choice");
    }

    #[test]
    fn shapes_wrap_across_the_edges_without_a_seam() {
        // Точка у левого края и точка у правого на одной высоте — соседи на торе.
        assert_eq!(wrapped((1.0, 50.0), (TILE - 1.0, 50.0)), (2.0, 0.0));
        assert_eq!(wrapped((TILE - 1.0, 5.0), (1.0, 5.0)), (-2.0, 0.0));
        // И сама плитка: крайние столбцы и строки продолжают друг друга — рядом
        // со швом узор не обрывается (соседние пиксели одинаково «живые»).
        for pattern in Pattern::ALL {
            let (side, px) = tile(pattern, 1.0, BG, INK);
            let at = |x: usize, y: usize| px[(y * side + x) * 4] as i32;
            let jump = (0..side).map(|y| (at(0, y) - at(side - 1, y)).abs()).max().unwrap();
            let inside = (1..side)
                .flat_map(|x| (0..side).map(move |y| (x, y)))
                .map(|(x, y)| (at(x, y) - at(x - 1, y)).abs())
                .max()
                .unwrap();
            assert!(jump <= inside.max(1) + 2, "{pattern:?}: the left and right edges meet like any other column");
        }
    }

    #[test]
    fn a_shape_is_solid_inside_and_clear_far_away() {
        let at = |shape| Placed { center: (0.0, 0.0), shape, turn: 0.3 };
        for shape in [
            Shape::Dot { r: 3.0 },
            Shape::Heart { r: 7.0 },
            Shape::Spark { r: 6.0 },
            Shape::Star { r: 7.0 },
            Shape::Flower { r: 7.0 },
            Shape::Plus { r: 6.0, width: 2.0 },
            Shape::HexFill { r: 4.0 },
            Shape::Flake { r: 8.0, width: 1.4 },
            Shape::Planet { r: 9.0, width: 1.3 },
            Shape::Strip { w: 3.0, h: 1.2 },
            Shape::HeartExact { r: 6.0 },
        ] {
            assert_eq!(coverage(&at(shape), (0.0, 0.0), 1.0), 1.0, "{shape:?}");
            assert_eq!(coverage(&at(shape), (30.0, 30.0), 1.0), 0.0, "{shape:?}");
        }
        for outline in [
            Shape::Ring { r: 6.0, width: 1.6 },
            Shape::Triangle { r: 7.0, width: 1.5 },
            Shape::Diamond { r: 7.0, width: 1.5 },
            Shape::Hex { r: 7.0, width: 1.5 },
        ] {
            assert_eq!(coverage(&at(outline), (0.0, 0.0), 1.0), 0.0, "{outline:?}: empty inside");
        }
        // Лист сплошной по обе стороны прожилки, а по ней — нет.
        let leaf = Placed { center: (0.0, 0.0), shape: Shape::Leaf { r: 12.0 }, turn: 0.0 };
        assert_eq!(coverage(&leaf, (1.6, 0.0), 1.0), 1.0);
        assert!(coverage(&leaf, (0.0, 0.0), 1.0) < 0.5, "the vein");
        assert_eq!(coverage(&leaf, (30.0, 30.0), 1.0), 0.0);
        let ring = Placed { center: (0.0, 0.0), shape: Shape::Ring { r: 6.0, width: 1.6 }, turn: 0.0 };
        assert_eq!(coverage(&ring, (6.0, 0.0), 1.0), 1.0);
        let wave = Placed { center: (0.0, 0.0), shape: Shape::Wave { half: 9.0, amp: 2.0, width: 1.6 }, turn: 0.0 };
        assert_eq!(coverage(&wave, (0.0, 0.0), 1.0), 1.0, "the wave goes through its middle");
        assert_eq!(coverage(&wave, (0.0, 6.0), 1.0), 0.0);
        // У шестиугольника контур по стороне; у снежинки лучи через 60°, между ними пусто.
        let hex = Placed { center: (0.0, 0.0), shape: Shape::Hex { r: 7.0, width: 1.5 }, turn: 0.0 };
        assert_eq!(coverage(&hex, (0.0, 7.0), 1.0), 1.0);
        let flake = Placed { center: (0.0, 0.0), shape: Shape::Flake { r: 8.0, width: 1.4 }, turn: 0.0 };
        for i in 0..6 {
            let a = i as f32 * std::f32::consts::PI / 3.0;
            assert_eq!(coverage(&flake, (a.cos() * 7.0, a.sin() * 7.0), 1.0), 1.0, "arm {i}");
        }
        assert_eq!(
            coverage(&flake, (std::f32::consts::FRAC_PI_6.cos() * 7.5, std::f32::consts::FRAC_PI_6.sin() * 7.5), 1.0),
            0.0
        );
        // У лапки пальчики отдельно от подушечки.
        let paw = Placed { center: (0.0, 0.0), shape: Shape::Paw { r: 10.0 }, turn: 0.0 };
        assert_eq!(coverage(&paw, (0.0, 2.8), 1.0), 1.0, "the pad");
        assert_eq!(coverage(&paw, (-5.5, -1.2), 1.0), 1.0, "a toe");
        assert_eq!(coverage(&paw, (-1.24, -1.3), 1.0), 0.0, "a gap between a toe and the pad");
    }

    #[test]
    fn the_dotted_paper_is_an_even_grid_that_meets_itself_at_the_edges() {
        let dots = dotted();
        assert_eq!(dots.len(), 144);
        let step = TILE / 12.0;
        assert!(
            dots.iter().all(|d| (d.center.0 - step / 2.0) % step == 0.0 && (d.center.1 - step / 2.0) % step == 0.0)
        );
    }
}
