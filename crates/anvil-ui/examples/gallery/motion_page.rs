//! Вкладка «Движение» витрины: вся шкала, кривые и каждая анимация набора вживую.
//!
//! Открыть сразу: `cargo run -p anvil-ui --example gallery -- --tab motion [--reduced]`.
//! Новая анимация в `anvil_ui::motion` — сюда строку-пример, так её видно и проверяют глазами
//! в обеих темах и на обоих языках, в том числе с `--reduced` («меньше движения»).

use anvil_ui::icons::{self, Icon};
use anvil_ui::motion::{self, Curve, Motion, effects, widgets as fx};
use anvil_ui::widgets::{self as w, Kind, Tone};
use anvil_ui::{Lang, Palette, semibold};
use eframe::egui::{self, Align2, FontId, Rect, RichText, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Copy, PartialEq)]
enum Orb {
    Waiting,
    Connected,
    Lost,
}

#[derive(Clone, Copy, PartialEq)]
enum Pick {
    First,
    Second,
    Third,
}

pub struct MotionDemo {
    reduced: bool,
    orb: Orb,
    orb_at: f64,
    fraction: f32,
    result_key: u32,
    result_ok: bool,
    shake_fire: bool,
    field: String,
    counter: u32,
    list_key: u32,
    reveal: bool,
    color_on: bool,
    pick: Pick,
    tab: usize,
    busy_chain: bool,
    /// `--scroll px`: на первом кадре прокрутить вкладку вниз — для снимков нижней половины.
    scroll: f32,
}

impl MotionDemo {
    pub fn new(reduced: bool, scroll: f32) -> Self {
        Self {
            reduced,
            orb: Orb::Waiting,
            orb_at: 0.0,
            fraction: 0.35,
            result_key: 0,
            result_ok: true,
            shake_fire: false,
            field: "amber-desktop --profile".into(),
            counter: 41,
            list_key: 0,
            reveal: true,
            color_on: false,
            pick: Pick::First,
            tab: 0,
            busy_chain: true,
            scroll,
        }
    }
}

/// Строка на двух языках: витрина показывает и русский, и английский.
fn tx<'a>(ctx: &egui::Context, ru: &'a str, en: &'a str) -> &'a str {
    if anvil_ui::lang::language(ctx) == Lang::Ru { ru } else { en }
}

/// Подпись слева, пример справа.
fn demo_row(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(150.0, 26.0), Sense::hover());
        ui.painter().text(
            egui::pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            p.weak,
        );
        content(ui);
    });
    ui.add_space(4.0);
}

impl MotionDemo {
    pub fn show(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        let ctx = ui.ctx().clone();
        ui.label(RichText::new(tx(&ctx, "Движение", "Motion")).font(semibold(24.0)).color(p.text));
        w::note(
            ui,
            tx(
                &ctx,
                "Шкала времени, кривые и готовые анимации набора. Всё из anvil_ui::motion; описание — docs/MOTION.md.",
                "Time scale, curves and the kit's ready animations. All from anvil_ui::motion; guide — docs/MOTION.md.",
            ),
        );
        ui.add_space(10.0);
        if w::switch(
            ui,
            &mut self.reduced,
            tx(
                &ctx,
                "Меньше движения (как «Показывать анимацию» выключено в Windows)",
                "Reduced motion (like “Show animations” off in Windows)",
            ),
        )
        .changed()
        {
            motion::set_reduced(&ctx, self.reduced);
        }
        ui.add_space(12.0);
        if self.scroll != 0.0 {
            ui.scroll_with_delta(Vec2::new(0.0, -std::mem::take(&mut self.scroll)));
        }

        ui.columns(2, |cols| {
            let (left, right) = cols.split_at_mut(1);
            self.left(&mut left[0]);
            self.right(&mut right[0]);
        });
    }

