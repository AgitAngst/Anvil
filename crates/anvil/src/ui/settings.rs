//! Окно настроек (§7.7): вкладки «Общее», «Пульт», «Запуск», «Движки», «Кузница». Всё применяется
//! сразу, кнопки «Сохранить» нет.

use anvil_ui::chrome;
use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette};
use eframe::egui::{self, Ui};

use crate::app::App;
use crate::i18n::{self, t};
use crate::worker;

/// Выбрать папку и добавить её к корням поиска.
pub fn add_root(app: &mut App) {
    let Some(dir) = rfd::FileDialog::new().set_title(t("Папка с проектами")).pick_folder() else {
        return;
    };
    if !app.config.roots.iter().any(|r| crate::registry::same_dir(r, &dir)) {
        app.config.roots.push(dir);
        app.save();
        app.rescan();
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let mut open = app.settings_open;
    chrome::dialog(ctx, "anvil-settings", t("Настройки"), 680.0, &mut open, |ui| {
        let mut tab = app.settings_tab;
        w::tabs(ui, &mut tab, &[t("Общее"), t("Пульт"), t("Запуск"), t("Движки"), t("Кузница")]);
        if tab != app.settings_tab {
            app.cancel_hotkey_capture();
            // Псевдоним пишется, когда поле отпустили; ушли с вкладки, не отпустив, — записать тут.
            app.flush();
        }
        app.settings_tab = tab;
        ui.add_space(14.0);
        match tab {
            0 => general(app, ui),
            1 => deck(app, ui),
            2 => launch(app, ui),
            3 => engines(app, ui),
            _ => forge(app, ui),
        }
    });
    if app.settings_open && !open {
        app.cancel_hotkey_capture();
        // То же при закрытии: псевдоним, набранный без выхода из поля, не теряется.
        app.flush();
    }
    app.settings_open = open;
}

/// Общее: оформление, язык, обновления — как у всей семьи.
fn general(app: &mut App, ui: &mut Ui) {
    if chrome::common_settings(ui, &mut app.config.common) {
        i18n::set(ui.ctx(), app.config.common.language);
        app.save();
    }
}

/// Пульт: закреплённое (порядок и псевдонимы) и убранное.
fn deck(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Закреплённое"));
    ui.add_space(2.0);
    if app.config.deck.pinned.is_empty() {
        w::note(ui, t("Ничего не закреплено — закрепить можно в меню строки Пульта (Ctrl+P)."));
    } else {
        w::note(
            ui,
            t(
                "Alt+1…9 в быстром запуске и на Пульте — по этому порядку. Псевдоним: набрать его точно — предмет первым.",
            ),
        );
        ui.add_space(4.0);
        let items = app.deck_items();
        // Номера — как у Пульта, палитры и трея: в порядке Пульта, без самого Anvil. Закреплённое, чего на
        // Пульте нет (убрано, скрыто, проект не найден), — в конце, без номера.
        let shown = crate::deck::numbered(&items, &app.config.deck);
        let mut rows: Vec<Pinned> =
            shown.iter().map(|i| Pinned { key: i.key.clone(), name: i.name.clone(), group: Some(i.group) }).collect();
        for key in &app.config.deck.pinned {
            if !rows.iter().any(|r| &r.key == key) {
                rows.push(Pinned { key: key.clone(), name: key.clone(), group: None });
            }
        }
        let mut swap: Option<(String, String)> = None;
        let mut unpin: Option<String> = None;
        let mut alias_done = false;
        for (n, row) in rows.iter().enumerate() {
            // Выше / Ниже — только с соседом из той же группы: группы на Пульте идут в своём порядке.
            let neighbour = |m: Option<usize>| m.and_then(|m| rows.get(m));
            let same = |other: Option<&Pinned>| other.is_some_and(|o| row.group.is_some() && o.group == row.group);
            let above = neighbour(n.checked_sub(1));
            let below = neighbour(Some(n + 1));
            if let Some(action) = pinned_row(app, ui, n, row, same(above), same(below), &mut alias_done) {
                match action {
                    RowAction::Up => swap = above.map(|o| (row.key.clone(), o.key.clone())),
                    RowAction::Down => swap = below.map(|o| (row.key.clone(), o.key.clone())),
                    RowAction::Unpin => unpin = Some(row.key.clone()),
                }
            }
            ui.add_space(4.0);
        }
        let pinned = &mut app.config.deck.pinned;
        if let Some((a, b)) = swap
            && let (Some(a), Some(b)) = (pinned.iter().position(|k| *k == a), pinned.iter().position(|k| *k == b))
        {
            pinned.swap(a, b);
            app.deck_view.order.clear();
            app.save();
        }
        if let Some(key) = unpin {
            app.config.deck.pinned.retain(|k| *k != key);
            app.config.deck.aliases.remove(&key);
            app.deck_view.order.clear();
            app.save();
        }
        if alias_done {
            app.flush();
        }
    }
    ui.add_space(12.0);
    removed(app, ui);
}

/// Строка закреплённого в настройках.
struct Pinned {
    key: String,
    name: String,
    /// Группа на Пульте; `None` — на Пульте его нет (номера и порядка тоже).
    group: Option<crate::deck::Group>,
}

enum RowAction {
    Up,
    Down,
    Unpin,
}

/// Alt+N, имя, псевдоним, «Выше» / «Ниже», «Открепить».
fn pinned_row(
    app: &mut App,
    ui: &mut Ui,
    n: usize,
    row: &Pinned,
    up: bool,
    down: bool,
    alias_done: &mut bool,
) -> Option<RowAction> {
    let p = Palette::of(ui);
    let mut action = None;
    let on_deck = row.group.is_some();
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(anvil_ui::theme::radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(10, 2))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                let keys = if on_deck && n < 9 { format!("Alt+{}", n + 1) } else { String::new() };
                ui.add_sized(
                    egui::vec2(48.0, 20.0),
                    egui::Label::new(egui::RichText::new(keys).font(egui::FontId::monospace(12.0)).color(p.weak)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if w::icon_button(ui, Icon::Close, t("Открепить")).clicked() {
                        action = Some(RowAction::Unpin);
                    }
                    if !on_deck {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(t("не на Пульте")).size(13.0).color(p.weak));
                    } else {
                        let other_group = t("Группы на Пульте идут в своём порядке");
                        let r = ui.add_enabled_ui(down, |ui| w::icon_button(ui, Icon::ArrowDown, t("Ниже"))).inner;
                        if r.clicked() {
                            action = Some(RowAction::Down);
                        }
                        if !down {
                            r.on_disabled_hover_text(other_group);
                        }
                        let r = ui.add_enabled_ui(up, |ui| w::icon_button(ui, Icon::ChevronUp, t("Выше"))).inner;
                        if r.clicked() {
                            action = Some(RowAction::Up);
                        }
                        if !up {
                            r.on_disabled_hover_text(other_group);
                        }
                        ui.add_space(6.0);
                        let mut alias = app.config.deck.aliases.get(&row.key).cloned().unwrap_or_default();
                        let edit = egui::TextEdit::singleline(&mut alias)
                            .hint_text(t("псевдоним"))
                            .desired_width(120.0)
                            .font(egui::FontId::monospace(12.5));
                        let r = ui.add(edit);
                        // На диск — когда поле отпустили, а не на каждую букву (`App::flush` — при уходе).
                        if r.changed() {
                            if alias.trim().is_empty() {
                                app.config.deck.aliases.remove(&row.key);
                            } else {
                                app.config.deck.aliases.insert(row.key.clone(), alias.clone());
                            }
                            app.config_dirty = true;
                        }
                        if r.lost_focus() {
                            *alias_done = true;
                        }
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let text = egui::RichText::new(&row.name).color(if on_deck { p.text } else { p.weak });
                        let text = if on_deck { text } else { text.font(egui::FontId::monospace(12.5)) };
                        ui.add(egui::Label::new(text).truncate());
                    });
                });
            });
        });
    action
}

