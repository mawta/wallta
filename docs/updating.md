# Cập nhật Wallta

Mở **Cài đặt → Cập nhật** để kiểm tra phiên bản, bấm **Tải bản mới**, rồi **Cài đặt**. Wallta tự kiểm tra tối đa mỗi ngày khi mở app; không tự tải hoặc cài. Bản cập nhật lấy từ [GitHub Releases công khai](https://github.com/mawta/wallta/releases/latest), không cần đăng nhập và bỏ qua bản prerelease.

Windows xác minh chữ ký bộ cài và phiên bản trước khi mở trình cài đặt. Chữ ký cập nhật không phải Authenticode; Windows vẫn có thể cảnh báo nhà phát hành chưa xác định.

Android/TV 8+ xác minh dung lượng, SHA-256, package, phiên bản và khóa ký APK. Nếu được hỏi, cho phép Wallta cài ứng dụng, trở lại app và bấm **Tiếp tục cài**, rồi xác nhận trong trình cài của Android. Giữ app mở khi tải.

## Chuyển từ 0.1.1

Cài thủ công [bản 0.1.2](https://github.com/mawta/wallta/releases/tag/v0.1.2) một lần để có chức năng cập nhật trong app.

APK debug cũ dùng khóa ký khác nên có thể phải gỡ trước khi cài bản mới. **Gỡ app sẽ xóa cài đặt và ảnh đã nhập vào app; sao lưu trước.** Từ 0.1.2 dùng khóa ký cố định để những lần nâng cấp tiếp theo giữ dữ liệu.

## Publishing a release

Use the public `mawta/wallta` repository for source and release downloads. Set these repository Actions secrets: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `WALLTA_ANDROID_KEYSTORE_BASE64`, `WALLTA_ANDROID_STORE_PASSWORD`, `WALLTA_ANDROID_KEY_ALIAS`, and `WALLTA_ANDROID_KEY_PASSWORD`. Keep encrypted backups of both signing keys and their passwords outside the repository; changing the Android key prevents normal upgrades, and changing the updater key requires a migration.

Keep the application version aligned in `package.json`, both application entries in `package-lock.json`, the Wallta entries in `src-tauri/Cargo.toml` and `Cargo.lock`, `src-tauri/tauri.conf.json`, and `android/app/build.gradle.kts`. Increment Android `versionCode` for every release; 0.1.3 uses code 4. Keep `minSdk` at 26.

Run `node --test scripts/release.test.mjs` and `node scripts/release.mjs privacy`. Push an existing stable tag such as `v0.1.2`, or run **Signed stable release** with that tag. The workflow checks out the tag, runs frontend/native tests, builds a signed Windows x64 NSIS installer and Android APK, and verifies the compiled APK identity. Normal pull-request builds need no signing secrets.

After both builds succeed, the workflow verifies the Windows signature against the embedded public key, generates `latest.json` and `SHA256SUMS`, creates a draft, and uploads exactly five files: the installer, its `.sig`, the APK, `latest.json`, and `SHA256SUMS`. It downloads those assets again and verifies their inventory and contents before publishing the stable release as latest. Existing releases and assets are never overwritten. If a run leaves a draft, inspect and remove that draft before rerunning the same tag.

The manifest endpoint is [latest.json](https://github.com/mawta/wallta/releases/latest/download/latest.json). `SHA256SUMS` covers both binaries, the Windows signature and the manifest. Updater signatures authenticate Windows installers; the Android package installer enforces the fixed APK signing key.
