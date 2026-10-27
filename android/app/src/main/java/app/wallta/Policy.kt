package app.wallta

import org.json.JSONArray
import org.json.JSONObject
import java.net.URI
import java.security.MessageDigest
import java.time.LocalDateTime
import java.time.LocalTime

data class Media(val id: String, val name: String, val url: String, val sourceId: String, val photos: Boolean = false) {
    fun json(public: Boolean = false) = JSONObject().put("id", id).put("name", name).put("url", if (public) "" else url).put("sourceId", sourceId).put("photos", photos)
    companion object { fun from(j: JSONObject) = Media(j.getString("id"), j.getString("name"), j.getString("url"), j.getString("sourceId"), j.optBoolean("photos")) }
}

object Policy {
    fun hash(value: String) = MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }
    fun http(url: String): Boolean = runCatching { val u = URI(url); u.scheme in listOf("https", "http") && !u.host.isNullOrEmpty() && u.userInfo == null }.getOrDefault(false)
    fun inSchedule(enabled: Boolean, days: Set<Int>, start: String, end: String, now: LocalDateTime): Boolean {
        if (!enabled) return true
        val a = runCatching { LocalTime.parse(start) }.getOrElse { return false }
        val b = runCatching { LocalTime.parse(end) }.getOrElse { return false }
        val day = now.dayOfWeek.value % 7
        val t = now.toLocalTime()
        if (a == b) return day in days
        if (a < b) return day in days && t >= a && t < b
        return if (t >= a) day in days else t < b && (day + 6) % 7 in days
    }
    fun next(ids: List<String>, used: Set<String>, shuffle: Boolean, current: String?): String? {
        if (ids.isEmpty()) return null
        if (!shuffle) return ids[(ids.indexOf(current) + 1).mod(ids.size)]
        val remaining = ids.filterNot { it in used }
        return (remaining.ifEmpty { ids.filterNot { it == current }.ifEmpty { ids } }).random()
    }
    fun manifest(text: String, base: String, source: String): List<Media> {
        require(text.toByteArray().size <= 4 * 1024 * 1024) { "Manifest exceeds 4 MB" }
        val trimmed = text.trim()
        val a = if (trimmed.startsWith("[")) JSONArray(trimmed) else JSONObject(trimmed).getJSONArray("images")
        require(a.length() <= 10000) { "Manifest exceeds 10,000 images" }
        return (0 until a.length()).mapNotNull { i ->
            val entry = a.get(i)
            val path = if (entry is String) entry else (entry as? JSONObject)?.optString("url") ?: return@mapNotNull null
            val url = runCatching { URI(base).resolve(path).toString() }.getOrNull() ?: return@mapNotNull null
            if (!http(url)) null else Media(hash("$source:$url"), (entry as? JSONObject)?.optString("name")?.ifBlank { null } ?: URI(url).path.substringAfterLast('/'), url, source)
        }.distinctBy { it.id }
    }
}
