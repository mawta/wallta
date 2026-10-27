# Wallta

Ứng dụng tự đổi hình nền cho Android, Android TV từ Android 8 trở lên và Windows.

Giao diện React + TypeScript dùng chung; Windows chạy bằng Tauri/Rust, Android/TV chạy bằng Kotlin và WebView đóng gói trong APK. Bộ lập lịch native chạy độc lập với giao diện. Laravel cung cấp relay tùy chọn cho Drive riêng trên TV.

## Chức năng

- Thư mục trên máy/USB, thư mục con; link ảnh, danh sách JSON và WebDAV/NAS.
- Google Drive công khai bằng API key; Drive riêng qua OAuth trên Windows hoặc relay HTTPS trên Android/TV.
- Google Photos: TV dùng Ambient API; Windows nhập ảnh qua Picker API; Android dùng trình chọn ảnh hệ thống/cloud nếu thiết bị có nhà cung cấp tương ứng. Nhập ảnh tạo bộ ảnh local, không tự đồng bộ album.
- Ngẫu nhiên dùng hết một vòng trước khi lặp, tuần tự, chọn ngày/khung giờ qua đêm, fit/fill/stretch, nền mờ, giới hạn cache và chỉ tải qua Wi-Fi.
- Windows có khay hệ thống, khởi động cùng máy. Android có WorkManager và TV screensaver. Đồng hồ hiển thị trong trình chiếu/screensaver.

## Chạy và build

```sh
npm ci
npm test -- --run
npm run dev
```

`npm run dev` mở bản xem trước tại `http://127.0.0.1:1420`; bản xem trước không đổi hình nền hệ thống. Nguồn online cần CORS, thư mục local cần chọn lại khi tải lại trang.

Windows 10/11 x64: cài Rust stable, Visual Studio C++ Build Tools và WebView2, sau đó `npm run tauri -- dev` hoặc `npm run tauri -- build`. Bộ cài NSIS nằm trong `src-tauri/target/release/bundle/nsis/`.

Android 8+ (API 26): dùng JDK 17, Gradle 8.14 và SDK 35. Xem [hướng dẫn Android/TV](android/README.md). CI tạo APK debug để sideload; bản cài Windows hiện chưa ký chứng thư phát hành.

Ba workflow GitHub Actions kiểm tra Windows, Android và relay. Bản cài được lưu trong Actions artifacts của repo private; phải đăng nhập tài khoản được cấp quyền để tải.

## Nguồn online

Link thư mục web thông thường không có chuẩn liệt kê ảnh. Dùng WebDAV hoặc cung cấp JSON tại URL:

```json
{"images":["a.jpg",{"url":"sub/b.webp","name":"Chiều trên núi"}]}
```

URL tương đối được tính theo URL JSON. Link một ảnh cũng được hỗ trợ. Wallta đọc ảnh gốc và tạo bản render/cache riêng.

## Google và giới hạn nền tảng

[Thiết lập Google](docs/setup-google.md) hướng dẫn API key, OAuth, Photos và [relay Laravel](relay/README.md). Chưa có credential Google đóng sẵn. Ambient yêu cầu quyền truy cập trong Google Photos Partner Program; Picker chỉ nhập các ảnh do người dùng chọn.

Android đổi hình nền tĩnh theo lịch tối thiểu **15 phút**, có thể trễ do hệ thống tiết kiệm pin. Chu kỳ trình chiếu/screensaver được đặt riêng. Bật Wallta trong mục Screensaver của TV để chạy khi màn hình nghỉ.

Ứng dụng hướng tới nhiều dòng TV. Hình nền Home phụ thuộc khả năng của hệ điều hành và launcher; không cam kết thay nền trên mọi launcher gốc.

Android cần WebView đủ mới để hỗ trợ bridge theo origin; TV có WebView quá cũ sẽ hiển thị hướng dẫn cập nhật. Không cài launcher thay thế. Windows cần tiến trình Wallta còn chạy trong khay; Quit dừng bộ lập lịch.

[Ma trận kiểm chứng](docs/testing.md) ghi rõ kết quả tự động và các ca cần thiết bị/credential thật. [Thiết kế](docs/superpowers/specs/2026-10-27-wallta-design.md) và [kế hoạch](docs/superpowers/plans/2026-10-27-wallta-implementation.md) lưu các quyết định kỹ thuật.
