//! Узоры обоев для egui-программ. Плитку рисует `anvil-wallpaper` (без окон — его
//! же берёт ядро телефона); здесь она становится текстурой и повторяется. Узоры,
//! правила и как взять их в клиент не на egui — docs/WALLPAPERS.md.
//!
//! ```ignore
//! let texture = self.tiles.get(ctx, Pattern::Snow, palette.chat_bg, palette.pattern);
//! if let Some(texture) = texture {
//!     anvil_ui::wallpaper::paint(ui.painter(), rect, &texture);
//! }
//! ```

use std::collections::HashMap;

use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, Vec2};

pub use anvil_wallpaper::{Pattern, STRENGTH, TILE, mix, tile};

use crate::Lang;

/// Плитка egui-картинкой: фон `bg` и узор цветом `ink` поверх, `scale` пикселей на
/// точку — чтобы на экране с масштабом 150 % узор не расплывался.
pub fn image(pattern: Pattern, scale: f32, bg: Color32, ink: Color32) -> ColorImage {
    let (side, rgba) = tile(pattern, scale, [bg.r(), bg.g(), bg.b()], [ink.r(), ink.g(), ink.b()]);
    ColorImage::from_rgba_unmultiplied([side, side], &rgba)
}

/// Название узора для людей на языке программы.
pub fn title(pattern: Pattern, lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => pattern.title_ru(),
        Lang::En => pattern.title_en(),
    }
}

/// Текстуры узоров: по одной на узор, цвета и масштаб экрана. Рисуются, когда
/// понадобились, — в ленте один узор, в выборе все.
#[derive(Default)]
pub struct Tiles {
    made: HashMap<(Pattern, [u8; 4], [u8; 4], u32), egui::TextureHandle>,
}

impl Tiles {
    /// Текстура узора на фоне `bg` цветом `ink`; `None` — «Без узора».
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        pattern: Pattern,
        bg: Color32,
        ink: Color32,
    ) -> Option<egui::TextureHandle> {
        if pattern == Pattern::Plain {
            return None;
        }
        let scale = ctx.pixels_per_point();
        let key = (pattern, bg.to_array(), ink.to_array(), (scale * 100.0).round() as u32);
        let texture = self.made.entry(key).or_insert_with(|| {
            let options =
                egui::TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..egui::TextureOptions::LINEAR };
            ctx.load_texture(format!("wallpaper-{}", pattern.code()), image(pattern, scale, bg, ink), options)
        });
        Some(texture.clone())
    }
}

/// Нарисовать узор, повторяя плитку, на весь `rect`. Узор стоит на месте, пока
/// содержимое прокручивается, — как обои, а не как часть переписки.
pub fn paint(painter: &egui::Painter, rect: Rect, texture: &egui::TextureHandle) {
    painter.image(texture.id(), rect, uv(rect.size()), Color32::WHITE);
}

/// Какая часть плитки ложится на прямоугольник этого размера: плитка — [`TILE`]
/// точек. Образец в выборе узора берёт `uv(size * 2.0)` — узор вдвое мельче,
/// иначе в маленьком образце видно одну-две фигуры.
pub fn uv(size: Vec2) -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(size.x / TILE, size.y / TILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_is_the_pattern_on_the_background_at_the_screen_scale() {
        let (bg, ink) = (Color32::from_rgb(14, 22, 33), Color32::from_rgb(159, 184, 208));
        let picture = image(Pattern::Amber, 1.0, bg, ink);
        assert_eq!(picture.size, [TILE as usize, TILE as usize]);
        assert!(picture.pixels.iter().all(|p| p.a() == 255), "the tile is opaque");
        let brightest = picture.pixels.iter().map(|p| p.r()).max().unwrap();
        assert_eq!(brightest, bg.lerp_to_gamma(ink, STRENGTH).r(), "mixed the way egui mixes");
        assert_eq!(image(Pattern::Snow, 1.5, bg, ink).size, [288, 288], "a 150 % screen gets a sharper tile");
    }

    #[test]
    fn a_tile_covers_its_own_size_and_titles_follow_the_language() {
        assert_eq!(uv(Vec2::splat(TILE)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)));
        assert_eq!(uv(Vec2::new(TILE * 2.0, TILE / 2.0)).max, Pos2::new(2.0, 0.5));
        assert_eq!(title(Pattern::Paws, Lang::Ru), "Лапки");
        assert_eq!(title(Pattern::Paws, Lang::En), "Paws");
    }
}
