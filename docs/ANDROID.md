# Программы под Android

Как устроены программы семьи на телефоне: инструменты, сборки под магазины, версия, подпись, выпуск,
обновление, вид, движение, значки и проверка. Для программ на ПК то же самое описывают
[STYLE.md](STYLE.md), [MOTION.md](MOTION.md), [ICONS.md](ICONS.md) и [RELEASES.md](RELEASES.md); здесь —
как эти правила выглядят на Android и чего на ПК нет. Общий гайдлайн UX/UI и гайдлайн самой программы
(`docs/ui-guideline.md`) действуют и здесь.

Общий набор кода на Kotlin — в папке `android/` этого репозитория, см. [раздел 13](#13-набор-android).

## Коротко

| Что | Как |
|---|---|
| Язык и вид | Kotlin, Jetpack Compose, Material 3 |
| Инструменты | Gradle 9.7.1, AGP 9.4.1, Kotlin 2.4.20, Compose BOM 2026.09.00, JDK 21 |
| Уровни API | `compileSdk` и `targetSdk` 37, `minSdk` 29 (Android 10) |
| Имя пакета | `io.github.agitangst.<app>`; после первой публикации не меняется |
| Сборки | по магазину: `github`, `rustore`, `play` |
| Версия | `versionName` = `X.Y.Z`, `versionCode` — из неё, номер сборки = число коммитов |
| Подпись | один ключ на программу для всех магазинов, вне репозитория |
| Выпуск | тег `vX.Y.Z`, `<app>-X.Y.Z-android.apk` и `SHA256SUMS` в GitHub Release |
| Обновление | только в сборке `github`: баннер, проверка SHA-256, установщик системы |
| Проверка | тесты и lint на ПК, снимки на телефоне — обе темы × оба языка |

## 1. Инструменты и проект

**Версии одни на всю семью** — те, что в таблице выше. Они записаны в `gradle/libs.versions.toml` и
`gradle/wrapper/gradle-wrapper.properties` каждой программы. Обновляют их разом во всех программах и сразу
правят эту таблицу; свою версию в одной программе не заводят.

**Раскладка.** Репозиторий только с Android — проект Gradle в корне (`settings.gradle.kts`, `app/`).
Репозиторий, где есть и другие части, — проект в `android/`. Общий код программы — модулями рядом с `app/`.

**ПК владельца.** JDK и SDK стоят один раз и только читаются: `JAVA_HOME = D:\Android\jdk-21`,
`ANDROID_HOME = D:\Android\Sdk`. Всё, что собирается, — в папке проекта на диске сборок (E:).

**Gradle бережёт память:** `org.gradle.jvmargs=-Xmx2g`, запуск с `--max-workers=2`, после работы —
`./gradlew --stop`. Gradle и cargo одновременно не гонять.

**Имя пакета** новой программы — `io.github.agitangst.<app>` (тот же адрес, что у репозитория на GitHub).
Программа, уже вышедшая под другим именем, его не меняет: для Android это другое приложение.

## 2. Сборки под магазины

Одно измерение вариантов — `store`:

| Сборка | Где | Самообновление | Что ещё |
|---|---|---|---|
| `github` | APK в GitHub Releases | есть | может отправлять SMS сама; сеть — только для проверки обновлений |
| `rustore` | RuStore | нет — обновляет магазин | APK или AAB |
| `play` | Google Play | нет — обновляет магазин | AAB; без `SEND_SMS`; особые разрешения — с объяснением в консоли Play |

Язык внутри программы и AAB: Play по умолчанию раскладывает AAB по языкам и ставит только язык системы — выбор другого
языка внутри программы тогда не находит строк. В сборке AAB разделение по языкам выключено:
`android { bundle { language { enableSplit = false } } }`.

- Что отличается по магазинам, живёт за интерфейсом в наборе исходников сборки (`src/github/`,
  `src/play/`), а не проверками `BuildConfig.FLAVOR` по всему коду.
- Разрешения, которые нужны одной сборке, — только в её манифесте (`src/github/AndroidManifest.xml`):
  `REQUEST_INSTALL_PACKAGES` и, у офлайн-программы, `INTERNET` есть только в `github`.
- Google Play даёт `SEND_SMS` почти только приложениям SMS по умолчанию: в сборке `play` программа
  открывает готовое сообщение, а «Отправить» нажимает человек.
- Точные будильники (`SCHEDULE_EXACT_ALARM`) и напоминание на весь экран (`USE_FULL_SCREEN_INTENT`) Play
  разрешает не всем: в `play` — с объяснением в консоли и с запасным путём, если разрешения нет.
- Начинают с одной сборки `github`; `rustore` и `play` добавляют, когда программа идёт в магазин.

## 3. Версия

- **`versionName`** — `X.Y.Z` по общей схеме (этап — вторая цифра, исправление — третья, после 0.9.x —
  1.0.0). Пред-выпуск — `X.Y.Z-beta.N` (`-rc.N`, `-alpha.N`), как в [RELEASES.md](RELEASES.md).
- **`versionCode`** = `(X·10000 + Y·100 + Z)·100 + K`, где `K` = 99 у выпуска и `N` у пред-выпуска
  (1–98): `0.2.0` → `20099`, `0.2.0-beta.3` → `20003`. Число растёт с каждой версией, пред-выпуск меньше
  своего выпуска — магазины и установщик Android этого и требуют.
- **Одно место:** `appVersion=X.Y.Z` в `gradle.properties`; `versionName` и `versionCode` считаются из него
  в `app/build.gradle.kts`, выпуск правит только эту строку.
- **Номер сборки** — число коммитов (`git rev-list --count HEAD`) в поле `BuildConfig`; в «О программе» —
  мелко рядом с версией. CI берёт историю целиком (`fetch-depth: 0`), иначе номер будет 1.

Готовый фрагмент для `app/build.gradle.kts`: версия — из `appVersion` в `gradle.properties` (например,
`appVersion=0.2.0`), номер сборки — из git (нет git или ошибка — 0), в коде это `BuildConfig.BUILD_NUMBER`:

```kotlin
val appVersion = providers.gradleProperty("appVersion").get()
val m = Regex("""(\d+)\.(\d+)\.(\d+)(?:-[a-z]+\.(\d+))?""").matchEntire(appVersion)
    ?: error("appVersion: X.Y.Z или X.Y.Z-beta.N, а не $appVersion")
val (major, minor, patch, preN) = m.destructured // preN пустая у выпуска
val buildNumber = runCatching {
    providers.exec { commandLine("git", "rev-list", "--count", "HEAD") }
        .standardOutput.asText.get().trim().toInt()
}.getOrDefault(0)

android {
    defaultConfig {
        versionName = appVersion
        versionCode = (major.toInt() * 10000 + minor.toInt() * 100 + patch.toInt()) * 100 +
            (preN.toIntOrNull() ?: 99)
        buildConfigField("int", "BUILD_NUMBER", "$buildNumber")
    }
    buildFeatures { buildConfig = true }
}
```

## 4. Подпись

- **Один ключ на программу** — для GitHub, RuStore и Google Play. В Play — «Подпись приложений Google Play»
  с этим же ключом (загрузить свой ключ, а не брать выданный Google): иначе APK с GitHub и из Play не
  обновляют друг друга.
- **Ключ создаёт владелец** (`keytool -genkeypair -v -keystore <app>-release.jks -keyalg RSA -keysize 4096
  -validity 36500 -alias <app>`) и хранит вне репозитория, с копией во втором месте. Потерянный ключ — новая
  версия больше не встанет поверх старой ни у кого.
- **На ПК** путь и пароли — в `keystore.properties` рядом с проектом (в `.gitignore`).
- **В CI** — секреты репозитория: `<APP>_KEYSTORE` (файл в base64), `<APP>_KEYSTORE_PASSWORD`,
  `<APP>_KEY_ALIAS`, `<APP>_KEY_PASSWORD`. Ключ видит только шаг сборки выпуска.
- **Без ключа выпуска нет:** сборка с отладочной подписью наружу не уходит — поверх неё потом не встанет
  настоящая.

## 5. Выпуск

По [RELEASES.md](RELEASES.md), с поправками для телефона:

- тег `vX.Y.Z` на коммите, где `appVersion` уже поднята;
- файл — `<app>-X.Y.Z-android.apk`, в том же `SHA256SUMS`, что и остальные файлы выпуска;
- заметки — по-английски, тело GitHub Release (тег с заметками — `git tag -a … --cleanup=verbatim -F notes.md`);
- CI — `.github/workflows/android-release.yml` по скилу `github-ci`: только по кнопке и на тег выпуска,
  Ubuntu, JDK 21 закреплён, потолок времени; тесты, lint, сборка `github` release, переименование,
  `SHA256SUMS`, публикация. Сборки для магазинов собираются тем же workflow по кнопке и в Release не идут.

## 6. Обновление (сборка `github`)

Как у `anvil-update` на ПК ([RELEASES.md](RELEASES.md), «Как устроено обновление»), но на модуле `anvil-update`
Android-набора (раздел 13) и с поправками телефона:

1. **Когда проверяется.** При каждом запуске программы (раз за процесс), если включено «Обновления → Проверять
   обновления» (по умолчанию включено), и по кнопке «Проверить» в «О программе». Расписания нет: пока программу не
   открыли, она в сеть не ходит. Ошибки авто-проверки не показываются — они видны только при проверке по кнопке.
2. **Что предлагается.** Самый новый выпуск новее текущей версии; черновики не в счёт; пред-выпуски (`-beta.N`) —
   только при включённом «Предлагать пред-выпуски» (по умолчанию выключено). Выпуск, в котором нет
   `<app>-X.Y.Z-android.apk` этой программы, **пропускается**. На ПК выпуск без архива показывается ссылкой на
   страницу, а на телефоне так нельзя: в репозитории живут выпуски других программ и частей (например, для ПК),
   и «Обновить» по ним нечего ставить.
3. **Баннер** над содержимым: «Вышла vX · Обновить · Что нового · Позже · Пропустить эту версию». Окон при запуске
   нет. «Что нового» открывает заметки выпуска (при обходе лимита заметок нет). «Позже» скрывает баннер до
   следующего запуска программы. «Пропустить эту версию» запоминает версию: авто-проверка её больше не предлагает
   (следующую за ней — предложит), а проверка по кнопке предлагает снова.
4. **Лимит запросов.** Список выпусков спрашивается без токена, а анонимных запросов — 60 в час на адрес. Ответ 403
   или 429 с признаками лимита (`retry-after` или `x-ratelimit-remaining: 0`) — не тупик: последний обычный выпуск
   узнаётся без API по переадресации `<сайт>/<репозиторий>/releases/latest`, а APK и `SHA256SUMS` берутся по
   адресам из соглашения (`…/releases/download/<тег>/<имя>`). Заметок к выпуску при этом нет, пред-выпуски не
   видны. Не вышло и это — ошибка с минутами до сброса лимита понятным текстом на языке программы
   (`UpdateError.describe`).
5. **Скачивание и проверка.** «Обновить» берёт `SHA256SUMS` выпуска и качает APK в кэш программы
   (`cacheDir/anvil-update/`; место чистится перед новой загрузкой и после установки) — с ходом в процентах и
   кнопкой «Отмена». SHA-256 считается по ходу и сверяется со строкой этого файла в `SHA256SUMS`: нет
   `SHA256SUMS`, нет строки для файла или сумма не совпала — не ставится ничего, а несовпавший файл удаляется.
   После ошибки «Повторить» качает заново.
6. **Разрешение.** Ставить APK из программы Android позволяет только с «Установка неизвестных приложений». Если
   его нет, вместо установки — строка, зачем оно нужно, и кнопка, открывающая его настройку для этой программы
   (`Updater.installPermissionIntent()`). Проверенный APK ждёт в кэше: когда человек вернулся в программу,
   установка продолжается сама, без новой загрузки. Если система за это время выгрузила программу, «Обновить»
   придётся нажать снова — APK скачается заново.
7. **Установка** — сессия `PackageInstaller`; её статус приходит в приёмник `InstallReceiver` (не экспортирован).
   Android показывает своё окно подтверждения — подтверждает человек, в обход него не поставить ничего. «Отмена»
   в окне системы — не ошибка: обновление снова предлагается. Отказ установщика — ошибка с текстом системы и
   «Повторить». После замены программа запускается уже новой версией; «Установлено» показывается при следующем
   запуске (версия, поставленная изнутри, запоминается).

От ПК отличается и то, чего нет: замены файлов и переключения `current`, отката версии — установку делает
система, прежние версии программа не хранит.

У офлайн-программы проверка обновлений — единственный выход в сеть: GitHub получает только запрос, данные
пользователя не уходят. Это пишется рядом с настройкой (в разделе «Обновления» набора пояснение уже есть).

## 7. Вид

- **Material 3, цвета — роли семьи:** фон, поверхность, текст, слабый текст, рамки и акцент программы в
  трёх значениях — заливка, текст на заливке, акцентный текст ([STYLE.md](STYLE.md), 2.2) — для светлой и
  тёмной темы. Из них строится `ColorScheme`; в коде цвета числами не пишутся.
- **Динамический цвет (Material You) выключен:** акцент — лицо программы.
- **Тема:** «Как в системе», «Светлая», «Тёмная». **Язык:** «Как в системе», «Русский», «English» —
  переключение внутри программы (в наборе — без AppCompat, раздел 13), на Android 13+ ещё и в системных
  настройках (`locales_config.xml`). Строки — `res/values` (английский) и `res/values-ru`; склонения — `plurals`;
  числа и даты — по языку.
- **Доступность:** цель нажатия ≥ 48 dp; крупный шрифт системы (1,3×) ничего важного не обрезает; у
  кнопки-значка — подпись для TalkBack; заголовки размечены как заголовки; контраст ≥ 4,5:1 в обеих темах.
- **Экран до краёв** (обязателен с API 35) с отступами системных панелей; жест «назад» с предпросмотром.
- **Настройки:** «Оформление» (тема, язык), «Обновления» (только `github`), дальше — разделы программы.
  **«О программе»:** знак, имя, версия `X.Y.Z` и мелко номер сборки, состояние обновления и одна кнопка по
  состоянию, ссылка на код со значком «наружу».

Акценты программ под Android (ПК — в [STYLE.md](STYLE.md)):

| Программа | Светлая: заливка / на ней / текст | Тёмная: заливка / на ней / текст | Заметка |
|---|---|---|---|
| Ampulse | `#0B6B63` / `#FFFFFF` / `#0B5A53` | `#4FBFB0` / `#1A2220` / `#7ED8CB` | бирюза рядом с `TEAL` (171°) — решение владельца 28.10.2026 |

## 8. Движение

- **Числа и кривые — из [MOTION.md](MOTION.md):** те же имена (`PRESS` 90, `STATE` 180, `LEAVE` 120,
  `LAYOUT` 200, `ENTER` 240, `CHECK` 380 …), кривые — `CubicBezierEasing` с теми же точками (`ease_out` —
  `0.16, 1, 0.3, 1`; `ease_in` — `0.7, 0, 0.84, 0`; `ease_standard` — `0.4, 0, 0.2, 1`; `ease_pop` —
  `0.34, 1.56, 0.64, 1`). Своё число — сначала в шкалу MOTION.md.
- **«Убрать анимацию»:** системная шкала аниматора 0 (`Settings.Global.ANIMATOR_DURATION_SCALE`) или
  «Удалить анимацию» в спецвозможностях — переходы мгновенные, бесконечного движения нет, смысл виден
  цветом, значком и словом. Перечитывать при возвращении в программу.
- Правила те же: 90–300 мс, одно яркое на экран, бесконечное — только пока идёт процесс.

## 9. Значки

Как рисовать — [ICONS.md](ICONS.md). На телефоне:

- **Адаптивный значок:** `ic_launcher_background`, `ic_launcher_foreground`, `ic_launcher_monochrome` —
  векторные drawable 108 dp, знак — в безопасной зоне 66 dp (по кругу), монохромный — для тематических
  значков Android 13+.
- **Значок уведомлений** `ic_notification` — 24 dp, один цвет, силуэт знака без фона.
- Проверка — `./gradlew :app:processGithubDebugResources` и глазами на телефоне: круг, скруглённый квадрат,
  тематический значок, строка уведомлений.

## 10. Данные и права

- Офлайн по умолчанию: сеть — только там, где без неё нельзя, и это видно в настройках.
- Личные данные (здоровье, переписка) не уходят в облачную копию Google без выбора человека:
  `dataExtractionRules` — облако выключено, перенос между телефонами — можно.
- Разрешение спрашивается в момент, когда нужно, с объяснением в одну строку; отказ не ломает экран —
  на его месте строка с причиной и кнопкой «Разрешить».

## 11. Проверка

- **На ПК:** `./gradlew :app:testGithubDebugUnitTest :app:lintGithubDebug --max-workers=2`; судить по коду
  выхода, вывод — в файл, в разговор хвост.
- **На телефоне:** установка по adb и снимки (скрипт `phone.py` из общих инструментов: `install`, `start`,
  `shot`, `tap`, `text`) — светлая и тёмная × русский и английский, крупный шрифт, маленький экран; пустое
  состояние, ошибка, нет разрешения.
- Живые проверки — не на данных владельца; снимки в публичных репозиториях — только в демо-режиме.

## 12. Документы программы

Как у любой программы семьи: `CLAUDE.md` с разделом «Сборка (Android)» (пути JDK и SDK, команды проверки,
`--max-workers=2`), `docs/README.md`, `docs/ui-guideline.md`, `HANDOFF.md`, `SPEC.md`, OpenSpec.

## 13. Набор (`android/`)

Проект Gradle в корне этого репозитория; Rust-часть его не видит, CI Anvil его пока не собирает. Пакеты —
`io.github.agitangst.anvil.compose` и `io.github.agitangst.anvil.update`. Программа берёт тему, «Оформление»,
«О программе», движение и обновление отсюда, а не пишет своё. Версия набора — 0.2.0, тега пока нет.

| Модуль | Что |
|---|---|
| `anvil-compose` | библиотека (Kotlin, Compose, Material 3); группа `io.github.agitangst.anvil`, артефакт `anvil-compose`; публикация через `maven-publish` настроена, нигде пока не опубликована |
| `anvil-update` | библиотека (Kotlin, Compose); группа `io.github.agitangst.anvil`, артефакт `anvil-update`; обновление программы из GitHub Releases, только для сборки `github`; зависит от `anvil-compose` (`api`), новых библиотек нет (сеть — `HttpURLConnection`, JSON — свой разбор, установка — `PackageInstaller`); публикация настроена так же, как у `anvil-compose` |
| `sample` | пример-приложение: палитра, акценты, сегменты, «Оформление», «Обновления», «О программе» с проверкой, баннер обновления, кнопка с движением — для снимков обеих тем и языков |

**Что даёт `anvil-compose`:**

| Что | Имя в коде | Заметка |
|---|---|---|
| Палитра по ролям | `AnvilPalette` | токены [STYLE.md](STYLE.md) (2.1); `light()`, `dark()`, `of(dark, accent)`; свои нейтральные цвета — через `copy()`; `soft(цвет)` — мягкая подложка (16 % в тёмной теме, 12 % в светлой); `tone(Tone)` — цвет смысла |
| Смысловой тон | `Tone` | `Neutral`, `Accent`, `Success`, `Warning`, `Danger`; рядом с цветом всегда слово |
| Акцент | `Accent`, `AccentShades`, `Accents` | тройка «заливка / на заливке / текст» для светлой и тёмной; готовые `VIOLET`, `AMBER`, `TEAL`, `ROSE`, `BLUE` ([STYLE.md](STYLE.md), 2.2) и `AMPULSE` (раздел 7); список — `Accents.all` |
| Тема | `AnvilTheme` | параметры `accent`, `mode`, `palette` (функция «тёмная ли тема → палитра»), `typography` (шрифты — программы); динамический цвет выключен; полосы системы — под тему; палитра внутри — `AnvilTheme.palette` |
| `ColorScheme` | `colorScheme()` | расширение `AnvilPalette`: `primary` — заливка акцента, `onPrimary` — текст на ней, фон и поверхность — `bg`, контейнеры — `card`/`raised`/`popup`, `outline` — `borderStrong`, `error` — `danger` |
| Выбор темы | `ThemeMode` | `System`, `Light`, `Dark` — «Как в системе», «Светлая», «Тёмная» |
| Выбор языка | `AppLanguage` | `System`, `Russian`, `English` — «Как в системе», «Русский», «English» |
| Настройки | `AnvilSettings(context)` | `themeMode`, `checkUpdates` (по умолчанию включено — выключатель «Проверять обновления» в `UpdatesSection`, значение уходит в `Updater.auto`), `language` (только чтение); хранит `SharedPreferences` `anvil`; тема — состояние Compose, применяется сразу |
| Язык внутри программы | `AnvilLocale`, `AnvilActivity` | Android 13+ — `LocaleManager` (система хранит выбор и пересоздаёт экран); ниже — выбор в `SharedPreferences`, `AnvilActivity` подменяет язык контекста, после выбора — `recreate()`; без AppCompat; программе нужен `android:localeConfig` в манифесте |
| Сегменты | `Segmented` | 2–4 коротких варианта; высота ≥ 48 dp, для TalkBack — роль выбора |
| «Оформление» | `AppearanceSection` | тема и язык сегментами; названия языков — на своём языке |
| «О программе» | `AboutCard`, `AboutAction` | знак (`mark`), имя, состояние (`status`, `statusTone`), одна кнопка (`action`), версия и `build`, `error`, ссылка на код (`codeUrl`) со значком «наружу» |
| Карточка, заголовок | `AnvilCard`, `SectionTitle` | карточка семьи (фон `card`, скругление 20, поля 16); подпись раздела — заголовок для TalkBack |
| Значок | `AnvilIcons.ExternalLink` | «наружу»: линии 1,8 на сетке 24 |
| Шкала движения | `Motion` | имена и миллисекунды как в [MOTION.md](MOTION.md); `Motion.SCALE` — шкала данными |
| Кривые | `AnvilEasing` | `Out`, `In`, `Standard`, `Pop`, `Linear`; имя `Easing` занято Compose |
| Анимация набора | `motionSpec(ms, easing)` | `tween` по шкале; при выключенной в системе анимации — `snap()` |
| «Убрать анимацию» | `LocalReducedMotion`, `isReducedMotion(context)`, `rememberReducedMotion()` | шкала аниматора 0; перечитывается при возвращении в программу; `AnvilTheme` кладёт флаг в `LocalReducedMotion` сама |

Строки набора и его модулей — `res/values` (английский) и `res/values-ru`. Пока нет: чисел и дат по языку, своих
шрифтов (программа передаёт свою `Typography`).

**Подключение темы и настроек** (импорты опущены; в манифесте программы — `android:localeConfig`, см. раздел 7):

```kotlin
class MainActivity : AnvilActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val settings = AnvilSettings(this)
        setContent {
            AnvilTheme(accent = Accents.BLUE, mode = settings.themeMode) {
                Scaffold { inner ->
                    AppearanceSection(settings, Modifier.padding(inner).padding(16.dp))
                }
            }
        }
    }
}
```

«О программе» — `AboutCard(name, version, mark = { … })`; состояние, кнопка, номер сборки и ссылка на код — по
необходимости. Свои нейтральные цвета: `AnvilTheme(accent, palette = { AnvilPalette.of(it, accent).copy(bg = …) })`.

**Что даёт `anvil-update`** (только сборка `github`):

| Что | Имя в коде | Заметка |
|---|---|---|
| Что обновлять | `UpdateConfig` | `app` — начало имени APK (`<app>-X.Y.Z-android.apk`), `version` — текущая (`BuildConfig.VERSION_NAME`), `repo` — `owner/name` на GitHub; `api` и `web` — адреса GitHub (по умолчанию `https://api.github.com` и `https://github.com`) |
| Проверяльщик | `Updater` | один на процесс — `Updater.shared(context, config)`; `state` и `showBanner` — наблюдаемые Compose; `auto(enabled)` — проверка при запуске (раз за процесс), `check()` — по кнопке, `update()` — «Обновить» и «Повторить», `cancel()` — «Отмена», `later()` — «Позже», `skip()` — «Пропустить эту версию»; `offerPrereleases` — «Предлагать пред-выпуски» (хранится в `SharedPreferences` `anvil`, по умолчанию выключено); `installPermissionIntent()` — настройка «Установка неизвестных приложений» |
| Состояние | `UpdateState` | `Idle`, `Checking`, `UpToDate`, `Available`, `Downloading` (байты из байтов), `NeedsPermission`, `Confirm`, `Installing`, `Installed`, `Failed` |
| Ошибка | `UpdateError` и `describe(context)` | `RateLimit` (с минутами до сброса), `Http`, `Network`, `BadAnswer`, `NoApk`, `NoSums`, `Checksum`, `Storage`, `Install`; `describe` — понятный текст на языке программы, строки модуля — RU/EN |
| Выпуск | `Update`, `Asset` | версия, тег, заметки, страница выпуска, APK и `SHA256SUMS` |
| Версия | `Version` | semver с пред-выпусками (`0.3.1 > 0.3.0`, `1.0.0 > 1.0.0-rc.1`); рядом `apkName` и `expectedSum` |
| Связь с экраном | `UpdaterEffects(updater)` | открывает окно подтверждения системы и продолжает установку после выдачи разрешения; `UpdateBanner` зовёт её сам |
| Баннер | `UpdateBanner(updater)` | «Вышла vX · Обновить · Что нового · Позже · Пропустить эту версию», ход скачивания с «Отменой», строка про разрешение, ошибка; «Что нового» — диалог из баннера |
| «Обновления» | `UpdatesSection(settings, updater)` | раздел настроек: «Проверять обновления» (`AnvilSettings.checkUpdates`), «Предлагать пред-выпуски» (`Updater.offerPrereleases`), пояснение про сеть |
| «О программе» | `AboutUpdate`, `rememberAboutUpdate(updater)` | состояние, тон, кнопка и ошибка для `AboutCard` (`status`, `statusTone`, `action`, `error`) |

**Подключение `anvil-update` — только в сборке `github`**, тем же способом (через `includeBuild` или JitPack), что
и `anvil-compose`: `githubImplementation("io.github.agitangst.anvil:anvil-update")`; `anvil-compose` придёт с ним.
Вместе с модулем в манифест `github` попадают `INTERNET`, `REQUEST_INSTALL_PACKAGES` и приёмник `InstallReceiver`,
в `rustore` и `play` — ничего. Вызовы модуля живут в `src/github/` (раздел 2), на их месте в остальных сборках пусто.

```kotlin
// src/github/…; settings = AnvilSettings(this), как в примере выше
val updater = Updater.shared(this, UpdateConfig(app = "myapp", version = BuildConfig.VERSION_NAME, repo = "owner/myapp"))
updater.auto(settings.checkUpdates) // при запуске, раз за процесс
setContent {
    AnvilTheme(accent = Accents.BLUE, mode = settings.themeMode) {
        Column {
            UpdateBanner(updater) // баннер, ход скачивания, «Что нового»; UpdaterEffects зовёт сам
            UpdatesSection(settings, updater) // «Проверять обновления», «Предлагать пред-выпуски»
            val about = rememberAboutUpdate(updater)
            AboutCard(name = "MyApp", version = BuildConfig.VERSION_NAME, mark = { /* знак */ },
                status = about.status, statusTone = about.tone, action = about.action, error = about.error)
        }
    }
}
```

**Версия набора.** `kitVersion` в [`android/gradle.properties`](../android/gradle.properties) — своя, третья в
репозитории (рядом с `kit-v*` для Rust и `v*` для приложения Anvil, [RELEASES.md](RELEASES.md)). Тег —
`android-kit-vX.Y.Z`; ставит его только владелец, по команде. Номера `kit-v*` и `v*` Android-набор не трогает.
Растёт по общей схеме: этап — вторая цифра, исправление — третья.

**Как программа подключает.** Два способа; выбор — за владельцем на шаге 4 плана (первая программа — Ampulse),
пока не выбран:

| Способ | Как | Заметка |
|---|---|---|
| Подмодуль + `includeBuild` | Anvil — git-подмодуль программы на теге `android-kit-vX.Y.Z`; в `settings.gradle.kts` — `includeBuild("anvil/android")`; в `app` — `implementation("io.github.agitangst.anvil:anvil-compose")` | версия набора видна по подмодулю; у проекта набора те же группа и версия, что у публикации, — Gradle подставляет модуль сам |
| JitPack | зависимость по тегу `android-kit-vX.Y.Z`, подмодуля нет | проект Gradle лежит в `android/`, а не в корне репозитория — сборку на JitPack сначала проверить |

**Сборка и проверка набора.** `JAVA_HOME` и `ANDROID_HOME` — как в разделе 1; из папки `android/`:

```sh
./gradlew :anvil-compose:testDebugUnitTest :anvil-update:testDebugUnitTest :anvil-compose:lintDebug :anvil-update:lintDebug :sample:assembleDebug --max-workers=2
./gradlew --stop
```

Судить по коду выхода, вывод — в файл. Тесты набора (первые три — `anvil-compose`, остальные — `anvil-update`,
идут на JVM без Android):

| Тест | Что проверяет |
|---|---|
| `ContrastTest` | пары «текст на фоне» у каждого акцента в обеих темах — контраст ≥ 4,5:1 (`text` и `weak` на `bg` и `card`, `onAccent` на `accent`, `accentText` на `card` и `bg`) |
| `MotionDocTest` | `Motion.SCALE` и `WAVE_DELAY`, `STAGGER`, `STAGGER_MAX` совпадают с [MOTION.md](MOTION.md): поменял число — поправь и таблицу |
| `SettingsParseTest` | разбор сохранённых темы и языка: незнакомое и пустое — «Как в системе», теги с регионом — по языку |
| `VersionTest` | порядок версий по semver с пред-выпусками: `0.3.1 > 0.3.0`, `1.0.0 > 1.0.0-rc.1`, `beta.2 < beta.10` |
| `ReleasesTest` | выбор выпуска (новее текущей, пред-выпуски по флажку, черновики, выпуск без APK этой программы, ничего новее), строки `SHA256SUMS`, тег из `Location` переадресации `releases/latest`, минуты до сброса лимита по заголовкам |
| `MiniJsonTest` | разбор JSON в том виде, в каком его отдаёт GitHub: объекты, массивы, строки с экранированием, числа, логические; битый ввод — ошибка |

Снимки `sample` (светлая и тёмная × русский и английский) — по разделу 11, когда подключён телефон.
