import { useEffect, useRef, useState } from 'react';
import QRCode from 'qrcode';
import { ArrowUpRight, Check, LoaderCircle } from 'lucide-react';
import { callNative, errorMessage, isAndroid, isNative } from '../lib/native';
import { googlePollDelay, googleVerificationUrl, type GoogleStatus } from '../lib/google';
import type { Source } from '../lib/types';

export function GoogleConnect({ source, onConnected }: { source: Source; onConnected: (changes: Partial<Source>) => void }) {
  const [clientId, setClientId] = useState('');
  const [clientSecret, setClientSecret] = useState('');
  const [relayUrl, setRelayUrl] = useState('');
  const [status, setStatus] = useState<GoogleStatus | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [qr, setQr] = useState('');
  const active = useRef(true);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  const relay = source.kind === 'drive' && isAndroid();
  const androidPhotos = source.kind === 'photos' && isAndroid();
  const url = googleVerificationUrl(status?.verificationUrl, relay ? relayUrl : undefined);
  useEffect(() => {
    let cancelled = false;
    setQr('');
    if (url) void QRCode.toDataURL(url, { width: 196, margin: 2, color: { dark: '#111714', light: '#ffffff' } }).then(value => { if (!cancelled) setQr(value); });
    return () => { cancelled = true; };
  }, [url]);
  useEffect(() => {
    if (!status || !['authorizing', 'selecting'].includes(status.phase)) return;
    const timer = setTimeout(async () => {
      try {
        const next = await callNative<GoogleStatus>('google_poll', { sourceId: source.id });
        if (!active.current) return;
        setStatus(next);
        if (next.phase === 'ready') onConnected({ location: next.location || source.location, kind: next.sourceKind || source.kind });
        if (next.phase === 'error') setError(next.message || 'Kết nối Google chưa hoàn tất.');
      } catch (err) { if (active.current) { setStatus(null); setError(errorMessage(err)); } }
    }, googlePollDelay(status.pollIntervalSeconds));
    return () => clearTimeout(timer);
  }, [status, source.id, source.kind, source.location, onConnected]);
  const begin = async () => {
    if (relay && !relayUrl.trim()) { setError('Nhập địa chỉ HTTPS của Wallta Drive Relay.'); return; }
    if (!relay && !androidPhotos && !clientId.trim()) { setError('Nhập OAuth Client ID từ dự án Google của Sếpp.'); return; }
    setBusy(true); setError('');
    try {
      const next = await callNative<GoogleStatus>('google_begin', { sourceId: source.id, kind: source.kind, clientId: clientId.trim(), clientSecret: clientSecret.trim(), relayUrl: relayUrl.trim(), location: source.location, displayName: source.name || 'Wallta' });
      if (!active.current) return;
      setStatus(next);
      if (next.phase === 'ready') onConnected({ location: next.location || source.location, kind: next.sourceKind || source.kind });
    }
    catch (err) { setError(errorMessage(err)); }
    finally { setBusy(false); }
  };
  if (!isNative()) return <div className="inline-note">Kết nối tài khoản Google trong bản cài Wallta. Bản xem trước hỗ trợ thư mục local, link ảnh và danh sách JSON.</div>;
  return <div className="google-connect">
    <strong>{source.kind === 'photos' ? 'Kết nối Google Photos' : 'Đăng nhập Google Drive'}</strong>
    <p className="muted small">{relay ? 'Quét QR trên điện thoại để cấp quyền đọc thư mục riêng. Relay do Sếpp quản lý giữ quyền Google; TV nhận khóa riêng cho thư mục này.' : androidPhotos ? 'Điện thoại mở trình chọn ảnh của hệ thống: chọn Google Photos nếu nhà cung cấp có sẵn. TV dùng OAuth client loại TVs and Limited Input devices đã được cấp Ambient API.' : 'Dùng OAuth client loại Desktop app. Wallta mở trình duyệt để Sếpp cấp quyền.'}</p>
    {relay ? <label className="field">Wallta Drive Relay<input autoComplete="off" placeholder="https://wallta.example.com" value={relayUrl} onChange={event => setRelayUrl(event.target.value)} /></label> : <><label className="field">OAuth Client ID{androidPhotos ? ' · chỉ cần trên TV' : ''}<input autoComplete="off" placeholder="…apps.googleusercontent.com" value={clientId} onChange={event => setClientId(event.target.value)} /></label><label className="field">Client secret · nếu được Google cấp<input type="password" autoComplete="new-password" value={clientSecret} onChange={event => setClientSecret(event.target.value)} /></label></>}
    <button type="button" className="button secondary" disabled={busy || status?.phase === 'authorizing' || status?.phase === 'selecting'} onClick={() => void begin()}>{busy ? <LoaderCircle size={16} className="spin" /> : status?.phase === 'ready' ? <Check size={16} /> : <ArrowUpRight size={16} />}{status?.phase === 'ready' ? 'Đã kết nối · Kết nối lại' : 'Kết nối tài khoản Google'}</button>
    {status && status.phase !== 'error' && <div className="google-status" role="status">{qr && <img src={qr} alt="Mã QR cấp quyền Google" width={196} height={196} />}<div>{status.userCode && <strong className="google-user-code">{status.userCode}</strong>}<p>{status.message || (status.phase === 'ready' ? 'Quyền truy cập đã sẵn sàng. Lưu nguồn ảnh để sử dụng.' : 'Hoàn tất cấp quyền trong trình duyệt hoặc trên điện thoại…')}</p>{url && <button type="button" className="text-button" onClick={() => void callNative('open_external', { url }).catch(err => setError(errorMessage(err)))}>Mở trang cấp quyền <ArrowUpRight size={14} /></button>}</div></div>}
    {error && <p className="form-error" role="alert">{error}</p>}
  </div>;
}
