//! Цвета, шрифты и стиль egui.
//!
//! Вид нейтральный, «инструментальный»: серые поверхности без оттенка и один
//! акцентный цвет. Акцент у каждой программы свой ([`Accent`]) — так семья
//! узнаётся по устройству окна, а программа — по цвету.
//!
//! Любой текст держит не меньше 4.5:1 к своему фону (WCAG 2.2 AA) в обеих темах
//! и со всеми акцентами. Следит за этим тест внизу файла.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle,
    Theme,
};

/// Какую тему показывать: как в системе, светлую или тёмную.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// Семейство полужирного шрифта: заголовки, кнопки, бейджи.
pub const SEMIBOLD: &str = "semibold";

/// Акцент одной темы.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swatch {
    /// Заливка главной кнопки, полоса прогресса, выбранное.
    pub fill: Color32,
    /// Текст и значки поверх `fill`.
    pub on_fill: Color32,
    /// Акцентный текст на обычном фоне: ссылки, выбранная вкладка.
    pub text: Color32,
}

/// Фирменный цвет программы — в тёмной и светлой теме.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accent {
    pub name: &'static str,
    pub dark: Swatch,
    pub light: Swatch,
}

impl Accent {
    /// Anvil: раскалённый металл.
    pub const EMBER: Accent = Accent {
        name: "Ember",
        dark: Swatch { fill: rgb(0xF0833A), on_fill: rgb(0x1B0E05), text: rgb(0xF59A5C) },
        light: Swatch { fill: rgb(0xC2531A), on_fill: rgb(0xFFFFFF), text: rgb(0xAD4712) },
    };
    /// Amber: янтарь.
    pub const AMBER: Accent = Accent {
        name: "Amber",
        dark: Swatch { fill: rgb(0xF2B54A), on_fill: rgb(0x1C1403), text: rgb(0xF2B54A) },
        light: Swatch { fill: rgb(0xF2B54A), on_fill: rgb(0x1C1403), text: rgb(0x8A5A00) },
    };
    /// Tetrachrome: бирюза.
    pub const TEAL: Accent = Accent {
        name: "Teal",
        dark: Swatch { fill: rgb(0x3CC8B4), on_fill: rgb(0x04201C), text: rgb(0x4FD6C3) },
        light: Swatch { fill: rgb(0x0E7C70), on_fill: rgb(0xFFFFFF), text: rgb(0x0B6E63) },
    };
    /// FFMincer: фарш.
    pub const ROSE: Accent = Accent {
        name: "Rose",
        dark: Swatch { fill: rgb(0xF0647A), on_fill: rgb(0x22060B), text: rgb(0xF4808F) },
        light: Swatch { fill: rgb(0xC22D48), on_fill: rgb(0xFFFFFF), text: rgb(0xB3263F) },
    };
    /// Нейтральный синий — для программ без своего цвета.
    pub const BLUE: Accent = Accent {
        name: "Blue",
        dark: Swatch { fill: rgb(0x6C9EF8), on_fill: rgb(0x06142E), text: rgb(0x86B0FA) },
        light: Swatch { fill: rgb(0x2A5FD0), on_fill: rgb(0xFFFFFF), text: rgb(0x2556C0) },
    };

    pub const ALL: [Accent; 5] = [Self::EMBER, Self::AMBER, Self::TEAL, Self::ROSE, Self::BLUE];

    pub fn swatch(&self, dark: bool) -> Swatch {
        if dark { self.dark } else { self.light }
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Все цвета одной темы с текущим акцентом.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Самый нижний фон окна: центральная область.
    pub bg: Color32,
    /// Боковые панели, шапка, полоса задач.
    pub surface: Color32,
    /// Карточки поверх фона.
    pub card: Color32,
    /// Кнопки, поля, дорожки переключателей.
    pub raised: Color32,
    /// Подсветка под курсором.
    pub hover: Color32,
    /// Разделители и рамки карточек.
    pub border: Color32,
    /// Рамка под курсором, рамка полей.
    pub border_strong: Color32,
    /// Фон поля ввода.
    pub field: Color32,
    pub text: Color32,
    /// Второстепенный текст: подписи, пояснения.
    pub weak: Color32,
    /// Самое тихое: плейсхолдеры, неактивные значки. Не для важного текста.
    pub faint: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub accent_text: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
}

impl Palette {
    pub fn new(dark: bool, accent: Accent) -> Palette {
        let a = accent.swatch(dark);
        if dark {
            Palette {
                dark,
                bg: rgb(0x0F0F11),
                surface: rgb(0x161618),
                card: rgb(0x1C1C1F),
                raised: rgb(0x252529),
                hover: rgb(0x2B2B30),
                border: rgb(0x2C2C31),
                border_strong: rgb(0x3D3D44),
                field: rgb(0x121214),
                text: rgb(0xECECEE),
                weak: rgb(0xA0A0A8),
                faint: rgb(0x6E6E76),
                accent: a.fill,
                on_accent: a.on_fill,
                accent_text: a.text,
                success: rgb(0x4CC38A),
                warning: rgb(0xE5B54A),
                danger: rgb(0xF2676B),
            }
        } else {
            Palette {
                dark,
                bg: rgb(0xF3F3F4),
                surface: rgb(0xFAFAFA),
                card: rgb(0xFFFFFF),
                raised: rgb(0xF0F0F2),
                hover: rgb(0xE8E8EB),
                border: rgb(0xE2E2E5),
                border_strong: rgb(0xCBCBD0),
                field: rgb(0xFFFFFF),
                text: rgb(0x17171A),
                weak: rgb(0x55555D),
                faint: rgb(0x8A8A92),
                accent: a.fill,
                on_accent: a.on_fill,
                accent_text: a.text,
                success: rgb(0x16703F),
                warning: rgb(0x8F5F00),
                danger: rgb(0xC4303A),
            }
        }
    }

