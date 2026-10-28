package io.github.agitangst.anvil.compose

import android.content.Context
import android.provider.Settings
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleResumeEffect

/**
 * Шкала времени движения, миллисекунды — те же имена и числа, что в таблице docs/MOTION.md («Шкала времени»).
 * Тест `MotionDocTest` сверяет их с документом: поменял число — поправь и таблицу.
 */
object Motion {
    const val PRESS = 90
    const val HOVER = 120
    const val STATE = 180
    const val LEAVE = 120
    const val LAYOUT = 200
    const val ENTER = 240
    const val GRAPH = 300
    const val SHAKE = 420
    const val CHECK = 380
    const val FLASH = 900
    const val EMPHASIS = 520
    const val WAVE = 600
    const val SPIN = 1000
    const val SPIN_SLOW = 1600
    const val GATHER = 1600
    const val BLINK = 1200
    const val SHIMMER = 1500
    const val PING = 1800
    const val DOTS = 1100

    /** Задержка волны после «Подключено». */
    const val WAVE_DELAY = 420

    /** Сдвиг между соседними строками при появлении списком… */
    const val STAGGER = 35

    /** …не дальше стольких строк. */
    const val STAGGER_MAX = 8

    /** Строки таблицы MOTION.md как данные: для витрины и теста. */
    val SCALE: List<Pair<String, Int>> = listOf(
        "PRESS" to PRESS, "HOVER" to HOVER, "STATE" to STATE, "LEAVE" to LEAVE, "LAYOUT" to LAYOUT,
        "ENTER" to ENTER, "GRAPH" to GRAPH, "SHAKE" to SHAKE, "CHECK" to CHECK, "FLASH" to FLASH,
        "EMPHASIS" to EMPHASIS, "WAVE" to WAVE, "SPIN" to SPIN, "SPIN_SLOW" to SPIN_SLOW, "GATHER" to GATHER,
        "BLINK" to BLINK, "SHIMMER" to SHIMMER, "PING" to PING, "DOTS" to DOTS,
    )
}

/** Кривые из docs/MOTION.md («Кривые»). Имя `Easing` занято Compose. */
object AnvilEasing {
    /** Приход: быстро и мягкое торможение — почти всё. */
    val Out: Easing = CubicBezierEasing(0.16f, 1f, 0.3f, 1f)

    /** Уход: разгон и резкий конец. */
    val In: Easing = CubicBezierEasing(0.7f, 0f, 0.84f, 0f)

    /** Туда-обратно внутри цикла: мигание, стягивание. */
    val Standard: Easing = CubicBezierEasing(0.4f, 0f, 0.2f, 1f)

    /** Один акцент с лёгким перелётом (галочка); не для постоянных переходов. */
    val Pop: Easing = CubicBezierEasing(0.34f, 1.56f, 0.64f, 1f)

    /** Вращение, бегущий блик. */
    val Linear: Easing = LinearEasing
}

/** `true` — в системе анимация выключена: переходы набора мгновенные. */
val LocalReducedMotion = staticCompositionLocalOf { false }

/**
 * Анимация выключена в системе: шкала аниматора 0 (настройки разработчика или «Удалить анимацию» в
 * спецвозможностях).
 */
fun isReducedMotion(context: Context): Boolean =
    Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f

/** Флаг «убрать анимацию», перечитывается при каждом возвращении в программу. */
@Composable
fun rememberReducedMotion(): Boolean {
    val context = LocalContext.current
    var reduced by remember { mutableStateOf(isReducedMotion(context)) }
    LifecycleResumeEffect(context) {
        reduced = isReducedMotion(context)
        onPauseOrDispose { }
    }
    return reduced
}

/** Анимация из шкалы: при выключенной в системе анимации — мгновенно. */
@Composable
fun <T> motionSpec(ms: Int, easing: Easing = AnvilEasing.Out): FiniteAnimationSpec<T> =
    if (LocalReducedMotion.current) snap() else tween(durationMillis = ms, easing = easing)
