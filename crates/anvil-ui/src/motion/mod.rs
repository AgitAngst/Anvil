//! Движение: шкала времени, кривые, «меньше движения» и готовые анимации для всех программ семьи.
//!
//! **Как устроено** (подробно — `docs/MOTION.md`, читай его первым):
//!
//! | Файл | Что там |
//! |---|---|
//! | `mod.rs` | шкала (константы длительностей), [`Motion`] — переходы, помнящие своё состояние, `install`/`tick` |
//! | `curve.rs` | кривые времени: `ease_out`, `ease_in`, `ease_standard`, `ease_pop`, `linear`, [`Curve`] |
//! | `pref.rs` | «Показывать анимацию» Windows и ручное «меньше движения» |
//! | `effects.rs` | чистая математика и рисование поверх `Painter`: дуга, кольца, встряска, галочка, точки, мерцание… |
//! | `widgets.rs` | готовые виджеты: скелетон, живая точка, «печатает», галочка результата, встряска, появление, раскрытие |
//! | `machine.rs` | машина состояний анимированного значка программы (без egui): состояние, события, сцена и показ |
//!
//! **Правила** (спецификация — раздел «Движение» дизайн-системы Anvil UI, решение 16 у Morok):
//! - короткие переходы 90–300 мс, один яркий момент на экран (сгущение «Подключено»);
//! - непрерывное движение (дуга, мерцание, точки) идёт **только пока идёт процесс**; в покое окно
//!   не просит кадров — `request_repaint` зовут только пока что-то движется;
//! - «Показывать анимацию» в Windows выключено (или включено «меньше движения» в программе) —
//!   все переходы мгновенные, ожидание — неподвижная дуга, встряска и вспышка не играют;
//! - цвета — только из `Palette`, свои числа времени — только из шкалы ниже.
//!
//! **Подключение в программе** — две строки:
//! ```ignore
//! anvil_ui::install(&cc.egui_ctx, accent, theme);   // уже зовёт motion::install
//! // в начале каждого кадра:
//! anvil_ui::motion::tick(ui.ctx());
//! // дальше в любом месте:
//! let motion = anvil_ui::motion::Motion::of(ui.ctx());
//! ```

pub mod curve;
pub mod effects;
pub mod machine;
pub mod pref;
pub mod widgets;

use std::fmt::Debug;
use std::hash::Hash;

use eframe::egui::{self, Color32, Id};

pub use curve::{Curve, bezier, ease_in, ease_out, ease_pop, ease_standard, linear};
pub use pref::{MotionPref, animations_enabled};

// ─── Шкала ──────────────────────────────────────────────────────────────────
// Секунды. Имена и числа менять только вместе с `docs/MOTION.md` — тест сверяет.

/// Нажатие: круг до 0.97.
pub const PRESS: f32 = 0.09;
/// Подсветка под курсором (строки подсвечивает набор; значение — для своих виджетов).
pub const HOVER: f32 = 0.12;
/// Смена состояния, появление диалога и меню.
pub const STATE: f32 = 0.18;
/// Уход: диалог, меню, всплывающее.
pub const LEAVE: f32 = 0.12;
/// Раскрытие баннера, сворачивание круга, ползунок сегментов и вкладок.
pub const LAYOUT: f32 = 0.20;
/// Появление строки списка (прозрачность и сдвиг).
pub const ENTER: f32 = 0.24;
/// Новая точка графика.
pub const GRAPH: f32 = 0.30;
/// Встряска после ошибки.
pub const SHAKE: f32 = 0.42;
/// Галочка результата дорисовывается.
pub const CHECK: f32 = 0.38;
/// Вспышка изменившейся строки гаснет.
pub const FLASH: f32 = 0.90;
/// «Подключено»: дымка сгущается в кольцо.
pub const EMPHASIS: f32 = 0.52;
/// Волна после сгущения: задержка и длина.
pub const WAVE_DELAY: f32 = 0.42;
pub const WAVE: f32 = 0.60;
/// Оборот дуги при подключении и переподключении.
pub const SPIN: f32 = 1.0;
pub const SPIN_SLOW: f32 = 1.6;
/// Кольца дымки стягиваются к кругу.
pub const GATHER: f32 = 1.6;
/// Мигание звена цепочки, которое «идёт».
pub const BLINK: f32 = 1.2;
/// Бегущий блик скелетона.
pub const SHIMMER: f32 = 1.5;
/// Живая точка: одно кольцо «пинга» и пауза.
pub const PING: f32 = 1.8;
/// Три точки «печатает»: один цикл.
pub const DOTS: f32 = 1.1;
/// Сдвиг между соседними строками при появлении списком.
pub const STAGGER: f32 = 0.035;
/// Сколько строк списка получают свой сдвиг; остальные появляются вместе с последней.
pub const STAGGER_MAX: usize = 8;

