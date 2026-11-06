# Wallta 0.1.13

[Download for Windows and Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.13) · [Quick start](quick-start.md) · [In-app updates](updating.md)

Built from [15cb0f7](https://github.com/mawta/wallta/commit/15cb0f7c113010d17a86ec1c97d0f343c06afa3d) by the [signed release workflow](https://github.com/mawta/wallta/actions/runs/42206016570).

## Changes

Fix Google Drive folder selection returning HTTP 400 in the Android APK, including Android TV after QR sign-in from another device. Android JSON parsing previously turned the first-page JSON null into the literal `"null"`. The bridge now sends an empty first-page token and preserves real continuation tokens. Existing Google connections, saved folders and app data are preserved; no Apps Script changes or reconnection are required.

## Downloads

| File | Size | SHA-256 |
|---|---:|---|
| latest.json | 1,183 B | `3ac124d09c7b6d03ef077930a32673fcbad0f279455c7321f5533df5170a5d60` |
| SHA256SUMS | 362 B | `b5736f0498a93f1fb2066b9a28887d357f6e505e5bf9bc9bf79cde13919c99d9` |
| Wallta_0.1.13_android.apk | 3,945,856 B | `77ff5bad65cdabcaf0b9bd5d721137a1bbe1b7b6c4f23c447b85356fc28a50fe` |
| Wallta_0.1.13_x64-setup.exe | 4,830,753 B | `8ccdb89d281b78670187e40e3b4cc39451a0fdd6cf01c4dedec090c6bc33655d` |
| Wallta_0.1.13_x64-setup.exe.sig | 436 B | `85838164455cdf2b5b0da9ed4abdaa60efb3c3361d0e14c0541d00b59bee850e` |

The release workflow downloaded and verified all five assets before publishing. Local verification of the published inventory, SHA-256 checksums and Windows updater signature passed. The unauthenticated latest manifest returns **0.1.13 / Android versionCode 14**. The published APK matches the separately verified signed build, with package `app.wallta`, minSdk `26`, and the same certificate as the previous stable release. All 29 bundled web assets and the LAN page match the production sources after HTML/SVG line-ending normalization. Local copies: `output/installers/0.1.13/`.

## Verification

- **154 Android tests**, zero failures or skips, and APK build passed in [Android CI](https://github.com/mawta/wallta/actions/runs/42206009154). The signed release workflow also tested and assembled the production APK. Locally, 150 tests passed on SDK 26/28; four existing FileProvider tests require Unix path separators and were skipped on Windows. Local debug APK build passed.
- **172 Rust tests** (155 library and 17 behavior tests), zero failures or skips, and Windows updater installer build passed in the release workflow. **115 frontend tests**, production build, **17 Apps Script protocol tests** and **12 release-tool tests** passed.
- Before the fix, the Android JSON regression failed on SDK 26/28. Live Google requests confirmed the exact old first-page request returned HTTP 400, while the corrected request returned HTTP 200. Root, shared, nested and real continuation-page requests succeeded. The temporary test connection was disconnected afterward.
- Independent review passed after correcting the broker test fixture and release lockfile version.

Installation/upgrade and remote-control acceptance on a physical Android phone or TV were not rerun; no ADB device was connected. Google verification remains pending. Windows installers are not Authenticode-signed; updater signatures are verified separately. [Google setup and data handling](setup-google.md).

[0.1.12 archive](artifacts-0.1.12.md) · [Verification matrix](testing.md)
