# Triển khai Apps Script cho Wallta

Không cần VPS. Chỉ chủ ứng dụng thiết lập một lần; sau khi gắn URL vào bản cài, người dùng không phải nhập URL hoặc credentials.

## 1. Tạo script

1. Mở [Apps Script](https://script.google.com/), tạo dự án **Wallta Google Connect**.
2. Dán nội dung [Code.gs](Code.gs) vào file `Code.gs` mặc định.
3. **Project Settings → Show appsscript.json manifest file in editor**. Mở `appsscript.json`, thay bằng [manifest trong thư mục này](appsscript.json). Không cần thêm thư viện.
4. Trong **Project Settings → IDs**, copy **Script ID**. Đây không phải ID deployment hoặc Google Cloud project.

## 2. Tạo OAuth client riêng

Trong Google Cloud project **wallta → Google Auth Platform → Clients**, tạo client loại **Web application**, tên **Wallta Android TV Connect**. Không dùng client Windows Desktop đang có.

Thêm **Authorized redirect URI** chính xác:

```text
https://script.google.com/macros/d/SCRIPT_ID/usercallback
```

Thay `SCRIPT_ID` bằng ID ở bước 1. Callback không phải URL `/exec`.

Project bật Google Drive API và Google Photos Picker API. Consent dùng `openid`, `userinfo.email`, `drive.readonly`, `photospicker.mediaitems.readonly`; mỗi lần kết nối chỉ xin quyền Drive hoặc Picker tương ứng. Photos Picker không phải API đọc toàn bộ thư viện Google Photos.

Trong Apps Script **Project Settings → Script Properties**, thêm:

| Property | Giá trị |
|---|---|
| `GOOGLE_CLIENT_ID` | Client ID của **Web application** vừa tạo |
| `GOOGLE_CLIENT_SECRET` | Client secret của client đó |

Giữ secret trong Script Properties; không dán lên GitHub hoặc gửi vào chat. Script chạy dưới tài khoản chủ triển khai nhưng chỉ dùng token của người đã cấp quyền qua OAuth; không dùng quyền Drive của chủ script.

## 3. Deploy

1. **Deploy → New deployment → Web app**.
2. **Execute as: Me**.
3. **Who has access: Anyone**. Đây là truy cập API, không phải tự cấp quyền Drive cho người lạ; mỗi tài khoản vẫn phải bấm đồng ý tại Google. Nếu tài khoản Workspace không cho phép Anyone, deployment này chưa dùng được cho TV công khai.
4. Bấm **Deploy**, cấp quyền chạy script khi Google yêu cầu.
5. Copy **Web app URL** kết thúc bằng `/exec`.
6. Thêm Script Property `WALLTA_WEB_APP_URL` bằng chính URL đó.
7. Mở URL `/exec`: phải thấy `service` là `wallta-google-auth`, `protocol` là `1` và `configured` là `true`. Trường `redirectUri` phải trùng URI đã đăng ký ở Google Cloud.

**Gửi emm URL `/exec` để gắn vào app và kiểm tra OAuth thật. Không gửi Client secret.** Bản 0.1.11 chưa sử dụng script này.

Khi sửa mã sau này, dùng **Deploy → Manage deployments → Edit → New version → Deploy** để giữ nguyên URL. Đổi Script Properties không cần tạo URL mới. Không dùng link `/dev` hoặc chia sẻ quyền chỉnh sửa dự án cho người dùng app.

## Chú ý khi chạy thử

- Có thể xuất hiện cảnh báo Google chưa xác minh; hồ sơ hiện đang chờ duyệt. Kiến trúc mới có lưu refresh token phía script, cần phản ánh đúng trong hồ sơ xác minh và chính sách riêng tư trước khi phát hành công khai.
- Thử bằng chính tài khoản Sếpp trước. Emm sẽ kiểm tra callback, chọn tài khoản, tải token mới sau khi hết hạn, lưu/mở lại và ngắt kết nối sau khi có URL.
- Apps Script có quota chung; bản này phù hợp thử nghiệm/triển khai nhỏ, chưa phải dịch vụ cho số lượng người dùng không giới hạn. Ảnh được tải trực tiếp từ Google, không truyền ảnh 4K qua script.
