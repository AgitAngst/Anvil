//! Витрина `anvil-ui`: макет командного центра Anvil и все элементы набора (код — `anvil_ui::demo`).
//!
//! `cargo run -p anvil-ui --example gallery --features demo [-- --dark|--light] [--en|--ru] [--accent amber] [--tab elements|deck|motion]
//!  [--reduced] [--size WxH] [--scroll px] [--dialog|--settings|--about] [--popup menu|more|select]`
//!
//! `--popup` открывает на вкладке «Элементы» меню темы, меню «⋯» или выпадающий выбор — для снимков.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anvil_ui::demo::{Demo, Open, Page};
use anvil_ui::{Accent, CommonSettings, Lang, ThemeChoice};
use eframe::egui::{self, Ui};

struct Gallery(Demo);

impl eframe::App for Gallery {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        anvil_ui::motion::tick(ui.ctx());
        self.0.ui(ui);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.0.clear_color(visuals)
    }
}

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
    let page = match value("--tab").as_deref() {
        Some("elements") => Page::Elements,
        Some("deck") => Page::Deck,
        Some("motion") => Page::Motion,
        _ => Page::Center,
    };
    let open = if has("--dialog") {
        Some(Open::Release)
    } else if has("--settings") {
        Some(Open::Settings)
    } else if has("--about") {
        Some(Open::About)
    } else {
        None
    };
    let reduced = has("--reduced");
    let popup = value("--popup");
    let scroll = value("--scroll").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    let mut settings = CommonSettings { theme, ..CommonSettings::default() };
    if has("--en") {
        settings.language = Lang::En;
    } else if has("--ru") {
        settings.language = Lang::Ru;
    }

    // `--size 1440x1800` — окно другого размера: длинную вкладку целиком видно на одном снимке.
    let size = value("--size")
        .and_then(|v| v.split_once('x').and_then(|(w, h)| Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])))
        .unwrap_or([1440.0, 900.0]);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Anvil — демо оформления")
            .with_inner_size(size)
            .with_min_inner_size([1100.0, 680.0]),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "anvil-gallery",
        options,
        Box::new(move |cc| {
            anvil_ui::install(&cc.egui_ctx, Accent::ALL[accent], theme);
            anvil_ui::motion::set_reduced(&cc.egui_ctx, reduced);
            settings.apply(&cc.egui_ctx);
            let mut demo = Demo::new(settings, accent, page, open, reduced, scroll);
            demo.popup = popup;
            Ok(Box::new(Gallery(demo)))
        }),
    )
}
