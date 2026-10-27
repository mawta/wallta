package app.wallta

import java.net.URI

/** Pure policies shared by preview decoding, provider credentials and both rotation paths. */
object MediaSafety {
    /** Call after omitted secrets have been merged with their saved values. */
    fun sourceConfig(source: org.json.JSONObject): String = org.json.JSONArray().put(source.optString("kind")).put(source.optString("location"))
        .put(source.optBoolean("recursive")).put(source.optBoolean("enabled", true)).put(source.optString("username")).put(source.optString("password"))
        .put(source.optString("apiKey")).put(source.optString("accessToken")).toString()
    fun visualSize(width: Int, height: Int): Pair<Int, Int> {
        require(width > 0 && height > 0)
        val longest = maxOf(width, height)
        if (longest <= 2048) return width to height
        return maxOf(1, (width.toLong() * 2048 / longest).toInt()) to maxOf(1, (height.toLong() * 2048 / longest).toInt())
    }
    fun sampleSize(width: Int, height: Int): Int {
        require(width > 0 && height > 0)
        var sample = 1
        while ((width.toLong() + sample - 1) / sample > 4096 || (height.toLong() + sample - 1) / sample > 4096 || ((width.toLong() + sample - 1) / sample) * ((height.toLong() + sample - 1) / sample) > 4_194_304) sample *= 2
        return sample
    }
    fun within(url: String, base: String): Boolean = runCatching {
        val target = URI(url); val expected = URI(base)
        fun port(uri: URI) = if (uri.port >= 0) uri.port else if (uri.scheme == "https") 443 else 80
        fun validPath(uri: URI) = uri.path != null && !uri.path.contains('\\') && uri.path.split('/').none { it == "." || it == ".." }
        target.userInfo == null && target.fragment == null && expected.userInfo == null && target.scheme == expected.scheme && target.scheme in listOf("https", "http") &&
            target.host != null && target.host.equals(expected.host, ignoreCase = true) && port(target) == port(expected) && validPath(target) && validPath(expected) &&
            (target.path == expected.path.trimEnd('/') || target.path.startsWith(expected.path.trimEnd('/') + "/"))
    }.getOrDefault(false)
    fun photosUrl(url: String): Boolean = runCatching {
        val uri = URI(url)
        uri.scheme == "https" && uri.userInfo == null && uri.port in listOf(-1, 443) && uri.fragment == null && (uri.host == "googleusercontent.com" || uri.host?.endsWith(".googleusercontent.com") == true)
    }.getOrDefault(false)
    fun <T> firstUsable(ids: List<String>, used: Set<String>, shuffle: Boolean, current: String?, ensureCurrent: () -> Unit = {}, discard: (T) -> Unit = {}, load: (String) -> T): Pair<String, T> {
        val failed = mutableSetOf<String>(); var cursor = current; var lastError: Exception? = null
        val offset = (ids.indexOf(current) + 1).coerceAtMost(ids.size)
        val sequential = ids.drop(offset) + ids.take(offset)
        repeat(minOf(20, ids.distinct().size)) {
            ensureCurrent()
            val available = ids.filterNot { it in failed }
            val id = if (shuffle) Policy.next(available, used, true, cursor) else sequential.firstOrNull { it !in failed }
            if (id == null) return@repeat
            try {
                val result = load(id)
                try { ensureCurrent() } catch (e: Exception) { discard(result); throw e }
                return id to result
            }
            catch (e: Exception) { ensureCurrent(); if (e is InterruptedException || e is java.util.concurrent.CancellationException) throw e; failed += id; cursor = id; lastError = e }
        }
        throw IllegalStateException("No usable image in ${minOf(20, ids.distinct().size)} candidates${lastError?.message?.let { ": $it" } ?: ""}", lastError)
    }
}
