# Wallta 0.1.8

[Download for Windows and Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.8) · [English introduction](../README.md) · [Quick start](quick-start.md) · [In-app updates](updating.md)

Built from [`e01fdd3`](https://github.com/mawta/wallta/commit/e01fdd3afeb0e92cd09a4dd469093838b702d26b) by the [signed release workflow](https://github.com/mawta/wallta/actions/runs/41038816691). Stable release published on October 30, 2026.

## Downloads

| File | Size | SHA-256 |
|---|---:|---|
| SHA256SUMS | 359 B | `683809e7c3350251b93ee807e7f6de87011ccb81f845f212a64b42380f5a9f4b` |
| Wallta_0.1.8_android.apk | 3,130,088 B | `0ec13e3db66cced7aed86177aee2065e18cb30e294a58eb7957ae5be251ce7e8` |
| Wallta_0.1.8_x64-setup.exe | 4,782,520 B | `a2e74b75fb0c3b78811b486e6771c4a763135881b3456eebc649d3fa0f581098` |
| Wallta_0.1.8_x64-setup.exe.sig | 436 B | `2213400528333f0771ed9f17f49cf304fa62131d5ca970d79443b32408292feb` |
| latest.json | 1,227 B | `1bba57926a76dae06d81dc6bf1f3b45d3fec3b95d806ec3f6c5067ae8eb56148` |

All five public assets were downloaded again. Exact inventory, SHA-256 checksums and the Windows update signature passed verification. The unauthenticated latest manifest returns **0.1.8 / Android versionCode 9**. APK package `app.wallta`, minSdk 26 and the existing signing certificate (`a36f8b9fea83ce87452189e5c51d0e145ceda0c87450cb047e4bb7a11cd329eb`) were verified. All **29 web assets** match the production build after HTML/SVG line-ending normalization; the bundled bilingual LAN page matches its source. Local installer copies are in `output/installers/0.1.8/` on the development machine.

## Changes

- English and Vietnamese interface, following the primary device language by default: Vietnamese for `vi`, English for other languages.
- **Settings → Language** offers System default, English and Tiếng Việt. Switching applies immediately without resetting an open form, and the preference is saved on the device.
- Offline translations cover shared screens, accessibility labels, connection help, update/upscale status and known native diagnostics. User photo/source names, identifiers, URLs and account connections remain unchanged; unknown diagnostics retain their original detail.
- The QR/LAN browser page has its own English/Vietnamese switch and follows the browser language by default. Transfer routes, limits, confirmation and retry behavior are unchanged.
- English README, repository description and discovery topics.

## Verification

- **94 frontend tests** and production build passed. Coverage includes device defaults, invalid/blocked storage, persistent overrides, subscriber updates, mounted switching and form identity, native message interpolation, unknown diagnostic preservation and LAN upload retry across language changes.
- **152 Rust tests** and Windows NSIS build passed in [Windows CI](https://github.com/mawta/wallta/actions/runs/41038165869).
- **111 Android tests**, zero failures or skips, and APK build passed in [Android CI](https://github.com/mawta/wallta/actions/runs/41038166043).
- **12 release-tool tests**, version consistency (0.1.8 / Android versionCode 9), tracked-file privacy and diff checks passed.
- Browser checks covered English/Vietnamese desktop and 390px layouts, selection persistence after reload, translated Google Drive configuration and zero console errors. The mobile dialog stayed within the viewport.
- Independent spec and quality reviews have no remaining important findings. [Merged change](https://github.com/mawta/wallta/pull/6).

The language preference is stored separately from photo/source settings on each device. Native OS dialogs, third-party pages, low-level unknown errors and generated collection names may retain their original language. Real-device install/upgrade, wallpaper/reboot and credentialed Google acceptance tests were not rerun for this interface release. Windows is not Authenticode-signed; Wallta update signatures are verified separately.

[0.1.7 archive](artifacts-0.1.7.md) · [0.1.6](artifacts-0.1.6.md) · [Verification matrix](testing.md)
