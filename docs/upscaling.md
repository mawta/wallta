# Nhiều thư mục và ảnh 4K

## Nguồn ảnh

Mở **Nguồn ảnh → Chỉnh sửa**. Chọn thêm thư mục trên thiết bị, chọn thư mục bằng tài khoản Drive đã lưu, hoặc nhập URL WebDAV rồi bấm **Thêm thư mục**. Có thể bỏ từng thư mục và lưu lại. Tối đa 50 thư mục mỗi nguồn; các thư mục WebDAV dùng chung máy chủ và thông tin đăng nhập. Ảnh trùng trong các thư mục chồng nhau chỉ được liệt kê một lần.

Ảnh nhập từ Google Photos và bộ ảnh nhận/4K do Wallta quản lý là bộ ảnh riêng. Drive riêng trên Android/TV dùng relay chỉ được đọc thư mục đã ghép quyền; muốn thêm thư mục riêng khác cần một nguồn/phiên ghép quyền khác.

## Nâng ảnh lên 4K

1. Bấm **4K** trên ảnh hoặc **Nguồn ảnh → Nâng ảnh 4K**.
2. Chọn một nguồn hoặc tất cả nguồn đang bật. Chọn chế độ **Nhanh** hoặc **AI**.
3. Bấm **Chạy tất cả**. Nếu chỉ cần một số ảnh, mở **Chọn từng ảnh**, chọn tối đa 50 ảnh rồi bấm **Chạy ảnh đã chọn**.
4. Tiến trình hiện ở cuối cửa sổ; vẫn có thể chuyển trang. Ảnh hoàn thành tự lưu vào một bộ ảnh riêng, ban đầu tắt. Chọn **Áp dụng khi xong** hoặc bấm **Áp dụng** sau khi chạy.

**Chạy tất cả** không bị giới hạn 50 ảnh như lựa chọn thủ công. Wallta chụp danh sách ảnh gốc lúc bắt đầu, xử lý tuần tự và loại bộ ảnh upscale do chính app quản lý. Số ảnh vẫn chịu giới hạn duyệt nguồn của thiết bị. Xem trước chỉ giữ tối đa 20 ảnh; số ảnh đã lưu có thể lớn hơn.

Hủy giữ các ảnh đã lưu. Đóng cửa sổ cấu hình hoặc chuyển trang vẫn tiếp tục; thoát hẳn app dừng tác vụ. Không tự tiếp tục queue sau khi khởi động lại. Ảnh hỏng có thể được bỏ qua; lỗi tài khoản, nguồn/cài đặt thay đổi, GPU hoặc lưu trữ dừng tác vụ và giữ phần đã hoàn thành.

Ảnh gốc giữ nguyên. Kết quả giữ tỷ lệ trong khung 3840 × 2160 với ảnh ngang hoặc 2160 × 3840 với ảnh dọc; ảnh vuông không bị kéo giãn thành 16:9. **Nhanh** dùng bộ lấy mẫu của nền tảng, không tái tạo chi tiết đã mất.

## Model AI trên Windows

AI xử lý tại máy, cần GPU/driver hỗ trợ Vulkan. Không cần API key; ảnh không được gửi tới dịch vụ AI. Android/TV hỗ trợ **Nhanh**. Bản xem trước web không xử lý ảnh.

| Model | Dùng cho | Dung lượng model |
|---|---|---:|
| Photo · RealESRGAN x4plus | Ảnh chụp, ảnh thông thường | 33.54 MB |
| Anime · RealESRGAN x4plus anime 6B | Anime, minh họa | 8.97 MB |
| Anime Light · RealESR AnimeVideo v3 x4 | Anime, hoạt hình; model gọn | 1.25 MB |

Bộ xử lý Windows dùng chung **6.34 MB**, tải một lần nếu chưa có. Dung lượng dùng đơn vị MB thập phân và bằng số byte file tải; không bao gồm dữ liệu mạng HTTP/TLS. App hiển thị **Chưa tải / Đã tải**; chỉ bấm chạy mới tải bộ xử lý và model đã chọn, không tải các model khác. File được kiểm tra dung lượng và SHA-256 trước khi dùng. Bộ x4plus đã cài và còn hợp lệ được dùng lại.

AI có thể làm mịn da, sai chữ hoặc tạo chi tiết khác ảnh gốc. Kích thước 4K không bảo đảm chi tiết như ảnh chụp 4K gốc; nên xem ảnh kết quả trước khi áp dụng. Tốc độ phụ thuộc GPU, RAM, driver và ảnh nguồn. Anime Light không phải chế độ tăng tốc dành cho ảnh chụp. Mỗi ảnh AI có giới hạn 10 phút, không giới hạn thời gian cả lô ở 30 phút.

## Dọn lưu trữ

Mở **Cài đặt → Lưu trữ**:

- **Dọn cache** xóa ảnh tải/preview và kết quả xử lý có thể tái tạo; giữ ảnh đã lưu và model AI.
- **Thư mục ảnh 4K** hiển thị từng bộ ảnh, số ảnh và dung lượng. Bấm biểu tượng xóa rồi xác nhận để xóa riêng bộ đã chọn. Ảnh gốc giữ nguyên.

Không dọn trong lúc xử lý/lưu ảnh. Nếu xóa bộ ảnh đang dùng, Wallta bỏ tham chiếu bộ đó; hình nền hệ thống đã đặt giữ nguyên tới lần áp dụng tiếp theo. Bộ ảnh từ 0.1.5/0.1.6 được nhận diện là legacy khi thiếu thông tin model.

## Bộ AI và giấy phép

File gốc từ [Real-ESRGAN NCNN/Vulkan Windows 20220424](https://github.com/xinntao/Real-ESRGAN/releases/tag/v0.2.5.0). Wallta phân phối riêng các file đã xác minh tại [AI models v1](https://github.com/mawta/wallta/releases/tag/ai-models-v1), kèm SHA256SUMS và giấy phép. Đây là bản hỗ trợ model, không phải bản cập nhật ứng dụng.

Giấy phép upstream được giữ tại [runtime](../src-tauri/src/upscale_runtime_LICENSE.txt) và [model](../src-tauri/src/upscale_model_LICENSE.txt). Mã SHA-256 từng file được cố định trong [catalog](../src-tauri/src/upscale_models.rs).
