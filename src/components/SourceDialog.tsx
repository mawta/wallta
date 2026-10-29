import { uuid } from '../lib/compat';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { X, FolderOpen, ArrowUpRight, Check, ChevronDown } from 'lucide-react';
import { isNative, isAndroid, callNative, registerPreviewFiles, errorMessage } from '../lib/native';
import { validateSource } from '../lib/rules';
import { sourceLabels, type Source, type SourceKind } from '../lib/types';
import { SourceIcon } from './ui';
import { GoogleConnect } from './GoogleConnect';
import { ConnectionHelp } from './ConnectionHelp';
import { DriveFolderPicker } from './DriveFolderPicker';
import { SavedGoogleConnections } from './SavedGoogleConnections';
import { addSourceFolder, setSourceFolders, sourceFolders, supportsFolders } from '../lib/folders';
import { FolderSelections } from './FolderSelections';

export interface SourceSaveOptions { apply: boolean; autostart: boolean }
interface Props { initial?: Source; initialKind?: SourceKind; chooseFolderOnOpen?: boolean; savedSources?: Source[]; onOpenSaved?: (source: Source) => void; onSave: (source: Source, options: SourceSaveOptions) => Promise<boolean>; onClose: () => void; busy: boolean }
export function SourceDialog({ initial, initialKind = 'local', chooseFolderOnOpen = false, savedSources = [], onOpenSaved, onSave, onClose, busy }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const picker = useRef<HTMLInputElement>(null);
  const [source, setSource] = useState<Source>(() => initial || { id: uuid(), name: '', kind: initialKind, location: '', recursive: true, enabled: true });
  const sourceRef = useRef(source); sourceRef.current = source;
  const [error, setError] = useState('');
  const [picking, setPicking] = useState(false);
  const [browseDrive, setBrowseDrive] = useState(chooseFolderOnOpen);
  const [reconnect, setReconnect] = useState(false);
  const [reselectPhotos, setReselectPhotos] = useState(false);
  const [apply, setApply] = useState(true);
  const [autostart, setAutostart] = useState(true);
  const [folderDraft, setFolderDraft] = useState('');
  const multipleFolders = supportsFolders(source);
  const folders = multipleFolders ? sourceFolders(source) : [];
  const desktopDrive = source.kind === 'drive' && isNative() && !isAndroid();
  const pendingDrive = desktopDrive && !source.location.trim() && source.google?.connected;
  const submitted = useRef(false);
  useEffect(() => () => { if (isNative()) void callNative('google_cancel', { sourceId: source.id, discardImports: !submitted.current }).catch(() => {}); }, [source.id]);
  useEffect(() => { const element = dialog.current; element?.showModal(); return () => element?.close(); }, []);
  const update = (value: Partial<Source>) => setSource(previous => ({ ...previous, ...value }));
  const chooseFolder = async () => {
    if (!isNative()) { picker.current?.click(); return; }
    setPicking(true); setError('');
    try {
      const location = await callNative<string | null>('pick_folder');
      if (location) {
        const name = (location.startsWith('content:') ? 'Ảnh trên thiết bị' : location.split(/[\\/]/).filter(Boolean).pop()) || 'Thư mục ảnh';
        const previous = sourceRef.current;
        setSource({ ...addSourceFolder(previous, { location, name }), name: previous.name || name });
      }
    } catch (err) { setError(errorMessage(err)); }
    finally { setPicking(false); }
  };
  const addFolderDraft = () => {
    try { setSource(addSourceFolder(source, { location: folderDraft })); setFolderDraft(''); setError(''); }
    catch (err) { setError(errorMessage(err)); }
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const next = { ...source, name: source.name.trim(), location: source.location.trim(), enabled: pendingDrive ? false : source.enabled };
    const validation = validateSource(next);
    if (validation) { setError(validation); return; }
    submitted.current = true;
    if (!await onSave(next, { apply: apply && Boolean(next.location) && next.enabled, autostart: isNative() && !isAndroid() && autostart })) submitted.current = false;
  };
  const kinds: SourceKind[] = ['local', 'manifest', 'drive', 'photos'];
  return <dialog ref={dialog} className="source-dialog" onCancel={event => { event.preventDefault(); if (!busy) onClose(); }} aria-labelledby="source-dialog-title">
    <div className="dialog-heading"><div><h2 id="source-dialog-title">{initial ? 'Chỉnh sửa nguồn ảnh' : 'Thêm nguồn ảnh'}</h2></div><button className="icon-button" aria-label="Đóng" onClick={onClose} disabled={busy}><X size={20} /></button></div>
    {!initial && <div className="source-kind-picker">{kinds.map(kind => <button key={kind} disabled={busy} className={(source.google?.provider === 'photos' ? kind === 'photos' : source.kind === kind) || (kind === 'manifest' && ['url','webdav'].includes(source.kind)) ? 'selected' : ''} onClick={() => { update({ kind, location: '', folders: undefined, google: undefined }); setBrowseDrive(false); setReconnect(false); setReselectPhotos(false); setFolderDraft(''); setError(''); }}><SourceIcon kind={kind} /><span>{kind === 'local' ? 'Thiết bị' : kind === 'manifest' ? 'Online' : sourceLabels[kind]}</span></button>)}</div>}
    {!initial && !source.google?.connected && (source.kind === 'drive' || source.kind === 'photos') && onOpenSaved && <SavedGoogleConnections sources={savedSources} provider={source.kind} busy={busy} action="Dùng kết nối" onOpen={onOpenSaved} />}
    <form onSubmit={submit}>
      <label className="field">Tên nguồn<input autoComplete="off" placeholder="Ví dụ: Ảnh gia đình" maxLength={80} value={source.name} onChange={event => update({ name: event.target.value })} required /></label>
      {multipleFolders && <FolderSelections folders={folders} busy={busy} onRemove={index => setSource(setSourceFolders(source, folders.filter((_, i) => i !== index)))} />}
      {source.kind === 'local' ? <>
        {source.google?.provider === 'photos' ? <div className="google-connect"><strong>Google Photos · {source.google.selectionName || 'Bộ ảnh đã nhập'}</strong>{source.google.account && <small>{source.google.account}</small>}<button type="button" className="button secondary" disabled={busy} onClick={() => setReselectPhotos(!reselectPhotos)}>Chọn lại ảnh Google Photos</button>{reselectPhotos && <GoogleConnect source={{ ...source, kind: 'photos', location: '' }} onConnected={changes => { update(changes); setReselectPhotos(false); }} />}</div> : <button type="button" className="folder-picker" onClick={chooseFolder} disabled={busy || picking}><FolderOpen size={26} /><strong>{picking ? 'Đang mở thư mục…' : source.location ? 'Thêm thư mục' : 'Chọn thư mục ảnh'}</strong><small>JPG, PNG, WebP · Máy / USB</small></button>}
        <input ref={picker} type="file" hidden multiple accept="image/jpeg,image/png,image/webp" {...({ webkitdirectory: '', directory: '' } as object)} onChange={event => {
          const files = event.target.files;
          if (!files?.length) return;
          const count = registerPreviewFiles(source.id, files);
          const folder = files[0].webkitRelativePath.split('/')[0] || 'Ảnh đã chọn';
          update({ location: `preview:${folder}`, folders: undefined, name: source.name || folder });
          setError(count ? '' : 'Thư mục chưa có ảnh JPG, PNG hoặc WebP.');
        }} />
        <label className="checkbox-row"><input type="checkbox" checked={source.recursive} onChange={event => update({ recursive: event.target.checked })} /> Bao gồm thư mục con</label>
      </> : <>
        {['manifest', 'url', 'webdav'].includes(source.kind) && <label className="field">Loại nguồn<div className="select-wrap"><select value={source.kind} disabled={busy} onChange={event => { update({ kind: event.target.value as SourceKind, location: '', folders: undefined }); setFolderDraft(''); }}><option value="manifest">Danh sách ảnh JSON</option><option value="url">Link một ảnh</option><option value="webdav">Thư mục WebDAV / NAS</option></select><ChevronDown size={16} /></div></label>}
        {desktopDrive && <div className="google-connect"><strong>{source.google?.connected ? 'Đã kết nối' : 'Google Drive'}</strong>{source.google?.account && <small>{source.google.account}</small>}<button type="button" className="button secondary" disabled={busy} onClick={() => setBrowseDrive(!browseDrive)}><FolderOpen size={17} />Chọn / đổi thư mục</button>{browseDrive && <DriveFolderPicker key={source.google?.connectionId || source.id} sourceId={source.id} location={source.location} selectionName={source.google?.selectionName} disabled={busy} onSelect={folder => { try { const previous = sourceRef.current; setSource(addSourceFolder({ ...previous, name: !previous.name || previous.name === 'Google Drive' ? folder.name : previous.name, google: { ...previous.google, provider: 'drive', connected: true, account: folder.account || previous.google?.account } }, { location: folder.id, name: folder.name })); setBrowseDrive(false); setError(''); } catch (err) { setError(errorMessage(err)); } }} />}</div>}
        {multipleFolders ? <div className="folder-entry"><label className="field">{source.kind === 'drive' ? 'Link hoặc ID thư mục Drive' : 'Địa chỉ thư mục WebDAV'}<input autoComplete="off" value={folderDraft} onChange={event => setFolderDraft(event.target.value)} placeholder={source.kind === 'drive' ? 'https://drive.google.com/drive/folders/…' : 'https://nas.example.com/photos/'} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); addFolderDraft(); } }} /></label><button type="button" className="button secondary" disabled={busy || !folderDraft.trim()} onClick={addFolderDraft}>Thêm thư mục</button></div> : source.kind === 'photos' && !isAndroid() ? <details className="advanced-settings"><summary>Google Photos Ambient · nâng cao</summary><label className="field">ID thiết bị Ambient<input value={source.location} onChange={event => update({ location: event.target.value })} autoComplete="off" /></label></details> : <label className="field">{source.kind === 'photos' ? 'ID thiết bị Google Photos Ambient' : 'Địa chỉ nguồn ảnh'}<input autoComplete="off" placeholder={source.kind === 'photos' ? 'Device ID đã được cấp quyền' : 'https://example.com/photos/manifest.json'} value={source.location} onChange={event => update({ location: event.target.value })} required={source.kind !== 'photos'} /></label>}
        {source.kind === 'manifest' && <div className="inline-note">JSON: mảng URL hoặc <code>{'{ "images": ["a.jpg", "b.webp"] }'}</code>. URL tương đối tính theo địa chỉ JSON.</div>}
        {source.kind === 'webdav' && <div className="field-pair"><label className="field">Tên đăng nhập<input autoComplete="off" value={source.username || ''} onChange={event => update({ username: event.target.value })} /></label><label className="field">Mật khẩu<input type="password" autoComplete="new-password" placeholder={initial ? 'Giữ mật khẩu đã lưu' : 'Mật khẩu WebDAV'} value={source.password ?? ''} onChange={event => update({ password: event.target.value })} /></label></div>}
        {source.kind === 'drive' && <div className="inline-note">Công khai: API key. Riêng: đăng nhập Google Drive.</div>}
        {source.kind === 'photos' && <div className="inline-note">{isAndroid() ? 'TV: album qua Ambient API. Điện thoại: trình chọn ảnh hệ thống.' : 'Windows: nhập ảnh qua Picker, không tự đồng bộ album. TV: album qua Ambient API.'}</div>}
        {['drive','webdav'].includes(source.kind) && <label className="checkbox-row"><input type="checkbox" checked={source.recursive} onChange={event => update({ recursive: event.target.checked })} /> Bao gồm thư mục con</label>}
        {(source.kind === 'drive' || source.kind === 'photos') && <ConnectionHelp kind={source.kind} />}
        {['drive','photos'].includes(source.kind) && (!source.google?.connected || reconnect ? <GoogleConnect key={source.kind} source={source} onConnected={changes => { update({ ...changes, enabled: changes.kind === 'drive' && !(changes.location ?? source.location) ? false : true }); setReconnect(false); setBrowseDrive(false); }} /> : <button type="button" className="text-button" disabled={busy} onClick={() => setReconnect(true)}>Kết nối lại tài khoản Google</button>)}
        {['drive','photos'].includes(source.kind) && <details className="advanced-settings" open={source.kind === 'photos'}><summary>Thông tin kết nối <ArrowUpRight size={13} /></summary>{source.kind === 'drive' && <label className="field">Google API key · thư mục công khai<input type="password" autoComplete="off" placeholder={initial ? 'Giữ API key đã lưu' : 'API key của dự án Google'} value={source.apiKey ?? ''} onChange={event => update({ apiKey: event.target.value })} /></label>}<label className="field">Access token · nguồn riêng<input type="password" autoComplete="off" placeholder={initial ? 'Giữ quyền truy cập đã lưu' : 'Token OAuth đã được cấp quyền'} value={source.accessToken ?? ''} onChange={event => update({ accessToken: event.target.value })} /></label><small>Token thủ công có thể hết hạn.</small></details>}
      </>}
      {pendingDrive ? <p className="muted small">Có thể lưu tài khoản và chọn thư mục sau.</p> : source.location && <><label className="checkbox-row"><input type="checkbox" checked={apply} onChange={event => setApply(event.target.checked)} />Áp dụng sau khi lưu</label>{apply && isNative() && !isAndroid() && <label className="checkbox-row"><input type="checkbox" checked={autostart} onChange={event => setAutostart(event.target.checked)} />Khởi động cùng Windows</label>}</>}
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="dialog-footer"><button type="button" className="button secondary" onClick={onClose} disabled={busy}>Hủy</button><button className="button primary" type="submit" disabled={busy || picking}><Check size={17} />{busy ? 'Đang lưu…' : pendingDrive ? 'Lưu kết nối' : apply && source.location && source.enabled ? 'Lưu & áp dụng' : 'Lưu nguồn'}</button></div>
    </form>
  </dialog>;
}
