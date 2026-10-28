package io.github.agitangst.anvil.sample

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.github.agitangst.anvil.compose.Accent
import io.github.agitangst.anvil.compose.Accents
import io.github.agitangst.anvil.compose.AboutCard
import io.github.agitangst.anvil.compose.AnvilCard
import io.github.agitangst.anvil.compose.AnvilPalette
import io.github.agitangst.anvil.compose.AnvilSettings
import io.github.agitangst.anvil.compose.AnvilTheme
import io.github.agitangst.anvil.compose.AppearanceSection
import io.github.agitangst.anvil.compose.LocalReducedMotion
import io.github.agitangst.anvil.compose.Motion
import io.github.agitangst.anvil.compose.SectionTitle
import io.github.agitangst.anvil.compose.Segmented
import io.github.agitangst.anvil.compose.ThemeMode
import io.github.agitangst.anvil.compose.motionSpec
import io.github.agitangst.anvil.update.UpdateBanner
import io.github.agitangst.anvil.update.UpdateConfig
import io.github.agitangst.anvil.update.Updater
import io.github.agitangst.anvil.update.UpdatesSection
import io.github.agitangst.anvil.update.rememberAboutUpdate
import java.util.Locale

/** Адрес исходного кода набора для карточки «О программе». */
private const val CODE_URL = "https://github.com/AgitAngst/Anvil"

/**
 * Что обновляет пример: репозиторий набора. Выпусков для Android в нём нет, поэтому проверка отвечает «Последняя
 * версия»; баннер появится, когда в репозитории будет выпуск с `anvil-sample-X.Y.Z-android.apk`.
 */
fun sampleUpdateConfig() = UpdateConfig(app = "anvil-sample", version = BuildConfig.VERSION_NAME, repo = "AgitAngst/Anvil")

/** Названия акцентов — строками ресурсов; незнакомый акцент показывается своим именем. */
private val accentLabels = mapOf(
    Accents.VIOLET to R.string.accent_violet,
    Accents.AMBER to R.string.accent_amber,
    Accents.TEAL to R.string.accent_teal,
    Accents.ROSE to R.string.accent_rose,
    Accents.BLUE to R.string.accent_blue,
    Accents.AMPULSE to R.string.accent_ampulse,
)

/** Ступени шкалы движения, которые выбирают сегменты: быстро, обычно, медленно (мс). */
private val motionSteps = listOf(Motion.PRESS, Motion.LAYOUT, Motion.GRAPH)

/**
 * Витрина набора: баннер обновления, акценты, палитра текущей темы, «Оформление», «Обновления», «О программе» и
 * движение. Цвета — только из палитры темы, цели нажатия — не меньше 48 dp, отступы системных панелей — через
 * `safeDrawing`.
 */
@Composable
fun SampleScreen(settings: AnvilSettings, updater: Updater, accent: Accent, onAccentChange: (Accent) -> Unit) {
    val p = AnvilTheme.palette
    // Проверка при первом показе экрана: раз за процесс и только если включено «Проверять обновления».
    if (!LocalInspectionMode.current) {
        LaunchedEffect(updater) { updater.auto(settings.checkUpdates) }
    }
    // Surface задаёт цвет содержимого: Text без цвета (например, подписи «Тема» и «Язык») не остаётся чёрным.
    Surface(Modifier.fillMaxSize(), color = p.bg, contentColor = p.text) {
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            // Баннер над содержимым и не прокручивается; скрыт — места не занимает (отступ у самой карточки).
            UpdateBanner(updater, Modifier.padding(start = 16.dp, top = 16.dp, end = 16.dp))
            Column(
                Modifier
                    .weight(1f)
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text(
                    stringResource(R.string.sample_title),
                    Modifier.semantics { heading() },
                    color = p.text,
                    style = MaterialTheme.typography.headlineSmall,
                )
                AccentPicker(accent, onAccentChange)
                PaletteSection()
                AppearanceSection(settings)
                UpdatesSection(settings, updater)
                AboutSection(updater)
                MotionSection()
            }
        }
    }
}

/** Подпись раздела над его содержимым — как у «Оформления» набора. */
@Composable
private fun Section(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionTitle(title, Modifier.padding(horizontal = 4.dp))
        content()
    }
}

@Composable
private fun accentLabel(accent: Accent): String = accentLabels[accent]?.let { stringResource(it) } ?: accent.name

/**
 * Выбор акцента: круг цвета заливки на каждый акцент, у выбранного — рамка цвета текста. Круги стоят в ряд без
 * карточки, чтобы шесть по 48 dp помещались и на экране шириной 320 dp.
 */
@Composable
private fun AccentPicker(selected: Accent, onSelect: (Accent) -> Unit) {
    val p = AnvilTheme.palette
    Section(stringResource(R.string.sample_accent)) {
        Row(Modifier.fillMaxWidth().selectableGroup()) {
            Accents.all.forEach { accent ->
                val label = accentLabel(accent)
                val on = accent == selected
                val ring by animateColorAsState(
                    if (on) p.text else Color.Transparent,
                    motionSpec(Motion.STATE),
                    label = "accent-ring",
                )
                Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                    Box(
                        Modifier
                            .size(48.dp)
                            .clip(CircleShape)
                            .background(if (p.dark) accent.dark.fill else accent.light.fill)
                            .border(3.dp, ring, CircleShape)
                            .selectable(selected = on, role = Role.RadioButton, onClick = { onSelect(accent) })
                            .semantics { contentDescription = label },
                    )
                }
            }
        }
        Text(
            stringResource(R.string.sample_accent_value, accentLabel(selected)),
            Modifier.padding(horizontal = 4.dp),
            color = p.weak,
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}

/** Токены палитры в порядке `docs/STYLE.md`. Имена — идентификаторы из кода, не переводятся. */
private fun AnvilPalette.tokens(): List<Pair<String, Color>> = listOf(
    "bg" to bg,
    "surface" to surface,
    "card" to card,
    "raised" to raised,
    "hover" to hover,
    "border" to border,
    "borderStrong" to borderStrong,
    "field" to field,
    "popup" to popup,
    "text" to text,
    "weak" to weak,
    "faint" to faint,
    "success" to success,
    "warning" to warning,
    "danger" to danger,
    "accent" to accent,
    "onAccent" to onAccent,
    "accentText" to accentText,
)

@Composable
private fun PaletteSection() {
    val p = AnvilTheme.palette
    Section(stringResource(if (p.dark) R.string.sample_palette_dark else R.string.sample_palette_light)) {
        AnvilCard {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                p.tokens().forEach { (name, color) -> TokenRow(name, color) }
            }
        }
    }
}

