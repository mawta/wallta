# Wallta 0.1.5

[Tải Windows và Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.5) · [Hướng dẫn cài](quick-start.md) · [Nhiều thư mục và ảnh 4K](upscaling.md) · [Cập nhật trong app](updating.md).

Build từ [`c791793`](https://github.com/mawta/wallta/commit/c7917931361d2d077dcd492b12f9d082af7654fe), qua [workflow phát hành có chữ ký](https://github.com/mawta/wallta/actions/runs/40854327178). Bản stable công khai, phát hành ngày 29/10/2026. Bản tải nằm tại `output/installers/0.1.5/` trên máy phát triển.

| File | Dung lượng | SHA-256 |
|---|---:|---|
| `Wallta_0.1.5_x64-setup.exe` | 4,734,779 bytes | `6b1f91c07217be4edd5d3d063137faad8993c14d89f7ec282b99461494865cf7` |
| `Wallta_0.1.5_android.apk` | 3,088,532 bytes | `6d7a1812c3a2838602d9a96619bebcaeb6045a46395d64d07b4d10ce3688273b` |

Đã tải lại đúng năm file công khai: EXE, chữ ký `.sig`, APK, `latest.json` và `SHA256SUMS`. Xác minh checksum, chữ ký cập nhật Windows ràng buộc phiên bản 0.1.5 và manifest đạt. Manifest Android có versionCode `6` và checksum APK khớp.

APK xác minh chữ ký v2, package `app.wallta`, versionName `0.1.5`, versionCode `6`, minSdk `26`. SHA-256 chứng chỉ phát hành: `a36f8b9fea83ce87452189e5c51d0e145ceda0c87450cb047e4bb7a11cd329eb`, cùng khóa từ 0.1.2. APK công khai khớp SHA-256 với artifact đã kiểm tra chữ ký; 29 file giao diện khớp production build từ commit phát hành, sau khi chuẩn hóa xuống dòng HTML/SVG giữa Windows và Linux.

## Thay đổi

- Thêm/bỏ nhiều thư mục trong cùng nguồn local, Drive hoặc WebDAV, ghi nhớ danh sách và loại ảnh trùng. Drive riêng qua relay Android/TV vẫn giới hạn thư mục đã ghép quyền.
- Giữ kết nối Drive sau khi lưu dù chưa chọn thư mục; có thể chọn lại sau.
- Nâng ảnh 4K chế độ Nhanh trên Windows/Android/TV; AI Real-ESRGAN xử lý tại máy trên Windows có GPU/driver Vulkan.
- Xem trước–sau, tiến độ, hủy và lưu thành nguồn riêng **Ảnh 4K**; có thể áp dụng ngay hoặc gửi qua Wi-Fi. Giữ nguyên ảnh gốc và tỷ lệ ảnh.

## Kiểm chứng

- React: 62 tests và production build đạt.
- Công cụ phát hành: 12 tests, kiểm tra phiên bản và loại trừ tài liệu nội bộ/credential đạt.
- Windows: 124 Rust tests, build NSIS và xác minh chữ ký cập nhật đạt.
- Android: 98 tests trên Linux đạt, không bỏ qua; build APK có chữ ký đạt.
- AI: xác minh archive và bốn file runtime/model chính thức; chạy runtime trên Radeon RX 470 qua Vulkan với fixture 16×16 → 64×64, exit 0. Đây là kiểm tra thực thi runtime, chưa đo chất lượng/tốc độ xử lý ảnh 4K hoặc thử toàn bộ luồng AI trong app đã cài.

Chưa thử OAuth bằng credential Google thật, cài/nâng cấp, thay wallpaper hoặc reboot trên thiết bị thật với bản này. Android/TV thật cần kiểm tra thêm ảnh lớn và bộ nhớ. Windows chưa ký Authenticode. Từ 0.1.2 dùng cùng khóa phát hành; APK debug cũ có thể cần gỡ vì khác khóa ký, làm mất dữ liệu trong app.

[Lưu trữ 0.1.4](artifacts-0.1.4.md) · [0.1.3](artifacts-0.1.3.md) · [0.1.2](artifacts-0.1.2.md) · [0.1.1](artifacts-0.1.1.md) · [0.1.0](artifacts-0.1.0.md) · [Ma trận thiết bị và Google](testing.md).
