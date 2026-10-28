package io.github.agitangst.anvil.update

import android.app.Activity
import android.content.Context
import android.content.Intent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.agitangst.anvil.compose.AboutAction
import io.github.agitangst.anvil.compose.AnvilCard
import io.github.agitangst.anvil.compose.AnvilEasing
import io.github.agitangst.anvil.compose.AnvilIcons
import io.github.agitangst.anvil.compose.AnvilSettings
import io.github.agitangst.anvil.compose.AnvilTheme
import io.github.agitangst.anvil.compose.LocalReducedMotion
import io.github.agitangst.anvil.compose.Motion
import io.github.agitangst.anvil.compose.SectionTitle
import io.github.agitangst.anvil.compose.Tone
import io.github.agitangst.anvil.compose.motionSpec

/** Мегабайт в байтах — размер файла показывается в МБ. */
private const val MB = 1024.0 * 1024.0

/**
 * Баннер обновления над содержимым экрана — окон при запуске нет. Сам зовёт [UpdaterEffects]. Виден, пока
 * [Updater.showBanner]; тон и слово в заголовке по состоянию: есть версия — акцент, нужно разрешение — внимание,
 * готово — успех, не удалось — опасность. Главная кнопка одна, остальные тише.
 *
 * @param modifier у самой карточки: отступы вокруг неё сворачиваются вместе с баннером. В колонке с промежутками
 * (`spacedBy`) скрытый баннер всё равно занимает место под промежуток — ставь его вне такой колонки.
 */
@Composable
fun UpdateBanner(updater: Updater, modifier: Modifier = Modifier) {
    UpdaterEffects(updater)
    val last = remember { LastShown() }
    if (updater.showBanner) last.state = updater.state
    var notesOpen by rememberSaveable { mutableStateOf(false) }
    val available = (updater.state as? UpdateState.Available)?.update

    AnimatedVisibility(
        visible = updater.showBanner,
        enter = fadeIn(motionSpec(Motion.ENTER)) + expandVertically(motionSpec(Motion.ENTER)),
        exit = fadeOut(motionSpec(Motion.LEAVE, AnvilEasing.In)) +
            shrinkVertically(motionSpec(Motion.LEAVE, AnvilEasing.In)),
    ) {
        last.state?.let { BannerCard(it, updater, modifier, onNotes = { notesOpen = true }) }
    }

    if (notesOpen && available != null) {
        NotesDialog(
            update = available,
            onDismiss = { notesOpen = false },
            onUpdate = {
                notesOpen = false
                updater.update()
            },
        )
    }
}

/** Последнее показанное состояние: уходя, баннер сжимается с прежним текстом, а не пустой карточкой. */
private class LastShown {
    var state: UpdateState? = null
}

