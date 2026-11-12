package app.wallta

import org.json.JSONObject

internal object MotionPolicy {
    fun mimeHint(mime: String?, name: String, url: String): String = when (mime.orEmpty().substringBefore(';').trim().lowercase(java.util.Locale.ROOT)) {
        "video/mp4", "video/webm" -> "video"
        "image/gif" -> "gif"
        else -> hint(name, url)
    }
    fun fileKind(file: java.io.File): String = file.inputStream().use { stream -> val header = ByteArray(16); val read = stream.read(header); detect(header.copyOf(maxOf(0, read))) }
    fun hint(name: String, url: String): String {
        val path = runCatching { java.net.URI(url).path }.getOrNull().orEmpty()
        return when {
            listOf(name, path).any { it.matches(Regex("(?i).*\\.(mp4|webm)$")) } -> "video"
            listOf(name, path).any { it.endsWith(".gif", true) } -> "gif"
            else -> "image"
        }
    }
    fun detect(header: ByteArray): String = when {
        header.size >= 6 && String(header, 0, 6, Charsets.US_ASCII) in listOf("GIF87a", "GIF89a") -> "gif"
        header.size >= 12 && String(header, 4, 4, Charsets.US_ASCII) == "ftyp" -> "video"
        header.size >= 4 && header.take(4) == listOf(0x1a.toByte(), 0x45.toByte(), 0xdf.toByte(), 0xa3.toByte()) -> "video"
        else -> "image"
    }
    fun available(state: JSONObject, media: Media, dream: Boolean): Boolean {
        val sources = state.getJSONArray("sources")
        val enabled = (0 until sources.length()).any { sources.getJSONObject(it).let { s -> s.optString("id") == media.sourceId && s.optBoolean("enabled", true) } }
        val selected = state.getJSONObject("settings").optString("homeSourceId")
        return enabled && (dream || selected.isBlank() || selected == media.sourceId)
    }
}
