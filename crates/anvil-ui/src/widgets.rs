//! Общие виджеты. Всё рисуется из [`Palette`], поэтому само следует за темой и акцентом.

use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, EventFilter, FontId, Galley, Key, Margin, MouseWheelUnit, Popup,
    Rect, RectAlign, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};

use crate::icons::{self, Icon};
use crate::lang::tr;
use crate::motion::{self, Motion};
use crate::theme::{Accent, Palette, radius, semibold};

/// Смысловой цвет: бейджи, точки состояния, баннеры, уведомления.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

impl Tone {
    pub fn color(self, p: &Palette) -> Color32 {
        match self {
            Tone::Neutral => p.weak,
            Tone::Accent => p.accent_text,
            Tone::Success => p.success,
            Tone::Warning => p.warning,
            Tone::Danger => p.danger,
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Tone::Neutral | Tone::Accent => Icon::Info,
            Tone::Success => Icon::Check,
            Tone::Warning | Tone::Danger => Icon::Warning,
        }
    }
}

/// Рамка фокуса с клавиатуры (WCAG 2.4.7): без неё самодельные кнопки не видно при Tab.
pub fn focus_ring(ui: &Ui, rect: Rect, response: &Response, corner: u8) {
    if response.has_focus() {
        let p = Palette::of(ui);
        ui.painter().rect_stroke(rect.expand(2.0), corner + 2, Stroke::new(2.0, p.accent_text), StrokeKind::Outside);
    }
}

/// Имя для диктора и AccessKit, даже если подписи на экране нет.
fn name(response: &Response, kind: WidgetType, label: &str) {
    let enabled = response.enabled();
    response.widget_info(|| WidgetInfo::labeled(kind, enabled, label));
}

// ─── Кнопки ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Главное действие экрана: залито акцентом. Одно на экран.
    Primary,
    /// Обычное действие: поверхность с рамкой.
    Secondary,
    /// Тихое действие: без фона до наведения.
    Ghost,
    /// Необратимое: красное.
    Danger,
}

/// Кнопка с необязательным значком слева.
pub fn button(ui: &mut Ui, kind: Kind, icon: Option<Icon>, text: &str) -> Response {
    button_sized(ui, kind, icon, text, Vec2::ZERO)
}

/// Кнопка не меньше `size`: большая главная кнопка внизу панели — во всю ширину и выше обычной.
/// Значок и текст — посередине.
pub fn button_sized(ui: &mut Ui, kind: Kind, icon: Option<Icon>, text: &str, size: Vec2) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let font = if kind == Kind::Primary { semibold(14.0) } else { FontId::proportional(14.0) };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let icon_size = 16.0;
    let gap = if icon.is_some() && !text.is_empty() { 7.0 } else { 0.0 };
    let icon_w = if icon.is_some() { icon_size } else { 0.0 };
    let pad = if text.is_empty() { 6.0 } else { 12.0 };
    let content_w = icon_w + gap + galley.size().x;
    let size = Vec2::new(pad * 2.0 + content_w, 30.0).max(size);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let hovered = response.hovered() && enabled;
    let pressed = response.is_pointer_button_down_on() && enabled;
    let (fill, border, fg) = match kind {
        Kind::Primary => {
            let fill = if pressed {
                p.accent.gamma_multiply(0.85)
            } else if hovered {
                p.accent.lerp_to_gamma(if p.dark { Color32::WHITE } else { Color32::BLACK }, 0.08)
            } else {
                p.accent
            };
            (fill, Color32::TRANSPARENT, p.on_accent)
        }
        Kind::Secondary => {
            let fill = if pressed {
                p.border
            } else if hovered {
                p.hover
            } else {
                p.raised
            };
            (fill, if hovered { p.border_strong } else { p.border }, p.text)
        }
        Kind::Ghost => {
            let fill = if pressed {
                p.border
            } else if hovered {
                p.hover
            } else {
                Color32::TRANSPARENT
            };
            (fill, Color32::TRANSPARENT, if hovered { p.text } else { p.weak })
        }
        // Заливка непрозрачная и под курсором не темнеет — темнеет рамка: иначе красный текст
        // теряет контраст в светлой теме.
        Kind::Danger => {
            let border = if hovered || pressed { p.danger } else { p.danger_line() };
            (p.badge_fill(p.danger), border, p.danger)
        }
    };
    // Недоступная кнопка любого вида — одна и та же: плоская, с тихим текстом.
    let (fill, border, fg) = if enabled {
        (fill, border, fg)
    } else if kind == Kind::Ghost {
        (Color32::TRANSPARENT, Color32::TRANSPARENT, p.faint)
    } else {
        (p.raised, p.border, p.faint)
    };

    let painter = ui.painter();
    painter.rect(rect, radius::CONTROL, fill, Stroke::new(1.0, border), StrokeKind::Inside);
    let mut x = rect.center().x - content_w / 2.0;
    if let Some(icon) = icon {
        let r = Rect::from_min_size(egui::pos2(x, rect.center().y - icon_size / 2.0), Vec2::splat(icon_size));
        icons::paint(painter, r, icon, fg);
        x += icon_size + gap;
    }
    painter.galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    focus_ring(ui, rect, &response, radius::CONTROL);
    name(&response, WidgetType::Button, text);
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

/// Квадратная кнопка-значок с подсказкой.
pub fn icon_button(ui: &mut Ui, icon: Icon, hint: &str) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let hovered = response.hovered() || response.is_pointer_button_down_on();
    if hovered {
        ui.painter().rect_filled(rect, radius::CONTROL, p.hover);
    }
    let color = if hovered { p.text } else { p.weak };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(17.0)), icon, color);
    focus_ring(ui, rect, &response, radius::CONTROL);
    name(&response, WidgetType::Button, hint);
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint)
}

// ─── Метки и состояние ──────────────────────────────────────────────────────

/// Бейдж-пилюля: «CI ✔», «+4», «новая версия».
pub fn badge(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let p = Palette::of(ui);
    let color = tone.color(&p);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), semibold(12.0), color);
    let size = Vec2::new(galley.size().x + 14.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 10, p.badge_fill(color));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    response
}

/// Точка состояния.
pub fn dot(ui: &mut Ui, tone: Tone) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, tone.color(&p));
    response
}

/// Кольцо вместо точки: «не запущен», «закрыт». Рядом всегда слово.
pub fn ring(ui: &mut Ui) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_stroke(rect.center(), 3.25, Stroke::new(1.5, p.weak));
    response
}

/// Клавиша: «Ctrl», «K».
pub fn kbd(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::of(ui);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::monospace(11.5), p.weak);
    let size = Vec2::new(galley.size().x + 10.0, 19.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(rect, radius::SMALL, p.raised, Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.weak);
    response
}

/// Подпись раздела: мелко, заглавными, разрядкой.
pub fn section_label(ui: &mut Ui, text: &str) {
    let p = Palette::of(ui);
    ui.label(RichText::new(text.to_uppercase()).font(semibold(11.0)).color(p.weak).extra_letter_spacing(0.8));
}

/// Заголовок блока внутри карточки.
pub fn title(ui: &mut Ui, text: &str, size: f32) {
    let p = Palette::of(ui);
    ui.label(RichText::new(text).font(semibold(size)).color(p.text));
}

/// Заголовок карточки: значок цвета `weak` и полужирная подпись.
pub fn card_title(ui: &mut Ui, icon: Icon, text: &str) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
        icons::paint(ui.painter(), rect, icon, p.weak);
        title(ui, text, 14.5);
    });
    ui.add_space(6.0);
}

/// Второстепенный текст с переносом.
pub fn note(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = Palette::of(ui);
    ui.label(RichText::new(text.into()).size(13.0).color(p.weak))
}

/// Моноширинный текст: хеши, версии, команды.
pub fn mono(ui: &mut Ui, text: &str, color: Option<Color32>) -> Response {
    let p = Palette::of(ui);
    ui.label(RichText::new(text).font(FontId::monospace(12.5)).color(color.unwrap_or(p.weak)))
}

// ─── Контейнеры ─────────────────────────────────────────────────────────────

/// Рамка карточки.
pub fn card_frame(ui: &Ui) -> egui::Frame {
    let p = Palette::of(ui);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius::CARD)
        .inner_margin(Margin::same(16))
}

/// Карточка на всю ширину.
pub fn card<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(ui)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui)
        })
        .inner
}

/// Горизонтальная линия-разделитель.
pub fn divider(ui: &mut Ui) {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.border));
}

/// Баннер: обновление, предупреждение, ошибка. Справа — действия.
pub fn banner(ui: &mut Ui, tone: Tone, title: &str, text: &str, actions: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    let color = tone.color(&p);
    egui::Frame::new()
        .fill(p.soft(color))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(radius::CARD)
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                icons::paint(ui.painter(), rect, tone.icon(), color);
                ui.add_space(4.0);
                ui.label(RichText::new(title).font(semibold(14.0)).color(p.text));
                // Сначала действия справа, потом текст в оставшемся месте: длинный текст (ошибка,
                // путь) обрезается многоточием и целиком виден в подсказке, а не раздвигает окно.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    actions(ui);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if !text.is_empty() {
                            let label = egui::Label::new(RichText::new(text).size(13.5).color(p.weak)).truncate();
                            ui.add(label).on_hover_text(text);
                        }
                    });
                });
            });
        });
}

/// Пустое состояние: значок, заголовок, пояснение — посередине.
pub fn empty_state(ui: &mut Ui, icon: Icon, heading: &str, text: &str) {
    let p = Palette::of(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 22.0, p.raised);
        icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(22.0)), icon, p.weak);
        ui.add_space(10.0);
        ui.label(RichText::new(heading).font(semibold(15.0)).color(p.text));
        ui.label(RichText::new(text).size(13.0).color(p.weak));
        ui.add_space(24.0);
    });
}

// ─── Выбор ──────────────────────────────────────────────────────────────────

