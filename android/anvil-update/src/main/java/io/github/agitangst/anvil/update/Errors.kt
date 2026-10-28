package io.github.agitangst.anvil.update

import android.content.Context

/** Что пошло не так при обновлении. Текст для человека — [describe], на языке программы. */
sealed interface UpdateError {
    /** Лимит запросов GitHub, а обход через `releases/latest` не помог. */
    data class RateLimit(val status: Int, val minutes: Long) : UpdateError

    /** GitHub ответил кодом, отличным от 200. */
    data class Http(val status: Int) : UpdateError

    /** Нет связи: сеть, DNS, таймаут. */
    data object Network : UpdateError

    /** Ответ GitHub не разобрать. */
    data object BadAnswer : UpdateError

    /** В выпуске нет APK этой программы (выпуск найден без API, по тегу). */
    data object NoApk : UpdateError

    /** В выпуске нет `SHA256SUMS` или строки для APK — непроверенный файл не ставится. */
    data object NoSums : UpdateError

    /** Сумма скачанного APK не совпала со строкой в `SHA256SUMS`. */
    data object Checksum : UpdateError

    /** Не хватило места или не удалось записать файл. */
    data object Storage : UpdateError

    /** Установщик системы отказал; `message` — его текст, если есть. */
    data class Install(val message: String?) : UpdateError
}

/** Понятный текст ошибки на языке программы. */
fun UpdateError.describe(context: Context): String = when (this) {
    is UpdateError.RateLimit -> context.getString(R.string.anvil_update_err_rate_limit, status, minutes)
    is UpdateError.Http -> context.getString(R.string.anvil_update_err_http, status)
    UpdateError.Network -> context.getString(R.string.anvil_update_err_network)
    UpdateError.BadAnswer -> context.getString(R.string.anvil_update_err_bad_answer)
    UpdateError.NoApk -> context.getString(R.string.anvil_update_err_no_apk)
    UpdateError.NoSums -> context.getString(R.string.anvil_update_err_no_sums)
    UpdateError.Checksum -> context.getString(R.string.anvil_update_err_checksum)
    UpdateError.Storage -> context.getString(R.string.anvil_update_err_storage)
    is UpdateError.Install ->
        if (message.isNullOrBlank()) context.getString(R.string.anvil_update_err_install_unknown)
        else context.getString(R.string.anvil_update_err_install, message)
}

/** Ошибка, которая несёт [UpdateError] через корутины. */
internal class UpdateException(val error: UpdateError) : Exception(error.toString())
