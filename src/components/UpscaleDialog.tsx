import { useEffect, useRef, useState } from 'react';
import { LoaderCircle, X } from 'lucide-react';
import { callNative, errorMessage, imageUrl, isNative } from '../lib/native';
import type { Capabilities, MediaItem, Source } from '../lib/types';

type Mode = 'fast' | 'ai';
interface Result { id: string; name: string; width: number; height: number; originalPreview: string; resultPreview: string }
interface Status { jobId: string | null; phase: 'idle' | 'downloading' | 'processing' | 'ready' | 'cancelled' | 'error'; mode: Mode; completed: number; total: number; message?: string; error?: string; results: Result[] }
interface Props { platform: Capabilities['platform']; media: MediaItem[]; initialIds?: string[]; onSaved: (source: Source, apply: boolean) => Promise<boolean>; onClose: () => void }
const idle: Status = { jobId: null, phase: 'idle', mode: 'fast', completed: 0, total: 0, results: [] };
export function UpscaleDialog({ platform, media, initialIds = [], onSaved, onClose }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const alive = useRef(true); const locked = useRef(false); const currentJob = useRef<string | null>(null); const saved = useRef(false);
  const [selected, setSelected] = useState(initialIds.filter(id => media.some(item => item.id === id)).slice(0, 50));
  const [mode, setMode] = useState<Mode>('fast');
  const [status, setStatus] = useState<Status>(idle);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState('');
  const [apply, setApply] = useState(false);
  const [resultIndex, setResultIndex] = useState(0);
  const active = ['downloading', 'processing'].includes(status.phase);
  const result = status.results[resultIndex] || status.results[0];
  const hasResults = (status.phase === 'ready' || status.phase === 'error') && status.results.length > 0;
  const receive = (next: Status) => { currentJob.current = next.jobId; if (alive.current) setStatus(next); };
  useEffect(() => {
    alive.current = true; const element = dialog.current; element?.showModal();
    if (isNative()) void callNative<Status>('upscale_status').then(async next => {
      if (!alive.current) { if (next.jobId) await callNative('upscale_cancel', { jobId: next.jobId }); return; }
      receive(next);
    }).catch(err => { if (alive.current) setError(errorMessage(err)); }).finally(() => { if (alive.current) setBusy(false); });
    else setBusy(false);
    return () => { alive.current = false; element?.close(); if (currentJob.current && !saved.current) void callNative('upscale_cancel', { jobId: currentJob.current }).catch(() => {}); };
  }, []);
  useEffect(() => {
    if (!active || busy) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try { const next = await callNative<Status>('upscale_status'); if (!disposed && alive.current) { receive(next); setError(''); } }
      catch (err) { if (!disposed && alive.current) setError(errorMessage(err)); }
      finally { if (!disposed && alive.current) timer = setTimeout(poll, 750); }
    };
    timer = setTimeout(poll, 750);
    return () => { disposed = true; clearTimeout(timer); };
  }, [status.jobId, active, busy]);
  const run = async (operation: () => Promise<void>) => {
    if (locked.current) return;
    locked.current = true; setBusy(true); setError('');
    try { await operation(); } catch (err) { if (alive.current) setError(errorMessage(err)); }
    finally { locked.current = false; if (alive.current) setBusy(false); }
  };
  const cancel = async () => { if (currentJob.current && !saved.current) receive(await callNative<Status>('upscale_cancel', { jobId: currentJob.current })); };
  const close = () => void run(async () => { await cancel(); currentJob.current = null; onClose(); });
  const start = () => void run(async () => {
    saved.current = false; setResultIndex(0);
    const next = await callNative<Status>('upscale_start', { ids: selected, mode });
    receive(next);
    if (!alive.current && next.jobId) await callNative('upscale_cancel', { jobId: next.jobId });
  });
  const save = () => void run(async () => {
    const source = await callNative<Source>('upscale_save', { jobId: status.jobId });
    saved.current = true;
    if (await onSaved(source, apply)) { currentJob.current = null; onClose(); }
    else setError('Ảnh đã lưu. Chưa áp dụng được; có thể thử lại hoặc đóng.');
  });
  const reset = () => void run(async () => { await cancel(); saved.current = false; currentJob.current = null; setStatus(idle); setResultIndex(0); });
  return <dialog ref={dialog} className="source-dialog upscale-dialog" aria-labelledby="upscale-title" onCancel={event => { event.preventDefault(); if (!busy) close(); }}>
    <div className="dialog-heading"><h2 id="upscale-title">Nâng ảnh lên 4K</h2><button className="icon-button" aria-label="Đóng" disabled={busy} onClick={close}><X size={20} /></button></div>
    {!active && !hasResults && <>
      <label className="field">Chế độ<select value={mode} disabled={busy} onChange={event => setMode(event.target.value as Mode)}><option value="fast">Nhanh</option><option value="ai" disabled={platform !== 'windows'}>AI</option></select></label>
      <p className="muted small">{mode === 'ai' ? 'Xử lý trên máy. Lần đầu tải bộ AI; cần GPU hỗ trợ Vulkan. AI có thể thay đổi chi tiết.' : 'Phóng ảnh giữ tỷ lệ, tối đa khung 4K. Giữ nguyên ảnh gốc.'}{platform !== 'windows' && ' AI chỉ có trên Windows.'}</p>
      <div className="subsection-heading"><h3>Chọn ảnh</h3><span>{selected.length}/50</span></div>
      <div className="lan-selection">{media.map(item => <label className="checkbox-row" key={item.id}><input type="checkbox" checked={selected.includes(item.id)} disabled={busy || (!selected.includes(item.id) && selected.length >= 50)} onChange={event => setSelected(previous => event.target.checked ? [...previous, item.id] : previous.filter(id => id !== item.id))} /><span>{item.name}</span></label>)}</div>
      {!isNative() && <p className="inline-note">Nâng ảnh 4K cần bản cài Wallta.</p>}
      <div className="dialog-footer"><button className="button secondary" disabled={busy} onClick={close}>Đóng</button><button className="button primary" disabled={busy || !isNative() || !selected.length} onClick={start}>Tạo ảnh 4K</button></div>
    </>}
    {active && <div className="upscale-progress" role="status"><LoaderCircle className="spin" size={26} /><strong>{status.phase === 'downloading' ? 'Chuẩn bị bộ AI…' : `Đang xử lý ${Math.min(status.completed + 1, status.total)}/${status.total}`}</strong><progress max={Math.max(1, status.total)} value={status.completed} /><button className="button secondary" disabled={busy} onClick={() => void run(cancel)}>Hủy xử lý</button></div>}
    {hasResults && result && <>
      {status.results.length > 1 && <label className="field">Ảnh kết quả<select value={resultIndex} onChange={event => setResultIndex(Number(event.target.value))}>{status.results.map((item, index) => <option value={index} key={item.id}>{item.name}</option>)}</select></label>}
      <div className="upscale-comparison"><figure><img src={imageUrl(result.originalPreview)} alt="Trước" /><figcaption>Trước</figcaption></figure><figure><img src={imageUrl(result.resultPreview)} alt="Sau" /><figcaption>Sau · {result.width} × {result.height}</figcaption></figure></div>
      <p className="muted small">{status.completed}/{status.total} ảnh · Lưu riêng vào nguồn “Ảnh 4K”.</p>
      <button className="text-button" disabled={busy} onClick={reset}>Tạo bộ ảnh khác</button>
      <label className="checkbox-row"><input type="checkbox" checked={apply} disabled={busy} onChange={event => setApply(event.target.checked)} />Áp dụng sau khi lưu</label>
      <div className="dialog-footer"><button className="button secondary" disabled={busy} onClick={close}>Đóng</button><button className="button primary" disabled={busy} onClick={save}>{busy && <LoaderCircle className="spin" size={16} />}Lưu ảnh 4K</button></div>
    </>}
    {active && <div className="dialog-footer"><button className="text-button" disabled={busy} onClick={close}>Hủy & đóng</button></div>}
    {(error || status.error) && <p className="form-error" role="alert">{error || status.error}</p>}
    {status.phase === 'cancelled' && <p role="status" className="muted small">Đã hủy xử lý.</p>}
  </dialog>;
}