@Composable
private fun BannerCard(state: UpdateState, updater: Updater, modifier: Modifier, onNotes: () -> Unit) {
    val p = AnvilTheme.palette
    val context = LocalContext.current
    val uri = LocalUriHandler.current
    when (state) {
        is UpdateState.Available -> {
            val u = state.update
            val hasNotes = u.notes.isNotBlank()
            BannerFrame(Tone.Accent, stringResource(R.string.anvil_update_available, u.version.toString()), modifier) {
                if (u.apk.size > 0) {
                    Text(
                        stringResource(R.string.anvil_update_size, u.apk.size / MB),
                        color = p.weak,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
                ButtonRow(alignEnd = true) {
                    PlainButton(stringResource(R.string.anvil_update_action_later)) { updater.later() }
                    MainButton(stringResource(R.string.anvil_update_action_update)) { updater.update() }
                }
                ButtonRow(alignEnd = false) {
                    // Заметок нет (выпуск найден без API) — «Что нового» открывает страницу выпуска в браузере.
                    if (hasNotes || u.page.isNotBlank()) {
                        QuietButton(stringResource(R.string.anvil_update_action_notes), outside = !hasNotes) {
                            if (hasNotes) onNotes() else runCatching { uri.openUri(u.page) }
                        }
                    }
                    QuietButton(stringResource(R.string.anvil_update_action_skip)) { updater.skip() }
                }
            }
        }

        is UpdateState.Downloading -> {
            val percent = percent(state.done, state.total)
            BannerFrame(
                Tone.Accent,
                stringResource(R.string.anvil_update_downloading, state.update.version.toString()),
                modifier,
            ) {
                Progress(percent?.let { it / 100f })
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        if (percent != null) {
                            stringResource(R.string.anvil_update_percent, percent)
                        } else {
                            stringResource(R.string.anvil_update_downloaded, state.done / MB)
                        },
                        Modifier.weight(1f),
                        color = p.weak,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    QuietButton(stringResource(R.string.anvil_update_action_cancel)) { updater.cancel() }
                }
            }
        }

        is UpdateState.NeedsPermission -> BannerFrame(
            Tone.Warning,
            stringResource(R.string.anvil_update_permission_title),
            modifier,
        ) {
            Text(
                stringResource(R.string.anvil_update_permission_body),
                color = p.text,
                style = MaterialTheme.typography.bodyMedium,
            )
            ButtonRow(alignEnd = true) {
                PlainButton(stringResource(R.string.anvil_update_action_later)) { updater.later() }
                MainButton(stringResource(R.string.anvil_update_action_allow)) { context.openInstallSettings(updater) }
            }
        }

        is UpdateState.Confirm -> InstallingBanner(state.update, modifier)
        is UpdateState.Installing -> InstallingBanner(state.update, modifier)

        is UpdateState.Installed -> BannerFrame(
            Tone.Success,
            stringResource(R.string.anvil_update_installed, state.version),
            modifier,
        ) {
            ButtonRow(alignEnd = true) {
                MainButton(stringResource(R.string.anvil_update_action_got_it)) { updater.later() }
            }
        }

        is UpdateState.Failed -> if (state.update != null) {
            BannerFrame(Tone.Danger, stringResource(R.string.anvil_update_failed), modifier) {
                Text(state.error.describe(context), color = p.text, style = MaterialTheme.typography.bodyMedium)
                ButtonRow(alignEnd = true) {
                    PlainButton(stringResource(R.string.anvil_update_action_later)) { updater.later() }
                    MainButton(stringResource(R.string.anvil_update_action_retry)) { updater.update() }
                }
            }
        }

        UpdateState.Idle, UpdateState.Checking, UpdateState.UpToDate -> Unit
    }
}

/** «Устанавливается…»: окно подтверждения открывает система, баннер только говорит об этом. */
@Composable
private fun InstallingBanner(update: Update, modifier: Modifier) {
    val p = AnvilTheme.palette
    BannerFrame(Tone.Accent, stringResource(R.string.anvil_update_installing, update.version.toString()), modifier) {
        Text(
            stringResource(R.string.anvil_update_installing_hint),
            color = p.weak,
            style = MaterialTheme.typography.bodyMedium,
        )
        Progress(null)
    }
}

/**
 * Карточка баннера: подложка тона (`soft` поверх `card`), слово-заголовок цвета тона. Заголовок объявляется
 * TalkBack при смене состояния; смысл несёт слово, цвет только усиливает.
 */
@Composable
private fun BannerFrame(tone: Tone, title: String, modifier: Modifier, content: @Composable ColumnScope.() -> Unit) {
    val p = AnvilTheme.palette
    val base = when (tone) {
        Tone.Neutral, Tone.Accent -> p.accent
        Tone.Success -> p.success
        Tone.Warning -> p.warning
        Tone.Danger -> p.danger
    }
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(p.soft(base).compositeOver(p.card))
            .animateContentSize(motionSpec(Motion.LAYOUT))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            title,
            Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            color = p.tone(tone),
            style = MaterialTheme.typography.titleMedium,
        )
        content()
    }
}

/**
 * Полоса хода: с долей — плавно по [Motion.STATE]; без доли — бегущая, а при выключенной в системе анимации
 * её нет вовсе (ход виден по числу рядом).
 */
