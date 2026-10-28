// Пример-приложение набора: витрина для снимков обеих тем и языков. Kotlin с AGP 9 встроен —
// отдельного плагина Kotlin для Android нет.
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

val kitVersion = providers.gradleProperty("kitVersion").get()

/**
 * versionCode по docs/ANDROID.md, раздел 3: (X*10000 + Y*100 + Z)*100 + K, где K = 99 у выпуска и N у
 * `-alpha.N`, `-beta.N`, `-rc.N` (1–98): 0.1.0 -> 10099, 0.1.0-beta.3 -> 10003.
 */
fun versionCodeOf(version: String): Int {
    val m = Regex("""(\d+)\.(\d+)\.(\d+)(?:-(?:alpha|beta|rc)\.(\d+))?""").matchEntire(version)
        ?: error("kitVersion must be X.Y.Z or X.Y.Z-beta.N, got: $version")
    val (x, y, z) = (1..3).map { m.groupValues[it].toInt() }
    val pre = m.groupValues[4]
    val k = if (pre.isEmpty()) 99 else pre.toInt().also { require(it in 1..98) { "pre-release number must be 1..98: $version" } }
    return (x * 10000 + y * 100 + z) * 100 + k
}

// Номер сборки — число коммитов; нет git или истории — 0.
val buildNumber: Int = runCatching {
    providers.exec {
        commandLine("git", "rev-list", "--count", "HEAD")
        isIgnoreExitValue = true
    }.standardOutput.asText.get().trim().toInt()
}.getOrDefault(0)

android {
    namespace = "io.github.agitangst.anvil.sample"
    // Свежие библиотеки Compose требуют компиляции против API 37.
    compileSdk = 37
    compileSdkMinor = 2

    defaultConfig {
        applicationId = "io.github.agitangst.anvil.sample"
        minSdk = 29
        targetSdk = 37
        versionCode = versionCodeOf(kitVersion)
        versionName = kitVersion
        buildConfigField("int", "BUILD_NUMBER", buildNumber.toString())
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

dependencies {
    implementation(project(":anvil-compose"))
    implementation(project(":anvil-update"))
    implementation(platform(libs.compose.bom))
    implementation(libs.activity.compose)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
}
