# Bản dùng thử Wallta 0.1.0

Mã ứng dụng đã build: [`c47d5473d9cb8cf6e69a751324a81f2f5d6325d7`](https://github.com/mawta/wallta/commit/c47d5473d9cb8cf6e69a751324a81f2f5d6325d7). Các commit tài liệu sau đó không thay đổi mã ứng dụng hay cấu hình build.

[Tải bản dùng thử trong repo private](https://github.com/mawta/wallta/releases/tag/v0.1.0), yêu cầu tài khoản có quyền truy cập. Các file cũng được giữ tại `output/installers/0.1.0/` trong thư mục dự án trên máy phát triển; không đưa binary vào lịch sử Git.

| File | Dung lượng | SHA-256 |
|---|---:|---|
| `Wallta_0.1.0_x64-setup.exe` | 4,094,507 bytes | `eae3ed988a34a2398cf75ae4a0eed495ba0c30f60504a3a8c004eb7048d6eae0` |
| `Wallta_0.1.0_android-debug.apk` | 3,638,796 bytes | `baa7f1894b898891829e93cb2eb7f6b81013c3bf91291f33f76997c0a2975a2b` |

`SHA256SUMS.txt` đi kèm bản tải. Đã kiểm tra PE header của bộ cài Windows; CRC toàn bộ APK và sự có mặt của manifest, DEX, resources, HTML/JS/CSS đóng gói. Kiểm tra cấu trúc gói không thay thế cài và chạy trên thiết bị.

## CI tại commit build

- [Windows desktop — thành công](https://github.com/mawta/wallta/actions/runs/40453767864): 18 Vitest, 31 Rust tests và NSIS installer.
- [Android APK — thành công](https://github.com/mawta/wallta/actions/runs/40453767895): 18 Vitest, 22 JVM tests và debug APK.
- [Drive relay — thành công](https://github.com/mawta/wallta/actions/runs/40453767943): 10 PHPUnit tests / 62 assertions.

Windows chưa ký chứng thư phát hành; APK ký debug để sideload. Chưa có kiểm thử trên TV/điện thoại/Windows cài thực hoặc Google credential thật. Google Photos TV cần quyền Partner Program; Drive riêng Android/TV cần relay HTTPS được cấu hình. Không có relay công cộng vận hành sẵn.

[Hướng dẫn dùng thử](quick-start.md) · [Ma trận kiểm chứng và các ca cần thiết bị](testing.md) · [Thiết lập Google](setup-google.md).
