# Updating Wallta

Open **Settings → Updates** to check your version and look for an update. Wallta also checks once a day while the app is open. Checks use the public [stable GitHub Release](https://github.com/mawta/wallta/releases/latest); no login is required. Prereleases are excluded. Wallta downloads and installs only when you select the update action.

On Windows, Wallta verifies the installer with its embedded updater public key, including the signed version, before opening the installer. These updater signatures do not provide Windows Authenticode signing: Windows may show an unknown publisher warning.

On Android 8.0 (API 26) and later, Wallta verifies the APK size, SHA-256, package, newer version and signing identity before asking Android to install it. Android may ask you to allow Wallta to install apps. Finish installation in the system prompt. Keep the app open while downloading.

## Moving from 0.1.1

Install [0.1.2 manually](https://github.com/mawta/wallta/releases/tag/v0.1.2) once to enable in-app updates. The older app cannot update itself.

Older Android debug APKs use a different signing key. Android may require uninstalling that app before installing the signed release. Uninstalling removes app settings and app-owned imported photos; preserve any photos and settings you need first. Releases from 0.1.2 use one fixed release key so future upgrades preserve app data.

## Publishing a release

Use the public `mawta/wallta` repository for source and release downloads. Set these repository Actions secrets: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `WALLTA_ANDROID_KEYSTORE_BASE64`, `WALLTA_ANDROID_STORE_PASSWORD`, `WALLTA_ANDROID_KEY_ALIAS`, and `WALLTA_ANDROID_KEY_PASSWORD`. Keep encrypted backups of both signing keys and their passwords outside the repository; changing the Android key prevents normal upgrades, and changing the updater key requires a migration.

Keep the application version aligned in `package.json`, both application entries in `package-lock.json`, the Wallta entries in `src-tauri/Cargo.toml` and `Cargo.lock`, `src-tauri/tauri.conf.json`, and `android/app/build.gradle.kts`. Increment Android `versionCode` for every release; 0.1.2 uses code 3. Keep `minSdk` at 26.

Run `node --test scripts/release.test.mjs` and `node scripts/release.mjs privacy`. Push an existing stable tag such as `v0.1.2`, or run **Signed stable release** with that tag. The workflow checks out the tag, runs frontend/native tests, builds a signed Windows x64 NSIS installer and Android APK, and verifies the compiled APK identity. Normal pull-request builds need no signing secrets.

After both builds succeed, the workflow verifies the Windows signature against the embedded public key, generates `latest.json` and `SHA256SUMS`, creates a draft, and uploads exactly five files: the installer, its `.sig`, the APK, `latest.json`, and `SHA256SUMS`. It downloads those assets again and verifies their inventory and contents before publishing the stable release as latest. Existing releases and assets are never overwritten. If a run leaves a draft, inspect and remove that draft before rerunning the same tag.

The manifest endpoint is [latest.json](https://github.com/mawta/wallta/releases/latest/download/latest.json). `SHA256SUMS` covers both binaries, the Windows signature and the manifest. Updater signatures authenticate Windows installers; the Android package installer enforces the fixed APK signing key.
