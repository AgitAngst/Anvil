package io.github.agitangst.anvil.update

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.provider.Settings
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.edit
import androidx.lifecycle.compose.LifecycleResumeEffect
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Что обновлять: `app` — начало имени APK (`<app>-X.Y.Z-android.apk`), `version` — текущая (`BuildConfig.VERSION_NAME`),
 * `repo` — `owner/name` на GitHub.
 */
data class UpdateConfig(
    val app: String,
    val version: String,
    val repo: String,
    val api: String = "https://api.github.com",
    val web: String = "https://github.com",
)

/** Состояние обновления. Вид — `UpdateBanner`, «О программе» — `rememberAboutUpdate`. */
sealed interface UpdateState {
    /** Ещё не проверяли (или авто-проверка выключена). */
    data object Idle : UpdateState

    data object Checking : UpdateState

    /** Новее нет. */
    data object UpToDate : UpdateState

    data class Available(val update: Update) : UpdateState

    /** `total` — 0, если размер неизвестен. */
    data class Downloading(val update: Update, val done: Long, val total: Long) : UpdateState

    /** APK проверен, но у программы нет разрешения «Установка неизвестных приложений». */
    data class NeedsPermission(val update: Update) : UpdateState

    /** Система просит подтвердить установку — окно открывает [UpdaterEffects]. */
    data class Confirm(val update: Update, val intent: Intent) : UpdateState

    data class Installing(val update: Update) : UpdateState

    /** Программа запущена уже новой версией после обновления изнутри. */
    data class Installed(val version: String) : UpdateState

    /** `update` — что не удалось поставить (тогда «Повторить» качает снова); `null` — не удалась проверка. */
    data class Failed(val error: UpdateError, val update: Update?) : UpdateState
}

/**
 * Обновление программы из GitHub Releases (сборка `github`), как `anvil-update` на ПК: проверка при запуске, если
 * включено, и по кнопке; пред-выпуски по флажку; пропущенная версия при авто-проверке не предлагается; лимит
 * запросов — обход через `releases/latest`; APK ставится только при совпадении SHA-256 со строкой в `SHA256SUMS`,
 * установку подтверждает человек в окне системы.
 *
 * Один на процесс: [shared]. Состояние — наблюдаемое Compose ([state], [showBanner]).
 */
class Updater private constructor(context: Context, val config: UpdateConfig) {
    private val app = context.applicationContext
    private val prefs = app.getSharedPreferences("anvil", Context.MODE_PRIVATE)
    private val github = GitHub(config.api, config.web)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var job: Job? = null
    private var autoDone = false
    private var pending: Pair<Update, File>? = null

    @Volatile
    private var cancelRequested = false

    /** Текущая версия программы. */
    val current: Version = Version.parse(config.version) ?: Version(0, 0, 0)

    var state: UpdateState by mutableStateOf(startState())
        private set

    /** «Позже»: баннер скрыт до следующего запуска программы. */
    var bannerHidden: Boolean by mutableStateOf(false)
        private set

    private var prerelease by mutableStateOf(prefs.getBoolean(KEY_PRERELEASE, false))

    /** «Предлагать пред-выпуски» (по умолчанию выключено). */
    var offerPrereleases: Boolean
        get() = prerelease
        set(value) {
            prerelease = value
            prefs.edit { putBoolean(KEY_PRERELEASE, value) }
        }

    /** Показывать ли баннер над содержимым. */
    val showBanner: Boolean
        get() = !bannerHidden && when (val s = state) {
            is UpdateState.Available, is UpdateState.Downloading, is UpdateState.NeedsPermission,
            is UpdateState.Confirm, is UpdateState.Installing, is UpdateState.Installed -> true
            is UpdateState.Failed -> s.update != null
            else -> false
        }

    private fun startState(): UpdateState {
        val installing = prefs.getString(KEY_INSTALLING, null) ?: return UpdateState.Idle
        prefs.edit { remove(KEY_INSTALLING) }
        cacheDir().deleteRecursively()
        return if (installing == current.toString()) UpdateState.Installed(installing) else UpdateState.Idle
    }

