package io.github.agitangst.anvil.compose

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color

/**
 * Палитра по ролям семьи — те же токены, что в docs/STYLE.md (2.1). Цвета в коде числами не пишутся:
 * только `AnvilTheme.palette` и её поля. Программа может заменить нейтральные цвета своими через `copy()`.
 */
@Immutable
data class AnvilPalette(
    val dark: Boolean,
    /** Фон экрана. */
    val bg: Color,
    /** Шапка, нижняя навигация. */
    val surface: Color,
    /** Карточки, диалоги, листы. */
    val card: Color,
    /** Кнопки, дорожки, подложки значков. */
    val raised: Color,
    /** Подсветка нажатого. */
    val hover: Color,
    /** Рамки карточек, разделители. */
    val border: Color,
    /** Рамка выбранного, разделитель на всплывающем. */
    val borderStrong: Color,
    /** Фон полей ввода. */
    val field: Color,
    /** Всплывающий слой: меню, выпадающий список. */
    val popup: Color,
    /** Основной текст. */
    val text: Color,
    /** Подписи и пояснения. */
    val weak: Color,
    /** Подсказки в полях и недоступное — не для важного текста. */
    val faint: Color,
    val success: Color,
    val warning: Color,
    val danger: Color,
    /** Заливка главной кнопки, включённого переключателя, полосы хода. */
    val accent: Color,
    /** Текст и значки на заливке акцента. */
    val onAccent: Color,
    /** Акцентный текст на обычном фоне: ссылки, выбранное. */
    val accentText: Color,
) {
    /** Мягкая подложка: цвет с прозрачностью 16 % в тёмной теме и 12 % в светлой. */
    fun soft(color: Color): Color = color.copy(alpha = if (dark) 0.16f else 0.12f)

    /** Цвет смыслового тона: текст рядом с ним обязателен — цвет не единственный носитель смысла. */
    fun tone(tone: Tone): Color = when (tone) {
        Tone.Neutral -> weak
        Tone.Accent -> accentText
        Tone.Success -> success
        Tone.Warning -> warning
        Tone.Danger -> danger
    }

    companion object {
        fun light(accent: Accent = Accents.BLUE): AnvilPalette = AnvilPalette(
            dark = false,
            bg = Color(0xFFF3F3F4),
            surface = Color(0xFFFAFAFA),
            card = Color(0xFFFFFFFF),
            raised = Color(0xFFF0F0F2),
            hover = Color(0xFFE8E8EB),
            border = Color(0xFFE2E2E5),
            borderStrong = Color(0xFFCBCBD0),
            field = Color(0xFFFFFFFF),
            popup = Color(0xFFF0F0F2),
            text = Color(0xFF17171A),
            weak = Color(0xFF55555D),
            faint = Color(0xFF8A8A92),
            success = Color(0xFF16703F),
            warning = Color(0xFF8F5F00),
            danger = Color(0xFFC4303A),
            accent = accent.light.fill,
            onAccent = accent.light.onFill,
            accentText = accent.light.text,
        )

        fun dark(accent: Accent = Accents.BLUE): AnvilPalette = AnvilPalette(
            dark = true,
            bg = Color(0xFF0F0F11),
            surface = Color(0xFF161618),
            card = Color(0xFF1C1C1F),
            raised = Color(0xFF252529),
            hover = Color(0xFF2B2B30),
            border = Color(0xFF2C2C31),
            borderStrong = Color(0xFF3D3D44),
            field = Color(0xFF121214),
            popup = Color(0xFF252529),
            text = Color(0xFFECECEE),
            weak = Color(0xFFA0A0A8),
            faint = Color(0xFF6E6E76),
            success = Color(0xFF4CC38A),
            warning = Color(0xFFE5B54A),
            danger = Color(0xFFF2676B),
            accent = accent.dark.fill,
            onAccent = accent.dark.onFill,
            accentText = accent.dark.text,
        )

        fun of(dark: Boolean, accent: Accent): AnvilPalette = if (dark) dark(accent) else light(accent)
    }
}

/** Смысловой тон строки состояния, метки, значка. */
enum class Tone { Neutral, Accent, Success, Warning, Danger }

/** Три значения акцента в одной теме: заливка, текст на ней, акцентный текст на обычном фоне. */
@Immutable
data class AccentShades(val fill: Color, val onFill: Color, val text: Color)

/** Акцент программы — для светлой и тёмной темы. */
@Immutable
data class Accent(val name: String, val light: AccentShades, val dark: AccentShades)

/**
 * Акценты семьи: те же, что в таблице docs/STYLE.md (2.2), и акценты Android-программ из docs/ANDROID.md.
 * Новая программа сначала берёт BLUE; свой акцент — по решению владельца, строкой здесь и в документах.
 */
object Accents {
    val VIOLET = Accent(
        "violet",
        light = AccentShades(Color(0xFF7439D9), Color(0xFFFFFFFF), Color(0xFF6229C4)),
        dark = AccentShades(Color(0xFFAB7CF6), Color(0xFF150A31), Color(0xFFBF9FFB)),
    )
    val AMBER = Accent(
        "amber",
        light = AccentShades(Color(0xFFF2B54A), Color(0xFF1C1403), Color(0xFF8A5A00)),
        dark = AccentShades(Color(0xFFF2B54A), Color(0xFF1C1403), Color(0xFFF2B54A)),
    )
    val TEAL = Accent(
        "teal",
        light = AccentShades(Color(0xFF0E7C70), Color(0xFFFFFFFF), Color(0xFF0B6E63)),
        dark = AccentShades(Color(0xFF3CC8B4), Color(0xFF04201C), Color(0xFF4FD6C3)),
    )
    val ROSE = Accent(
        "rose",
        light = AccentShades(Color(0xFFC22D48), Color(0xFFFFFFFF), Color(0xFFB3263F)),
        dark = AccentShades(Color(0xFFF0647A), Color(0xFF22060B), Color(0xFFF4808F)),
    )
    val BLUE = Accent(
        "blue",
        light = AccentShades(Color(0xFF2A5FD0), Color(0xFFFFFFFF), Color(0xFF2556C0)),
        dark = AccentShades(Color(0xFF6C9EF8), Color(0xFF06142E), Color(0xFF86B0FA)),
    )

    /** Ampulse: бирюза рядом с TEAL — решение владельца 28.10.2026. */
    val AMPULSE = Accent(
        "ampulse",
        light = AccentShades(Color(0xFF0B6B63), Color(0xFFFFFFFF), Color(0xFF0B5A53)),
        dark = AccentShades(Color(0xFF4FBFB0), Color(0xFF1A2220), Color(0xFF7ED8CB)),
    )

    val all: List<Accent> = listOf(VIOLET, AMBER, TEAL, ROSE, BLUE, AMPULSE)
}
