# Thiết lập Google cho Wallta

Không có secret dùng chung trong Git hay bộ cài. Chủ ứng dụng tạo Google Cloud project, cấu hình consent screen, bật API cần dùng và thêm tài khoản thử nghiệm nếu dự án còn Testing. Tài khoản chỉ nhìn thấy dữ liệu được cấp quyền. Token nhập tay chỉ dùng để thử; hết hạn cần nhập lại.

Trong app: **Nguồn ảnh → Thêm nguồn ảnh → Google Drive / Google Photos → Cách kết nối · lấy API key / Client ID**. Hướng dẫn có thể thu gọn, không cần rời app để đọc. Các bước bên dưới đối chiếu tài liệu Google ngày 27/10/2026.

Local, USB và **Gửi / nhận qua Wi-Fi** không cần Google Cloud, key hay tài khoản Google. Google Photos qua trình chọn ảnh trên điện thoại Android cũng không cần Client ID.

| Muốn kết nối | Cần tạo / chuẩn bị | Nhập trong Wallta |
|---|---|---|
| Drive công khai | Drive API + API key | Link thư mục + Google API key trong Thông tin kết nối |
| Drive riêng, Windows | Drive API + OAuth Desktop app | OAuth Client ID / secret nếu được cấp; chọn thư mục sau |
| Drive riêng, Android / TV | Relay HTTPS + OAuth Web application | Link thư mục + địa chỉ Wallta Drive Relay |
| Photos, Windows | Photos Picker API + OAuth Desktop app | OAuth Client ID / secret nếu được cấp |
| Photos, điện thoại Android | Trình chọn ảnh hệ thống / cloud provider | Bấm kết nối và chọn ảnh, không nhập key |
| Album Photos tự đổi trên TV | Quyền Ambient từ Partner Program + OAuth TVs and Limited Input devices | Client ID / secret; Device ID nhận được sau kết nối |

## Tạo project và OAuth consent

