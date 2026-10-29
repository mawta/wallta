package app.wallta

object UpscalePolicy {
    fun sampleSize(width: Int, height: Int): Int {
        require(width > 0 && height > 0 && width.toLong() * height <= 120_000_000) { "Invalid or oversized image" }
        var sample = 1
        while ((width.toLong() + sample - 1) / sample > 8192 || (height.toLong() + sample - 1) / sample > 8192 || ((width.toLong() + sample - 1) / sample) * ((height.toLong() + sample - 1) / sample) > 16_777_216) sample *= 2
        return sample
    }
    fun target(width: Int, height: Int): Pair<Int, Int> {
        require(width > 0 && height > 0 && width.toLong() * height <= 120_000_000) { "Invalid or oversized image" }
        val maxWidth = if (width >= height) 3840 else 2160
        val maxHeight = if (width >= height) 2160 else 3840
        val scale = minOf(maxWidth.toDouble() / width, maxHeight.toDouble() / height)
        return maxOf(1, (width * scale).toInt()) to maxOf(1, (height * scale).toInt())
    }
    fun selection(ids: List<String>, mode: String) {
        require(mode == "fast") { if (mode == "ai") "AI 4K is available on Windows. Use Fast on Android/TV." else "Unknown processing mode" }
        require(ids.size in 1..50 && ids.distinct().size == ids.size && ids.all { it.matches(Regex("[a-f0-9]{64}")) }) { "Select 1–50 unique images" }
    }
}
