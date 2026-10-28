# Design

## Context

Правила — `docs/ANDROID.md`; цвета и роли — `docs/STYLE.md`; движение — `docs/MOTION.md`. Инструменты на ПК
уже стоят (JDK 21 и SDK 37 на `D:\Android`, Gradle 9.7.1 и AGP 9.4.1 в кэше). Телефон по adb к ПК сейчас не
подключён.

## Goals / Non-Goals

**Goals:** библиотека, которую программа подключает одной зависимостью и получает тему, выбор темы и языка,
«Оформление», «О программе» и движение; пример-приложение для снимков.

**Non-Goals:** обновление из GitHub (шаг 3), числа и даты по языку, свои шрифты (программа передаёт свою
`Typography`), публикация и тег набора, сборка `android/` в CI Anvil.

## Decisions

- **Где.** `android/` — отдельный проект Gradle в корне Anvil: модули `anvil-compose` (библиотека) и `sample`
  (приложение). Пакет — `io.github.agitangst.anvil.compose`. Rust-часть его не видит.
- **Версия набора** — своя, третья в репозитории: `kitVersion` в `android/gradle.properties`, тег
  `android-kit-vX.Y.Z` (только по команде). Номера `kit-v*` и `v*` не трогаются.
- **Как программа подключает.** Модуль публикуется через `maven-publish` (группа `io.github.agitangst.anvil`).
  Способ — подмодуль Anvil и `includeBuild` или JitPack по тегу — решается на шаге 4 вместе с Ampulse.
- **Палитра** — `data class AnvilPalette` с токенами `STYLE.md` и значениями по умолчанию оттуда же; программа
  меняет нейтральные через `copy()`. Акцент — `Accent` (светлая и тёмная тройка), акценты семьи —
  в `Accents` (те же, что в `STYLE.md`, и Ampulse из `ANDROID.md`).
- **`ColorScheme`** строится из ролей: `primary` — заливка акцента, `onPrimary` — текст на ней, фон и поверхность —
  `bg`, контейнеры — `card`/`raised`/`popup`, `outline` — `border_strong`, `error` — `danger`. Палитра доступна и
  напрямую (`AnvilTheme.palette`) — виджеты набора берут цвета из неё.
- **Язык без AppCompat.** Android 13+: `LocaleManager.applicationLocales` (система хранит и пересоздаёт экран
  сама). Ниже: выбор в `SharedPreferences`, `AnvilActivity.attachBaseContext` подменяет язык контекста,
  после выбора — `recreate()`. Новой зависимости нет.
- **Хранение** — `SharedPreferences` (`anvil`): тема, язык, «проверять обновления» (по умолчанию включено — для
  шага 3). DataStore не нужен ради трёх значений.
- **Движение** — `object Motion` (миллисекунды) и `object AnvilEasing` (`CubicBezierEasing`; имя `Easing` занято Compose), `motionSpec(ms, easing)`
  отдаёт `snap()` при выключенной анимации; флаг — `Settings.Global.ANIMATOR_DURATION_SCALE`, перечитывается
  при возвращении в программу. Тест читает таблицу `docs/MOTION.md` и сверяет имена и числа.
- **Строки набора** — `res/values` (английский) и `res/values-ru`; названия языков — без перевода.

## Risks / Trade-offs

- [Телефона нет у ПК] → снимки обеих тем и языков с примера-приложения — когда владелец подключит телефон;
  до этого проверка — тесты, lint и сборка.
- [Подмена контекста ниже Android 13 не меняет язык системных окон] → это ожидаемо; системные окна остаются
  на языке системы.
- [Третий номер версии в одном репозитории] → он описан в `ANDROID.md` и `RELEASES.md`; мастер выпуска Anvil
  его пока не знает.