@Composable
private fun Progress(fraction: Float?) {
    val p = AnvilTheme.palette
    if (fraction != null) {
        val shown by animateFloatAsState(fraction, motionSpec(Motion.STATE), label = "update-progress")
        LinearProgressIndicator(
            progress = { shown },
            modifier = Modifier.fillMaxWidth(),
            color = p.accent,
            trackColor = p.raised,
        )
    } else if (!LocalReducedMotion.current) {
        LinearProgressIndicator(modifier = Modifier.fillMaxWidth(), color = p.accent, trackColor = p.raised)
    }
}

/** Доля скачанного в процентах; `null`, если размер неизвестен. */
private fun percent(done: Long, total: Long): Int? =
    if (total > 0) (done * 100 / total).toInt().coerceIn(0, 100) else null

/** Ряд кнопок: переносится на узком экране; главная — справа, дополнительные и тихие — слева. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ButtonRow(alignEnd: Boolean, content: @Composable () -> Unit) {
    FlowRow(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp, if (alignEnd) Alignment.End else Alignment.Start),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) { content() }
}

/** Главная кнопка: залита акцентом, одна на баннер. */
@Composable
private fun MainButton(text: String, onClick: () -> Unit) {
    Button(onClick, Modifier.heightIn(min = 48.dp)) { Text(text) }
}

/** Обычная кнопка: подложка `raised`, как у кнопок семьи. */
@Composable
private fun PlainButton(text: String, onClick: () -> Unit) {
    FilledTonalButton(onClick, Modifier.heightIn(min = 48.dp)) { Text(text) }
}

/** Тихая кнопка: только текст акцента; [outside] — открывает браузер, значок «наружу». */
@Composable
private fun QuietButton(text: String, outside: Boolean = false, onClick: () -> Unit) {
    val p = AnvilTheme.palette
    TextButton(onClick, Modifier.heightIn(min = 48.dp)) {
        Text(text, color = p.accentText)
        if (outside) {
            Spacer(Modifier.width(6.dp))
            Icon(
                AnvilIcons.ExternalLink,
                stringResource(R.string.anvil_update_opens_browser),
                Modifier.size(18.dp),
                tint = p.accentText,
            )
        }
    }
}

/** Открыть настройку «Установка неизвестных приложений»; вернувшись, [UpdaterEffects] продолжит установку. */
private fun Context.openInstallSettings(updater: Updater) {
    val intent = updater.installPermissionIntent()
    if (this !is Activity) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    runCatching { startActivity(intent) }
}

/** «Что нового»: заметки выпуска прокручиваемым текстом; «Обновить» — главная, «Закрыть» — тихая. */
@Composable
private fun NotesDialog(update: Update, onDismiss: () -> Unit, onUpdate: () -> Unit) {
    val p = AnvilTheme.palette
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = { MainButton(stringResource(R.string.anvil_update_action_update), onUpdate) },
        dismissButton = { QuietButton(stringResource(R.string.anvil_update_action_close), onClick = onDismiss) },
        title = {
            Text(
                stringResource(R.string.anvil_update_notes_title, update.version.toString()),
                Modifier.semantics { heading() },
            )
        },
        text = {
            Text(
                update.notes,
                Modifier.verticalScroll(rememberScrollState()),
                style = MaterialTheme.typography.bodyMedium,
            )
        },
        containerColor = p.popup,
        titleContentColor = p.text,
        textContentColor = p.text,
    )
}

/**
 * Раздел настроек «Обновления» (сборка `github`): «Проверять обновления» с пояснением про сеть и «Предлагать
 * пред-выпуски». Выключатели применяются сразу; нажимается вся строка.
 */
@Composable
fun UpdatesSection(settings: AnvilSettings, updater: Updater, modifier: Modifier = Modifier) {
    val p = AnvilTheme.palette
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionTitle(stringResource(R.string.anvil_update_section), Modifier.padding(horizontal = 4.dp))
        AnvilCard {
            SwitchRow(
                title = stringResource(R.string.anvil_update_auto),
                hint = stringResource(R.string.anvil_update_auto_hint),
                checked = settings.checkUpdates,
                onChange = { settings.checkUpdates = it },
            )
            HorizontalDivider(color = p.border)
            SwitchRow(
                title = stringResource(R.string.anvil_update_pre),
                hint = stringResource(R.string.anvil_update_pre_hint),
                checked = updater.offerPrereleases,
                onChange = { updater.offerPrereleases = it },
            )
        }
    }
}

