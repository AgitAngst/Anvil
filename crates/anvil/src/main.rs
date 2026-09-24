//! Anvil — командный центр Rust-программ.

// В релизе не поднимаем окно консоли.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod amber;
mod app;
mod config;
mod deps;
mod git;
mod github;
mod i18n;
mod installs;
mod jobs;
mod launch;
mod notify;
mod open;
mod procs;
mod registry;
mod release;
mod run;
mod rustsec;
mod tasks;
mod ui;
mod worker;

use anvil_ui::Icon;
use eframe::egui;

struct Anvil(app::App);

impl eframe::App for Anvil {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::draw(&mut self.0, ui);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        anvil_ui::chrome::clear_color(visuals, app::ACCENT)
    }
}

fn main() -> eframe::Result<()> {
    // Следы прошлого обновления (*.old-…, папка загрузки) — прочь.
    anvil_update::cleanup();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Anvil")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([1040.0, 640.0])
            .with_icon(std::sync::Arc::new(anvil_ui::appicon::icon_data(app::ACCENT, Icon::Hammer))),
        centered: true,
        ..Default::default()
    };
    eframe::run_native("Anvil", options, Box::new(|cc| Ok(Box::new(Anvil(app::App::new(cc))))))
}
