//! Кривые времени: доля пути `t` (0..1) → доля результата.
//!
//! Все — кубические Безье `cubic-bezier(x1, y1, x2, y2)`, как в CSS. Новую кривую добавляй сюда:
//! функция `ease_*` и строка в [`Curve`], тест «начинается и кончается на месте» подхватит её сам.

/// Кубическая кривая Безье `cubic-bezier(x1, y1, x2, y2)` по доле времени `t`.
pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 || t == 1.0 {
        return t;
    }
    let curve = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    // Найти параметр s, при котором x(s) = t: делением пополам — кривая по x монотонна.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if curve(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    curve(y1, y2, (lo + hi) / 2.0)
}

/// Приход: быстро и мягкое торможение. `cubic-bezier(0.16, 1, 0.3, 1)`.
pub fn ease_out(t: f32) -> f32 {
    bezier(0.16, 1.0, 0.3, 1.0, t)
}

/// Уход: разгон и резкий конец. `cubic-bezier(0.7, 0, 0.84, 0)`. Уходы окна идут обратным ходом
/// прихода (так делает egui); эта кривая — для своих уходов.
pub fn ease_in(t: f32) -> f32 {
    bezier(0.7, 0.0, 0.84, 0.0, t)
}

/// Туда и обратно внутри одного цикла: мигание, стягивание колец. `cubic-bezier(0.4, 0, 0.2, 1)`.
pub fn ease_standard(t: f32) -> f32 {
    bezier(0.4, 0.0, 0.2, 1.0, t)
}

/// Приход с лёгким перелётом (до ~1.1) и возвратом: галочка, «щёлкнуло на место».
/// `cubic-bezier(0.34, 1.56, 0.64, 1)`. Не для постоянных переходов — только для одного акцента.
pub fn ease_pop(t: f32) -> f32 {
    bezier(0.34, 1.56, 0.64, 1.0, t)
}

/// Без ускорения: вращение, бегущая полоса.
pub fn linear(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// Кривая по имени — чтобы передавать её параметром и показывать в витрине.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Out,
    In,
    Standard,
    Pop,
    Linear,
}

impl Curve {
    pub const ALL: [Curve; 5] = [Curve::Out, Curve::In, Curve::Standard, Curve::Pop, Curve::Linear];

    pub fn at(self, t: f32) -> f32 {
        match self {
            Curve::Out => ease_out(t),
            Curve::In => ease_in(t),
            Curve::Standard => ease_standard(t),
            Curve::Pop => ease_pop(t),
            Curve::Linear => linear(t),
        }
    }

    /// Имя для витрины и документации.
    pub fn name(self) -> &'static str {
        match self {
            Curve::Out => "ease_out",
            Curve::In => "ease_in",
            Curve::Standard => "ease_standard",
            Curve::Pop => "ease_pop",
            Curve::Linear => "linear",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_start_and_end_in_place() {
        for curve in Curve::ALL {
            assert_eq!(curve.at(0.0), 0.0, "{}", curve.name());
            assert_eq!(curve.at(1.0), 1.0, "{}", curve.name());
        }
        // Без перелёта кривая не идёт назад.
        for curve in [Curve::Out, Curve::In, Curve::Standard, Curve::Linear] {
            let mut last = 0.0;
            for i in 1..=20 {
                let v = curve.at(i as f32 / 20.0);
                assert!(v >= last - 1e-4, "{}: кривая не идёт назад", curve.name());
                last = v;
            }
        }
        // Приход быстрый в начале, уход — в конце.
        assert!(ease_out(0.25) > 0.6, "{}", ease_out(0.25));
        assert!(ease_in(0.5) < 0.2, "{}", ease_in(0.5));
    }

    #[test]
    fn pop_overshoots_a_little_and_settles() {
        let peak = (1..100).map(|i| ease_pop(i as f32 / 100.0)).fold(0.0f32, f32::max);
        assert!((1.02..1.2).contains(&peak), "перелёт {peak}");
    }
}
