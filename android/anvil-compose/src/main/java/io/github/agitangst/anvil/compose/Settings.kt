package io.github.agitangst.anvil.compose

import android.app.Activity
import android.app.LocaleManager
import android.content.Context
import android.content.res.Configuration
import android.os.Build
import android.os.LocaleList
import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.edit
import java.util.Locale

/** Тема: «Как в системе», «Светлая», «Тёмная». */
enum class ThemeMode(val key: String) {
    System("system"),
    Light("light"),
    Dark("dark");

    companion object {
        fun parse(key: String?): ThemeMode = entries.firstOrNull { it.key == key } ?: System
    }
}

/** Язык программы: «Как в системе», «Русский», «English». */
enum class AppLanguage(val tag: String) {
    System(""),
    Russian("ru"),
    English("en");

    companion object {
        /** По тегу языка (`ru`, `ru-RU`, `en-US`); незнакомый — «Как в системе». */
        fun parse(tag: String?): AppLanguage {
            val lang = tag.orEmpty().substringBefore('-').substringBefore('_').lowercase()
            return entries.firstOrNull { it != System && it.tag == lang } ?: System
        }
    }
}

private const val PREFS = "anvil"
private const val KEY_THEME = "theme"
private const val KEY_LANGUAGE = "language"
private const val KEY_UPDATES = "check_updates"

private fun Context.anvilPrefs() = getSharedPreferences(PREFS, Context.MODE_PRIVATE)

/**
 * Общие настройки программы: тема, язык, «проверять обновления» (по умолчанию включено). Хранятся на телефоне
 * и применяются сразу; тема — наблюдаемое состояние Compose.
 */
class AnvilSettings(context: Context) {
    private val app = context.applicationContext
    private val prefs = app.anvilPrefs()

    private var theme by mutableStateOf(ThemeMode.parse(prefs.getString(KEY_THEME, null)))
    private var updates by mutableStateOf(prefs.getBoolean(KEY_UPDATES, true))

    var themeMode: ThemeMode
        get() = theme
        set(value) {
            theme = value
            prefs.edit { putString(KEY_THEME, value.key) }
        }

    var checkUpdates: Boolean
        get() = updates
        set(value) {
            updates = value
            prefs.edit { putBoolean(KEY_UPDATES, value) }
        }

    /** Выбранный язык; меняется через [AnvilLocale.apply] — экран после этого пересоздаётся. */
    val language: AppLanguage get() = AnvilLocale.current(app)
}

/**
 * Язык внутри программы. Android 13+ — системный механизм (`LocaleManager`): система хранит выбор, показывает его
 * в своих настройках (у программы — `android:localeConfig`) и пересоздаёт экран сама. Ниже — выбор в настройках
 * набора и подмена языка контекста в [AnvilActivity].
 */
object AnvilLocale {
    fun current(context: Context): AppLanguage =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val locales = context.getSystemService(LocaleManager::class.java).applicationLocales
            if (locales.isEmpty) AppLanguage.System else AppLanguage.parse(locales[0].toLanguageTag())
        } else {
            AppLanguage.parse(context.anvilPrefs().getString(KEY_LANGUAGE, null))
        }

    fun apply(activity: Activity, language: AppLanguage) {
        if (current(activity) == language) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            activity.getSystemService(LocaleManager::class.java).applicationLocales =
                if (language == AppLanguage.System) LocaleList.getEmptyLocaleList() else LocaleList.forLanguageTags(language.tag)
        } else {
            activity.anvilPrefs().edit { putString(KEY_LANGUAGE, language.tag) }
            activity.recreate()
        }
    }

    /** Контекст с выбранным языком — для `attachBaseContext` ниже Android 13. */
    fun wrap(base: Context): Context {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) return base
        val language = AppLanguage.parse(base.anvilPrefs().getString(KEY_LANGUAGE, null))
        if (language == AppLanguage.System) return base
        val config = Configuration(base.resources.configuration)
        config.setLocales(LocaleList(Locale.forLanguageTag(language.tag)))
        return base.createConfigurationContext(config)
    }
}

/** Основа экрана программы: язык из настроек набора на Android ниже 13. */
open class AnvilActivity : ComponentActivity() {
    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AnvilLocale.wrap(newBase))
    }
}
