//! Витрина `anvil-ui`: макет командного центра Anvil и все элементы набора.
//!
//! `cargo run -p anvil-ui --example gallery [-- --dark|--light] [--en|--ru] [--accent amber] [--tab elements]
//!  [--dialog|--settings|--about]`

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::Instant;

use anvil_ui::chrome::{self, AboutAction, AppInfo};
use anvil_ui::widgets::{self as w, Toasts};
use anvil_ui::{Accent, CommonSettings, Icon, Kind, Lang, Palette, ThemeChoice, Tone, semibold};
use eframe::egui::{self, RichText, Sense, Stroke, Ui, Vec2};

const INFO: AppInfo = AppInfo {
    name: "Anvil",
    icon: Icon::Hammer,
    version: env!("CARGO_PKG_VERSION"),
    tagline: "Командный центр Rust-программ",
    repository: env!("CARGO_PKG_REPOSITORY"),
};

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();

    let theme = if has("--dark") {
        ThemeChoice::Dark
    } else if has("--light") {
        ThemeChoice::Light
    } else {
        ThemeChoice::System
    };
    let accent = value("--accent")
        .and_then(|name| Accent::ALL.iter().position(|a| a.name.eq_ignore_ascii_case(&name)))
        .unwrap_or(0);
    let page = if value("--tab").as_deref() == Some("elements") { Page::Elements } else { Page::Center };
    let open = if has("--dialog") {
        Some(Open::Release)
    } else if has("--settings") {
        Some(Open::Settings)
    } else if has("--about") {
        Some(Open::About)
    } else {
        None
    };
    let mut settings = CommonSettings { theme, ..CommonSettings::default() };
    if has("--en") {
        settings.language = Lang::En;
    } else if has("--ru") {
        settings.language = Lang::Ru;
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Anvil — демо оформления")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1100.0, 680.0]),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "anvil-gallery",
        options,
        Box::new(move |cc| {
            anvil_ui::install(&cc.egui_ctx, Accent::ALL[accent], theme);
            settings.apply(&cc.egui_ctx);
            Ok(Box::new(Gallery::new(settings, accent, page, open)))
        }),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Open {
    Release,
    Settings,
    About,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Center,
    Elements,
}

struct Project {
    name: &'static str,
    about: &'static str,
    path: &'static str,
    tone: Tone,
    trailing: Option<(&'static str, Tone)>,
    version: &'static str,
    ahead: &'static str,
    ci: (&'static str, Tone),
    dirty: Option<&'static str>,
    binaries: &'static [(&'static str, bool)],
}

const PROJECTS: [Project; 4] = [
    Project {
        name: "Amber",
        about: "Приватный мессенджер: сервер, десктоп, Android",
        path: r"D:\dev_personal\amber",
        tone: Tone::Success,
        trailing: Some(("0.3.1", Tone::Accent)),
        version: "v0.3.0",
        ahead: "+4 коммита",
        ci: ("CI пройден", Tone::Success),
        dirty: None,
        binaries: &[("amber-desktop", true), ("amber-server", false), ("amber-admin", false)],
    },
    Project {
        name: "Tetrachrome",
        about: "Упаковщик каналов текстур",
        path: r"D:\dev_personal\Tetrachrome",
        tone: Tone::Warning,
        trailing: Some(("3 файла", Tone::Warning)),
        version: "v0.1.0",
        ahead: "+1 коммит",
        ci: ("без CI", Tone::Neutral),
        dirty: Some("3 изменённых файла"),
        binaries: &[("tetrachrome", false)],
    },
    Project {
        name: "FFMincer",
        about: "Конвертер аудио и видео поверх ffmpeg",
        path: r"D:\dev_personal\FFMincer",
        tone: Tone::Neutral,
        trailing: Some(("↓2", Tone::Neutral)),
        version: "v0.1.0",
        ahead: "отстаёт на 2",
        ci: ("без CI", Tone::Neutral),
        dirty: None,
        binaries: &[("ffmincer", false)],
    },
    Project {
        name: "Anvil",
        about: "Командный центр и общий набор",
        path: r"D:\dev_personal\Anvil",
        tone: Tone::Danger,
        trailing: Some(("сборка", Tone::Danger)),
        version: "—",
        ahead: "нет тегов",
        ci: ("CI упал", Tone::Danger),
        dirty: Some("новый репозиторий"),
        binaries: &[("anvil", false)],
    },
];

struct Gallery {
    settings: CommonSettings,
    accent: usize,
    page: Page,
    project: usize,
    detail_tab: usize,
    search: String,
    field: String,
    toasts: Toasts,
    confirm_open: bool,
    settings_open: bool,
    about_open: bool,
    banner_open: bool,
    notify: bool,
    started: Instant,
}

impl Gallery {
    fn new(settings: CommonSettings, accent: usize, page: Page, open: Option<Open>) -> Self {
        Self {
            settings,
            accent,
            page,
            project: 0,
            detail_tab: 0,
            search: String::new(),
            field: "amber-desktop --profile test".into(),
            toasts: Toasts::default(),
            confirm_open: open == Some(Open::Release),
            settings_open: open == Some(Open::Settings),
            about_open: open == Some(Open::About),
            banner_open: true,
            notify: true,
            started: Instant::now(),
        }
    }
}

impl eframe::App for Gallery {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let p = Palette::of(ui);
        self.top_bar(ui);
        self.task_bar(ui);
        match self.page {
            Page::Center => {
                chrome::side_panel(ui, "projects", 248.0, |ui| self.sidebar(ui));
                chrome::content(ui, |ui| self.project_view(ui));
            }
            Page::Elements => chrome::content(ui, |ui| self.elements(ui)),
        }

        let ctx = ui.ctx().clone();
        let mut settings_open = self.settings_open;
        chrome::dialog(&ctx, "settings", anvil_ui::tr(&ctx, "Настройки"), 480.0, &mut settings_open, |ui| {
            if chrome::common_settings(ui, &mut self.settings) {
                // Здесь программа сохранила бы настройки на диск.
            }
            ui.add_space(10.0);
            w::section_label(ui, "Anvil");
            ui.add_space(2.0);
            w::switch(ui, &mut self.notify, "Уведомлять о конце долгих задач");
        });
        self.settings_open = settings_open;
        let status = Some("Установлена последняя версия");
        if chrome::about(&ctx, &mut self.about_open, &INFO, status) == Some(AboutAction::CheckUpdates) {
            self.toasts.push("Обновлений нет", Tone::Success);
        }
        if self.confirm_open {
            let body = |ui: &mut Ui| {
                w::note(ui, "Будет сделано по порядку:");
                ui.add_space(6.0);
                for step in [
                    "Cargo.toml: 0.3.0 → 0.3.1 (7 крейтов)",
                    "git commit -m \"Release v0.3.1\"",
                    "git tag v0.3.1",
                    "git push origin main v0.3.1",
                    "CI соберёт выпуск в amber-releases",
                ] {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        anvil_ui::icons::paint(ui.painter(), rect, Icon::ArrowRight, p.faint);
                        w::mono(ui, step, Some(p.text));
                    });
                }
                ui.add_space(8.0);
                w::banner(ui, Tone::Warning, "Push не отменить.", "Тег увидят все.", |_| {});
            };
            match w::confirm(&ctx, "release", "Выпустить Amber v0.3.1?", body, "Выпустить", false) {
                Some(true) => {
                    self.confirm_open = false;
                    self.toasts.push("Выпуск v0.3.1 запущен — ждём CI", Tone::Accent);
                }
                Some(false) => self.confirm_open = false,
                None => {}
            }
        }
        self.toasts.show(&ctx);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        chrome::clear_color(visuals, Accent::ALL[self.accent])
    }
}

