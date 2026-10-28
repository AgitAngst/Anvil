package io.github.agitangst.anvil.update

/**
 * Небольшой разбор JSON — ровно на ответы GitHub: объекты (`Map`), массивы (`List`), строки с экранированием,
 * числа (`Double`), `true`/`false`/`null`. Без зависимостей, поэтому тесты идут на обычной JVM.
 */
internal object MiniJson {
    class Error(message: String) : Exception(message)

    fun parse(text: String): Any? {
        val reader = Reader(text)
        val value = reader.value()
        reader.space()
        if (!reader.end()) throw Error("trailing data at ${reader.pos}")
        return value
    }

    private class Reader(val s: String) {
        var pos = 0

        fun end() = pos >= s.length

        fun space() {
            while (pos < s.length && s[pos].isWhitespace()) pos++
        }

        fun value(): Any? {
            space()
            if (end()) throw Error("unexpected end")
            return when (val c = s[pos]) {
                '{' -> obj()
                '[' -> array()
                '"' -> string()
                't' -> word("true", true)
                'f' -> word("false", false)
                'n' -> word("null", null)
                else -> if (c == '-' || c.isDigit()) number() else throw Error("unexpected '$c' at $pos")
            }
        }

        private fun word(w: String, v: Any?): Any? {
            if (!s.startsWith(w, pos)) throw Error("bad literal at $pos")
            pos += w.length
            return v
        }

        private fun obj(): Map<String, Any?> {
            pos++
            val map = LinkedHashMap<String, Any?>()
            space()
            if (s.getOrNull(pos) == '}') { pos++; return map }
            while (true) {
                space()
                if (s.getOrNull(pos) != '"') throw Error("expected key at $pos")
                val key = string()
                space()
                if (s.getOrNull(pos) != ':') throw Error("expected ':' at $pos")
                pos++
                map[key] = value()
                space()
                when (s.getOrNull(pos)) {
                    ',' -> pos++
                    '}' -> { pos++; return map }
                    else -> throw Error("expected ',' or '}' at $pos")
                }
            }
        }

        private fun array(): List<Any?> {
            pos++
            val list = ArrayList<Any?>()
            space()
            if (s.getOrNull(pos) == ']') { pos++; return list }
            while (true) {
                list += value()
                space()
                when (s.getOrNull(pos)) {
                    ',' -> pos++
                    ']' -> { pos++; return list }
                    else -> throw Error("expected ',' or ']' at $pos")
                }
            }
        }

        private fun string(): String {
            pos++
            val out = StringBuilder()
            while (true) {
                if (end()) throw Error("unterminated string")
                when (val c = s[pos++]) {
                    '"' -> return out.toString()
                    '\\' -> {
                        if (end()) throw Error("unterminated escape")
                        when (val e = s[pos++]) {
                            '"', '\\', '/' -> out.append(e)
                            'b' -> out.append('\b')
                            'f' -> out.append('\u000C')
                            'n' -> out.append('\n')
                            'r' -> out.append('\r')
                            't' -> out.append('\t')
                            'u' -> {
                                if (pos + 4 > s.length) throw Error("bad \\u escape")
                                out.append(s.substring(pos, pos + 4).toInt(16).toChar())
                                pos += 4
                            }
                            else -> throw Error("bad escape '\\$e'")
                        }
                    }
                    else -> out.append(c)
                }
            }
        }

        private fun number(): Double {
            val start = pos
            if (s[pos] == '-') pos++
            while (pos < s.length && (s[pos].isDigit() || s[pos] in ".eE+-")) pos++
            return s.substring(start, pos).toDoubleOrNull() ?: throw Error("bad number at $start")
        }
    }
}
