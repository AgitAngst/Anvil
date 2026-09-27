//! Общие виджеты. Всё рисуется из [`Palette`], поэтому само следует за темой и акцентом.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Margin, Rect, Response, RichText, Sense, Stroke,
    StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};

use crate::icons::{self, Icon};
use crate::lang::tr;
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

/// Сегментный выбор: «Система · Светлая · Тёмная».
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, Option<Icon>, &str)]) -> Response {
    let p = Palette::of(ui);
    let font = FontId::proportional(13.0);
    let widths: Vec<f32> = options
        .iter()
        .map(|(_, icon, text)| {
            let w = ui.painter().layout_no_wrap(text.to_string(), font.clone(), p.text).size().x;
            w + if icon.is_some() { 21.0 } else { 0.0 } + 20.0
        })
        .collect();
    let size = Vec2::new(widths.iter().sum::<f32>() + 4.0, 30.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(rect, radius::CONTROL + 1, p.raised, Stroke::new(1.0, p.border), StrokeKind::Inside);

    let mut x = rect.left() + 2.0;
    for (i, ((option, icon, text), w)) in options.iter().zip(widths).enumerate() {
        let seg = Rect::from_min_size(egui::pos2(x, rect.top() + 2.0), Vec2::new(w, rect.height() - 4.0));
        let id = response.id.with(i);
        let r = ui.interact(seg, id, Sense::click());
        let selected = *value == *option;
        if r.clicked() && !selected {
            *value = *option;
            response.mark_changed();
        }
        if selected {
            ui.painter().rect(seg, radius::CONTROL - 1, p.card, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
        } else if r.hovered() {
            ui.painter().rect_filled(seg, radius::CONTROL - 1, p.hover);
        }
        let color = if selected { p.text } else { p.weak };
        let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), color);
        let content_w = galley.size().x + if icon.is_some() { 21.0 } else { 0.0 };
        let mut cx = seg.center().x - content_w / 2.0;
        if let Some(icon) = icon {
            let ir = Rect::from_min_size(egui::pos2(cx, seg.center().y - 8.0), Vec2::splat(16.0));
            icons::paint(ui.painter(), ir, *icon, color);
            cx += 21.0;
        }
        ui.painter().galley(egui::pos2(cx, seg.center().y - galley.size().y / 2.0), galley, color);
        focus_ring(ui, seg, &r, radius::CONTROL);
        r.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, *text));
        r.on_hover_cursor(CursorIcon::PointingHand);
        x += w;
    }
    response
}

/// Вкладки с полоской акцента под выбранной. Возвращает, сменилась ли вкладка.
pub fn tabs(ui: &mut Ui, selected: &mut usize, labels: &[&str]) -> bool {
    let p = Palette::of(ui);
    let mut changed = false;
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
                let bar = Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.0), rect.right_bottom());
                ui.painter().rect_filled(bar, 1, p.accent);
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
    ui.painter().rect_filled(rect, 3, p.raised);
    let fill = match fraction {
        Some(f) => Rect::from_min_size(rect.min, Vec2::new(rect.width() * f.clamp(0.0, 1.0), rect.height())),
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
    let t = ui.input(|i| i.time) as f32;
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
    ui.ctx().request_repaint();
    response
}

// ─── Меню ───────────────────────────────────────────────────────────────────

/// Выпадающее меню под кнопкой: открывается и закрывается её щелчком.
pub fn menu<R>(button: &Response, width: f32, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let p = Palette::of_ctx(&button.ctx);
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border_strong))
        .corner_radius(radius::CARD)
        .shadow(button.ctx.global_style().visuals.popup_shadow)
        .inner_margin(Margin::same(5));
    egui::Popup::menu(button)
        .width(width)
        .gap(4.0)
        .frame(frame)
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            content(ui)
        })
        .map(|r| r.inner)
}

/// Пункт меню: значок, текст, клавиши справа. Щелчок закрывает меню.
pub fn menu_item(ui: &mut Ui, icon: Option<Icon>, text: &str, shortcut: Option<&str>) -> Response {
    menu_item_toned(ui, icon, text, shortcut, false)
}

/// Опасный пункт меню: красный.
pub fn menu_item_danger(ui: &mut Ui, icon: Option<Icon>, text: &str) -> Response {
    menu_item_toned(ui, icon, text, None, true)
}

fn menu_item_toned(ui: &mut Ui, icon: Option<Icon>, text: &str, shortcut: Option<&str>, danger: bool) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let enabled = ui.is_enabled();
    let hovered = response.hovered() && enabled;
    let color = match (enabled, danger) {
        (false, _) => p.faint,
        (true, true) => p.danger,
        (true, false) => p.text,
    };
    if hovered {
        let fill = if danger { p.badge_fill(p.danger) } else { p.hover };
        ui.painter().rect_filled(rect, radius::CONTROL, fill);
    }
    if let Some(icon) = icon {
        let r = Rect::from_min_size(egui::pos2(rect.left() + 9.0, rect.center().y - 8.0), Vec2::splat(16.0));
        icons::paint(ui.painter(), r, icon, if danger || hovered { color } else { p.weak });
    }
    let x = rect.left() + if icon.is_some() { 34.0 } else { 10.0 };
    ui.painter().text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(14.0), color);
    if let Some(keys) = shortcut {
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            keys,
            FontId::proportional(12.5),
            p.faint,
        );
    }
    if response.clicked() {
        ui.close();
    }
    name(&response, WidgetType::Button, text);
    response
}

/// Разделитель между группами пунктов меню.
pub fn menu_separator(ui: &mut Ui) {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(rect.x_range().shrink(6.0), rect.center().y, Stroke::new(1.0, p.border));
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

    pub fn show(&mut self, ctx: &egui::Context) {
        self.items.retain(|(_, _, at)| at.elapsed() < Self::LIFE);
        if self.items.is_empty() {
            return;
        }
        let p = Palette::of_ctx(ctx);
        egui::Area::new(egui::Id::new("anvil-toasts"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -48.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for (text, tone, at) in &self.items {
                    let age = at.elapsed().as_secs_f32();
                    let fade = ((Self::LIFE.as_secs_f32() - age) / 0.3).clamp(0.0, 1.0) * (age / 0.15).clamp(0.0, 1.0);
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
/// посередине. `accent` — цвет программы семьи; `None` — нейтральный знак (Godot, Unity, набор).
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

/// Стопка знаков набора: знаки 28, каждый следующий на 18 правее, с кольцом 2 px цвета `under`.
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

/// Узел цепочки набора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainNode {
    Running,
    Stopped,
    Crashed,
    Starting,
}

/// Цепочка набора: узлы по порядку шагов и справа текст («2 из 3 работают»).
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
pub fn list_row(ui: &mut Ui, id: egui::Id, selected: bool, height: f32, label: &str) -> Response {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    if selected {
        ui.painter().rect_filled(rect, radius::CONTROL, p.raised);
        let bar = Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - 14.0), Vec2::new(3.0, 28.0));
        ui.painter().rect_filled(bar, 2, p.accent);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, radius::CONTROL, p.hover);
    }
    focus_ring(ui, rect, &response, radius::CONTROL);
    response.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, label));
    response
}
