package io.github.agitangst.anvil.sample

import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import io.github.agitangst.anvil.compose.Accents
import io.github.agitangst.anvil.compose.AnvilActivity
import io.github.agitangst.anvil.compose.AnvilSettings
import io.github.agitangst.anvil.compose.AnvilTheme

/** Единственный экран примера: тема набора с выбранным акцентом и витрина его частей. */
class MainActivity : AnvilActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val settings = AnvilSettings(this)
        setContent {
            // Акцент хранится по имени: так он переживает пересоздание экрана (смену языка или темы).
            var accentName by rememberSaveable { mutableStateOf(Accents.AMPULSE.name) }
            val accent = Accents.all.firstOrNull { it.name == accentName } ?: Accents.AMPULSE
            AnvilTheme(accent, settings.themeMode) {
                SampleScreen(settings, accent, onAccentChange = { accentName = it.name })
            }
        }
    }
}
