package io.github.agitangst.anvil.update

import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/** Обращения к GitHub. Всё блокирующее — звать из `Dispatchers.IO`. */
internal class GitHub(private val api: String, private val web: String) {
    private val userAgent = "anvil-update-android/${BuildConfig.KIT_VERSION}"

    private fun open(url: String, followRedirects: Boolean = true): HttpURLConnection =
        (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = TIMEOUT_MS
            readTimeout = TIMEOUT_MS
            instanceFollowRedirects = followRedirects
            setRequestProperty("User-Agent", userAgent)
        }

    /** Список выпусков. Лимит — [RateLimited]; другой код — [UpdateError.Http]; нет связи — [UpdateError.Network]. */
    fun releases(repo: String): List<ApiRelease> {
        val conn = open("$api/repos/$repo/releases?per_page=20")
        try {
            conn.setRequestProperty("Accept", "application/vnd.github+json")
            val code = try { conn.responseCode } catch (e: IOException) { throw UpdateException(UpdateError.Network) }
            if (code == 403 || code == 429) {
                val minutes = rateLimitMinutes({ conn.getHeaderField(it) }, System.currentTimeMillis() / 1000)
                if (minutes != null) throw RateLimited(code, minutes)
            }
            if (code != 200) throw UpdateException(UpdateError.Http(code))
            val text = try { conn.inputStream.bufferedReader().use { it.readText() } } catch (e: IOException) {
                throw UpdateException(UpdateError.Network)
            }
            return try { parseReleases(text) } catch (e: MiniJson.Error) { throw UpdateException(UpdateError.BadAnswer) }
        } finally {
            conn.disconnect()
        }
    }

    /** Тег последнего обычного выпуска по переадресации `<сайт>/<репозиторий>/releases/latest`; без API. */
    fun latestTag(repo: String): String? {
        val conn = open("$web/$repo/releases/latest", followRedirects = false)
        try {
            val code = try { conn.responseCode } catch (e: IOException) { return null }
            if (code !in 300..399) return null
            return conn.getHeaderField("Location")?.let(::tagFromLocation)
        } finally {
            conn.disconnect()
        }
    }

    /** Небольшой текстовый файл выпуска (`SHA256SUMS`); `null` — его нет (404). */
    fun text(url: String): String? {
        val conn = open(url)
        try {
            val code = try { conn.responseCode } catch (e: IOException) { throw UpdateException(UpdateError.Network) }
            if (code == 404) return null
            if (code != 200) throw UpdateException(UpdateError.Http(code))
            return try { conn.inputStream.bufferedReader().use { it.readText() } } catch (e: IOException) {
                throw UpdateException(UpdateError.Network)
            }
        } finally {
            conn.disconnect()
        }
    }

    /**
     * Скачать файл в [target], считая SHA-256 по ходу; [progress] — (скачано, всего; 0 — неизвестно), [cancelled] —
     * прервать. Возвращает сумму в hex.
     */
    fun download(url: String, target: File, progress: (Long, Long) -> Unit, cancelled: () -> Boolean): String {
        val conn = open(url)
        try {
            val code = try { conn.responseCode } catch (e: IOException) { throw UpdateException(UpdateError.Network) }
            if (code == 404) throw UpdateException(UpdateError.NoApk)
            if (code != 200) throw UpdateException(UpdateError.Http(code))
            val total = conn.contentLengthLong.coerceAtLeast(0)
            val digest = MessageDigest.getInstance("SHA-256")
            try {
                conn.inputStream.use { input ->
                    target.outputStream().use { output ->
                        val buffer = ByteArray(64 * 1024)
                        var done = 0L
                        while (true) {
                            if (cancelled()) throw kotlinx.coroutines.CancellationException("cancelled")
                            val n = input.read(buffer)
                            if (n < 0) break
                            output.write(buffer, 0, n)
                            digest.update(buffer, 0, n)
                            done += n
                            progress(done, total)
                        }
                    }
                }
            } catch (e: IOException) {
                target.delete()
                // Место кончилось — запись падает с ENOSPC; остальное — сеть.
                throw UpdateException(if (e.message.orEmpty().contains("ENOSPC")) UpdateError.Storage else UpdateError.Network)
            }
            return digest.digest().joinToString("") { "%02x".format(it) }
        } finally {
            conn.disconnect()
        }
    }

    /** Лимит запросов API: код ответа и минуты до сброса. */
    class RateLimited(val status: Int, val minutes: Long) : Exception("GitHub: HTTP $status (rate limit), retry in $minutes min")

    private companion object {
        const val TIMEOUT_MS = 60_000
    }
}