    /// Палитра того, что сейчас на экране.
    pub fn of(ui: &egui::Ui) -> Palette {
        Self::of_ctx(ui.ctx())
    }

    pub fn of_ctx(ctx: &egui::Context) -> Palette {
        Palette::new(ctx.global_style().visuals.dark_mode, accent(ctx))
    }

    /// Мягкая подложка цвета: бейджи, выбранная строка, баннер.
    pub fn soft(&self, color: Color32) -> Color32 {
        color.gamma_multiply(if self.dark { 0.16 } else { 0.12 })
    }
}

fn accent_id() -> egui::Id {
    egui::Id::new("anvil-ui-accent")
}

/// Текущий акцент программы.
pub fn accent(ctx: &egui::Context) -> Accent {
    ctx.data(|d| d.get_temp(accent_id())).unwrap_or(Accent::BLUE)
}

/// Поставить шрифты и стиль. Звать один раз при запуске.
pub fn install(ctx: &egui::Context, accent: Accent, choice: ThemeChoice) {
    ctx.set_fonts(fonts());
    set_accent(ctx, accent);
    choose(ctx, choice);
}

/// Сменить акцент на ходу — стиль обеих тем пересобирается.
pub fn set_accent(ctx: &egui::Context, accent: Accent) {
    ctx.data_mut(|d| d.insert_temp(accent_id(), accent));
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, |style| apply(style, theme, accent));
    }
}

/// Переключить тему: системную, светлую или тёмную.
pub fn choose(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
}

/// Системный шрифт интерфейса, если он есть, а за ним — встроенные в egui:
/// в них символы и чёрно-белые эмодзи, которых в системном может не быть.
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_fallback = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    let regular = load_font(&mut fonts, "ui-regular", REGULAR_FONTS);
    let semibold = load_font(&mut fonts, "ui-semibold", SEMIBOLD_FONTS);
    let mono = load_font(&mut fonts, "ui-mono", MONO_FONTS);

    let mut proportional = fallback.clone();
    if let Some(name) = &regular {
        proportional.insert(0, name.clone());
    }
    let mut bold = proportional.clone();
    if let Some(name) = &semibold {
        bold.insert(0, name.clone());
    }
    let mut monospace = mono_fallback;
    if let Some(name) = &mono {
        monospace.insert(0, name.clone());
    }
    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), bold);
    fonts.families.insert(FontFamily::Monospace, monospace);
    fonts
}

#[cfg(windows)]
const REGULAR_FONTS: &[&str] = &[r"C:\Windows\Fonts\segoeui.ttf"];
#[cfg(windows)]
const SEMIBOLD_FONTS: &[&str] = &[r"C:\Windows\Fonts\seguisb.ttf", r"C:\Windows\Fonts\segoeuib.ttf"];
#[cfg(windows)]
const MONO_FONTS: &[&str] = &[r"C:\Windows\Fonts\CascadiaMono.ttf", r"C:\Windows\Fonts\consola.ttf"];

#[cfg(not(windows))]
const REGULAR_FONTS: &[&str] =
    &["/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", "/usr/share/fonts/noto/NotoSans-Regular.ttf"];
#[cfg(not(windows))]
const SEMIBOLD_FONTS: &[&str] =
    &["/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf", "/usr/share/fonts/noto/NotoSans-SemiBold.ttf"];
#[cfg(not(windows))]
const MONO_FONTS: &[&str] =
    &["/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", "/usr/share/fonts/TTF/DejaVuSansMono.ttf"];

/// Первый нашедшийся шрифт из списка. Нет ни одного — остаётся встроенный.
fn load_font(fonts: &mut FontDefinitions, name: &str, paths: &[&str]) -> Option<String> {
    let bytes = paths.iter().find_map(|path| std::fs::read(path).ok())?;
    fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
    Some(name.to_owned())
}