/// Строка шкалы для витрины и документации.
pub struct Step {
    pub name: &'static str,
    pub secs: f32,
    /// Кривая по умолчанию.
    pub curve: Curve,
    pub what: &'static str,
}

/// Вся шкала таблицей (порядок — как в `docs/MOTION.md`).
pub const SCALE: &[Step] = &[
    Step { name: "PRESS", secs: PRESS, curve: Curve::Out, what: "нажатие: до 0.97" },
    Step { name: "HOVER", secs: HOVER, curve: Curve::Out, what: "подсветка под курсором" },
    Step {
        name: "STATE", secs: STATE, curve: Curve::Out, what: "смена состояния, диалог, меню"
    },
    Step {
        name: "LEAVE", secs: LEAVE, curve: Curve::In, what: "уход диалога, меню, всплывающего"
    },
    Step {
        name: "LAYOUT", secs: LAYOUT, curve: Curve::Out, what: "баннер, сворачивание, ползунок вкладок"
    },
    Step { name: "ENTER", secs: ENTER, curve: Curve::Out, what: "появление строки списка" },
    Step {
        name: "GRAPH", secs: GRAPH, curve: Curve::Out, what: "новая точка графика, масштаб"
    },
    Step { name: "SHAKE", secs: SHAKE, curve: Curve::Linear, what: "встряска при ошибке" },
    Step { name: "CHECK", secs: CHECK, curve: Curve::Pop, what: "галочка результата" },
    Step { name: "FLASH", secs: FLASH, curve: Curve::Out, what: "вспышка изменившегося" },
    Step {
        name: "EMPHASIS", secs: EMPHASIS, curve: Curve::Out, what: "«Подключено»: дымка в кольцо"
    },
    Step { name: "WAVE", secs: WAVE, curve: Curve::Out, what: "волна после «Подключено»" },
    Step { name: "SPIN", secs: SPIN, curve: Curve::Linear, what: "оборот дуги ожидания" },
    Step {
        name: "SPIN_SLOW", secs: SPIN_SLOW, curve: Curve::Linear, what: "оборот дуги переподключения"
    },
    Step { name: "GATHER", secs: GATHER, curve: Curve::Standard, what: "кольца стягиваются" },
    Step { name: "BLINK", secs: BLINK, curve: Curve::Standard, what: "мигание звена «идёт»" },
    Step { name: "SHIMMER", secs: SHIMMER, curve: Curve::Linear, what: "блик скелетона" },
    Step { name: "PING", secs: PING, curve: Curve::Out, what: "кольцо живой точки" },
    Step { name: "DOTS", secs: DOTS, curve: Curve::Standard, what: "точки «печатает»" },
];

/// Ключ, которым отличают экземпляры одного эффекта: строка, число, кортеж — что угодно с `Hash + Debug`.
pub trait Key: Hash + Debug {}
impl<T: Hash + Debug + ?Sized> Key for T {}

// ─── Включено ли движение ───────────────────────────────────────────────────

fn motion_id() -> Id {
    Id::new("anvil-ui-motion")
}
fn pref_id() -> Id {
    Id::new("anvil-ui-motion-pref")
}
fn reduced_id() -> Id {
    Id::new("anvil-ui-motion-reduced")
}

/// Записать состояние движения в контекст и в стиль egui (`animation_time`: переходы самого egui —
/// диалоги, переключатели — идут по шкале, а при «меньше движения» — мгновенно).
fn apply(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(motion_id(), Motion { enabled }));
    let time = if enabled { STATE } else { 0.0 };
    ctx.all_styles_mut(|style| style.animation_time = time);
}