/// Запуск: быстрый запуск, окно и трей, запуски и журналы (§7.7).
fn launch(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    w::section_label(ui, t("Быстрый запуск"));
    ui.add_space(2.0);
    hotkey(app, ui);
    let mut close_after = app.config.quick.close_after;
    w::switch(ui, &mut close_after, t("Закрывать быстрый запуск после запуска"));
    let mut layout = app.config.quick.layout;
    w::switch(ui, &mut layout, t("Искать и в русской раскладке («фь еу» = «am te»)"));
    if (close_after, layout) != (app.config.quick.close_after, app.config.quick.layout) {
        app.config.quick.close_after = close_after;
        app.config.quick.layout = layout;
        app.save();
    }

    ui.add_space(10.0);
    w::section_label(ui, t("Окно и трей"));
    ui.add_space(2.0);
    chrome::setting_row(ui, t("Открывать на"), |ui| {
        use crate::config::OpenOn;
        let before = app.config.window.open_on;
        let mut open_on = before;
        w::segmented(
            ui,
            &mut open_on,
            &[
                (OpenOn::Deck, None, t("Пульте")),
                (OpenOn::Forge, None, t("Кузнице")),
                (OpenOn::Last, None, t("Где был")),
            ],
        );
        if open_on != before {
            app.config.window.open_on = open_on;
            app.config.window.last = if app.mode == crate::app::Mode::Forge { OpenOn::Forge } else { OpenOn::Deck };
            app.save();
        }
    });
    let has_tray = app.tray.is_some();
    let mut tray = app.config.window.close_to_tray;
    let r = ui.add_enabled_ui(has_tray, |ui| w::switch(ui, &mut tray, t("Крестик прячет Anvil в трей"))).response;
    r.on_disabled_hover_text(t("Трея нет: значок не создался"));
    if tray != app.config.window.close_to_tray {
        app.config.window.close_to_tray = tray;
        app.save();
    }
    let before = crate::autostart::enabled();
    let mut autostart = before;
    let r = ui
        .add_enabled_ui(cfg!(windows), |ui| {
            w::switch(ui, &mut autostart, t("Запускать Anvil при входе в Windows, сразу в трей"))
        })
        .response;
    r.on_hover_text(t("Запись Anvil в «Автозагрузке» Windows (реестр, раздел Run) с этим exe."));
    if autostart != before {
        let result = crate::autostart::set(autostart);
        app.report(result);
    }
    w::note(ui, t("Выход из Anvil ничего не останавливает: запущенное продолжит работать."));

    ui.add_space(10.0);
    w::section_label(ui, t("Запуски"));
    ui.add_space(2.0);
    let mut crash = app.config.notify_crash;
    w::switch(ui, &mut crash, t("Уведомлять, если программа упала"));
    if crash != app.config.notify_crash {
        app.config.notify_crash = crash;
        app.notifier.set_crash(crash);
        app.save();
    }
    let mut notify = app.config.notify;
    w::switch(ui, &mut notify, t("Уведомлять о конце долгих задач"));
    if notify != app.config.notify {
        app.config.notify = notify;
        app.notifier.set_enabled(notify);
        app.save();
    }
    chrome::setting_row(ui, t("Писать вывод"), |ui| {
        use crate::config::Output;
        let before = app.config.runs.output;
        let mut output = before;
        w::segmented(
            ui,
            &mut output,
            &[
                (Output::Services, None, t("Службы и сборки")),
                (Output::All, None, t("Всё")),
                (Output::Nothing, None, t("Ничего")),
            ],
        );
        if output != before {
            app.config.runs.output = output;
            app.save();
        }
    });
    let note = match app.config.runs.output {
        crate::config::Output::Services => {
            t("Вывод установленных программ не пишется — бережём диск. На запуск — до двух файлов по 5 МБ.")
        }
        crate::config::Output::All => {
            t("Пишется вывод всех запусков — и установленных программ, и игр. На запуск — до двух файлов по 5 МБ.")
        }
        crate::config::Output::Nothing => t("Вывод запусков в файлы не пишется."),
    };
    w::note(ui, note);
    let dir = crate::runs::default_dir(&app.config_path);
    chrome::setting_row(ui, t("Журналы и данные"), |ui| {
        if w::icon_button(ui, Icon::Folder, t("Открыть папку")).clicked() {
            let _ = std::fs::create_dir_all(&dir);
            let result = crate::open::folder(&dir);
            app.report(result);
        }
        let text = dir.display().to_string();
        ui.add(
            egui::Label::new(egui::RichText::new(&text).font(egui::FontId::monospace(12.5)).color(p.weak)).truncate(),
        )
        .on_hover_text(&text);
    });
    chrome::setting_row(ui, t("Принудительно останавливать через"), |ui| {
        let before = app.config.runs.force_after;
        let mut secs = before;
        w::segmented(ui, &mut secs, &[(5, None, t("5 с")), (10, None, t("10 с")), (30, None, t("30 с"))]);
        if secs != before {
            app.config.runs.force_after = secs;
            crate::runs::set_force_after(secs);
            app.save();
        }
    });
    w::note(ui, t("Столько ждать, пока программа закроется сама; потом — вопрос, остановить ли принудительно."));
}

