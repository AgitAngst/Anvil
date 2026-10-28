package io.github.agitangst.anvil.update

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/** Порядок версий — как у `anvil-update` на ПК. */
class VersionTest {
    private fun v(s: String) = Version.parse(s)!!

    @Test
    fun parses() {
        assertEquals(Version(1, 2, 3), v("v1.2.3"))
        assertEquals(Version(0, 4, 0, "beta.1"), v("0.4.0-beta.1"))
        assertEquals(Version(1, 0, 0), v("1.0.0+build.7"))
        assertEquals("0.4.0-beta.1", v("V0.4.0-beta.1").toString())
        assertNull(Version.parse("1.2"))
        assertNull(Version.parse("1.2.3.4"))
        assertNull(Version.parse("1.2.x"))
        assertNull(Version.parse("1.2.3-"))
        assertNull(Version.parse("kit-v1.0.0"))
    }

    @Test
    fun orders() {
        assertTrue(v("0.3.1") > v("0.3.0"))
        assertTrue(v("0.10.0") > v("0.9.9"))
        assertTrue(v("1.0.0") > v("1.0.0-rc.1"))
        assertTrue(v("1.0.0-rc.1") > v("1.0.0-beta.9"))
        assertTrue(v("1.0.0-beta.10") > v("1.0.0-beta.2"))
        assertTrue(v("1.0.0-beta") < v("1.0.0-beta.1"))
        assertTrue(v("1.0.0-alpha.1") < v("1.0.0-alpha.beta"))
        assertEquals(0, v("1.2.3").compareTo(v("v1.2.3")))
    }
}
