//! Эффекты: чистая математика (`fn(t) -> число`) и рисование поверх `egui::Painter`.
//!
//! Здесь нет состояния и нет времени — фазу `t` (0..1) даёт [`super::Motion`]: `once` для
//! одноразового, `cycle` для бесконечного. Так эффект одинаково рисуется в кадре и в тесте, а
//! «меньше движения» решает вызывающий: `cycle` при выключенном движении отдаёт 0 — эффект стоит.
//!
//! Новый эффект: функция сюда, строка в `docs/MOTION.md`, пример в витрине (`--tab motion`).
//! Цвета приходят параметрами (из `Palette`), своих цветов здесь нет.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use eframe::egui::{self, Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Vec2};

use super::{GATHER, STAGGER, STAGGER_MAX, ease_out, ease_standard};

// ─── Математика ─────────────────────────────────────────────────────────────

/// Мигание «идёт»: прозрачность 1 → 0.35 → 1 за цикл (`phase` 0..1). Фаза 0 — ровно 1.
pub fn blink(phase: f32) -> f32 {
    let half = if phase < 0.5 { phase * 2.0 } else { (1.0 - phase) * 2.0 };
    egui::lerp(1.0..=0.35, ease_standard(half))
}

/// Встряска: затухающее колебание по x в px; `t` 0..1, три размаха.
pub fn shake_offset(t: f32, amplitude: f32) -> f32 {
    if t >= 1.0 { 0.0 } else { amplitude * (1.0 - t) * (t * 3.0 * TAU).sin() }
}

/// Вспышка: сила 1 → 0.
pub fn flash(t: f32) -> f32 {
    1.0 - ease_out(t)
}

/// Появление: `(прозрачность, сдвиг вниз в px)`; в конце `(1, 0)`.
pub fn enter(t: f32) -> (f32, f32) {
    let e = ease_out(t);
    (e, (1.0 - e) * 6.0)
}

/// Задержка появления строки `index`: шаг [`STAGGER`], не дальше [`STAGGER_MAX`] строк.
pub fn stagger(index: usize) -> f32 {
    index.min(STAGGER_MAX) as f32 * STAGGER
}

// ─── Рисование ──────────────────────────────────────────────────────────────