/// Поле сочетания быстрого запуска: клавиши, «Изменить…» — следующее нажатое станет сочетанием.
fn hotkey(app: &mut App, ui: &mut Ui) {
    use crate::hotkey::State;
    // Ловим, только когда настройки сверху: над ними палитра (щелчок по значку трея) или вопрос —
    // нажатия их, а не сочетания.
    let on_top = app.palette.is_none() && ui.ctx().memory(|m| m.top_modal_layer()) == Some(ui.layer_id());
    if app.hotkey_capture && on_top {
        use egui::{Event, Key};
        // Ждём сочетание: см. `captured`.
        let pressed = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Key { key, physical_key, pressed: true, repeat: false, modifiers } => {
                    Some((*key, *physical_key, *modifiers))
                }
                // Ctrl+…+C, X, V egui отдаёт как «копировать», «вырезать», «вставить».
                Event::Copy => Some((Key::C, None, i.modifiers)),
                Event::Cut => Some((Key::X, None, i.modifiers)),
                Event::Paste(_) => Some((Key::V, None, i.modifiers)),
                // «Вставить» при пустом буфере не приходит вовсе — остаётся отпускание V.
                Event::Key { key: key @ (Key::C | Key::X | Key::V), pressed: false, modifiers, .. }
                    if modifiers.command =>
                {
                    Some((*key, None, *modifiers))
                }
                _ => None,
            })
        });
        if let Some((key, physical, mods)) = pressed {
            if key == Key::Escape && !mods.any() {
                app.cancel_hotkey_capture();
            } else if let Some(combo) = captured(key, physical, mods) {
                let text = combo.keys().join("+");
                app.hotkey.set(Some(combo));
                app.config.quick.hotkey = text;
                app.hotkey_capture = false;
                app.hotkey_seen = State::Pending;
                app.save();
            }
            // Нажатое — сочетанию, а не полям и кнопкам диалога.
            ui.ctx().input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(e, Event::Key { .. } | Event::Text(_) | Event::Copy | Event::Cut | Event::Paste(_))
                });
            });
        }
    }
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(36.0);
        ui.label(egui::RichText::new(t("Сочетание клавиш")).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.hotkey_capture {
                if w::button(ui, Kind::Ghost, None, t("Отмена")).clicked() {
                    app.cancel_hotkey_capture();
                }
                ui.label(egui::RichText::new(t("Нажмите сочетание… · Esc — отмена")).size(13.0).color(p.accent_text));
                ui.ctx().request_repaint();
            } else {
                let keys: Vec<String> = app.hotkey.combo().map(|c| c.keys()).unwrap_or_default();
                let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space((ui.available_width() - 260.0).max(0.0));
                    if w::hotkey_field(ui, &keys, t("Изменить…"), None).clicked() {
                        // Снять нынешнее: иначе его нажатие перехватит Windows, а не диалог.
                        app.hotkey.set(None);
                        app.hotkey_capture = true;
                    }
                });
            }
        });
    });
    if !app.hotkey_capture {
        let (free, text) = match app.hotkey.state() {
            State::Ready => (true, t("Свободно — работает из любой программы")),
            State::Busy => (false, t("Занято другой программой — выберите другое")),
            State::Pending => (true, t("Проверяю…")),
            State::Invalid => (false, t("Сочетание не задано")),
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            w::dot(ui, if free { anvil_ui::Tone::Success } else { anvil_ui::Tone::Danger });
            let color = if free { p.weak } else { p.text };
            ui.label(egui::RichText::new(text).size(13.0).color(color));
        });
        if app.hotkey.state() == State::Pending {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
    ui.add_space(4.0);
}