/// Переключатель: подпись слева, тумблер справа, на всю ширину.
pub fn switch(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = Palette::of(ui);
    let width = ui.available_width();
    let track = Vec2::new(34.0, 20.0);
    let galley = ui.painter().layout(label.to_owned(), FontId::proportional(14.0), p.text, (width - 50.0).max(40.0));
    let height = galley.size().y.max(track.y) + 8.0;
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let enabled = ui.is_enabled();
    let dim = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
    ui.painter().galley(egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0), galley, dim(p.text));

    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let track_rect = Rect::from_min_size(egui::pos2(rect.right() - track.x, rect.center().y - track.y / 2.0), track);
    let fill = p.border_strong.lerp_to_gamma(p.accent, t);
    ui.painter().rect_filled(track_rect, 10, dim(fill));
    let knob = egui::lerp((track_rect.left() + 10.0)..=(track_rect.right() - 10.0), t);
    let knob_color = if *on { p.on_accent } else { p.card };
    ui.painter().circle_filled(egui::pos2(knob, track_rect.center().y), 7.0, dim(knob_color));
    focus_ring(ui, track_rect, &response, 10);
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, *on, label));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Переключатель в строку: тумблер и подпись справа, шириной по содержимому — несколько
/// в ряд («Инвертировать», «sRGB»). Подпись может быть пустой: тогда только тумблер.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = Palette::of(ui);
    let track = Vec2::new(30.0, 18.0);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(13.5), p.text);
    let text_w = if label.is_empty() { 0.0 } else { galley.size().x + 8.0 };
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(track.x + text_w, 26.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let enabled = ui.is_enabled();
    let dim = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let track_rect = Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - track.y / 2.0), track);
    let fill = p.border_strong.lerp_to_gamma(p.accent, t);
    ui.painter().rect_filled(track_rect, 9, dim(fill));
    let knob = egui::lerp((track_rect.left() + 9.0)..=(track_rect.right() - 9.0), t);
    let knob_color = if *on { p.on_accent } else { p.card };
    ui.painter().circle_filled(egui::pos2(knob, track_rect.center().y), 6.0, dim(knob_color));
    if !label.is_empty() {
        let pos = egui::pos2(track_rect.right() + 8.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, dim(p.text));
    }
    focus_ring(ui, track_rect, &response, 9);
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, *on, label));
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

/// Сегментный выбор: «Система · Светлая · Тёмная». Ширина сегмента — по подписи.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, Option<Icon>, &str)]) -> Response {
    let widths: Vec<f32> = options.iter().map(|(_, icon, text)| segment_content(ui, *icon, text) + 20.0).collect();
    let size = Vec2::new(widths.iter().sum::<f32>() + 4.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let mut x = rect.left() + 2.0;
    let segs: Vec<Rect> = widths
        .iter()
        .map(|w| {
            let seg = Rect::from_min_size(egui::pos2(x, rect.top() + 2.0), Vec2::new(*w, rect.height() - 4.0));
            x += w;
            seg
        })
        .collect();
    paint_segments(ui, response, rect, &segs, false, value, options)
}

/// Сегменты поровну на всю ширину: соседние выборы одной группы («Тема» и «Язык») стоят одной колонкой.
/// Самая длинная подпись не помещается в долю — варианты встают столбиком, по строке на вариант.
/// Нужна другая ширина — дать её снаружи (`ui.allocate_ui`).
pub fn segmented_full<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, Option<Icon>, &str)],
) -> Response {
    let width = ui.available_width();
    let count = options.len().max(1) as f32;
    let each = (width - 4.0) / count;
    let widest = options.iter().map(|(_, icon, text)| segment_content(ui, *icon, text)).fold(0.0, f32::max);
    // Подписи нужно хотя бы 8 по бокам, иначе она касается краёв сегмента.
    let column = widest + 16.0 > each;
    let size = if column { Vec2::new(width, count * 26.0 + 4.0) } else { Vec2::new(width, 30.0) };
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let segs: Vec<Rect> = (0..options.len())
        .map(|i| {
            let i = i as f32;
            if column {
                Rect::from_min_size(
                    egui::pos2(rect.left() + 2.0, rect.top() + 2.0 + i * 26.0),
                    Vec2::new(width - 4.0, 26.0),
                )
            } else {
                Rect::from_min_size(egui::pos2(rect.left() + 2.0 + i * each, rect.top() + 2.0), Vec2::new(each, 26.0))
            }
        })
        .collect();
    paint_segments(ui, response, rect, &segs, column, value, options)
}

/// Ширина значка и подписи сегмента.
fn segment_content(ui: &Ui, icon: Option<Icon>, text: &str) -> f32 {
    let w = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(13.0), Color32::PLACEHOLDER).size().x;
    w + if icon.is_some() { 21.0 } else { 0.0 }
}

/// Дорожка, ползунок выбранного и подписи сегментов в готовых местах `segs`. `column` — сегменты
/// столбиком: подпись у левого края, а не посередине.
fn paint_segments<T: PartialEq + Copy>(
    ui: &mut Ui,
    mut response: Response,
    rect: Rect,
    segs: &[Rect],
    column: bool,
    value: &mut T,
    options: &[(T, Option<Icon>, &str)],
) -> Response {
    let p = Palette::of(ui);
    let font = FontId::proportional(13.0);
    ui.painter().rect(rect, radius::CONTROL + 1, p.raised, Stroke::new(1.0, p.border), StrokeKind::Inside);
    // Ползунок выбранного: скользит к новому месту за LAYOUT (позиции — от угла дорожки, чтобы сдвиг
    // окна или прокрутка не запускали переход).
    if let Some(sel) = options.iter().position(|(option, ..)| *value == *option) {
        let (ctx, motion) = (ui.ctx().clone(), Motion::of(ui.ctx()));
        let seg = segs[sel];
        let edge = |name: &str, at: f32| motion.value(&ctx, response.id.with(name), at, motion::LAYOUT);
        let left = edge("thumb-left", seg.left() - rect.left());
        let right = edge("thumb-right", seg.right() - rect.left());
        let top = edge("thumb-top", seg.top() - rect.top());
        let bottom = edge("thumb-bottom", seg.bottom() - rect.top());
        let thumb = Rect::from_min_max(rect.min + Vec2::new(left, top), rect.min + Vec2::new(right, bottom));
        ui.painter().rect(thumb, radius::CONTROL - 1, p.card, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
    }
    for (i, ((option, icon, text), seg)) in options.iter().zip(segs.iter().copied()).enumerate() {
        let id = response.id.with(i);
        let r = ui.interact(seg, id, Sense::click());
        let selected = *value == *option;
        if r.clicked() && !selected {
            *value = *option;
            response.mark_changed();
        }
        if !selected && r.hovered() {
            ui.painter().rect_filled(seg, radius::CONTROL - 1, p.hover);
        }
        let color = if selected { p.text } else { p.weak };
        let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), color);
        let content_w = galley.size().x + if icon.is_some() { 21.0 } else { 0.0 };
        let mut cx = if column { seg.left() + 10.0 } else { seg.center().x - content_w / 2.0 };
        if let Some(icon) = icon {
            let ir = Rect::from_min_size(egui::pos2(cx, seg.center().y - 8.0), Vec2::splat(16.0));
            icons::paint(ui.painter(), ir, *icon, color);
            cx += 21.0;
        }
        ui.painter().galley(egui::pos2(cx, seg.center().y - galley.size().y / 2.0), galley, color);
        focus_ring(ui, seg, &r, radius::CONTROL);
        r.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, *text));
        r.on_hover_cursor(CursorIcon::PointingHand);
    }
    response
}

/// Откуда идёт заливка ползунка.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SliderFill {
    /// От начала: громкость, яркость, доля.
    #[default]
    Start,
    /// От середины, с меткой середины: баланс, сдвиг, ±. Значение тогда — словами («Центр», «Л 20»).
    Center,
}

/// Ширина колонки значения справа от ползунка — «100%» и «Л 100» не сдвигают дорожку.
const SLIDER_VALUE_W: f32 = 44.0;

/// Ползунок: дорожка `border_strong`, заливка `accent`, ручка 14 (под курсором 16), значение числом справа.
/// Мышь — щелчок и перетаскивание; в фокусе стрелки — шаг, PageUp/PageDown — крупный шаг, Home/End — края;
/// колесо над ним — шаг на щелчок колеса (страницу под ним при этом не прокручивает).
///
/// ```ignore
/// Slider::new(&mut volume, 0.0..=1.0).step(0.02).text(format!("{:.0}%", volume * 100.0)).label("Громкость").show(ui);
/// Slider::new(&mut balance, -1.0..=1.0).center().text(balance_words(balance)).show(ui);
/// ```
#[must_use = "ползунок появится только после `.show(ui)`"]
pub struct Slider<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    step: Option<f32>,
    page: Option<f32>,
    fill: SliderFill,
    width: Option<f32>,
    text: Option<String>,
    level: Option<f32>,
    quadratic: bool,
    dimmed: bool,
    label: &'a str,
}

impl<'a> Slider<'a> {
    pub fn new(value: &'a mut f32, range: RangeInclusive<f32>) -> Self {
        Slider {
            value,
            range,
            step: None,
            page: None,
            fill: SliderFill::Start,
            width: None,
            text: None,
            level: None,
            quadratic: false,
            dimmed: false,
            label: "",
        }
    }

    /// Шаг стрелок, колеса и сетка, к которой прилипает значение. По умолчанию — сотая диапазона.
    pub fn step(mut self, step: f32) -> Self {
        self.step = Some(step);
        self
    }

    /// Крупный шаг PageUp/PageDown. По умолчанию — пять шагов.
    pub fn page(mut self, page: f32) -> Self {
        self.page = Some(page);
        self
    }

    /// Заливка от середины с меткой середины.
    pub fn center(mut self) -> Self {
        self.fill = SliderFill::Center;
        self
    }

    pub fn fill(mut self, fill: SliderFill) -> Self {
        self.fill = fill;
        self
    }

    /// Ширина вместе с колонкой значения. По умолчанию — всё доступное место.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Значение справа, моноширинным: «45%», «Центр», «Л 20».
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Уровень сигнала — доля диапазона 0..1: полоска `accent_text` под дорожкой (громкость: что сейчас звучит).
    pub fn level(mut self, level: f32) -> Self {
        self.level = Some(level);
        self
    }

    /// Квадратичная шкала: место на дорожке — корень доли диапазона, тихое точнее (громкость). Стрелки и колесо
    /// по-прежнему шагают по значению. Только с заливкой от начала.
    pub fn quadratic(mut self, quadratic: bool) -> Self {
        self.quadratic = quadratic;
        self
    }

