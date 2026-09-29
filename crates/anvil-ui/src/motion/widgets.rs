//! Готовые анимированные виджеты: берёшь и ставишь в интерфейс, «меньше движения» они учитывают сами.
//!
//! Все берут состояние из `Motion::of(ctx)`, цвета — из `Palette`. Кадров просят, только пока что-то
//! движется. Идентификатор `key` — любое значение, которое отличает этот экземпляр от соседей;
//! сменил `key` — эффект сыграет заново (так «повторить» в витрине).

use eframe::egui::{self, Rect, Response, Sense, Stroke, Ui, UiBuilder, Vec2};

use super::{CHECK, DOTS, Key, LAYOUT, Motion, PING, SHIMMER, ease_out, ease_pop, effects};
use crate::theme::{Palette, radius};
use crate::widgets::Tone;

/// Скелетон: серая плашка `size` с бегущим бликом — «здесь скоро появится содержимое».
/// Движение выключено — просто плашка.
pub fn skeleton(ui: &mut Ui, size: Vec2) -> Response {
    let p = Palette::of(ui);
    let motion = Motion::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let phase = motion.enabled.then(|| motion.cycle(ui.ctx(), SHIMMER));
    let glow = if p.dark { p.hover } else { p.card };
    effects::shimmer(ui.painter(), rect, radius::SMALL, p.raised, glow, phase);
    response
}

/// Живая точка: точка тона и расходящееся кольцо — «связь есть, данные идут». Кольцо есть, только
/// пока движение включено; сама точка — всегда.
pub fn live_dot(ui: &mut Ui, tone: Tone) -> Response {
    let p = Palette::of(ui);
    let motion = Motion::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    let color = tone.color(&p);
    if motion.enabled {
        effects::ping(ui.painter(), rect.center(), 4.0, 9.0, color, motion.cycle(ui.ctx(), PING));
    }
    ui.painter().circle_filled(rect.center(), 4.0, color);
    response
}

/// «Печатает…»: три точки. Движение выключено — три ровные точки.
pub fn typing_dots(ui: &mut Ui) -> Response {
    let p = Palette::of(ui);
    let motion = Motion::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(30.0, 16.0), Sense::hover());
    if motion.enabled {
        effects::typing_dots(ui.painter(), rect.center(), 8.0, 2.5, p.weak, motion.cycle(ui.ctx(), DOTS));
    } else {
        for i in 0..3 {
            let pos = egui::pos2(rect.center().x + (i as f32 - 1.0) * 8.0, rect.center().y);
            ui.painter().circle_filled(pos, 2.5, p.weak.gamma_multiply(0.7));
        }
    }
    response
}

/// Значок результата: круг с галочкой (`ok`) или крестиком. При первом показе круг «щёлкает» на
/// место, знак дорисовывается. `key` меняют, чтобы сыграть заново.
pub fn result_mark(ui: &mut Ui, key: impl Key, ok: bool, size: f32) -> Response {
    let p = Palette::of(ui);
    let motion = Motion::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let id = response.id.with(key);
    let at = motion.first_seen(ui.ctx(), id);
    let t = motion.once(ui.ctx(), at, 0.0, CHECK);
    let color = if ok { p.success } else { p.danger };
    let pop = egui::lerp(0.6..=1.0, ease_pop(t).min(1.15));
    let painter = ui.painter();
    painter.circle(
        rect.center(),
        size / 2.0 * pop - 0.5,
        p.badge_fill(color),
        Stroke::new(1.0, color.gamma_multiply(0.45)),
    );
    let inner = Rect::from_center_size(rect.center(), Vec2::splat(size * 0.62 * pop));
    let stroke = Stroke::new((size / 12.0).max(1.5), color);
    let draw = ease_out(((t - 0.15) / 0.85).clamp(0.0, 1.0));
    if ok {
        effects::check_mark(painter, inner, draw, stroke);
    } else {
        effects::cross_mark(painter, inner, draw, stroke);
    }
    response
}

/// Внутренность, сдвинутая на `delta` и с прозрачностью `alpha`; занятое место — как без сдвига.
/// Дочерний `Ui` создаётся всегда одинаково — идентификаторы виджетов внутри не прыгают.
fn shifted<R>(ui: &mut Ui, key: impl Key, delta: Vec2, alpha: f32, content: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_rect_before_wrap();
    let builder = UiBuilder::new().id_salt(key).max_rect(avail.translate(delta)).layout(*ui.layout());
    let mut child = ui.new_child(builder);
    child.set_opacity(alpha);
    let result = content(&mut child);
    let used = child.min_rect().translate(-delta);
    ui.allocate_rect(used, Sense::hover());
    result
}

/// Встряска содержимого по горизонтали: «не так» — неверный ввод, отказ. `fire` истинно в кадре
/// события (ошибка случилась) — встряска начинается заново. Движение выключено — не трясёт.
pub fn shake<R>(ui: &mut Ui, key: impl Key, fire: bool, amplitude: f32, content: impl FnOnce(&mut Ui) -> R) -> R {
    let id = ui.id().with(("anvil-shake", key_id(&key)));
    let dx = Motion::of(ui.ctx()).shake(ui.ctx(), id, fire, amplitude);
    shifted(ui, key, Vec2::new(dx, 0.0), 1.0, content)
}

