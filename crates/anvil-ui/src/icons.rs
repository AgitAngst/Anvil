//! Значки, нарисованные линиями в сетке 24×24.
//!
//! Шрифтовые значки выглядят по-разному на разных системах, и не у всех символов
//! есть глиф. Нарисованные кодом — одинаково чёткие при любом масштабе.

use std::f32::consts::{PI, TAU};

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    ArrowDown,
    ArrowRight,
    ArrowUp,
    /// Ветка git.
    Branch,
    Check,
    Clock,
    Close,
    /// `</>`: открыть в редакторе.
    Code,
    Download,
    File,
    Folder,
    Gear,
    /// Молоток: собрать.
    Hammer,
    Info,
    Lock,
    Moon,
    /// Три точки: ещё действия.
    More,
    /// Монитор: «как в системе».
    Monitor,
    /// Коробка: пакет, зависимость.
    Package,
    Pause,
    Pencil,
    Play,
    Plus,
    Refresh,
    /// Ракета: выпуск.
    Rocket,
    Search,
    Server,
    Stop,
    Sun,
    Terminal,
    Trash,
    Warning,
}

/// Нарисовать значок в квадрате `rect`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - egui::vec2(12.0, 12.0) * scale;
    let at = |x: f32, y: f32| origin + egui::vec2(x, y) * scale;
    let stroke = Stroke::new((1.7 * scale).max(1.2), color);
    let line = |points: Vec<Pos2>| painter.add(Shape::line(points, stroke));
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| {
        let steps = ((to - from).abs() * r).ceil().max(6.0) as usize;
        line(arc_points(cx, cy, r, from, to, steps).into_iter().map(|(x, y)| at(x, y)).collect())
    };
    let boxed = |a: Pos2, b: Pos2, r: f32| {
        painter.rect_stroke(Rect::from_min_max(a, b), r * scale, stroke, egui::StrokeKind::Middle)
    };

    match icon {
        Icon::ArrowDown => {
            line(vec![at(6.0, 9.5), at(12.0, 15.5), at(18.0, 9.5)]);
        }
        Icon::ArrowRight => {
            line(vec![at(9.5, 6.0), at(15.5, 12.0), at(9.5, 18.0)]);
        }
        Icon::ArrowUp => {
            line(vec![at(12.0, 19.0), at(12.0, 5.5)]);
            line(vec![at(6.5, 11.0), at(12.0, 5.5), at(17.5, 11.0)]);
        }
        Icon::Branch => {
            painter.circle_stroke(at(7.0, 5.5), 2.2 * scale, stroke);
            painter.circle_stroke(at(7.0, 18.5), 2.2 * scale, stroke);
            painter.circle_stroke(at(17.0, 7.5), 2.2 * scale, stroke);
            line(vec![at(7.0, 7.7), at(7.0, 16.3)]);
            line(vec![at(17.0, 9.7), at(17.0, 11.0), at(15.0, 13.5), at(9.0, 14.5), at(7.0, 16.3)]);
        }
        Icon::Check => {
            line(vec![at(5.0, 12.5), at(10.0, 17.5), at(19.5, 7.0)]);
        }
        Icon::Clock => {
            painter.circle_stroke(at(12.0, 12.0), 8.0 * scale, stroke);
            line(vec![at(12.0, 7.5), at(12.0, 12.0), at(15.0, 14.0)]);
        }
        Icon::Close => {
            line(vec![at(6.5, 6.5), at(17.5, 17.5)]);
            line(vec![at(17.5, 6.5), at(6.5, 17.5)]);
        }
        Icon::Code => {
            line(vec![at(8.0, 7.0), at(3.5, 12.0), at(8.0, 17.0)]);
            line(vec![at(16.0, 7.0), at(20.5, 12.0), at(16.0, 17.0)]);
            line(vec![at(13.5, 5.0), at(10.5, 19.0)]);
        }
        Icon::Download => {
            line(vec![at(12.0, 4.5), at(12.0, 15.5)]);
            line(vec![at(7.5, 11.0), at(12.0, 15.5), at(16.5, 11.0)]);
            line(vec![at(5.5, 19.5), at(18.5, 19.5)]);
        }
        Icon::File => {
            line(vec![at(7.0, 4.0), at(14.0, 4.0), at(18.0, 8.0), at(18.0, 20.0), at(7.0, 20.0), at(7.0, 4.0)]);
            line(vec![at(14.0, 4.0), at(14.0, 8.0), at(18.0, 8.0)]);
        }
        Icon::Folder => {
            line(vec![
                at(3.5, 7.0),
                at(3.5, 18.5),
                at(20.5, 18.5),
                at(20.5, 8.5),
                at(11.5, 8.5),
                at(9.5, 6.0),
                at(3.5, 6.0),
                at(3.5, 7.0),
            ]);
        }
        Icon::Gear => {
            painter.circle_stroke(at(12.0, 12.0), 3.0 * scale, stroke);
            let mut points = Vec::new();
            for i in 0..=48 {
                let a = i as f32 / 48.0 * TAU;
                let tooth = ((a * 8.0 / TAU).fract() - 0.5).abs() < 0.25;
                let r = if tooth { 8.5 } else { 6.6 };
                points.push(at(12.0 + r * a.cos(), 12.0 + r * a.sin()));
            }
            line(points);
        }
        Icon::Hammer => {
            // Рукоять по диагонали, боёк залит поперёк неё.
            line(vec![at(4.5, 19.5), at(13.0, 11.0)]);
            let (cx, cy) = (15.0, 9.0);
            let u = (0.707, 0.707); // вдоль бойка
            let v = (0.707, -0.707); // вдоль рукояти
            let corner = |a: f32, b: f32| at(cx + u.0 * a + v.0 * b, cy + u.1 * a + v.1 * b);
            let head =
                vec![corner(-6.5, -2.8), corner(4.5, -2.8), corner(6.5, 0.0), corner(4.5, 2.8), corner(-6.5, 2.8)];
            painter.add(Shape::convex_polygon(head, color, Stroke::NONE));
        }
        Icon::Info => {
            painter.circle_stroke(at(12.0, 12.0), 8.5 * scale, stroke);
            line(vec![at(12.0, 11.0), at(12.0, 16.5)]);
            painter.circle_filled(at(12.0, 7.8), 1.2 * scale, color);
        }
        Icon::Lock => {
            painter.rect_filled(Rect::from_min_max(at(5.5, 10.5), at(18.5, 20.5)), 2.5 * scale, color);
            arc(12.0, 10.5, 4.0, PI, TAU);
        }
        Icon::Moon => {
            // Серп: дуга большого круга снаружи малого и дуга малого внутри большого.
            let (big, small) = ((12.0, 12.0, 8.0), (16.5, 7.5, 6.5));
            let inside = |(x, y): (f32, f32), (cx, cy, r): (f32, f32, f32)| (x - cx).hypot(y - cy) < r;
            let ring = |(cx, cy, r): (f32, f32, f32)| arc_points(cx, cy, r, 0.0, TAU, 96);
            let outer = ring(big);
            let start =
                (0..outer.len()).find(|&i| inside(outer[i], small) && !inside(outer[(i + 1) % outer.len()], small));
            if let Some(start) = start {
                let mut points: Vec<(f32, f32)> = (1..outer.len())
                    .map(|k| outer[(start + k) % outer.len()])
                    .take_while(|&pt| !inside(pt, small))
                    .collect();
                let inner = ring(small);
                let from = (0..inner.len())
                    .find(|&i| inside(inner[i], big) && !inside(inner[(i + inner.len() - 1) % inner.len()], big));
                if let Some(from) = from {
                    let arc: Vec<(f32, f32)> = (0..inner.len())
                        .map(|k| inner[(from + k) % inner.len()])
                        .take_while(|&pt| inside(pt, big))
                        .collect();
                    points.extend(arc.into_iter().rev());
                }
                let points: Vec<Pos2> = points.into_iter().map(|(x, y)| at(x, y)).collect();
                painter.add(Shape::closed_line(points, stroke));
            }
        }
        Icon::More => {
            for x in [5.5, 12.0, 18.5] {
                painter.circle_filled(at(x, 12.0), 1.8 * scale, color);
            }
        }
        Icon::Monitor => {
            boxed(at(3.5, 4.5), at(20.5, 16.0), 2.0);
            line(vec![at(12.0, 16.0), at(12.0, 19.5)]);
            line(vec![at(8.0, 19.5), at(16.0, 19.5)]);
        }
        Icon::Package => {
            let outline =
                vec![at(12.0, 3.5), at(20.0, 7.5), at(20.0, 16.5), at(12.0, 20.5), at(4.0, 16.5), at(4.0, 7.5)];
            painter.add(Shape::closed_line(outline, stroke));
            line(vec![at(4.0, 7.5), at(12.0, 11.5), at(20.0, 7.5)]);
            line(vec![at(12.0, 11.5), at(12.0, 20.5)]);
        }
        Icon::Pause => {
            painter.rect_filled(Rect::from_min_max(at(7.0, 5.5), at(10.5, 18.5)), scale, color);
            painter.rect_filled(Rect::from_min_max(at(13.5, 5.5), at(17.0, 18.5)), scale, color);
        }
        Icon::Pencil => {
            line(vec![at(5.0, 19.0), at(6.0, 15.0), at(16.0, 5.0), at(19.0, 8.0), at(9.0, 18.0), at(5.0, 19.0)]);
            line(vec![at(14.0, 7.0), at(17.0, 10.0)]);
        }
        Icon::Play => {
            let points = vec![at(8.0, 5.5), at(18.5, 12.0), at(8.0, 18.5)];
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        Icon::Plus => {
            line(vec![at(12.0, 5.0), at(12.0, 19.0)]);
            line(vec![at(5.0, 12.0), at(19.0, 12.0)]);
        }
        Icon::Refresh => {
            arc(12.0, 12.0, 7.0, -0.35 * PI, 1.35 * PI);
            line(vec![at(15.5, 3.8), at(17.8, 6.2), at(14.6, 7.6)]);
        }
        Icon::Rocket => {
            let body = vec![at(12.0, 3.0), at(15.5, 7.5), at(15.5, 15.0), at(8.5, 15.0), at(8.5, 7.5)];
            painter.add(Shape::closed_line(body, stroke));
            line(vec![at(8.5, 11.5), at(5.5, 15.0), at(5.5, 17.5), at(8.5, 15.0)]);
            line(vec![at(15.5, 11.5), at(18.5, 15.0), at(18.5, 17.5), at(15.5, 15.0)]);
            line(vec![at(10.5, 17.5), at(12.0, 21.0), at(13.5, 17.5)]);
            painter.circle_stroke(at(12.0, 9.5), 1.4 * scale, stroke);
        }
        Icon::Search => {
            painter.circle_stroke(at(10.5, 10.5), 6.0 * scale, stroke);
            line(vec![at(15.0, 15.0), at(20.0, 20.0)]);
        }
        Icon::Server => {
            for top in [4.5, 13.0] {
                boxed(at(4.5, top), at(19.5, top + 6.5), 1.5);
                painter.circle_filled(at(8.0, top + 3.25), 1.1 * scale, color);
            }
        }
        Icon::Stop => {
            painter.rect_filled(Rect::from_min_max(at(6.5, 6.5), at(17.5, 17.5)), 2.0 * scale, color);
        }
        Icon::Sun => {
            painter.circle_stroke(at(12.0, 12.0), 4.0 * scale, stroke);
            for i in 0..8 {
                let a = i as f32 / 8.0 * TAU;
                let (s, c) = a.sin_cos();
                line(vec![at(12.0 + 6.8 * c, 12.0 + 6.8 * s), at(12.0 + 9.2 * c, 12.0 + 9.2 * s)]);
            }
        }
        Icon::Terminal => {
            boxed(at(3.5, 5.0), at(20.5, 19.0), 2.0);
            line(vec![at(7.0, 9.5), at(10.0, 12.0), at(7.0, 14.5)]);
            line(vec![at(12.0, 15.0), at(16.5, 15.0)]);
        }
        Icon::Trash => {
            line(vec![at(4.5, 7.0), at(19.5, 7.0)]);
            line(vec![at(9.5, 7.0), at(10.0, 4.5), at(14.0, 4.5), at(14.5, 7.0)]);
            line(vec![at(6.5, 7.0), at(7.5, 19.5), at(16.5, 19.5), at(17.5, 7.0)]);
            line(vec![at(10.5, 10.5), at(10.5, 16.5)]);
            line(vec![at(13.5, 10.5), at(13.5, 16.5)]);
        }
        Icon::Warning => {
            let outline = vec![at(12.0, 3.5), at(21.0, 19.5), at(3.0, 19.5)];
            painter.add(Shape::closed_line(outline, stroke));
            line(vec![at(12.0, 9.5), at(12.0, 14.0)]);
            painter.circle_filled(at(12.0, 16.8), 1.1 * scale, color);
        }
    }
}

fn arc_points(cx: f32, cy: f32, r: f32, from: f32, to: f32, steps: usize) -> Vec<(f32, f32)> {
    (0..=steps)
        .map(|i| {
            let a = from + (to - from) * i as f32 / steps as f32;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}
