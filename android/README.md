# Wallta Android host

This app packages the shared React build inside the APK. It never loads a hosted website. Android 8.0/API 26 is the minimum; a reasonably current Android System WebView with origin-scoped WebMessageListener support is required.

## Build

Use Node 22, JDK 17, Android SDK platform 35/build-tools 35.0.0, and Gradle 8.14. The CI workflow provisions these tools using their official setup actions; a Gradle wrapper binary is intentionally not vendored.

```sh
npm ci
npm test -- --run
npm run build
node scripts/prepare-android.mjs
cd android
gradle testDebugUnitTest assembleDebug
```

Debug APK: `app/build/outputs/apk/debug/app-debug.apk`. CI uploads it as `Wallta-Android-debug`. This debug APK is for sideload testing, not a signed Play Store release.

Stable GitHub Releases from 0.1.2 use a fixed signing key and support **Settings → Updates**. Release builds require the four signing environment variables described in [the update guide](../docs/updating.md); debug keys are never used for release. Moving from an older debug APK may require uninstalling it, which removes settings and app-owned imports. Back up those files first.

## Platform behavior

- Phone and TV launchers are supported. The app does not implement or replace the TV Home launcher.
- WorkManager rotates static Home wallpapers at a minimum of 15 minutes. Doze, OEM battery controls and system scheduling can delay a run. Set running and autostart preferences before reboot. A currently executing WallpaperManager binder operation finishes before pause is acknowledged; provider downloads happen outside the state lock.
- The DreamService has its own timer and does not require the activity to stay open. Select Wallta in the system Screensaver menu. Standard non-interactive DreamService behavior exits on a remote key or touch.
- Wallpaper support is conditional on firmware and launcher. A positive Android API capability does not prove the TV Home UI will display the wallpaper; visually confirm on the device. Screensaver settings can also be hidden by OEM firmware.
- SAF folders support USB and recursive DocumentsProviders. No all-files permission is requested. The firmware must provide ACTION_OPEN_DOCUMENT_TREE; if it does not, use an online source or install a compatible DocumentsProvider file manager. Reconnect the same USB device after boot.
- Local/direct/JSON/WebDAV/Drive sources support downloaded-image fallback. Image bodies are capped at 25 MB, manifests at 4 MB/10,000 entries, folders at 1,000 and traversal depth at 16. Decoding is sampled to bound bitmap memory. WebDAV credentials require HTTPS. Public HTTP images and manifests are permitted for LAN sources; the WebView cannot access them directly.
- The UI WebView preview and native ImageView Screensaver receive only JPEG derivatives bounded to a 2048-pixel longest edge, with native decoding sampled to at most 4,194,304 pixels before scaling. The Screensaver creates no WebView and owns its displayed bitmaps on the UI thread. Original source/import files remain untouched. Original and derivative cache bytes both count against the configured cache limit. Unavailable SAF/USB roots preserve the prior catalog and allow downloaded-image fallback.
- Provider credentials are attached only within the active provider's origin and media path. Committing a new relay nonce/base invalidates the source catalog and owned cache atomically. Each wallpaper/screensaver operation attempts at most 20 distinct candidates, skips unreadable images, and consumes rotation state only for the selected success; cancellation prevents another candidate from starting.
- Changing source recursion, enabled state, credentials, kind or location invalidates its catalog and owned image/preview caches in the same locked commit. Omitted secrets are merged before comparison. Recursion and credential edits preserve an existing OAuth grant; source removal or kind/location changes retain the separate disconnect lifecycle.
- Google credentials, tokens, relay capabilities and native source credentials are encrypted with an Android Keystore AES/GCM key and never returned by the bridge. Omitted credential properties preserve existing values; an explicit empty string clears them. Removing a source removes owned cache entries and unused SAF grants.
- OAuth starts in a separate pending encrypted record. Only saving a matching, ready source commits it. Closing a source form cancels pending HTTP requests and invalidates their nonce so late responses cannot re-create credentials. Saved grants survive form cancellation. Pending sessions expire after one hour and are capped at 32.
- On phones with Google Play services, connecting Google first opens Android's Google account chooser, then Play Services authorization for Drive readonly or Photos Picker access. Background renewal uses the saved account without opening consent UI. Register the APK package and signing certificate as an Android OAuth client; the stable release client does not authorize a debug-signed APK. No Web OAuth client secret or refresh token is embedded in the APK.
- TVs and devices without supported Play Services use the bundled Apps Script broker URL and QR sign-in. The broker holds each consenting user's Google refresh grant; the app stores only its connection capability and short-lived access token in encrypted native storage. Drive images and Picker selections are fetched directly from Google. Polling is limited to once per five seconds and pairing expires after ten minutes. Cancelling an unsaved connection revokes both its pairing and delivered capability; closing a saved editor preserves its connection.
- Drive accounts can be saved before choosing folders, then reused for multiple accessible folders. Google Photos uses Google's Photos Picker, followed by a managed local import of up to 50 images/256 MB, at most 25 MB per image. Imports request images bounded to 4096 pixels per dimension and validate image containers and decoded pixels. Retrying or cancelling preserves the previously saved import; Clear cache preserves its originals. The separate system image picker remains an explicit file-import alternative.
- Existing saved Laravel Drive relay and Photos Ambient sources retain their legacy provider paths. Legacy relay grants remain restricted to their approved single folder. Legacy Ambient listings retain their daily request ceiling, online requirement, one-hour image-cache lifetime and cache-purge behavior.

