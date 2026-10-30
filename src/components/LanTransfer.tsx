import { t, localeTag } from '../lib/i18n';
import { useEffect, useRef, useState } from 'react';
import QRCode from 'qrcode';
import { LoaderCircle, Wifi, X } from 'lucide-react';
import { callNative, errorMessage, isNative } from '../lib/native';
import type { MediaItem, Source } from '../lib/types';

export interface LanStatus {
  active: boolean; urls: string[]; expiresAt: number; receivedCount: number; receivedBytes: number;
  outgoing: { id: string; name: string; size: number }[]; error?: string;
}
export interface LanTransferProps { sources: Source[]; media: MediaItem[]; onClose: () => void; onReceived: () => Promise<unknown> }
export function LanTransfer({ sources, media, onClose, onReceived }: LanTransferProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const alive = useRef(true);
  const started = useRef(false);
  const locked = useRef(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [status, setStatus] = useState<LanStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [address, setAddress] = useState(0);
  const [qr, setQr] = useState('');
  const localSources = new Set(sources.filter(source => source.kind === 'local').map(source => source.id));
  const choices = media.filter(item => localSources.has(item.sourceId));
  const url = status?.urls[address] || status?.urls[0] || '';
  useEffect(() => {
    alive.current = true; const element = dialog.current; element?.showModal();
    return () => { alive.current = false; element?.close(); if (started.current) void callNative('lan_stop').catch(() => {}); };
  }, []);
  useEffect(() => {
    let cancelled = false; setQr('');
    if (url) void QRCode.toDataURL(url, { width: 220, margin: 2 }).then(value => { if (!cancelled) setQr(value); }).catch(() => { if (!cancelled) setError(t("Không tạo được QR; dùng link bên dưới.")); });
    return () => { cancelled = true; };
  }, [url]);
  useEffect(() => {
    if (!status?.active || busy) return;
    let cancelled = false;
    const timer = setTimeout(async () => {
      try { const next = await callNative<LanStatus>('lan_status'); if (!cancelled && alive.current) setStatus(next); }
      catch (err) { if (!cancelled && alive.current) { setError(errorMessage(err)); setStatus(previous => previous ? { ...previous, active: false } : null); } }
    }, 1000);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [status, busy]);
  const run = async (operation: () => Promise<void>) => {
    if (locked.current) return; locked.current = true; setBusy(true); setError('');
    try { await operation(); } catch (err) { if (alive.current) setError(errorMessage(err)); }
    finally { locked.current = false; if (alive.current) setBusy(false); }
  };
  const close = () => { if (!locked.current) void run(async () => { if (started.current) await callNative('lan_stop'); started.current = false; onClose(); }); };
  const begin = () => void run(async () => {
    started.current = true;
    try {
      const next = await callNative<LanStatus>('lan_start', { mediaIds: selected });
      if (!alive.current) { await callNative('lan_stop'); started.current = false; return; }
      setAddress(0); setStatus(next);
    } catch (err) {
      // A bridge timeout can precede native completion; stop queues behind that start.
      try { await callNative('lan_stop'); started.current = false; } catch { /* Retry cleanup on close/unmount. */ }
      throw err;
    }
  });
  return <dialog ref={dialog} className="source-dialog lan-dialog" aria-labelledby="lan-title" onCancel={event => { event.preventDefault(); close(); }}>
    <div className="dialog-heading"><h2 id="lan-title">{t("Gửi / nhận qua Wi-Fi")}</h2><button className="icon-button" aria-label={t("Đóng")} disabled={busy} onClick={close}><X size={20} /></button></div>
    <p className="muted small">{t("Cùng mạng Wi-Fi/LAN. Dùng mạng tin cậy; truyền HTTP không mã hóa.")}</p>
    {!status?.active ? <>
      {status && <p className="inline-note">{t("Phiên đã dừng hoặc hết hạn.")}</p>}
      <h3 className="lan-label">{t("Ảnh cho máy kia tải")}</h3>
      <p className="muted small">{t("Chọn tối đa 50 ảnh local. Bỏ trống nếu chỉ nhận.")}</p>
      {choices.length > 0 && <div className="lan-selection">{choices.map(item => <label className="checkbox-row" key={item.id}><input type="checkbox" checked={selected.includes(item.id)} disabled={busy || (!selected.includes(item.id) && selected.length >= 50)} onChange={event => setSelected(previous => event.target.checked ? [...previous, item.id] : previous.filter(id => id !== item.id))} /><span>{item.name}</span></label>)}</div>}
      {!isNative() && <p className="inline-note">{t("Gửi / nhận Wi-Fi chỉ có trong bản cài.")}</p>}
      <div className="dialog-footer"><button className="button secondary" onClick={close} disabled={busy}>{t("Đóng")}</button><button className="button primary" onClick={begin} disabled={busy || !isNative()}>{busy ? <LoaderCircle className="spin" size={16} /> : <Wifi size={16} />}{t("Bật gửi / nhận")}</button></div>
    </> : <>
      <div className="lan-connection">{qr && <img src={qr} alt={t("QR gửi nhận ảnh qua Wi-Fi")} width={220} height={220} />}<div><p>{t("Quét QR hoặc mở link trên máy kia.")}</p><p className="muted small">{t("Giữ Wallta mở. Hết hạn lúc")} {new Date(status.expiresAt).toLocaleTimeString(localeTag(), { hour: '2-digit', minute: '2-digit' })}.</p></div></div>
      {status.urls.length > 1 && <label className="field">{t("Địa chỉ mạng")}<select value={address} onChange={event => setAddress(Number(event.target.value))}>{status.urls.map((value, index) => <option key={value} value={index}>{new URL(value).host}</option>)}</select></label>}
      <label className="field">{t("Link kết nối")}<input readOnly value={url} onFocus={event => event.target.select()} /></label>
      <p role="status" className="lan-count">{t("Đã nhận:")} {status.receivedCount} {t("ảnh ·")} {(status.receivedBytes / 1024 / 1024).toFixed(1)} MB</p>
      <p className="muted small">{t("Chỉ thêm vào thư viện sau khi xác nhận. Dừng sẽ bỏ ảnh chưa nhận.")}</p>
      <details className="connection-help"><summary>{t("Không kết nối được?")}</summary><p>{t("Cả hai máy phải cùng mạng; tắt mạng khách hoặc AP isolation. Trên Windows, cho phép Wallta qua firewall ở mạng riêng. Giới hạn 50 ảnh / 256 MB, mỗi ảnh tối đa 25 MB.")}</p></details>
      <div className="dialog-footer"><button className="button secondary" disabled={busy} onClick={close}>{t("Dừng & đóng")}</button><button className="button primary" disabled={busy || status.receivedCount === 0} onClick={() => void run(async () => { await callNative('lan_accept'); started.current = false; await onReceived(); onClose(); })}>{busy && <LoaderCircle className="spin" size={16} />}{t("Thêm ảnh đã nhận")}</button></div>
    </>}
    {(error || status?.error) && <p className="form-error" role="alert">{t(error || status?.error || '')}</p>}
  </dialog>;
}