/// Один раз при запуске (`anvil_ui::install` зовёт сам): прочитать флаг Windows и поставить шкалу.
pub fn install(ctx: &egui::Context) {
    let pref = MotionPref::new();
    let enabled = animations_enabled() && !reduced(ctx);
    ctx.data_mut(|d| d.insert_temp(pref_id(), pref));
    apply(ctx, enabled);
}

/// В начале каждого кадра: перечитать флаг Windows (дёшево — по фокусу и раз в 3 с) и, если он
/// сменился, переключить `Motion` и `animation_time`. Своих перерисовок не просит.
pub fn tick(ctx: &egui::Context) {
    let mut pref = ctx.data_mut(|d| d.get_temp::<MotionPref>(pref_id())).unwrap_or_default();
    let system = pref.enabled(ctx);
    ctx.data_mut(|d| d.insert_temp(pref_id(), pref));
    let enabled = system && !reduced(ctx);
    if enabled != Motion::of(ctx).enabled {
        apply(ctx, enabled);
    }
}

/// Ручное «всегда меньше движения» — настройка программы поверх системной.
pub fn set_reduced(ctx: &egui::Context, reduced: bool) {
    ctx.data_mut(|d| d.insert_temp(reduced_id(), reduced));
    let system =
        ctx.data_mut(|d| d.get_temp::<MotionPref>(pref_id())).map_or_else(animations_enabled, |mut p| p.enabled(ctx));
    apply(ctx, system && !reduced);
}

/// Включено ли ручное «меньше движения».
pub fn reduced(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(reduced_id())).unwrap_or(false)
}

// ─── Motion ─────────────────────────────────────────────────────────────────

/// Переходы окна: включены ли, и обёртки над анимациями egui с кривыми шкалы.
///
/// `Copy`, две одинаковые копии не спорят: состояние переходов живёт в `egui::Context` по `Id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Motion {
    pub enabled: bool,
}

impl Default for Motion {
    fn default() -> Self {
        Motion { enabled: true }
    }
}

impl Motion {
    /// Текущее состояние программы (после [`install`] и [`tick`]); без них — включено.
    pub fn of(ctx: &egui::Context) -> Motion {
        ctx.data(|d| d.get_temp(motion_id())).unwrap_or_default()
    }

    /// Доля перехода к `value`: приход — за `secs` кривой прихода, уход — за [`LEAVE`] обратным ходом
    /// той же кривой (разгон и резкий конец, как у [`ease_in`]).
    pub fn toggle(&self, ctx: &egui::Context, id: Id, value: bool, secs: f32) -> f32 {
        if !self.enabled {
            // Всё мгновенно — и в памяти egui значение сразу целевое.
            return ctx.animate_bool_with_time(id, value, 0.0);
        }
        ctx.animate_bool_with_time_and_easing(id, value, if value { secs } else { LEAVE }, ease_out)
    }

    /// Плавное число: график, масштаб скорости, ползунок.
    pub fn value(&self, ctx: &egui::Context, id: Id, value: f32, secs: f32) -> f32 {
        ctx.animate_value_with_time(id, value, if self.enabled { secs } else { 0.0 })
    }

    /// Доля одноразового перехода, начатого в `since` (время egui), длиной `secs` с задержкой
    /// `delay`; пока идёт — просит следующий кадр.
    pub fn once(&self, ctx: &egui::Context, since: f64, delay: f32, secs: f32) -> f32 {
        if !self.enabled {
            return 1.0;
        }
        let now = ctx.input(|i| i.time);
        let t = ((now - since) as f32 - delay) / secs;
        if t < 1.0 {
            ctx.request_repaint();
        }
        t.clamp(0.0, 1.0)
    }

    /// Фаза бесконечного цикла длиной `period`: 0..1. Просит кадры, только пока её спрашивают —
    /// то есть пока идёт процесс. Выключено — неподвижно (0).
    pub fn cycle(&self, ctx: &egui::Context, period: f32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        ctx.request_repaint();
        let now = ctx.input(|i| i.time) as f32;
        (now / period).fract()
    }

