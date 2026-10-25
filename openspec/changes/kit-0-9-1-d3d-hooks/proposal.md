## Why

Владелец (25.10.2026, «анвил3д»): перевести Kamerton с собственной копии окна (`d3d.rs`, `shell.rs`) на крейт `anvil-d3d` набора.
У копии Kamerton три отличия от крейта: поле `Config::active` (всплывашка не берёт фокус), готовая всплывашка `Config::popup` и
перехват события окна до egui (медиа-клавиши громкости, которых egui не отдаёт).

## What Changes

- `Config::active` (по умолчанию `true`), `Config::popup(title, size)`.
- `Content::window_event(&WindowEvent)` — событие окна до egui; `anvil_d3d::WindowEvent` — реэкспорт `winit`.
- Версия набора 0.9.1.

## Impact

`crates/anvil-d3d` (`shell.rs`, `lib.rs`), `docs/D3D.md`. Совместимо: прежний вызов с `..Default::default()` работает как раньше.
