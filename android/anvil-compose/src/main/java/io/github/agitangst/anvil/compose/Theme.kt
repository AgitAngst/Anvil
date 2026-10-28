package io.github.agitangst.anvil.compose

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat

/** Палитра текущей темы; её же видят виджеты набора. */
val LocalAnvilPalette = staticCompositionLocalOf { AnvilPalette.light() }

/** Доступ к палитре внутри [AnvilTheme]. */
object AnvilTheme {
    val palette: AnvilPalette
        @Composable @ReadOnlyComposable
        get() = LocalAnvilPalette.current
}

/**
 * Тема программы: палитра по ролям семьи, `ColorScheme` Material 3 из неё, полосы системы под тему и флаг
 * «убрать анимацию». Динамический цвет системы не применяется — акцент программы её лицо.
 *
 * @param palette палитра для тёмной (`true`) или светлой темы; по умолчанию — нейтральные цвета набора.
 * @param typography шрифты программы; у набора своих нет.
 */
@Composable
fun AnvilTheme(
    accent: Accent,
    mode: ThemeMode = ThemeMode.System,
    palette: (dark: Boolean) -> AnvilPalette = { AnvilPalette.of(it, accent) },
    typography: Typography = Typography(),
    content: @Composable () -> Unit,
) {
    val dark = when (mode) {
        ThemeMode.System -> isSystemInDarkTheme()
        ThemeMode.Light -> false
        ThemeMode.Dark -> true
    }
    val p = palette(dark)
    SystemBarsEffect(dark)
    CompositionLocalProvider(
        LocalAnvilPalette provides p,
        LocalReducedMotion provides rememberReducedMotion(),
    ) {
        MaterialTheme(colorScheme = p.colorScheme(), typography = typography, content = content)
    }
}

/** `ColorScheme` Material 3 из ролей палитры: стандартные виджеты Material окрашиваются так же, как виджеты набора. */
fun AnvilPalette.colorScheme(): ColorScheme {
    val base = if (dark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = accent,
        onPrimary = onAccent,
        primaryContainer = soft(accent).compositeOver(card),
        onPrimaryContainer = accentText,
        inversePrimary = accentText,
        secondary = accent,
        onSecondary = onAccent,
        secondaryContainer = raised,
        onSecondaryContainer = text,
        tertiary = accent,
        onTertiary = onAccent,
        tertiaryContainer = raised,
        onTertiaryContainer = text,
        background = bg,
        onBackground = text,
        surface = bg,
        onSurface = text,
        surfaceVariant = raised,
        onSurfaceVariant = weak,
        surfaceTint = Color.Transparent,
        inverseSurface = text,
        inverseOnSurface = bg,
        error = danger,
        onError = if (dark) bg else Color.White,
        errorContainer = soft(danger).compositeOver(card),
        onErrorContainer = danger,
        outline = borderStrong,
        outlineVariant = border,
        scrim = Color.Black,
        surfaceBright = card,
        surfaceDim = bg,
        surfaceContainerLowest = field,
        surfaceContainerLow = card,
        surfaceContainer = card,
        surfaceContainerHigh = popup,
        surfaceContainerHighest = raised,
    )
}

/** Значки строки состояния и навигации — под тему программы, даже если она не совпадает с системной. */
@Composable
private fun SystemBarsEffect(dark: Boolean) {
    val view = LocalView.current
    if (view.isInEditMode) return
    SideEffect {
        val window = view.context.findActivity()?.window ?: return@SideEffect
        WindowCompat.getInsetsController(window, view).apply {
            isAppearanceLightStatusBars = !dark
            isAppearanceLightNavigationBars = !dark
        }
    }
}

internal tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