/// Нажатая клавиша с модификаторами → сочетание. Клавиша — какую видит раскладка, а не вышло — та, что
/// на этом месте (Ctrl+Shift+7 даёт «?», а нужна 7).
fn captured(key: egui::Key, physical: Option<egui::Key>, mods: egui::Modifiers) -> Option<crate::hotkey::Combo> {
    combo_of(key, mods).or_else(|| physical.and_then(|k| combo_of(k, mods)))
}

/// Нужен Ctrl или Alt и ещё один модификатор; с одним Ctrl или Alt — только пробел и F1…F24: Ctrl+C
/// или Alt+F отняли бы у всех программ копирование и меню. Клавиша — из тех, что понимает
/// [`crate::hotkey::Combo`].
fn combo_of(key: egui::Key, mods: egui::Modifiers) -> Option<crate::hotkey::Combo> {
    if !(mods.ctrl || mods.alt) {
        return None;
    }
    let count = [mods.ctrl, mods.alt, mods.shift].into_iter().filter(|&on| on).count();
    let free = key == egui::Key::Space || key.name().strip_prefix('F').is_some_and(|n| n.parse::<u8>().is_ok());
    if count < 2 && !free {
        return None;
    }
    let mut text = String::new();
    for (on, name) in [(mods.ctrl, "Ctrl"), (mods.alt, "Alt"), (mods.shift, "Shift")] {
        if on {
            text.push_str(name);
            text.push('+');
        }
    }
    text.push_str(key.name());
    crate::hotkey::Combo::parse(&text)
}