    /// Приглушить заливку до `faint`: родитель выключен или «без звука». Ползунок при этом работает.
    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }

    /// Имя для диктора: «Громкость», «Баланс».
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let p = Palette::of(ui);
        let enabled = ui.is_enabled();
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let span = (hi - lo).max(f32::EPSILON);
        let step = self.step.unwrap_or(span / 100.0).max(f32::EPSILON);
        let page = self.page.unwrap_or(step * 5.0);
        let snap = |v: f32| (lo + ((v - lo) / step).round() * step).clamp(lo, hi);

        let width = self.width.unwrap_or_else(|| ui.available_width());
        let galley = self.text.map(|t| ui.painter().layout_no_wrap(t, FontId::monospace(12.5), p.weak));
        let value_w = galley.as_ref().map_or(0.0, |g| g.size().x.max(SLIDER_VALUE_W) + 10.0);
        let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::click_and_drag());
        let area = Rect::from_min_max(rect.min, egui::pos2(rect.right() - value_w, rect.bottom()));
        let track = Rect::from_min_max(
            egui::pos2(area.left() + 8.0, area.center().y - 2.0),
            egui::pos2(area.right() - 8.0, area.center().y + 2.0),
        );

        let mut v = *self.value;
        let pressed = response.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
        if enabled
            && pressed
            && let Some(pos) = response.interact_pointer_pos()
        {
            let t = ((pos.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
            v = snap(lo + if self.quadratic { t * t } else { t } * span);
            response.request_focus();
        }
        if enabled && response.has_focus() {
            // Стрелки двигают ползунок, а не фокус на соседний элемент.
            let filter = EventFilter { horizontal_arrows: true, vertical_arrows: true, ..Default::default() };
            ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));
            let (delta, home, end) = ui.input(|i| {
                let up = i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp);
                let down = i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown);
                let pages = f32::from(i.key_pressed(Key::PageUp)) - f32::from(i.key_pressed(Key::PageDown));
                (
                    step * (f32::from(up) - f32::from(down)) + page * pages,
                    i.key_pressed(Key::Home),
                    i.key_pressed(Key::End),
                )
            });
            if delta != 0.0 {
                v = snap(v + delta);
            }
            if home {
                v = lo;
            }
            if end {
                v = hi;
            }
        }
        if enabled && response.hovered() {
            let notches: f32 = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { unit: MouseWheelUnit::Line, delta, .. } => Some(delta.y),
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y / 50.0),
                        _ => None,
                    })
                    .sum()
            });
            if notches != 0.0 {
                v = snap(v + notches.signum() * step * notches.abs().round().max(1.0));
            }
            // Колесо над ползунком — его: прокрутка под ним стоит.
            ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
        if v != *self.value {
            *self.value = v;
            response.mark_changed();
        }

        let painter = ui.painter();
        // Место на дорожке (0..1) по доле диапазона: на квадратичной шкале — корень доли.
        let at = |share: f32| if self.quadratic { share.clamp(0.0, 1.0).sqrt() } else { share.clamp(0.0, 1.0) };
        let pos = at((v - lo) / span);
        let fill = if enabled && !self.dimmed { p.accent } else { p.faint };
        painter.rect_filled(track, 2, p.border_strong);
        let origin: f32 = match self.fill {
            SliderFill::Start => 0.0,
            SliderFill::Center => {
                let x = track.center().x;
                let mark = [egui::pos2(x, track.center().y - 6.0), egui::pos2(x, track.center().y + 6.0)];
                painter.line_segment(mark, Stroke::new(2.0, p.weak));
                0.5
            }
        };
        let filled = Rect::from_min_max(
            egui::pos2(track.left() + track.width() * origin.min(pos), track.top()),
            egui::pos2(track.left() + track.width() * origin.max(pos), track.bottom()),
        );
        painter.rect_filled(filled, 2, fill);
        if let Some(level) = self.level
            && enabled
            && !self.dimmed
            && level > 0.0
        {
            let line = Rect::from_min_max(
                egui::pos2(track.left(), track.bottom() + 2.0),
                egui::pos2(track.left() + track.width() * at(level), track.bottom() + 4.0),
            );
            painter.rect_filled(line, 1, p.accent_text);
        }
        let knob = egui::pos2(track.left() + track.width() * pos, track.center().y);
        let grown = enabled && (response.hovered() || response.dragged());
        painter.circle(knob, if grown { 8.0 } else { 7.0 }, fill, Stroke::new(2.0, p.card));
        if let Some(galley) = galley {
            let place = egui::pos2(rect.right() - galley.size().x, rect.center().y - galley.size().y / 2.0);
            painter.galley(place, galley, p.weak);
        }
        focus_ring(ui, area, &response, radius::CONTROL);
        response.widget_info(|| WidgetInfo::slider(enabled, f64::from(v), self.label));
        if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
    }
}

/// Ползунок без настроек: от начала, шаг — сотая диапазона, на всю ширину. Остальное — [`Slider`].
pub fn slider(ui: &mut Ui, value: &mut f32, range: RangeInclusive<f32>, text: impl Into<String>) -> Response {
    Slider::new(value, range).text(text).show(ui)
}

/// Вкладки с полоской акцента под выбранной. Возвращает, сменилась ли вкладка.
pub fn tabs(ui: &mut Ui, selected: &mut usize, labels: &[&str]) -> bool {
    let p = Palette::of(ui);
    let mut changed = false;
    let bar_id = ui.make_persistent_id(("anvil-tabs-bar", labels.first().copied().unwrap_or(""), labels.len()));
    let mut bar = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        for (i, label) in labels.iter().enumerate() {
            let is = *selected == i;
            let color = if is { p.text } else { p.weak };
            let font = if is { semibold(14.0) } else { FontId::proportional(14.0) };
            let galley = ui.painter().layout_no_wrap(label.to_string(), font, color);
            let (rect, r) = ui.allocate_exact_size(Vec2::new(galley.size().x, 32.0), Sense::click());
            let color = if r.hovered() && !is { p.text } else { color };
            ui.painter().galley(egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0 - 2.0), galley, color);
            if is {
                bar = Some((rect.left(), rect.right(), rect.bottom()));
            }
            if r.clicked() && !is {
                *selected = i;
                changed = true;
            }
            focus_ring(ui, rect, &r, radius::SMALL);
            r.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, is, *label));
            r.on_hover_cursor(CursorIcon::PointingHand);
        }
    });
    let rect = ui.min_rect();
    ui.painter().hline(rect.x_range(), rect.bottom() + 0.5, Stroke::new(1.0, p.border));
    // Полоска акцента скользит от прежней вкладки к новой; позиции — от левого края ряда.
    if let Some((left, right, bottom)) = bar {
        let (ctx, motion) = (ui.ctx().clone(), Motion::of(ui.ctx()));
        let from = motion.value(&ctx, bar_id.with("left"), left - rect.left(), motion::LAYOUT);
        let to = motion.value(&ctx, bar_id.with("right"), right - rect.left(), motion::LAYOUT);
        let bar =
            Rect::from_min_max(egui::pos2(rect.left() + from, bottom - 2.0), egui::pos2(rect.left() + to, bottom));
        ui.painter().rect_filled(bar, 1, p.accent);
    }
    changed
}

/// Строка списка навигации: выбранная — с полоской акцента слева.
pub fn nav_item(ui: &mut Ui, selected: bool, tone: Tone, text: &str, trailing: Option<(&str, Tone)>) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, radius::CONTROL, p.soft(p.accent));
        let bar = Rect::from_min_size(egui::pos2(rect.left(), rect.top() + 8.0), Vec2::new(3.0, rect.height() - 16.0));
        ui.painter().rect_filled(bar, 2, p.accent);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, radius::CONTROL, p.hover);
    }
    ui.painter().circle_filled(egui::pos2(rect.left() + 16.0, rect.center().y), 4.0, tone.color(&p));
    let font = if selected { semibold(14.0) } else { FontId::proportional(14.0) };
    ui.painter().text(egui::pos2(rect.left() + 30.0, rect.center().y), Align2::LEFT_CENTER, text, font, p.text);
    if let Some((label, tone)) = trailing {
        let color = tone.color(&p);
        let galley = ui.painter().layout_no_wrap(label.to_owned(), semibold(11.5), color);
        let pill = Rect::from_center_size(
            egui::pos2(rect.right() - 10.0 - (galley.size().x + 12.0) / 2.0, rect.center().y),
            Vec2::new(galley.size().x + 12.0, 18.0),
        );
        ui.painter().rect_filled(pill, 9, p.badge_fill(color));
        ui.painter().galley(pill.center() - galley.size() / 2.0, galley, color);
    }
    focus_ring(ui, rect, &response, radius::CONTROL);
    response.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, text));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Высота строки-перехода [`nav_rows`].
pub const NAV_ROW: f32 = 40.0;

/// Переходы на другие страницы отдельным блоком: строки в рамке `border`, у каждой значок, подпись,
/// значение справа (`weak`, пустое — нет) и стрелка ›; под курсором — `hover`. Значки — у всех строк
/// или ни у одной. Возвращает, по какой строке щёлкнули.
///
/// `rows`: `&[(Some(Icon::Keyboard), "Горячие клавиши", ""), (Some(Icon::Layers), "Профили", "3")]`.
pub fn nav_rows(ui: &mut Ui, rows: &[(Option<Icon>, &str, &str)]) -> Option<usize> {
    debug_assert!(
        rows.iter().all(|(icon, ..)| icon.is_some()) || rows.iter().all(|(icon, ..)| icon.is_none()),
        "значки — у всех переходов или ни у одного"
    );
    let p = Palette::of(ui);
    let icons = rows.iter().any(|(icon, ..)| icon.is_some());
    let height = (rows.len() as f32 * (NAV_ROW + 1.0) - 1.0).max(0.0);
    let (frame, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    ui.painter().rect_stroke(frame, radius::CARD, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let mut clicked = None;
    for (i, (icon, text, value)) in rows.iter().enumerate() {
        let top = frame.top() + i as f32 * (NAV_ROW + 1.0);
        let row = Rect::from_min_size(egui::pos2(frame.left(), top), Vec2::new(frame.width(), NAV_ROW)).shrink(1.0);
        let r = ui.interact(row, response.id.with(i), Sense::click());
        let hovered = r.hovered() && ui.is_enabled();
        if hovered {
            ui.painter().rect_filled(row, radius::CONTROL, p.hover);
        }
        if i > 0 {
            let y = top - 0.5;
            ui.painter().hline((frame.left() + 12.0)..=(frame.right() - 12.0), y, Stroke::new(1.0, p.border));
        }
        let mut x = row.left() + 12.0;
        if icons {
            if let Some(icon) = icon {
                let glyph = Rect::from_min_size(egui::pos2(x, row.center().y - 8.0), Vec2::splat(16.0));
                icons::paint(ui.painter(), glyph, *icon, if hovered { p.text } else { p.weak });
            }
            x += 16.0 + 10.0;
        }
        let arrow = Rect::from_center_size(egui::pos2(row.right() - 16.0, row.center().y), Vec2::splat(16.0));
        icons::paint(ui.painter(), arrow, Icon::ArrowRight, p.weak);
        let mut right = arrow.left() - 6.0;
        if !value.is_empty() {
            let room = (right - x - 60.0).max(40.0);
            let galley = one_line(ui, value, FontId::proportional(13.0), p.weak, room);
            right -= galley.size().x;
            ui.painter().galley(egui::pos2(right, row.center().y - galley.size().y / 2.0), galley, p.weak);
            right -= 12.0;
        }
        let galley = one_line(ui, text, FontId::proportional(14.0), p.text, (right - x).max(20.0));
        ui.painter().galley(egui::pos2(x, row.center().y - galley.size().y / 2.0), galley, p.text);
        focus_ring(ui, row, &r, radius::CONTROL);
        let said = if value.is_empty() { (*text).to_owned() } else { format!("{text} · {value}") };
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), &said));
        if r.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Текст в одну строку не шире `width`: что не влезло — многоточием.
fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = TextWrapping::truncate_at_width(width);
    ui.painter().layout_job(job)
}

