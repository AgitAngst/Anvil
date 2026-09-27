//! Отрисовка окна. Каждая часть — в своём файле.

mod amber;
mod deck;
pub mod deps;
mod dialogs;
mod github;
pub mod install;
mod jobs;
mod overview;
pub mod page;
pub mod palette;
mod presets;
mod project;
pub mod release;
mod settings;
mod sidebar;

use anvil_ui::chrome::{self, AboutAction, AppInfo};
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone};
use eframe::egui::{self, Ui};

use crate::app::{App, Mode, View};
use crate::i18n::{self, t};
use crate::worker::Busy;

pub fn info() -> AppInfo {
    AppInfo {
        name: "Anvil",
        icon: Icon::Hammer,
        version: env!("CARGO_PKG_VERSION"),
        tagline: t("Пульт и кузница моих программ"),
        repository: env!("CARGO_PKG_REPOSITORY"),
    }
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    // Быстрый запуск: окно на время стало окном без рамки — рисуется только он.
    if app.quick.is_some() {
        palette::show_quick(app, ui);
        return;
    }
    // Спрятано в трей — рисовать нечего: кадры идут ради фоновой работы (`background`).
    if app.hidden {
        return;
    }
    let ctx = ui.ctx().clone();
    shortcuts(app, &ctx);
    // Пульт считается раз за кадр: он нужен и строкам, и чипам в строке состояния.
    let deck = deck::Frame::new(app);
    top_bar(app, ui);
    status_bar(app, ui, &deck.chips());
    jobs::panel(app, ui);
    if app.mode == Mode::Forge {
        chrome::side_panel(ui, "projects", 260.0, |ui| sidebar::show(app, ui));
    }
    chrome::content(ui, |ui| {
        let updater = app.updater.clone();
        if anvil_update::ui::banner(ui, &updater, &mut app.config.common) {
            app.save();
        }
        match (app.mode, app.view) {
            (Mode::Deck, _) if app.deck_view.page.is_some() => page::show(app, ui, &deck),
            (Mode::Deck, _) => deck::show(app, ui, &deck),
            (Mode::Forge, View::Project) => project::show(app, ui),
            (Mode::Forge, View::Overview) => overview::show(app, ui),
        }
    });

    settings::show(app, &ctx);
    presets::show(app, &ctx);
    release::show(app, &ctx);
    dialogs::show(app, &ctx);
    deps::overview(app, &ctx);
    deps::confirm(app, &ctx);
    let info = info();
    let status = anvil_update::ui::about_status(&ctx, &app.updater);
    if chrome::about(&ctx, &mut app.about_open, &info, status.as_deref()) == Some(AboutAction::CheckUpdates) {
        app.updater.check(app.config.common.prerelease, None, true);
    }
    palette::show(app, &ctx);
    app.toasts.show(&ctx);
}

/// Фоновая работа окна — и когда оно спрятано в трей: просьбы трея, сочетание, щелчки по
/// уведомлениям Windows, меню трея и точка на значке, ошибки — в уведомления Windows.
pub fn background(app: &mut App, ctx: &egui::Context) {
    use crate::tray::Request;
    use std::time::Instant;
    let double = crate::tray::double_click();
    let requests: Vec<Request> = app.tray_rx.try_iter().collect();
    for request in requests {
        match request {
            // Одиночный щелчок по значку ждёт, не двойной ли он: двойной — окно, а не быстрый
            // запуск. Хвост двойного щелчка и щелчок, которым быстрый запуск закрыли (окно потеряло
            // фокус), его не открывают.
            Request::Quick => {
                let after_show = app.tray_shown_at.is_some_and(|at| at.elapsed() < double);
                if !after_show && !app.quick_just_closed(double) {
                    app.quick_pending = Some(Instant::now());
                }
            }
            Request::Show => {
                app.quick_pending = None;
                app.tray_shown_at = Some(Instant::now());
                tray_request(app, Request::Show);
            }
            other => tray_request(app, other),
        }
    }
    if let Some(at) = app.quick_pending {
        if at.elapsed() >= double {
            app.quick_pending = None;
            app.quick_launch();
        } else {
            ctx.request_repaint_after(double.saturating_sub(at.elapsed()));
        }
    }
    let presses = app.hotkey.pressed.try_iter().count();
    for _ in 0..presses {
        app.quick_launch();
    }
    let clicks: Vec<String> = app.clicks.try_iter().collect();
    for click in clicks {
        toast_click(app, &click);
    }
    // Сочетание занято другой программой — сказать один раз: быстрый запуск остаётся в трее.
    let state = app.hotkey.state();
    if state != app.hotkey_seen {
        if state == crate::hotkey::State::Busy {
            let keys = app.hotkey.combo().map(|c| c.keys().join("+")).unwrap_or_default();
            let text =
                format!("{keys}: {}", t("сочетание занято другой программой — выберите другое: Настройки → Запуск"));
            app.toasts.push(text, Tone::Warning);
        }
        app.hotkey_seen = state;
    }
    needs_window(app, ctx);
    forward_toasts(app, ctx);
    tray_menu(app);
}

