# Kết nối Google

Bản cài chính thức có sẵn cấu hình Google. Người dùng không cần nhập Client ID, secret hoặc URL dịch vụ. Hồ sơ Google đang chờ duyệt, nên Google có thể hiện cảnh báo ứng dụng chưa xác minh.

| Thiết bị | Cách đăng nhập |
|---|---|
| Windows | **Kết nối tài khoản Google** → đăng nhập và cấp quyền trong trình duyệt |
| Điện thoại Android có Google Play services | **Kết nối tài khoản Google** → hộp chọn tài khoản hệ thống → cấp quyền |
| Android TV / thiết bị không có Play services | **Kết nối tài khoản Google** → quét QR bằng điện thoại → kiểm tra mã → đăng nhập Google |

Nếu đăng nhập hệ thống thất bại trên điện thoại, chọn **Kết nối bằng QR**. Local, USB và chuyển ảnh qua Wi-Fi không cần tài khoản Google.

## Google Drive

Kết nối rồi **Lưu kết nối** ngay; chưa cần chọn thư mục. Tài khoản đã lưu nằm ở **Cài đặt → Tài khoản đã kết nối**. Mở lại nguồn, vào **Nâng cao → Chọn / đổi thư mục** để chọn nhiều thư mục bất cứ lúc nào. Nguồn chưa có thư mục được lưu ở trạng thái tắt. Sau khi chọn thư mục, **Lưu & áp dụng** để bắt đầu dùng ảnh.

Wallta yêu cầu quyền đọc Drive và dùng API để đọc các thư mục ảnh đã chọn. Xóa nguồn sẽ xóa kết nối của nguồn đó, không xóa ảnh gốc trên Drive. Thu hồi toàn bộ quyền tại [Google Account permissions](https://myaccount.google.com/permissions).

## Google Photos

Kết nối tài khoản rồi **Mở Google Photos**, chọn ảnh và bấm **Done**. Trên TV, quét QR để mở trình chọn trên điện thoại. Quay lại Wallta và chờ ảnh nhập xong trước khi lưu. Mở nguồn đã lưu và bấm **Chọn lại ảnh** để thay bộ ảnh; hủy lần chọn mới giữ bộ ảnh cũ.

Photos Picker chỉ cấp quyền cho những ảnh đã chọn. Wallta tải bản sao về thiết bị, không tự đồng bộ album. Xóa nguồn đã nhập sẽ xóa bản sao do Wallta quản lý, giữ ảnh gốc trong Google Photos. Tích hợp Ambient cũ dành riêng cho project được Google duyệt trong Photos Partner Program.

## Dịch vụ QR và dữ liệu

Dịch vụ [Google Apps Script](../google-apps-script/README.md) xử lý cấp quyền, giữ email và token Google trong Script Properties riêng của dịch vụ. Người vận hành và các biên tập viên script đáng tin cậy có thể truy cập kho này. Thiết bị giữ capability được mã hóa bằng Android Keystore và nhận access token ngắn hạn; refresh token và Web client secret không được gửi vào app. Ảnh tải trực tiếp từ Google về thiết bị.

Mã đăng nhập hết hạn sau 10 phút; kết nối dịch vụ hết hạn sau 30 ngày không sử dụng. Hủy kết nối chưa lưu hoặc xóa nguồn sẽ yêu cầu dịch vụ xóa quyền của thiết bị. Cần mạng để yêu cầu này hoàn tất. Xem [chính sách dữ liệu](https://mawta.github.io/wallta/privacy.html).

## Bản tự build

- **Windows:** tạo OAuth client loại Desktop app; cung cấp JSON qua biến môi trường WALLTA_GOOGLE_OAUTH_CLIENT_JSON khi build, hoặc WALLTA_GOOGLE_OAUTH_CLIENT_FILE để chạy thử local. Không commit file credential. Callback loopback và PKCE được Wallta quản lý.
- **Android:** tạo OAuth client loại Android trong project đã bật Drive API và Photos Picker API, với package app.wallta và SHA-1 của chứng chỉ ký APK. Chứng chỉ debug và release cần client riêng. Không nhúng Web client secret vào APK.
- **TV/QR:** triển khai [Apps Script theo hướng dẫn](../google-apps-script/DEPLOY.vi.md), đăng ký Web OAuth client với URI /usercallback chính xác do endpoint health trả về. Đặt ID/secret/URL trong Script Properties và thay GoogleConnections.BROKER_URL khi build riêng.
- **Drive công khai:** tạo API key giới hạn cho Drive API. Trong Nâng cao, nhập link/ID thư mục công khai và API key. Không dùng key để thay thế OAuth cho ảnh riêng.

OAuth scopes: drive.readonly, photospicker.mediaitems.readonly và quyền email để hiển thị tài khoản. Dùng project ở chế độ Testing thì thêm tài khoản vào Test users. Bản Android ký khác hoặc project chưa bật API có thể báo lỗi cấp quyền.

Relay Laravel trong thư mục [relay](../relay/README.md) vẫn có cho các bản cũ hoặc triển khai riêng. Bản chính thức mới dùng kết nối hệ thống/Apps Script, không yêu cầu nhập URL relay.

Tài liệu chính thức: [Authorization trên Android](https://developers.google.com/identity/authorization/android), [Photos Picker](https://developers.google.com/photos/picker/guides/get-started-picker), [Apps Script web apps](https://developers.google.com/apps-script/guides/web).
