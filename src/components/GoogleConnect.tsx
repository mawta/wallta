import { useEffect, useRef, useState } from 'react';
import QRCode from 'qrcode';
import { ArrowUpRight, Check, LoaderCircle } from 'lucide-react';
import { callNative, errorMessage, isAndroid, isNative } from '../lib/native';
import { googleConnectionChanges, googlePollDelay, googleVerificationUrl, type GoogleStatus } from '../lib/google';
import type { Source } from '../lib/types';

const startingRequests = new Map<string, symbol>();

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
  const connected = (next: GoogleStatus) => onConnected(googleConnectionChanges(source, next));
  const connectedRef = useRef(connected);
  connectedRef.current = connected;
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
        if (next.phase === 'ready') connectedRef.current(next);
        if (next.phase === 'error') setError(next.message || 'Kết nối Google chưa hoàn tất.');
      } catch (err) { if (active.current) { setStatus(null); setError(errorMessage(err)); } }
    }, googlePollDelay(status.pollIntervalSeconds));
    return () => clearTimeout(timer);
  }, [status, source.id]);
  const begin = async () => {
    if (relay && !relayUrl.trim()) { setError('Nhập địa chỉ HTTPS của Wallta Drive Relay.'); return; }
    if (!relay && !androidPhotos && !clientId.trim()) { setError('Nhập OAuth Client ID.'); return; }
    const request = Symbol();
    startingRequests.set(source.id, request);
    const cancelLateStart = async () => {
      if (startingRequests.get(source.id) === request) await callNative('google_cancel', { sourceId: source.id, discardImports: true }).catch(() => {});
    };
    setBusy(true); setError('');
    try {
      const next = await callNative<GoogleStatus>('google_begin', { sourceId: source.id, kind: source.kind, clientId: clientId.trim(), clientSecret: clientSecret.trim(), relayUrl: relayUrl.trim(), location: source.location, displayName: source.name || 'Wallta' });
      if (!active.current) { await cancelLateStart(); return; }
      setStatus(next);
      if (next.phase === 'ready') connected(next);
    }
    catch (err) { if (active.current) setError(errorMessage(err)); else await cancelLateStart(); }
    finally { if (startingRequests.get(source.id) === request) startingRequests.delete(source.id); if (active.current) setBusy(false); }
  };
  if (!isNative()) return <div className="inline-note">Đăng nhập Google chỉ có trong bản cài.</div>;
  return <div className="google-connect">
    <strong>{source.kind === 'photos' ? 'Kết nối Google Photos' : 'Đăng nhập Google Drive'}</strong>
    <p className="muted small">{relay ? 'Nhập relay HTTPS, quét QR trên điện thoại để cấp quyền thư mục.' : androidPhotos ? 'Điện thoại: chọn ảnh hệ thống/cloud nếu có. TV: OAuth loại TVs and Limited Input Devices, có quyền Ambient API.' : 'OAuth loại Desktop app. Cấp quyền trong trình duyệt.'}</p>
    {relay ? <label className="field">Wallta Drive Relay<input autoComplete="off" placeholder="https://wallta.example.com" value={relayUrl} onChange={event => setRelayUrl(event.target.value)} /></label> : <><label className="field">OAuth Client ID{androidPhotos ? ' · chỉ cần trên TV' : ''}<input autoComplete="off" placeholder="…apps.googleusercontent.com" value={clientId} onChange={event => setClientId(event.target.value)} /></label><label className="field">Client secret · nếu được Google cấp<input type="password" autoComplete="new-password" value={clientSecret} onChange={event => setClientSecret(event.target.value)} /></label></>}
    <button type="button" className="button secondary" disabled={busy || status?.phase === 'authorizing' || status?.phase === 'selecting'} onClick={() => void begin()}>{busy ? <LoaderCircle size={16} className="spin" /> : status?.phase === 'ready' ? <Check size={16} /> : <ArrowUpRight size={16} />}{status?.phase === 'ready' ? 'Đã kết nối · Kết nối lại' : 'Kết nối tài khoản Google'}</button>
    {status && status.phase !== 'error' && <div className="google-status" role="status">{qr && <img src={qr} alt="Mã QR cấp quyền Google" width={196} height={196} />}<div>{status.userCode && <strong className="google-user-code">{status.userCode}</strong>}<p>{status.message || (status.phase === 'ready' ? 'Đã cấp quyền. Lưu nguồn ảnh để sử dụng.' : 'Cấp quyền trong trình duyệt hoặc trên điện thoại…')}</p>{url && <button type="button" className="text-button" onClick={() => void callNative('open_external', { url }).catch(err => setError(errorMessage(err)))}>Mở trang cấp quyền <ArrowUpRight size={14} /></button>}</div></div>}
    {error && <p className="form-error" role="alert">{error}</p>}
  </div>;
}
