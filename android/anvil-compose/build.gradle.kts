// Библиотека набора. Kotlin с AGP 9 встроен — отдельного плагина Kotlin для Android нет.
plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.compose)
    `maven-publish`
}

val kitVersion = providers.gradleProperty("kitVersion").get()

android {
    namespace = "io.github.agitangst.anvil.compose"
    // Свежие библиотеки Compose требуют компиляции против API 37.
    compileSdk = 37
    compileSdkMinor = 2

    defaultConfig {
        minSdk = 29
    }

    buildFeatures { compose = true }

    publishing {
        singleVariant("release") { withSourcesJar() }
    }
}

dependencies {
    api(platform(libs.compose.bom))
    api(libs.compose.material3)
    api(libs.compose.ui)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.core.ktx)

    testImplementation(platform(libs.junit.bom))
    testImplementation(libs.junit.jupiter)
    testRuntimeOnly(libs.junit.launcher)
}

tasks.withType<Test> {
    useJUnitPlatform()
    // Тест шкалы движения сверяет её с таблицей docs/MOTION.md.
    systemProperty("anvil.docs", rootProject.file("../docs").absolutePath)
}

// Публикация: группа io.github.agitangst.anvil, версия — kitVersion. Как программы берут набор
// (подмодуль и includeBuild или JitPack по тегу), — docs/ANDROID.md, раздел «Набор».
afterEvaluate {
    publishing {
        publications {
            register<MavenPublication>("release") {
                groupId = "io.github.agitangst.anvil"
                artifactId = "anvil-compose"
                version = kitVersion
                from(components["release"])
            }
        }
    }
}
