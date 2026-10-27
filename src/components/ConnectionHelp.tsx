import { useState } from 'react';
import { callNative, errorMessage, isAndroid, isNative } from '../lib/native';

function HelpLink({ href, children }: { href: string; children: React.ReactNode }) {
  const [error, setError] = useState('');
  return <><a href={href} target="_blank" rel="noopener noreferrer" onClick={event => {
    if (isNative()) { event.preventDefault(); void callNative('open_external', { url: href }).catch(err => setError(errorMessage(err))); }
  }}>{children} ↗</a>{error && <span role="alert" className="form-error">{error}</span>}</>;
}
const consoleUrl = 'https://console.cloud.google.com/';
const clientsUrl = 'https://console.cloud.google.com/auth/clients';
export function ConnectionHelp({ kind }: { kind: 'drive' | 'photos' }) {
  const android = isAndroid();
  return <details className="connection-help"><summary>Cách kết nối · lấy API key / Client ID</summary>
    {kind === 'drive' ? <>
      <h4>Thư mục Drive công khai · API key</h4>
      <ol><li>Mở <HelpLink href={consoleUrl}>Google Cloud Console</HelpLink>, tạo hoặc chọn project.</li><li>APIs & Services → Library → <HelpLink href="https://console.cloud.google.com/apis/library/drive.googleapis.com">Google Drive API</HelpLink> → Enable.</li><li>APIs & Services → Credentials → Create credentials → API key. Trong API restrictions, chỉ cho Google Drive API.</li><li>Drive → chia sẻ thư mục: Anyone with the link, Viewer. Ảnh phải được phép xem qua link; chỉ dùng cách này nếu muốn công khai ảnh.</li><li>Dán link thư mục vào ô phía trên. Mở “Thông tin kết nối”, dán key vào “Google API key”.</li></ol>
      <h4>Drive riêng · {android ? 'Android / TV' : 'Windows'}</h4>
      {android ? <ol><li>Drive riêng cần Wallta Drive Relay chạy trên máy chủ HTTPS; app chưa có relay dùng chung.</li><li>Tạo OAuth client loại Web application. Redirect URI: <code>https://TEN-RELAY/oauth/callback</code>.</li><li>Người quản trị relay đặt GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET và APP_URL trong cấu hình máy chủ. Không dán secret của relay vào TV.</li><li>Nhập URL relay và link thư mục trong Wallta → kết nối → quét QR, đăng nhập Google, rồi lưu nguồn.</li></ol> : <ol><li>Bật Drive API như trên. Google Auth platform → Branding → Get Started, điền tên app và email.</li><li>Audience → External (tài khoản cá nhân) → Test users: thêm email đăng nhập. Data Access: thêm scope <code>https://www.googleapis.com/auth/drive.readonly</code>.</li><li><HelpLink href={clientsUrl}>Clients</HelpLink> → Create client → Desktop app. Sao chép Client ID và client secret nếu được cấp.</li><li>Dán vào các ô OAuth trong Wallta → Kết nối tài khoản Google → cấp quyền → lưu nguồn. Desktop app dùng callback local tự động, không nhập redirect URI của relay.</li></ol>}
    </> : <>
      {android ? <><h4>Điện thoại Android · không cần key</h4><p>Bấm kết nối để chọn ảnh trong trình chọn ảnh hệ thống. Google Photos xuất hiện khi máy có nhà cung cấp cloud tương thích. Ảnh được chép vào máy; không tự đồng bộ album.</p></> : <><h4>Windows · Google Photos Picker</h4><ol><li>Tạo project trên <HelpLink href={consoleUrl}>Google Cloud Console</HelpLink>, bật <HelpLink href="https://console.cloud.google.com/apis/library/photospicker.googleapis.com">Google Photos Picker API</HelpLink>.</li><li>Google Auth platform → Branding: khai báo app; Audience: thêm tài khoản thử nghiệm; Data Access: thêm <code>https://www.googleapis.com/auth/photospicker.mediaitems.readonly</code>.</li><li><HelpLink href={clientsUrl}>Clients</HelpLink> → Create client → Desktop app. Dán Client ID / client secret vào Wallta.</li><li>Kết nối, chọn ảnh trên Google rồi lưu nguồn. Chỉ nhập bản sao ảnh đã chọn, không tự đồng bộ album.</li></ol></>}
      <h4>Google Photos trên TV · album tự đổi</h4>
      <p>Cần được duyệt vào <HelpLink href="https://developers.google.com/photos/partner-program/overview">Google Photos Partner Program</HelpLink> để dùng Ambient API. API key thông thường không đủ.</p>
      <ol><li>Sau khi được duyệt, dùng project và quyền Ambient theo hướng dẫn Google.</li><li>Tạo OAuth client loại TVs and Limited Input devices. Nhập Client ID / secret trong Wallta.</li><li>Kết nối → quét QR → cấp quyền → chọn album ở trang thiết lập Google → chờ sẵn sàng → lưu nguồn. Device ID do kết nối tạo, không phải ID album.</li></ol>
      <p>Chưa có quyền Ambient: dùng local/USB hoặc gửi ảnh qua Wi-Fi.</p>
    </>}
    <h4>Lỗi thường gặp</h4><ul><li><code>access_denied</code>: kiểm tra Test users và đúng tài khoản.</li><li><code>redirect_uri_mismatch</code>: relay phải đăng ký đúng URL HTTPS và đường dẫn callback.</li><li><code>API not enabled / 403</code>: bật API trong đúng project; kiểm tra quyền thư mục và giới hạn key.</li><li>Access token nhập tay có thể hết hạn; ưu tiên kết nối OAuth.</li></ul>
    <p><HelpLink href="https://developers.google.com/workspace/guides/create-credentials">Tài liệu tạo credential</HelpLink> · <HelpLink href="https://developers.google.com/workspace/guides/configure-oauth-consent">Cấu hình consent</HelpLink></p>
  </details>;
}
