package app.wallta

internal object PlaybackRange {
    fun parse(header: String?, length: Long): LongRange {
        require(length > 0)
        if (header == null) return 0 until length
        val match = Regex("bytes=(\\d*)-(\\d*)").matchEntire(header) ?: error("Invalid byte range")
        val (a, b) = match.destructured
        require(a.isNotEmpty() || b.isNotEmpty())
        val start = if (a.isEmpty()) (length - b.toLong()).coerceAtLeast(0) else a.toLong()
        val end = if (a.isEmpty() || b.isEmpty()) length - 1 else b.toLong().coerceAtMost(length - 1)
        require(start in 0 until length && end >= start)
        return start..end
    }
}