    fn left(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Clock, tx(&ctx, "Шкала", "Scale"));
            scale_table(ui);
        });
        ui.add_space(12.0);
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Play, tx(&ctx, "Кривые", "Curves"));
            curves(ui);
        });
    }

    fn right(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let p = Palette::of(ui);
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Broadcast, tx(&ctx, "Ожидание и «Подключено»", "Waiting and “Connected”"));
            ui.horizontal(|ui| {
                let before = self.orb;
                w::segmented(
                    ui,
                    &mut self.orb,
                    &[
                        (Orb::Waiting, None, tx(&ctx, "Подключаюсь", "Connecting")),
                        (Orb::Connected, None, tx(&ctx, "Подключено", "Connected")),
                        (Orb::Lost, None, tx(&ctx, "Нет ответа", "No answer")),
                    ],
                );
                if before != self.orb {
                    self.orb_at = ui.input(|i| i.time);
                }
                if w::button(ui, Kind::Ghost, Some(Icon::Refresh), tx(&ctx, "Заново", "Replay")).clicked() {
                    self.orb_at = ui.input(|i| i.time);
                }
            });
            ui.add_space(4.0);
            ui.vertical_centered(|ui| self.draw_orb(ui));
        });
        ui.add_space(12.0);

        w::card(ui, |ui| {
            w::card_title(ui, Icon::Refresh, tx(&ctx, "Идёт работа", "Work in progress"));
            demo_row(ui, tx(&ctx, "Крутилка", "Spinner"), |ui| {
                w::spinner(ui, 18.0);
            });
            demo_row(ui, tx(&ctx, "Полоса, неизвестно", "Bar, unknown"), |ui| {
                w::progress(ui, None, 220.0);
            });
            demo_row(ui, tx(&ctx, "Полоса, доля", "Bar, fraction"), |ui| {
                w::progress(ui, Some(self.fraction), 220.0);
                ui.add(egui::Slider::new(&mut self.fraction, 0.0..=1.0).show_value(false));
            });
            demo_row(ui, tx(&ctx, "Цепочка", "Chain"), |ui| self.chain(ui));
            demo_row(ui, tx(&ctx, "Живая точка", "Live dot"), |ui| {
                fx::live_dot(ui, Tone::Success);
                w::note(ui, tx(&ctx, "связь есть", "connected"));
            });
            demo_row(ui, tx(&ctx, "«Печатает…»", "“Typing…”"), |ui| {
                fx::typing_dots(ui);
            });
            demo_row(ui, tx(&ctx, "Скелетон", "Skeleton"), |ui| {
                ui.vertical(|ui| {
                    fx::skeleton(ui, Vec2::new(240.0, 12.0));
                    ui.add_space(4.0);
                    fx::skeleton(ui, Vec2::new(180.0, 12.0));
                });
            });
        });
        ui.add_space(12.0);

        w::card(ui, |ui| {
            w::card_title(ui, Icon::Check, tx(&ctx, "Результат и внимание", "Result and attention"));
            demo_row(ui, tx(&ctx, "Галочка / крестик", "Check / cross"), |ui| {
                fx::result_mark(ui, (self.result_key, self.result_ok), self.result_ok, 28.0);
                if w::button(ui, Kind::Secondary, None, tx(&ctx, "Успех", "Success")).clicked() {
                    self.result_ok = true;
                    self.result_key += 1;
                }
                if w::button(ui, Kind::Secondary, None, tx(&ctx, "Ошибка", "Failure")).clicked() {
                    self.result_ok = false;
                    self.result_key += 1;
                }
            });
            demo_row(ui, tx(&ctx, "Встряска", "Shake"), |ui| {
                let fire = std::mem::take(&mut self.shake_fire);
                fx::shake(ui, "field", fire, 6.0, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.field).desired_width(190.0));
                });
                if w::button(ui, Kind::Danger, None, tx(&ctx, "Не так", "Wrong")).clicked() {
                    self.shake_fire = true;
                }
            });
            demo_row(ui, tx(&ctx, "Вспышка", "Flash"), |ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(150.0, 26.0), Sense::hover());
                fx::changed_highlight(ui, rect, "counter", self.counter);
                ui.painter().text(
                    rect.left_center() + Vec2::new(8.0, 0.0),
                    Align2::LEFT_CENTER,
                    format!("{} {}", tx(&ctx, "Запусков:", "Runs:"), self.counter),
                    FontId::proportional(13.5),
                    p.text,
                );
                if w::button(ui, Kind::Secondary, None, "+1").clicked() {
                    self.counter += 1;
                }
            });
            demo_row(ui, tx(&ctx, "Появление списком", "List enter"), |ui| {
                if w::button(ui, Kind::Secondary, Some(Icon::Refresh), tx(&ctx, "Заново", "Replay")).clicked() {
                    self.list_key += 1;
                }
            });
            for (i, name) in ["amber-desktop", "amber-server", "tetrachrome", "ffmincer"].iter().enumerate() {
                fx::enter(ui, (self.list_key, i), i, |ui| {
                    ui.horizontal(|ui| {
                        w::dot(ui, if i == 1 { Tone::Warning } else { Tone::Success });
                        w::mono(ui, name, Some(p.text));
                    });
                });
            }
        });
        ui.add_space(12.0);
        w::card(ui, |ui| {
            w::card_title(ui, Icon::Layers, tx(&ctx, "Ползунок и раскрытие", "Slider and reveal"));
            demo_row(ui, tx(&ctx, "Сегменты", "Segments"), |ui| {
                w::segmented(
                    ui,
                    &mut self.pick,
                    &[
                        (Pick::First, None, tx(&ctx, "Один", "One")),
                        (Pick::Second, None, tx(&ctx, "Два", "Two")),
                        (Pick::Third, None, tx(&ctx, "Три", "Three")),
                    ],
                );
            });
            ui.add_space(6.0);
            w::tabs(
                ui,
                &mut self.tab,
                &[
                    tx(&ctx, "Коммиты", "Commits"),
                    tx(&ctx, "Зависимости", "Dependencies"),
                    tx(&ctx, "Выпуски", "Releases"),
                ],
            );
            ui.add_space(10.0);
            demo_row(ui, tx(&ctx, "Раскрытие", "Reveal"), |ui| {
                w::toggle(ui, &mut self.reveal, tx(&ctx, "Показать подробности", "Show details"));
            });
            fx::reveal(ui, "details", self.reveal, |ui| {
                w::banner(
                    ui,
                    Tone::Accent,
                    tx(&ctx, "Подробности.", "Details."),
                    tx(
                        &ctx,
                        "Высота выезжает за 200 мс, содержимое проявляется.",
                        "Height slides in 200 ms while the content fades in.",
                    ),
                    |_| {},
                );
            });
            demo_row(ui, tx(&ctx, "Цвет", "Colour"), |ui| {
                let motion = Motion::of(ui.ctx());
                let p = Palette::of(ui);
                let target = if self.color_on { p.success } else { p.danger };
                let shown = motion.color(ui.ctx(), ui.id().with("color-demo"), target, motion::STATE);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(64.0, 22.0), Sense::hover());
                ui.painter().rect_filled(rect, 6, shown);
                if w::button(ui, Kind::Secondary, None, tx(&ctx, "Сменить", "Switch")).clicked() {
                    self.color_on = !self.color_on;
                }
            });
        });
    }

    /// Пять звеньев цепочки: «идёт» мигает, остальные ровные.
    fn chain(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        let ctx = ui.ctx().clone();
        let motion = Motion::of(&ctx);
        let alpha = if self.busy_chain { effects::blink(motion.cycle(&ctx, motion::BLINK)) } else { 1.0 };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(120.0, 20.0), Sense::hover());
        for i in 0..5 {
            let c = egui::pos2(rect.left() + 8.0 + 26.0 * i as f32, rect.center().y);
            let (fill, border) = if i == 3 && self.busy_chain {
                (p.accent.gamma_multiply(alpha), p.accent.gamma_multiply(alpha))
            } else if i < 3 {
                (p.success, p.success)
            } else {
                (p.bg, p.border_strong)
            };
            ui.painter().circle(c, 4.25, fill, Stroke::new(1.5, border));
        }
        w::toggle(ui, &mut self.busy_chain, "");
    }

    /// Круг подключения из Morok: дымка стягивается, сгущается в кольцо и расходится волной.
    fn draw_orb(&self, ui: &mut Ui) {
        let p = Palette::of(ui);
        let ctx = ui.ctx().clone();
        let motion = Motion::of(&ctx);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(192.0), Sense::hover());
        let c = rect.center();
        let painter = ui.painter();
        let radii = [71.0, 83.5, 96.0];
        let alphas = [0.9, 0.5, 0.22];
        let (fill, border, fg, label) = match self.orb {
            Orb::Waiting => {
                if motion.enabled {
                    effects::gather_rings(painter, c, &radii, p.accent, motion.cycle(&ctx, motion::GATHER));
                }
                effects::spin_arc(painter, c, 66.5, 3.0, p.accent, motion.cycle(&ctx, motion::SPIN), true);
                (p.card, p.border_strong, p.weak, tx(&ctx, "Подключаюсь", "Connecting"))
            }
            Orb::Connected => {
                let t = motion.once(&ctx, self.orb_at, 0.0, motion::EMPHASIS);
                let wave = motion.once(&ctx, self.orb_at, motion::WAVE_DELAY, motion::WAVE);
                let outer: Vec<(f32, f32)> = radii[1..].iter().copied().zip(alphas[1..].iter().copied()).collect();
                let settle = effects::Settle { core: 68.0, rings: &outer, haze: p.success, wave: p.success };
                effects::settle(painter, c, &settle, t, wave);
                (
                    p.badge_fill(p.success),
                    p.success.gamma_multiply(0.45),
                    p.success,
                    tx(&ctx, "Подключено", "Connected"),
                )
            }
            Orb::Lost => {
                let haze = p.warning.gamma_multiply(0.55);
                for (radius, a) in radii.iter().zip(alphas) {
                    effects::ring(painter, c, *radius, 1.0, haze.gamma_multiply(a), true);
                }
                effects::spin_arc(painter, c, 66.5, 3.0, p.warning, motion.cycle(&ctx, motion::SPIN_SLOW), false);
                (
                    p.badge_fill(p.warning),
                    p.warning.gamma_multiply(0.45),
                    p.warning,
                    tx(&ctx, "Нет ответа", "No answer"),
                )
            }
        };
        painter.circle(c, 59.0, fill, Stroke::new(1.0, border));
        icons::paint(painter, Rect::from_center_size(c - Vec2::new(0.0, 12.0), Vec2::splat(22.0)), Icon::Lock, fg);
        painter.text(c + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, label, semibold(13.0), fg);
    }
}