1. Mở [Google Cloud Console](https://console.cloud.google.com/), đăng nhập tài khoản quản lý dự án. Ở bộ chọn project trên cùng, tạo project mới hoặc chọn project riêng cho Wallta.
2. **APIs & Services → Library**, tìm API cần dùng ở bảng trên, bấm **Enable**. API key và OAuth client phải thuộc chính project đã bật API.
3. Với OAuth: **Google Auth platform → Branding → Get Started**, điền tên ứng dụng, email hỗ trợ và liên hệ; hoàn tất theo yêu cầu Google.
4. **Audience**: tài khoản Gmail cá nhân thường dùng **External**. Khi dự án ở Testing, thêm chính email đăng nhập vào **Test users**. Internal chỉ phù hợp tổ chức Workspace đủ điều kiện.
5. **Data Access → Add or Remove Scopes**: Drive dùng `https://www.googleapis.com/auth/drive.readonly`; Photos Picker dùng `https://www.googleapis.com/auth/photospicker.mediaitems.readonly`. Ambient theo quyền Google đã duyệt cho project.
6. **Clients → Create client**, chọn đúng loại trong bảng. Copy Client ID và secret nếu được cấp. Không tạo một client Android thông thường rồi dán vào ô Desktop/TV/Web.

[Hướng dẫn consent chính thức](https://developers.google.com/workspace/guides/configure-oauth-consent) · [Tạo credential theo loại ứng dụng](https://developers.google.com/workspace/guides/create-credentials).

## Google Drive công khai

Bật Google Drive API, tạo API key giới hạn cho API này, chia sẻ thư mục phù hợp. Thêm nguồn Drive với link `/drive/folders/ID` hoặc ID, mở Thông tin kết nối và nhập API key. Thư mục công khai vẫn cần API key để gọi API liệt kê. Không suy luận danh sách ảnh từ HTML của Drive.

1. Bật [Google Drive API](https://console.cloud.google.com/apis/library/drive.googleapis.com).
2. **APIs & Services → Credentials → Create credentials → API key**. Mở key vừa tạo, **API restrictions → Restrict key → Google Drive API**.
3. Key này gọi từ bản cài, không phải website. Không đặt giới hạn HTTP referrer rồi mong bản cài dùng được. Nếu giới hạn theo IP, IP phải khớp thiết bị/mạng gọi API; không chia sẻ key ra ngoài phạm vi sử dụng.
4. Trên Drive, **Share → General access → Anyone with the link → Viewer**. Chỉ chọn cách này nếu chủ động muốn người có link xem được ảnh. Kiểm tra quyền các file bên trong thư mục.
5. Trong Wallta, điền tên nguồn và link thư mục. Mở **Thông tin kết nối**, dán vào **Google API key · thư mục công khai**, rồi thêm nguồn.

Không cần OAuth Client ID cho cách này. Với ảnh riêng, giữ thư mục riêng và dùng OAuth/relay bên dưới.

## Google Drive riêng trên Windows

Tạo OAuth client loại **Desktop app**. Trong nguồn Drive, nhập OAuth Client ID và client secret nếu Google cấp. Bấm **Kết nối tài khoản Google**, cấp quyền đọc Drive trong trình duyệt rồi **Lưu kết nối**. Không cần chọn thư mục ngay. Callback loopback `http://127.0.0.1:<port>/oauth/callback` dùng PKCE và state. Refresh token lưu trong Windows Credential Manager sau khi lưu nguồn thành công.

Tài khoản đã lưu nằm ở **Cài đặt → Tài khoản đã kết nối → Quản lý**. Khi mở **Thêm nguồn → Google Drive**, chọn **Dùng kết nối** để mở lại tài khoản cũ, không cần nhập lại Client ID/secret. Bấm **Chọn / đổi thư mục** bất cứ lúc nào rồi **Lưu & áp dụng**. Form đăng nhập trống dành cho kết nối mới, không biểu thị kết nối cũ bị mất. Google Photos đã nhập cũng xuất hiện trong danh sách tài khoản để mở lại bộ ảnh hoặc chọn lại ảnh.

Trong **Google Auth platform → Clients → Create client → Desktop app**, đặt tên như `Wallta Windows`. Không cần đăng ký redirect URI relay cho loại Desktop app. Dùng đúng Client ID của Desktop app; Wallta tự chọn cổng callback local. OAuth app ở Testing có thể cần kết nối lại khi Google làm refresh token hết hạn.

Scope đọc toàn Drive được Google cấp cho ứng dụng; Wallta chỉ liệt kê thư mục đã chọn. Muốn ngắt quyền local, xóa nguồn. Để thu hồi hoàn toàn quyền ở phía Google, dùng [trang quyền tài khoản Google](https://myaccount.google.com/permissions). Xóa nguồn không xóa ảnh gốc trên Drive.

## Drive riêng trên Android/TV

Chạy [relay Laravel](../relay/README.md) tại HTTPS với OAuth client loại **Web application**. Trong Wallta, nhập thư mục Drive và địa chỉ relay, bấm kết nối, quét QR bằng điện thoại. Kiểm tra đúng tên thiết bị/thư mục trên trang relay trước khi đăng nhập Google; quay lại TV rồi lưu nguồn.

Người triển khai relay tạo **Clients → Create client → Web application**, đăng ký **Authorized redirect URIs** chính xác `https://TEN-MIEN-RELAY/oauth/callback`. Trên máy chủ relay đặt `APP_URL=https://TEN-MIEN-RELAY`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, tạo `APP_KEY` và cấu hình HTTPS theo README relay. Trong TV/điện thoại chỉ nhập URL relay; không cần copy secret Web application vào thiết bị. Repo chưa có relay công cộng để nhập sẵn.

Google refresh/access token ở relay, được mã hóa bằng APP_KEY. TV giữ một capability mã hóa bằng Android Keystore, giới hạn truy cập đúng thư mục qua relay. Hủy hộp thêm nguồn sẽ hủy pairing đang chờ; xóa nguồn thu hồi capability. Relay chỉ là tùy chọn cho nguồn riêng, không cần cho local/URL/JSON/WebDAV.

Không dùng device OAuth với `drive.readonly` trên TV: Google giới hạn scope của [luồng thiết bị đầu vào hạn chế](https://developers.google.com/identity/protocols/oauth2/limited-input-device).

## Google Photos trên TV

Google giới hạn **Ambient API** cho thành viên đã được chấp thuận trong [Photos Partner Program](https://developers.google.com/photos/partner-program/overview). Có Cloud project thông thường chưa đủ để bật luồng này. Sau khi được chấp thuận, dùng cấu hình Ambient do Google cung cấp và OAuth client **TVs and Limited Input devices**.

Trong nguồn Photos, nhập client ID/secret, bấm kết nối và dùng điện thoại mở QR/mã xác nhận. Sau bước cấp quyền, chọn album qua trang cài đặt thiết bị Ambient, chờ trạng thái sẵn sàng và lưu nguồn. Wallta tôn trọng khoảng polling, giới hạn lượt liệt kê, không dùng Photos cache làm nguồn offline; ngắt quyền/xóa nguồn xóa cache thuộc nguồn.

Tích hợp Ambient chưa được kiểm chứng với project được cấp quyền hoặc TV thật. Nếu Google chưa cấp Ambient, dùng Drive/local hoặc nhập ảnh đã chọn.

## Google Photos trên Windows và Android

Windows dùng OAuth client Desktop app và [Google Photos Picker API](https://developers.google.com/photos/picker/guides/get-started-picker). Bật Picker API, đăng nhập, chọn các ảnh trong giao diện Google rồi Wallta tải bản sao thành một nguồn local. Giới hạn 500 ảnh/512 MB mỗi lần nhập. Bộ ảnh không tự theo dõi thay đổi album; nhập lại khi cần.

Android mở trình chọn ảnh hệ thống, dùng ACTION_OPEN_DOCUMENT trên thiết bị cũ. Có thể chọn Google Photos/cloud chỉ khi nhà cung cấp đã được cài và tích hợp với trình chọn ảnh. Giới hạn 50 ảnh/256 MB; không cần OAuth client ID trên điện thoại. Ảnh đã nhập là bản sao local do Wallta quản lý; xóa nguồn sẽ xóa những bản sao đó, không xóa ảnh trên Google.

## Khắc phục lỗi kết nối

| Lỗi / hiện tượng | Kiểm tra |
|---|---|
| `access_denied`, tài khoản chưa được phép | Email có trong Audience → Test users của đúng project; không chọn nhầm tài khoản |
| `redirect_uri_mismatch` ở relay | Scheme HTTPS, domain, cổng nếu có và `/oauth/callback` trùng hoàn toàn; APP_URL đúng domain bên ngoài |
| `API not enabled`, `SERVICE_DISABLED`, 403 | API đã bật ở project sở hữu key/client; API restrictions đúng API; quyền xem thư mục/file đủ |
| `invalid_client` | Loại OAuth client đúng nền tảng; ID/secret cùng một client và không thừa khoảng trắng |
| Token nhập tay dùng được rồi hết | Kết nối OAuth để quản lý refresh token; không xem access token là key vĩnh viễn |
| Photos TV không bật được API | Cần được duyệt Ambient Partner, không giải quyết bằng tạo thêm API key thông thường |
| Android picker không có Photos | Cập nhật Google Photos/nhà cung cấp cloud; nếu firmware không hỗ trợ thì nhập local hoặc chuyển qua Wi-Fi |

## Bảo quản credential

Không đưa secret vào manifest JSON, URL hoặc Git. Frontend không nhận lại credential đã lưu. Các ô để trống khi chỉnh sửa giữ credential cũ nếu trường bị bỏ qua; chuỗi rỗng gửi rõ ràng sẽ xóa credential. Nguồn mới phải lưu thành công mới sử dụng quyền OAuth đang chờ. Khóa APP_KEY relay cần lưu bền và sao lưu kín; đổi khóa làm token mã hóa cũ không đọc được.
# Chọn và đổi nguồn đã lưu

Windows: kết nối Google Drive → chọn thư mục → **Lưu & áp dụng**. Có thể bấm **Lưu nguồn** trước khi chọn thư mục; nguồn hiện **Chưa chọn thư mục**. Sau đó vào **Nguồn ảnh → Chọn / đổi thư mục**, không cần tạo nguồn hay đăng nhập lại khi quyền vẫn còn hiệu lực.

**Lưu & áp dụng** chọn ảnh từ chính thư mục đó và bật tự đổi. Tùy chọn **Khởi động cùng Windows** nằm ngay trong form và có thể đổi ở Cài đặt. Các nguồn khác đang bật vẫn tham gia vòng đổi ảnh. Tổng quan hiện nhà cung cấp, tài khoản và thư mục của ảnh hiện tại; thẻ nguồn phân biệt **Đang hiển thị**, **Đã bật** và **Đã tắt**.

Google Photos trên Windows lưu bản sao bộ ảnh đã chọn và giữ nhãn Google Photos sau khi nhập. **Chọn lại ảnh** trên nguồn đã lưu để thay bộ ảnh. Hủy lần chọn mới giữ nguyên bộ ảnh cũ. Không tự theo dõi ảnh mới trong album Google Photos có sẵn; cần tính năng này hãy dùng thư mục Drive.

Android/TV Drive vẫn chọn link thư mục trước khi cấp quyền relay. TV Photos dùng album qua Ambient khi project đã được Google cấp quyền.