/// Поле поиска со значком лупы и подсказкой-клавишей справа.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str, shortcut: Option<&str>, width: f32) -> Response {
    let id = ui.make_persistent_id(("search", hint));
    search_field_with_id(ui, id, text, hint, shortcut, width)
}

/// То же с заданным id — чтобы по сочетанию клавиш передать полю фокус.
pub fn search_field_with_id(
    ui: &mut Ui,
    id: egui::Id,
    text: &mut String,
    hint: &str,
    shortcut: Option<&str>,
    width: f32,
) -> Response {
    let p = Palette::of(ui);
    let focused = ui.memory(|m| m.has_focus(id));
    let border = if focused { p.accent } else { p.border };
    egui::Frame::new()
        .fill(p.field)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(radius::CONTROL)
        .inner_margin(Margin { left: 8, right: 6, top: 3, bottom: 3 })
        .show(ui, |ui| {
            ui.set_width(width - 16.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                icons::paint(ui.painter(), rect, Icon::Search, p.faint);
                let reserve = if shortcut.is_some() { 64.0 } else { 0.0 };
                let edit = egui::TextEdit::singleline(text)
                    .id(id)
                    .hint_text(RichText::new(hint).color(p.faint))
                    .frame(egui::Frame::NONE)
                    .desired_width(ui.available_width() - reserve);
                let r = ui.add(edit);
                if let Some(keys) = shortcut {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for key in keys.split('+').rev() {
                            kbd(ui, key);
                        }
                    });
                }
                r
            })
            .inner
        })
        .inner
}

// ─── Ход работы ─────────────────────────────────────────────────────────────

/// Тонкая полоса прогресса. `None` — неизвестно сколько: бегущий отрезок.
pub fn progress(ui: &mut Ui, fraction: Option<f32>, width: f32) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 6.0), Sense::hover());
    let motion = Motion::of(ui.ctx());
    ui.painter().rect_filled(rect, 3, p.raised);
    let fill = match fraction {
        Some(f) => {
            // Новая доля наступает за GRAPH, а не скачком.
            let f = motion.value(ui.ctx(), response.id.with("fill"), f.clamp(0.0, 1.0), motion::GRAPH);
            Rect::from_min_size(rect.min, Vec2::new(rect.width() * f, rect.height()))
        }
        None if !motion.enabled => {
            // Движение выключено: неподвижный отрезок посередине — «идёт, но сколько — неизвестно».
            let w = rect.width() * 0.3;
            Rect::from_min_size(egui::pos2(rect.center().x - w / 2.0, rect.top()), Vec2::new(w, rect.height()))
        }
        None => {
            let t = (ui.input(|i| i.time) * 0.8).fract() as f32;
            let w = rect.width() * 0.3;
            let x = egui::lerp((rect.left() - w)..=rect.right(), t);
            ui.ctx().request_repaint();
            Rect::from_min_max(
                egui::pos2(x.max(rect.left()), rect.top()),
                egui::pos2((x + w).min(rect.right()), rect.bottom()),
            )
        }
    };
    if fill.width() > 0.0 {
        ui.painter().rect_filled(fill, 3, p.accent);
    }
    response
}

/// Крутилка: дуга, бегущая по кругу, — «идёт работа».
pub fn spinner(ui: &mut Ui, size: f32) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    // Движение выключено — дуга стоит и кадров не просит.
    let moving = Motion::of(ui.ctx()).enabled;
    let t = if moving { ui.input(|i| i.time) as f32 } else { 0.0 };
    let r = size / 2.0 - 1.5;
    let start = t * 5.0;
    let points: Vec<egui::Pos2> = (0..=24)
        .map(|i| {
            let a = start + i as f32 / 24.0 * std::f32::consts::PI * 1.4;
            rect.center() + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    ui.painter().circle_stroke(rect.center(), r, Stroke::new(2.0, p.raised));
    ui.painter().add(egui::Shape::line(points, Stroke::new(2.0, p.accent_text)));
    if moving {
        ui.ctx().request_repaint();
    }
    response
}

// ─── Меню ───────────────────────────────────────────────────────────────────

// Меню — поднятый слой `popup` с рамкой `border_strong` и тенью `popup_shadow`: фон `card` совпадал с
// карточкой или диалогом под меню, и пункты казались лежащими прямо на строках под ними.

/// Просвет между кнопкой и её меню.
const POPUP_GAP: f32 = 4.0;
/// Внутренний отступ меню.
const POPUP_MARGIN: i8 = 5;
/// Высота пункта меню.
const MENU_ITEM: f32 = 30.0;
/// Сколько вариантов выпадающего выбора видно без прокрутки.
const SELECT_ROWS: usize = 10;

/// Рамка всплывающего слоя: меню и выпадающий список.
fn popup_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.popup)
        .stroke(Stroke::new(1.0, p.border_strong))
        .corner_radius(radius::CARD)
        .shadow(p.popup_shadow())
        .inner_margin(Margin::same(POPUP_MARGIN))
}

/// Открыть ли всплывающее над кнопкой: внизу оно не помещается, а сверху места больше. `height` — его
/// высота с прошлого кадра (в первом кадре её ещё нет — тогда вниз), `below` и `above` — место под и над кнопкой.
fn popup_goes_up(height: Option<f32>, below: f32, above: f32) -> bool {
    height.is_some_and(|height| height > below && above > below)
}

/// Место всплывающего у своей кнопки: под ней или над ней (не сбоку, поверх её же строки). `end` — правым
/// краем по правому краю кнопки (меню), иначе левым по левому (выпадающий список).
fn popup_align(button: &Response, popup: egui::Id, end: bool) -> RectAlign {
    let ctx = &button.ctx;
    let screen = ctx.content_rect();
    let height = ctx.memory(|m| m.area_rect(popup)).map(|r| r.height());
    let below = screen.bottom() - button.rect.bottom() - POPUP_GAP;
    let above = button.rect.top() - screen.top() - POPUP_GAP;
    match (popup_goes_up(height, below, above), end) {
        (false, true) => RectAlign::BOTTOM_END,
        (true, true) => RectAlign::TOP_END,
        (false, false) => RectAlign::BOTTOM_START,
        (true, false) => RectAlign::TOP_START,
    }
}

/// Выпадающее меню у кнопки: открывается и закрывается её щелчком, Esc и щелчок мимо закрывают. Правый
/// край — по правому краю кнопки, под ней; внизу нет места — над ней. Ширина подписей не знает —
/// подогнать её под самую длинную подпись: [`menu_fit`].
pub fn menu<R>(button: &Response, width: f32, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    menu_sized(button, |_| width, content)
}

/// То же меню шириной по самой длинной подписи на текущем языке, но не уже `floor`. `entries` — подписи
/// пунктов и их клавиши (`("Изменить…", Some("F2"))`); мерятся, только когда меню открыто.
pub fn menu_fit<R>(
    button: &Response,
    floor: f32,
    entries: &[(&str, Option<&str>)],
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    menu_sized(button, |ctx| menu_width(ctx, floor, entries), content)
}

/// Ширина меню по подписям: место значка (34), подпись, клавиши через 16, поля меню и рамка; не меньше `floor`.
/// Пункты рисуют подписи без обрезки — меню уже подписи уводило бы её за край.
pub fn menu_width(ctx: &egui::Context, floor: f32, entries: &[(&str, Option<&str>)]) -> f32 {
    let width = |text: &str, size: f32| {
        ctx.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::PLACEHOLDER)).size().x
    };
    let widest = entries
        .iter()
        .map(|(text, keys)| width(text, 14.0) + keys.map_or(0.0, |keys| 16.0 + width(keys, 12.5)))
        .fold(0.0, f32::max);
    floor.max(2.0 * f32::from(POPUP_MARGIN) + 2.0 + 34.0 + widest + 10.0).ceil()
}

fn menu_sized<R>(
    button: &Response,
    width: impl FnOnce(&egui::Context) -> f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let id = Popup::default_response_id(button);
    // Закрытому меню ни ширина, ни место не нужны: подписи не мерятся на каждую строку списка в каждом кадре.
    if !(button.clicked() || Popup::is_id_open(&button.ctx, id)) {
        return None;
    }
    let width = width(&button.ctx);
    let p = Palette::of_ctx(&button.ctx);
    Popup::menu(button)
        .width(width)
        .gap(POPUP_GAP)
        .align(popup_align(button, id, true))
        .align_alternatives(&[])
        .frame(popup_frame(&p))
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            content(ui)
        })
        .map(|r| r.inner)
}

/// Пункт меню: значок, текст, клавиши справа. Щелчок закрывает меню. Значки — у всех пунктов меню или
/// ни у одного.
pub fn menu_item(ui: &mut Ui, icon: Option<Icon>, text: &str, shortcut: Option<&str>) -> Response {
    let r = menu_row(ui, MenuMark::Icon(icon), text, shortcut, false);
    name(&r, WidgetType::Button, text);
    r
}

