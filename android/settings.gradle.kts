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

// Витрина набора: снимки обеих тем и языков.
include(":sample")
