<!doctype html>
<html lang="vi"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Kết nối Wallta</title>
<style>body{font-family:system-ui;background:#101714;color:#eaf2ed;max-width:540px;margin:10vh auto;padding:24px;line-height:1.7}h1{color:#c5f47b}code{overflow-wrap:anywhere}button{background:#c5f47b;border:0;border-radius:12px;padding:16px 24px;font-size:16px;font-weight:700;cursor:pointer}small{color:#a4b8ab}</style>
<h1>Wallta · Google Drive</h1><p>Cấp quyền cho <strong>{{ $pair['displayName'] }}</strong> đọc ảnh trong thư mục:</p><p><code>{{ $pair['folderId'] }}</code></p>
<p>Chỉ tiếp tục nếu Sếpp vừa tạo kết nối này trên thiết bị của mình. Google sẽ mở trang đăng nhập; Wallta không nhận mật khẩu Google.</p>
<form method="POST" action="{{ url('/connect/'.$code) }}">@csrf<button>Kết nối Google Drive</button></form>
<p><small>Relay lưu quyền Google đã mã hóa và cấp cho TV một khóa chỉ dùng với thư mục này. Sếpp có thể ngắt kết nối trong Wallta.</small></p></html>
