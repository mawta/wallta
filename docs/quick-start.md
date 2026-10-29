# Dùng thử Wallta

## Cài đặt

- Windows 10/11 x64: mở bộ cài `Wallta_0.1.3_x64-setup.exe`. Windows có thể cảnh báo nhà phát hành chưa xác định vì chưa ký Authenticode.
- Android/Android TV 8+: cho phép cài từ nguồn đang dùng rồi mở `Wallta_0.1.3_android.apk`. Nếu đã cài APK debug cũ, có thể phải gỡ trước vì khác khóa ký. Gỡ app xóa cấu hình và ảnh đã nhập vào app; sao lưu ảnh cần giữ trước.
- Tải bộ cài tại GitHub Releases công khai, không cần đăng nhập. Không cần tài khoản Wallta.

Từ 0.1.2, mở **Cài đặt → Cập nhật** để kiểm tra, tải và cài bản mới. App tự kiểm tra tối đa mỗi ngày khi mở. [Chi tiết cập nhật](updating.md).

## Bộ ảnh đầu tiên

1. Mở **Nguồn ảnh → Thêm nguồn ảnh**, chọn nguồn trên thiết bị, đặt tên và **Chọn thư mục ảnh**. Có thể chọn USB và bật quét thư mục con.
2. Bấm **Lưu & áp dụng** để đổi ảnh ngay và bật tự đổi. Bỏ chọn **Áp dụng sau khi lưu** nếu chỉ muốn lưu nguồn. Với TV, trở về Home để xác nhận launcher thật sự hiển thị wallpaper.
3. Trong **Lịch & hiển thị**, chọn nhịp đổi ảnh, ngẫu nhiên/tuần tự, cách lấp đầy và lịch ngày/giờ nếu cần. Bấm **Lưu cài đặt**.
4. Trên Windows, giữ chọn **Khởi động cùng Windows** khi lưu và áp dụng nếu muốn chạy sau đăng nhập. Có thể đổi tùy chọn này trong **Cài đặt → Khởi động cùng thiết bị**; bật/tắt tự đổi ở **Tổng quan**.

Android đổi wallpaper nền tối thiểu 15 phút và có thể trễ do Doze/chính sách pin. Windows cần Wallta còn chạy ở khay hệ thống; đóng cửa sổ sẽ ẩn xuống khay, chọn Quit sẽ dừng ứng dụng.

## Screensaver trên TV

Mở **Cài đặt → Screensaver của TV → Thiết lập**, chọn Wallta trong menu của hệ thống. Chu kỳ trong **Trình chiếu & screensaver** độc lập với chu kỳ wallpaper nền; tùy chọn đồng hồ nằm trong **Lịch & hiển thị**. Nhấn phím remote để thoát screensaver. Tên menu và khả năng kích hoạt tùy firmware.

Phím mũi tên di chuyển focus, Enter chọn. Trong ô nhập dùng Up/Down để rời ô; trong danh sách chọn dùng Left/Right để rời danh sách.

## Nguồn khác

- Chuyển ảnh giữa điện thoại và TV/PC: **Nguồn ảnh → Gửi / nhận qua Wi-Fi**. Máy kia mở QR/link bằng trình duyệt; nhận xong xác nhận thêm ảnh. [Các bước chuyển ảnh](local-transfer.md).

- Link online: dùng một URL ảnh, JSON danh sách ảnh hoặc WebDAV. Một trang web chia sẻ thư mục bất kỳ không bảo đảm liệt kê được ảnh. [Định dạng JSON](../README.md#nguồn-online).
- Drive: thư mục công khai cần API key. Drive riêng dùng OAuth Windows hoặc [relay HTTPS](../relay/README.md) trên Android/TV. Trên Windows, có thể lưu tài khoản trước, rồi bấm **Chọn thư mục** trên nguồn đã lưu. Chọn xong bấm **Lưu & áp dụng**; lần sau dùng **Đổi thư mục**. Tài khoản và thư mục được hiển thị ngay trên nguồn.
- Photos: [cần thiết lập Google](setup-google.md). TV dùng Ambient với quyền Partner Program; Windows dùng Picker để nhập bộ ảnh đã chọn; điện thoại dùng trình chọn ảnh hệ thống/cloud nếu có. Bộ ảnh nhập không tự theo dõi album.

Để ngắt nguồn, xóa nguồn trong **Nguồn ảnh**. **Dọn bộ nhớ** chỉ xóa cache, không xóa ảnh gốc trong thư mục của Sếpp; ảnh nhập riêng do Wallta quản lý sẽ bị xóa khi xóa chính nguồn nhập đó.

[Các giới hạn và ca cần kiểm thử thiết bị](testing.md).