/// Вся шкала: имя константы, миллисекунды, кривая, что за переход.
fn scale_table(ui: &mut Ui) {
    let p = Palette::of(ui);
    let ctx = ui.ctx().clone();
    let ru = anvil_ui::lang::language(&ctx) == Lang::Ru;
    for step in motion::SCALE {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
            let y = rect.center().y;
            let painter = ui.painter();
            painter.text(egui::pos2(rect.left(), y), Align2::LEFT_CENTER, step.name, FontId::monospace(12.5), p.text);
            let ms = format!("{} мс", (step.secs * 1000.0).round());
            let ms = if ru { ms } else { ms.replace("мс", "ms") };
            painter.text(
                egui::pos2(rect.left() + 118.0, y),
                Align2::LEFT_CENTER,
                ms,
                FontId::monospace(12.5),
                p.accent_text,
            );
            painter.text(
                egui::pos2(rect.left() + 188.0, y),
                Align2::LEFT_CENTER,
                step.curve.name(),
                FontId::monospace(12.0),
                p.weak,
            );
            painter.text(
                egui::pos2(rect.left() + 308.0, y),
                Align2::LEFT_CENTER,
                if ru { step.what } else { english(step.name) },
                FontId::proportional(12.5),
                p.weak,
            );
        });
    }
}

