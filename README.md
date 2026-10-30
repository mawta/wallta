# Wallta

Automatic wallpaper rotation for Windows, Android, and Android TV. Build a collection from local folders, cloud storage, or image links, set a schedule, and bring your photos to the desktop or TV screensaver.

[Download the latest release](https://github.com/mawta/wallta/releases/latest) · [Setup guide](docs/quick-start.md) · [Updates](docs/updating.md) · [Builds and checksums](docs/artifacts.md)

## Platforms

| Platform | Requirements and behavior |
|---|---|
| Windows | Windows 10/11 x64 with WebView2. Runs in the system tray; optional start at sign-in. |
| Android | Android 8.0+ (API 26) with a current Android System WebView. Native background wallpaper scheduling, with a minimum interval of 15 minutes. |
| Android TV / TV box | Android 8.0+ with a compatible WebView. TV screensaver and remote navigation; Home wallpaper depends on the firmware and launcher. |

The interface supports **English and Vietnamese**. It follows the device language by default (Vietnamese for `vi`, English otherwise). Choose System, English, or Vietnamese in **Settings → Language**; your choice is saved on the device.

## What you can do

- **Combine photo sources:** multiple folders per local, Google Drive, or WebDAV/NAS source, including supported USB storage. Direct image links and JSON image lists are supported too.
- **Transfer over Wi-Fi:** send and receive photos between a phone and a TV or PC using a QR code or link. Only one device needs Wallta; the other uses a browser on the same LAN. No Google key or Internet connection is needed. [Transfer guide](docs/local-transfer.md).
- **Control rotation:** shuffle through a full cycle before repeating, use sequential order, choose weekdays and overnight time windows, and set fit/fill/stretch, blurred backgrounds, cache limits, or Wi-Fi-only downloads.
- **Upscale to 4K:** use Fast mode on Windows, Android, and TV, or local AI on Windows. Run all eligible photos or select individual images. Results become a separate collection; originals stay untouched. [Upscaling guide](docs/upscaling.md).
- **Import from Google Photos:** use the Picker API on Windows or the system/cloud photo picker on Android when available. TV album access uses the restricted Ambient API; see the Google requirements below.
- **Use a TV screensaver:** select Wallta in the system screensaver settings, with a separate slideshow interval and an optional clock.
- **Manage storage and updates:** clear regenerable cache, remove individual upscaled collections, and check for app updates in Settings.

## Install and get started

1. Open [GitHub Releases](https://github.com/mawta/wallta/releases/latest) and download the Windows x64 `.exe` installer or Android `.apk`. Downloads are public; no Wallta account is required. On Android/TV, allow installation from the app used to open the APK if prompted.
2. Open **Sources**, add a local source, name it, and choose a photo folder. Add more folders or enable scanning subfolders as needed.
3. Choose **Save & apply** to show a photo immediately and enable rotation. Configure the interval, order, and display options in **Schedule & display**.
4. On Windows, enable start at sign-in if desired and keep Wallta running in the tray. On TV, select Wallta in the system **Screensaver** menu and check Home wallpaper support on the device.

Windows may show an unknown-publisher warning: the installer has an update-verification signature but is not Authenticode-signed. Replacing an old debug APK may require uninstalling it because its signing key differs; back up app-owned photos first, as uninstalling removes them and the settings.

**Settings → Updates** checks for a stable release at most once a day when the app opens. Downloading and installing remain your choice. See the [update guide](docs/updating.md) and [quick start](docs/quick-start.md) for details. Most detailed user guides are currently in Vietnamese.

## 4K processing and storage

Fast mode uses the platform's image resampler. Windows AI runs on your computer using a Vulkan-capable GPU and driver, without an AI API key or uploading photos to an AI service.

| Windows AI model | Best suited to | Model download |
|---|---|---:|
| Photo · RealESRGAN x4plus | Photos and general images | 33.54 MB |
| Anime · RealESRGAN x4plus anime 6B | Anime and illustrations | 8.97 MB |
| Anime Light · RealESR AnimeVideo v3 x4 | Anime and animation, smaller model | 1.25 MB |

The shared Windows runtime is **6.34 MB**, downloaded once if missing. Wallta downloads only the selected model and required runtime when you start a job, then verifies file sizes and SHA-256 checksums. Download sizes use decimal MB and exclude network overhead.

**Run all** processes a snapshot of the eligible source images; manual selection is limited to 50 images. Results preserve aspect ratio within a landscape or portrait 4K frame. Upscaling does not guarantee the detail of a native 4K photo; inspect results before applying them.

In **Settings → Storage**, clearing cache keeps saved photos and AI models. Upscaled collections can be deleted individually while preserving the source originals. See [processing limits and storage behavior](docs/upscaling.md).

<a id="nguồn-online"></a>

## Online sources

A normal web folder link does not provide a standard way to list its images. Use WebDAV or serve a JSON list at a URL:

```json
{"images":["a.jpg",{"url":"sub/b.webp","name":"Mountain evening"}]}
```

Relative image URLs resolve against the JSON URL. A direct image URL also works. Wallta reads the originals and creates separate render/cache files.

## Google setup and platform limits

Drive and Windows/TV Photos integrations require your own Google Cloud credentials; none are bundled. Android's system photo picker does not need a Client ID. Public Drive folders use an API key. Private Drive uses desktop OAuth on Windows or an optional self-hosted HTTPS [Laravel relay](relay/README.md) on Android/TV. There is no preconfigured public relay. Saved Windows accounts can be reused when choosing or changing folders. See the [Google setup guide](docs/setup-google.md).

Google Photos imports on Windows and Android create local copies of selected photos; they **do not sync an album**. TV album access through Ambient requires approval in Google's Photos Partner Program and has not been validated with an approved project and a real TV. An ordinary Cloud project alone does not unlock Ambient.

Android's background wallpaper interval is at least **15 minutes**, and battery-saving policies can delay it. Slideshow/screensaver timing is separate. TV Home wallpaper and screensaver settings depend on the firmware and launcher; support is not guaranteed on every TV. Wallta does not replace the Home launcher. Devices with an outdated WebView may need an update before the app can run.

On Windows, closing the window hides Wallta in the tray; **Quit** stops the scheduler. Device behavior, Google credentials, and LAN/firewall conditions still require real-world checks. The [verification matrix](docs/testing.md) distinguishes automated results from outstanding device acceptance tests.

## Development

The shared interface uses React and TypeScript. Windows uses Tauri/Rust; Android/TV uses Kotlin with the React build packaged inside the APK. Native schedulers run independently of the interface. The optional Drive relay uses Laravel.

```sh
npm ci
npm test -- --run
npm run dev
```

The web preview opens at `http://127.0.0.1:1420`. It does not change system wallpaper or process upscaling. Online sources need CORS support, and local folders must be selected again after reloading the page.

For a production frontend build, run `npm run build`.

**Windows:** install Rust stable, Visual Studio C++ Build Tools, and WebView2, then run:

```sh
npm run tauri -- dev
# or build the NSIS installer
npm run tauri -- build
```

The installer is written to `src-tauri/target/release/bundle/nsis/`.

**Android/TV:** use Node 22, JDK 17, Gradle 8.14, and Android SDK 35/build-tools 35.0.0:

```sh
npm run build
node scripts/prepare-android.mjs
cd android
gradle testDebugUnitTest assembleDebug
```

See the [Android host README](android/README.md) for APK paths, signing, and device checks, and the [relay README](relay/README.md) for optional server setup. GitHub Actions workflows cover Windows, Android, the relay, and signed stable releases. [Build records and checksums](docs/artifacts.md) document the published installers.
