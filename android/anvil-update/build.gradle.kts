// Обновление Android-программы из GitHub Releases. Программа подключает модуль только в сборке `github`
// (`githubImplementation`): вместе с ним приходят разрешения на сеть и установку.
plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.compose)
    `maven-publish`
}

val kitVersion = providers.gradleProperty("kitVersion").get()

// Те же группа и версия, что у публикации: includeBuild подставляет модуль сам.
group = "io.github.agitangst.anvil"
version = kitVersion

android {
    namespace = "io.github.agitangst.anvil.update"
    compileSdk = 37
    compileSdkMinor = 2

    defaultConfig {
        minSdk = 29
        buildConfigField("String", "KIT_VERSION", "\"$kitVersion\"")
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    publishing {
        singleVariant("release") { withSourcesJar() }
    }
}

dependencies {
    api(project(":anvil-compose"))
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.core.ktx)

    testImplementation(platform(libs.junit.bom))
    testImplementation(libs.junit.jupiter)
    testRuntimeOnly(libs.junit.launcher)
}

tasks.withType<Test> {
    useJUnitPlatform()
}

afterEvaluate {
    publishing {
        publications {
            register<MavenPublication>("release") {
                groupId = "io.github.agitangst.anvil"
                artifactId = "anvil-update"
                version = kitVersion
                from(components["release"])
            }
        }
    }
}
