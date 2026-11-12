package app.wallta

/** Visible native decoders hold at most one current and one pending file per surface. */
internal object ActiveMediaPins {
    private val ids = mutableMapOf<String, Int>()
    @Synchronized fun acquire(id: String) { ids[id] = (ids[id] ?: 0) + 1 }
    @Synchronized fun release(id: String) { val count = ids[id] ?: return; if (count <= 1) ids.remove(id) else ids[id] = count - 1 }
    @Synchronized fun contains(id: String) = ids.containsKey(id)
}