/// Опасный пункт меню: красный. Ставится последним, после разделителя.
pub fn menu_item_danger(ui: &mut Ui, icon: Option<Icon>, text: &str) -> Response {
    let r = menu_row(ui, MenuMark::Icon(icon), text, None, true);
    name(&r, WidgetType::Button, text);
    r
}

/// Пункт выбора в меню: текущий — с галочкой. Место галочки есть у всех таких пунктов, поэтому подписи
/// стоят на одной вертикали. Варианты — прямо в меню, без подменю.
pub fn menu_check(ui: &mut Ui, checked: bool, text: &str) -> Response {
    let r = menu_row(ui, MenuMark::Check(checked), text, None, false);
    let enabled = ui.is_enabled();
    r.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, enabled, checked, text));
    r
}

/// Что стоит в месте значка пункта.
enum MenuMark {
    Icon(Option<Icon>),
    /// Галочка текущего; место есть и без неё.
    Check(bool),
}

fn menu_row(ui: &mut Ui, mark: MenuMark, text: &str, shortcut: Option<&str>, danger: bool) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), MENU_ITEM), Sense::click());
    let enabled = ui.is_enabled();
    let hovered = response.hovered() && enabled;
    let color = match (enabled, danger) {
        (false, _) => p.faint,
        (true, true) => p.danger,
        (true, false) => p.text,
    };
    if hovered {
        let fill = if danger { p.badge_fill(p.danger) } else { p.popup_hover };
        ui.painter().rect_filled(rect, radius::CONTROL, fill);
    }
    let slot = Rect::from_min_size(egui::pos2(rect.left() + 9.0, rect.center().y - 8.0), Vec2::splat(16.0));
    let (glyph, slotted) = match mark {
        MenuMark::Icon(icon) => (icon.map(|i| (i, if danger || hovered { color } else { p.weak })), icon.is_some()),
        MenuMark::Check(checked) => (checked.then_some((Icon::Check, color)), true),
    };
    if let Some((icon, tint)) = glyph {
        icons::paint(ui.painter(), slot, icon, tint);
    }
    let x = rect.left() + if slotted { 34.0 } else { 10.0 };
    ui.painter().text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(14.0), color);
    if let Some(keys) = shortcut {
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            keys,
            FontId::proportional(12.5),
            // Не `faint`: у того на `popup` 3:1, а сочетание надо прочитать.
            if enabled { p.weak } else { p.faint },
        );
    }
    focus_ring(ui, rect, &response, radius::CONTROL);
    if response.clicked() {
        ui.close();
    }
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

/// Разделитель между группами пунктов меню: `border_strong` — `border` на `popup` почти не виден.
pub fn menu_separator(ui: &mut Ui) {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(rect.x_range().shrink(6.0), rect.center().y, Stroke::new(1.0, p.border_strong));
}

/// Выпадающий выбор одного из многих (5 и больше; до 4 коротких — [`segmented`]): поле с текущим значением
/// и галочкой вниз, по щелчку — список под полем (внизу нет места — над ним) шириной не уже поля, текущий
/// вариант с галочкой. В фокусе ↑/↓ меняют значение без списка, Enter и пробел открывают список.
pub fn select<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)], width: f32) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let popup = Popup::default_response_id(&response);
    let open = Popup::is_id_open(ui.ctx(), popup);
    let current = options.iter().position(|(option, _)| *option == *value);

    if enabled && response.has_focus() && !open && !options.is_empty() {
        let filter = EventFilter { vertical_arrows: true, ..Default::default() };
        ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));
        let step = ui.input(|i| i32::from(i.key_pressed(Key::ArrowDown)) - i32::from(i.key_pressed(Key::ArrowUp)));
        if step != 0 {
            let last = options.len() as i32 - 1;
            let next = current.map_or(0, |i| (i as i32 + step).clamp(0, last) as usize);
            if Some(next) != current {
                *value = options[next].0;
                response.mark_changed();
            }
        }
    }
    let shown = options.iter().find(|(option, _)| *option == *value).map_or("", |(_, text)| *text);

    let hovered = response.hovered() && enabled;
    let (fill, border, fg) = if !enabled {
        (p.raised, p.border, p.faint)
    } else if hovered || open {
        (p.hover, p.border_strong, p.text)
    } else {
        (p.raised, p.border, p.text)
    };
    ui.painter().rect(rect, radius::CONTROL, fill, Stroke::new(1.0, border), StrokeKind::Inside);
    let chevron = Rect::from_center_size(egui::pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(16.0));
    icons::paint(
        ui.painter(),
        chevron,
        if open { Icon::ChevronUp } else { Icon::ArrowDown },
        if enabled { p.weak } else { p.faint },
    );
    let room = (chevron.left() - 6.0 - (rect.left() + 12.0)).max(10.0);
    let galley = one_line(ui, shown, FontId::proportional(14.0), fg, room);
    let cut =
        galley.size().x + 0.5 < ui.painter().layout_no_wrap(shown.to_owned(), FontId::proportional(14.0), fg).size().x;
    ui.painter().galley(egui::pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    focus_ring(ui, rect, &response, radius::CONTROL);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::ComboBox, enabled, shown));

    if response.clicked() || open {
        let mut picked = None;
        let entries: Vec<(&str, Option<&str>)> = options.iter().map(|(_, text)| (*text, None)).collect();
        let list_width = menu_width(ui.ctx(), width, &entries);
        Popup::menu(&response)
            .width(list_width)
            .gap(POPUP_GAP)
            .align(popup_align(&response, popup, false))
            .align_alternatives(&[])
            .frame(popup_frame(&p))
            .show(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                // Длинный список — не выше SELECT_ROWS строк, дальше прокрутка.
                egui::ScrollArea::vertical()
                    .id_salt(popup)
                    .max_height(SELECT_ROWS as f32 * (MENU_ITEM + 1.0) - 1.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (option, text) in options {
                            if menu_check(ui, *option == *value, text).clicked() {
                                picked = Some(*option);
                            }
                        }
                    });
            });
        if let Some(option) = picked
            && option != *value
        {
            *value = option;
            response.mark_changed();
        }
    }
    let response = if cut { response.on_hover_text(shown) } else { response };
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

// ─── Всплывающее ────────────────────────────────────────────────────────────

/// Всплывающие уведомления в правом нижнем углу.
#[derive(Default)]
pub struct Toasts {
    items: Vec<(String, Tone, Instant)>,
}

impl Toasts {
    const LIFE: Duration = Duration::from_secs(4);

    pub fn push(&mut self, text: impl Into<String>, tone: Tone) {
        self.items.push((text.into(), tone, Instant::now()));
    }

    /// Уведомления, появившиеся после `at`: окно спрятано — их надо показать иначе.
    pub fn since(&self, at: Instant) -> Vec<(String, Tone)> {
        self.items.iter().filter(|(_, _, t)| *t > at).map(|(text, tone, _)| (text.clone(), *tone)).collect()
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.items.retain(|(_, _, at)| at.elapsed() < Self::LIFE);
        if self.items.is_empty() {
            return;
        }
        let p = Palette::of_ctx(ctx);
        let moving = Motion::of(ctx).enabled;
        egui::Area::new(egui::Id::new("anvil-toasts"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -48.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for (text, tone, at) in &self.items {
                    let age = at.elapsed().as_secs_f32();
                    // Появляются и гаснут плавно; движение выключено — просто есть и потом нет.
                    let fade = if moving {
                        ((Self::LIFE.as_secs_f32() - age) / 0.3).clamp(0.0, 1.0) * (age / 0.15).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let color = tone.color(&p);
                    egui::Frame::new()
                        .fill(p.card)
                        .stroke(Stroke::new(1.0, p.border_strong))
                        .corner_radius(radius::CARD)
                        .shadow(ctx.global_style().visuals.popup_shadow)
                        .inner_margin(Margin::symmetric(14, 10))
                        .multiply_with_opacity(fade)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                                icons::paint(ui.painter(), rect, tone.icon(), color.gamma_multiply(fade));
                                ui.label(RichText::new(text).color(p.text.gamma_multiply(fade)));
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

/// Диалог подтверждения. Возвращает `Some(true)` — подтвердили, `Some(false)` — отменили.
///
/// Необратимое подтверждается только здесь, и в теле пишется, что именно произойдёт.
pub fn confirm(
    ctx: &egui::Context,
    id: &str,
    heading: &str,
    body: impl FnOnce(&mut Ui),
    confirm_label: &str,
    danger: bool,
) -> Option<bool> {
    let p = Palette::of_ctx(ctx);
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius::WINDOW)
        .shadow(ctx.global_style().visuals.window_shadow)
        .inner_margin(Margin::same(20));
    let mut answer = None;
    let response = egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(if p.dark { 150 } else { 90 }))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new(heading).font(semibold(17.0)).color(p.text));
            ui.add_space(8.0);
            body(ui);
            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let kind = if danger { Kind::Danger } else { Kind::Primary };
                if button(ui, kind, None, confirm_label).clicked() {
                    answer = Some(true);
                }
                if button(ui, Kind::Secondary, None, tr(ctx, "Отмена")).clicked() {
                    answer = Some(false);
                }
            });
        });
    if answer.is_none() && response.should_close() {
        answer = Some(false);
    }
    answer
}

/// Ключ–значение в одну строку: «Ветка   main».
pub fn field_row(ui: &mut Ui, key: &str, value: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(110.0, 22.0), Sense::hover());
        ui.painter().text(
            egui::pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            key,
            FontId::proportional(13.0),
            p.weak,
        );
        value(ui);
    });
}

/// Скруглённый прямоугольник-подложка — для своих раскладок.
pub fn fill_rect(ui: &Ui, rect: Rect, color: Color32, corner: impl Into<CornerRadius>) {
    ui.painter().rect_filled(rect, corner, color);
}

// ─── Пульт: знаки, чипы, строки ─────────────────────────────────────────────

/// Нарисовать знак предмета в `rect`: квадрат со скруглением в четверть стороны и значок
/// посередине. `accent` — цвет программы семьи; `None` — нейтральный знак (Godot, Unity).
pub fn paint_mark(ui: &Ui, rect: Rect, accent: Option<Accent>, icon: Icon) {
    let p = Palette::of(ui);
    let (fill, fg, edge) = p.mark(accent);
    let corner = (rect.width() * 0.25).round() as u8;
    let stroke = edge.map_or(Stroke::NONE, |c| Stroke::new(1.0, c));
    ui.painter().rect(rect, corner, fill, stroke, StrokeKind::Inside);
    let inner = Rect::from_center_size(rect.center(), Vec2::splat(rect.width() * 0.58));
    icons::paint(ui.painter(), inner, icon, fg);
}

