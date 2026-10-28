package io.github.agitangst.anvil.update

/** Файл выпуска, который можно скачать. `size` — 0, если неизвестен. */
data class Asset(val name: String, val url: String, val size: Long)

/** Доступное обновление. */
data class Update(
    val version: Version,
    val tag: String,
    /** Заметки к выпуску — тело GitHub Release; пусто, если выпуск найден без API. */
    val notes: String,
    /** Страница выпуска. */
    val page: String,
    val apk: Asset,
    /** `SHA256SUMS` выпуска; без него обновление не ставится. */
    val sums: Asset?,
)

/** Выпуск из ответа API GitHub — только нужные поля. */
internal data class ApiRelease(
    val tag: String,
    val body: String,
    val page: String,
    val draft: Boolean,
    val prerelease: Boolean,
    val assets: List<Asset>,
)

/** Имя APK по соглашению (docs/RELEASES.md, «Под Android»): `<app>-X.Y.Z-android.apk`. */
fun apkName(app: String, version: Version): String = "$app-$version-android.apk"

/** Список выпусков из ответа `GET /repos/<repo>/releases`. */
internal fun parseReleases(json: String): List<ApiRelease> {
    val list = MiniJson.parse(json) as? List<*> ?: throw MiniJson.Error("not an array")
    return list.mapNotNull { item ->
        val r = item as? Map<*, *> ?: return@mapNotNull null
        val tag = r["tag_name"] as? String ?: return@mapNotNull null
        ApiRelease(
            tag = tag,
            body = r["body"] as? String ?: "",
            page = r["html_url"] as? String ?: "",
            draft = r["draft"] == true,
            prerelease = r["prerelease"] == true,
            assets = (r["assets"] as? List<*>).orEmpty().mapNotNull { a ->
                val m = a as? Map<*, *> ?: return@mapNotNull null
                val name = m["name"] as? String ?: return@mapNotNull null
                val url = m["browser_download_url"] as? String ?: return@mapNotNull null
                Asset(name, url, (m["size"] as? Double)?.toLong() ?: 0L)
            },
        )
    }
}

/**
 * Самый новый выпуск новее текущей версии, в котором есть APK этой программы. Черновики не в счёт, пред-выпуски —
 * только если просили. Выпуски без своего APK (в репозитории живут и другие программы) пропускаются — в этом
 * отличие от ПК, где выпуск без архива показывается ссылкой на страницу.
 */
internal fun choose(releases: List<ApiRelease>, app: String, current: Version, prerelease: Boolean): Update? =
    releases.asSequence()
        .filter { !it.draft }
        .mapNotNull { r -> Version.parse(r.tag)?.let { it to r } }
        .filter { (v, r) -> prerelease || !(r.prerelease || v.isPrerelease) }
        .filter { (v, _) -> v > current }
        .mapNotNull { (v, r) ->
            val apk = r.assets.firstOrNull { it.name == apkName(app, v) } ?: return@mapNotNull null
            Update(v, r.tag, r.body.trim(), r.page, apk, r.assets.firstOrNull { it.name == "SHA256SUMS" })
        }
        .maxByOrNull { it.version }

/**
 * Выпуск по тегу без API: APK и `SHA256SUMS` — по адресам из соглашения, размер и заметки неизвестны. Есть ли
 * файлы на самом деле, выяснится при скачивании.
 */
internal fun fromTag(app: String, web: String, repo: String, tag: String, version: Version): Update {
    fun file(name: String) = Asset(name, "$web/$repo/releases/download/$tag/$name", 0)
    return Update(version, tag, "", "$web/$repo/releases/tag/$tag", file(apkName(app, version)), file("SHA256SUMS"))
}

/** Тег из переадресации `…/releases/latest` → `…/releases/tag/<тег>`; иначе `null`. */
internal fun tagFromLocation(location: String): String? =
    location.substringAfter("/releases/tag/", "").substringBefore('?').substringBefore('#').trimEnd('/').ifEmpty { null }

/** Сумма файла из `SHA256SUMS`: строка `<hex>  <имя>` (как у `sha256sum`, допускается `*` перед именем). */
fun expectedSum(sums: String, file: String): String? = sums.lineSequence().firstNotNullOfOrNull { line ->
    val trimmed = line.trim()
    val gap = trimmed.indexOfFirst { it.isWhitespace() }
    if (gap <= 0) return@firstNotNullOfOrNull null
    val name = trimmed.substring(gap).trim().trimStart('*')
    if (name == file) trimmed.substring(0, gap).lowercase() else null
}

/**
 * Через сколько минут сбросится лимит запросов GitHub (не меньше 1) по заголовкам ответа: `retry-after` в секундах
 * или `x-ratelimit-reset` при `x-ratelimit-remaining: 0`. `null` — это не лимит.
 */
internal fun rateLimitMinutes(header: (String) -> String?, nowSeconds: Long): Long? {
    val seconds = header("retry-after")?.trim()?.toLongOrNull()
        ?: if (header("x-ratelimit-remaining")?.trim() == "0") {
            (header("x-ratelimit-reset")?.trim()?.toLongOrNull() ?: return null) - nowSeconds
        } else {
            return null
        }
    return maxOf(1, (seconds + 59) / 60)
}
