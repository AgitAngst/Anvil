## Purpose

Программа подменяет палитру набора (высокий контраст системы и т. п.), и виджеты набора рисуются ею.

## ADDED Requirements

### Requirement: Подмена палитры
Набор SHALL давать `theme::set_palette_override(ctx, Some(палитра))` и `theme::set_palette_override(ctx, None)`. Пока
подмена задана, `Palette::of` и `Palette::of_ctx` MUST возвращать её, а стиль egui MUST быть собран из неё. После
`None` MUST возвращаться таблица набора (`Palette::new` для темы и акцента) и прежний стиль. Без подмены поведение набора
MUST не меняться.

#### Scenario: Подменили и сняли
- **WHEN** программа задаёт палитру с красным фоном, а затем `None`
- **THEN** `Palette::of_ctx` сначала даёт красный фон, потом — фон таблицы набора; `panel_fill` стиля так же

#### Scenario: Акцент сменили при подмене
- **WHEN** при заданной подмене программа зовёт `set_accent`
- **THEN** подмена остаётся в силе

### Requirement: Цвет очистки окна
`chrome::clear_color_of(ctx)` SHALL возвращать цвет `bg` с учётом подмены.

#### Scenario: Окно чистится цветом подмены
- **WHEN** задана палитра с фоном `bg`
- **THEN** `clear_color_of(ctx)` — `bg` этой палитры