/// Знак предмета: 16 — чипы и история, 28 — строки, 40 — заголовок службы, 56 — страница предмета.
pub fn item_mark(ui: &mut Ui, accent: Option<Accent>, icon: Icon, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_mark(ui, rect, accent, icon);
    response
}

/// Стопка знаков (задел под наборы Пульта — в Anvil 0.3 их нет): знаки 28, каждый следующий на 18 правее, с кольцом 2 px цвета `under`.
pub fn mark_stack(ui: &mut Ui, marks: &[(Option<Accent>, Icon)], under: Color32) -> Response {
    let size = 28.0;
    let width = if marks.is_empty() { 0.0 } else { size + 18.0 * (marks.len() - 1) as f32 };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, size), Sense::hover());
    for (i, (accent, icon)) in marks.iter().enumerate() {
        let r = Rect::from_min_size(egui::pos2(rect.left() + 18.0 * i as f32, rect.top()), Vec2::splat(size));
        ui.painter().rect_filled(r.expand(2.0), 9, under);
        paint_mark(ui, r, *accent, *icon);
    }
    response
}

/// Что вернула раздельная кнопка: основная часть и стрелка с меню.
pub struct SplitResponse {
    pub main: Response,
    pub menu: Response,
}

/// Раздельная кнопка: слева действие, справа стрелка, открывающая меню (`menu(&r.menu, …)`).
/// Одна пилюля высотой 30; у каждой части свой фокус.
pub fn split_button(ui: &mut Ui, kind: Kind, icon: Option<Icon>, text: &str, menu_hint: &str) -> SplitResponse {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let font = if kind == Kind::Primary { semibold(14.0) } else { FontId::proportional(14.0) };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let gap = if icon.is_some() { 7.0 } else { 0.0 };
    let content_w = if icon.is_some() { 16.0 } else { 0.0 } + gap + galley.size().x;
    let main_w = 24.0 + content_w;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(main_w + 28.0, 30.0), Sense::hover());
    let main_rect = Rect::from_min_size(rect.min, Vec2::new(main_w, 30.0));
    let menu_rect = Rect::from_min_max(egui::pos2(main_rect.right(), rect.top()), rect.max);
    let main = ui.interact(main_rect, ui.id().with(("split-main", text)), Sense::click());
    let menu = ui.interact(menu_rect, ui.id().with(("split-menu", text)), Sense::click());

    let (base, border, fg, divider) = match (enabled, kind) {
        (false, _) => (p.raised, p.border, p.faint, p.border),
        (true, Kind::Primary) => (p.accent, Color32::TRANSPARENT, p.on_accent, p.on_accent.gamma_multiply(0.3)),
        (true, _) => (p.raised, p.border, p.text, p.border),
    };
    let zone_fill = |r: &Response| {
        if !enabled || !(r.hovered() || r.is_pointer_button_down_on()) {
            return base;
        }
        match kind {
            Kind::Primary => p.accent.lerp_to_gamma(if p.dark { Color32::WHITE } else { Color32::BLACK }, 0.08),
            _ => p.hover,
        }
    };
    let painter = ui.painter();
    let r = radius::CONTROL;
    painter.rect_filled(main_rect, CornerRadius { nw: r, sw: r, ne: 0, se: 0 }, zone_fill(&main));
    painter.rect_filled(menu_rect, CornerRadius { nw: 0, sw: 0, ne: r, se: r }, zone_fill(&menu));
    painter.rect_stroke(rect, r, Stroke::new(1.0, border), StrokeKind::Inside);
    painter.vline(main_rect.right(), rect.y_range(), Stroke::new(1.0, divider));

    let mut x = main_rect.left() + 12.0;
    if let Some(icon) = icon {
        let ir = Rect::from_min_size(egui::pos2(x, rect.center().y - 8.0), Vec2::splat(16.0));
        icons::paint(painter, ir, icon, fg);
        x += 16.0 + gap;
    }
    painter.galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    icons::paint(painter, Rect::from_center_size(menu_rect.center(), Vec2::splat(16.0)), Icon::ArrowDown, fg);

    focus_ring(ui, main_rect, &main, radius::CONTROL);
    focus_ring(ui, menu_rect, &menu, radius::CONTROL);
    name(&main, WidgetType::Button, text);
    name(&menu, WidgetType::Button, menu_hint);
    let menu = menu.on_hover_text(menu_hint);
    if enabled {
        SplitResponse {
            main: main.on_hover_cursor(CursorIcon::PointingHand),
            menu: menu.on_hover_cursor(CursorIcon::PointingHand),
        }
    } else {
        SplitResponse { main, menu }
    }
}

/// Чип источника: что запустится. «установлена `0.4.0`», «сборка `2353af9`», «не установлена».
/// Без заливки, в рамке — одинаково читается на любом фоне строки.
pub fn source_chip(ui: &mut Ui, word: &str, mono: Option<&str>) -> Response {
    let p = Palette::of(ui);
    let word_g = ui.painter().layout_no_wrap(word.to_owned(), FontId::proportional(12.0), p.weak);
    let mono_g = mono.map(|m| ui.painter().layout_no_wrap(m.to_owned(), FontId::monospace(12.5), p.weak));
    let space = if mono_g.is_some() { 4.0 } else { 0.0 };
    let inner = word_g.size().x + space + mono_g.as_ref().map_or(0.0, |g| g.size().x);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(inner + 14.0, 20.0), Sense::hover());
    ui.painter().rect_stroke(rect, radius::SMALL, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
    let y = rect.center().y;
    let w = word_g.size();
    ui.painter().galley(egui::pos2(rect.left() + 7.0, y - w.y / 2.0), word_g, p.weak);
    if let Some(g) = mono_g {
        let size = g.size();
        ui.painter().galley(egui::pos2(rect.left() + 7.0 + w.x + space, y - size.y / 2.0), g, p.weak);
    }
    response
}