impl Gallery {
    fn top_bar(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        chrome::top_bar(ui, |ui| {
            chrome::brand(ui, INFO.icon, INFO.name);
            ui.add_space(14.0);
            let mut page = self.page;
            w::segmented(ui, &mut page, &[(Page::Center, None, "Командный центр"), (Page::Elements, None, "Элементы")]);
            self.page = page;
            ui.add_space(14.0);
            w::search_field(ui, &mut self.search, "Найти проект или действие…", Some("Ctrl+K"), 340.0);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ctx = ui.ctx().clone();
                let gear = w::icon_button(ui, Icon::Gear, anvil_ui::tr(&ctx, "Настройки"));
                w::menu(&gear, 230.0, |ui| {
                    if w::menu_item(ui, Some(Icon::Gear), anvil_ui::tr(&ctx, "Настройки"), Some("Ctrl+,")).clicked()
                    {
                        self.settings_open = true;
                    }
                    if w::menu_item(ui, Some(Icon::Info), anvil_ui::tr(&ctx, "О программе"), None).clicked() {
                        self.about_open = true;
                    }
                    w::menu_separator(ui);
                    if w::menu_item(ui, Some(Icon::Refresh), anvil_ui::tr(&ctx, "Проверить обновления"), None).clicked()
                    {
                        self.toasts.push("Обновлений нет", Tone::Success);
                    }
                });
                if w::icon_button(ui, Icon::Refresh, "Обновить состояние").clicked() {
                    self.toasts.push("Состояние обновлено", Tone::Success);
                }
                ui.add_space(6.0);
                let before = self.settings.theme;
                w::segmented(
                    ui,
                    &mut self.settings.theme,
                    &[
                        (ThemeChoice::Dark, Some(Icon::Moon), ""),
                        (ThemeChoice::Light, Some(Icon::Sun), ""),
                        (ThemeChoice::System, Some(Icon::Monitor), ""),
                    ],
                );
                if self.settings.theme != before {
                    anvil_ui::choose(ui.ctx(), self.settings.theme);
                }
                ui.add_space(10.0);
                // Акценты справа налево, чтобы шли в порядке ALL слева направо.
                for (i, accent) in Accent::ALL.iter().enumerate().rev() {
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                    let fill = accent.swatch(p.dark).fill;
                    ui.painter().circle_filled(rect.center(), 8.0, fill);
                    if i == self.accent {
                        ui.painter().circle_stroke(rect.center(), 10.5, Stroke::new(1.5, p.text));
                    }
                    if r.on_hover_text(accent.name).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.accent = i;
                        anvil_ui::set_accent(ui.ctx(), *accent);
                    }
                }
                ui.add_space(2.0);
                w::note(ui, "Акцент");
            });
        });
    }

    fn task_bar(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        chrome::status_bar(ui, |ui| {
            w::section_label(ui, "Задачи");
            ui.add_space(6.0);
            w::spinner(ui, 14.0);
            w::mono(ui, "cargo build --release -p amber-desktop", Some(p.text));
            ui.add_space(8.0);
            let t = (self.started.elapsed().as_secs_f32() / 40.0).fract();
            w::progress(ui, Some(t), 220.0);
            w::mono(ui, &format!("{}/301", (t * 301.0) as u32), None);
            ui.ctx().request_repaint();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                w::button(ui, Kind::Ghost, Some(Icon::Terminal), "Лог");
                w::badge(ui, "ещё 2 в очереди", Tone::Neutral);
                w::mono(ui, &format!("0:{:02}", self.started.elapsed().as_secs() % 60), None);
            });
        });
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            w::section_label(ui, "Проекты");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                w::icon_button(ui, Icon::Plus, "Добавить путь");
            });
        });
        ui.add_space(4.0);
        for (i, project) in PROJECTS.iter().enumerate() {
            if w::nav_item(ui, self.project == i, project.tone, project.name, project.trailing).clicked() {
                self.project = i;
            }
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            w::section_label(ui, "Состояние");
        });
        ui.add_space(4.0);
        let p = Palette::of(ui);
        for (tone, text) in [
            (Tone::Success, "1 обновление доступно"),
            (Tone::Warning, "1 проект с правками"),
            (Tone::Danger, "1 упавший CI"),
        ] {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                w::dot(ui, tone);
                ui.label(RichText::new(text).size(13.0).color(p.weak));
            });
        }
    }

    fn project_view(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        let project = &PROJECTS[self.project];

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(project.name).font(semibold(24.0)).color(p.text));
                    ui.add_space(6.0);
                    w::badge(ui, project.version, Tone::Neutral);
                    w::badge(ui, project.ahead, Tone::Accent);
                    w::badge(ui, project.ci.0, project.ci.1);
                });
                ui.horizontal(|ui| {
                    w::note(ui, project.about);
                    w::note(ui, "·");
                    w::mono(ui, project.path, None);
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let more = w::icon_button(ui, Icon::More, "Ещё");
                w::menu(&more, 240.0, |ui| {
                    w::menu_item(ui, Some(Icon::Folder), "Открыть папку", None);
                    w::menu_item(ui, Some(Icon::Terminal), "Терминал в папке", None);
                    w::menu_item(ui, Some(Icon::Code), "Открыть в VS Code", Some("Ctrl+E"));
                    w::menu_separator(ui);
                    w::menu_item(ui, Some(Icon::Package), "Зависимости", None);
                    ui.add_enabled_ui(false, |ui| w::menu_item(ui, Some(Icon::Rocket), "Выпуск (нужен CI)", None));
                    w::menu_separator(ui);
                    if w::menu_item_danger(ui, Some(Icon::Trash), "cargo clean · 4,2 ГБ").clicked() {
                        self.toasts.push("target/ очищен — 4,2 ГБ", Tone::Success);
                    }
                });
                w::icon_button(ui, Icon::Code, "Открыть в VS Code");
                w::icon_button(ui, Icon::Terminal, "Терминал в папке");
                ui.add_space(8.0);
                if w::button(ui, Kind::Secondary, Some(Icon::Rocket), "Выпуск…").clicked() {
                    self.confirm_open = true;
                }
                if w::button(ui, Kind::Secondary, Some(Icon::Check), "Тесты").clicked() {
                    self.toasts.push("Тесты: 148 прошли, 0 упали", Tone::Success);
                }
                if w::button(ui, Kind::Secondary, Some(Icon::Hammer), "Собрать").clicked() {
                    self.toasts.push("Сборка поставлена в очередь", Tone::Neutral);
                }
                if w::button(ui, Kind::Primary, Some(Icon::Play), "Запустить").clicked() {
                    self.toasts.push(format!("{} запущен", project.binaries[0].0), Tone::Success);
                }
            });
        });
        ui.add_space(18.0);

        if self.banner_open && self.project == 0 {
            let mut update = false;
            let mut later = false;
            w::banner(
                ui,
                Tone::Accent,
                "Вышла v0.3.1",
                "Установлена 0.3.0 · 12 изменений · 18 МБ",
                |ui| {
                    later = w::button(ui, Kind::Ghost, None, "Позже").clicked();
                    w::button(ui, Kind::Ghost, None, "Что нового");
                    update = w::button(ui, Kind::Primary, Some(Icon::Download), "Обновить").clicked();
                },
            );
            if update {
                self.toasts.push("Скачивание v0.3.1…", Tone::Accent);
            }
            if later {
                self.banner_open = false;
            }
            ui.add_space(14.0);
        }

        ui.columns(3, |cols| {
            w::card(&mut cols[0], |ui| {
                w::card_title(ui, Icon::Branch, "Git");
                w::field_row(ui, "Ветка", |ui| {
                    w::mono(ui, "main", Some(p.text));
                });
                w::field_row(ui, "Дерево", |ui| match project.dirty {
                    Some(text) => {
                        w::dot(ui, Tone::Warning);
                        ui.label(text);
                    }
                    None => {
                        w::dot(ui, Tone::Success);
                        ui.label("чисто");
                    }
                });
                w::field_row(ui, "С origin", |ui| {
                    ui.label("↑0 ↓0");
                });
                w::field_row(ui, "Коммит", |ui| {
                    w::mono(ui, "451da94", Some(p.accent_text));
                    w::note(ui, "2 ч назад");
                });
            });
            w::card(&mut cols[1], |ui| {
                w::card_title(ui, Icon::Package, "Установлено");
                w::field_row(ui, "Версия", |ui| {
                    w::mono(ui, "0.3.0", Some(p.text));
                    w::badge(ui, "есть 0.3.1", Tone::Accent);
                });
                w::field_row(ui, "Где", |ui| {
                    w::mono(ui, r"%LOCALAPPDATA%\Programs\amber", None);
                });
                w::field_row(ui, "Прошлые", |ui| {
                    ui.label("0.2.0, 0.1.0");
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    w::button(ui, Kind::Secondary, Some(Icon::ArrowUp), "Обновить");
                    w::button(ui, Kind::Ghost, None, "Откатить");
                });
            });
            w::card(&mut cols[2], |ui| {
                w::card_title(ui, Icon::Terminal, "Бинарники");
                for (name, running) in project.binaries {
                    ui.horizontal(|ui| {
                        w::dot(ui, if *running { Tone::Success } else { Tone::Neutral });
                        w::mono(ui, name, Some(p.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if *running {
                                w::icon_button(ui, Icon::Stop, "Остановить");
                                w::note(ui, "PID 18244");
                            } else {
                                w::icon_button(ui, Icon::Play, "Запустить");
                            }
                        });
                    });
                }
            });
        });

        ui.add_space(18.0);
        w::tabs(ui, &mut self.detail_tab, &["Коммиты", "Зависимости", "Выпуски", "Заметки"]);
        ui.add_space(8.0);
        match self.detail_tab {
            0 => commits(ui),
            1 => dependencies(ui),
            2 => releases(ui),
            _ => w::empty_state(ui, Icon::Pencil, "Заметок пока нет", "Здесь будут HANDOFF.md и TODO.md проекта."),
        }
    }

    fn elements(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        ui.label(RichText::new("Элементы набора").font(semibold(24.0)).color(p.text));
        w::note(ui, "Всё, из чего собираются окна программ. Цвета берутся из палитры — тема и акцент меняются вверху.");
        ui.add_space(16.0);

        ui.columns(2, |cols| {
            let (left, right) = cols.split_at_mut(1);
            let left = &mut left[0];
            let right = &mut right[0];

            w::card(left, |ui| {
                w::card_title(ui, Icon::Hammer, "Кнопки");
                ui.horizontal_wrapped(|ui| {
                    w::button(ui, Kind::Primary, Some(Icon::Play), "Главная");
                    w::button(ui, Kind::Secondary, Some(Icon::Hammer), "Обычная");
                    w::button(ui, Kind::Ghost, None, "Тихая");
                    w::button(ui, Kind::Danger, Some(Icon::Trash), "Удалить");
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(false, |ui| {
                        w::button(ui, Kind::Primary, None, "Недоступна");
                        w::button(ui, Kind::Secondary, None, "Недоступна");
                    });
                    w::icon_button(ui, Icon::Folder, "Папка");
                    w::icon_button(ui, Icon::Terminal, "Терминал");
                    w::icon_button(ui, Icon::Gear, "Настройки");
                    let menu = w::button(ui, Kind::Secondary, Some(Icon::More), "Меню");
                    w::menu(&menu, 220.0, |ui| {
                        w::menu_item(ui, Some(Icon::Play), "Запустить", Some("F5"));
                        w::menu_item(ui, Some(Icon::Hammer), "Собрать", Some("Ctrl+B"));
                        w::menu_separator(ui);
                        w::menu_item_danger(ui, Some(Icon::Trash), "Удалить");
                    });
                });
            });
            left.add_space(12.0);

            w::card(left, |ui| {
                w::card_title(ui, Icon::Info, "Метки");
                ui.horizontal_wrapped(|ui| {
                    w::badge(ui, "v0.3.0", Tone::Neutral);
                    w::badge(ui, "+4 коммита", Tone::Accent);
                    w::badge(ui, "CI пройден", Tone::Success);
                    w::badge(ui, "3 файла", Tone::Warning);
                    w::badge(ui, "CI упал", Tone::Danger);
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for tone in [Tone::Neutral, Tone::Accent, Tone::Success, Tone::Warning, Tone::Danger] {
                        w::dot(ui, tone);
                    }
                    ui.add_space(12.0);
                    w::kbd(ui, "Ctrl");
                    w::kbd(ui, "Shift");
                    w::kbd(ui, "B");
                });
            });
            left.add_space(12.0);

            w::card(left, |ui| {
                w::card_title(ui, Icon::Pencil, "Ввод");
                w::search_field(ui, &mut self.search, "Поиск…", Some("Ctrl+K"), ui.available_width());
                ui.add_space(4.0);
                ui.add(egui::TextEdit::singleline(&mut self.field).desired_width(f32::INFINITY));
                ui.add_space(4.0);
                w::switch(ui, &mut self.settings.check_updates, "Проверять обновления при запуске");
                w::switch(ui, &mut self.settings.prerelease, "Предлагать пред-выпуски");
                w::switch(ui, &mut self.notify, "Уведомлять о конце долгих задач");
                ui.add_space(4.0);
                let mut theme = self.settings.theme;
                w::segmented(
                    ui,
                    &mut theme,
                    &[
                        (ThemeChoice::System, Some(Icon::Monitor), "Система"),
                        (ThemeChoice::Light, Some(Icon::Sun), "Светлая"),
                        (ThemeChoice::Dark, Some(Icon::Moon), "Тёмная"),
                    ],
                );
                if theme != self.settings.theme {
                    self.settings.theme = theme;
                    anvil_ui::choose(ui.ctx(), theme);
                }
            });
            left.add_space(12.0);

            w::card(left, |ui| {
                w::card_title(ui, Icon::Clock, "Ход работы");
                let t = (self.started.elapsed().as_secs_f32() / 6.0).fract();
                ui.horizontal(|ui| {
                    w::progress(ui, Some(t), 260.0);
                    w::mono(ui, &format!("{:>3.0}%", t * 100.0), None);
                });
                ui.add_space(4.0);
                w::progress(ui, None, 260.0);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if w::button(ui, Kind::Secondary, None, "Уведомление").clicked() {
                        self.toasts.push("Сборка завершена за 41 с", Tone::Success);
                    }
                    if w::button(ui, Kind::Secondary, None, "Ошибка").clicked() {
                        self.toasts.push("cargo test: 2 теста упали", Tone::Danger);
                    }
                    if w::button(ui, Kind::Secondary, None, "Диалог").clicked() {
                        self.confirm_open = true;
                    }
                    if w::button(ui, Kind::Secondary, None, "Настройки").clicked() {
                        self.settings_open = true;
                    }
                    if w::button(ui, Kind::Secondary, None, "О программе").clicked() {
                        self.about_open = true;
                    }
                });
            });

            w::card(right, |ui| {
                w::card_title(ui, Icon::Warning, "Баннеры");
                w::banner(ui, Tone::Accent, "Вышла v0.3.1", "12 изменений", |ui| {
                    w::button(ui, Kind::Primary, None, "Обновить");
                });
                ui.add_space(6.0);
                w::banner(ui, Tone::Success, "Готово", "Установлена 0.3.1", |_| {});
                ui.add_space(6.0);
                w::banner(ui, Tone::Warning, "Есть правки", "3 файла не закоммичены", |_| {});
                ui.add_space(6.0);
                w::banner(ui, Tone::Danger, "CI упал", "clippy: 2 ошибки", |ui| {
                    w::button(ui, Kind::Ghost, None, "Лог");
                });
            });
            right.add_space(12.0);

            w::card(right, |ui| {
                w::card_title(ui, Icon::File, "Текст");
                ui.label(RichText::new("Заголовок окна").font(semibold(24.0)).color(p.text));
                ui.label(RichText::new("Заголовок карточки").font(semibold(15.0)).color(p.text));
                ui.label("Основной текст — 14 pt, Segoe UI.");
                w::note(ui, "Пояснение — 13 pt, приглушённый.");
                w::section_label(ui, "Подпись раздела");
                w::mono(ui, "cargo build --release   451da94", Some(p.text));
                ui.label(RichText::new("Акцентная ссылка").color(p.accent_text));
            });
            right.add_space(12.0);

            w::card(right, |ui| {
                w::card_title(ui, Icon::Sun, "Палитра");
                let tokens = [
                    ("bg", p.bg),
                    ("surface", p.surface),
                    ("card", p.card),
                    ("raised", p.raised),
                    ("border", p.border),
                    ("text", p.text),
                    ("weak", p.weak),
                    ("faint", p.faint),
                    ("accent", p.accent),
                    ("success", p.success),
                    ("warning", p.warning),
                    ("danger", p.danger),
                ];
                egui::Grid::new("tokens").num_columns(4).spacing(Vec2::new(10.0, 8.0)).show(ui, |ui| {
                    for (i, (name, color)) in tokens.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::hover());
                            ui.painter().rect(
                                rect,
                                5,
                                *color,
                                Stroke::new(1.0, p.border_strong),
                                egui::StrokeKind::Inside,
                            );
                            w::mono(ui, name, None);
                        });
                        if i % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
            });
            right.add_space(12.0);

            w::card(right, |ui| {
                w::card_title(ui, Icon::Package, "Значки");
                let all = [
                    Icon::ArrowDown,
                    Icon::ArrowRight,
                    Icon::ArrowUp,
                    Icon::Branch,
                    Icon::Check,
                    Icon::Clock,
                    Icon::Close,
                    Icon::Code,
                    Icon::Download,
                    Icon::File,
                    Icon::Folder,
                    Icon::Gear,
                    Icon::Hammer,
                    Icon::Info,
                    Icon::Lock,
                    Icon::Moon,
                    Icon::More,
                    Icon::Monitor,
                    Icon::Package,
                    Icon::Pause,
                    Icon::Pencil,
                    Icon::Play,
                    Icon::Plus,
                    Icon::Refresh,
                    Icon::Rocket,
                    Icon::Search,
                    Icon::Server,
                    Icon::Stop,
                    Icon::Sun,
                    Icon::Terminal,
                    Icon::Trash,
                    Icon::Warning,
                ];
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                    for icon in all {
                        let (rect, r) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                        ui.painter().rect_filled(rect, 6, if r.hovered() { p.hover } else { p.raised });
                        anvil_ui::icons::paint(ui.painter(), rect.shrink(8.0), icon, p.text);
                        r.on_hover_text(format!("{icon:?}"));
                    }
                });
            });
        });

        ui.add_space(12.0);
        w::card(ui, |ui| {
            w::empty_state(
                ui,
                Icon::Search,
                "Ничего не нашлось",
                "Пустое состояние: значок, заголовок и что делать дальше.",
            );
        });
    }
}

