import { useEffect, useState } from 'react';
import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { imageUrl } from '../lib/native';
import type { MediaItem, Settings } from '../lib/types';
import { selectNext } from '../lib/rules';

export function Slideshow({ media, settings, onClose }: { media: MediaItem[]; settings: Settings; onClose: () => void }) {
  const [current, setCurrent] = useState(media[0]);
  const [clock, setClock] = useState(new Date());
  useEffect(() => {
    const timer = setInterval(() => setCurrent(previous => selectNext(media, previous?.id || null, settings.order) || previous), (settings.slideshowSeconds || 30) * 1000);
    const clockTimer = setInterval(() => setClock(new Date()), 1000);
    return () => { clearInterval(timer); clearInterval(clockTimer); };
  }, [media, settings.order, settings.slideshowSeconds]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape' || event.key === 'Backspace') onClose();
      if (event.key === 'ArrowRight' || event.key === 'Enter') setCurrent(previous => selectNext(media, previous?.id || null, settings.order) || previous);
      if (event.key === 'ArrowLeft') setCurrent(previous => media[(media.findIndex(item => item.id === previous?.id) + media.length - 1) % media.length]);
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [media, settings.order, onClose]);
  return <div className="slideshow" role="dialog" aria-modal="true" aria-label="Trình chiếu ảnh">
    {settings.blurBackground && <img className="slideshow-blur" src={imageUrl(current?.url)} alt="" />}
    <img className="slideshow-image" style={{ objectFit: settings.fit === 'fill' ? 'cover' : settings.fit === 'fit' ? 'contain' : 'fill' }} src={imageUrl(current?.url)} alt={current?.name || ''} />
    {settings.showClock && <div className="slideshow-clock">{clock.toLocaleTimeString('vi-VN', { hour: '2-digit', minute: '2-digit' })}<small>{clock.toLocaleDateString('vi-VN', { weekday: 'long', day: 'numeric', month: 'long' })}</small></div>}
    <button className="slideshow-close icon-button" aria-label="Thoát trình chiếu" autoFocus onClick={onClose}><X /></button>
    <div className="slideshow-controls"><button className="icon-button" aria-label="Ảnh trước" onClick={() => setCurrent(previous => media[(media.findIndex(item => item.id === previous?.id) + media.length - 1) % media.length])}><ChevronLeft /></button><span>{current?.name}</span><button className="icon-button" aria-label="Ảnh tiếp theo" onClick={() => setCurrent(previous => selectNext(media, previous?.id || null, settings.order) || previous)}><ChevronRight /></button></div>
  </div>;
}