    private fun cacheDir() = File(app.cacheDir, "anvil-update")

    /**
     * Проверка при запуске программы: раз за процесс, если включено «Проверять обновления». Ошибки авто-проверки
     * не показываются — они видны при проверке по кнопке.
     */
    fun auto(enabled: Boolean) {
        if (autoDone) return
        autoDone = true
        if (enabled && state is UpdateState.Idle) check(manual = false)
    }

    /** Проверить сейчас. По кнопке ([manual]) пропущенная версия тоже предлагается, ошибка — в состоянии. */
    fun check(manual: Boolean = true) {
        when (state) {
            is UpdateState.Checking, is UpdateState.Downloading, is UpdateState.Installing, is UpdateState.Confirm -> return
            else -> Unit
        }
        job?.cancel()
        state = UpdateState.Checking
        job = scope.launch {
            state = try {
                val update = withContext(Dispatchers.IO) { fetch() }
                prefs.edit { putLong(KEY_LAST_CHECK, System.currentTimeMillis()) }
                when {
                    update == null -> UpdateState.UpToDate
                    !manual && update.version.toString() == prefs.getString(KEY_SKIP, null) -> UpdateState.UpToDate
                    else -> {
                        if (manual) bannerHidden = false
                        UpdateState.Available(update)
                    }
                }
            } catch (e: UpdateException) {
                if (manual) UpdateState.Failed(e.error, null) else UpdateState.Idle
            }
        }
    }

    private fun fetch(): Update? = try {
        choose(github.releases(config.repo), config.app, current, offerPrereleases)
    } catch (e: GitHub.RateLimited) {
        // Лимит API: последний обычный выпуск — по переадресации, файлы — по адресам из соглашения.
        val tag = github.latestTag(config.repo)
        val version = tag?.let(Version::parse) ?: throw UpdateException(UpdateError.RateLimit(e.status, e.minutes))
        if (version > current) fromTag(config.app, config.web, config.repo, tag, version) else null
    }

    /** «Позже». */
    fun later() {
        bannerHidden = true
    }

    /** «Пропустить эту версию»: при авто-проверке она больше не предлагается. */
    fun skip() {
        (state as? UpdateState.Available)?.let { prefs.edit { putString(KEY_SKIP, it.update.version.toString()) } }
        bannerHidden = true
    }

    /** «Обновить» и «Повторить»: скачать APK, сверить SHA-256 и отдать установщику системы. */
    fun update() {
        val u = when (val s = state) {
            is UpdateState.Available -> s.update
            is UpdateState.Failed -> s.update ?: return
            else -> return
        }
        job?.cancel()
        cancelRequested = false
        state = UpdateState.Downloading(u, 0, u.apk.size)
        job = scope.launch {
            try {
                val file = withContext(Dispatchers.IO) { fetchVerified(u) }
                install(u, file)
            } catch (e: UpdateException) {
                state = UpdateState.Failed(e.error, u)
            } catch (e: Cancelled) {
                cacheDir().deleteRecursively()
                state = UpdateState.Available(u)
            }
        }
    }

    /** «Отмена» во время скачивания. */
    fun cancel() {
        cancelRequested = true
    }

    private fun fetchVerified(u: Update): File {
        val sums = u.sums?.url?.let(github::text) ?: throw UpdateException(UpdateError.NoSums)
        val expected = expectedSum(sums, u.apk.name) ?: throw UpdateException(UpdateError.NoSums)
        val dir = cacheDir().apply { deleteRecursively() }
        if (!dir.mkdirs()) throw UpdateException(UpdateError.Storage)
        val file = File(dir, u.apk.name)
        var shown = -1L
        val actual = try {
            github.download(u.apk.url, file, { done, total ->
                // Не чаще раза на процент (или на 256 КБ, если размер неизвестен).
                val step = if (total > 0) done * 100 / total else done / (256 * 1024)
                if (step != shown) {
                    shown = step
                    scope.launch { if (state is UpdateState.Downloading) state = UpdateState.Downloading(u, done, total) }
                }
            }, { cancelRequested })
        } catch (e: kotlinx.coroutines.CancellationException) {
            file.delete()
            throw Cancelled()
        }
        if (!actual.equals(expected, ignoreCase = true)) {
            file.delete()
            throw UpdateException(UpdateError.Checksum)
        }
        return file
    }

