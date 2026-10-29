# Nhiều thư mục và ảnh 4K

## Nguồn ảnh

Mở **Nguồn ảnh → Chỉnh sửa**. Chọn thêm thư mục trên thiết bị, chọn thư mục bằng tài khoản Drive đã lưu, hoặc nhập URL WebDAV và bấm **Thêm thư mục**. Có thể bỏ từng thư mục rồi lưu lại. Tối đa 50 thư mục mỗi nguồn; các thư mục WebDAV dùng chung máy chủ và thông tin đăng nhập. Ảnh trùng trong các thư mục chồng nhau chỉ được liệt kê một lần.

Ảnh nhập từ Google Photos và bộ ảnh nhận/4K do Wallta quản lý là bộ ảnh riêng. Drive riêng trên Android/TV dùng relay chỉ được đọc thư mục đã ghép quyền; muốn thêm thư mục riêng khác cần một nguồn/phiên ghép quyền khác.

## Nâng ảnh lên 4K

1. Bấm **4K** trên ảnh hoặc **Nguồn ảnh → Nâng ảnh 4K**.
2. Chọn tối đa 50 ảnh và chế độ **Nhanh** hoặc **AI**.
3. Bấm **Tạo ảnh 4K**, xem ảnh trước–sau. Có thể hủy trong lúc xử lý.
4. Bấm **Lưu ảnh 4K** để thêm nguồn local **Ảnh 4K**. Chọn **Áp dụng sau khi lưu** nếu muốn dùng ngay.

Ảnh gốc giữ nguyên. Kết quả giữ tỷ lệ trong khung 3840 × 2160 với ảnh ngang hoặc 2160 × 3840 với ảnh dọc; ảnh vuông không bị kéo giãn thành 16:9. Nguồn kết quả được lưu riêng, dùng được trong trình chiếu và gửi qua Wi-Fi.

**Nhanh** phóng ảnh bằng bộ lấy mẫu của nền tảng. Chế độ này không tái tạo chi tiết đã mất. **AI** dùng Real-ESRGAN x4plus trên Windows, xử lý tại máy, cần GPU/driver hỗ trợ Vulkan. Lần đầu Wallta tải bộ xử lý/model chính thức và kiểm tra SHA-256. Không cần API key; ảnh không được gửi tới dịch vụ AI.

AI có thể làm mịn da, sai chữ hoặc tạo chi tiết khác ảnh gốc. Nên xem trước khi lưu. Tốc độ và khả năng chạy phụ thuộc GPU, RAM, driver và ảnh nguồn; chưa có kết quả đo trên mọi thiết bị. Android/TV chỉ hỗ trợ Nhanh; bản xem trước web cần bản cài để xử lý.

Từ 0.1.6, AI trên Windows giới hạn 10 phút cho mỗi ảnh, không ngắt cả lô sau 30 phút. Nếu một ảnh gặp lỗi, lô dừng và vẫn có thể xem/lưu các ảnh đã hoàn thành, theo số lượng hiện trên màn hình. Ảnh chưa hoàn thành không được đưa vào bộ ảnh đã lưu. Hủy hoặc đóng khi chưa lưu sẽ xóa kết quả tạm; thay đổi nguồn/cài đặt khiến kết quả cũ không còn lưu được.

Đổi nguồn/cài đặt trong khi xử lý làm kết quả cũ hết hiệu lực; tạo lại sau khi hoàn tất thay đổi. Đóng cửa sổ nâng ảnh khi chưa lưu sẽ hủy kết quả tạm. Ảnh đã lưu vẫn nằm trong dữ liệu ứng dụng và không bị xóa khi dọn cache.

## Bộ AI và giấy phép

Runtime: [Real-ESRGAN-ncnn-vulkan](https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan), bản Windows `20220424` từ [Real-ESRGAN v0.2.5.0](https://github.com/xinntao/Real-ESRGAN/releases/tag/v0.2.5.0). SHA-256 archive:

```text
abc02804e17982a3be33675e4d471e91ea374e65b70167abc09e31acb412802d
```

Model: [Real-ESRGAN x4plus](https://github.com/xinntao/Real-ESRGAN). Giấy phép upstream được giữ tại [runtime](../src-tauri/src/upscale_runtime_LICENSE.txt) và [model](../src-tauri/src/upscale_model_LICENSE.txt).
