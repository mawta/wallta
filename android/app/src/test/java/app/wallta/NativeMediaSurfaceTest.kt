package app.wallta

import android.media.MediaPlayer
import android.os.Handler
import android.os.Looper
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implements
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.RealObject
import java.io.File
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, shadows = [NativeMediaSurfaceTest.VideoSink::class])
class NativeMediaSurfaceTest {
    // Robolectric's latest audio-service proxy references Bluetooth types absent on API 28.
    // This sink models the asynchronous codec boundary and records playback/release calls.
    @Implements(MediaPlayer::class)
    class VideoSink {
        @RealObject lateinit var player: MediaPlayer
        var released = false
        var playing = false
        var loops = false
        var left = 1f
        var right = 1f
        private var prepared: MediaPlayer.OnPreparedListener? = null
        companion object { var latest: VideoSink? = null }
        @Implementation protected fun __constructor__() { latest = this }
        @Implementation protected fun setSurface(surface: android.view.Surface?) {}
        @Implementation protected fun setDataSource(path: String) {}
        @Implementation protected fun setVolume(a: Float, b: Float) { left = a; right = b }
        @Implementation protected fun setLooping(value: Boolean) { loops = value }
        @Implementation protected fun setVideoScalingMode(mode: Int) {}
        @Implementation protected fun setOnPreparedListener(listener: MediaPlayer.OnPreparedListener?) { prepared = listener }
        @Implementation protected fun setOnErrorListener(listener: MediaPlayer.OnErrorListener?) {}
        @Implementation protected fun prepareAsync() { Handler(Looper.getMainLooper()).postDelayed({ if (!released) prepared?.onPrepared(player) }, 50) }
        @Implementation protected fun start() { check(!released); playing = true }
        @Implementation protected fun release() { released = true; playing = false }
    }
    private fun holder(): android.view.SurfaceHolder {
        val surface = android.view.Surface(android.graphics.SurfaceTexture(0))
        return java.lang.reflect.Proxy.newProxyInstance(android.view.SurfaceHolder::class.java.classLoader, arrayOf(android.view.SurfaceHolder::class.java)) { _, method, _ ->
            if (method.name == "getSurface") surface else null
        } as android.view.SurfaceHolder
    }
    @Test fun videosLoopMutedAndReleaseCodecWhenHidden() {
        val file = File.createTempFile("native-motion", ".mp4")
        val surface = NativeMediaSurface(holder())
        var ready = 0
        val errors = mutableListOf<String>()
        try {
            surface.show(file, "video", null, null, "fill", { ready++ }, { errors += it })
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
            val decoder = VideoSink.latest!!
            assertEquals(emptyList<String>(), errors)
            assertEquals(1, ready)
            assertTrue(decoder.playing)
            assertTrue(decoder.loops)
            assertEquals(0f, decoder.left)
            assertEquals(0f, decoder.right)
            surface.stop()
            assertTrue(decoder.released)
            assertFalse(decoder.playing)
        } finally { surface.stop(); file.delete() }
    }
    @Test fun stoppingDuringPreparationCannotStartHiddenVideo() {
        val file = File.createTempFile("native-motion", ".mp4")
        val surface = NativeMediaSurface(holder())
        var ready = false
        try {
            surface.show(file, "video", null, null, "fill", { ready = true }, { fail(it) })
            surface.stop()
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1))
            assertFalse(ready)
            assertTrue(VideoSink.latest!!.released)
            assertFalse(VideoSink.latest!!.playing)
        } finally { surface.stop(); file.delete() }
    }
}
