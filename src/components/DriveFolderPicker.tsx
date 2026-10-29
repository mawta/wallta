import { useEffect, useRef, useState } from 'react';
import { Check, FolderOpen, LoaderCircle, RefreshCw } from 'lucide-react';
import { callNative, errorMessage } from '../lib/native';

export interface DriveFolder { id: string; name: string; account?: string }
interface FolderPage { folders: DriveFolder[]; nextPageToken?: string; account?: string }
function folderId(location: string) { return /\/folders\/([A-Za-z0-9_-]+)/.exec(location)?.[1] || (/^[A-Za-z0-9_-]+$/.test(location) ? location : 'root'); }

export function DriveFolderPicker({ sourceId, location, selectionName, onSelect, disabled = false }: {
  sourceId: string; location: string; selectionName?: string; onSelect: (folder: DriveFolder) => void; disabled?: boolean;
}) {
  const [path, setPath] = useState<DriveFolder[]>(() => location ? [{ id: folderId(location), name: selectionName || 'Thư mục đã lưu' }] : []);
  const [shared, setShared] = useState(false);
  const [page, setPage] = useState<FolderPage>({ folders: [] });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const generation = useRef(0);
  const current = path[path.length - 1];
  const parentId = current?.id || 'root';
  const listShared = shared && path.length === 0;
  const load = async (pageToken?: string) => {
    const request = ++generation.current;
    setLoading(true); setError('');
    try {
      const next = await callNative<FolderPage>('google_drive_folders', { sourceId, parentId, pageToken: pageToken || null, shared: listShared });
      if (generation.current !== request) return;
      setPage(previous => ({ ...next, folders: pageToken ? [...previous.folders, ...next.folders.filter(f => !previous.folders.some(old => old.id === f.id))] : next.folders }));
    } catch (err) { if (generation.current === request) setError(errorMessage(err)); }
    finally { if (generation.current === request) setLoading(false); }
  };
  useEffect(() => {
    setPage({ folders: [] }); void load();
    return () => { generation.current++; };
    // A changed folder starts a new request; responses for old paths are discarded.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceId, parentId, listShared]);
  const choose = async () => {
    const request = ++generation.current; setLoading(true); setError('');
    try {
      const folder = await callNative<DriveFolder>('google_drive_select', { sourceId, folderId: parentId });
      if (generation.current === request) onSelect(folder);
    } catch (err) { if (generation.current === request) setError(errorMessage(err)); }
    finally { if (generation.current === request) setLoading(false); }
  };
  return <section className="drive-folder-picker" aria-label="Chọn thư mục Google Drive" aria-busy={loading}>
    <div className="drive-folder-tabs"><button type="button" className={`text-button ${!shared ? 'active' : ''}`} disabled={disabled || loading} onClick={() => { setShared(false); setPath([]); }}>Drive của tôi</button><button type="button" className={`text-button ${shared ? 'active' : ''}`} disabled={disabled || loading} onClick={() => { setShared(true); setPath([]); }}>Được chia sẻ</button></div>
    {page.account && <small className="muted">{page.account}</small>}
    <nav className="drive-breadcrumb" aria-label="Đường dẫn thư mục"><button type="button" disabled={disabled || loading} onClick={() => setPath([])}>{shared ? 'Được chia sẻ' : 'Drive của tôi'}</button>{path.map((folder, index) => <span key={folder.id}> / <button type="button" disabled={disabled || loading} onClick={() => setPath(path.slice(0, index + 1))}>{folder.name}</button></span>)}</nav>
    <div className="drive-folder-list">{page.folders.map(folder => <button type="button" key={folder.id} disabled={disabled || loading} onClick={() => setPath([...path, folder])}><FolderOpen size={18} /><span>{folder.name}</span></button>)}{!loading && !error && !page.folders.length && <p className="muted small">Không có thư mục con.</p>}</div>
    {loading && <p className="muted small" role="status"><LoaderCircle size={15} className="spin" /> Đang tải thư mục…</p>}
    {error && <div role="alert" className="form-error">{error}<button type="button" className="text-button" disabled={disabled || loading} onClick={() => void load()}><RefreshCw size={14} />Thử lại</button></div>}
    {page.nextPageToken && <button type="button" className="text-button" disabled={disabled || loading} onClick={() => void load(page.nextPageToken)}>Xem thêm</button>}
    {!listShared && <button type="button" className="button secondary" disabled={disabled || loading || Boolean(error)} onClick={() => void choose()}><Check size={16} />Dùng thư mục này</button>}
  </section>;
}
