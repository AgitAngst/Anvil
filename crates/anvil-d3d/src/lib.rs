//! Окно egui на Direct3D 11 без eframe: свой цикл (winit + egui-winit) и свой рисовальщик.
//!
//! Зачем: eframe рисует через OpenGL (glow) или wgpu, а на маленьком окне это главная статья памяти.
//! Замер в Kamerton (22.10.2026, своя память процесса окна 360 × 405): wgpu 430–560 МБ, OpenGL ~110 МБ,
//! D3D11 ~26 МБ; открытие окна ~0,2 с против ~0,4 с. Подробности и границы — `docs/D3D.md`.
//!
//! ```ignore
//! struct Hello;
//! impl anvil_d3d::Content for Hello {
//!     fn ui(&mut self, ui: &mut egui::Ui) {
//!         egui::CentralPanel::default().show(ui, |ui| ui.label("Привет"));
//!     }
//! }
//! let config = anvil_d3d::Config { title: "Hello".into(), size: [320.0, 120.0], ..Default::default() };
//! anvil_d3d::run(config, Box::new(|_ctx, _hwnd| Ok(Box::new(Hello) as Box<dyn anvil_d3d::Content>)))?;
//! ```
//!
//! Только Windows; на других системах крейт пустой.

#[cfg(windows)]
mod painter;
#[cfg(windows)]
mod shell;

#[cfg(windows)]
pub use painter::Painter;
#[cfg(windows)]
pub use shell::{Config, Content, Make, run};
