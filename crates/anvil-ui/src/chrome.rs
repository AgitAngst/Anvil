//! Каркас окна: панели с общими отступами, знак программы, диалоги
//! «О программе» и «Настройки», общие настройки семьи.

use eframe::egui::{self, Color32, Margin, RichText, Sense, Stroke, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::lang::{self, Lang, tr};
use crate::theme::{self, Accent, Palette, ThemeChoice, radius, semibold};
use crate::widgets::{self as w, Kind};

// ─── Панели ─────────────────────────────────────────────────────────────────

/// Высота шапки и строки состояния — одна на все программы.
pub const TOP_BAR: f32 = 56.0;
pub const STATUS_BAR: f32 = 38.0;

fn bar_frame(p: &Palette, x: i8) -> egui::Frame {
    egui::Frame::new().fill(p.surface).inner_margin(Margin::symmetric(x, 0)).stroke(Stroke::new(1.0, p.border))
}

/// Шапка окна: знак, название, главные переключатели. Содержимое — в одну строку по центру.
pub fn top_bar<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    egui::Panel::top("anvil-top-bar")
        .exact_size(TOP_BAR)
        .frame(bar_frame(&p, 16))
        .show(ui, |ui| ui.horizontal_centered(content).inner)
        .inner
}

/// Строка состояния внизу: задачи, ход, подсказки.
pub fn status_bar<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    egui::Panel::bottom("anvil-status-bar")
        .exact_size(STATUS_BAR)
        .frame(bar_frame(&p, 16))
        .show(ui, |ui| ui.horizontal_centered(content).inner)
        .inner
}

/// Боковая панель слева: списки, навигация.
pub fn side_panel<R>(ui: &mut Ui, id: &str, width: f32, content: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    egui::Panel::left(egui::Id::new(id))
        .resizable(false)
        .exact_size(width)
        .frame(egui::Frame::new().fill(p.surface).inner_margin(Margin::symmetric(10, 14)))
        .show(ui, content)
        .inner
}

/// Основная область: фон окна и общие поля, с прокруткой.
pub fn content<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    egui::CentralPanel::no_frame()
        .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin::symmetric(28, 22)))
        .show(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, content).inner)
        .inner
}

/// Цвет, которым eframe чистит окно, — чтобы при изменении размера не мелькало чужое.
pub fn clear_color(visuals: &egui::Visuals, accent: Accent) -> [f32; 4] {
    egui::Rgba::from(Palette::new(visuals.dark_mode, accent).bg).to_array()
}

/// Знак программы: квадрат акцента со значком.
pub fn app_mark(ui: &mut Ui, icon: Icon, size: f32) -> egui::Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(rect, (size * 0.25) as u8, p.accent);
    icons::paint(ui.painter(), rect.shrink(size * 0.21), icon, p.on_accent);
    response
}

/// Знак и название в шапке.
pub fn brand(ui: &mut Ui, icon: Icon, name: &str) {
    let p = Palette::of(ui);
    app_mark(ui, icon, 28.0);
    ui.add_space(2.0);
    ui.label(RichText::new(name).font(semibold(16.0)).color(p.text));
}

// ─── Диалоги ────────────────────────────────────────────────────────────────

/// Диалог посреди окна с затемнением позади: заголовок, крестик, прокрутка.
/// Закрывается крестиком, Esc и щелчком мимо.
pub fn dialog<R>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    width: f32,
    open: &mut bool,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    if !*open {
        return None;
    }
    let p = Palette::of_ctx(ctx);
    let screen = ctx.content_rect();
    let width = width.min(screen.width() - 32.0).max(220.0);
    let max_height = (screen.height() - 64.0).max(160.0);
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius::WINDOW)
        .shadow(ctx.global_style().visuals.window_shadow)
        .inner_margin(Margin { left: 20, right: 12, top: 14, bottom: 18 });
    let mut close = false;
    let response = egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(if p.dark { 150 } else { 90 }))
        .show(ctx, |ui| {
            ui.set_width(width - 32.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(semibold(17.0)).color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = w::icon_button(ui, Icon::Close, tr(ctx, "Закрыть")).clicked();
                });
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(max_height - 80.0)
                // Без этого прокрутка во всплывающем слое сжимается до высоты по умолчанию,
                // а не до края окна, и низ диалога (кнопки!) уезжает под обрез.
                .min_scrolled_height(max_height - 80.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(width - 44.0);
                    content(ui)
                })
                .inner
        });
    if close || response.should_close() {
        *open = false;
    }
    Some(response.inner)
}

