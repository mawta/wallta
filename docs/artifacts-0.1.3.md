# Wallta 0.1.3

[Tải Windows và Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.3) · [Hướng dẫn cài](quick-start.md) · [Cập nhật trong app](updating.md).

Build từ [`1473c32`](https://github.com/mawta/wallta/commit/1473c3249ebd021fb1601dfa68b66502f185520d), qua [workflow phát hành có chữ ký](https://github.com/mawta/wallta/actions/runs/40818810546). Bản stable công khai, không cần đăng nhập. Bản tải nằm tại `output/installers/0.1.3/` trên máy phát triển.

| File | Dung lượng | SHA-256 |
|---|---:|---|
| `Wallta_0.1.3_x64-setup.exe` | 4,603,645 bytes | `719538fb9267662c6c2ede7fb7700ad7815b2d904d9b0499653ee62dc07c7a56` |
| `Wallta_0.1.3_android.apk` | 3,032,765 bytes | `7f84fee536b28875cf84e0ac732c0d1602c21f64a45ae0553ddd822ad6028d6f` |

Đã tải lại đúng năm file công khai: EXE, chữ ký `.sig`, APK, `latest.json` và `SHA256SUMS`. Xác minh checksum, chữ ký Windows ràng buộc phiên bản 0.1.3, cùng manifest tại [đường cập nhật công khai](https://github.com/mawta/wallta/releases/latest/download/latest.json) đạt.

APK xác minh chữ ký v2, package `app.wallta`, versionCode `4`, minSdk `26`. SHA-256 chứng chỉ phát hành: `a36f8b9fea83ce87452189e5c51d0e145ceda0c87450cb047e4bb7a11cd329eb`, khớp bản 0.1.2. 29 file giao diện trong APK khớp production build từ commit phát hành, sau khi chuẩn hóa xuống dòng HTML/SVG giữa Windows và Linux.

## Kiểm chứng

- React: 43 tests và production build đạt.
- Công cụ phát hành: 12 tests đạt; kiểm tra phiên bản và tài liệu nội bộ/credential không theo dõi trong Git đạt.
- Windows: 78 Rust tests, build NSIS và xác minh chữ ký cập nhật đạt.
- Android: 77 tests trên Linux đạt, không bỏ qua; build APK có chữ ký đạt.

Chưa thử OAuth bằng credential Google thật, cài/nâng cấp, thay wallpaper hoặc reboot trên thiết bị thật. Windows chưa ký Authenticode. APK debug cũ có thể cần gỡ trước vì khác khóa ký; gỡ app xóa dữ liệu trong app. Từ 0.1.2 dùng cùng khóa phát hành.

[Lưu trữ 0.1.2](artifacts-0.1.2.md) · [0.1.1](artifacts-0.1.1.md) · [0.1.0](artifacts-0.1.0.md) · [Ma trận thiết bị và Google](testing.md).
