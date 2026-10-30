# Kiểm chứng Wallta

Đối chiếu mã nguồn và bằng chứng thực thi; CI build không thay thế kiểm tra trên thiết bị. Windows chưa ký Authenticode; APK từ 0.1.2 dùng khóa phát hành cố định.

## Bổ sung trong 0.1.9

- Sửa bộ quét nguồn local trên Windows: tiền tố đường dẫn canonical `\\?\` được xử lý như đường dẫn file, không phải query URL. Các bộ ảnh upscale đã lưu được đọc lại sau khi cập nhật và làm mới thư viện, không cần xử lý ảnh lại.
- Regression test tạo thư mục canonical thật, kiểm tra JPG/PNG, bỏ qua metadata, giữ bộ lọc URL online và kiểm tra lưu/bật nguồn rồi mở lại engine.
- Kiểm tra trực tiếp trên Windows bằng bộ quét production: bộ ảnh đã lưu được đọc đủ 146 ảnh, không thay đổi dữ liệu.

## Bổ sung trong 0.1.8

- React: **94 tests** và production build đạt; ngôn ngữ thiết bị, chọn/nhớ ngôn ngữ, storage bị chặn, đổi ngay khi form đang mở, giữ tên nguồn, template lỗi native và giữ chẩn đoán chưa biết.
- Trang LAN: mặc định Anh/Việt theo trình duyệt, chọn ngôn ngữ riêng; chuyển ngôn ngữ trong lúc retry không gửi lại ảnh đã thành công, giữ tên file và token URL.
- Windows: **152 Rust tests** và NSIS build đạt. Android: **111 tests** trên Linux, không lỗi/bỏ qua, APK build đạt.
- Công cụ phát hành: **12 tests**, phiên bản 0.1.8/versionCode 9, privacy và diff check đạt.
- Trình duyệt: desktop/390px Anh–Việt, nhớ lựa chọn sau reload, form Drive tiếng Anh không tràn ngang, không có console error. Spec/quality review không còn lỗi quan trọng.

[Windows CI](https://github.com/mawta/wallta/actions/runs/41038165869) · [Android CI](https://github.com/mawta/wallta/actions/runs/41038166043) · [Phát hành có chữ ký](https://github.com/mawta/wallta/actions/runs/41038816691) · [Bộ cài và checksum](artifacts.md). Chưa chạy lại cài/nâng cấp, wallpaper/reboot và OAuth có credential trên thiết bị thật cho bản giao diện này. Hộp thoại hệ thống, trang bên thứ ba, lỗi chưa biết và tên bộ ảnh đã tạo có thể giữ ngôn ngữ ban đầu.


## Bổ sung trong 0.1.7

- React: **77 tests** và production build đạt; kiểm tra desktop/390px, chọn model, chạy tất cả, điều hướng và một thanh tiến trình dưới cùng bằng native bridge giả lập, không có lỗi console.
- Windows: **152 Rust tests**, NSIS build đạt; All vượt 50 ảnh, lưu tăng dần, hủy/lỗi giữ kết quả, lọc nguồn kết quả, tiến độ, phân loại lỗi runtime, xác minh model và giới hạn xóa trong vùng app sở hữu.
- Android: **111 tests trên Linux**, không lỗi/bỏ qua; APK có chữ ký đạt. Local Windows có 4 test điều kiện nền tảng bỏ qua; Linux chạy đầy đủ. Bao gồm All 75 ảnh, 20 preview, revision, khóa trạng thái và giữ bộ ảnh legacy khi ngắt nguồn.
- Công cụ phát hành: **12 tests**, phiên bản 0.1.7/versionCode 8 và kiểm tra loại trừ tài liệu nội bộ/credential đạt.
- Tải lại đủ 5 asset công khai: SHA-256, chữ ký cập nhật Windows, APK signer/package/minSdk và 29 web asset đạt; manifest latest truy cập không đăng nhập trả về 0.1.7.
- Ba model Photo/Anime/Anime Light đã chạy runtime chính thức trên Vulkan/Radeon RX 470, fixture **16×16 → 64×64**, exit 0. Asset công khai khớp byte/SHA-256 đã pin. Chưa đo chất lượng hoặc tốc độ xử lý ảnh thật 4K.

[Windows CI](https://github.com/mawta/wallta/actions/runs/41015935942) · [Android CI](https://github.com/mawta/wallta/actions/runs/41015935937) · [Phát hành có chữ ký](https://github.com/mawta/wallta/actions/runs/41016491179) · [Bộ cài và checksum](artifacts.md). Chưa thử cài/nâng cấp và wallpaper/reboot trên thiết bị thật cho bản này; chạy runtime riêng không thay thế kiểm tra AI trong app đã cài. Không tự tiếp tục queue sau khi thoát app.

## Bổ sung trong 0.1.6

- React: 64 tests và production build đạt, gồm hiển thị/xem trước/lưu các ảnh đã hoàn thành sau khi lô lỗi; không hiện nút lưu nếu chưa có ảnh hoàn thành.
- Windows: 125 Rust tests và NSIS build đạt. Regression chạy worker thật: ảnh đầu thành công, ảnh sau lỗi, vẫn lưu được ảnh đầy đủ, giữ nguyên ảnh gốc và lưu lặp không nhân đôi nguồn. Test này đã thất bại trên mã cũ rồi đạt sau sửa.
- Android: 98 tests đạt, không bỏ qua, và build APK đạt; không thay đổi bộ xử lý native Android. AI vẫn chỉ có trên Windows.
- Công cụ phát hành: 12 tests, phiên bản 0.1.6/versionCode 7, định dạng và loại trừ tài liệu nội bộ/credential đạt.

[Windows CI](https://github.com/mawta/wallta/actions/runs/40861115633) · [Android CI](https://github.com/mawta/wallta/actions/runs/40861115566) · [Thay đổi](https://github.com/mawta/wallta/pull/4). Kiểm tra giới hạn theo mã: đã bỏ đồng hồ 30 phút của cả lô Windows, giữ đồng hồ 10 phút mỗi ảnh AI, giới hạn số ảnh/dung lượng và hủy tiến trình con. Chưa chạy một lô AI thật kéo dài hơn 30 phút hoặc nâng cấp trên thiết bị thật.

## Bổ sung trong 0.1.5

| Phạm vi | Kết quả |
|---|---|
| React/TypeScript | 62 tests và production build đạt; nhiều thư mục, giữ kết nối, tiến độ/hủy, xem trước–sau và lưu bộ ảnh 4K |
| Windows | 124 Rust tests đạt; nhiều thư mục, giới hạn ảnh, quản lý tiến trình AI và dọn kết quả tạm; NSIS build đạt |
| Android | 98 tests trên Linux đạt, không bỏ qua; nhiều thư mục, EXIF, xử lý 4K và giữ kết nối Drive khi chưa chọn thư mục |
| Công cụ phát hành | 12 tests, kiểm tra phiên bản 0.1.5/versionCode 6 và loại trừ tài liệu nội bộ/credential đạt |
| AI thực thi | Runtime/model chính thức đã xác minh SHA-256; chạy Vulkan trên Radeon RX 470, ảnh fixture 16×16 → 64×64, exit 0; chưa đo chất lượng/tốc độ ảnh 4K |

[Windows CI](https://github.com/mawta/wallta/actions/runs/40853163967) · [Android CI](https://github.com/mawta/wallta/actions/runs/40853163961) · [Thay đổi](https://github.com/mawta/wallta/pull/3). Chưa thử đầy đủ cài/nâng cấp, OAuth Google thật, thay wallpaper/reboot hoặc xử lý 4K trên điện thoại/TV thật. Chạy runtime AI riêng không thay thế kiểm thử luồng AI trong bản app đã cài.

## Bổ sung trong 0.1.3

| Phạm vi | Kết quả |
|---|---|
| React/TypeScript | 43 tests và production build đạt; lưu tài khoản Drive trước/chọn thư mục sau, bỏ phản hồi cũ, polling không bị reset, áp dụng đúng nguồn và giữ nguồn nếu áp dụng lỗi |
| Windows | 78 Rust tests và NSIS build đạt; metadata/cache theo tài khoản và lần cấp quyền, duyệt Drive, lưu nguồn chưa chọn thư mục, dọn import Photos theo tham chiếu và trạng thái phiên |
| Android | 77 tests trên Linux đạt, không bỏ qua; build APK đạt, áp dụng theo source ID |
| Phát hành | 12 tests công cụ đạt; phiên bản 0.1.3, Android versionCode 4; kiểm tra tài liệu nội bộ/credential không được theo dõi trong Git đạt |

[Windows CI](https://github.com/mawta/wallta/actions/runs/40818152582) · [Android CI](https://github.com/mawta/wallta/actions/runs/40818152579) · [Thay đổi](https://github.com/mawta/wallta/pull/1). Chưa thử OAuth bằng credential Google thật, thay wallpaper hoặc khởi động lại trên thiết bị thật. Windows Photos lưu bộ ảnh đã nhập, không tự theo dõi album; Drive ghi nhớ thư mục và đọc lại danh sách ảnh khi chạy.

## Bổ sung trong 0.1.2

| Phạm vi | Kết quả |
|---|---|
| React/TypeScript | 33 tests và production build đạt; tải/cài riêng, tiến độ, hủy, lỗi, quyền cài, không kiểm tra khi trang bị ẩn |
| Windows | 57 Rust tests và NSIS build đạt; chữ ký cập nhật có ràng buộc phiên bản, giới hạn tải, throttle và chờ lưu dữ liệu trước khi cài |
| Android | 76 tests trên Linux đạt, không bỏ qua; API 26/28 kiểm tra APK identity/signer, FileProvider, quyền cài, hủy và handoff; APK ký phát hành đạt |
| Phát hành | 12 tests công cụ đạt; tải lại năm asset công khai, xác minh checksum/chữ ký, manifest latest không cần đăng nhập |

[Commit, CI và checksum 0.1.2](artifacts-0.1.2.md). Chưa chạy nâng cấp thực trên Windows/điện thoại/TV; cần kiểm tra quyền cài, hủy trình cài, giữ dữ liệu và khởi động lại trên thiết bị.

## Bổ sung trong 0.1.1

| Phạm vi | Kết quả |
|---|---|
| React/TypeScript | 23 tests và production build đạt; chọn local, stop/accept, cleanup khi start timeout, retry upload không lặp ảnh đã báo thành công |
| Windows | 46 Rust tests đạt; upload/download TCP, truyền nhiều gói có trễ, Host/Origin/token/framing, giới hạn, expiry/cleanup, xác nhận nhận và giữ cấu hình mới nhất; NSIS build đạt |
| Android | 39 JVM tests đạt; TCP thật, giới hạn/hủy/xác nhận, container PNG/JPEG/WebP và native pixel decoding trên Robolectric SDK 26/28; APK build đạt |
| Trình duyệt LAN | Trang thật chạy bằng Java host trên loopback; gửi ảnh và tải ngược thành công, SHA-256 khớp bản gốc; đã lưu ảnh chụp trang và hướng dẫn Google |

[Commit, CI và checksum 0.1.1](artifacts-0.1.1.md). Các ca LAN qua Wi-Fi/firewall trên cặp thiết bị thật và đăng nhập Google bằng credential thật chưa được thực hiện.

## Bằng chứng nền từ bản 0.1.0

| Phạm vi | Kết quả / bằng chứng |
|---|---|
| React/TypeScript | 18 Vitest tests đạt, production Vite build đạt |
| Thao tác trình duyệt | Thêm nguồn JSON thật 2 ảnh, Next, trình chiếu/ArrowRight/Escape, lưu lịch 300 giây và tải lại đều đạt |
| Bố cục | Đã xem ảnh chụp desktop 1440×900, mobile 390×844 và TV 1920×1080; mobile không tràn ngang |
| Bàn phím/D-pad web | Đổi mục sidebar bằng mũi tên; rời input bằng Up/Down và select bằng Left/Right; focus hiển thị |
| Lỗi lưu/hủy OAuth UI | Mounted React regression tái hiện đóng lúc đang lưu, sau sửa không đóng qua nút/Escape cho tới khi xong |
| Windows native | 31 Rust tests đạt trên CI: selection/schedule/parsers/WebDAV recursive/cache ownership/offline/cancellation và OAuth PKCE/state; NSIS build thành công |
| Android native | 22 JVM tests đạt trên CI; compile Kotlin và tạo debug APK thành công, screensaver dùng native ImageView. Phần hardware cần kiểm tra bên dưới |
| Relay Laravel | 10 tests / 62 assertions đạt trên PHP 8.4, Google HTTP fake; PKCE/state, credential mã hóa, scope thư mục, refresh/disconnect, cancel và limiter riêng |
| Relay race review | Hai callback đồng thời: bên thua 410; hủy khi token exchange đang chạy: callback 410, không tạo lại pair; alias root chuyển ID chuẩn |
| Dependency audit | npm và Composer audit không tìm thấy advisory tại thời điểm chạy 27/10/2026 |

Test trước sửa đã quan sát được lỗi: pause bị chặn bởi download; preview ghi sau pause; WebDAV bỏ qua thư mục con; đóng nguồn trong lúc lưu; phản hồi Google đến sau khi đóng; relay dùng chung rate limit; chưa có cancel endpoint. Lịch sử commit và log CI giữ bằng chứng RED/GREEN. Một số test JVM Android được viết trước mã sửa nhưng không chạy được bước RED tại máy này do thiếu JDK/SDK; kết quả GREEN được kiểm tra trên CI.

Ảnh chụp, fixture và bộ cài nằm ở `output/`, được Git bỏ qua. Không dùng ảnh giả để báo số lượng thư viện; khung cảnh SVG khi chưa có ảnh được ghi rõ là minh họa. Ảnh kiểm thử được tạo từ SVG của dự án.

[Bản cài, SHA-256 và các lượt CI đã kiểm chứng](artifacts.md).

## Cần thiết bị/credential thật

- Windows 10/11: cài NSIS, thử Wallpaper API, khay hệ thống, autostart, nhiều màn hình, khóa màn hình và reboot. Test tự động không gọi thay hình nền của máy người dùng.
- Android 8+ phone: SAF/USB tháo cắm, quyền đọc sau process death, Keystore, chọn cloud Photos và import; background trong Doze/reboot; xóa nguồn khi tải chậm.
- Android TV: D-pad và Back trong WebView, menu Screensaver, DreamService qua đêm/remote exit, ảnh lớn trên TV ít RAM; kiểm tra Home wallpaper riêng cho từng firmware/launcher.
- Google: OAuth project thật, consent/refresh/revoke, quota và URL hết hạn; Photos Ambient phải được Google Partner Program chấp thuận. Chưa có project/credential trong repo.
- Relay: chạy HTTPS thật, proxy/session cookie/cache bền, hoàn tất QR từ điện thoại, TV poll và ngắt kết nối. Chưa triển khai server.
- LAN: điện thoại ↔ TV/Windows trên cùng Wi-Fi; quét QR, tải ra ảnh đã chọn, nhận và xác nhận ảnh, đổi IP, guest/AP isolation, firewall, hết hạn khi màn hình khóa. Thử ảnh lớn trên TV ít RAM.

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
