import { t, useLocale } from './lib/i18n';
import { useEffect, useState } from 'react';
import { LayoutDashboard, Images, Clock3, Settings2, Plus, ArrowUpRight, ArrowRight, RefreshCw, Play, Pause, SkipForward, Expand, Check, Monitor, Smartphone, Tv, Ellipsis, Pencil, Trash2, X, AlertCircle, WifiOff, ImagePlus, type LucideIcon } from 'lucide-react';
import { useWallta } from './lib/useWallta';
import { useUpdates } from './lib/useUpdates';
import { callNative, isNative } from './lib/native';
import { sourceLabels, type Source, type SourceKind, type MediaItem } from './lib/types';
import { SourceIcon, Toggle, SectionTitle, intervalLabel } from './components/ui';
import { SourceDialog, type SourceSaveOptions } from './components/SourceDialog';
import { LanTransfer } from './components/LanTransfer';
import { PhotoPreview } from './components/PhotoPreview';
import { MediaImage } from './components/MediaImage';
import { UpscaleDialog } from './components/UpscaleDialog';
import { UpscaleProgress } from './components/UpscaleProgress';
import { useUpscale } from './lib/useUpscale';
import { applyUpscale } from './lib/applyUpscale';
import { sourceFolders, supportsFolders } from './lib/folders';
import { Slideshow } from './components/Slideshow';
import { Schedule } from './pages/Schedule';
import { SettingsPage } from './pages/Settings';

type Page = 'overview' | 'sources' | 'schedule' | 'settings';
const nav: { page: Page; label: string; Icon: LucideIcon }[] = [{ page: 'overview', label: 'Tổng quan', Icon: LayoutDashboard }, { page: 'sources', label: 'Nguồn ảnh', Icon: Images }, { page: 'schedule', label: 'Lịch & hiển thị', Icon: Clock3 }, { page: 'settings', label: 'Cài đặt', Icon: Settings2 }];

