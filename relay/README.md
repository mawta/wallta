# Wallta Drive Relay

Relay Laravel 12 tùy chọn cho Google Drive riêng trên Android/TV. Máy chủ giữ Google refresh/access token đã mã hóa; TV chỉ giữ capability riêng cho một thư mục. Đây là mã nguồn để tự triển khai, chưa có relay công cộng được vận hành sẵn.

## Cấu hình

Cần PHP 8.2+, Composer, HTTPS và cache/session bền. Cấu hình mặc định dùng file cache/session, không cần DB cho luồng relay. Với nhiều máy chủ, dùng Redis chung hỗ trợ atomic locks và session chung.

```sh
composer install --no-dev --prefer-dist --optimize-autoloader
cp .env.example .env
php artisan key:generate
```

Sửa `.env`: `APP_ENV=production`, `APP_DEBUG=false`, `APP_URL=https://relay.example.com`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `SESSION_SECURE_COOKIE=true`. Tạo OAuth client **Web application**, đăng ký redirect URI chính xác `https://relay.example.com/oauth/callback`, bật Google Drive API và consent scope `https://www.googleapis.com/auth/drive.readonly`.

Webroot phải là `public/`, storage và bootstrap/cache phải ghi được. Chỉ expose HTTPS. Nếu dùng proxy, cấu hình trusted proxy theo hạ tầng thực tế để Laravel nhận đúng HTTPS; không tin mọi proxy từ Internet. Không log body/token hoặc URL callback query tại reverse proxy. Chạy `php artisan config:cache` sau khi sửa cấu hình. Giữ APP_KEY và cache mã hóa riêng tư; không xóa toàn cache nếu muốn giữ kết nối đang dùng.

TV: nhập URL relay trong nguồn Drive, nhập folder ID/link và tạo QR. Điện thoại mở QR, xác nhận thiết bị/thư mục rồi cấp quyền Google. Quay lại TV lưu nguồn. Nếu relay trả lỗi cấu hình/Google chưa duyệt, ứng dụng hiển thị lỗi và không giả lập thành công.

## Hợp đồng API

| Thao tác | Yêu cầu | Kết quả |
|---|---|---|
| `POST /api/pair` | `{folderId,displayName}` | `deviceCode,verificationUrl,pollIntervalSeconds,expiresIn` |
| `POST /api/pair/poll` | `{deviceCode}` | `phase`; khi ready có `token,location` |
| `DELETE /api/pair` | `{deviceCode}` | Hủy pair và capability nếu đã cấp |
| `GET /api/drive/files` | Bearer capability, `folderId,pageToken?` | Drive file/folder IDs và nextPageToken |
| `GET /api/drive/media/{id}` | Bearer capability | Nội dung ảnh tối đa 32 MB |
| `DELETE /api/connection` | Bearer capability | Thu hồi capability |

Pair hết hạn sau 15 phút. Capability tồn tại tối đa 30 ngày kể từ lần ghi/refresh; hết hạn cần kết nối lại. Mỗi API có bucket rate limit riêng theo IP để việc tải ảnh không khóa callback/disconnect. Cùng IP vẫn chia sẻ hạn mức của cùng thao tác. Refresh và thu hồi được khóa tránh tái tạo credential sau ngắt kết nối. Google folder alias `root` được chuyển sang ID thật khi cấp quyền.

Server kiểm tra metadata/chuỗi parent trước khi cho đọc ảnh/thư mục; tối đa 16 lần duyệt parent, không hỗ trợ shortcut sang nơi khác. Bộ lập lịch phía TV cũng giới hạn độ sâu/số thư mục/số ảnh. Quyền OAuth upstream là đọc Drive; capability downstream chỉ dùng cho thư mục đã xác nhận. Ngắt kết nối xóa credential local trên relay; thu hồi toàn bộ consent qua trang Google Account Permissions nếu cần. Không tự revoke refresh token của cả project vì có thể ảnh hưởng thiết bị khác.

Hủy pairing có thể gửi yêu cầu xóa trong background; nếu mất mạng, mã pair sẽ hết hạn và credential server sẽ hết TTL. Nếu token đã được cấp, client thử thu hồi cả pair lẫn connection. Ảnh đi qua RAM relay, không lưu lâu dài hoặc phục vụ như file công khai.

## Kiểm thử

```sh
composer install
vendor/bin/phpunit
```

Tests fake Google endpoints, kiểm tra PKCE/state, pending/denied/cancel, credential không lộ, giới hạn thư mục, refresh/disconnect và rate limit độc lập. Workflow `Drive relay` chạy cùng suite. Kiểm tra OAuth thật cần credential, consent và một HTTPS relay đã triển khai; chưa thực hiện trong môi trường phát triển này.