    /// Плавный переход цвета к `target` за `secs`. Смена цели посреди перехода стартует с того
    /// цвета, что виден сейчас.
    pub fn color(&self, ctx: &egui::Context, id: Id, target: Color32, secs: f32) -> Color32 {
        let now = ctx.input(|i| i.time);
        let (from, to, at) = ctx.data(|d| d.get_temp::<(Color32, Color32, f64)>(id)).unwrap_or((target, target, now));
        let shown = |from: Color32, to: Color32, at: f64| {
            let t = if self.enabled { ((now - at) as f32 / secs).clamp(0.0, 1.0) } else { 1.0 };
            if t < 1.0 {
                ctx.request_repaint();
            }
            from.lerp_to_gamma(to, ease_out(t))
        };
        if to == target {
            let color = shown(from, to, at);
            // Дошли — запомнить цель как исходную, чтобы не считать вечно.
            ctx.data_mut(|d| d.insert_temp(id, (from, to, at)));
            return color;
        }
        let current = shown(from, to, at);
        ctx.data_mut(|d| d.insert_temp(id, (current, target, now)));
        if self.enabled { current } else { target }
    }

    /// Момент, когда `id` впервые показали (время egui). Не показывали хотя бы один кадр —
    /// отсчёт начинается заново: страницу закрыли и открыли, строка снова «входит».
    pub fn first_seen(&self, ctx: &egui::Context, id: Id) -> f64 {
        let now = ctx.input(|i| i.time);
        let frame = ctx.cumulative_frame_nr();
        let (at, _) =
            ctx.data(|d| d.get_temp::<(f64, u64)>(id)).filter(|(_, last)| frame <= last + 1).unwrap_or((now, frame));
        ctx.data_mut(|d| d.insert_temp(id, (at, frame)));
        at
    }

    /// Появление: `(прозрачность, сдвиг вниз в px)` для строки `index` списка. Строки идут одна за
    /// другой с шагом [`STAGGER`]. Выключено — сразу `(1, 0)`.
    pub fn enter(&self, ctx: &egui::Context, id: Id, index: usize) -> (f32, f32) {
        if !self.enabled {
            return (1.0, 0.0);
        }
        let at = self.first_seen(ctx, id);
        let t = self.once(ctx, at, effects::stagger(index), ENTER);
        effects::enter(t)
    }

    /// Встряска: пока `fire` истина, начинается заново; возвращает сдвиг по x в px. Выключено — 0.
    pub fn shake(&self, ctx: &egui::Context, id: Id, fire: bool, amplitude: f32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let now = ctx.input(|i| i.time);
        if fire {
            ctx.data_mut(|d| d.insert_temp(id, now));
        }
        let Some(at) = ctx.data(|d| d.get_temp::<f64>(id)) else { return 0.0 };
        let t = ((now - at) as f32) / SHAKE;
        if t >= 1.0 {
            return 0.0;
        }
        ctx.request_repaint();
        effects::shake_offset(t, amplitude)
    }

