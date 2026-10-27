# Wallta

Ứng dụng tự đổi hình nền cho Android, Android TV từ Android 8 trở lên và Windows.

Nguồn ảnh dự kiến: thư mục local/USB, thư mục Google Drive, thư mục online và Google Photos.

## Trạng thái

Repo mới, đang duyệt thiết kế; chưa có mã ứng dụng hay bản cài đặt.

Thiết kế đề xuất: [docs/superpowers/specs/2026-10-27-wallta-design.md](docs/superpowers/specs/2026-10-27-wallta-design.md).

Mục tiêu công nghệ: ReactJS + TypeScript + Tauri 2, Kotlin cho các dịch vụ Android/TV. Chỉ bổ sung Laravel nếu cần cầu nối xác thực Google Drive riêng trên TV.

Ứng dụng hướng tới nhiều dòng TV. Hình nền Home phụ thuộc khả năng của hệ điều hành và launcher; không cam kết thay nền trên mọi launcher gốc.
