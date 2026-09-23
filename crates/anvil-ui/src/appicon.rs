//! Значок программы для окна и exe: знак семьи (квадрат акцента со значком),
//! растеризованный без видеокарты.
//!
//! Фигуры берутся те же, что рисует [`crate::chrome::app_mark`]; epaint раскладывает
//! их на треугольники, а здесь они закрашиваются с 4×4 выборками на пиксель.

use eframe::egui::{self, Color32, Rect, Shape, Vec2};
use eframe::epaint::{Mesh, TessellationOptions, Tessellator};

use crate::icons::{self, Icon};
use crate::theme::Accent;

/// Значок `size`×`size`, RGBA без предумножения — как ждут `IconData` и ico.
pub fn rgba(accent: Accent, icon: Icon, size: u32) -> Vec<u8> {
    let swatch = accent.dark;
    let s = size as f32;
    let square = Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(s));
    let mut shapes = vec![Shape::rect_filled(square, s * 0.22, swatch.fill)];
    shapes.extend(icons::shapes(square.shrink(s * 0.19), icon, swatch.on_fill));

    let options = TessellationOptions { feathering: false, ..Default::default() };
    let mut tessellator = Tessellator::new(1.0, options, [1, 1], Vec::new());
    let mut mesh = Mesh::default();
    for shape in shapes {
        tessellator.tessellate_shape(shape, &mut mesh);
    }
    rasterize(&mesh, size)
}

/// Значок окна для `ViewportBuilder::with_icon`.
pub fn icon_data(accent: Accent, icon: Icon) -> egui::IconData {
    const SIZE: u32 = 64;
    egui::IconData { rgba: rgba(accent, icon, SIZE), width: SIZE, height: SIZE }
}

const SS: usize = 4;

/// Закрасить треугольники сетки поверх прозрачного фона, по порядку, с наложением «поверх».
fn rasterize(mesh: &Mesh, size: u32) -> Vec<u8> {
    let n = size as usize * SS;
    // Предумноженный цвет каждой выборки.
    let mut buf = vec![[0f32; 4]; n * n];
    let step = 1.0 / SS as f32;

    for tri in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| mesh.vertices[i as usize]);
        let area = edge(a.pos, b.pos, c.pos);
        if area.abs() < 1e-6 {
            continue;
        }
        let min_x = a.pos.x.min(b.pos.x).min(c.pos.x);
        let max_x = a.pos.x.max(b.pos.x).max(c.pos.x);
        let min_y = a.pos.y.min(b.pos.y).min(c.pos.y);
        let max_y = a.pos.y.max(b.pos.y).max(c.pos.y);
        let to_index = |v: f32| ((v / step).floor().max(0.0) as usize).min(n);
        for sy in to_index(min_y)..to_index(max_y + step) {
            for sx in to_index(min_x)..to_index(max_x + step) {
                let p = egui::pos2((sx as f32 + 0.5) * step, (sy as f32 + 0.5) * step);
                let w0 = edge(b.pos, c.pos, p) / area;
                let w1 = edge(c.pos, a.pos, p) / area;
                let w2 = edge(a.pos, b.pos, p) / area;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let color = mix(a.color, b.color, c.color, w0, w1, w2);
                let dst = &mut buf[sy * n + sx];
                let rest = 1.0 - color[3];
                for k in 0..4 {
                    dst[k] = color[k] + dst[k] * rest;
                }
            }
        }
    }

    let size = size as usize;
    let mut out = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let v = buf[(y * SS + sy) * n + x * SS + sx];
                    for k in 0..4 {
                        sum[k] += v[k];
                    }
                }
            }
            let count = (SS * SS) as f32;
            let alpha = sum[3] / count;
            let px = &mut out[(y * size + x) * 4..][..4];
            if alpha > 0.0 {
                for k in 0..3 {
                    px[k] = (sum[k] / count / alpha * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                px[3] = (alpha * 255.0).round() as u8;
            }
        }
    }
    out
}

fn edge(a: egui::Pos2, b: egui::Pos2, p: egui::Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

fn mix(a: Color32, b: Color32, c: Color32, wa: f32, wb: f32, wc: f32) -> [f32; 4] {
    let ch = |i: usize| (a[i] as f32 * wa + b[i] as f32 * wb + c[i] as f32 * wc) / 255.0;
    [ch(0), ch(1), ch(2), ch(3)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_is_opaque_inside_and_transparent_at_corner() {
        let size = 32;
        let px = rgba(Accent::EMBER, Icon::Hammer, size);
        let at = |x: usize, y: usize| &px[(y * size as usize + x) * 4..][..4];
        assert_eq!(at(0, 0)[3], 0, "угол скруглён");
        assert_eq!(at(3, size as usize / 2)[3], 255, "край квадрата залит");
        let fill = Accent::EMBER.dark.fill;
        assert_eq!(&at(3, size as usize / 2)[..3], &[fill.r(), fill.g(), fill.b()]);
    }
}