export default function App() {
  const { locale } = useLocale();
  useEffect(() => { document.documentElement.lang = locale; document.title = t('Wallta — Hình nền tự động'); }, [locale]);
  const wallta = useWallta();
  const { state, capabilities, media, busy: operationBusy, loaded, notice, save, next, toggle, refresh, perform, notify } = wallta;
  const updates = useUpdates(loaded);
  const upscaleController = useUpscale(loaded, { onChanged: refresh, onApply: async source => {
    const applied = await perform(async () => { await applyUpscale(source); });
    if (applied) { await refresh(); notify(t("Đã áp dụng ảnh 4K.")); }
    return Boolean(applied);
  } });
  const busy = operationBusy || updates.status.phase === 'installing';
  const [page, setPage] = useState<Page>('overview');
  const [dialog, setDialog] = useState<{ initial?: Source; kind?: SourceKind; choose?: boolean } | null>(null);
  const [slideshow, setSlideshow] = useState(false);
  const [photoPreview,setPhotoPreview]=useState<{items:MediaItem[];initialId:string}|null>(null);
  const [transfer, setTransfer] = useState(false);
  const [upscale, setUpscale] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState<Source | null>(null);
  const [menu, setMenu] = useState<string | null>(null);
  const enabledSources = state.sources.filter(source => source.enabled);
  const activeSource = state.sources.find(source => source.id === state.current?.sourceId);
  const platform = capabilities?.platform || 'browser';
  const PlatformIcon = platform === 'windows' || platform === 'browser' ? Monitor : platform === 'android-tv' ? Tv : Smartphone;
  const add = (kind: SourceKind = 'local') => setDialog({ kind });
  const saveSource = async (source: Source, options: SourceSaveOptions) => {
    const saved = await wallta.saveSource(source, options);
    if (saved) setDialog(null);
    return saved;
  };
  const sourceCards = (limit?: number) => state.sources.slice(0, limit).map(source => <article className={`source-card ${!source.enabled && !source.google?.connected ? 'disabled-source' : ''}`} key={source.id}>
    <div className={`source-icon ${source.google?.provider || source.kind}`}><SourceIcon kind={source.google?.provider || source.kind} size={24} /></div>
    <div className="source-card-title"><strong>{source.name}</strong><span>{t(sourceLabels[source.google?.provider || source.kind])}</span>{supportsFolders(source) && sourceFolders(source).length > 0 && <small>{sourceFolders(source).length} {t("thư mục")}</small>}{source.location && <small title={source.location}>{source.google?.selectionName || source.location}</small>}{source.google?.account && <small>{source.google.account}</small>}{source.kind === 'drive' && <button className="text-button" disabled={busy} onClick={() => setDialog({ initial: source, choose: true })}><Images size={13} />{t("Chọn / đổi thư mục")}</button>}{source.google?.provider === 'photos' && <button className="text-button" disabled={busy} onClick={() => setDialog({ initial: source })}>{t("Chọn lại ảnh")}</button>}</div>
    <div className="source-card-menu"><button className="icon-button" aria-label={t("Tùy chọn {0}", { 0: source.name })} aria-expanded={menu === source.id} onClick={() => setMenu(menu === source.id ? null : source.id)}><Ellipsis size={20} /></button>{menu === source.id && <div className="dropdown-menu"><button onClick={() => { setDialog({ initial: source }); setMenu(null); }}><Pencil size={14} />{t("Chỉnh sửa")}</button><button className="danger-text" onClick={() => { setDeleting(source); setMenu(null); }}><Trash2 size={14} />{t("Xóa nguồn")}</button></div>}</div>
    <div className="source-card-bottom"><span className={`source-status ${source.enabled ? 'enabled' : ''}`}><i />{!source.location ? source.google?.connected ? t("Đã kết nối · Chưa chọn thư mục") : t("Chưa chọn thư mục") : source.id === state.current?.sourceId ? t("Đang hiển thị") : source.enabled ? t("Đã bật") : t("Đã tắt")}</span><Toggle checked={source.enabled} label={t("Bật nguồn {0}", { 0: source.name })} disabled={busy || !source.location} onChange={() => void save({ ...state, sources: state.sources.map(item => item.id === source.id ? { ...item, enabled: !item.enabled } : item) })} /></div>
  </article>);
  return <div className={`app-shell ${upscaleController.status.jobId ? 'has-upscale-bar' : ''}`}>
    <aside className="sidebar">
      <a className="brand" href="#" onClick={event => { event.preventDefault(); setPage('overview'); }} aria-label={t("Wallta - Tổng quan")}><img src="./wallta.svg" alt="" /><span>wallta<span className="brand-dot">.</span></span></a>
      <nav aria-label={t("Điều hướng chính")}>{nav.map(({ page: value, label, Icon }) => <button key={value} className={`nav-item ${page === value ? 'active' : ''}`} onClick={() => setPage(value)} aria-label={t(label)} aria-current={page === value ? 'page' : undefined}><Icon size={19} strokeWidth={1.6} /><span>{t(label)}</span>{value === 'settings' && ['available', 'ready', 'permissionRequired'].includes(updates.status.phase) && <i className="update-dot" aria-label={t("Có bản cập nhật")} />}{value === 'sources' && state.sources.length > 0 && <span className="nav-count">{state.sources.length}</span>}</button>)}</nav>
      <div className="sidebar-spacer" />
      <div className="device-status"><span className="device-icon"><PlatformIcon size={18} /></span><div><strong>{platform === 'browser' ? t("Bản xem trước") : platform === 'windows' ? 'Windows' : platform === 'android-tv' ? 'Android TV' : 'Android'}</strong><small>{state.running ? t("Tự đổi: Bật") : t("Tự đổi: Tắt")}</small>{platform === 'windows' && <small>{t("Cùng Windows:")} {state.settings.autostart ? t("Bật") : t("Tắt")}</small>}</div><i className={state.running ? 'live' : ''} /></div>
    </aside>
    <div className="main-column"><header className="topbar"><div className="breadcrumb">Wallta <span>/</span><strong>{t(nav.find(item => item.page === page)?.label || '')}</strong></div><div className="topbar-right"><span className="version-pill">{updates.status.currentVersion}</span><button className="icon-button" title={t("Làm mới nguồn ảnh")} aria-label={t("Làm mới nguồn ảnh")} disabled={busy} onClick={() => void refresh()}><RefreshCw size={17} className={busy ? 'spin' : ''} /></button><span className="avatar">W</span></div></header>
      <main className="main-content">
        {platform === 'browser' && <div className="preview-banner"><Monitor size={14} /><span>{t("Bản web: không đổi hình nền hệ thống hoặc chạy nền.")}</span></div>}
        {!loaded ? <div className="loading-screen"><RefreshCw className="spin" size={30} /><p>{t("Đang tải…")}</p></div> : <>
        {page === 'overview' && <>
          <SectionTitle title={t("Tổng quan")}><button className="button primary" onClick={() => add()}><Plus size={18} />{t("Thêm nguồn ảnh")}</button></SectionTitle>
          <div className="overview-grid"><section className="wallpaper-card">
            <div className="wallpaper-preview"><MediaImage item={state.current} alt={state.current?.name || t("Ảnh minh họa")} style={{ objectFit: state.settings.fit === 'fill' ? 'cover' : state.settings.fit === 'fit' ? 'contain' : 'fill' }} /><div className="wallpaper-shade" /><span className="photo-badge"><span className={state.running ? 'pulse-dot' : 'quiet-dot'} />{state.current ? t("Ảnh hiện tại") : t("Ảnh minh họa")}</span><button className="preview-expand icon-button" aria-label={t("Xem trước ảnh")} disabled={!state.current} onClick={() => {if(state.current)setPhotoPreview({items:media.some(item=>item.id===state.current?.id)?media:[state.current],initialId:state.current.id});}}><Expand size={17} /></button><div className="wallpaper-caption">{activeSource && <span>{t(sourceLabels[activeSource.google?.provider || activeSource.kind])} · {activeSource.google?.selectionName || activeSource.name}{activeSource.google?.account ? ` · ${activeSource.google.account}` : ''}</span>}<h3>{state.current?.name || t("Chưa có ảnh")}</h3>{!state.current && <p>{t("Thêm nguồn ảnh để bắt đầu.")}</p>}</div><div className="photo-index">{state.current ? String(Math.max(1, media.findIndex(item => item.id === state.current?.id) + 1)).padStart(2, '0') : '—'} <span>/ {String(media.length).padStart(2, '0')}</span></div></div>
            <div className="playback-bar"><div className="playback-status"><span className={`status-indicator ${state.running ? 'running' : ''}`} /><div><strong>{state.running ? t("Tự đổi: Bật") : t("Tự đổi: Tắt")}</strong>{state.running && <small>{intervalLabel(state.settings.intervalSeconds)} · {state.settings.order === 'shuffle' ? t("Ngẫu nhiên") : t("Tuần tự")}</small>}</div></div><div className="playback-actions"><button className="text-button" disabled={busy || !state.current} onClick={() => setUpscale(state.current ? [state.current.id] : [])}>4K</button><button className="icon-button" title={t("Đổi ảnh ngay")} aria-label={t("Đổi ảnh ngay")} disabled={busy || !media.length} onClick={() => void next()}><SkipForward size={18} /></button><button className={`play-button ${state.running ? 'playing' : ''}`} aria-label={state.running ? t("Tạm dừng tự đổi ảnh") : t("Bật tự đổi ảnh")} disabled={busy || (!media.length && !state.running)} onClick={() => void toggle()}>{state.running ? <Pause size={17} fill="currentColor" /> : <Play size={17} fill="currentColor" />}</button></div></div>
          </section><aside className="overview-aside"><section className="panel quick-settings"><h3>{t("Thiết lập")}</h3><div className="quick-setting"><div className="quick-icon"><Clock3 size={18} /></div><div><small>{t("Chu kỳ đổi ảnh")}</small><strong>{intervalLabel(state.settings.intervalSeconds)}</strong></div><button className="icon-button" aria-label={t("Sửa chu kỳ đổi ảnh")} onClick={() => setPage('schedule')}><ArrowUpRight size={17} /></button></div><div className="quick-setting"><div className="quick-icon"><Images size={18} /></div><div><small>{t("Thứ tự")}</small><strong>{state.settings.order === 'shuffle' ? t("Ngẫu nhiên") : t("Tuần tự")}</strong></div></div><div className="quick-setting"><div className="quick-icon"><Expand size={18} /></div><div><small>{t("Hiển thị")}</small><strong>{state.settings.fit === 'fill' ? t("Phủ kín") : state.settings.fit === 'fit' ? t("Vừa ảnh") : t("Kéo giãn")}</strong></div></div><button className="text-button quick-link" onClick={() => setPage('schedule')}>{t("Chỉnh sửa")} <ArrowRight size={15} /></button></section><section className="library-summary"><span className="summary-decoration"><Images size={66} strokeWidth={.7} /></span><span className="eyebrow">{t("THƯ VIỆN")}</span><div className="library-number">{media.length}<span>{t("ảnh khả dụng")}</span></div><div className="library-meta"><span><i />{enabledSources.length} {t("nguồn đang bật")}</span><button className="icon-button" aria-label={t("Xem nguồn ảnh")} onClick={() => setPage('sources')}><ArrowUpRight size={18} /></button></div></section></aside></div>
          {state.error && <div className="source-error"><WifiOff size={17} /><div><strong>{t("Lỗi nguồn ảnh")}</strong><p>{t(state.error)}</p></div><button className="text-button" onClick={() => setPage('sources')}>{t("Xem nguồn")} <ArrowRight size={14} /></button></div>}
          <div className="subsection-heading"><div><h3>{t("Nguồn ảnh")}</h3></div><button className="text-button" onClick={() => setPage('sources')}>{t("Tất cả nguồn ảnh")} <ArrowRight size={15} /></button></div>
          <div className="source-grid">{sourceCards(3)}{state.sources.length < 3 && <button className="add-source-card" onClick={() => add()}><span><Plus size={21} /></span><strong>{t("Thêm nguồn ảnh")}</strong><small>{t("Thiết bị, Drive, Photos hoặc online")}</small></button>}</div>

        </>}
        {page === 'sources' && <><SectionTitle title={t("Nguồn ảnh")}><div className="section-actions"><button className="button secondary" disabled={busy || !media.length} onClick={() => setUpscale([])}><Expand size={16} />{t("Nâng ảnh 4K")}</button><button className="button secondary" onClick={() => setTransfer(true)}><ArrowUpRight size={16} />{t("Gửi / nhận qua Wi-Fi")}</button><button className="button primary" onClick={() => add()}><Plus size={18} />{t("Thêm nguồn ảnh")}</button></div></SectionTitle><div className="source-type-strip">{(['local','drive','manifest','photos'] as SourceKind[]).map(kind => <button key={kind} onClick={() => add(kind)}><SourceIcon kind={kind} size={22} /><div><strong>{t(sourceLabels[kind])}</strong><small>{kind === 'local' ? t("Máy / USB") : kind === 'drive' ? t("Thư mục Drive") : kind === 'photos' ? t("Nhập ảnh / album TV") : t("JSON, WebDAV & link ảnh")}</small></div><Plus size={16} /></button>)}</div><div className="subsection-heading"><div><h3>{t("Nguồn đã thêm")} <span className="count-badge">{state.sources.length}</span></h3><p>{enabledSources.length} {t("nguồn đang bật ·")} {media.length} {t("ảnh khả dụng")}</p></div><button className="button secondary" disabled={busy} onClick={() => void refresh()}><RefreshCw size={15} />{t("Làm mới")}</button></div>{state.sources.length ? <div className="source-grid full">{sourceCards()}<button className="add-source-card" onClick={() => add()}><span><Plus size={21} /></span><strong>{t("Thêm nguồn ảnh")}</strong></button></div> : <div className="panel empty-library"><span className="empty-icon"><ImagePlus size={34} strokeWidth={1.2} /></span><h3>{t("Chưa có nguồn ảnh")}</h3><p>{t("Chọn thư mục hoặc thêm nguồn online.")}</p><button className="button primary" onClick={() => add()}><Plus size={17} />{t("Thêm nguồn ảnh")}</button></div>}{state.error && <div className="source-error"><AlertCircle size={18} /><p>{t(state.error)}</p></div>}{media.length > 0 && <><div className="subsection-heading"><div><h3>{t("Ảnh")}</h3><p>{media.length} {t("ảnh từ các nguồn đang bật")}</p></div><button className="text-button" onClick={() => setSlideshow(true)}>{t("Trình chiếu")} <ArrowUpRight size={15} /></button></div><div className="media-grid">{media.slice(0, 24).map(item => <div className="media-tile" key={item.id}><button className="media-preview-button" aria-label={t("Xem trước {0}",{0:item.name})} onClick={()=>setPhotoPreview({items:media,initialId:item.id})}><MediaImage item={item} loading="lazy" alt={item.name} /></button><span>{item.name}</span><button className="media-upscale" aria-label={t("Nâng 4K {0}", { 0: item.name })} disabled={busy} onClick={() => setUpscale([item.id])}>4K</button></div>)}</div></>}</>}
        {page === 'schedule' && <Schedule settings={state.settings} capabilities={capabilities} busy={busy} onSave={settings => save({ ...state, settings })} />}
        {page === 'settings' && <SettingsPage state={state} capabilities={capabilities} busy={busy} onSave={save} perform={perform} notify={notify} updates={updates} upscaleActive={upscaleController.active || upscaleController.busy} onStorageChanged={refresh} onManageSource={source => setDialog({ initial: source })} />}
        </>}
      </main>
    </div>
    {notice && <div className={`toast ${notice.error ? 'error' : ''}`} role={notice.error ? 'alert' : 'status'}>{notice.error ? <AlertCircle size={18} /> : <Check size={18} />}<span>{t(notice.text)}</span><button aria-label={t("Đóng thông báo")} className="icon-button" onClick={() => notify('')}><X size={16} /></button></div>}
    {dialog && <SourceDialog key={dialog.initial?.id || `new-${dialog.kind}`} savedSources={state.sources} onOpenSaved={source => setDialog({ initial: source })} initial={dialog.initial} initialKind={dialog.kind} chooseFolderOnOpen={dialog.choose} busy={busy} onClose={() => setDialog(null)} onSave={saveSource} />}
    {upscale && <UpscaleDialog platform={platform} sources={state.sources} media={media} initialIds={upscale} controller={upscaleController} onClose={() => setUpscale(null)} onPreview={item=>{setUpscale(null);setPhotoPreview({items:[item],initialId:item.id});}} />}
    <UpscaleProgress controller={upscaleController} onResults={() => setUpscale([])} />
    {transfer && <LanTransfer sources={state.sources} media={media} onClose={() => setTransfer(false)} onReceived={async () => { await refresh(); notify(t("Đã thêm ảnh nhận qua Wi-Fi.")); }} />}
    {deleting && <div className="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="delete-title"><div className="confirm-dialog"><div className="source-icon"><Trash2 size={24} /></div><h2 id="delete-title">{t("Ngắt nguồn ảnh này?")}</h2><p>{t("Wallta sẽ ngừng dùng “")}{deleting.name}{t("”. Ảnh gốc của bạn vẫn được giữ nguyên.")}</p><div className="dialog-footer"><button autoFocus className="button secondary" onClick={() => setDeleting(null)}>{t("Giữ lại")}</button><button disabled={busy} className="button danger" onClick={() => { void save({ ...state, sources: state.sources.filter(source => source.id !== deleting.id) }).then(saved => { if (saved) setDeleting(null); }); }}>{t("Ngắt nguồn ảnh")}</button></div></div></div>}
    {photoPreview && <PhotoPreview items={photoPreview.items} initialId={photoPreview.initialId} onClose={()=>setPhotoPreview(null)} />}
    {slideshow && media.length > 0 && <Slideshow media={media} settings={state.settings} onClose={() => setSlideshow(false)} />}
    {capabilities?.screensaver && page === 'overview' && <button className="floating-tv-button" onClick={() => void perform(async () => { if (isNative()) await callNative('open_screensaver_settings'); })}><Tv size={16} />{t("Thiết lập screensaver")}</button>}
  </div>;
}