/// Английские пояснения к шкале — только для витрины (в коде набора пояснения по-русски).
fn english(name: &str) -> &'static str {
    match name {
        "PRESS" => "press: down to 0.97",
        "HOVER" => "highlight under the pointer",
        "STATE" => "state change, dialog, menu",
        "LEAVE" => "dialog, menu, popup leaving",
        "LAYOUT" => "banner, collapse, tab slider",
        "ENTER" => "list row appearing",
        "GRAPH" => "new graph point, scale",
        "SHAKE" => "shake on error",
        "CHECK" => "result check drawn",
        "FLASH" => "changed value flash",
        "EMPHASIS" => "“Connected”: haze into ring",
        "WAVE" => "wave after “Connected”",
        "SPIN" => "waiting arc turn",
        "SPIN_SLOW" => "reconnecting arc turn",
        "GATHER" => "rings gather",
        "BLINK" => "chain link blink",
        "SHIMMER" => "skeleton sheen",
        "PING" => "live dot ring",
        "DOTS" => "typing dots",
        _ => "",
    }
}

/// Каждая кривая: график и точка, которая бежит по дорожке с её ускорением.
fn curves(ui: &mut Ui) {
    let p = Palette::of(ui);
    let ctx = ui.ctx().clone();
    let motion = Motion::of(&ctx);
    // Бег 1.2 с и пауза: доля цикла 0..1 → ход 0..0.7, стоим.
    let phase = motion.cycle(&ctx, 2.0);
    let run = (phase / 0.6).min(1.0);
    for curve in Curve::ALL {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::hover());
            let painter = ui.painter();
            painter.text(
                egui::pos2(rect.left(), rect.center().y),
                Align2::LEFT_CENTER,
                curve.name(),
                FontId::monospace(12.5),
                p.text,
            );
            let plot = Rect::from_min_size(egui::pos2(rect.left() + 118.0, rect.top() + 6.0), Vec2::new(72.0, 32.0));
            painter.rect_stroke(plot, 3, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
            let line: Vec<egui::Pos2> = (0..=40)
                .map(|i| {
                    let t = i as f32 / 40.0;
                    // Перелёт `ease_pop` выше рамки не рисуем — обрезаем на верхнем крае.
                    egui::pos2(
                        plot.left() + t * plot.width(),
                        plot.bottom() - curve.at(t).min(1.15) * plot.height() * 0.87,
                    )
                })
                .collect();
            painter
                .with_clip_rect(plot.expand2(Vec2::new(0.0, 3.0)))
                .add(egui::Shape::line(line, Stroke::new(1.6, p.accent)));
            let track =
                Rect::from_min_size(egui::pos2(plot.right() + 22.0, rect.center().y - 1.0), Vec2::new(180.0, 2.0));
            painter.rect_filled(track, 1, p.border_strong);
            let x = track.left() + curve.at(run) * track.width();
            painter.circle_filled(egui::pos2(x, track.center().y), 6.0, p.accent);
        });
    }
}
