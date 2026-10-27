# Kiểm chứng Wallta 0.1

Đối chiếu mã nguồn và bằng chứng thực thi; CI build không thay thế kiểm tra trên thiết bị. Đây là bản thử nghiệm có bộ cài Windows chưa ký chứng thư và APK ký debug.

## Đã thực hiện

| Phạm vi | Kết quả / bằng chứng |
|---|---|
| React/TypeScript | 17 Vitest tests đạt, production Vite build đạt |
| Thao tác trình duyệt | Thêm nguồn JSON thật 2 ảnh, Next, trình chiếu/ArrowRight/Escape, lưu lịch 300 giây và tải lại đều đạt |
| Bố cục | Đã xem ảnh chụp desktop 1440×900, mobile 390×844 và TV 1920×1080; mobile không tràn ngang |
| Bàn phím/D-pad web | Đổi mục sidebar bằng mũi tên; rời input bằng Up/Down và select bằng Left/Right; focus hiển thị |
| Lỗi lưu/hủy OAuth UI | Mounted React regression tái hiện đóng lúc đang lưu, sau sửa không đóng qua nút/Escape cho tới khi xong |
| Windows native | CI chạy Rust selection/schedule/parsers/cacheownership/offline/cancellation và OAuth PKCE/state; NSIS build đã thành công. Lượt cuối được ghi trong GitHub Actions |
| Android native | CI compile Kotlin, chạy policy JVM tests và tạo debug APK. Phần hardware cần kiểm tra bên dưới |
| Relay Laravel | 12 tests / 64 assertions đạt trên PHP 8.4, Google HTTP fake; PKCE/state, credential mã hóa, scope thư mục, refresh/disconnect, cancel và limiter riêng |
| Relay race review | Hai callback đồng thời: bên thua 410; hủy khi token exchange đang chạy: callback 410, không tạo lại pair; alias root chuyển ID chuẩn |
| Dependency audit | npm và Composer audit không tìm thấy advisory tại thời điểm chạy 27/10/2026 |

Test trước sửa đã quan sát được lỗi: pause bị chặn bởi download; preview ghi sau pause; WebDAV bỏ qua thư mục con; đóng nguồn trong lúc lưu; relay dùng chung rate limit; chưa có cancel endpoint. Lịch sử commit và log CI giữ bằng chứng RED/GREEN.

Ảnh chụp, fixture và bộ cài nằm ở `output/`, được Git bỏ qua. Không dùng ảnh giả để báo số lượng thư viện; khung cảnh SVG khi chưa có ảnh được ghi rõ là minh họa. Ảnh kiểm thử được tạo từ SVG của dự án.

## Cần thiết bị/credential thật

- Windows 10/11: cài NSIS, thử Wallpaper API, khay hệ thống, autostart, nhiều màn hình, khóa màn hình và reboot. Test tự động không gọi thay hình nền của máy người dùng.
- Android 8+ phone: SAF/USB tháo cắm, quyền đọc sau process death, Keystore, chọn cloud Photos và import; background trong Doze/reboot; xóa nguồn khi tải chậm.
- Android TV: D-pad và Back trong WebView, menu Screensaver, DreamService qua đêm/remote exit, ảnh lớn trên TV ít RAM; kiểm tra Home wallpaper riêng cho từng firmware/launcher.
- Google: OAuth project thật, consent/refresh/revoke, quota và URL hết hạn; Photos Ambient phải được Google Partner Program chấp thuận. Chưa có project/credential trong repo.
- Relay: chạy HTTPS thật, proxy/session cookie/cache bền, hoàn tất QR từ điện thoại, TV poll và ngắt kết nối. Chưa triển khai server.

Không có kết luận “hoạt động trên mọi TV Android 8+” khi chưa thử firmware. Một số TV cần cập nhật WebView hoặc DocumentsProvider; một số launcher không hiển thị wallpaper dù Android trả thành công.

## Lặp lại kiểm tra

```sh
npm ci
npm test -- --run
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri -- build
node scripts/prepare-android.mjs
# JDK17 + SDK35 + Gradle8.14
cd android
gradle testDebugUnitTest assembleDebug
```

Relay riêng: `cd relay`, `composer install`, `vendor/bin/phpunit`. `.github/workflows/` là cấu hình môi trường CI có thể lặp lại. Xem thêm [các ca nghiệm thu Android](../android/README.md).

## Cài APK thử nghiệm

Bật quyền cài ứng dụng từ nguồn được chọn hoặc dùng `adb install app-debug.apk`. APK debug không phải Play Store release; các runner CI có thể dùng debug keystore khác nhau, nên có thể phải gỡ bản debug cũ trước khi cài bản từ lượt build khác. Chưa có khóa ký phát hành của chủ ứng dụng trong Git.
