# Передача дел

Замысел и этапы — [SPEC.md](SPEC.md). Здесь — где остановились и как тут работать.

## Состояние (23.09.2026)

**K0 пройден**: репозиторий `AgitAngst/Anvil` (публичный), крейт `crates/anvil-ui`, витрина, CI.

`anvil-ui`:
- `theme.rs` — палитра-токены (`bg`, `surface`, `card`, `raised`, `hover`, `border`, `text`, `weak`, `faint`,
  акцент, `success`/`warning`/`danger`), акцент на программу (`Accent::EMBER` — Anvil, `AMBER`, `TEAL` —
  Tetrachrome, `ROSE` — FFMincer, `BLUE` — прочие), шрифты (Segoe UI / Cascadia Mono), стиль egui.
  Акцент лежит в `ctx.data`, `Palette::of(ui)` собирает палитру из темы и акцента. Тест контраста.
- `icons.rs` — значки линиями в сетке 24×24.
- `widgets.rs` — кнопки (`Kind`), значок-кнопка, бейдж, точка, клавиша, карточка, баннер, пустое
  состояние, переключатель, сегменты, вкладки, строка навигации, поиск, прогресс, крутилка, меню,
  уведомления (`Toasts`), подтверждение (`confirm`).
- `chrome.rs` — панели окна (`top_bar`, `status_bar`, `side_panel`, `content`), знак программы,
  `dialog`, «О программе» (`about`), общие настройки (`CommonSettings`, `common_settings`).
- `lang.rs` — EN/RU для строк набора, язык системы по умолчанию; тест полноты словаря.
- Фича `serde` — сериализация `CommonSettings`, `ThemeChoice`, `Lang`.

Тега `kit-v0.1.0` ещё нет — ставится по команде (от него зависят программы на этапе K2).

## Дальше

Следующий этап по SPEC — **A0**: крейт `crates/anvil` (окно командного центра на `anvil-ui`),
сканирование проектов, список и карточка, git-состояние. Без команды не начинать.

## Как здесь работать

- egui/eframe 0.36 новее обучающих данных (`App::ui(&mut Ui)`, `egui::Panel`, `ctx.global_style()`,
  `Popup::menu`, `Modal`) — сверять с исходниками в `~/.cargo/registry/src/*/egui-0.36.2`.
- `cargo clippy` не пересобирает exe витрины — перед снимками `cargo build -p anvil-ui --example gallery`.
- Снимки окна — только `PrintWindow(hwnd, dc, 2)` по окну процесса витрины, не весь экран.
  Флаги витрины (`--dark`, `--settings`, …) открывают нужное без кликов; кнопку можно нажать через
  UI Automation `InvokePattern` по имени (AccessKit), так открывались меню.
- Строки набора — только через `tr(ctx, "…")` с русским ключом и переводом в `lang.rs`, иначе тест красный.
- Новые цвета — только токенами палитры; пары «текст–фон» добавлять в тест контраста.
- Правила вида — [docs/STYLE.md](docs/STYLE.md); меняешь значения или добавляешь виджет — поправь и его.
- Длинные Python-скрипты правок — файлом, не heredoc в Bash (heredoc ломается).
- `cargo fmt` — ширина 120 (`rustfmt.toml`); CI требует fmt, clippy `-D warnings` и тесты на Windows и Linux.