/** Строка палитры: имя токена, его значение и образец цвета с рамкой (иначе `card` на `card` не видно). */
@Composable
private fun TokenRow(name: String, color: Color) {
    val p = AnvilTheme.palette
    val shape = RoundedCornerShape(8.dp)
    Row(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            name,
            Modifier.weight(1f),
            color = p.text,
            fontFamily = FontFamily.Monospace,
            style = MaterialTheme.typography.bodyMedium,
        )
        Text(
            String.format(Locale.ROOT, "#%06X", color.toArgb() and 0xFFFFFF),
            color = p.weak,
            fontFamily = FontFamily.Monospace,
            style = MaterialTheme.typography.labelSmall,
        )
        Spacer(Modifier.width(12.dp))
        Box(
            Modifier
                .size(width = 56.dp, height = 32.dp)
                .clip(shape)
                .background(color)
                .border(1.dp, p.borderStrong, shape),
        )
    }
}

@Composable
private fun AboutSection(updater: Updater) {
    val update = rememberAboutUpdate(updater)
    Section(stringResource(R.string.sample_about)) {
        AboutCard(
            name = stringResource(R.string.app_name),
            version = BuildConfig.VERSION_NAME,
            mark = { AppMark() },
            build = BuildConfig.BUILD_NUMBER,
            status = update.status,
            statusTone = update.tone,
            action = update.action,
            error = update.error,
            codeUrl = CODE_URL,
        )
    }
}

/** Знак примера: скруглённый квадрат акцента с буквой «A»; для TalkBack — украшение, имя читается рядом. */
@Composable
private fun AppMark() {
    val p = AnvilTheme.palette
    Box(
        Modifier
            .fillMaxSize()
            .clip(RoundedCornerShape(12.dp))
            .background(p.accent)
            .clearAndSetSemantics {},
        contentAlignment = Alignment.Center,
    ) {
        Text("A", color = p.onAccent, style = MaterialTheme.typography.titleLarge)
    }
}

/**
 * Движение: сегменты выбирают ступень шкалы, кнопка разворачивает и сворачивает блок — высота, отступ и цвет
 * идут по [motionSpec]. Рядом сказано, включена ли анимация в системе: выключена — блок меняется мгновенно.
 */
@Composable
private fun MotionSection() {
    val p = AnvilTheme.palette
    var step by rememberSaveable { mutableStateOf(1) }
    var expanded by rememberSaveable { mutableStateOf(false) }
    val ms = motionSteps[step]
    val blockHeight by animateDpAsState(if (expanded) 96.dp else 0.dp, motionSpec(ms), label = "block-height")
    val blockGap by animateDpAsState(if (expanded) 12.dp else 0.dp, motionSpec(ms), label = "block-gap")
    val blockColor by animateColorAsState(
        if (expanded) p.soft(p.accent).compositeOver(p.card) else p.raised,
        motionSpec(ms),
        label = "block-color",
    )
    Section(stringResource(R.string.sample_motion)) {
        AnvilCard {
            Text(stringResource(R.string.sample_motion_time), style = MaterialTheme.typography.titleSmall)
            Segmented(
                options = motionSteps.map { stringResource(R.string.sample_ms, it) },
                selected = step,
                onSelect = { step = it },
            )
            Column {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Button(onClick = { expanded = !expanded }, modifier = Modifier.heightIn(min = 48.dp)) {
                        Text(stringResource(if (expanded) R.string.sample_collapse else R.string.sample_expand))
                    }
                    Text(
                        stringResource(
                            if (LocalReducedMotion.current) R.string.sample_motion_off else R.string.sample_motion_on,
                        ),
                        Modifier.weight(1f),
                        color = p.weak,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
                Box(
                    Modifier
                        .fillMaxWidth()
                        .padding(top = blockGap)
                        .height(blockHeight)
                        .clip(RoundedCornerShape(12.dp))
                        .background(blockColor)
                        .padding(horizontal = 12.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    // Свёрнутый блок пуст и для TalkBack: текст есть, только пока блок виден.
                    if (blockHeight > 0.dp) {
                        Text(
                            stringResource(R.string.sample_motion_block),
                            color = p.text,
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun SamplePreview(mode: ThemeMode) {
    val context = LocalContext.current
    val settings = remember { AnvilSettings(context) }
    val updater = remember { Updater.shared(context, sampleUpdateConfig()) }
    AnvilTheme(Accents.AMPULSE, mode) { SampleScreen(settings, updater, Accents.AMPULSE, onAccentChange = {}) }
}

@Preview(name = "Light", widthDp = 360, heightDp = 1500)
@Composable
private fun SampleLightPreview() {
    SamplePreview(ThemeMode.Light)
}

@Preview(name = "Dark", widthDp = 360, heightDp = 1500)
@Composable
private fun SampleDarkPreview() {
    SamplePreview(ThemeMode.Dark)
}
