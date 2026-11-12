import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { callNative, errorMessage, imageUrl, isNative } from '../lib/native';
import { mediaKind } from '../lib/media';
import { t } from '../lib/i18n';
import type { MediaItem } from '../lib/types';

export function MotionPreview({ item, className, style, controls = false }: { item: MediaItem; className?: string; style?: CSSProperties; controls?: boolean }) {
  const [loaded, setLoaded] = useState<{ key: string; url?: string; error?: string }>();
  const video = useRef<HTMLVideoElement>(null);
  const key = `${item.sourceId}:${item.id}:${item.url}`;
  const current = loaded?.key === key ? loaded : undefined;
  const kind = mediaKind(item);
  useEffect(() => {
    let disposed = false;
    const request = isNative() ? callNative<string>('get_playback_preview', { id: item.id }) : Promise.resolve(item.url);
    void request.then(url => { if (!url) throw new Error(t('Không thể tải ảnh.')); if (!disposed) setLoaded({ key, url: imageUrl(url) }); }).catch(error => { if (!disposed) setLoaded({ key, error: errorMessage(error) }); });
    return () => { disposed = true; };
  }, [key]);
  useEffect(() => {
    const element = video.current;
    if (!element) return;
    const visibility = () => { if (document.hidden) element.pause(); else void element.play().catch(() => {}); };
    document.addEventListener('visibilitychange', visibility);
    visibility();
    return () => { document.removeEventListener('visibilitychange', visibility); element.pause(); element.removeAttribute('src'); element.load(); };
  }, [current?.url]);
  const failure = () => setLoaded({ key, error: t('Không thể phát tệp này. Hãy thử video MP4 H.264 hoặc GIF.') });
  if (current?.error) return <p className="preview-message" role="alert">{t(current.error)}</p>;
  if (!current?.url) return <p className="preview-message" role="status">{t('Đang tải ảnh…')}</p>;
  return kind === 'video'
    ? <video ref={video} key={key} src={current.url} className={className} style={style} aria-label={item.name} muted autoPlay loop playsInline controls={controls} disablePictureInPicture onError={failure} />
    : <img key={key} src={current.url} className={className} style={style} alt={item.name} onError={failure} />;
}