/// Что показать в «О программе».
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub name: &'static str,
    pub icon: Icon,
    /// Обычно `env!("CARGO_PKG_VERSION")`.
    pub version: &'static str,
    /// Одна строка о том, что делает программа.
    pub tagline: &'static str,
    /// Обычно `env!("CARGO_PKG_REPOSITORY")`; пусто — ссылки нет.
    pub repository: &'static str,
}

/// Что пользователь сделал в «О программе».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AboutAction {
    CheckUpdates,
}

/// Окно «О программе». `status` — строка о состоянии обновлений от `anvil-update`, если есть.
pub fn about(ctx: &egui::Context, open: &mut bool, info: &AppInfo, status: Option<&str>) -> Option<AboutAction> {
    let mut action = None;
    dialog(ctx, "anvil-about", tr(ctx, "О программе"), 400.0, open, |ui| {
        let p = Palette::of(ui);
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            app_mark(ui, info.icon, 56.0);
            ui.add_space(10.0);
            ui.label(RichText::new(info.name).font(semibold(20.0)).color(p.text));
            ui.label(RichText::new(format!("{} {}", tr(ctx, "Версия"), info.version)).size(13.0).color(p.weak));
            ui.add_space(6.0);
            ui.label(RichText::new(info.tagline).color(p.text));
            ui.add_space(12.0);
            if w::button(ui, Kind::Secondary, Some(Icon::Refresh), tr(ctx, "Проверить обновления")).clicked()
            {
                action = Some(AboutAction::CheckUpdates);
            }
            if let Some(status) = status {
                ui.add_space(4.0);
                w::note(ui, status);
            }
            ui.add_space(10.0);
            if !info.repository.is_empty() {
                ui.hyperlink_to(RichText::new(tr(ctx, "Исходный код")).size(13.0), info.repository);
            }
            ui.label(RichText::new(tr(ctx, "Часть семьи Anvil")).size(12.0).color(p.faint));
        });
    });
    action
}

// ─── Общие настройки ────────────────────────────────────────────────────────

/// Настройки, одинаковые у всех программ семьи. Программа хранит их у себя.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct CommonSettings {
    pub theme: ThemeChoice,
    pub language: Lang,
    pub check_updates: bool,
    pub prerelease: bool,
    /// Версия, которую пользователь пропустил: авто-проверка её больше не предлагает.
    pub skip_version: Option<String>,
}

impl Default for CommonSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            language: lang::system_language(),
            check_updates: true,
            prerelease: false,
            skip_version: None,
        }
    }
}

impl CommonSettings {
    /// Применить к окну: тема и язык.
    pub fn apply(&self, ctx: &egui::Context) {
        theme::choose(ctx, self.theme);
        lang::set_language(ctx, self.language);
    }
}

/// Раздел общих настроек — вставляется в окно настроек программы.
/// Тему и язык применяет сразу; возвращает, изменилось ли что-нибудь (чтобы сохранить).
pub fn common_settings(ui: &mut Ui, settings: &mut CommonSettings) -> bool {
    let ctx = ui.ctx().clone();
    let before = settings.clone();

    w::section_label(ui, tr(&ctx, "Оформление"));
    ui.add_space(2.0);
    setting_row(ui, tr(&ctx, "Тема"), |ui| {
        w::segmented(
            ui,
            &mut settings.theme,
            &[
                (ThemeChoice::System, Some(Icon::Monitor), tr(&ctx, "Система")),
                (ThemeChoice::Light, Some(Icon::Sun), tr(&ctx, "Светлая")),
                (ThemeChoice::Dark, Some(Icon::Moon), tr(&ctx, "Тёмная")),
            ],
        );
    });
    setting_row(ui, tr(&ctx, "Язык"), |ui| {
        let options: Vec<(Lang, Option<Icon>, &str)> = Lang::ALL.iter().map(|l| (*l, None, l.native_name())).collect();
        w::segmented(ui, &mut settings.language, &options);
    });

    ui.add_space(10.0);
    w::section_label(ui, tr(&ctx, "Обновления"));
    ui.add_space(2.0);
    w::switch(ui, &mut settings.check_updates, tr(&ctx, "Проверять при запуске"));
    ui.add_enabled_ui(settings.check_updates, |ui| {
        w::switch(ui, &mut settings.prerelease, tr(&ctx, "Предлагать пред-выпуски"));
    });
    w::note(ui, tr(&ctx, "Программа сама узнаёт о новых версиях на GitHub. Больше ничего наружу не отправляет."));

    let changed = *settings != before;
    if changed {
        settings.apply(&ctx);
    }
    changed
}

/// Строка настройки: подпись слева, управление справа.
pub fn setting_row(ui: &mut Ui, label: &str, control: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(36.0);
        ui.label(RichText::new(label).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}
