package io.github.agitangst.anvil.compose

import androidx.activity.compose.LocalActivity
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/** Карточка семьи: фон `card`, скругление 20, поля 16. */
@Composable
fun AnvilCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(AnvilTheme.palette.card)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        content = content,
    )
}

/** Подпись раздела: прописные, разрядка, слабый цвет; для TalkBack — заголовок. */
@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(),
        modifier.semantics { heading() },
        color = AnvilTheme.palette.weak,
        style = MaterialTheme.typography.labelLarge,
    )
}

/**
 * Сегменты: один из 2–4 коротких вариантов, видимых сразу. Подложка `raised`; выбранный в светлой теме — карточка с
 * тенью, в тёмной — `borderStrong` (карточка там темнее подложки и выглядела бы провалом, как в STYLE.md);
 * высота не меньше 48 dp, для TalkBack — группа переключателей.
 */
@Composable
fun Segmented(
    options: List<String>,
    selected: Int,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val p = AnvilTheme.palette
    Row(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(28.dp))
            .background(p.raised)
            .padding(4.dp)
            .selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        options.forEachIndexed { i, label ->
            val on = i == selected
            val chosen = if (p.dark) p.borderStrong else p.card
            val fill by animateColorAsState(if (on) chosen else Color.Transparent, motionSpec(Motion.STATE), label = "segment")
            Box(
                Modifier
                    .weight(1f)
                    .heightIn(min = 48.dp)
                    .then(if (on && !p.dark) Modifier.shadow(1.dp, RoundedCornerShape(24.dp)) else Modifier)
                    .clip(RoundedCornerShape(24.dp))
                    .background(fill)
                    .selectable(selected = on, role = Role.RadioButton, onClick = { onSelect(i) })
                    .padding(horizontal = 8.dp, vertical = 6.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    label,
                    color = if (on) p.text else p.weak,
                    style = MaterialTheme.typography.labelLarge,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/**
 * Раздел «Оформление»: тема и язык. Применяется сразу; язык пересоздаёт экран (так делает и система).
 * Названия языков — на своём языке, не переводятся.
 */
@Composable
fun AppearanceSection(settings: AnvilSettings, modifier: Modifier = Modifier) {
    val activity = LocalActivity.current
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionTitle(stringResource(R.string.anvil_appearance), Modifier.padding(horizontal = 4.dp))
        AnvilCard {
            Text(stringResource(R.string.anvil_theme), style = MaterialTheme.typography.titleSmall)
            Segmented(
                options = listOf(
                    stringResource(R.string.anvil_as_system),
                    stringResource(R.string.anvil_light),
                    stringResource(R.string.anvil_dark),
                ),
                selected = ThemeMode.entries.indexOf(settings.themeMode),
                onSelect = { settings.themeMode = ThemeMode.entries[it] },
            )
            Text(stringResource(R.string.anvil_language), style = MaterialTheme.typography.titleSmall)
            Segmented(
                options = listOf(stringResource(R.string.anvil_as_system), "Русский", "English"),
                selected = AppLanguage.entries.indexOf(settings.language),
                onSelect = { i -> activity?.let { AnvilLocale.apply(it, AppLanguage.entries[i]) } },
            )
        }
    }
}

/** Одна кнопка карточки «О программе» — по состоянию: «Проверить», «Обновить», «Перезапустить», «Повторить». */
data class AboutAction(val label: String, val onClick: () -> Unit)

/**
 * «О программе» одной строкой: знак слева, имя и под ним состояние, одна кнопка справа; ниже — версия и мелко
 * номер сборки, ошибка понятным текстом и ссылка на код со значком «наружу».
 *
 * @param mark знак программы — квадрат акцента со значком, 48 dp.
 * @param build номер сборки (число коммитов) или `null`.
 */
@Composable
fun AboutCard(
    name: String,
    version: String,
    mark: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    build: Int? = null,
    status: String? = null,
    statusTone: Tone = Tone.Neutral,
    action: AboutAction? = null,
    error: String? = null,
    codeUrl: String? = null,
) {
    val p = AnvilTheme.palette
    AnvilCard(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(48.dp)) { mark() }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(name, style = MaterialTheme.typography.titleMedium, color = p.text)
                if (status != null) {
                    Text(status, style = MaterialTheme.typography.bodyMedium, color = p.tone(statusTone))
                }
            }
            if (action != null) {
                Spacer(Modifier.width(12.dp))
                Button(onClick = action.onClick, modifier = Modifier.heightIn(min = 48.dp)) { Text(action.label) }
            }
        }
        Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.anvil_version, version), style = MaterialTheme.typography.bodyMedium, color = p.weak)
            if (build != null) {
                Text(stringResource(R.string.anvil_build, build), style = MaterialTheme.typography.bodySmall, color = p.faint)
            }
        }
        if (error != null) {
            Text(error, style = MaterialTheme.typography.bodyMedium, color = p.danger)
        }
        if (codeUrl != null) {
            val uri = LocalUriHandler.current
            // Без отступа кнопки: ссылка стоит ровно под строкой версии, цель нажатия — те же 48 dp.
            TextButton(
                onClick = { uri.openUri(codeUrl) },
                modifier = Modifier.heightIn(min = 48.dp),
                contentPadding = PaddingValues(horizontal = 0.dp),
            ) {
                Text(stringResource(R.string.anvil_source_code), color = p.accentText)
                Spacer(Modifier.width(6.dp))
                Icon(AnvilIcons.ExternalLink, contentDescription = stringResource(R.string.anvil_opens_browser), tint = p.accentText, modifier = Modifier.size(18.dp))
            }
        }
    }
}

/** Значки набора: линии 1,8 на сетке 24, как у остальных значков семьи. */
object AnvilIcons {
    /** «Наружу»: ссылка открывает браузер или другую программу. */
    val ExternalLink: ImageVector by lazy {
        stroke(
            "anvil.external_link",
            "M15 3h6v6", "M10 14 21 3", "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
        )
    }

    private fun stroke(name: String, vararg paths: String): ImageVector =
        ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f).apply {
            paths.forEach {
                addPath(
                    pathData = addPathNodes(it),
                    stroke = SolidColor(Color.Black),
                    strokeLineWidth = 1.8f,
                    strokeLineCap = StrokeCap.Round,
                    strokeLineJoin = StrokeJoin.Round,
                )
            }
        }.build()
}
