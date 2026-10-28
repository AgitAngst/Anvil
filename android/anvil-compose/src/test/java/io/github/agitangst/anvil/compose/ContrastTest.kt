package io.github.agitangst.anvil.compose

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import kotlin.math.pow

/** Пары «текст на фоне» палитр набора и акцентов семьи — не ниже 4,5:1 в обеих темах (общий гайдлайн, §10). */
class ContrastTest {
    private fun channel(c: Int): Double {
        val v = c / 255.0
        return if (v <= 0.03928) v / 12.92 else ((v + 0.055) / 1.055).pow(2.4)
    }

    private fun luminance(color: Color): Double {
        val argb = color.toArgb()
        return 0.2126 * channel(argb shr 16 and 0xFF) + 0.7152 * channel(argb shr 8 and 0xFF) + 0.0722 * channel(argb and 0xFF)
    }

    private fun contrast(a: Color, b: Color): Double {
        val (hi, lo) = listOf(luminance(a), luminance(b)).sortedDescending()
        return (hi + 0.05) / (lo + 0.05)
    }

    @Test
    fun textPairsAreReadable() {
        val problems = mutableListOf<String>()
        for (accent in Accents.all) {
            for (p in listOf(AnvilPalette.light(accent), AnvilPalette.dark(accent))) {
                val theme = if (p.dark) "dark" else "light"
                val pairs = listOf(
                    "text/bg" to (p.text to p.bg),
                    "text/card" to (p.text to p.card),
                    "weak/bg" to (p.weak to p.bg),
                    "weak/card" to (p.weak to p.card),
                    "onAccent/accent" to (p.onAccent to p.accent),
                    "accentText/card" to (p.accentText to p.card),
                    "accentText/bg" to (p.accentText to p.bg),
                )
                for ((name, pair) in pairs) {
                    val ratio = contrast(pair.first, pair.second)
                    if (ratio < 4.5) problems += "${accent.name} $theme $name: ${"%.2f".format(ratio)}"
                }
            }
        }
        assertTrue(problems.isEmpty(), "contrast below 4.5: $problems")
    }
}