/// Вопрос, которого в спрятанном окне не видно (остановить принудительно, занятый exe, поставить,
/// откатить), — показать окно: иначе он ждал бы, пока окно откроют случайно.
fn needs_window(app: &mut App, ctx: &egui::Context) {
    let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
    let asking = app.quick.is_some() && app.palette.as_ref().is_some_and(palette::State::asking);
    let pending = app.force_confirm.is_some()
        || app.stop_confirm.is_some()
        || app.install_confirm.is_some()
        || app.rollback_confirm.is_some()
        || app.uninstall_confirm.is_some()
        || (app.locked.is_some() && !asking);
    if pending && (app.hidden || minimized || (app.quick.is_some() && !asking)) {
        app.show_window();
    }
}

fn find_item(app: &mut App, key: &str) -> Option<crate::deck::Item> {
    app.deck_items().into_iter().find(|i| i.key == key)
}

fn show_page(app: &mut App, key: String) {
    app.show_window();
    app.set_mode(Mode::Deck);
    app.open_page(key);
}

fn tray_request(app: &mut App, request: crate::tray::Request) {
    use crate::tray::Request;
    match request {
        Request::Show => app.show_window(),
        Request::Quick => app.quick_launch(),
        Request::Quit => app.quit(),
        Request::Main(key) => {
            if let Some(item) = find_item(app, &key) {
                // Работающая служба — её журнал на странице: окно надо показать.
                if deck::opens_page(app, &item, None) {
                    show_page(app, key);
                } else {
                    deck::run_main(app, &item);
                }
            }
        }
        Request::Page(key) => show_page(app, key),
        Request::Stop(key) => {
            let run = app.runs.iter().rev().find(|r| r.key == key && r.running());
            match run.map(|r| (r.name.clone(), r.pid, r.from_code)) {
                // Своя сборка из кода — сразу; остальное — с вопросом в окне (§5.12).
                Some((name, pid, true)) => app.stop_run(name, pid),
                Some((name, pid, false)) => {
                    let dir = find_item(app, &key).map(|i| i.project).unwrap_or_default();
                    app.show_window();
                    app.stop_confirm = Some((name, pid, dir));
                }
                // Запущено не из Anvil: остановка — с вопросом, по PID из снимка процессов.
                None => {
                    let item = find_item(app, &key);
                    let running = item.as_ref().and_then(|i| {
                        let godot = i.kind == crate::registry::Kind::Godot && app.godot_editor().is_some();
                        deck::look(app, i, godot).running.map(|(pid, _)| (i.name.clone(), pid, i.project.clone()))
                    });
                    match running {
                        Some(stop) => {
                            app.show_window();
                            app.stop_confirm = Some(stop);
                        }
                        None => show_page(app, key),
                    }
                }
            }
        }
    }
}

/// Щелчок по уведомлению Windows: `open:` — страница на «Запусках», `log:` — журнал, `again:` —
/// запустить тем же профилем, `show` — окно.
fn toast_click(app: &mut App, click: &str) {
    let (kind, key) = click.split_once(':').unwrap_or((click, ""));
    let key = key.to_owned();
    match kind {
        "open" => {
            let engine = find_item(app, &key)
                .is_some_and(|i| matches!(i.kind, crate::registry::Kind::Godot | crate::registry::Kind::Unity));
            show_page(app, key);
            // «Запуски» — у программы первая вкладка, у Godot и Unity — вторая.
            app.deck_view.page_tab = if engine { 1 } else { 0 };
        }
        "log" => {
            let last = app.runs.iter().rev().find(|r| r.key == key).map(|r| (r.service, r.log.clone()));
            match last {
                // У программы — файл вывода; у службы журнал на её странице.
                Some((false, Some(log))) if log.is_file() => {
                    let result = crate::open::file(&log);
                    app.report(result);
                }
                _ => show_page(app, key),
            }
        }
        "again" => {
            let profile = app.runs.iter().rev().find(|r| r.key == key).map(|r| r.profile.clone());
            if let (Some(item), Some(profile)) = (find_item(app, &key), profile) {
                deck::launch_profile(app, &item, &profile);
            }
        }
        _ => app.show_window(),
    }
}

