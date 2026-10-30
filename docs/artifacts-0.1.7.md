# Wallta 0.1.7

[Tải Windows và Android/TV](https://github.com/mawta/wallta/releases/tag/v0.1.7) · [Hướng dẫn cài](quick-start.md) · [Ảnh 4K](upscaling.md) · [Cập nhật trong app](updating.md).

Build từ [`13e7036`](https://github.com/mawta/wallta/commit/13e70365d2f3e08b4906866e764b0a6d59d8d2af), qua [workflow phát hành có chữ ký](https://github.com/mawta/wallta/actions/runs/41016491179). Bản stable công khai ngày 30/10/2026. Bộ cài tải lại nằm tại `output/installers/0.1.7/` trên máy phát triển.

| File | Dung lượng | SHA-256 |
|---|---:|---|
| SHA256SUMS | 359 B | `d5144e82be0ebee7995c19f0941259a4f8d10f1a85f133e9c49db7297a243f04` |
| Wallta_0.1.7_android.apk | 3,104,956 B | `8ea25ed9bd1e34459386bd63a29c14f9440e8b6a4d9bcde744aa7aa4b1836b79` |
| Wallta_0.1.7_x64-setup.exe | 4,763,076 B | `70d05e6dcfb98f105176340afad3d5c914bdb7ac2535fb4ba0e0207e8ac3d466` |
| Wallta_0.1.7_x64-setup.exe.sig | 436 B | `2e8814a9a981275a7cc09a85b5b47348984eda3e519a04aeee2f028b4b9bf28b` |
| latest.json | 1,277 B | `48e0467c63a6692874f2c81dbbdf20de2bf6eb940b547552d094c9bcd092be5d` |

Đã tải lại đủ **5 asset công khai**, đối chiếu inventory/SHA-256 và chữ ký cập nhật Windows đạt. Endpoint `releases/latest/download/latest.json` truy cập không đăng nhập trả về **0.1.7 / Android versionCode 8**. APK có package `app.wallta`, minSdk 26, targetSdk 35; khóa ký SHA-256 `a36f8b9fea83ce87452189e5c51d0e145ceda0c87450cb047e4bb7a11cd329eb`. Toàn bộ **29 web asset** trong APK khớp production build (chuẩn hóa xuống dòng HTML/SVG).

## Thay đổi

- **Chạy tất cả** ảnh gốc từ một nguồn hoặc các nguồn đang bật; lựa chọn thủ công vẫn tối đa 50 ảnh. Xử lý tuần tự, tự lưu từng ảnh vào một bộ ảnh tắt ban đầu; hủy/lỗi giữ phần đã lưu. Xem trước tối đa 20 ảnh, số ảnh đã lưu tính toàn bộ.
- Một thanh tiến trình dưới cùng, giữ khi chuyển trang/đóng cấu hình. Chỉ tính ảnh đã xử lý xong; tải model hiển thị số byte. Có lựa chọn áp dụng khi hoàn thành.
- AI Windows chọn Photo **33.54 MB**, Anime **8.97 MB**, Anime Light **1.25 MB**. Bộ xử lý dùng chung **6.34 MB**. Chỉ tải model đã chọn khi chạy, xác minh từng file; không cần API key. Android/TV dùng Nhanh.
- Dọn cache giữ ảnh đã lưu/model; liệt kê và xác nhận xóa từng thư mục 4K. Bộ ảnh có tên nguồn, chế độ/model và thời gian. Giữ/nhận diện bộ ảnh legacy, kể cả khi ngắt nguồn; xóa giới hạn trong thư mục app sở hữu.

## Kiểm chứng

- React: **77 tests**, production build đạt; kiểm tra bố cục desktop/390px và điều hướng trên trình duyệt với native bridge giả lập, không có lỗi console.
- Windows: **152 Rust tests** và NSIS build đạt; All 61 ảnh gốc, bỏ cây kết quả trước giới hạn duyệt nguồn, lưu tăng dần, giữ kết quả khi hủy/lỗi, nguồn thay đổi, chống link/junction, tiến độ đúng và phân loại lỗi runtime/ảnh online.
- Android: **111 tests** trên Linux, không lỗi/bỏ qua, APK có chữ ký đạt. Local Windows có 4 test điều kiện nền tảng bỏ qua; Linux đã chạy đầy đủ. Bao gồm All 75 ảnh, một bộ ảnh, 20 preview, own/external revision, thứ tự khóa updater/state, dọn/xóa và legacy orphan/detach.
- Công cụ phát hành: **12 tests**, phiên bản 0.1.7/versionCode 8, privacy và inventory đạt. Các đánh giá spec, chất lượng và tích hợp độc lập không còn finding quan trọng.
- [Model assets v1](https://github.com/mawta/wallta/releases/tag/ai-models-v1): 8 file thực thi/model khớp byte/SHA-256 đã pin; tải lại 10 file tài nguyên/giấy phép và so SHA256SUMS đạt. Bản prerelease này không thay thế bản app stable.
- Ba model thực thi bằng runtime chính thức trên Vulkan/Radeon RX 470: fixture **16×16 → 64×64**, exit 0. Đây là kiểm tra thực thi; chưa đo so sánh chất lượng ảnh thật 4K.

Chưa thử cài/nâng cấp đầy đủ hoặc đổi wallpaper/reboot trên điện thoại/TV thật cho bản này; kiểm tra thiết bị thật không được thay thế bởi CI/Robolectric. Không tự tiếp tục queue sau khi thoát app. Kích thước 4K không bảo đảm chi tiết như ảnh chụp 4K gốc. Windows chưa ký Authenticode; chữ ký cập nhật Wallta đã được xác minh.

[Lưu trữ 0.1.6](artifacts-0.1.6.md) · [0.1.5](artifacts-0.1.5.md) · [0.1.4](artifacts-0.1.4.md) · [0.1.3](artifacts-0.1.3.md) · [0.1.2](artifacts-0.1.2.md) · [0.1.1](artifacts-0.1.1.md) · [0.1.0](artifacts-0.1.0.md) · [Ma trận kiểm thử](testing.md).
