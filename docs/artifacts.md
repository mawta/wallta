# Wallta 0.1.6

[Tải Windows và Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.6) · [Hướng dẫn cài](quick-start.md) · [Ảnh 4K](upscaling.md) · [Cập nhật trong app](updating.md).

Build từ [`c66cc3a`](https://github.com/mawta/wallta/commit/c66cc3aacca43fa61649599cb688f2ab79bc8a43), qua [workflow phát hành có chữ ký](https://github.com/mawta/wallta/actions/runs/40861799700). Bản stable công khai ngày 29/10/2026. Bản tải nằm tại `output/installers/0.1.6/` trên máy phát triển.

| File | Dung lượng | SHA-256 |
|---|---:|---|
| `Wallta_0.1.6_x64-setup.exe` | 4,726,796 bytes | `cff3afb9cb199d264665ff921b3cf90047d3d6ebed39a6927cfff4a96c04e9d5` |
| `Wallta_0.1.6_android.apk` | 3,088,544 bytes | `a98a1d16e6303c38242fe681698c989c305b8c808d684cb6324edad1b834f603` |

Đã tải lại đủ năm file công khai: EXE, chữ ký .sig, APK, latest.json và SHA256SUMS. Kiểm tra inventory, checksum và chữ ký cập nhật Windows ràng buộc phiên bản 0.1.6 đạt. Manifest latest tải được không cần đăng nhập, phiên bản 0.1.6, Android versionCode 7.

APK công khai khớp SHA-256 với artifact đã xác minh chữ ký, package app.wallta, versionName 0.1.6, versionCode 7, minSdk 26. SHA-256 chứng chỉ phát hành: `a36f8b9fea83ce87452189e5c51d0e145ceda0c87450cb047e4bb7a11cd329eb`, cùng khóa từ 0.1.2. 29 file giao diện khớp production build, sau khi chuẩn hóa xuống dòng HTML/SVG giữa Windows và Linux.

## Thay đổi

- Bỏ giới hạn 30 phút của cả lô AI Windows; giữ giới hạn 10 phút cho từng ảnh AI, giới hạn số ảnh/dung lượng và hủy tiến trình con.
- Nếu ảnh sau gặp lỗi, giữ file đầy đủ và preview của các ảnh đã hoàn thành. Có thể xem/lưu chúng; màn hình hiển thị số ảnh hoàn thành/tổng số.
- Giữ kiểm tra nguồn/cài đặt thay đổi, hủy và không lưu khi worker còn chạy. Ảnh gốc và bộ xử lý AI không thay đổi; Android native không thay đổi.

## Kiểm chứng

- React: 64 tests và production build đạt.
- Windows: 125 Rust tests và NSIS build đạt. Regression worker thật thất bại trên mã cũ, đạt sau sửa: giữ ảnh đã xong, lưu đúng kích thước đầy đủ, giữ nguyên ảnh gốc, lưu lặp không tạo nguồn trùng.
- Android: 98 tests trên Linux đạt, không bỏ qua, build APK có chữ ký đạt.
- Công cụ phát hành: 12 tests, kiểm tra phiên bản và loại trừ tài liệu nội bộ/credential đạt.
- Đánh giá mã độc lập không có lỗi cần sửa trước phát hành.

Chưa chạy lô AI thật dài hơn 30 phút, thử cài/nâng cấp hoặc luồng AI đầy đủ trong bản app đã cài trên thiết bị thật. Bản này sửa timeout/lưu kết quả, không thay đổi chất lượng model AI. Xem [bằng chứng runtime AI và giới hạn 0.1.5](artifacts-0.1.5.md). Windows chưa ký Authenticode; chữ ký cập nhật của Wallta đã được xác minh.

[Lưu trữ 0.1.5](artifacts-0.1.5.md) · [0.1.4](artifacts-0.1.4.md) · [0.1.3](artifacts-0.1.3.md) · [0.1.2](artifacts-0.1.2.md) · [0.1.1](artifacts-0.1.1.md) · [0.1.0](artifacts-0.1.0.md) · [Ma trận kiểm thử](testing.md).
