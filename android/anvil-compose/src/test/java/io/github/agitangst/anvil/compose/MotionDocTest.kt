package io.github.agitangst.anvil.compose

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import java.io.File

/** Шкала движения набора совпадает с таблицей «Шкала времени» в docs/MOTION.md — имена и миллисекунды. */
class MotionDocTest {
    @Test
    fun scaleMatchesMotionMd() {
        val docs = File(System.getProperty("anvil.docs") ?: "../../docs")
        val row = Regex("""^\|\s*`([A-Z_]+)`\s*\|\s*(\d+)\s*\|""")
        val documented = File(docs, "MOTION.md").readLines()
            .mapNotNull { row.find(it) }
            .associate { it.groupValues[1] to it.groupValues[2].toInt() }
        assertEquals(documented, Motion.SCALE.toMap())
    }

    @Test
    fun inlineConstantsMatch() {
        val text = File(System.getProperty("anvil.docs") ?: "../../docs", "MOTION.md").readText()
        assertEquals(true, text.contains("`WAVE_DELAY` ${Motion.WAVE_DELAY}"), "WAVE_DELAY")
        assertEquals(true, text.contains("`STAGGER` ${Motion.STAGGER} мс"), "STAGGER")
        assertEquals(true, text.contains("`STAGGER_MAX` = ${Motion.STAGGER_MAX}"), "STAGGER_MAX")
    }
}
