# Thiết lập Google cho Wallta

Không có secret dùng chung trong Git hay bộ cài. Chủ ứng dụng tạo Google Cloud project, cấu hình consent screen, bật API cần dùng và thêm tài khoản thử nghiệm nếu dự án còn Testing. Tài khoản chỉ nhìn thấy dữ liệu được cấp quyền. Token nhập tay chỉ dùng để thử; hết hạn cần nhập lại.

## Google Drive công khai

Bật Google Drive API, tạo API key giới hạn cho API này, chia sẻ thư mục phù hợp. Thêm nguồn Drive với link `/drive/folders/ID` hoặc ID, mở Thông tin kết nối và nhập API key. Thư mục công khai vẫn cần API key để gọi API liệt kê. Không suy luận danh sách ảnh từ HTML của Drive.

## Google Drive riêng trên Windows

Tạo OAuth client loại **Desktop app**. Trong nguồn Drive, nhập link thư mục và OAuth Client ID; nhập client secret nếu Google cấp. Bấm Kết nối tài khoản Google, cấp quyền đọc Drive trong trình duyệt rồi lưu nguồn ảnh. Callback loopback `http://127.0.0.1:<port>/oauth/callback` dùng PKCE và state. Refresh token lưu trong Windows Credential Manager sau khi lưu nguồn thành công.

Scope đọc toàn Drive được Google cấp cho ứng dụng; Wallta chỉ liệt kê thư mục đã chọn. Muốn ngắt quyền local, xóa nguồn. Để thu hồi hoàn toàn quyền ở phía Google, dùng [trang quyền tài khoản Google](https://myaccount.google.com/permissions). Xóa nguồn không xóa ảnh gốc trên Drive.

## Drive riêng trên Android/TV

Chạy [relay Laravel](../relay/README.md) tại HTTPS với OAuth client loại **Web application**. Trong Wallta, nhập thư mục Drive và địa chỉ relay, bấm kết nối, quét QR bằng điện thoại. Kiểm tra đúng tên thiết bị/thư mục trên trang relay trước khi đăng nhập Google; quay lại TV rồi lưu nguồn.

Google refresh/access token ở relay, được mã hóa bằng APP_KEY. TV giữ một capability mã hóa bằng Android Keystore, giới hạn truy cập đúng thư mục qua relay. Hủy hộp thêm nguồn sẽ hủy pairing đang chờ; xóa nguồn thu hồi capability. Relay chỉ là tùy chọn cho nguồn riêng, không cần cho local/URL/JSON/WebDAV.

Không dùng device OAuth với `drive.readonly` trên TV: Google giới hạn scope của [luồng thiết bị đầu vào hạn chế](https://developers.google.com/identity/protocols/oauth2/limited-input-device).

## Google Photos trên TV

Google giới hạn **Ambient API** cho thành viên đã được chấp thuận trong [Photos Partner Program](https://developers.google.com/photos/partner-program/overview). Có Cloud project thông thường chưa đủ để bật luồng này. Sau khi được chấp thuận, dùng cấu hình Ambient do Google cung cấp và OAuth client **TVs and Limited Input devices**.

Trong nguồn Photos, nhập client ID/secret, bấm kết nối và dùng điện thoại mở QR/mã xác nhận. Sau bước cấp quyền, chọn album qua trang cài đặt thiết bị Ambient, chờ trạng thái sẵn sàng và lưu nguồn. Wallta tôn trọng khoảng polling, giới hạn lượt liệt kê, không dùng Photos cache làm nguồn offline; ngắt quyền/xóa nguồn xóa cache thuộc nguồn.

Tích hợp Ambient chưa được kiểm chứng với project được cấp quyền hoặc TV thật. Nếu Google chưa cấp Ambient, dùng Drive/local hoặc nhập ảnh đã chọn.

## Google Photos trên Windows và Android

Windows dùng OAuth client Desktop app và [Google Photos Picker API](https://developers.google.com/photos/picker/guides/get-started-picker). Bật Picker API, đăng nhập, chọn các ảnh trong giao diện Google rồi Wallta tải bản sao thành một nguồn local. Giới hạn 500 ảnh/512 MB mỗi lần nhập. Bộ ảnh không tự theo dõi thay đổi album; nhập lại khi cần.

Android mở trình chọn ảnh hệ thống, dùng ACTION_OPEN_DOCUMENT trên thiết bị cũ. Có thể chọn Google Photos/cloud chỉ khi nhà cung cấp đã được cài và tích hợp với trình chọn ảnh. Giới hạn 50 ảnh/256 MB; không cần OAuth client ID trên điện thoại. Ảnh đã nhập là bản sao local do Wallta quản lý; xóa nguồn sẽ xóa những bản sao đó, không xóa ảnh trên Google.

## Bảo quản credential

Không đưa secret vào manifest JSON, URL hoặc Git. Frontend không nhận lại credential đã lưu. Các ô để trống khi chỉnh sửa giữ credential cũ nếu trường bị bỏ qua; chuỗi rỗng gửi rõ ràng sẽ xóa credential. Nguồn mới phải lưu thành công mới sử dụng quyền OAuth đang chờ. Khóa APP_KEY relay cần lưu bền và sao lưu kín; đổi khóa làm token mã hóa cũ không đọc được.