    private fun install(u: Update, file: File) {
        if (!app.packageManager.canRequestPackageInstalls()) {
            pending = u to file
            state = UpdateState.NeedsPermission(u)
            return
        }
        pending = null
        state = UpdateState.Installing(u)
        prefs.edit { putString(KEY_INSTALLING, u.version.toString()) }
        try {
            Installer.install(app, file)
        } catch (e: Exception) {
            prefs.edit { remove(KEY_INSTALLING) }
            state = UpdateState.Failed(UpdateError.Install(e.message), u)
        }
    }

    /** Открыть настройку «Установка неизвестных приложений» для этой программы. */
    fun installPermissionIntent(): Intent =
        Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${app.packageName}"))

    /** Вернулись в программу: если разрешение на установку дали — ставим. Зовёт [UpdaterEffects]. */
    fun onResume() {
        val (u, file) = pending ?: return
        if (state is UpdateState.NeedsPermission && app.packageManager.canRequestPackageInstalls()) install(u, file)
    }

    /** Окно подтверждения системы открыто. */
    fun confirmShown() {
        (state as? UpdateState.Confirm)?.let { state = UpdateState.Installing(it.update) }
    }

    internal fun onInstallStatus(status: Int, message: String?, confirm: Intent?) {
        val u = when (val s = state) {
            is UpdateState.Installing -> s.update
            is UpdateState.Confirm -> s.update
            else -> return
        }
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> if (confirm != null) state = UpdateState.Confirm(u, confirm)
            PackageInstaller.STATUS_SUCCESS -> state = UpdateState.Installed(u.version.toString())
            PackageInstaller.STATUS_FAILURE_ABORTED -> {
                // «Отмена» в окне системы — не ошибка: обновление снова предлагается.
                prefs.edit { remove(KEY_INSTALLING) }
                state = UpdateState.Available(u)
            }
            else -> {
                prefs.edit { remove(KEY_INSTALLING) }
                state = UpdateState.Failed(UpdateError.Install(message), u)
            }
        }
    }

    private class Cancelled : Exception("cancelled")

    companion object {
        private const val KEY_SKIP = "update_skip"
        private const val KEY_LAST_CHECK = "update_last_check"
        private const val KEY_INSTALLING = "update_installing"
        private const val KEY_PRERELEASE = "update_prerelease"

        private val instances = mutableMapOf<String, Updater>()

        /** Один проверяльщик на программу в процессе: переживает пересоздание экрана (смену языка, поворот). */
        fun shared(context: Context, config: UpdateConfig): Updater = synchronized(instances) {
            instances.getOrPut(config.app) { Updater(context, config) }
        }

        internal fun dispatch(status: Int, message: String?, confirm: Intent?) {
            synchronized(instances) { instances.values.toList() }.forEach { it.onInstallStatus(status, message, confirm) }
        }
    }
}

/**
 * Связь проверяльщика с экраном: открыть окно подтверждения системы и доставить установку после выдачи разрешения.
 * Звать один раз на экране, где показывается обновление (`UpdateBanner` зовёт сам).
 */
@Composable
fun UpdaterEffects(updater: Updater) {
    val context = LocalContext.current
    LifecycleResumeEffect(updater) {
        updater.onResume()
        onPauseOrDispose { }
    }
    val state = updater.state
    LaunchedEffect(state) {
        if (state is UpdateState.Confirm) {
            val intent = Intent(state.intent)
            if (context !is Activity) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(intent)
            updater.confirmShown()
        }
    }
}