/// Окна не видно (трей, свёрнуто, быстрый запуск) — ошибки из уведомлений в окне уходят в Windows,
/// на значке — точка до открытия окна.
fn forward_toasts(app: &mut App, ctx: &egui::Context) {
    let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
    if app.hidden || minimized || app.quick.is_some() {
        // Только ошибки: предупреждения (origin не ответил, сочетание занято) ждут в окне.
        for (text, tone) in app.toasts.since(app.toasts_seen) {
            if tone == Tone::Danger {
                app.notifier.alert("Anvil".to_owned(), text);
                app.alert = true;
            }
        }
    }
    app.toasts_seen = std::time::Instant::now();
}

/// Меню трея (раз в секунду, пересобирается, только если изменилось) и точка на значке.
fn tray_menu(app: &mut App) {
    use crate::tray::{Entry, Request};
    if app.tray.is_none() || app.tray_built.elapsed() < std::time::Duration::from_secs(1) {
        return;
    }
    app.tray_built = std::time::Instant::now();
    let frame = deck::Frame::new(app);
    let lower = |text: &str| {
        let mut chars = text.chars();
        chars.next().map(|c| c.to_lowercase().chain(chars).collect::<String>()).unwrap_or_default()
    };
    let mut menu = vec![Entry::Header("Anvil".to_owned()), Entry::Separator];
    let items: Vec<_> = frame.items.iter().zip(&frame.looks).filter(|(i, l)| !i.is_self() && !l.this).collect();
    let running: Vec<_> = items.iter().filter(|(_, l)| l.running.is_some()).collect();
    if !running.is_empty() {
        menu.push(Entry::Header(t("Запущено").to_owned()));
        for (item, look) in running {
            if item.group == crate::deck::Group::Services {
                let label = match look.port {
                    Some(port) => format!("{} · :{port}", item.name),
                    None => item.name.clone(),
                };
                let stop = if look.own { t("Остановить") } else { t("Остановить…") };
                let sub = vec![
                    (t("Журнал").to_owned(), Request::Page(item.key.clone())),
                    (stop.to_owned(), Request::Stop(item.key.clone())),
                ];
                menu.push(Entry::Sub(label, sub));
            } else {
                menu.push(Entry::Item(
                    format!("{} — {}", item.name, t("к окну")),
                    None,
                    Request::Main(item.key.clone()),
                ));
            }
        }
        menu.push(Entry::Separator);
    }
    let pinned: Vec<_> = items.iter().filter(|(i, _)| app.config.deck.pinned.contains(&i.key)).collect();
    if !pinned.is_empty() {
        menu.push(Entry::Header(t("Закреплено").to_owned()));
        for (n, (item, look)) in pinned.into_iter().enumerate() {
            let keys = (n < 9).then(|| format!("Alt+{}", n + 1));
            // «Unity … не найден» — не действие: только имя.
            let label = if look.main == deck::Main::Nothing {
                item.name.clone()
            } else {
                format!("{} — {}", item.name, lower(&look.hint))
            };
            menu.push(Entry::Item(label, keys, Request::Main(item.key.clone())));
        }
        menu.push(Entry::Separator);
    }
    // Сочетание занято другой программой — видно прямо в меню: быстрый запуск остаётся щелчком.
    let hotkey = app.hotkey.combo().map(|c| c.keys().join("+")).map(|keys| match app.hotkey.state() {
        crate::hotkey::State::Busy => format!("{keys} — {}", t("занято")),
        _ => keys,
    });
    menu.push(Entry::Item(t("Быстрый запуск").to_owned(), hotkey, Request::Quick));
    menu.push(Entry::Item(t("Открыть Anvil").to_owned(), None, Request::Show));
    menu.push(Entry::Separator);
    menu.push(Entry::Item(t("Выход — запущенное продолжит работать").to_owned(), None, Request::Quit));
    let alert = app.alert || app.runs.iter().any(|r| r.end == crate::runs::End::Crashed && !r.seen);
    if let Some(tray) = &mut app.tray {
        tray.set_alert(alert);
        tray.set_menu(menu);
    }
}

fn shortcuts(app: &mut App, ctx: &egui::Context) {
    use egui::{Key, KeyboardShortcut, Modifiers};
    // Настройки ждут новое сочетание — нажатое принадлежит им.
    if app.hotkey_capture {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
        app.refresh();
        app.fetch();
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma))) {
        app.settings_open = true;
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::K))) {
        if app.palette.is_some() {
            app.palette = None;
        } else {
            palette::open(app);
        }
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0))) {
        app.toggle_view();
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1))) {
        app.set_mode(Mode::Deck);
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num2))) {
        app.set_mode(Mode::Forge);
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::L))) {
        app.log_open = !app.log_open;
    }
}

