# Wallta 0.1.9

[Download for Windows and Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.9) · [Quick start](quick-start.md) · [In-app updates](updating.md)

Built from [01fdac1](https://github.com/mawta/wallta/commit/01fdac1db8245196c3a18c3ff08c33d7ec98d289) by the [signed release workflow](https://github.com/mawta/wallta/actions/runs/41041741518).

## Changes

Fixed saved upscaled collections showing zero images on Windows. The local scanner now uses filesystem extensions instead of treating the question mark in canonical Windows paths as a URL delimiter. Existing images and saved sources remain intact; refresh the library after updating. No reprocessing is needed.

## Downloads

| File | Size | SHA-256 |
|---|---:|---|
| SHA256SUMS | 359 B | `dbe0ae88b54977f217ac2d6d246efe255d5a222d757364485d559eb97b73a9e7` |
| Wallta_0.1.9_android.apk | 3,130,092 B | `d0df417fa5145279c21bca13a0ed2cb32e937f8cd0d9f63d5a1d28b967c7fdb2` |
| Wallta_0.1.9_x64-setup.exe | 4,783,598 B | `29327a0c9f3f163d89e95897e0ef289a2dc6ddf7e656a97531d1173eb42557c4` |
| Wallta_0.1.9_x64-setup.exe.sig | 436 B | `26c39815086c71125f9f6ece73249cd550b024c451caaf70f52e5b2c5a958d74` |
| latest.json | 1,137 B | `3eb0312809ec9ddf33c93d6942e70a5a5f89bc54dfe1095f5effc1226902f14f` |

All five public assets were downloaded again. Exact inventory, SHA-256 checksums and the Windows update signature passed verification. The unauthenticated latest manifest returns **0.1.9 / Android versionCode 10**. APK signature, existing signing certificate, package app.wallta and minSdk 26 were verified; all 29 bundled web assets match the production build after HTML/SVG line-ending normalization. Local copies: output/installers/0.1.9/.

## Verification

- **154 Rust tests** and Windows installer build passed in [Windows CI](https://github.com/mawta/wallta/actions/runs/41040937742). New regressions scan a real canonical folder and save/enable/reopen an upscaled source.
- A Windows regression harness reproduced zero images before the fix and passed afterward. A read-only probe through the production scanner listed **146 existing images** without changing application data.
- **94 frontend tests**, production build and **12 release-tool tests** passed.
- **111 Android tests**, zero failures or skips, and APK build passed in [Android CI](https://github.com/mawta/wallta/actions/runs/41040937519).
- Independent code review reported no important findings. [Merged change](https://github.com/mawta/wallta/pull/7).

Real-device install/upgrade and live wallpaper application were not rerun. Windows installers are not Authenticode-signed; Wallta updater signatures are verified separately.

[0.1.8 archive](artifacts-0.1.8.md) · [Verification matrix](testing.md)
