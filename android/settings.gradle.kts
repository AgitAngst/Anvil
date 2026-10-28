// Набор Anvil для Android-программ семьи: библиотека и пример-приложение для проверки на телефоне.
// Правила — docs/ANDROID.md в корне репозитория.
rootProject.name = "anvil-android"

pluginManagement {
    repositories {
        google()
        gradlePluginPortal()
        mavenCentral()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

// Тема, выбор темы и языка, «Оформление», «О программе», шкала движения.
include(":anvil-compose")

// Обновление из GitHub Releases — только для сборки `github` (разрешения на сеть и установку).
include(":anvil-update")

// Витрина набора: снимки обеих тем и языков.
include(":sample")
