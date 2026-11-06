# Wallta 0.1.10

[Download for Windows and Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.10) · [Quick start](quick-start.md) · [In-app updates](updating.md)

Built from [a3e3e6a](https://github.com/mawta/wallta/commit/a3e3e6a9e462392addac3d6f513cf2dd37768a8d) by the [signed release workflow](https://github.com/mawta/wallta/actions/runs/41377868789).

## Changes

Photos open in a large preview with fit/100% viewing and previous/next controls. Newly upscaled photos support an aligned Before/After divider controlled by mouse, touch, keyboard or TV D-pad. The full viewer reads saved 4K output, while originals load on demand using persisted source/photo identities. Existing collections without associations remain viewable as results only. [Controls and original availability](photo-preview.md).

## Downloads

| File | Size | SHA-256 |
|---|---:|---|
| latest.json | 1,261 B | `96e422dc552bff97a27b3550c43a70a0e346958c6056961be58dd68119e61fdd` |
| SHA256SUMS | 362 B | `db3255f5f5a053f3cde90e438545a991106571acbbe6f938281651c7e5684539` |
| Wallta_0.1.10_android.apk | 3,137,600 B | `090f47ef37a7a7af81b5aab0c8b0fce61ffd188c74f6d0b05d442b5149657ee6` |
| Wallta_0.1.10_x64-setup.exe | 4,822,296 B | `baddc19e575d08b9b79f72685846985dbfda1291c5ed7dcc63d76a90f5213712` |
| Wallta_0.1.10_x64-setup.exe.sig | 436 B | `7e75de4e902569ba630a8b7dc4bd32808cff66ad39fc953cd6ed14b61f5a9b2f` |

All five public assets were downloaded again. Exact inventory, SHA-256 checksums and the Windows update signature passed verification. The unauthenticated latest manifest returns **0.1.10 / Android versionCode 11**. APK signature, existing signing certificate, package app.wallta and minSdk 26 were verified; all 29 bundled web assets match the production build after HTML/SVG line-ending normalization. Local copies: output/installers/0.1.10/.

## Verification

- **170 Rust tests** (153 library and 17 behavior tests) and Windows installer build passed in [Windows CI](https://github.com/mawta/wallta/actions/runs/41377320684).
- **122 Android tests**, zero failures or skips, and APK build passed in [Android CI](https://github.com/mawta/wallta/actions/runs/41377320657).
- **107 frontend tests**, production build and **12 release-tool tests** passed. Browser checks covered mouse/touch, 100% zoom/pan, TV D-pad focus, English/Vietnamese and result-only viewing using explicit fixtures.
- Regression tests cover durable associations after reopening, disabled sources, missing originals, saved 4K bytes/dimensions, local ID compatibility, refreshed Photos URLs and clearing cache during a preview request.
- Spec and independent code review reported no important findings. [Merged change](https://github.com/mawta/wallta/pull/8).

Real-device install/upgrade, live Google credentials, TV WebView/remote acceptance and wallpaper application were not rerun. Windows installers are not Authenticode-signed; Wallta updater signatures are verified separately.

[0.1.9 archive](artifacts-0.1.9.md) · [Verification matrix](testing.md)