/// Движки: редактор Godot, редакторы Unity из Hub.
fn engines(app: &mut App, ui: &mut Ui) {
    let p = Palette::of(ui);
    w::section_label(ui, "Godot");
    ui.add_space(2.0);
    let editor = app.godot_editor();
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        ui.spacing_mut().item_spacing.x = 6.0;
        match &editor {
            Some(path) => {
                let version = crate::engines::godot_editor_version(path).unwrap_or_default();
                ui.label(egui::RichText::new(format!("Godot {version}").trim()).color(p.text));
                w::dot(ui, anvil_ui::Tone::Success);
                ui.label(egui::RichText::new(t("найден")).size(13.0).color(p.weak));
            }
            None => {
                ui.label(egui::RichText::new("Godot").color(p.text));
                w::dot(ui, anvil_ui::Tone::Warning);
                ui.label(egui::RichText::new(t("не найден")).size(13.0).color(p.text));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.config.godot.is_some() && w::button(ui, Kind::Ghost, None, t("Искать в PATH")).clicked() {
                app.reset_godot();
            }
            if w::button(ui, Kind::Secondary, Some(Icon::Folder), t("Выбрать…")).clicked()
                && let Some(path) = rfd::FileDialog::new().add_filter("Godot", &["exe"]).pick_file()
            {
                app.set_godot(path);
            }
        });
    });
    if let Some(path) = &editor {
        let text = path.display().to_string();
        ui.add(
            egui::Label::new(egui::RichText::new(&text).font(egui::FontId::monospace(12.5)).color(p.weak)).truncate(),
        )
        .on_hover_text(&text);
    }
    // Кому нужен: проекты Godot и их версии.
    let need: Vec<String> = app
        .projects
        .iter()
        .filter(|pr| pr.kind == crate::registry::Kind::Godot)
        .map(|pr| match pr.engine.as_ref().and_then(|e| e.version.clone()) {
            Some(v) => format!("{} ({v})", pr.name()),
            None => pr.name(),
        })
        .collect();
    if !need.is_empty() {
        w::note(ui, format!("{} {}", t("Нужен для:"), need.join(", ")));
    }

    ui.add_space(12.0);
    w::section_label(ui, "Unity");
    ui.add_space(2.0);
    let installed = crate::engines::unity_editors();
    chrome::setting_row(ui, t("Редакторы из Unity Hub"), |ui| {
        let text = if installed.is_empty() { t("не найдены").to_owned() } else { installed.join(" · ") };
        ui.add(
            egui::Label::new(egui::RichText::new(&text).font(egui::FontId::monospace(12.5)).color(p.text)).truncate(),
        )
        .on_hover_text(&text);
    });
    w::note(ui, t("Проект открывается в версии из его ProjectVersion.txt."));
    for pr in app.projects.iter().filter(|pr| pr.kind == crate::registry::Kind::Unity) {
        let Some(version) = pr.engine.as_ref().and_then(|e| e.version.clone()) else { continue };
        if !installed.contains(&version) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                w::dot(ui, anvil_ui::Tone::Warning);
                let text = format!("{}: {} {version} — {}", pr.name(), t("нужен"), t("не установлен"));
                ui.label(egui::RichText::new(text).size(13.0).color(p.text));
            });
        }
    }
}

