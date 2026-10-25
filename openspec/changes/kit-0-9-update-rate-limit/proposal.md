## Why

Владелец (25.10.2026): при проверке обновления в Kamerton показывалось «GitHub: HTTP 403» — лимит анонимных
запросов к API GitHub (60 в час на адрес). Просил понятный текст и обход лимита.

## What Changes

- `anvil-update`: на ответ 403/429 — запасной путь без API (`releases/latest` → тег → адреса архива и
  `SHA256SUMS` по соглашению о выпусках); не вышло — ошибка «GitHub: HTTP 403 (rate limit), retry in N min».
- `anvil_update::errors::describe(error, ru)` — понятный текст на языке окна; баннер и `about_status` зовут его.
- `Config::web` — адрес сайта (меняется только в проверках), как `api`.
- Без изменений: форма `State`, `Config::new`, поведение при обычном ответе API.

## Impact

`crates/anvil-update` (`lib.rs`, `releases.rs`, `errors.rs`, `ui.rs`), `docs/RELEASES.md`. Версия набора — 0.9.0 (в
тот же ещё не выпущенный набор, что и подмена палитры).