fn commits(ui: &mut Ui) {
    let p = Palette::of(ui);
    let rows = [
        ("451da94", "TODO: раздел «Задумки»", "2 ч назад"),
        ("9b34ec5", "Пересылка сообщений", "5 ч назад"),
        ("d9fd3f2", "Голосовые, обложки видео, файлы до 2 ГБ", "вчера"),
        ("ff60830", "M11: до 32 участников, группы", "вчера"),
        ("e050b87", "Смена сертификата без новых приглашений", "2 дня назад"),
    ];
    w::card(ui, |ui| {
        for (i, (hash, message, when)) in rows.iter().enumerate() {
            if i > 0 {
                w::divider(ui);
            }
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                w::mono(ui, hash, Some(p.accent_text));
                ui.add_space(6.0);
                ui.label(*message);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| w::note(ui, *when));
            });
        }
    });
}

fn dependencies(ui: &mut Ui) {
    let p = Palette::of(ui);
    let rows = [
        ("eframe", "0.36.2", "0.36.2", None),
        ("tokio", "1.47.1", "1.48.0", Some(("совместимо", Tone::Success))),
        ("russh", "0.63.0", "0.64.1", Some(("ломающее", Tone::Warning))),
        ("rustls", "0.23.31", "0.23.32", Some(("RUSTSEC-2026-0117", Tone::Danger))),
    ];
    w::card(ui, |ui| {
        egui::Grid::new("deps").num_columns(4).spacing(Vec2::new(28.0, 10.0)).min_col_width(80.0).show(ui, |ui| {
            for head in ["Крейт", "Сейчас", "Последняя", ""] {
                w::section_label(ui, head);
            }
            ui.end_row();
            for (name, now, last, status) in rows {
                w::mono(ui, name, Some(p.text));
                w::mono(ui, now, None);
                w::mono(ui, last, None);
                match status {
                    Some((text, tone)) => w::badge(ui, text, tone),
                    None => w::badge(ui, "актуально", Tone::Neutral),
                };
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            w::button(ui, Kind::Secondary, Some(Icon::ArrowUp), "Обновить совместимые");
            w::note(ui, "Потом прогоню тесты; красные — верну Cargo.lock.");
        });
    });
}

fn releases(ui: &mut Ui) {
    w::card(ui, |ui| {
        for (i, (tag, when, assets)) in
            [("v0.3.0", "23.09.2026", "3 файла"), ("v0.2.0", "18.09.2026", "2 файла")].iter().enumerate()
        {
            if i > 0 {
                w::divider(ui);
            }
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                w::badge(ui, tag, if i == 0 { Tone::Accent } else { Tone::Neutral });
                ui.label(*when);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    w::button(ui, Kind::Ghost, Some(Icon::Download), assets);
                });
            });
        }
    });
}
