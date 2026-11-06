# Wallta 0.1.11

[Download for Windows and Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.11) · [Quick start](quick-start.md) · [In-app updates](updating.md)

Built from [7e41233](https://github.com/mawta/wallta/commit/7e412338608e0c1d2c7d4c76e6f2c6cef2ebbded) by the [signed release workflow](https://github.com/mawta/wallta/actions/runs/42193257407).

## Changes

Official Windows builds connect Google Drive and Google Photos without asking for client credentials. Optional folder and manual access controls are grouped under Advanced. The imported Photos card separates the account, collection and reselect action with clearer typography. Picker sessions respect polling/expiration, recover from temporary connection failures and retain a browser link if opening fails. Google verification is pending; [platform differences and custom builds](setup-google.md).

## Downloads

| File | Size | SHA-256 |
|---|---:|---|
| latest.json | 1,327 B | `a8f4871d06a7cc8daa13e5fa313d07ad3c656ebb89359d7bb0cb9e33e0362cc6` |
| SHA256SUMS | 362 B | `116cf5b9bffbc9d648ad07902a6b20bda7a82a1da3718b12e67779da5a883dc3` |
| Wallta_0.1.11_android.apk | 3,137,280 B | `9f351b5f663b3c4971b15129ca21abfe83c78cc4667d47e55a2d173e8c98d488` |
| Wallta_0.1.11_x64-setup.exe | 4,831,169 B | `8ec0cc583376165e87a4e3138982ae58a51eadc010d646079a7b8892ee3d84ca` |
| Wallta_0.1.11_x64-setup.exe.sig | 436 B | `e26beba4622aba259554056fe65ba99c45ad3348bd1f61a84402d4a5fb6814b9` |

The release workflow downloaded and verified all five public assets before publishing. Local inventory, SHA-256 checksums and the Windows updater signature passed verification. The unauthenticated latest manifest returns **0.1.11 / Android versionCode 12**. CI verified the APK signature, package app.wallta and minSdk 26. All 29 bundled web assets match the production build after HTML/SVG line-ending normalization. Local copies: output/installers/0.1.11/.

## Verification

- **172 Rust tests** (155 library and 17 behavior tests), zero failures or skips, and Windows installer build passed in [Windows CI](https://github.com/mawta/wallta/actions/runs/42193249833).
- **122 Android tests**, zero failures or skips, and APK build passed in [Android CI](https://github.com/mawta/wallta/actions/runs/42193249757).
- **110 frontend tests**, production build and **12 release-tool tests** passed.
- Browser checks covered English/Vietnamese and narrow layouts; Windows Google sign-in and Photos import were exercised with a real account during preparation. Independent code review found no Critical or Important issue.

Real-device install/upgrade, live Android/TV Google flows and wallpaper/reboot were not rerun for this release. Windows installers are not Authenticode-signed; Wallta updater signatures are verified separately.

[0.1.10 archive](artifacts-0.1.10.md) · [Verification matrix](testing.md)
