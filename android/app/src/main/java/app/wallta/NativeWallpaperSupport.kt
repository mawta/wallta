package app.wallta

import android.app.UiModeManager
import android.app.WallpaperManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.Configuration

internal object NativeWallpaperSupport {
    fun tv(context: Context) = context.getSystemService(UiModeManager::class.java).currentModeType == Configuration.UI_MODE_TYPE_TELEVISION
    fun lock(context: Context): Boolean = !tv(context) && WallpaperManager.getInstance(context).let { it.isWallpaperSupported && it.isSetWallpaperAllowed }
    fun intent(context: Context) = Intent(WallpaperManager.ACTION_CHANGE_LIVE_WALLPAPER)
        .putExtra(WallpaperManager.EXTRA_LIVE_WALLPAPER_COMPONENT, ComponentName(context, WalltaLiveWallpaperService::class.java))
    fun live(context: Context) = !tv(context) && WallpaperManager.getInstance(context).let { it.isWallpaperSupported && it.isSetWallpaperAllowed } && intent(context).resolveActivity(context.packageManager) != null
    fun selected(context: Context) = WallpaperManager.getInstance(context).wallpaperInfo?.component == ComponentName(context, WalltaLiveWallpaperService::class.java)
}