/** Строка-выключатель: подпись и пояснение слева, тумблер справа; вся строка — один переключатель для TalkBack. */
@Composable
private fun SwitchRow(title: String, hint: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    val p = AnvilTheme.palette
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clip(RoundedCornerShape(12.dp))
            .toggleable(value = checked, role = Role.Switch, onValueChange = onChange)
            .padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, color = p.text, style = MaterialTheme.typography.bodyLarge)
            Text(hint, color = p.weak, style = MaterialTheme.typography.bodyMedium)
        }
        // Нажатие обрабатывает строка, поэтому сам тумблер без обработчика.
        Switch(checked = checked, onCheckedChange = null)
    }
}

/**
 * Состояние обновления для `AboutCard`: строка состояния [status] с тоном [tone], одна кнопка [action] по
 * состоянию (или нет) и понятный текст ошибки [error].
 */
data class AboutUpdate(val status: String?, val tone: Tone, val action: AboutAction?, val error: String?)

/**
 * Собрать для `AboutCard` состояние и кнопку обновления: «Проверить», «Обновить», «Разрешить», «Повторить».
 * Окно подтверждения установки открывает [UpdaterEffects] — его зовёт [UpdateBanner], поставь баннер на тот же экран.
 */
@Composable
fun rememberAboutUpdate(updater: Updater): AboutUpdate {
    val context = LocalContext.current
    return when (val s = updater.state) {
        UpdateState.Idle -> AboutUpdate(
            stringResource(R.string.anvil_update_about_idle),
            Tone.Neutral,
            AboutAction(stringResource(R.string.anvil_update_action_check)) { updater.check() },
            null,
        )

        UpdateState.Checking -> AboutUpdate(stringResource(R.string.anvil_update_about_checking), Tone.Neutral, null, null)

        UpdateState.UpToDate -> AboutUpdate(
            stringResource(R.string.anvil_update_about_current),
            Tone.Success,
            AboutAction(stringResource(R.string.anvil_update_action_check)) { updater.check() },
            null,
        )

        is UpdateState.Available -> AboutUpdate(
            stringResource(R.string.anvil_update_available, s.update.version.toString()),
            Tone.Accent,
            AboutAction(stringResource(R.string.anvil_update_action_update)) { updater.update() },
            null,
        )

        is UpdateState.Downloading -> {
            val percent = percent(s.done, s.total)
            AboutUpdate(
                if (percent != null) {
                    stringResource(R.string.anvil_update_about_downloading, percent)
                } else {
                    stringResource(R.string.anvil_update_about_downloading_unknown)
                },
                Tone.Accent,
                null,
                null,
            )
        }

        is UpdateState.NeedsPermission -> AboutUpdate(
            stringResource(R.string.anvil_update_permission_title),
            Tone.Warning,
            AboutAction(stringResource(R.string.anvil_update_action_allow)) { context.openInstallSettings(updater) },
            null,
        )

        is UpdateState.Confirm, is UpdateState.Installing ->
            AboutUpdate(stringResource(R.string.anvil_update_about_installing), Tone.Accent, null, null)

        is UpdateState.Installed -> AboutUpdate(
            stringResource(R.string.anvil_update_about_installed),
            Tone.Success,
            AboutAction(stringResource(R.string.anvil_update_action_check)) { updater.check() },
            null,
        )

        is UpdateState.Failed -> AboutUpdate(
            stringResource(R.string.anvil_update_about_failed),
            Tone.Danger,
            // Не удалась установка — «Повторить» качает снова; не удалась проверка — проверяет снова.
            AboutAction(stringResource(R.string.anvil_update_action_retry)) {
                if (s.update != null) updater.update() else updater.check()
            },
            s.error.describe(context),
        )
    }
}
