import { t, localeTag } from '../lib/i18n';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { MediaImage } from './MediaImage';
import type { MediaItem, Settings } from '../lib/types';
import { selectNext, ShuffleCycle } from '../lib/rules';

export function Slideshow({ media, settings, onClose }: { media: MediaItem[]; settings: Settings; onClose: () => void }) {
  const [current, setCurrent] = useState<MediaItem | undefined>(media[0]);
  const [clock, setClock] = useState(new Date());
  const cycle = useRef(new ShuffleCycle());
  const currentRef = useRef(current);
  const show = useCallback((item: MediaItem | undefined) => { currentRef.current = item; setCurrent(item); }, []);
  const next = useCallback(() => {
    const previous = currentRef.current;
    show((settings.order === 'shuffle' ? cycle.current.next(media, previous?.id || null) : selectNext(media, previous?.id || null, 'sequential')) || previous);
  }, [media, settings.order, show]);
  const previous = useCallback(() => show(media[(media.findIndex(item => item.id === currentRef.current?.id) + media.length - 1) % media.length]), [media, show]);
  useEffect(() => {
    const timer = setInterval(next, (settings.slideshowSeconds || 30) * 1000);
    const clockTimer = setInterval(() => setClock(new Date()), 1000);
    return () => { clearInterval(timer); clearInterval(clockTimer); };
  }, [next, settings.slideshowSeconds]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape' || event.key === 'Backspace') { event.preventDefault(); onClose(); }
      if (event.key === 'ArrowRight') { event.preventDefault(); next(); }
      if (event.key === 'ArrowLeft') { event.preventDefault(); previous(); }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [next, previous, onClose]);
  return <div className="slideshow" role="dialog" aria-modal="true" aria-label={t("Trình chiếu ảnh")}>
    {settings.blurBackground && <MediaImage item={current} className="slideshow-blur" alt="" />}
    <MediaImage item={current} className="slideshow-image" style={{ objectFit: settings.fit === 'fill' ? 'cover' : settings.fit === 'fit' ? 'contain' : 'fill' }} alt={current?.name || ''} />
    {settings.showClock && <div className="slideshow-clock">{clock.toLocaleTimeString(localeTag(), { hour: '2-digit', minute: '2-digit' })}<small>{clock.toLocaleDateString(localeTag(), { weekday: 'long', day: 'numeric', month: 'long' })}</small></div>}
    <button className="slideshow-close icon-button" aria-label={t("Thoát trình chiếu")} autoFocus onClick={onClose}><X /></button>
    <div className="slideshow-controls"><button className="icon-button" aria-label={t("Ảnh trước")} onClick={previous}><ChevronLeft /></button><span>{current?.name}</span><button className="icon-button" aria-label={t("Ảnh tiếp theo")} onClick={next}><ChevronRight /></button></div>
  </div>;
}
