# Wallta 0.1.4

[Tải Windows và Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.4) · [Hướng dẫn cài](quick-start.md) · [Cập nhật trong app](updating.md).

Build từ [`da54292`](https://github.com/mawta/wallta/commit/da54292fc67d1449424d3e1b66332aff025e3355), qua [workflow phát hành có chữ ký](https://github.com/mawta/wallta/actions/runs/40821659871). Bản stable công khai. Bản tải nằm tại `output/installers/0.1.4/` trên máy phát triển.

| File | Dung lượng | SHA-256 |
|---|---:|---|
| `Wallta_0.1.4_x64-setup.exe` | 4,606,641 bytes | `8c7f26602f144ee8ad4190430c4e206887f01aafb76dbe8fd1eeda5109c190f7` |
| `Wallta_0.1.4_android.apk` | 3,033,377 bytes | `99305b4ad1c26db17d77cdcde0f7f925a029442cbe8c02897b08ac711cb4f8fa` |

Đã tải lại đúng năm file công khai: EXE, chữ ký `.sig`, APK, `latest.json` và `SHA256SUMS`. Xác minh checksum, chữ ký Windows ràng buộc phiên bản 0.1.4 và [manifest cập nhật công khai](https://github.com/mawta/wallta/releases/latest/download/latest.json) đạt.

APK xác minh chữ ký v2, package `app.wallta`, versionCode `5`, minSdk `26`. SHA-256 chứng chỉ phát hành: `a36f8b9fea83ce87452189e5c51d0e145ceda0c87450cb047e4bb7a11cd329eb`, cùng khóa từ 0.1.2. 29 file giao diện trong APK khớp production build từ commit phát hành, sau khi chuẩn hóa xuống dòng HTML/SVG giữa Windows và Linux.

## Thay đổi

- Tài khoản Google đã lưu xuất hiện trong Cài đặt và khi thêm nguồn Drive/Photos. Chọn Dùng kết nối để mở lại đúng nguồn, không phải tạo kết nối mới.
- Drive có nút Lưu kết nối khi chưa chọn thư mục. Đăng nhập không tự mở trình chọn thư mục; có thể chọn hoặc đổi sau.
- Nguồn Drive chưa chọn thư mục hiển thị rõ Đã kết nối.

## Kiểm chứng

- React: 48 tests và production build đạt, gồm đăng nhập → lưu kết nối → đóng toàn bộ giao diện → mở lại → chọn thư mục bằng cùng source ID.
- Công cụ phát hành: 12 tests, kiểm tra phiên bản và kiểm tra loại trừ tài liệu nội bộ/credential đạt.
- Windows: 78 Rust tests, build NSIS và xác minh chữ ký cập nhật đạt.
- Android: 77 tests trên Linux đạt, không bỏ qua; build APK có chữ ký đạt.

Chưa thử OAuth bằng credential Google thật với bản này, cài/nâng cấp, thay wallpaper hoặc reboot trên thiết bị thật. Windows chưa ký Authenticode. Từ 0.1.2 dùng cùng khóa phát hành; APK debug cũ có thể cần gỡ vì khác khóa ký, làm mất dữ liệu trong app.

[Lưu trữ 0.1.3](artifacts-0.1.3.md) · [0.1.2](artifacts-0.1.2.md) · [0.1.1](artifacts-0.1.1.md) · [0.1.0](artifacts-0.1.0.md) · [Ma trận thiết bị và Google](testing.md).
