import { uuid } from '../lib/compat';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { X, FolderOpen, ArrowUpRight, Check, ChevronDown } from 'lucide-react';
import { isNative, isAndroid, callNative, registerPreviewFiles, errorMessage } from '../lib/native';
import { validateSource } from '../lib/rules';
import { sourceLabels, type Source, type SourceKind } from '../lib/types';
import { SourceIcon } from './ui';
import { GoogleConnect } from './GoogleConnect';
import { ConnectionHelp } from './ConnectionHelp';

interface Props { initial?: Source; initialKind?: SourceKind; onSave: (source: Source) => Promise<boolean>; onClose: () => void; busy: boolean }
export function SourceDialog({ initial, initialKind = 'local', onSave, onClose, busy }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const picker = useRef<HTMLInputElement>(null);
  const [source, setSource] = useState<Source>(() => initial || { id: uuid(), name: '', kind: initialKind, location: '', recursive: true, enabled: true });
  const [error, setError] = useState('');
  const [picking, setPicking] = useState(false);
  const submitted = useRef(false);
  useEffect(() => () => { if (isNative()) void callNative('google_cancel', { sourceId: source.id, discardImports: !submitted.current }).catch(() => {}); }, [source.id]);
  useEffect(() => { const element = dialog.current; element?.showModal(); return () => element?.close(); }, []);
  const update = (value: Partial<Source>) => setSource(previous => ({ ...previous, ...value }));
  const chooseFolder = async () => {
    if (!isNative()) { picker.current?.click(); return; }
    setPicking(true); setError('');
    try {
      const location = await callNative<string | null>('pick_folder');
      if (location) update({ location, name: source.name || (location.startsWith('content:') ? 'Ảnh trên thiết bị' : location.split(/[\\/]/).filter(Boolean).pop()) || 'Thư mục ảnh' });
    } catch (err) { setError(errorMessage(err)); }
    finally { setPicking(false); }
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const validation = validateSource(source);
    if (validation) { setError(validation); return; }
    submitted.current = true;
    if (!await onSave({ ...source, name: source.name.trim(), location: source.location.trim() })) submitted.current = false;
  };
  const kinds: SourceKind[] = ['local', 'manifest', 'drive', 'photos'];
  return <dialog ref={dialog} className="source-dialog" onCancel={event => { event.preventDefault(); if (!busy) onClose(); }} aria-labelledby="source-dialog-title">
    <div className="dialog-heading"><div><h2 id="source-dialog-title">{initial ? 'Chỉnh sửa nguồn ảnh' : 'Thêm nguồn ảnh'}</h2></div><button className="icon-button" aria-label="Đóng" onClick={onClose} disabled={busy}><X size={20} /></button></div>
    {!initial && <div className="source-kind-picker">{kinds.map(kind => <button key={kind} className={source.kind === kind || (kind === 'manifest' && ['url','webdav'].includes(source.kind)) ? 'selected' : ''} onClick={() => { update({ kind, location: '' }); setError(''); }}><SourceIcon kind={kind} /><span>{kind === 'local' ? 'Thiết bị' : kind === 'manifest' ? 'Online' : sourceLabels[kind]}</span></button>)}</div>}
    <form onSubmit={submit}>
      <label className="field">Tên nguồn<input autoComplete="off" placeholder="Ví dụ: Ảnh gia đình" maxLength={80} value={source.name} onChange={event => update({ name: event.target.value })} required /></label>
      {source.kind === 'local' ? <>
        <button type="button" className="folder-picker" onClick={chooseFolder} disabled={picking}><FolderOpen size={26} /><strong>{picking ? 'Đang mở thư mục…' : source.location ? 'Đã chọn thư mục · Chọn lại' : 'Chọn thư mục ảnh'}</strong><small>{source.location || 'JPG, PNG, WebP · Có thể chọn USB'}</small></button>
        <input ref={picker} type="file" hidden multiple accept="image/jpeg,image/png,image/webp" {...({ webkitdirectory: '', directory: '' } as object)} onChange={event => {
          const files = event.target.files;
          if (!files?.length) return;
          const count = registerPreviewFiles(source.id, files);
          const folder = files[0].webkitRelativePath.split('/')[0] || 'Ảnh đã chọn';
          update({ location: `preview:${folder}`, name: source.name || folder });
          setError(count ? '' : 'Thư mục chưa có ảnh JPG, PNG hoặc WebP.');
        }} />
        <label className="checkbox-row"><input type="checkbox" checked={source.recursive} onChange={event => update({ recursive: event.target.checked })} /> Bao gồm thư mục con</label>
      </> : <>
        {['manifest', 'url', 'webdav'].includes(source.kind) && <label className="field">Loại nguồn<div className="select-wrap"><select value={source.kind} onChange={event => update({ kind: event.target.value as SourceKind })}><option value="manifest">Danh sách ảnh JSON</option><option value="url">Link một ảnh</option><option value="webdav">Thư mục WebDAV / NAS</option></select><ChevronDown size={16} /></div></label>}
        <label className="field">{source.kind === 'drive' ? 'Link hoặc ID thư mục Drive' : source.kind === 'photos' ? 'ID thiết bị Google Photos Ambient' : 'Địa chỉ nguồn ảnh'}<input autoComplete="off" placeholder={source.kind === 'drive' ? 'https://drive.google.com/drive/folders/…' : source.kind === 'photos' ? 'Device ID đã được cấp quyền' : 'https://example.com/photos/manifest.json'} value={source.location} onChange={event => update({ location: event.target.value })} required /></label>
        {source.kind === 'manifest' && <div className="inline-note">JSON: mảng URL hoặc <code>{'{ "images": ["a.jpg", "b.webp"] }'}</code>. URL tương đối tính theo địa chỉ JSON.</div>}
        {source.kind === 'webdav' && <div className="field-pair"><label className="field">Tên đăng nhập<input autoComplete="off" value={source.username || ''} onChange={event => update({ username: event.target.value })} /></label><label className="field">Mật khẩu<input type="password" autoComplete="new-password" placeholder={initial ? 'Giữ mật khẩu đã lưu' : 'Mật khẩu WebDAV'} value={source.password ?? ''} onChange={event => update({ password: event.target.value })} /></label></div>}
        {source.kind === 'drive' && <div className="inline-note">Công khai: API key. Riêng: đăng nhập Google Drive.</div>}
        {source.kind === 'photos' && <div className="inline-note">{isAndroid() ? 'TV: album qua Ambient API. Điện thoại: trình chọn ảnh hệ thống.' : 'Windows: nhập ảnh qua Picker, không tự đồng bộ album. TV: album qua Ambient API.'}</div>}
        {['drive','webdav'].includes(source.kind) && <label className="checkbox-row"><input type="checkbox" checked={source.recursive} onChange={event => update({ recursive: event.target.checked })} /> Bao gồm thư mục con</label>}
        {(source.kind === 'drive' || source.kind === 'photos') && <ConnectionHelp kind={source.kind} />}
        {['drive','photos'].includes(source.kind) && <GoogleConnect key={source.kind} source={source} onConnected={update} />}
        {['drive','photos'].includes(source.kind) && <details className="advanced-settings" open={source.kind === 'photos'}><summary>Thông tin kết nối <ArrowUpRight size={13} /></summary>{source.kind === 'drive' && <label className="field">Google API key · thư mục công khai<input type="password" autoComplete="off" placeholder={initial ? 'Giữ API key đã lưu' : 'API key của dự án Google'} value={source.apiKey ?? ''} onChange={event => update({ apiKey: event.target.value })} /></label>}<label className="field">Access token · nguồn riêng<input type="password" autoComplete="off" placeholder={initial ? 'Giữ quyền truy cập đã lưu' : 'Token OAuth đã được cấp quyền'} value={source.accessToken ?? ''} onChange={event => update({ accessToken: event.target.value })} /></label><small>Token thủ công có thể hết hạn.</small></details>}
      </>}
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="dialog-footer"><button type="button" className="button secondary" onClick={onClose} disabled={busy}>Hủy</button><button className="button primary" type="submit" disabled={busy || picking}><Check size={17} />{busy ? 'Đang lưu…' : initial ? 'Lưu thay đổi' : 'Thêm nguồn ảnh'}</button></div>
    </form>
  </dialog>;
}
