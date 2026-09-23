# Anvil

Командный центр для моих программ на Rust и общий набор, из которого они собраны.

- **`anvil-ui`** — единый вид всех программ на egui: темы, акцент на программу, значки,
  виджеты, каркас окна, окна «О программе» и «Настройки». Готов.
- **`anvil-update`** — проверка и установка обновлений изнутри программы. Впереди.
- **`anvil`** — окно командного центра. Сейчас: все проекты и их git-состояние, версии, запущенные
  бинарники, быстрые переходы; сборка, тесты, clippy, fmt и запуск с живым логом и разбором ошибок.
  Впереди: GitHub, установка, выпуски.

Замысел, этапы и решения — в [SPEC.md](SPEC.md); где остановились — в [HANDOFF.md](HANDOFF.md);
правила внешнего вида — в [docs/STYLE.md](docs/STYLE.md).

![Anvil](docs/anvil-dark.png)

![Элементы набора](docs/elements-light.png)

## Запуск

```sh
cargo run -p anvil --release
```

Настройки — `%APPDATA%\Anvil\anvil.toml`, или `anvil.toml` рядом с exe, если он там лежит
(портативный режим). При первом запуске из репозитория Anvil ищет проекты в соседних папках.

## Витрина

```sh
cargo run -p anvil-ui --example gallery
```

Флаги: `--dark` / `--light`, `--en` / `--ru`, `--accent ember|amber|teal|rose|blue`,
`--tab elements`, `--dialog` / `--settings` / `--about`.

## Подключение `anvil-ui`

```toml
[dependencies]
anvil-ui = { git = "https://github.com/AgitAngst/Anvil", tag = "kit-v0.1.0", features = ["serde"] }
```

```rust
use anvil_ui::{Accent, CommonSettings, Icon, Kind, chrome, widgets as w};

// При запуске:
anvil_ui::install(&cc.egui_ctx, Accent::TEAL, settings.common.theme);
settings.common.apply(&cc.egui_ctx);

// В кадре:
chrome::top_bar(ui, |ui| chrome::brand(ui, Icon::Package, "Tetrachrome"));
chrome::content(ui, |ui| {
    w::card(ui, |ui| {
        if w::button(ui, Kind::Primary, Some(Icon::Play), "Упаковать").clicked() { /* … */ }
    });
});
```

- Цвета — только из `Palette::of(ui)`: тема и акцент меняются на ходу, и всё следует за ними.
- Любой текст держит контраст не меньше 4.5:1 в обеих темах и со всеми акцентами — это проверяет тест.
- Строки самого набора переводятся на EN/RU (`anvil_ui::lang`); строки программы переводит программа.
- Для правок набора «на живую» — в `.cargo/config.toml` программы (не коммитить):

  ```toml
  [patch."https://github.com/AgitAngst/Anvil"]
  anvil-ui = { path = "D:/dev_personal/Anvil/crates/anvil-ui" }
  ```

## Лицензия

MIT
