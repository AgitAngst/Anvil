//! Anvil — командный центр Rust-программ.

// В релизе не поднимаем окно консоли.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod amber;
mod app;
mod autostart;
mod builds;
mod config;
mod deck;
mod deps;
mod engines;
mod git;
mod github;
mod hotkey;
mod i18n;
mod installs;
mod instance;
mod jobs;
mod launch;
mod layout;
mod notify;
mod open;
mod procs;
mod registry;
mod release;
mod run;
mod runs;
mod rustsec;
mod tasks;
mod tray;
mod ui;
mod worker;

use eframe::egui;

struct Anvil(app::App);

impl eframe::App for Anvil {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Зовётся и когда окно спрятано в трей: трей, сочетание и уведомления — здесь, не в `ui`.
        self.0.tick(ctx);
        ui::background(&mut self.0, ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::draw(&mut self.0, ui);
    }

    fn on_exit(&mut self) {
        // Псевдоним, набранный без выхода из поля, — на диск и при выходе.
        self.0.flush();
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        anvil_ui::chrome::clear_color(visuals, app::ACCENT)
    }
}

fn main() -> eframe::Result<()> {
    // Помощник мягкой остановки: `anvil --ctrl-break <pid>` посылает Ctrl+Break консольной службе.
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("--ctrl-break") {
            let ok = args.get(2).and_then(|p| p.parse().ok()).is_some_and(runs::ctrl_break);
            std::process::exit(if ok { 0 } else { 1 });
        }
    }
    // Anvil с этими настройками уже работает (спрятан в трей) — показать его и выйти.
    let claim = instance::claim(&config::path());
    if claim == instance::Claim::Taken {
        return Ok(());
    }
    // Автозапуск (`--tray`) — сразу спрятанным в трей. Но не после обновления: перезапуск передаёт
    // прежние аргументы, а пользователь только что смотрел на окно.
    let tray_start = autostart::tray_start() && claim == instance::Claim::Fresh;
    // Следы прошлого обновления (*.old-…, папка загрузки) — прочь.
    anvil_update::cleanup();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Anvil")
            .with_visible(!tray_start)
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1040.0, 640.0])
            .with_icon(std::sync::Arc::new(egui::IconData {
                rgba: include_bytes!(concat!(env!("OUT_DIR"), "/window.rgba")).to_vec(),
                width: 64,
                height: 64,
            })),
        centered: true,
        ..Default::default()
    };
    eframe::run_native("Anvil", options, Box::new(move |cc| Ok(Box::new(Anvil(app::App::new(cc, tray_start))))))
}
