import { useEffect, useRef, useState, type PointerEvent } from 'react';
import { MoveHorizontal } from 'lucide-react';
import { t } from '../lib/i18n';

interface Props {
  afterUrl: string; beforeUrl?: string; width: number; height: number;
  actualSize?: boolean; name?: string; onDimensions?: (width: number, height: number) => void;
}
export function ImageComparison({ afterUrl, beforeUrl, width, height, actualSize = false, name, onDimensions }: Props) {
  const viewport = useRef<HTMLDivElement>(null);
  const frame = useRef<HTMLDivElement>(null);
  const range = useRef<HTMLInputElement>(null);
  const dragging = useRef(false);
  const [position, setPosition] = useState(50);
  const [bounds, setBounds] = useState({ width: 640, height: 400 });
  const [afterFailed, setAfterFailed] = useState(false);
  const [beforeFailed, setBeforeFailed] = useState(false);
  const compare = Boolean(beforeUrl) && !beforeFailed;
  useEffect(() => {
    setPosition(50); setAfterFailed(false); setBeforeFailed(false); dragging.current = false;
    viewport.current?.scrollTo?.(0, 0);
  }, [afterUrl, beforeUrl]);
  useEffect(() => {
    const measure = () => {
      const rect = viewport.current?.getBoundingClientRect();
      if (rect?.width && rect.height) setBounds({ width: rect.width, height: rect.height });
    };
    measure();
    const observer = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(measure);
    if (viewport.current) observer?.observe(viewport.current);
    window.addEventListener('resize', measure);
    return () => { observer?.disconnect(); window.removeEventListener('resize', measure); };
  }, []);
  const scale = actualSize ? 1 : Math.min(bounds.width / Math.max(1, width), bounds.height / Math.max(1, height), 1);
  const update = (event: PointerEvent<HTMLElement>) => {
    const rect = frame.current?.getBoundingClientRect();
    if (rect?.width) setPosition(Math.round(Math.max(0, Math.min(100, (event.clientX - rect.left) * 100 / rect.width))));
  };
  const start = (event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0 || !compare) return;
    event.preventDefault(); dragging.current = true;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    range.current?.focus({ preventScroll: true }); update(event);
  };
  return <div className={`comparison-viewport ${actualSize ? 'actual-size' : ''}`} ref={viewport} tabIndex={actualSize ? 0 : undefined} aria-label={t('Vùng xem ảnh')}>
    {afterFailed ? <p className="preview-message" role="alert">{t('Không thể tải ảnh.')}</p> : <div className="comparison-scroll-space">
      <div className="comparison-frame" ref={frame} style={{ width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) }}>
        <img key={afterUrl} className="comparison-image" src={afterUrl} alt={compare ? t('Sau') : name || t('Ảnh')} draggable={false} onLoad={event => onDimensions?.(event.currentTarget.naturalWidth, event.currentTarget.naturalHeight)} onError={() => setAfterFailed(true)} />
        {compare && <>
          <img key={beforeUrl} className="comparison-image comparison-before" src={beforeUrl} alt={t('Trước')} draggable={false} style={{ clipPath: `inset(0 ${100 - position}% 0 0)` }} onError={() => setBeforeFailed(true)} />
          <span className="comparison-label before-label">{t('Trước')}</span><span className="comparison-label after-label">{t('Sau')}</span>
          <div className="comparison-divider" style={{ left: `${position}%` }} aria-hidden="true"><span><MoveHorizontal size={18} /></span></div>
          <input ref={range} className="comparison-range" type="range" min="0" max="100" step="1" value={position} aria-label={t('So sánh Trước / Sau')} aria-valuetext={`${position}% ${t('Trước')}`} style={actualSize ? { left: `clamp(20px, ${position}%, calc(100% - 20px))` } : undefined}
            onChange={event => { if (!dragging.current) setPosition(Number(event.target.value)); }} onPointerDown={start} onPointerMove={event => { if (dragging.current) update(event); }} onPointerUp={() => { dragging.current = false; }} onPointerCancel={() => { dragging.current = false; }}
            onKeyDown={event => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) { event.preventDefault(); event.stopPropagation(); setPosition(previous => event.key === 'Home' ? 0 : event.key === 'End' ? 100 : Math.max(0, Math.min(100, previous + (event.key === 'ArrowLeft' || event.key === 'ArrowDown' ? -1 : 1)))); } }} />
        </>}
      </div>
    </div>}
    {beforeFailed && <p className="preview-original-missing">{t('Không tìm thấy ảnh gốc để so sánh.')}</p>}
  </div>;
}