/// Кузница — как в 0.2: папки, виды проектов, опрос origin, `-j`, GitHub.
fn forge(app: &mut App, ui: &mut Ui) {
    roots(app, ui);
    ui.add_space(12.0);
    hidden(app, ui);
    kinds(app, ui);
    ui.add_space(12.0);
    fetch(app, ui);
    ui.add_space(12.0);
    build(app, ui);
    ui.add_space(12.0);
    super::github::settings(app, ui);
}

/// Какие проекты искать: Rust, Godot, Unity, просто git.
fn kinds(app: &mut App, ui: &mut Ui) {
    ui.add_space(12.0);
    w::section_label(ui, t("Виды проектов"));
    ui.add_space(2.0);
    let mut changed = false;
    for (kind, label) in
        [("rust", "Rust"), ("godot", "Godot"), ("unity", "Unity"), ("git", t("Git (просто репозиторий)"))]
    {
        // Как у поиска проектов — без учёта регистра («Rust» в anvil.toml — тоже Rust).
        let mut on = app.config.kinds.iter().any(|k| k.eq_ignore_ascii_case(kind));
        let before = on;
        w::switch(ui, &mut on, label);
        if on != before {
            if on {
                app.config.kinds.push(kind.to_owned());
            } else {
                app.config.kinds.retain(|k| !k.eq_ignore_ascii_case(kind));
            }
            changed = true;
        }
    }
    if changed {
        app.save();
        app.rescan();
    }
}