fn top_bar(app: &mut App, ui: &mut Ui) {
    chrome::top_bar(ui, |ui| {
        let info = info();
        chrome::brand(ui, info.icon, info.name);
        ui.add_space(18.0);
        let mut mode = app.mode;
        w::segmented(
            ui,
            &mut mode,
            &[(Mode::Deck, Some(Icon::Play), t("Пульт")), (Mode::Forge, Some(Icon::Hammer), t("Кузница"))],
        )
        .on_hover_text("Ctrl+1 · Ctrl+2");
        if mode != app.mode {
            app.set_mode(mode);
        }
        ui.add_space(10.0);
        palette::launcher(app, ui, 340.0);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let gear = w::icon_button(ui, Icon::Gear, t("Меню"));
            w::menu(&gear, 240.0, |ui| {
                if w::menu_item(ui, Some(Icon::Gear), t("Настройки"), Some("Ctrl+,")).clicked() {
                    app.settings_open = true;
                }
                if w::menu_item(ui, Some(Icon::Info), t("О программе"), None).clicked() {
                    app.about_open = true;
                }
                w::menu_separator(ui);
                if w::menu_item(ui, Some(Icon::File), t("Открыть anvil.toml"), None).clicked() {
                    let dir = app.config_path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                    if !app.config_path.exists() {
                        app.save();
                    }
                    app.report(crate::open::code(&app.config_path, &dir));
                }
            });
            let busy = app.busy.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                if w::icon_button(ui, Icon::Refresh, t("Перечитать всё · F5")).clicked() {
                    app.refresh();
                    app.fetch();
                }
            });
        });
    });
}

fn status_bar(app: &mut App, ui: &mut Ui, chips: &[(anvil_ui::Mark, String, String, String)]) {
    let p = Palette::of(ui);
    chrome::status_bar(ui, |ui| {
        if jobs::status(app, ui) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| right_side(app, ui, chips));
            return;
        }
        match app.busy {
            Some(busy) => {
                w::spinner(ui, 14.0);
                let text = match busy {
                    Busy::Scanning => t("Ищу проекты…"),
                    Busy::Refreshing => t("Читаю состояние…"),
                    Busy::Fetching => t("Спрашиваю origin…"),
                };
                ui.label(egui::RichText::new(text).size(13.0).color(p.text));
            }
            None => {
                w::dot(ui, Tone::Success);
                let when = app.refreshed_at.map(i18n::ago).unwrap_or_else(|| "—".into());
                w::note(ui, format!("{} {when}", t("Состояние прочитано")));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| right_side(app, ui, chips));
    });
}

/// Правая часть строки состояния: что запущено и кнопка консоли.
fn right_side(app: &mut App, ui: &mut Ui, chips: &[(anvil_ui::Mark, String, String, String)]) {
    let p = Palette::of(ui);
    console_toggle(app, ui);
    if chips.is_empty() {
        return;
    }
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), egui::Stroke::new(1.0, p.border));
    ui.add_space(4.0);
    ui.spacing_mut().item_spacing.x = 6.0;
    for (mark, name, tail, key) in chips.iter().rev() {
        // Чип — страница того, что запущено: там журнал, остановка, профили.
        if w::running_chip(ui, mark.accent, mark.icon, name, tail).clicked() {
            app.set_mode(Mode::Deck);
            app.open_page(key.clone());
        }
    }
    ui.add_space(2.0);
    w::section_label(ui, t("Запущено"));
}

/// Консоль — задачи и их вывод. На Пульте закрыта, пока не понадобится.
fn console_toggle(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    let hint = if app.log_open {
        t("Скрыть консоль · Ctrl+L")
    } else {
        t("Показать консоль · Ctrl+L")
    };
    let r = w::button(ui, Kind::Ghost, Some(Icon::Terminal), t("Консоль"));
    if app.log_open {
        // Открытая консоль — кнопка нажата.
        ui.painter().rect_filled(r.rect, anvil_ui::theme::radius::CONTROL, p.raised);
        let icon = egui::Rect::from_min_size(
            egui::pos2(r.rect.left() + 12.0, r.rect.center().y - 8.0),
            egui::Vec2::splat(16.0),
        );
        anvil_ui::icons::paint(ui.painter(), icon, Icon::Terminal, p.text);
        ui.painter().text(
            egui::pos2(icon.right() + 7.0, r.rect.center().y),
            egui::Align2::LEFT_CENTER,
            t("Консоль"),
            egui::FontId::proportional(14.0),
            p.text,
        );
    }
    // Для диктора — переключатель: слышно, открыта ли консоль.
    let open = app.log_open;
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, open, t("Консоль")));
    if r.on_hover_text(hint).clicked() {
        app.log_open = !app.log_open;
    }
}
