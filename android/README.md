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

## Platform behavior

- Phone and TV launchers are supported. The app does not implement or replace the TV Home launcher.
- WorkManager rotates static Home wallpapers at a minimum of 15 minutes. Doze, OEM battery controls and system scheduling can delay a run. Set running and autostart preferences before reboot. A currently executing WallpaperManager binder operation finishes before pause is acknowledged; provider downloads happen outside the state lock.
- The DreamService has its own timer and does not require the activity to stay open. Select Wallta in the system Screensaver menu. Standard non-interactive DreamService behavior exits on a remote key or touch.
- Wallpaper support is conditional on firmware and launcher. A positive Android API capability does not prove the TV Home UI will display the wallpaper; visually confirm on the device. Screensaver settings can also be hidden by OEM firmware.
- SAF folders support USB and recursive DocumentsProviders. No all-files permission is requested. The firmware must provide ACTION_OPEN_DOCUMENT_TREE; if it does not, use an online source or install a compatible DocumentsProvider file manager. Reconnect the same USB device after boot.
- Local/direct/JSON/WebDAV/Drive sources support downloaded-image fallback. Image bodies are capped at 25 MB, manifests at 4 MB/10,000 entries, folders at 1,000 and traversal depth at 16. Decoding is sampled to bound bitmap memory. WebDAV credentials require HTTPS. Public HTTP images and manifests are permitted for LAN sources; the WebView cannot access them directly.
- Google credentials, tokens, relay capabilities and native source credentials are encrypted with an Android Keystore AES/GCM key and never returned by the bridge. Omitted credential properties preserve existing values; an explicit empty string clears them. Removing a source removes owned cache entries and unused SAF grants.
- OAuth starts in a separate pending encrypted record. Only saving a matching, ready source commits it. Closing a source form cancels pending HTTP requests and invalidates their nonce so late responses cannot re-create credentials. Saved grants survive form cancellation. Pending sessions expire after one hour and are capped at 32.
- Photos Ambient uses a Google OAuth client of type TVs and Limited Input Devices with the Ambient API enabled and approved for public use. It is available on supported TVs, with QR authorization and album selection on a phone. Phones receive an explicit unsupported/actionable response; Wallta does not claim access to arbitrary Google Photos albums on phones.
- Ambient listings cache for at least 15 minutes with a per-device 240 daily list-request ceiling. Photos are excluded from offline fallback and have a maximum one-hour image-cache lifetime. Disconnect/revocation/deselection/source removal purges owned Photos cache entries. API approval, real Google credentials, and real devices are needed to validate the complete Google flow.
- Private Drive can use an optional HTTPS Laravel OAuth relay, whose pairing capability remains in native encrypted storage. Without a relay, use a public folder API key or manually supplied token. Device OAuth is never incorrectly used for Drive readonly access.

## Required device acceptance checks

1. Install APK on an Android 8+ phone and TV. Open the packaged UI offline. Verify D-pad focus, Back, scrolling, and a connected keyboard.
2. Choose a local/USB folder, including nested files; preview and set wallpaper. Remove/reinsert USB and confirm readable errors. Reopen after process death and verify the persisted grant.
3. Set shuffle with three images, cycle and restart mid-cycle; verify no repeat until all are used. Test sequential, overnight weekday schedule, Wi-Fi only, source removal, and clear cache.
4. Start automatic rotation, close the UI, wait for an OS-scheduled update, reboot, update the APK, then pause while a slow provider is downloading. Confirm no later wallpaper apply after pause is acknowledged. Confirm that failure is displayed rather than recorded as success.
5. Select Wallta in Screensaver settings; verify interval, fit/stretch, blurred letterbox background, clock, remote-key exit and no requests after DreamService teardown. Confirm TV Home wallpaper independently for every firmware family.
6. With real provider fixtures, verify JSON relative URLs, WebDAV Basic over TLS, Drive paging/nesting and expired token handling. With a configured relay, complete private Drive pairing and verify disconnect revocation.
7. With a verified Ambient project, complete QR auth, observe slow-down/expiry behavior, choose albums, refresh tokens, revoke access in Google, and confirm Photos cache purges. Verify no Photos image is shown after network loss or local disconnect.

Pure JVM policy tests cover scheduling (including overnight weekdays), complete shuffle cycles, sequential wrap, relative manifest parsing, unsafe URL filtering and entry bounds. They do not validate firmware behavior, WebView rendering, Keystore, SAF, WorkManager timing, Google consent or the live relay.
