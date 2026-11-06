# Wallta 0.1.12

[Download for Windows and Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.12) · [Quick start](quick-start.md) · [In-app updates](updating.md)

Built from [fe8e6e3](https://github.com/mawta/wallta/commit/fe8e6e3524fc440f8c7ef57ee57548d66ffe77d9) by the [signed release workflow](https://github.com/mawta/wallta/actions/runs/42202687298).

## Changes

Android phones with Google Play services use the system Google account chooser; Android TV and devices without Play services use the preconfigured Apps Script QR service. Users no longer enter a service URL or client credentials. Save a Drive account before choosing multiple folders, then change folders under Advanced. Google Photos Picker imports selected images locally, with asynchronous import polling and cancellation that preserves saved collections. Google verification remains pending.

## Downloads

| File | Size | SHA-256 |
|---|---:|---|
| latest.json | 1,332 B | `c9b1bce411dc440c3fc45cd63642781e5438d82d2766cf7f94b2f52861698dc0` |
| SHA256SUMS | 362 B | `52a884894655bc299e946f096f90cb052bffcdd0f37d06cab0169e43b737dfcc` |
| Wallta_0.1.12_android.apk | 3,945,420 B | `c22b8786e6d66169f47e17cd5e8823c39a313afde2a66b97eaa723cec78e5010` |
| Wallta_0.1.12_x64-setup.exe | 4,830,704 B | `c886acb5e6836e90f8d7bd979aa853e25a9548389a441317833369083ededb97` |
| Wallta_0.1.12_x64-setup.exe.sig | 436 B | `a743c0e601760706438f135458949d7756120d4f66edcad8e18fc42be1773737` |

The release workflow downloaded and verified all five assets before publishing. Local inventory, SHA-256 checksums and the Windows updater signature passed verification. The unauthenticated latest manifest returns **0.1.12 / Android versionCode 13**. The APK signature matches the prior stable release certificate and registered Android OAuth client; package app.wallta and minSdk 26 were verified. All 29 bundled web assets match the production frontend after HTML/SVG line-ending normalization. Local copies: output/installers/0.1.12/.

## Verification

- **172 Rust tests** (155 library and 17 behavior tests), zero failures or skips, and Windows installer build passed in the signed release workflow.
- **148 Android tests**, zero failures or skips, and local debug APK build passed. CI also tested and assembled the signed release APK.
- **115 frontend tests**, production build, **17 Apps Script protocol tests** and **12 release-tool tests** passed.
- Live service checks completed Drive and Photos Google consent callbacks and access-token delivery, listed Drive folders, and created/deleted a Photos Picker session. Temporary test grants were removed. Production Kotlin/OkHttp pair/poll/cancel transport passed against the deployed service.
- Independent spec and quality reviews passed after fixing import timeout recovery, stale completion polling and replacement cancellation.

Release-signed native chooser/consent, full Photos selection/import on an Android phone or TV, install/upgrade, wallpaper and reboot behavior still require physical-device acceptance. No ADB device was connected. Google consent can show an unverified-app warning. Windows installers are not Authenticode-signed; updater signatures are verified separately. [Google setup and data handling](setup-google.md).

[0.1.11 archive](artifacts-0.1.11.md) · [Verification matrix](testing.md)
