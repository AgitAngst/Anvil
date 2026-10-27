## Why

Владелец (28.10.2026, план эволюции Anvil, этап 2): «код универсальный, и в Anvil было демо». Витрина всех элементов и анимаций
набора сейчас — пример `cargo run -p anvil-ui --example gallery` (1800 строк в `examples/`): её не видно в самой программе,
у других программ её нет, а копия в Anvil разошлась бы с набором.

## What Changes

- **Набор 0.10.0.** Витрина переезжает в крейт `anvil-ui` как модуль `demo` за feature `demo` (по умолчанию выключена, другие
  программы её не получают и не пересобирают): `anvil_ui::demo::Demo` — `new(settings, accent, page, open, reduced, scroll)`,
  `ui(&mut Ui)`, `clear_color(&Visuals)`; `Page`, `Open`. Пример `gallery` становится тонкой обёрткой над ним
  (`required-features = ["demo"]`, запуск — `cargo run -p anvil-ui --example gallery --features demo`); флаги прежние.
- **Anvil 0.5.0.** Окно «Демо оформления» — отдельное окно (viewport) с этой же витриной: команда в палитре `Ctrl+K` и пункт
  меню. Тема, язык и акцент демо — свои: при закрытии Anvil возвращает настройки владельца.
- Правила и документы: `docs/STYLE.md`, `docs/MOTION.md`, `README.md`, `SPEC.md`, `HANDOFF.md`.

## Non-goals

Новые элементы или анимации; снимки для `docs/` заново; тег `kit-v0.10.0` и выпуск приложения (только по команде владельца);
перевод других программ на новую версию набора.

## Impact

`crates/anvil-ui` (`Cargo.toml`, `src/lib.rs`, `src/demo/`, `examples/gallery.rs`), `crates/anvil` (`Cargo.toml`, `app.rs`,
`ui/`, `lang/en.tsv`), корневой `Cargo.toml` (версия набора). Совместимо: без feature `demo` набор не меняется.
