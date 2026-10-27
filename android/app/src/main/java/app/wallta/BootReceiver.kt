package app.wallta

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action !in listOf(Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED)) return
        val pending = goAsync()
        Thread {
            try { val store = StateStore(context); val state = store.state(); if (state.getJSONObject("settings").optBoolean("autostart") && !state.optBoolean("running")) store.setRunning(true); WallpaperWorker.schedule(context) }
            finally { pending.finish() }
        }.start()
    }
}