/// Чип «Запущено» в строке состояния: знак 16, имя, хвост моно (`2:14`, `:18731`). Нажимается.
pub fn running_chip(ui: &mut Ui, accent: Option<Accent>, icon: Icon, label: &str, tail: &str) -> Response {
    let p = Palette::of(ui);
    let name_g = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(12.0), p.text);
    let tail_g = ui.painter().layout_no_wrap(tail.to_owned(), FontId::monospace(12.5), p.weak);
    let tail_w = if tail.is_empty() { 0.0 } else { 6.0 + tail_g.size().x };
    let width = 3.0 + 16.0 + 6.0 + name_g.size().x + tail_w + 9.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::click());
    let fill = if response.hovered() { p.hover } else { p.raised };
    ui.painter().rect(rect, 11, fill, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let mark = Rect::from_min_size(egui::pos2(rect.left() + 3.0, rect.center().y - 8.0), Vec2::splat(16.0));
    paint_mark(ui, mark, accent, icon);
    let y = rect.center().y;
    let n = name_g.size();
    let x = mark.right() + 6.0;
    ui.painter().galley(egui::pos2(x, y - n.y / 2.0), name_g, p.text);
    if !tail.is_empty() {
        let t = tail_g.size();
        ui.painter().galley(egui::pos2(x + n.x + 6.0, y - t.y / 2.0), tail_g, p.weak);
    }
    focus_ring(ui, rect, &response, 11);
    name(&response, WidgetType::Button, &format!("{label} {tail}"));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Узел цепочки — см. [`chain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainNode {
    Running,
    Stopped,
    Crashed,
    Starting,
}

/// Цепочка состояний (задел под наборы Пульта — в Anvil 0.3 их нет): узлы по порядку шагов и справа текст («2 из 3 работают»).
pub fn chain(ui: &mut Ui, nodes: &[ChainNode], text: &str) -> Response {
    let p = Palette::of(ui);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(13.0), p.text);
    let nodes_w = if nodes.is_empty() { 0.0 } else { 10.0 + 22.0 * (nodes.len() - 1) as f32 };
    let text_w = if text.is_empty() { 0.0 } else { 10.0 + galley.size().x };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(nodes_w + text_w, 20.0), Sense::hover());
    let painter = ui.painter();
    let y = rect.center().y;
    for (i, node) in nodes.iter().enumerate() {
        let cx = rect.left() + 5.0 + 22.0 * i as f32;
        if i + 1 < nodes.len() {
            let link = Rect::from_min_size(egui::pos2(cx + 5.0, y - 1.0), Vec2::new(12.0, 2.0));
            painter.rect_filled(link, 0, p.border_strong);
        }
        let c = egui::pos2(cx, y);
        match node {
            ChainNode::Running => {
                painter.circle_filled(c, 5.0, p.success);
            }
            ChainNode::Stopped => {
                painter.circle_stroke(c, 4.25, Stroke::new(1.5, p.weak));
            }
            ChainNode::Crashed => {
                painter.circle_filled(c, 5.0, p.danger);
                let s = Stroke::new(1.5, p.card);
                painter.line_segment([c + Vec2::new(-2.0, -2.0), c + Vec2::new(2.0, 2.0)], s);
                painter.line_segment([c + Vec2::new(2.0, -2.0), c + Vec2::new(-2.0, 2.0)], s);
            }
            ChainNode::Starting => {
                painter.circle_stroke(c, 4.25, Stroke::new(1.5, p.border_strong));
                let start = -std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4;
                let points: Vec<egui::Pos2> = (0..=8)
                    .map(|k| {
                        let a = start + std::f32::consts::FRAC_PI_2 * k as f32 / 8.0;
                        c + Vec2::new(a.cos(), a.sin()) * 4.25
                    })
                    .collect();
                painter.add(egui::Shape::line(points, Stroke::new(1.5, p.accent_text)));
            }
        }
    }
    if !text.is_empty() {
        let g = galley.size();
        painter.galley(egui::pos2(rect.left() + nodes_w + 10.0, y - g.y / 2.0), galley, p.text);
    }
    response
}

/// Ряд подсказок клавиш: группы «клавиши — подпись», между группами 16.
/// `groups`: например `&[(&["Enter"], "запустить"), (&["Ctrl", "Enter"], "из кода")]`.
pub fn key_hints(ui: &mut Ui, groups: &[(&[&str], &str)]) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, (keys, label)) in groups.iter().enumerate() {
            if i > 0 {
                ui.add_space(16.0);
            }
            for (k, key) in keys.iter().enumerate() {
                if k > 0 {
                    ui.add_space(3.0);
                }
                kbd(ui, key);
            }
            ui.add_space(6.0);
            ui.label(RichText::new(*label).size(12.0).color(p.weak));
        }
    });
}

/// Поле сочетания клавиш: клавиши, «Изменить…», под ними — свободно ли сочетание.
/// Возвращает ответ кнопки «Изменить…». `status`: `(true, "Свободно — …")` или `(false, "Занято …")`.
pub fn hotkey_field(ui: &mut Ui, keys: &[&str], change: &str, status: Option<(bool, &str)>) -> Response {
    let p = Palette::of(ui);
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for key in keys {
                kbd(ui, key);
            }
            ui.add_space(7.0);
            button(ui, Kind::Ghost, None, change)
        })
        .inner;
    if let Some((free, text)) = status {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            dot(ui, if free { Tone::Success } else { Tone::Danger });
            let color = if free { p.weak } else { p.text };
            ui.label(RichText::new(text).size(13.0).color(color));
        });
    }
    response
}

/// Строка списка Пульта и быстрого запуска: во всю ширину, высотой `height`. Рисует только фон:
/// под курсором — `hover`, выбранная — `raised` и полоска акцента слева. Колонки рисует вызывающий
/// в `response.rect`; кнопки поверх строки заводить после неё, чтобы щелчок доставался им.
///
/// Строка не берёт фокус клавиатуры: по строкам ходит сам список (↑↓ и Enter — у вызывающего),
/// а Tab ведёт к кнопкам строки. Иначе Enter на строке в фокусе был бы её щелчком.
pub fn list_row(ui: &mut Ui, id: egui::Id, selected: bool, height: f32, label: &str) -> Response {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let response = ui.interact(rect, id, Sense::CLICK);
    if selected {
        ui.painter().rect_filled(rect, radius::CONTROL, p.raised);
        let bar = Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - 14.0), Vec2::new(3.0, 28.0));
        ui.painter().rect_filled(bar, 2, p.accent);
    } else if ui.rect_contains_pointer(rect) {
        // Не `response.hovered()`: над кнопкой строки он гаснет, и фон мигал бы.
        ui.painter().rect_filled(rect, radius::CONTROL, p.hover);
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, label));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput};

    fn context(theme: ThemeChoice) -> Context {
        let ctx = Context::default();
        crate::install(&ctx, Accent::BLUE, theme);
        ctx
    }

    /// Один кадр окна `screen` с событиями `events`.
    fn run(ctx: &Context, screen: Vec2, events: Vec<Event>, mut add: impl FnMut(&mut Ui)) {
        let input =
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen)), events, ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| add(ui));
        output.textures_delta.clear();
    }

    fn key(key: Key) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    /// Щелчок левой кнопкой в `pos`: подвести, нажать, отпустить — три кадра.
    fn click(pos: Pos2) -> [Vec<Event>; 3] {
        let button =
            |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        [vec![Event::PointerMoved(pos)], vec![button(true)], vec![button(false)]]
    }

    const SCREEN: Vec2 = Vec2::new(460.0, 400.0);

    // ─── Меню ───────────────────────────────────────────────────────────────

    /// Один кадр окна `screen` с кнопкой «⋯» в `at`; меню открыто, когда `open`. Возвращает место меню.
    fn menu_frame(ctx: &Context, screen: Vec2, at: Pos2, open: bool) -> Option<Rect> {
        let mut id = None;
        run(ctx, screen, vec![], |ui| {
            let button = ui.put(Rect::from_center_size(at, Vec2::splat(30.0)), egui::Button::new("⋯"));
            id = Some(Popup::default_response_id(&button));
            if open {
                Popup::open_id(ui.ctx(), Popup::default_response_id(&button));
            }
            menu(&button, 236.0, |ui| {
                for text in ["Подключить", "Изменить…", "Подробности…", "Скопировать адрес", "Поделиться…"]
                {
                    menu_item(ui, None, text, None);
                }
                menu_separator(ui);
                menu_item_danger(ui, None, "Удалить…");
            });
        });
        ctx.memory(|m| m.area_rect(id?))
    }

    /// Несколько кадров подряд: первый — «меряющий» (меню невидимо), дальше — на месте.
    fn settle(ctx: &Context, screen: Vec2, at: Pos2) -> Rect {
        let mut rect = menu_frame(ctx, screen, at, true);
        for _ in 0..3 {
            rect = menu_frame(ctx, screen, at, false);
        }
        rect.expect("меню открыто")
    }

    /// Меню у верхней строки — под её кнопкой, правым краем по её правому краю, и целиком в окне.
    #[test]
    fn the_menu_opens_under_its_own_button_with_the_right_edges_aligned() {
        let screen = Vec2::new(460.0, 760.0);
        let at = Pos2::new(420.0, 200.0);
        for theme in [ThemeChoice::Dark, ThemeChoice::Light] {
            let ctx = context(theme);
            let rect = settle(&ctx, screen, at);
            let button = Rect::from_center_size(at, Vec2::splat(30.0));
            assert!(rect.top() >= button.bottom(), "{theme:?}: меню {rect:?} не под кнопкой {button:?}");
            assert!(rect.top() - button.bottom() < 12.0, "{theme:?}: меню далеко от кнопки: {rect:?}");
            assert!((rect.right() - button.right()).abs() < 3.0, "{theme:?}: правые края: {rect:?} и {button:?}");
            assert!(Rect::from_min_size(Pos2::ZERO, screen).contains_rect(rect), "{theme:?}: меню вне окна: {rect:?}");
        }
    }

    /// У нижней строки меню не лезет вниз — оно над кнопкой, а не сбоку поверх её строки.
    #[test]
    fn the_menu_of_the_last_row_goes_up() {
        let screen = Vec2::new(460.0, 680.0);
        let at = Pos2::new(420.0, 620.0);
        let rect = settle(&context(ThemeChoice::Dark), screen, at);
        let button = Rect::from_center_size(at, Vec2::splat(30.0));
        assert!(rect.bottom() <= button.top(), "меню {rect:?} не над кнопкой {button:?}");
        assert!(button.top() - rect.bottom() < 12.0, "меню далеко от кнопки: {rect:?}");
        assert!((rect.right() - button.right()).abs() < 3.0, "правые края: {rect:?} и {button:?}");
        assert!(rect.top() >= 0.0, "меню вышло за верх окна: {rect:?}");
    }

    /// Нигде не помещается — меню остаётся в окне и садится туда, где больше места.
    #[test]
    fn a_menu_that_fits_nowhere_takes_the_roomier_side_and_stays_in_the_window() {
        let screen = Vec2::new(460.0, 330.0);
        for (y, up) in [(100.0, false), (230.0, true)] {
            let at = Pos2::new(420.0, y);
            let rect = settle(&context(ThemeChoice::Light), screen, at);
            assert!(Rect::from_min_size(Pos2::ZERO, screen).contains_rect(rect), "y {y}: меню вне окна: {rect:?}");
            assert!((rect.center().y < at.y) == up, "y {y}: меню {rect:?} с неверной стороны от кнопки");
        }
    }

    #[test]
    fn the_side_follows_the_room() {
        assert!(!popup_goes_up(None, 10.0, 500.0), "высота ещё неизвестна — вниз");
        assert!(!popup_goes_up(Some(200.0), 300.0, 100.0), "внизу помещается");
        assert!(popup_goes_up(Some(200.0), 100.0, 300.0), "внизу нет, вверху есть");
        assert!(popup_goes_up(Some(400.0), 100.0, 250.0), "нигде не помещается — где больше места");
        assert!(!popup_goes_up(Some(400.0), 250.0, 100.0), "нигде не помещается — где больше места");
    }

    /// Ширина — по самой длинной подписи, не уже `floor`; клавиши справа добавляются к подписи.
    #[test]
    fn the_menu_width_fits_the_longest_label() {
        let ctx = context(ThemeChoice::Dark);
        let mut widths = (0.0, 0.0, 0.0, 0.0);
        run(&ctx, SCREEN, vec![], |_| {
            widths = (
                menu_width(&ctx, 220.0, &[("Настройки…", None), ("Проверять обновления при запуске…", None)]),
                menu_width(&ctx, 220.0, &[("Settings…", None), ("Check for updates…", None)]),
                menu_width(&ctx, 0.0, &[("Изменить…", Some("F2"))]),
                menu_width(&ctx, 0.0, &[("Изменить…", None)]),
            );
        });
        assert!(widths.0 > 236.0, "длинная русская подпись шире 220: {}", widths.0);
        assert_eq!(widths.1, 220.0, "короткие подписи — не уже `floor`");
        assert!(widths.2 > widths.3, "клавиши справа добавляются к подписи");
    }

    /// Закрытому меню ширина не нужна: подписи не мерятся на каждую строку списка в каждом кадре.
    #[test]
    fn a_closed_menu_does_not_measure_its_labels() {
        let ctx = context(ThemeChoice::Dark);
        let calls = std::cell::Cell::new(0);
        let frame = |open: bool| {
            run(&ctx, SCREEN, vec![], |ui| {
                let at = Rect::from_center_size(Pos2::new(420.0, 100.0), Vec2::splat(30.0));
                let button = ui.put(at, egui::Button::new("⋯"));
                if open {
                    Popup::open_id(ui.ctx(), Popup::default_response_id(&button));
                }
                let width = |_: &egui::Context| {
                    calls.set(calls.get() + 1);
                    236.0
                };
                menu_sized(&button, width, |ui| menu_item(ui, None, "Подключить", None));
            });
        };
        for _ in 0..3 {
            frame(false);
        }
        assert_eq!(calls.get(), 0, "закрытое меню мерило подписи");
        frame(true);
        assert_eq!(calls.get(), 1, "открываемое меню не померило подписи");
        frame(false);
        assert_eq!(calls.get(), 2, "открытое меню мерит подписи, пока открыто");
    }

    // ─── Ползунок ───────────────────────────────────────────────────────────

    fn slider_frame(ctx: &Context, value: &mut f32, events: Vec<Event>) -> Response {
        let mut response = None;
        run(ctx, SCREEN, events, |ui| {
            response = Some(Slider::new(value, 0.0..=1.0).step(0.02).text("40%").show(ui));
        });
        response.expect("ползунок нарисован")
    }

    /// В фокусе: стрелки — шаг, PageUp/PageDown — пять шагов, Home/End — края. Фокус без клавиш значение
    /// не трогает — даже не на сетке шага.
    #[test]
    fn the_slider_follows_the_keys() {
        let ctx = context(ThemeChoice::Dark);
        let mut value = 0.333;
        let r = slider_frame(&ctx, &mut value, vec![]);
        ctx.memory_mut(|m| m.request_focus(r.id));
        for _ in 0..2 {
            slider_frame(&ctx, &mut value, vec![]);
        }
        assert_eq!(value, 0.333, "фокус без клавиш сдвинул значение");
        value = 0.4;
        let press = |k: Key, value: &mut f32| {
            let r = slider_frame(&ctx, value, vec![key(k)]);
            assert!(r.has_focus(), "{k:?}: стрелки увели фокус с ползунка");
            *value
        };
        assert!((press(Key::ArrowRight, &mut value) - 0.42).abs() < 1e-4);
        assert!((press(Key::ArrowDown, &mut value) - 0.40).abs() < 1e-4);
        assert!((press(Key::PageUp, &mut value) - 0.50).abs() < 1e-4);
        assert_eq!(press(Key::End, &mut value), 1.0);
        assert_eq!(press(Key::ArrowRight, &mut value), 1.0, "за край не уходит");
        assert_eq!(press(Key::Home, &mut value), 0.0);
    }

    /// Колесо над ползунком — шаг на щелчок колеса; щелчок по дорожке ставит значение под курсором.
    #[test]
    fn the_slider_follows_the_wheel_and_the_pointer() {
        let ctx = context(ThemeChoice::Light);
        let mut value = 0.4;
        let r = slider_frame(&ctx, &mut value, vec![]);
        let over = r.rect.left_center() + Vec2::new(100.0, 0.0);
        slider_frame(&ctx, &mut value, vec![Event::PointerMoved(over)]);
        let wheel = |y: f32| Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: Vec2::new(0.0, y),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        };
        slider_frame(&ctx, &mut value, vec![wheel(1.0)]);
        assert!((value - 0.42).abs() < 1e-4, "колесо вверх: {value}");
        slider_frame(&ctx, &mut value, vec![wheel(-2.0)]);
        assert!((value - 0.38).abs() < 1e-4, "два щелчка вниз: {value}");
        // Дорожка — от 8 до (ширина − колонка значения − 8): её середина — 0.5.
        let track_left = r.rect.left() + 8.0;
        let track_right = r.rect.right() - (SLIDER_VALUE_W + 10.0) - 8.0;
        let middle = Pos2::new((track_left + track_right) / 2.0, r.rect.center().y);
        for events in click(middle) {
            slider_frame(&ctx, &mut value, events);
        }
        assert!((value - 0.5).abs() < 0.011, "щелчок по середине дорожки: {value}");
    }

    /// Квадратичная шкала: середина дорожки — четверть диапазона, а стрелки шагают по значению.
    #[test]
    fn the_quadratic_slider_puts_a_quarter_in_the_middle() {
        let ctx = context(ThemeChoice::Dark);
        let mut value = 0.4;
        let frame = |value: &mut f32, events: Vec<Event>| {
            let mut response = None;
            run(&ctx, SCREEN, events, |ui| {
                response = Some(Slider::new(value, 0.0..=1.0).step(0.02).quadratic(true).show(ui));
            });
            response.expect("ползунок нарисован")
        };
        let r = frame(&mut value, vec![]);
        let middle = Pos2::new((r.rect.left() + 8.0 + r.rect.right() - 8.0) / 2.0, r.rect.center().y);
        for events in click(middle) {
            frame(&mut value, events);
        }
        assert!((value - 0.25).abs() < 0.015, "середина квадратичной дорожки: {value}");
        let r = frame(&mut value, vec![]);
        assert!(r.has_focus(), "щелчок дал ползунку фокус");
        let before = value;
        frame(&mut value, vec![key(Key::ArrowRight)]);
        assert!((value - before - 0.02).abs() < 1e-4, "стрелка — шаг по значению: {before} → {value}");
    }

    // ─── Выбор ──────────────────────────────────────────────────────────────

    const OPTIONS: [(usize, &str); 6] =
        [(0, "Шум"), (1, "Вороной"), (2, "Кирпичи"), (3, "Соты"), (4, "Плитка"), (5, "Градиент")];

    fn select_frame(ctx: &Context, value: &mut usize, events: Vec<Event>) -> Response {
        let mut response = None;
        run(ctx, SCREEN, events, |ui| {
            ui.add_space(20.0);
            response = Some(select(ui, value, &OPTIONS, 200.0));
        });
        response.expect("выбор нарисован")
    }

    /// В фокусе ↑/↓ меняют значение без списка и не уводят фокус; на краях стоят.
    #[test]
    fn the_select_steps_with_the_arrows() {
        let ctx = context(ThemeChoice::Dark);
        let mut value = 1;
        let r = select_frame(&ctx, &mut value, vec![]);
        ctx.memory_mut(|m| m.request_focus(r.id));
        select_frame(&ctx, &mut value, vec![]);
        for (k, want) in [(Key::ArrowDown, 2), (Key::ArrowUp, 1), (Key::ArrowUp, 0), (Key::ArrowUp, 0)] {
            let r = select_frame(&ctx, &mut value, vec![key(k)]);
            assert_eq!(value, want, "{k:?}");
            assert!(r.has_focus(), "{k:?}: стрелка увела фокус");
        }
        assert!(!Popup::is_id_open(&ctx, Popup::default_response_id(&r)), "стрелки открыли список");
    }

    /// Щелчок открывает список под полем, щелчок по варианту ставит его и закрывает список.
    #[test]
    fn the_select_list_picks_a_value() {
        let ctx = context(ThemeChoice::Light);
        let mut value = 1;
        let r = select_frame(&ctx, &mut value, vec![]);
        for events in click(r.rect.center()) {
            select_frame(&ctx, &mut value, events);
        }
        let popup = Popup::default_response_id(&r);
        for _ in 0..3 {
            select_frame(&ctx, &mut value, vec![]);
        }
        assert!(Popup::is_id_open(&ctx, popup), "щелчок не открыл список");
        let list = ctx.memory(|m| m.area_rect(popup)).expect("список на экране");
        assert!(list.top() >= r.rect.bottom(), "список {list:?} не под полем {:?}", r.rect);
        assert!((list.left() - r.rect.left()).abs() < 1.0, "список {list:?} не от левого края поля {:?}", r.rect);
        assert!(list.width() >= r.rect.width() - 0.5, "список уже поля: {list:?}");
        // Пункты по 30 через 1, с отступом меню 5 и рамкой 1: четвёртый — «Соты».
        let fourth = Pos2::new(list.center().x, list.top() + 6.0 + 3.0 * 31.0 + 15.0);
        for events in click(fourth) {
            select_frame(&ctx, &mut value, events);
        }
        select_frame(&ctx, &mut value, vec![]);
        assert_eq!(value, 3);
        assert!(!Popup::is_id_open(&ctx, popup), "список не закрылся");
    }

    /// Длинный список не выше десяти строк: дальше прокрутка, а не край окна.
    #[test]
    fn a_long_select_list_scrolls() {
        let ctx = context(ThemeChoice::Dark);
        let options: Vec<(usize, String)> = (0..30).map(|i| (i, format!("Вариант {i}"))).collect();
        let options: Vec<(usize, &str)> = options.iter().map(|(i, text)| (*i, text.as_str())).collect();
        let mut value = 0;
        let screen = Vec2::new(460.0, 900.0);
        let mut id = None;
        for open in [true, false, false, false] {
            run(&ctx, screen, vec![], |ui| {
                let r = select(ui, &mut value, &options, 200.0);
                id = Some(Popup::default_response_id(&r));
                if open {
                    Popup::open_id(ui.ctx(), Popup::default_response_id(&r));
                }
            });
        }
        let list = ctx.memory(|m| m.area_rect(id.expect("выбор нарисован"))).expect("список открыт");
        let most = SELECT_ROWS as f32 * (MENU_ITEM + 1.0) + 2.0 * f32::from(POPUP_MARGIN) + 2.0;
        assert!(list.height() <= most + 0.5, "список {} выше {most}", list.height());
        assert!(list.height() > most - 40.0, "список {} обрезан короче десяти строк", list.height());
    }

    /// Равные сегменты: подписи помещаются — в ряд высотой 30, нет — столбиком по 26 на вариант.
    #[test]
    fn equal_segments_fall_into_a_column_when_labels_do_not_fit() {
        let ctx = context(ThemeChoice::Dark);
        let options = [(0, None, "При каждом запуске"), (1, None, "Раз в неделю"), (2, None, "Никогда")];
        let mut sizes = Vec::new();
        for width in [440.0, 220.0] {
            run(&ctx, SCREEN, vec![], |ui| {
                let mut value = 0;
                let r = ui.allocate_ui(Vec2::new(width, 0.0), |ui| segmented_full(ui, &mut value, &options)).inner;
                sizes.push((r.rect.width(), r.rect.height()));
            });
        }
        assert_eq!(sizes[0], (440.0, 30.0));
        assert_eq!(sizes[1], (220.0, 3.0 * 26.0 + 4.0));
    }

    /// Строки-переходы: щелчок по второй строке возвращает 1.
    #[test]
    fn nav_rows_report_the_clicked_row() {
        let ctx = context(ThemeChoice::Light);
        let rows = [(Some(Icon::Keyboard), "Горячие клавиши", ""), (Some(Icon::Layers), "Профили", "3")];
        let frame = |events: Vec<Event>| {
            let (mut clicked, mut top) = (None, 0.0);
            run(&ctx, SCREEN, events, |ui| {
                top = ui.cursor().top();
                clicked = nav_rows(ui, &rows);
            });
            (clicked, top)
        };
        let (clicked, top) = frame(vec![]);
        assert_eq!(clicked, None);
        let second = Pos2::new(200.0, top + NAV_ROW + 1.0 + NAV_ROW / 2.0);
        let [moved, down, up] = click(second);
        frame(moved);
        frame(down);
        assert_eq!(frame(up).0, Some(1));
    }
}
