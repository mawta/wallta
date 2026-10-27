# Bản dùng thử Wallta 0.1.1

Windows build từ commit trước khi làm sạch lịch sử `6c9acae`; mã nguồn tương ứng hiện tại là [`455091f`](https://github.com/mawta/wallta/commit/455091f0368605a523e4552093f2968b5728f23a). Android build từ commit cũ `08da730`, tương ứng [`ea1709b`](https://github.com/mawta/wallta/commit/ea1709b1c7aa2b1b4b96f9b6a63888affd3b3f23). Các thay đổi giữa hai bản chỉ thuộc bộ giải mã ảnh và kiểm thử Android. Bộ cài gốc được giữ nguyên.

[Tải bản dùng thử 0.1.1](https://github.com/mawta/wallta/releases/tag/v0.1.1), không cần đăng nhập. Các file cũng được giữ tại `output/installers/0.1.1/` trong thư mục dự án trên máy phát triển; không đưa binary vào lịch sử Git.

Thêm hướng dẫn lấy key/OAuth ngay trong form Drive/Photos và chuyển ảnh qua QR/link Wi-Fi hai chiều. Máy còn lại chỉ cần trình duyệt; Wallta xác nhận trước khi thêm ảnh nhận vào thư viện. [Cách chuyển ảnh](local-transfer.md).

| File | Dung lượng | SHA-256 |
|---|---:|---|
| `Wallta_0.1.1_x64-setup.exe` | 4,165,823 bytes | `f26f9ec89b994b90cd78052ba261657281e569ce76a8096c46c655888d4c5e83` |
| `Wallta_0.1.1_android-debug.apk` | 3,667,255 bytes | `65d39201310cb73d3f7ed369e1d9fc2e7ac71a18b3b2b1ccfdc160c74376d6f6` |

`SHA256SUMS.txt` đi kèm bản tải. Đã kiểm tra PE header của bộ cài Windows; CRC toàn bộ 148 mục trong APK; manifest, DEX, resources, giao diện/hướng dẫn Google và native LAN host. Trang LAN trong APK khớp nguồn dùng chung. Kiểm tra cấu trúc gói không thay thế cài và chạy trên thiết bị.

## CI tại commit build

- [Windows desktop — thành công](https://github.com/mawta/wallta/actions/runs/40473826503): 23 Vitest, 46 Rust tests và NSIS installer.
- [Android APK — thành công](https://github.com/mawta/wallta/actions/runs/40474876432): 23 Vitest, 39 JVM tests và debug APK. Kiểm thử giải mã dùng native graphics Robolectric SDK 26/28, không phải thiết bị thật.
- Relay không đổi từ [lượt kiểm chứng trước](https://github.com/mawta/wallta/actions/runs/40453767943): 10 PHPUnit tests / 62 assertions.

Đã thử trang truyền ảnh thật qua trình duyệt với Java HTTP host trên loopback: upload thành công, tải ngược ảnh đã chọn và SHA-256 khớp ảnh gốc. Kiểm thử React xác nhận chọn ảnh local, dừng trước khi đóng, nhận trước khi refresh và cleanup khi bridge timeout; kiểm thử trang web xác nhận gửi tiếp không lặp ảnh đã báo thành công.

Windows chưa ký chứng thư phát hành; APK ký debug để sideload. Chưa có kiểm thử trên TV/điện thoại/Windows cài thực hoặc Google credential thật. Google Photos TV cần quyền Partner Program; Drive riêng Android/TV cần relay HTTPS được cấu hình. Không có relay công cộng vận hành sẵn.

[Hướng dẫn dùng thử](quick-start.md) · [Ma trận kiểm chứng và các ca cần thiết bị](testing.md) · [Thiết lập Google](setup-google.md) · [Lưu trữ bản 0.1.0](artifacts-0.1.0.md).
