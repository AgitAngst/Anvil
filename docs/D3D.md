# anvil-d3d — окно egui на Direct3D 11

Крейт набора `crates/anvil-d3d` (с набора 0.7.0): окно egui без eframe — свой цикл (winit + egui-winit)
и свой рисовальщик мешей egui на Direct3D 11. Только Windows; на других системах крейт пустой.
Появился в Kamerton (окно микшера у трея), перенесён в набор 22.10.2026.

## Зачем

На маленьком окне главная статья памяти — графика. Замер в Kamerton (своя память процесса окна
360 × 405, release, видеокарта AMD, Windows 10):

| Чем рисует | Своя память | Открытие окна |
|---|---|---|
| eframe + wgpu (по умолчанию) | 546–558 МБ | — |
| eframe + wgpu dx12 / vulkan | 433 / 434 МБ | — |
| eframe + OpenGL (glow) | ~110 МБ | ~0,4 с |
| **anvil-d3d** | **~26 МБ** | **~0,2 с** |

Голые устройства без egui (сверх пустого процесса): D3D11 +4 МБ, D3D12 +30, OpenGL +43, Vulkan +58.

Брать, когда окно маленькое или живёт недолго (всплывашки, трей, быстрый запуск). Большие окна с
несколькими viewport и прочим из eframe — на eframe.

## Как пользоваться

```rust
struct App;
impl anvil_d3d::Content for App {
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| ui.label("Привет"));
    }
}
let config = anvil_d3d::Config { title: "App".into(), size: [420.0, 220.0], ..Default::default() };
anvil_d3d::run(config, Box::new(|ctx, hwnd| {
    anvil_ui::install(ctx, anvil_ui::Accent::BLUE, anvil_ui::ThemeChoice::System);
    Ok(Box::new(App) as Box<dyn anvil_d3d::Content>)
}))?;
```

- `Config`: заголовок, логический размер, место (физические пиксели; всплывашка, которая ставит себя
  сама, — `Some([-32000, -32000])`), рамка, изменение размера, поверх всех, без кнопки на панели задач.
- `make` получает контекст egui и HWND окна — для своих вызовов Win32 (место, регион углов, фокус).
- `Content::clear` — цвет фона кадра (по умолчанию `panel_fill`); для окна набора — `Palette::card` или
  `Palette::bg`.
- Закрыть — `ViewportCommand::Close`; снимок кадра — `ViewportCommand::Screenshot`, ответ — событие
  `egui::Event::Screenshot` на следующем кадре (снимается до `Present`). Пример:
  `cargo run -p anvil-d3d --example hello -- --snap out.ppm [--dark]`.

## Окно наготове (всплывашка у трея)

Как в Kamerton (и EarTrumpet): окно создаётся при старте и не уничтожается — открывается за 5–10 мс.

- В `make` сразу закрыть окно плащом DWM (`DwmSetWindowAttribute(DWMWA_CLOAK, TRUE)`): цикл покажет его
  после первого кадра, но видно его не будет. Первый кадр нарисовать с настоящим содержимым — прогрев
  шрифтов, текстур, значков; в `Content::after_present` первого кадра — `ShowWindow(SW_HIDE)`.
- Открыть из другого потока: `ShowWindow(SW_SHOWNOACTIVATE)` (под плащом) и `InvalidateRect` — спрятанному
  окну Windows не шлёт WM_PAINT, а побудка egui (`request_repaint`) после кадра спрятанного окна может не
  дойти. В кадре — поставить окно на место; в `after_present` — снять плащ и дать фокус.
- Закрыть — плащ и `SW_HIDE`; `Content::close_requested` вернуть `false`, чтобы Alt+F4 только прятал.
- Щелчок по значку, снявший фокус с окна, не должен открыть его снова: не открывать ~300 мс после закрытия.

## Как устроено

- Окно создаётся спрятанным; первый кадр рисуется сразу (невидимому окну Windows не шлёт WM_PAINT) и
  только после него окно показывается — без белой вспышки и уже на своём месте.
- Кадры — только по запросу egui: `set_request_repaint_callback` будит цикл winit из любого потока,
  `request_repaint_after` — через `ControlFlow::WaitUntil`. В покое цикл спит.
- Рисовальщик: устройство D3D11 (уровень 11_0), цепочка кадров flip-discard B8G8R8A8, размер буферов
  подгоняется к окну. Один шейдер (`D3DCompile` при запуске). Цвета egui — sRGB с умноженной альфой,
  смешивание в гамме (ONE / INV_SRC_ALPHA), текстуры R8G8B8A8_UNORM — как у egui_glow. Все сетки кадра —
  в одни динамические буферы, рисуются кусками со своими ножницами и текстурой.

## Чего нет

- Нескольких viewport (`show_viewport_*`), `PaintCallback`, MSAA, sRGB-буфера кадра.
- Сохранения состояния окна (`persistence`) и `ViewportCommand` кроме `Close` и `Screenshot`.
- Значка окна и AccessKit — программа ставит сама, если нужно.