/// Точки дуги окружности от угла `from` до `to` (радианы, 0 — вправо, по часовой).
pub fn arc(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = 32;
    (0..=steps)
        .map(|i| {
            let a = from + (to - from) * i as f32 / steps as f32;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

/// Кольцо; `dashed` — пунктиром («нет ответа», «переподключение»).
pub fn ring(painter: &Painter, center: Pos2, radius: f32, width: f32, color: Color32, dashed: bool) {
    if dashed {
        let mut points = arc(center, radius, 0.0, TAU);
        points.push(points[0]);
        painter.extend(Shape::dashed_line(&points, Stroke::new(width, color), 4.0, 4.0));
    } else {
        painter.circle_stroke(center, radius, Stroke::new(width, color));
    }
}

/// Дуга ожидания: четверть окружности, бегущая по кругу. `phase` 0..1 — от `Motion::cycle(SPIN)`;
/// `tail` — бледный «хвост» ещё на четверть впереди (так у подключения).
pub fn spin_arc(painter: &Painter, center: Pos2, radius: f32, width: f32, color: Color32, phase: f32, tail: bool) {
    let start = -FRAC_PI_2 - FRAC_PI_4 + phase * TAU;
    painter.add(Shape::line(arc(center, radius, start, start + FRAC_PI_2), Stroke::new(width, color)));
    if tail {
        let points = arc(center, radius, start + FRAC_PI_2, start + FRAC_PI_2 * 2.0);
        painter.add(Shape::line(points, Stroke::new(width, color.gamma_multiply(0.35))));
    }
}

/// Дымка ожидания: кольца `radii` по очереди стягиваются к кругу. `cycle` 0..1 — от
/// `Motion::cycle(GATHER)`.
pub fn gather_rings(painter: &Painter, center: Pos2, radii: &[f32], color: Color32, cycle: f32) {
    for (i, base) in radii.iter().enumerate() {
        let phase = (cycle - 0.2 / GATHER * i as f32).rem_euclid(1.0);
        let e = ease_standard(phase);
        let scale = egui::lerp(1.12..=0.84, e);
        let alpha = if phase < 0.35 { phase / 0.35 * 0.6 } else { (1.0 - phase) / 0.65 * 0.6 };
        ring(painter, center, base * scale, 1.0, color.gamma_multiply(alpha), false);
    }
}

/// Что сгущается и расходится в [`settle`].
pub struct Settle<'a> {
    /// Радиус кольца, в которое дымка сгущается.
    pub core: f32,
    /// Внешние кольца дымки: радиус и прозрачность; они гаснут, пока идёт сгущение.
    pub rings: &'a [(f32, f32)],
    pub haze: Color32,
    pub wave: Color32,
}

/// «Подключено» — единственный яркий момент окна: дымка сгущается в сплошное кольцо на месте
/// дуги (`t` 0..1 за `EMPHASIS`) и расходится одной волной (`w` 0..1 за `WAVE` с `WAVE_DELAY`).
pub fn settle(painter: &Painter, center: Pos2, s: &Settle, t: f32, w: f32) {
    let e = ease_out(t);
    let alpha = if t < 0.55 { t / 0.55 } else { 1.0 };
    ring(
        painter,
        center,
        s.core * egui::lerp(1.36..=1.0, e),
        egui::lerp(1.0..=3.0, e),
        s.haze.gamma_multiply(alpha),
        false,
    );
    let fade = 1.0 - e;
    if fade > 0.0 {
        for (radius, a) in s.rings {
            ring(painter, center, radius * egui::lerp(1.0..=0.82, e), 1.0, s.haze.gamma_multiply(a * fade), false);
        }
    }
    // И одна волна — один всплеск, потом тишина.
    if w > 0.0 && w < 1.0 {
        let e = ease_out(w);
        ring(painter, center, s.core * egui::lerp(1.0..=1.42, e), 2.0, s.wave.gamma_multiply(0.55 * (1.0 - e)), false);
    }
}

/// Галочка, дорисовываемая от начала к концу: `t` 0..1 — доля пути (уже пропущенная через кривую).
pub fn check_mark(painter: &Painter, rect: Rect, t: f32, stroke: Stroke) {
    let at = |x: f32, y: f32| rect.min + Vec2::new(rect.width() * x, rect.height() * y);
    draw_path(painter, &[at(0.22, 0.52), at(0.42, 0.72), at(0.78, 0.30)], t, stroke);
}

/// Крестик: сначала одна черта, потом другая.
pub fn cross_mark(painter: &Painter, rect: Rect, t: f32, stroke: Stroke) {
    let at = |x: f32, y: f32| rect.min + Vec2::new(rect.width() * x, rect.height() * y);
    draw_path(painter, &[at(0.30, 0.30), at(0.70, 0.70)], (t * 2.0).min(1.0), stroke);
    if t > 0.5 {
        draw_path(painter, &[at(0.70, 0.30), at(0.30, 0.70)], (t * 2.0 - 1.0).min(1.0), stroke);
    }
}

/// Ломаная, нарисованная на долю `t` своей длины, с круглым концом.
fn draw_path(painter: &Painter, points: &[Pos2], t: f32, stroke: Stroke) {
    let total: f32 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = total * t.clamp(0.0, 1.0);
    if left <= 0.0 {
        return;
    }
    let mut shown = vec![points[0]];
    for pair in points.windows(2) {
        let len = pair[0].distance(pair[1]);
        if left >= len {
            shown.push(pair[1]);
            left -= len;
        } else {
            shown.push(pair[0].lerp(pair[1], left / len));
            break;
        }
    }
    let end = *shown.last().unwrap_or(&points[0]);
    painter.add(Shape::line(shown, stroke));
    painter.circle_filled(end, stroke.width / 2.0, stroke.color);
}

/// «Пинг» живой точки: одно кольцо расходится и гаснет, дальше пауза. `phase` 0..1 — от
/// `Motion::cycle(PING)`.
pub fn ping(painter: &Painter, center: Pos2, from: f32, to: f32, color: Color32, phase: f32) {
    const ACTIVE: f32 = 0.7;
    if phase >= ACTIVE {
        return;
    }
    let e = ease_out(phase / ACTIVE);
    ring(painter, center, egui::lerp(from..=to, e), 1.5, color.gamma_multiply(0.55 * (1.0 - e)), false);
}

/// Три точки «печатает»: по очереди подпрыгивают и светлеют. `phase` 0..1 — от `Motion::cycle(DOTS)`.
pub fn typing_dots(painter: &Painter, center: Pos2, gap: f32, radius: f32, color: Color32, phase: f32) {
    for i in 0..3 {
        let local = (phase - i as f32 * 0.16).rem_euclid(1.0);
        let bump = if local < 0.5 { (local / 0.5 * PI).sin() } else { 0.0 };
        let pos = Pos2::new(center.x + (i as f32 - 1.0) * gap, center.y - bump * radius * 1.4);
        painter.circle_filled(pos, radius, color.gamma_multiply(0.35 + 0.65 * bump));
    }
}

/// Скелетон: плашка `base` и бегущий по ней блик `glow`. `phase` 0..1 — от `Motion::cycle(SHIMMER)`;
/// `None` — блика нет (движение выключено): просто плашка.
pub fn shimmer(painter: &Painter, rect: Rect, corner: u8, base: Color32, glow: Color32, phase: Option<f32>) {
    painter.rect_filled(rect, corner, base);
    let Some(phase) = phase else { return };
    let band = (rect.width() * 0.4).max(40.0);
    let x = egui::lerp((rect.left() - band)..=rect.right(), phase);
    let (left, mid, right) = (x, x + band / 2.0, x + band);
    let clear = Color32::TRANSPARENT;
    let mut mesh = Mesh::default();
    for (px, color) in [(left, clear), (mid, glow), (right, clear)] {
        mesh.colored_vertex(Pos2::new(px, rect.top()), color);
        mesh.colored_vertex(Pos2::new(px, rect.bottom()), color);
    }
    // Две полосы из двух треугольников: вершины идут парами (верх, низ).
    for i in 0..2u32 {
        let k = i * 2;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    painter.with_clip_rect(rect).add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blink_is_flat_at_phase_zero_and_dips_in_the_middle() {
        assert_eq!(blink(0.0), 1.0);
        assert!((blink(0.5) - 0.35).abs() < 1e-4);
        assert!((blink(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn shake_starts_at_rest_swings_and_dies_out() {
        assert_eq!(shake_offset(0.0, 6.0), 0.0);
        let swing = (1..40).map(|i| shake_offset(i as f32 / 40.0, 6.0).abs()).fold(0.0f32, f32::max);
        assert!(swing > 3.0 && swing <= 6.0, "размах {swing}");
        assert_eq!(shake_offset(1.0, 6.0), 0.0);
    }

    #[test]
    fn flash_and_enter_end_at_rest() {
        assert_eq!(flash(0.0), 1.0);
        assert_eq!(flash(1.0), 0.0);
        assert_eq!(enter(0.0), (0.0, 6.0));
        assert_eq!(enter(1.0), (1.0, 0.0));
    }

    #[test]
    fn stagger_grows_then_caps() {
        assert_eq!(stagger(0), 0.0);
        assert!(stagger(3) > stagger(1));
        assert_eq!(stagger(STAGGER_MAX), stagger(STAGGER_MAX + 50));
    }

    #[test]
    fn drawing_helpers_do_not_panic_at_the_edges() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::splat(24.0));
            let stroke = Stroke::new(2.0, Color32::WHITE);
            for t in [0.0, 0.3, 0.5, 1.0] {
                check_mark(&painter, rect, t, stroke);
                cross_mark(&painter, rect, t, stroke);
                ping(&painter, rect.center(), 4.0, 9.0, Color32::GREEN, t);
                typing_dots(&painter, rect.center(), 8.0, 2.5, Color32::WHITE, t);
                shimmer(&painter, rect, 4, Color32::DARK_GRAY, Color32::WHITE, Some(t));
                spin_arc(&painter, rect.center(), 10.0, 2.0, Color32::WHITE, t, true);
                gather_rings(&painter, rect.center(), &[10.0, 14.0], Color32::WHITE, t);
                let s = Settle { core: 10.0, rings: &[(14.0, 0.5)], haze: Color32::GREEN, wave: Color32::GREEN };
                settle(&painter, rect.center(), &s, t, t);
            }
            shimmer(&painter, rect, 4, Color32::DARK_GRAY, Color32::WHITE, None);
        });
        output.textures_delta.clear();
    }
}