## Lock and motion wallpaper

Home and Lock each save their own source, automatic toggle and interval. A blank Home source uses all enabled sources. Lock runs independently of Home, uses a still poster for GIF/video, and is disabled on TV firmware. Android WorkManager intervals remain at least 15 minutes and may be delayed by power management.

Enable Animated wallpaper, then confirm Wallta in Android Live Wallpaper settings for native muted looping MP4/WebM and GIF playback on Home. The native TV screensaver uses the same animated toggle. Turning it off displays a still at the actual surface resolution; GIF/video preview thumbnails remain bounded JPEGs. Hidden/destroyed surfaces cancel pending requests and release their codecs. Removing or editing a source revalidates active playback.

Native cached video downloads stream to disk and are capped at 128 MB. Image limits remain 25 MB/120 MP (64 MB for managed upscale files), animated GIF playback is capped at 8 MP, and video dimensions at 8192 pixels/32 MP. Visible decoders pin their active files against normal cache eviction. Native codec/firmware support determines MP4/WebM playback; Android 8.0 poster extraction above 4 MP requires a newer OS. Device acceptance must cover the system chooser, GIF/video motion, muted looping, source revocation, Lock-only rotation with Home stopped, and teardown while a download or codec is preparing.

## Required device acceptance checks

1. Install APK on an Android 8+ phone and TV. Open the packaged UI offline. Verify D-pad focus, Back, scrolling, and a connected keyboard.
2. Choose a local/USB folder, including nested files; preview and set wallpaper. Remove/reinsert USB and confirm readable errors. Reopen after process death and verify the persisted grant.
3. Set shuffle with three images, cycle and restart mid-cycle; verify no repeat until all are used. Test sequential, overnight weekday schedule, Wi-Fi only, source removal, and clear cache.
4. Start automatic rotation, close the UI, wait for an OS-scheduled update, reboot, update the APK, then pause while a slow provider is downloading. Confirm no later wallpaper apply after pause is acknowledged. Confirm that failure is displayed rather than recorded as success.
5. Select Wallta in Screensaver settings; verify interval, fit/stretch, blurred letterbox background, clock, remote-key exit and no requests after DreamService teardown. Confirm TV Home wallpaper independently for every firmware family.
6. With real provider fixtures, verify JSON relative URLs, WebDAV Basic over TLS, Drive paging/nesting and expired token handling. On a release-signed phone, choose between Google accounts, save before folder selection, reopen, select multiple folders, and verify silent renewal. Verify denied consent produces a reconnect action without background UI.
7. On TV, complete Apps Script QR sign-in and verify pairing expiry, unsaved cancellation and source-removal disconnection. Import a Google Photos Picker selection on phone and TV, then reselect/cancel and confirm the saved selection survives. Verify imported images work offline and source removal deletes only its owned originals. Keep an existing relay/Ambient fixture to check backward compatibility.

Pure JVM policy tests cover scheduling (including overnight weekdays), complete shuffle cycles, sequential wrap, relative manifest parsing, unsafe URL filtering and entry bounds. They do not validate firmware behavior, WebView rendering, Keystore, SAF, WorkManager timing, Google consent or the live relay.
