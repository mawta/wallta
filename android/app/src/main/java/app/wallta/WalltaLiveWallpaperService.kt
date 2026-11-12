package app.wallta

import android.service.wallpaper.WallpaperService
import android.view.SurfaceHolder

class WalltaLiveWallpaperService : WallpaperService() {
    override fun onCreateEngine(): Engine = MotionEngine()
    inner class MotionEngine : Engine() {
        private var playback: NativePlaybackController? = null
        override fun onSurfaceCreated(holder: SurfaceHolder) {
            super.onSurfaceCreated(holder)
            playback = NativePlaybackController(applicationContext, holder, dream = false, preview = isPreview)
            if (isVisible) playback?.start()
        }
        override fun onVisibilityChanged(visible: Boolean) { if (visible) playback?.start() else playback?.stop() }
        override fun onSurfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) { super.onSurfaceChanged(holder, format, width, height); playback?.stop(); if (isVisible) playback?.start() }
        override fun onSurfaceDestroyed(holder: SurfaceHolder) { playback?.destroy(); playback = null; super.onSurfaceDestroyed(holder) }
        override fun onDestroy() { playback?.destroy(); playback = null; super.onDestroy() }
    }
}
