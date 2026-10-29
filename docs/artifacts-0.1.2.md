# Wallta 0.1.2

[Tải bộ cài Windows và Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.2) · [Hướng dẫn cài](quick-start.md) · [Cập nhật trong app](updating.md).

Cả hai bộ cài được build từ [`903a473`](https://github.com/mawta/wallta/commit/903a4736d76c4c8dda0b63f1f9cfd0cea49c7739), qua [workflow phát hành có chữ ký](https://github.com/mawta/wallta/actions/runs/40486059129). Bản stable công khai, không cần đăng nhập. Bản tải cũng nằm tại `output/installers/0.1.2/` trên máy phát triển.

| File | Dung lượng | SHA-256 |
|---|---:|---|
| `Wallta_0.1.2_x64-setup.exe` | 4,546,930 bytes | `415b549cdd02b3b93962a58b84a3808b32acc34a4b44101fc55c0fb7c13c84e1` |
| `Wallta_0.1.2_android.apk` | 3,029,777 bytes | `7cd53bcb2436e40b00a72d9108c357576074c13f0df4e8656af610ac32233fd9` |

Release gồm đúng năm file: EXE, chữ ký `.sig`, APK, `latest.json` và `SHA256SUMS`. Đã tải lại bản công khai và xác minh checksum, chữ ký Windows gắn với phiên bản 0.1.2, cùng manifest tại [đường cập nhật công khai](https://github.com/mawta/wallta/releases/latest/download/latest.json).

APK được `apksigner` xác minh chữ ký v2, package `app.wallta`, versionCode `3`, minSdk `26`. SHA-256 chứng chỉ phát hành: `a36f8b9fea83ce87452189e5c51d0e145ceda0c87450cb047e4bb7a11cd329eb`, khớp khóa đã lưu. CRC toàn gói đạt; 29 file giao diện và trang LAN khớp mã nguồn/build, sau khi chuẩn hóa xuống dòng HTML/SVG giữa Windows và Linux.

## Kiểm chứng

- React: 33 tests và production build đạt.
- Công cụ phát hành: 12 tests đạt; chặn ghi đè bản cũ, giảm phiên bản/code và artifact sai.
- Windows: 57 Rust tests, build NSIS và xác minh chữ ký cập nhật đạt.
- Android: 76 tests đạt trên Linux, không bỏ qua ca nào; build APK ký phát hành đạt. Có ca API 26/28 kiểm tra signer, FileProvider, quyền cài và hủy tác vụ.

Chưa thử cài/nâng cấp trên TV, điện thoại hoặc Windows thực. Windows chưa ký Authenticode. **APK debug cũ có thể phải gỡ trước; gỡ app xóa cài đặt và ảnh nhập trong app. Sao lưu trước khi chuyển sang 0.1.2.**

[Lưu trữ 0.1.1](artifacts-0.1.1.md) · [Lưu trữ 0.1.0](artifacts-0.1.0.md) · [Ma trận thiết bị và Google](testing.md).
