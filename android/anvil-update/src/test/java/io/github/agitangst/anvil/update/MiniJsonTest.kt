package io.github.agitangst.anvil.update

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Test

class MiniJsonTest {
    @Test
    fun readsWhatGitHubSends() {
        val value = MiniJson.parse(
            """ { "a": [1, -2.5, 3e2, true, false, null], "s": "q\"\\\/\n\tяé", "o": {}, "l": [] } """,
        ) as Map<*, *>
        assertEquals(listOf(1.0, -2.5, 300.0, true, false, null), value["a"])
        assertEquals("q\"\\/\n\tяé", value["s"])
        assertEquals(emptyMap<String, Any?>(), value["o"])
        assertEquals(emptyList<Any?>(), value["l"])
    }

    @Test
    fun rejectsBrokenInput() {
        listOf("", "{", "[1,]", "{\"a\" 1}", "\"open", "tru", "{} x", "[01x]").forEach { bad ->
            assertThrows(MiniJson.Error::class.java, { MiniJson.parse(bad) }, bad)
        }
    }
}