/// Радиусы скругления: мелкие детали, кнопки и поля, карточки, окна.
pub mod radius {
    pub const SMALL: u8 = 4;
    pub const CONTROL: u8 = 6;
    pub const CARD: u8 = 10;
    pub const WINDOW: u8 = 12;
}

fn apply(style: &mut egui::Style, theme: Theme, accent: Accent) {
    let dark = theme == Theme::Dark;
    let p = Palette::new(dark, accent);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };

    v.panel_fill = p.surface;
    v.window_fill = p.card;
    v.faint_bg_color = p.raised;
    v.extreme_bg_color = p.field;
    v.text_edit_bg_color = Some(p.field);
    v.code_bg_color = p.raised;
    v.hyperlink_color = p.accent_text;
    v.error_fg_color = p.danger;
    v.warn_fg_color = p.warning;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent_text);

    v.window_corner_radius = CornerRadius::same(radius::WINDOW);
    v.menu_corner_radius = CornerRadius::same(radius::CARD);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_shadow =
        Shadow { offset: [0, 12], blur: 36, spread: 0, color: Color32::from_black_alpha(if dark { 140 } else { 45 }) };
    v.popup_shadow =
        Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(if dark { 120 } else { 35 }) };

    let r = CornerRadius::same(radius::CONTROL);
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;
    for (state, fill, border) in [
        (&mut w.inactive, p.raised, p.border),
        (&mut w.hovered, p.hover, p.border_strong),
        (&mut w.active, p.soft(p.accent), p.accent),
        (&mut w.open, p.raised, p.border_strong),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, border);
        state.corner_radius = r;
        state.expansion = 0.0;
        state.fg_stroke = Stroke::new(1.5, p.text);
    }
    style.visuals = v;

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.button_padding = egui::vec2(10.0, 4.0);
    s.interact_size.y = 28.0;
    s.window_margin = Margin::same(20);
    s.menu_margin = Margin::same(6);
    s.icon_width = 16.0;
    s.icon_width_inner = 9.0;
    s.scroll = egui::style::ScrollStyle::floating();

    style.text_styles = [
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Heading, semibold(20.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Отношение контраста по WCAG 2.2: от 1 (одинаковые) до 21 (чёрное к белому).
    pub(crate) fn contrast(a: Color32, b: Color32) -> f32 {
        fn channel(c: u8) -> f32 {
            let c = f32::from(c) / 255.0;
            if c <= 0.039_28 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        let luminance = |c: Color32| 0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b());
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Полупрозрачная подложка поверх фона — так её и видит глаз.
    fn over(top: Color32, under: Color32) -> Color32 {
        let rest = 1.0 - f32::from(top.a()) / 255.0;
        let mix = |t: u8, u: u8| (f32::from(t) + f32::from(u) * rest).round().min(255.0) as u8;
        Color32::from_rgb(mix(top.r(), under.r()), mix(top.g(), under.g()), mix(top.b(), under.b()))
    }

    #[test]
    fn every_text_reads_on_its_background() {
        let mut failures = Vec::new();
        for accent in Accent::ALL {
            for dark in [true, false] {
                let p = Palette::new(dark, accent);
                let theme = if dark { "тёмная" } else { "светлая" };
                let mut pairs = vec![
                    ("текст на фоне", p.text, p.bg),
                    ("текст на карточке", p.text, p.card),
                    ("текст на кнопке", p.text, p.raised),
                    ("подпись на панели", p.weak, p.surface),
                    ("подпись на карточке", p.weak, p.card),
                    ("подпись на кнопке", p.weak, p.raised),
                    ("акцентный текст на панели", p.accent_text, p.surface),
                    ("акцентный текст на карточке", p.accent_text, p.card),
                    ("надпись главной кнопки", p.on_accent, p.accent),
                    ("акцент на выбранной строке", p.accent_text, over(p.soft(p.accent), p.surface)),
                    ("текст на выбранной строке", p.text, over(p.soft(p.accent), p.surface)),
                ];
                for (name, tone) in [("успех", p.success), ("внимание", p.warning), ("ошибка", p.danger)]
                {
                    pairs.push((name, tone, p.card));
                    pairs.push((name, tone, over(p.soft(tone), p.card)));
                }
                for (what, text, background) in pairs {
                    let ratio = contrast(text, background);
                    if ratio < 4.5 {
                        failures.push(format!("{} / {theme}: {what} — {ratio:.2}:1", accent.name));
                    }
                }
            }
        }
        failures.dedup();
        assert!(
            failures.is_empty(),
            "контраст ниже 4.5:1:
{}",
            failures.join(
                "
"
            )
        );
    }
}
