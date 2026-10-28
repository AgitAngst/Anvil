# Tasks

## 1. Проект Gradle

- [x] 1.1 `android/`: `settings.gradle.kts`, корневой `build.gradle.kts`, `gradle/libs.versions.toml`, wrapper 9.7.1, `gradle.properties` (`kitVersion=0.1.0`, `-Xmx2g`), модули `anvil-compose` (библиотека, `maven-publish`) и `sample`; `.gitignore` — каталоги сборки. Проверка: `./gradlew :anvil-compose:assembleDebug --max-workers=2` с кодом 0.

## 2. Тема

- [x] 2.1 `AnvilPalette` (токены `STYLE.md`, светлая и тёмная), `Accent` и `Accents`, `AnvilTheme` (палитра в `CompositionLocal`, `ColorScheme` из ролей, своя `Typography`), полосы системы по теме. Проверка: тест контраста пар (текст, слабый — на `bg` и `card`; текст на заливке акцента; акцентный текст на `card`) ≥ 4,5:1 для всех акцентов в обеих темах — зелёный.

## 3. Движение

- [x] 3.1 `Motion` (шкала из `MOTION.md`), `AnvilEasing` (кривые), «убрать анимацию» по `ANIMATOR_DURATION_SCALE`, `motionSpec`. Проверка: тест, который читает таблицу `docs/MOTION.md` и сверяет имена и миллисекунды, — зелёный.

## 4. Тема и язык: выбор и хранение

- [x] 4.1 `AnvilSettings` (тема, язык, «проверять обновления»), `AnvilLocale` (Android 13+ — `LocaleManager`, ниже — подмена контекста), `AnvilActivity`. Проверка: тест разбора и записи значений настроек — зелёный.

## 5. Готовые части

- [x] 5.1 `Segmented` (48 dp, роль выбора), `AppearanceSection` (тема и язык), `AboutCard` (знак, имя, состояние, одна кнопка, версия и сборка, ссылка на код), строки RU/EN. Проверка: `:anvil-compose:lintDebug` без ошибок, строки есть в обоих языках.

## 6. Пример-приложение

- [x] 6.1 `sample`: палитра, акценты, сегменты, «Оформление», «О программе», кнопка с движением. Проверка: `./gradlew :sample:assembleDebug` собирает APK; снимки — светлая и тёмная × русский и английский, если подключён телефон (29.10.2026 телефона не было — снимков нет).

## 7. Документы и сдача

- [x] 7.1 `docs/ANDROID.md` — раздел «Набор» (модули, что даёт, версия и тег, как подключить), `SPEC.md` (шаг 2), `HANDOFF.md`. Проверка: ссылки в документах ведут на существующие файлы.
- [x] 7.2 Полный прогон: `./gradlew :anvil-compose:testDebugUnitTest :anvil-compose:lintDebug :sample:assembleDebug --max-workers=2`, затем `./gradlew --stop`; код выхода 0.
