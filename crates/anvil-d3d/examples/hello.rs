// Окно с элементами набора на Direct3D 11: `cargo run -p anvil-d3d --example hello`.
// `--snap <файл.png>` — снять кадр через 10 кадров и выйти (проверка без экрана); `--dark` — тёмная тема.

#[cfg(windows)]
fn main() -> Result<(), String> {
    use anvil_ui::widgets::{self, Kind};
    use egui::{Event, ViewportCommand};

    struct Hello {
        on: bool,
        frames: u32,
        snap: Option<String>,
    }

    impl anvil_d3d::Content for Hello {
        fn ui(&mut self, ui: &mut egui::Ui) {
            let ctx = ui.ctx().clone();
            egui::CentralPanel::default().show(ui, |ui| {
                widgets::title(ui, "anvil-d3d", 24.0);
                widgets::note(ui, "egui на Direct3D 11 без eframe");
                ui.add_space(12.0);
                widgets::switch(ui, &mut self.on, "Переключатель");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    widgets::button(ui, Kind::Primary, None, "Главная");
                    widgets::button(ui, Kind::Secondary, None, "Обычная");
                });
            });
            if let Some(path) = &self.snap {
                self.frames += 1;
                if self.frames == 10 {
                    ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
                }
                let image = ctx.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(image) = image {
                    save_ppm(path, &image);
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                ctx.request_repaint();
            }
        }
    }

    // Снимок в PPM: без зависимостей ради примера.
    fn save_ppm(path: &str, image: &egui::ColorImage) {
        let mut data = format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
        for c in &image.pixels {
            data.extend_from_slice(&[c.r(), c.g(), c.b()]);
        }
        let _ = std::fs::write(path, data);
        println!("snapshot saved: {path}");
    }

    let args: Vec<String> = std::env::args().collect();
    let snap = args.iter().position(|a| a == "--snap").and_then(|i| args.get(i + 1)).cloned();
    let theme =
        if args.iter().any(|a| a == "--dark") { anvil_ui::ThemeChoice::Dark } else { anvil_ui::ThemeChoice::Light };
    let config = anvil_d3d::Config { title: "anvil-d3d".into(), size: [420.0, 220.0], ..Default::default() };
    anvil_d3d::run(
        config,
        Box::new(move |ctx, _hwnd| {
            anvil_ui::install(ctx, anvil_ui::Accent::VIOLET, theme);
            Ok(Box::new(Hello { on: true, frames: 0, snap }) as Box<dyn anvil_d3d::Content>)
        }),
    )
}

#[cfg(not(windows))]
fn main() {
    eprintln!("anvil-d3d: только Windows");
}
