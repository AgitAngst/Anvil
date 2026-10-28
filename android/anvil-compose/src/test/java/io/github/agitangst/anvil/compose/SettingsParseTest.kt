package io.github.agitangst.anvil.compose

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

/** Разбор сохранённых значений: незнакомое и пустое — «Как в системе», теги с регионом — по языку. */
class SettingsParseTest {
    @Test
    fun themeMode() {
        assertEquals(ThemeMode.Dark, ThemeMode.parse("dark"))
        assertEquals(ThemeMode.Light, ThemeMode.parse("light"))
        assertEquals(ThemeMode.System, ThemeMode.parse(null))
        assertEquals(ThemeMode.System, ThemeMode.parse("sepia"))
        ThemeMode.entries.forEach { assertEquals(it, ThemeMode.parse(it.key)) }
    }

    @Test
    fun language() {
        assertEquals(AppLanguage.Russian, AppLanguage.parse("ru"))
        assertEquals(AppLanguage.Russian, AppLanguage.parse("ru-RU"))
        assertEquals(AppLanguage.English, AppLanguage.parse("en_US"))
        assertEquals(AppLanguage.English, AppLanguage.parse("EN"))
        assertEquals(AppLanguage.System, AppLanguage.parse("de"))
        assertEquals(AppLanguage.System, AppLanguage.parse(""))
        assertEquals(AppLanguage.System, AppLanguage.parse(null))
        AppLanguage.entries.forEach { assertEquals(it, AppLanguage.parse(it.tag)) }
    }
}