/// Появление строки списка: прозрачность и сдвиг на 6 px, строки идут с шагом `STAGGER`. `index` —
/// номер строки в списке. Строку не показывали хотя бы кадр — при возвращении «входит» снова.
pub fn enter<R>(ui: &mut Ui, key: impl Key, index: usize, content: impl FnOnce(&mut Ui) -> R) -> R {
    let id = ui.id().with(("anvil-enter", key_id(&key)));
    let (alpha, dy) = Motion::of(ui.ctx()).enter(ui.ctx(), id, index);
    shifted(ui, key, Vec2::new(0.0, dy), alpha, content)
}

/// Подсветка изменившегося: пока `value` то же — ничего; сменилось — `rect` вспыхивает цветом
/// акцента и гаснет за `FLASH`. Зови до содержимого строки, чтобы подсветка легла под текст.
pub fn changed_highlight(ui: &Ui, rect: Rect, key: impl Key, value: impl Key) {
    let p = Palette::of(ui);
    let id = ui.id().with(("anvil-flash", key_id(&key)));
    let strength = Motion::of(ui.ctx()).flash(ui.ctx(), id, value);
    if strength > 0.0 {
        ui.painter().rect_filled(rect, radius::CONTROL, p.accent.gamma_multiply(0.22 * strength));
    }
}

/// Раскрытие: содержимое выезжает по высоте за `LAYOUT` (и обратно быстрее). `None` — закрыто и
/// ничего не занимает. Высоту содержимого запоминает с прошлого кадра: на самом первом кадре
/// открытия оно ещё не видно, дальше — плавно.
pub fn reveal<R>(ui: &mut Ui, key: impl Key, open: bool, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let motion = Motion::of(ui.ctx());
    let id = ui.id().with(("anvil-reveal", key_id(&key)));
    let t = motion.toggle(ui.ctx(), id, open, LAYOUT);
    if t <= 0.0 {
        return None;
    }
    let height_id = id.with("height");
    let full: f32 = ui.ctx().data(|d| d.get_temp(height_id)).unwrap_or(0.0);
    let avail = ui.available_rect_before_wrap();
    let shown = if t >= 1.0 { f32::INFINITY } else { full * t };
    let window = Rect::from_min_size(avail.min, Vec2::new(avail.width(), shown));
    let builder = UiBuilder::new().id_salt(key).max_rect(avail).layout(*ui.layout());
    let mut child = ui.new_child(builder);
    child.set_clip_rect(child.clip_rect().intersect(window));
    child.set_opacity(t);
    let result = content(&mut child);
    let height = child.min_rect().height();
    ui.ctx().data_mut(|d| d.insert_temp(height_id, height));
    let taken = if t >= 1.0 { height } else { full * t };
    ui.allocate_rect(
        Rect::from_min_size(avail.min, Vec2::new(child.min_rect().width().max(1.0), taken)),
        Sense::hover(),
    );
    Some(result)
}

/// Отпечаток ключа: `key` потом уходит в `id_salt`, а брать `Id` из него надо раньше.
fn key_id(key: &impl Key) -> egui::Id {
    egui::Id::NULL.with(key)
}

#[cfg(test)]
mod tests {
    use super::super::tests::frame_ui;
    use super::*;

    #[test]
    fn reveal_opens_and_closes() {
        let ctx = egui::Context::default();
        let mut shown = Vec::new();
        let mut heights = Vec::new();
        for (i, want) in
            [false, false, true, true, true, true, true, false, false, false, false, false].iter().enumerate()
        {
            frame_ui(&ctx, i as f64 * 0.1, |ui| {
                let r = reveal(ui, "r", *want, |ui| {
                    ui.label("привет");
                    ui.label("ещё строка");
                });
                shown.push(r.is_some());
                heights.push(ui.min_rect().height());
            });
        }
        assert!(!shown[0] && !shown[1], "закрытое ничего не показывает");
        assert!(shown[2], "открытие начинается сразу");
        assert!(shown[6], "и держится открытым");
        assert!(!shown[11], "закрытое после ухода не занимает места");
        let open_h = heights[6];
        assert!(open_h > heights[0], "открытое выше закрытого: {open_h} против {}", heights[0]);
    }

    #[test]
    fn shake_and_enter_keep_the_layout_at_rest() {
        let ctx = egui::Context::default();
        let mut rest = Vec::new();
        for i in 0..40 {
            frame_ui(&ctx, i as f64 * 0.1, |ui| {
                let before = ui.min_rect().height();
                shake(ui, "s", false, 6.0, |ui| ui.label("поле"));
                enter(ui, "e", 0, |ui| ui.label("строка"));
                rest.push(ui.min_rect().height() - before);
            });
        }
        let (first, last) = (rest[0], rest[39]);
        assert!(last > 0.0);
        assert!((first - last).abs() < 0.5, "высота не зависит от кадра: {first} и {last}");
    }

    #[test]
    fn small_widgets_allocate_the_same_size_on_and_off() {
        let ctx = egui::Context::default();
        let mut sizes = Vec::new();
        for enabled in [true, false] {
            super::super::apply(&ctx, enabled);
            frame_ui(&ctx, 1.0, |ui| {
                let a = skeleton(ui, Vec2::new(120.0, 12.0)).rect.size();
                let b = live_dot(ui, Tone::Success).rect.size();
                let c = typing_dots(ui).rect.size();
                let d = result_mark(ui, "m", true, 24.0).rect.size();
                sizes.push([a, b, c, d]);
            });
        }
        assert_eq!(sizes[0], sizes[1]);
    }
}
