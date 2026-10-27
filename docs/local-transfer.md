# Gửi / nhận ảnh qua Wi-Fi

Không cần key Google hoặc Internet. Cài Wallta trên **ít nhất một** thiết bị; thiết bị kia dùng trình duyệt. Hỗ trợ điện thoại ↔ Android TV/box và điện thoại ↔ Windows.

1. Cho hai máy vào cùng Wi-Fi/LAN, mở Wallta → **Nguồn ảnh → Gửi / nhận qua Wi-Fi**.
2. Nếu muốn gửi ảnh từ Wallta, chọn ảnh local cho máy kia tải. Để trống nếu chỉ muốn nhận ảnh.
3. Bấm **Bật gửi / nhận**. Giữ ứng dụng và màn hình mở.
4. Máy kia quét QR hoặc mở link hiển thị. Chọn ảnh rồi **Gửi ảnh**, hoặc bấm **Tải ảnh** ở ảnh đã được chia sẻ.
5. Trở lại Wallta, bấm **Thêm ảnh đã nhận** để lưu thành nguồn local mới. Ảnh gốc trên máy gửi không thay đổi. Khi chỉ tải ảnh ra, tải xong có thể bấm **Dừng & đóng**.

Phiên tự dừng sau 10 phút. Đóng hộp thoại, dừng hoặc hết hạn sẽ bỏ ảnh chưa xác nhận; ảnh đã lưu thành nguồn local vẫn còn. Mỗi phiên tối đa 50 ảnh / 256 MB nhận vào, mỗi ảnh không quá 25 MB, JPG/PNG/WebP hợp lệ. Chỉ những ảnh local được chủ động chọn mới được tải ra; nguồn Drive/Photos online không được tự xuất qua LAN.

Link có mã ngẫu nhiên riêng mỗi phiên. Ai có link trong thời gian phiên còn mở có thể gửi ảnh và tải các ảnh đã chọn. Dùng mạng tin cậy: giao thức HTTP trong LAN không mã hóa. Không chuyển tiếp cổng ra Internet hoặc gửi QR cho người ngoài. Wallta không tự thay đổi firewall/router.

## Nếu không mở được link

- Kiểm tra cả hai máy cùng mạng, không dùng guest Wi-Fi hoặc mạng bật AP/client isolation.
- Nếu có nhiều card mạng, đổi **Địa chỉ mạng** trong hộp gửi/nhận và quét QR tương ứng. Không dùng IP của adapter ảo/VPN nếu điện thoại không truy cập được.
- Windows: cho phép Wallta qua firewall trên mạng riêng khi hệ thống hỏi. Không cần mở trên mạng công cộng.
- TV/điện thoại: giữ Wallta ở trước màn hình; tắt app hoặc đổi mạng cần tạo phiên mới.
- Trình duyệt hỏi quyền mạng local: chỉ cấp quyền khi đúng địa chỉ IP/QR đang hiển thị trong Wallta.
- Gửi lỗi giữa chừng: bấm **Gửi tiếp phần còn lại**. Trình duyệt chỉ thử lại từ ảnh chưa báo thành công. Nếu mất mạng ngay khi máy nhận đã lưu nhưng chưa trả lời, kiểm tra số ảnh nhận trước khi thử lại ảnh đó.

Kiểm thử tự động dùng TCP loopback để xác nhận giao thức, giới hạn và cleanup. Khả năng đi qua Wi-Fi/firewall/firmware thực tế cần thử trên chính cặp thiết bị.
