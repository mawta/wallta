import { useEffect, useRef, useState, type KeyboardEvent }  from 'react';
import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { t, useLocale } from '../lib/i18n';
import { callNative, errorMessage, imageUrl, isNative } from '../lib/native';
import type { MediaItem } from '../lib/types';
import { ImageComparison } from './ImageComparison';

export interface PhotoPreviewData {
  url: string; width: number; height: number;
  before?: { url: string; width: number; height: number };
  comparisonUnavailable?: boolean;
}
export function PhotoPreview({ items, initialId, onClose }: { items: MediaItem[]; initialId: string; onClose: () => void }) {
  useLocale();
  const dialog = useRef<HTMLDialogElement>(null);
  const [index, setIndex] = useState(Math.max(0, items.findIndex(item => item.id === initialId)));
  const [actualSize, setActualSize] = useState(false);
  const [loaded, setLoaded] = useState<{ key: string; data: PhotoPreviewData }>();
  const [failure, setFailure] = useState<{ key: string; text: string }>();
  const item = items[index];
  const key = item ? `${item.sourceId}:${item.id}` : '';
  const data = loaded?.key === key ? loaded.data : undefined;
  const error = failure?.key === key ? failure.text : '';
  useEffect(() => {
    const focused = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => { dialog.current?.close(); if (focused?.isConnected) focused.focus({ preventScroll: true }); };
  }, []);
  useEffect(() => {
    let disposed = false;
    setActualSize(false); setLoaded(undefined); setFailure(undefined);
    if (!item) return;
    const result = isNative() ? callNative<PhotoPreviewData>('get_photo_preview', { id: item.id, sourceId: item.sourceId }) : Promise.resolve({ url: item.url, width: 0, height: 0 });
    void result.then(value => {
      if (!value?.url || !Number.isFinite(value.width) || !Number.isFinite(value.height)) throw new Error(t('Không thể tải ảnh.'));
      if (!disposed) setLoaded({ key, data: value });
    }).catch(reason => { if (!disposed) setFailure({ key, text: errorMessage(reason) }); });
    return () => { disposed = true; };
  }, [key]);
  const move = (offset: number) => setIndex(previous => (previous + offset + items.length) % items.length);
  const handleKey = (event: KeyboardEvent<HTMLDialogElement>) => {
    if (event.defaultPrevented) return;
    if (event.key === 'Escape' || event.key === 'Backspace') { event.preventDefault(); event.stopPropagation(); onClose(); return; }
    if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)) return;
    const target = event.target as HTMLElement;
    const focus = (selector: string) => { const element = dialog.current?.querySelector<HTMLElement>(selector); element?.focus({preventScroll:true}); return Boolean(element); };
    const control = target.closest<HTMLElement>('[data-preview-control]')?.dataset.previewControl;
    const main = (fallback = '[data-preview-control="next"]:not(:disabled)') => focus('input[type="range"]') || focus('.actual-size') || focus(fallback) || focus('[data-preview-control="close"]');
    let handled = true;
    if (target.matches('input[type="range"]')) {
      if (event.key === 'ArrowUp') focus('[data-preview-control="zoom"]');
      else if (event.key === 'ArrowDown') { if (!(actualSize && focus('.actual-size'))) focus('[data-preview-control="next"]:not(:disabled)') || focus('[data-preview-control="close"]'); }
      else handled = false;
    } else if (target.matches('.actual-size')) {
      const horizontal = event.key === 'ArrowLeft' || event.key === 'ArrowRight';
      if (event.key === 'ArrowUp' && target.scrollTop <= 0) focus('[data-preview-control="zoom"]');
      else if (event.key === 'ArrowDown' && target.scrollTop + target.clientHeight >= target.scrollHeight) focus('[data-preview-control="next"]:not(:disabled)') || focus('[data-preview-control="close"]');
      else target.scrollBy({left:horizontal?(event.key==='ArrowLeft'?-80:80):0,top:horizontal?0:(event.key==='ArrowUp'?-80:80)});
    } else if (control === 'zoom' || control === 'close') {
      if (event.key === 'ArrowDown') main();
      else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') focus(control === 'close' ? '[data-preview-control="zoom"]' : '[data-preview-control="close"]');
      else handled = false;
    } else if (control === 'previous' || control === 'next') {
      if (event.key === 'ArrowUp') main('[data-preview-control="zoom"]');
      else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') focus(control === 'next' ? '[data-preview-control="previous"]:not(:disabled)' : '[data-preview-control="next"]:not(:disabled)');
      else handled = false;
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') { if (items.length > 1) move(event.key === 'ArrowLeft' ? -1 : 1); }
    else handled = false;
    if (handled) { event.preventDefault(); event.stopPropagation(); }
  };
  return <dialog ref={dialog} className="photo-preview-dialog" aria-label={t('Xem trước ảnh')} onCancel={event => { event.preventDefault(); onClose(); }} onKeyDown={handleKey}>
    <header className="photo-preview-heading"><div><strong>{item?.name}</strong>{data && data.width > 0 && <small>{data.width} × {data.height}</small>}</div><div className="photo-preview-actions">
      <button data-preview-control="zoom" className="button secondary" disabled={!data} aria-pressed={actualSize} onClick={() => setActualSize(value => !value)}>{actualSize ? t('Vừa cửa sổ') : '100%'}</button>
      <button data-preview-control="close" autoFocus className="icon-button" aria-label={t('Đóng')} onClick={onClose}><X size={21} /></button>
    </div></header>
    <div className="photo-preview-body">
      {error ? <p className="preview-message" role="alert">{t(error)}</p> : data ? <ImageComparison key={key} afterUrl={imageUrl(data.url)} beforeUrl={data.before ? imageUrl(data.before.url) : undefined} width={data.width || 640} height={data.height || 400} name={item?.name} actualSize={actualSize} onDimensions={(width, height) => {
        if (!data.width && width && height) setLoaded({ key, data: { ...data, width, height } });
      }} /> : <p className="preview-message" role="status">{t('Đang tải ảnh…')}</p>}
    </div>
    <footer className="photo-preview-footer"><button data-preview-control="previous" className="icon-button" aria-label={t('Ảnh trước')} disabled={items.length < 2} onClick={() => move(-1)}><ChevronLeft /></button><div><span>{index + 1} / {items.length}</span>{data?.comparisonUnavailable && <small>{t('Không tìm thấy ảnh gốc để so sánh.')}</small>}</div><button data-preview-control="next" className="icon-button" aria-label={t('Ảnh tiếp theo')} disabled={items.length < 2} onClick={() => move(1)}><ChevronRight /></button></footer>
  </dialog>;
}
