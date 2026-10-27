//! «Демо оформления»: витрина набора (`anvil_ui::demo`) в своём окне.

use eframe::egui;

use crate::app::App;
use crate::i18n::t;

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(demo) = app.demo.as_mut() else { return };
    let mut close = false;
    let builder = egui::ViewportBuilder::default()
        .with_title(t("Anvil — демо оформления"))
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([1100.0, 680.0]);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("anvil-demo"), builder, |ui, _class| {
        demo.ui(ui);
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            close = true;
        }
    });
    if close {
        app.close_demo(ctx);
    }
}
