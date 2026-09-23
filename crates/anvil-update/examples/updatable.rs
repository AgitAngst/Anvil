//! Маленькая программа с обновлением — для ручной и сквозной проверки `anvil-update`.
//!
//! `cargo run -p anvil-update --example updatable -- --version 0.1.0 --repo owner/name [--api http://127.0.0.1:8765]`

// Окно без консоли — как у настоящих программ семьи.
#![windows_subsystem = "windows"]

use anvil_ui::chrome::{self, AboutAction, AppInfo};
use anvil_ui::widgets as w;
use anvil_ui::{Accent, CommonSettings, Icon, Kind};
use anvil_update::{Config, Updater};
use eframe::egui;

struct App {
    updater: Updater,
    settings: CommonSettings,
    about: bool,
    version: &'static str,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.updater.auto(&self.settings);
        chrome::top_bar(ui, |ui| chrome::brand(ui, Icon::Package, "Updatable"));
        chrome::content(ui, |ui| {
            anvil_update::ui::banner(ui, &self.updater, &mut self.settings);
            w::card(ui, |ui| {
                w::title(ui, &format!("Updatable v{}", self.version), 18.0);
                let dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));
                let fresh = dir.is_some_and(|d| d.join("NEW.txt").exists());
                w::note(
                    ui,
                    if fresh {
                        "Файлы обновлены: рядом лежит NEW.txt из архива."
                    } else {
                        "Исходная копия."
                    },
                );
                ui.add_space(8.0);
                if w::button(ui, Kind::Secondary, Some(Icon::Info), "О программе").clicked() {
                    self.about = true;
                }
            });
        });
        let info = AppInfo {
            name: "Updatable",
            icon: Icon::Package,
            version: self.version,
            tagline: "Пример для anvil-update",
            repository: "",
        };
        let status = anvil_update::ui::about_status(&ctx, &self.updater);
        if chrome::about(&ctx, &mut self.about, &info, status.as_deref()) == Some(AboutAction::CheckUpdates) {
            self.updater.check(self.settings.prerelease, None, true);
        }
    }
}

fn main() -> eframe::Result<()> {
    anvil_update::cleanup();
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let version: &'static str = value("--version").unwrap_or_else(|| env!("CARGO_PKG_VERSION").into()).leak();
    let repo = value("--repo").unwrap_or_else(|| "AgitAngst/Anvil".into());
    let mut config = Config::new("updatable", version, &repo);
    if let Some(api) = value("--api") {
        config.api = api;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Updatable").with_inner_size([760.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "updatable",
        options,
        Box::new(move |cc| {
            anvil_ui::install(&cc.egui_ctx, Accent::BLUE, anvil_ui::ThemeChoice::Dark);
            let settings = CommonSettings { theme: anvil_ui::ThemeChoice::Dark, ..CommonSettings::default() };
            settings.apply(&cc.egui_ctx);
            Ok(Box::new(App { updater: Updater::new(config, cc.egui_ctx.clone()), settings, about: false, version }))
        }),
    )
}