fn roots(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Папки с проектами"));
    ui.add_space(2.0);
    w::note(ui, t("Проект — это сама папка или её подпапка, где есть Cargo.toml, project.godot или проект Unity."));
    ui.add_space(4.0);
    let mut remove = None;
    for (i, root) in app.config.roots.iter().enumerate() {
        path_row(ui, &root.to_string_lossy(), |ui| {
            if w::icon_button(ui, Icon::Trash, t("Убрать папку")).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        app.config.roots.remove(i);
        app.save();
        app.rescan();
    }
    ui.add_space(4.0);
    if w::button(ui, Kind::Secondary, Some(Icon::Plus), t("Добавить папку…")).clicked() {
        add_root(app);
    }
}

fn hidden(app: &mut App, ui: &mut Ui) {
    if app.config.hidden.is_empty() {
        return;
    }
    w::section_label(ui, t("Скрытые проекты"));
    ui.add_space(4.0);
    let mut restore = None;
    for (i, path) in app.config.hidden.iter().enumerate() {
        path_row(ui, &worker::display_name(path), |ui| {
            if w::button(ui, Kind::Ghost, None, t("Вернуть")).clicked() {
                restore = Some(i);
            }
        });
    }
    if let Some(i) = restore {
        app.config.hidden.remove(i);
        app.save();
    }
}

/// Убранное с Пульта — вернуть.
fn removed(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Убрано с Пульта"));
    ui.add_space(4.0);
    if app.config.deck.removed.is_empty() {
        w::note(ui, t("Ничего не убрано."));
        return;
    }
    let mut restore = None;
    for (i, key) in app.config.deck.removed.iter().enumerate() {
        let (project, what) = key.rsplit_once('|').unwrap_or((key.as_str(), ""));
        // Ключ — путь в нижнем регистре; имя берётся у самого проекта, если он найден.
        let path = app.projects.iter().map(|p| &p.path).find(|p| p.to_string_lossy().to_lowercase() == project);
        let name = worker::display_name(path.map_or(std::path::Path::new(project), |p| p.as_path()));
        let label = if what == "engine" || what.is_empty() { name } else { format!("{name} · {what}") };
        path_row(ui, &label, |ui| {
            if w::button(ui, Kind::Ghost, None, t("Вернуть")).clicked() {
                restore = Some(i);
            }
        });
    }
    if let Some(i) = restore {
        app.config.deck.removed.remove(i);
        app.save();
    }
}

fn fetch(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Опрос origin"));
    ui.add_space(2.0);
    chrome::setting_row(ui, t("Проверять новые коммиты"), |ui| {
        let before = app.config.fetch_minutes;
        let mut minutes = before;
        w::segmented(
            ui,
            &mut minutes,
            &[(0, None, t("вручную")), (5, None, t("5 мин")), (15, None, t("15 мин")), (60, None, t("час"))],
        );
        if minutes != before {
            app.config.fetch_minutes = minutes;
            app.save();
        }
    });
    w::note(ui, t("git fetch только узнаёт о новом на origin и ничего не меняет в рабочей копии."));
}

fn build(app: &mut App, ui: &mut Ui) {
    w::section_label(ui, t("Сборка"));
    ui.add_space(2.0);
    chrome::setting_row(ui, t("Сборок разом (cargo -j)"), |ui| {
        let before = app.config.build_jobs;
        let mut jobs = before;
        w::segmented(ui, &mut jobs, &[(0, None, t("авто")), (8, None, "8"), (4, None, "4"), (2, None, "2")]);
        if jobs != before {
            app.config.build_jobs = jobs;
            app.save();
        }
    });
    w::note(ui, t("Меньше — медленнее, но надёжнее: большим workspace может не хватить памяти на компоновку."));
}

fn path_row(ui: &mut Ui, text: &str, trailing: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(anvil_ui::theme::radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(10, 2))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                // Сначала кнопка справа, потом путь в оставшемся месте: длинный путь обрезается,
                // а не раздувает диалог.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    trailing(ui);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let text_style = egui::RichText::new(text).font(egui::FontId::monospace(12.5)).color(p.text);
                        ui.add(egui::Label::new(text_style).truncate()).on_hover_text(text);
                    });
                });
            });
        });
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::captured;
    use eframe::egui::{Key, Modifiers};

    fn keys(key: Key, mods: Modifiers) -> Option<String> {
        captured(key, None, mods).map(|c| c.keys().join("+"))
    }

    #[test]
    fn capture_needs_ctrl_or_alt() {
        let ctrl_alt = Modifiers { ctrl: true, alt: true, ..Modifiers::NONE };
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(keys(Key::Space, ctrl_alt).as_deref(), Some("Ctrl+Alt+Space"));
        assert_eq!(keys(Key::K, ctrl_alt).as_deref(), Some("Ctrl+Alt+K"));
        assert_eq!(keys(Key::Num7, ctrl_shift).as_deref(), Some("Ctrl+Shift+7"));
        assert_eq!(keys(Key::F9, Modifiers::CTRL).as_deref(), Some("Ctrl+F9"));
        assert_eq!(keys(Key::Space, Modifiers::ALT).as_deref(), Some("Alt+Space"));
        // С одним Ctrl или Alt буква — нет: Ctrl+C, Alt+F нужны самим программам.
        assert_eq!(keys(Key::C, Modifiers::CTRL), None);
        assert_eq!(keys(Key::K, Modifiers::ALT), None);
        // Без Ctrl и Alt — не сочетание: отняло бы у программ обычный ввод.
        assert_eq!(keys(Key::K, Modifiers::NONE), None);
        assert_eq!(keys(Key::K, Modifiers::SHIFT), None);
        assert_eq!(keys(Key::K, Modifiers::SHIFT | Modifiers::CTRL).as_deref(), Some("Ctrl+Shift+K"));
        // Раскладка дала знак — берётся клавиша на том же месте.
        let shifted = captured(Key::Questionmark, Some(Key::Num7), ctrl_shift).map(|c| c.keys().join("+"));
        assert_eq!(shifted.as_deref(), Some("Ctrl+Shift+7"));
        // Клавиши, которых сочетание не понимает.
        assert_eq!(keys(Key::Enter, Modifiers::CTRL), None);
        assert_eq!(keys(Key::ArrowUp, ctrl_alt), None);
    }
}
