package io.github.agitangst.anvil.update

/**
 * Версия `X.Y.Z` с пред-выпуском (`-beta.3`), сравнение — по semver, как у `anvil-update` на ПК:
 * `0.3.1 > 0.3.0`, `1.0.0 > 1.0.0-rc.1`, `beta.2 < beta.10`.
 */
data class Version(val major: Long, val minor: Long, val patch: Long, val pre: String? = null) : Comparable<Version> {
    val isPrerelease: Boolean get() = pre != null

    override fun compareTo(other: Version): Int {
        compareValues(major, other.major).takeIf { it != 0 }?.let { return it }
        compareValues(minor, other.minor).takeIf { it != 0 }?.let { return it }
        compareValues(patch, other.patch).takeIf { it != 0 }?.let { return it }
        return when {
            pre == null && other.pre == null -> 0
            pre == null -> 1
            other.pre == null -> -1
            else -> comparePre(pre, other.pre)
        }
    }

    override fun toString(): String = "$major.$minor.$patch" + (pre?.let { "-$it" } ?: "")

    companion object {
        /** `v1.2.3`, `1.2.3-beta.1`, `1.2.3+build` — без лишних частей; иначе `null`. */
        fun parse(text: String): Version? {
            val body = text.trim().trimStart('v', 'V').substringBefore('+')
            val core: String
            val pre: String?
            val dash = body.indexOf('-')
            if (dash >= 0) {
                core = body.substring(0, dash)
                pre = body.substring(dash + 1).ifEmpty { return null }
            } else {
                core = body
                pre = null
            }
            val parts = core.split('.')
            if (parts.size != 3) return null
            val numbers = parts.map { it.toLongOrNull() ?: return null }
            return Version(numbers[0], numbers[1], numbers[2], pre)
        }

        private fun comparePre(a: String, b: String): Int {
            val left = a.split('.')
            val right = b.split('.')
            for (i in 0 until maxOf(left.size, right.size)) {
                val x = left.getOrNull(i) ?: return -1
                val y = right.getOrNull(i) ?: return 1
                val xn = x.toLongOrNull()
                val yn = y.toLongOrNull()
                val order = when {
                    xn != null && yn != null -> compareValues(xn, yn)
                    xn != null -> -1
                    yn != null -> 1
                    else -> x.compareTo(y)
                }
                if (order != 0) return order
            }
            return 0
        }
    }
}
