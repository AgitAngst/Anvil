package io.github.agitangst.anvil.update

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

/** Выбор выпуска, разбор ответа GitHub, суммы, тег из переадресации, минуты лимита. */
class ReleasesTest {
    private fun release(tag: String, prerelease: Boolean = false, draft: Boolean = false, vararg files: String) = ApiRelease(
        tag = tag,
        body = "Notes for $tag\n",
        page = "https://github.com/o/r/releases/tag/$tag",
        draft = draft,
        prerelease = prerelease,
        assets = files.map { Asset(it, "https://dl/$it", 10) },
    )

    private val current = Version.parse("0.3.0")!!

    @Test
    fun picksNewestStableWithItsApk() {
        val list = listOf(
            release("v0.3.1", files = arrayOf("demo-0.3.1-android.apk", "SHA256SUMS")),
            release("v0.3.2", files = arrayOf("demo-0.3.2-android.apk", "SHA256SUMS")),
            release("v0.4.0-beta.1", prerelease = true, files = arrayOf("demo-0.4.0-beta.1-android.apk")),
            release("v0.5.0", draft = true, files = arrayOf("demo-0.5.0-android.apk")),
            release("v0.2.9", files = arrayOf("demo-0.2.9-android.apk")),
        )
        val u = choose(list, "demo", current, prerelease = false)!!
        assertEquals("v0.3.2", u.tag)
        assertEquals("demo-0.3.2-android.apk", u.apk.name)
        assertEquals("https://dl/SHA256SUMS", u.sums!!.url)
        assertEquals("Notes for v0.3.2", u.notes)
    }

    @Test
    fun prereleasesOnlyOnRequest() {
        val list = listOf(release("v0.4.0-beta.1", prerelease = true, files = arrayOf("demo-0.4.0-beta.1-android.apk")))
        assertNull(choose(list, "demo", current, prerelease = false))
        assertEquals("v0.4.0-beta.1", choose(list, "demo", current, prerelease = true)!!.tag)
        // Тег пред-выпуска без флажка GitHub — всё равно пред-выпуск.
        val unflagged = listOf(release("v0.4.0-rc.1", files = arrayOf("demo-0.4.0-rc.1-android.apk")))
        assertNull(choose(unflagged, "demo", current, prerelease = false))
    }

    @Test
    fun skipsReleasesWithoutThisApp() {
        val list = listOf(
            release("v1.1.0", files = arrayOf("desktop-1.1.0-windows-x64.zip", "SHA256SUMS")),
            release("v0.3.1", files = arrayOf("demo-0.3.1-android.apk")),
        )
        assertEquals("v0.3.1", choose(list, "demo", current, prerelease = false)!!.tag)
        assertNull(choose(list.take(1), "demo", current, prerelease = false))
    }

    @Test
    fun nothingNewer() {
        val list = listOf(release("v0.3.0", files = arrayOf("demo-0.3.0-android.apk")), release("kit-v9.0.0"))
        assertNull(choose(list, "demo", current, prerelease = true))
    }

    @Test
    fun parsesGitHubAnswer() {
        val json = """
            [{"url":"x","html_url":"https://github.com/o/r/releases/tag/v0.4.0","tag_name":"v0.4.0","name":"0.4.0",
              "draft":false,"prerelease":false,"body":"## What's new\n- \"quotes\"",
              "assets":[{"name":"demo-0.4.0-android.apk","size":12345678,"browser_download_url":"https://github.com/o/r/releases/download/v0.4.0/demo-0.4.0-android.apk","uploader":{"login":"x"}},
                        {"name":"SHA256SUMS","size":90,"browser_download_url":"https://github.com/o/r/releases/download/v0.4.0/SHA256SUMS"}]},
             {"tag_name":"v0.3.9","draft":true,"prerelease":false,"body":null,"assets":[]}]
        """.trimIndent()
        val list = parseReleases(json)
        assertEquals(2, list.size)
        assertEquals("## What's new\n- \"quotes\"", list[0].body)
        assertEquals(12345678L, list[0].assets[0].size)
        assertEquals("", list[1].body)
        val u = choose(list, "demo", current, prerelease = false)!!
        assertEquals("https://github.com/o/r/releases/download/v0.4.0/demo-0.4.0-android.apk", u.apk.url)
    }

    @Test
    fun fromTagUsesTheConvention() {
        val u = fromTag("demo", "https://github.com", "o/r", "v0.4.0", Version.parse("0.4.0")!!)
        assertEquals("https://github.com/o/r/releases/download/v0.4.0/demo-0.4.0-android.apk", u.apk.url)
        assertEquals("https://github.com/o/r/releases/download/v0.4.0/SHA256SUMS", u.sums!!.url)
        assertEquals("", u.notes)
    }

    @Test
    fun expectedSums() {
        val sums = "3F5C9A  demo-0.4.0-android.apk\nabc123 *other.zip\n\n"
        assertEquals("3f5c9a", expectedSum(sums, "demo-0.4.0-android.apk"))
        assertEquals("abc123", expectedSum(sums, "other.zip"))
        assertNull(expectedSum(sums, "missing.apk"))
    }

    @Test
    fun tagFromRedirect() {
        assertEquals("v0.4.0", tagFromLocation("https://github.com/o/r/releases/tag/v0.4.0"))
        assertEquals("v0.4.0", tagFromLocation("https://github.com/o/r/releases/tag/v0.4.0/"))
        assertNull(tagFromLocation("https://github.com/o/r/releases"))
    }

    @Test
    fun rateLimitMinutesFromHeaders() {
        fun headers(vararg pairs: Pair<String, String>): (String) -> String? = { pairs.toMap()[it] }
        assertEquals(2, rateLimitMinutes(headers("retry-after" to "90"), 0))
        assertEquals(24, rateLimitMinutes(headers("x-ratelimit-remaining" to "0", "x-ratelimit-reset" to "2400"), 1000))
        assertEquals(1, rateLimitMinutes(headers("x-ratelimit-remaining" to "0", "x-ratelimit-reset" to "1000"), 1000))
        assertNull(rateLimitMinutes(headers("x-ratelimit-remaining" to "5"), 0))
        assertNull(rateLimitMinutes(headers(), 0))
    }
}