    /// Вспышка изменившегося: `key` — любой хеш значения. Пока значение то же — 0; сменилось —
    /// 1, за [`FLASH`] гаснет до 0. Первое значение не вспыхивает. Выключено — всегда 0.
    pub fn flash(&self, ctx: &egui::Context, id: Id, key: impl Key) -> f32 {
        let key = Id::NULL.with(key).value();
        let now = ctx.input(|i| i.time);
        let (last, at) = ctx.data(|d| d.get_temp::<(u64, f64)>(id)).unwrap_or((key, f64::NEG_INFINITY));
        let at = if last == key { at } else { now };
        ctx.data_mut(|d| d.insert_temp(id, (key, at)));
        if !self.enabled {
            return 0.0;
        }
        let t = ((now - at) as f32 / FLASH).clamp(0.0, 1.0);
        if t < 1.0 {
            ctx.request_repaint();
        }
        effects::flash(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Прогнать кадр egui со временем `time` и рисованием в `Ui`.
    pub(crate) fn frame_ui(ctx: &egui::Context, time: f64, mut run: impl FnMut(&mut egui::Ui)) {
        let input = egui::RawInput { time: Some(time), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| run(ui));
        output.textures_delta.clear();
    }

    /// Прогнать кадр egui со временем `time`.
    pub(crate) fn frame(ctx: &egui::Context, time: f64, mut run: impl FnMut(&egui::Context)) {
        let input = egui::RawInput { time: Some(time), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| run(ui.ctx()));
        // Текстуры шрифтов никто не рисует — отпустить их явно.
        output.textures_delta.clear();
    }

    #[test]
    fn switched_off_everything_is_instant() {
        let ctx = egui::Context::default();
        let off = Motion { enabled: false };
        let id = Id::new("x");
        let mut got = Vec::new();
        frame(&ctx, 0.0, |ctx| got.push(off.toggle(ctx, id, false, STATE)));
        frame(&ctx, 0.01, |ctx| got.push(off.toggle(ctx, id, true, STATE)));
        frame(&ctx, 0.02, |ctx| got.push(off.once(ctx, 0.0, 0.0, EMPHASIS)));
        frame(&ctx, 0.03, |ctx| got.push(off.cycle(ctx, SPIN)));
        assert_eq!(got, [0.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn switched_off_new_effects_do_not_play() {
        let ctx = egui::Context::default();
        let off = Motion { enabled: false };
        let blue = Color32::BLUE;
        frame(&ctx, 0.0, |ctx| {
            assert_eq!(off.shake(ctx, Id::new("s"), true, 6.0), 0.0);
            assert_eq!(off.flash(ctx, Id::new("f"), 1), 0.0);
            assert_eq!(off.flash(ctx, Id::new("f"), 2), 0.0);
            assert_eq!(off.enter(ctx, Id::new("e"), 3), (1.0, 0.0));
            assert_eq!(off.color(ctx, Id::new("c"), Color32::RED, STATE), Color32::RED);
            assert_eq!(off.color(ctx, Id::new("c"), blue, STATE), blue);
        });
    }

    #[test]
    fn at_rest_nothing_asks_for_frames() {
        let ctx = egui::Context::default();
        let on = Motion { enabled: true };
        let id = Id::new("y");
        // Переход шёл и кончился: кадров больше не просят.
        frame(&ctx, 0.0, |ctx| {
            on.toggle(ctx, id, false, STATE);
        });
        frame(&ctx, 0.05, |ctx| {
            on.toggle(ctx, id, true, STATE);
        });
        frame(&ctx, 1.0, |ctx| {
            on.toggle(ctx, id, true, STATE);
            on.once(ctx, 0.0, 0.0, EMPHASIS);
        });
        let mut output = ctx.run_ui(egui::RawInput { time: Some(1.1), ..Default::default() }, |ui| {
            on.toggle(ui.ctx(), id, true, STATE);
            on.once(ui.ctx(), 0.0, 0.0, EMPHASIS);
        });
        output.textures_delta.clear();
        let repaint = output.viewport_output.values().map(|v| v.repaint_delay).min().unwrap();
        assert!(repaint > std::time::Duration::from_secs(1), "в покое окно не просит кадров: {repaint:?}");
    }

    #[test]
    fn at_rest_new_effects_do_not_ask_for_frames_either() {
        let ctx = egui::Context::default();
        let on = Motion { enabled: true };
        // Всё одноразовое сыграло и кончилось.
        frame(&ctx, 0.0, |ctx| {
            on.shake(ctx, Id::new("s"), true, 6.0);
            on.flash(ctx, Id::new("f"), 1);
            on.enter(ctx, Id::new("e"), 0);
            on.color(ctx, Id::new("c"), Color32::RED, STATE);
        });
        frame(&ctx, 0.1, |ctx| {
            on.enter(ctx, Id::new("e"), 0);
            on.flash(ctx, Id::new("f"), 2);
            on.color(ctx, Id::new("c"), Color32::BLUE, STATE);
        });
        // Ещё один кадр после конца всего — как в живом окне: первые кадры egui сам просит повтор.
        frame(&ctx, 4.9, |ctx| {
            on.enter(ctx, Id::new("e"), 0);
        });
        let mut output = ctx.run_ui(egui::RawInput { time: Some(5.0), ..Default::default() }, |ui| {
            let ctx = ui.ctx();
            assert_eq!(on.shake(ctx, Id::new("s"), false, 6.0), 0.0);
            assert_eq!(on.flash(ctx, Id::new("f"), 2), 0.0);
            assert_eq!(on.enter(ctx, Id::new("e"), 0).0, 1.0);
            assert_eq!(on.color(ctx, Id::new("c"), Color32::BLUE, STATE), Color32::BLUE);
        });
        output.textures_delta.clear();
        let repaint = output.viewport_output.values().map(|v| v.repaint_delay).min().unwrap();
        assert!(repaint > std::time::Duration::from_secs(1), "в покое кадров не просят: {repaint:?}");
    }

    #[test]
    fn flash_fires_on_change_only() {
        let ctx = egui::Context::default();
        let on = Motion { enabled: true };
        let id = Id::new("flash");
        let mut seen = Vec::new();
        frame(&ctx, 0.0, |ctx| seen.push(on.flash(ctx, id, "a")));
        frame(&ctx, 0.1, |ctx| seen.push(on.flash(ctx, id, "a")));
        frame(&ctx, 0.2, |ctx| seen.push(on.flash(ctx, id, "b")));
        frame(&ctx, 0.5, |ctx| seen.push(on.flash(ctx, id, "b")));
        frame(&ctx, 2.0, |ctx| seen.push(on.flash(ctx, id, "b")));
        assert_eq!(seen[0], 0.0, "первое значение не вспыхивает");
        assert_eq!(seen[1], 0.0);
        assert!(seen[2] > 0.95, "смена — вспышка: {}", seen[2]);
        assert!(seen[3] > 0.0 && seen[3] < seen[2], "гаснет: {}", seen[3]);
        assert_eq!(seen[4], 0.0);
    }

    #[test]
    fn shake_plays_then_stops() {
        let ctx = egui::Context::default();
        let on = Motion { enabled: true };
        let id = Id::new("shake");
        let mut moved = false;
        frame(&ctx, 0.0, |ctx| {
            on.shake(ctx, id, true, 6.0);
        });
        for i in 1..20 {
            frame(&ctx, i as f64 * 0.02, |ctx| moved |= on.shake(ctx, id, false, 6.0) != 0.0);
        }
        assert!(moved, "встряска должна двигаться");
        frame(&ctx, 1.0, |ctx| assert_eq!(on.shake(ctx, id, false, 6.0), 0.0));
    }

    #[test]
    fn color_moves_from_what_is_shown() {
        let ctx = egui::Context::default();
        let on = Motion { enabled: true };
        let id = Id::new("color");
        let mut got = Vec::new();
        frame(&ctx, 0.0, |ctx| got.push(on.color(ctx, id, Color32::BLACK, STATE)));
        frame(&ctx, 0.1, |ctx| got.push(on.color(ctx, id, Color32::WHITE, STATE)));
        frame(&ctx, 0.19, |ctx| got.push(on.color(ctx, id, Color32::WHITE, STATE)));
        frame(&ctx, 0.5, |ctx| got.push(on.color(ctx, id, Color32::WHITE, STATE)));
        assert_eq!(got[0], Color32::BLACK);
        assert_eq!(got[1], Color32::BLACK, "в кадр смены цель ещё не видна");
        assert!(got[2].r() > 0 && got[2].r() < 255, "идёт: {:?}", got[2]);
        assert_eq!(got[3], Color32::WHITE);
    }

    #[test]
    fn enter_restarts_after_a_gap() {
        let ctx = egui::Context::default();
        let on = Motion { enabled: true };
        let id = Id::new("row");
        let mut alpha = Vec::new();
        frame(&ctx, 0.0, |ctx| alpha.push(on.enter(ctx, id, 0).0));
        frame(&ctx, 1.0, |ctx| alpha.push(on.enter(ctx, id, 0).0));
        // Строку не показывали два кадра — снова «входит».
        frame(&ctx, 1.1, |_| {});
        frame(&ctx, 1.2, |_| {});
        frame(&ctx, 2.0, |ctx| alpha.push(on.enter(ctx, id, 0).0));
        assert_eq!(alpha[0], 0.0);
        assert_eq!(alpha[1], 1.0);
        assert_eq!(alpha[2], 0.0, "после перерыва отсчёт заново");
    }

    /// Каждая константа шкалы описана в `docs/MOTION.md` — чтобы описание не отставало от кода.
    #[test]
    fn scale_is_documented() {
        let doc = include_str!("../../../../docs/MOTION.md");
        for step in SCALE {
            assert!(doc.contains(&format!("`{}`", step.name)), "в docs/MOTION.md нет `{}`", step.name);
        }
    }
}
