import { t } from '../lib/i18n';
import { useEffect, useRef, useState } from 'react';
import QRCode from 'qrcode';
import { ArrowUpRight, Check, LoaderCircle, RotateCw } from 'lucide-react';
import { callNative, errorMessage, isAndroid, isNative } from '../lib/native';
import { googleConnectionChanges, googlePollDelay, googleVerificationUrl, type GoogleStatus } from '../lib/google';
import type { Source } from '../lib/types';

const startingRequests = new Map<string, symbol>();
const setupGuide = 'https://github.com/mawta/wallta/blob/main/docs/setup-google.md';

export function GoogleConnect({ source, onConnected }: { source: Source; onConnected: (changes: Partial<Source>) => void }) {
  const [clientId, setClientId] = useState('');
  const [clientSecret, setClientSecret] = useState('');
  const android = isNative() && isAndroid();
  const [defaultClient, setDefaultClient] = useState(android);
  const [androidMode, setAndroidMode] = useState<'native' | 'broker'>('native');
  const [status, setStatus] = useState<GoogleStatus | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [qr, setQr] = useState('');
  const active = useRef(true);
  const attempt = useRef(0);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  const desktop = isNative() && !isAndroid();
  useEffect(() => {
    if (!android) return;
    let cancelled = false;
    void callNative<{ mode: 'native' | 'broker'; configured: boolean }>('google_connection_options').then(options => {
      if (!cancelled && (options?.mode === 'native' || options?.mode === 'broker')) setAndroidMode(options.mode);
    }).catch(err => { if (!cancelled) setError(errorMessage(err)); });
    return () => { cancelled = true; };
  }, [android]);
  useEffect(() => {
    if (!desktop) return;
    let cancelled = false;
    void callNative<boolean>('google_default_client_available').then(available => {
      if (!cancelled) setDefaultClient(available === true);
    }).catch(() => { /* Keep the manual connection form. */ });
    return () => { cancelled = true; };
  }, [desktop]);
  const connected = (next: GoogleStatus) => onConnected(googleConnectionChanges(source, next));
  const connectedRef = useRef(connected);
  connectedRef.current = connected;
  const url = googleVerificationUrl(status?.verificationUrl);
  const pending = status?.phase === 'authorizing' || status?.phase === 'selecting';
  useEffect(() => {
    let cancelled = false;
    setQr('');
    if (url) void QRCode.toDataURL(url, { width: 196, margin: 2, color: { dark: '#111714', light: '#ffffff' } }).then(value => { if (!cancelled) setQr(value); });
    return () => { cancelled = true; };
  }, [url]);
  useEffect(() => {
    if (!status || !['authorizing', 'selecting'].includes(status.phase)) return;
    const currentAttempt = attempt.current;
    const timer = setTimeout(async () => {
      try {
        const next = await callNative<GoogleStatus>('google_poll', { sourceId: source.id });
        if (!active.current || currentAttempt !== attempt.current) return;
        setStatus(next);
        if (next.phase === 'ready') connectedRef.current(next);
        if (next.phase === 'error') setError(next.message || t("Kết nối Google chưa hoàn tất."));
      } catch (err) { if (active.current && currentAttempt === attempt.current) { setStatus(null); setError(errorMessage(err)); } }
    }, googlePollDelay(status.pollIntervalSeconds));
    return () => clearTimeout(timer);
  }, [status, source.id]);
  const begin = async (mode?: 'broker') => {
    if (mode) setAndroidMode(mode);
    if (!android && !defaultClient && !clientId.trim()) { setError(t("Nhập OAuth Client ID.")); return; }
    const request = Symbol();
    startingRequests.set(source.id, request);
    const cancelLateStart = async () => {
      if (startingRequests.get(source.id) === request) await callNative('google_cancel', { sourceId: source.id, discardImports: true }).catch(() => {});
    };
    attempt.current += 1;
    setBusy(true); setError(''); setStatus(null);
    try {
      if (pending) await callNative('google_cancel', { sourceId: source.id, discardImports: true });
      const next = await callNative<GoogleStatus>('google_begin', { sourceId: source.id, kind: source.kind, clientId: defaultClient ? '' : clientId.trim(), clientSecret: defaultClient ? '' : clientSecret.trim(), relayUrl: '', ...(android && (mode || androidMode === 'broker') ? { mode: 'broker' } : {}), location: source.location, displayName: source.name || 'Wallta' });
      if (!active.current) { await cancelLateStart(); return; }
      setStatus(next);
      if (next.phase === 'ready') connected(next);
    }
    catch (err) { if (active.current) setError(errorMessage(err)); else await cancelLateStart(); }
    finally { if (startingRequests.get(source.id) === request) startingRequests.delete(source.id); if (active.current) setBusy(false); }
  };
  if (!isNative()) return <div className="inline-note">{t("Đăng nhập Google chỉ có trong bản cài.")}</div>;
  return <div className="google-connect">
    {!defaultClient && <><label className="field">OAuth Client ID<input autoComplete="off" placeholder="…apps.googleusercontent.com" value={clientId} onChange={event => setClientId(event.target.value)} /></label><label className="field">{t("Client secret · nếu được Google cấp")}<input type="password" autoComplete="new-password" value={clientSecret} onChange={event => setClientSecret(event.target.value)} /></label></>}
    <button type="button" className="google-sign-in" disabled={busy || pending} onClick={() => void begin()} aria-busy={busy || pending}>
      <span className="google-sign-in-logo" aria-hidden="true"><svg width="20" height="20" viewBox="0 0 48 48"><path fill="#4285f4" d="M43.6 24.5c0-1.5-.1-3-.4-4.5H24v8.5h11a9.4 9.4 0 0 1-4.1 6.2v5.2h6.7c3.9-3.6 6-8.9 6-15.4Z"/><path fill="#34a853" d="M24 44c5.5 0 10.1-1.8 13.5-5l-6.7-5.2c-1.8 1.2-4.1 1.9-6.8 1.9-5.3 0-9.8-3.6-11.4-8.4H5.7v5.4A20.4 20.4 0 0 0 24 44Z"/><path fill="#fbbc05" d="M12.6 27.3a12.2 12.2 0 0 1 0-7.8v-5.4H5.7a20.4 20.4 0 0 0 0 18.6l6.9-5.4Z"/><path fill="#ea4335" d="M24 11.7c3 0 5.6 1 7.7 3l5.8-5.8C34.1 5.6 29.5 4 24 4A20.4 20.4 0 0 0 5.7 14.1l6.9 5.4c1.6-4.8 6.1-7.8 11.4-7.8Z"/></svg></span>
      <span>{busy ? t("Đang kết nối…") : pending ? status?.phase === 'selecting' ? t("Đang chọn ảnh…") : t("Đang chờ cấp quyền…") : status?.phase === 'ready' ? t("Đã kết nối · Kết nối lại") : t("Kết nối tài khoản Google")}</span>
      {busy || pending ? <LoaderCircle size={17} className="spin" aria-hidden="true" /> : status?.phase === 'ready' ? <Check size={17} aria-hidden="true" /> : <ArrowUpRight size={17} aria-hidden="true" />}
    </button>
    {pending && <button type="button" className="text-button google-retry" disabled={busy} onClick={() => void begin()}><RotateCw size={14} />{t("Thử lại")}</button>}
    {!defaultClient && <button type="button" className="text-button" onClick={() => void callNative('open_external', { url: setupGuide }).catch(err => setError(errorMessage(err)))}>{t("Hướng dẫn cấu hình")} <ArrowUpRight size={13} /></button>}
    {status && status.phase !== 'error' && <div className="google-status" role="status">{androidMode === 'broker' && qr && <img src={qr} alt={t("Mã QR cấp quyền Google")} width={196} height={196} />}<div>{status.account && <strong className="google-status-account">{status.account}</strong>}{status.userCode && <strong className="google-user-code">{status.userCode}</strong>}<p>{t(status.message || '') || (status.phase === 'ready' ? t("Đã cấp quyền. Lưu nguồn ảnh để sử dụng.") : t("Cấp quyền trong trình duyệt hoặc trên điện thoại…"))}</p>{url && <button type="button" className="text-button" onClick={() => void callNative('open_external', { url }).catch(err => setError(errorMessage(err)))}>{status.phase === 'selecting' ? t("Mở Google Photos") : t("Mở trang cấp quyền")} <ArrowUpRight size={14} /></button>}</div></div>}
    {error && <p className="form-error" role="alert">{t(error)}</p>}
    {error && android && androidMode === 'native' && <button type="button" className="button secondary" disabled={busy} onClick={() => void begin('broker')}>{t("Kết nối bằng QR")}</button>}
  </div>;
}
