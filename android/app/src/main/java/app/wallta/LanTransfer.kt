package app.wallta

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.Inet4Address
import java.net.NetworkInterface

/** Owned by one bridge/Activity; never starts a background service or restores a listener. */
class LanTransfer(private val context: Context, private val store: StateStore) {
    private val root = File(context.cacheDir, "lan-transfer")
    @Volatile private var server: LanTransferServer? = null
    @Volatile private var closed = false

    init { root.deleteRecursively() } // Only app-owned, unaccepted transfer staging.

    @Synchronized fun start(mediaIds: JSONArray): JSONObject {
        check(!closed) { "Activity closed" }
        stop()
        require(mediaIds.length() <= 50) { "Choose at most 50 local photos" }
        val ids = (0 until mediaIds.length()).map { mediaIds.getString(it) }
        require(ids.distinct().size == ids.size) { "Duplicate image selection" }
        val addresses = NetworkInterface.getNetworkInterfaces().toList()
            .filter { it.isUp && !it.isLoopback }
            .flatMap { it.inetAddresses.toList() }
            .filterIsInstance<Inet4Address>()
            .mapNotNull { it.hostAddress }
            .filter { LanTransferServer.privateIpv4(it) }.distinct()
        require(addresses.isNotEmpty()) { "No private Wi-Fi/LAN IPv4 address is available. Connect to Wi-Fi and try again." }
        val page = context.assets.open("lan/index.html").use { it.readBytes() }
        val next = LanTransferServer(root, page, LanImageValidator::validate,
            addresses, System::currentTimeMillis, LanTransferServer.Limits.standard())
        server = next
        try {
            check(!closed) { "Activity closed" }
            next.start(MediaRepository(context, store).lanExports(ids))
            check(!closed) { "Activity closed" }
            return JSONObject(next.status().json())
        } catch (e: Exception) { next.close(); if (server === next) server = null; throw e }
    }

    @Synchronized fun status(): JSONObject = JSONObject(server?.status()?.json() ?: LanTransferServer.Status().json())
    @Synchronized fun stop(): JSONObject { server?.close(); server = null; return status() }
    @Synchronized fun accept(): JSONObject {
        check(!closed) { "Activity closed" }
        val current = server ?: error("Start a transfer session first")
        val updated = current.accept { files -> PhotoImporter(context, store).acceptTransferred(files) }
        server = null
        return updated
    }
    // Must cancel a startup even when a slow DocumentsProvider is reading an original.
    fun close() { closed = true; server?.close(); server = null }
}
