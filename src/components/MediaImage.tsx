import { useEffect, useState, type ImgHTMLAttributes } from 'react';
import { callNative, imageUrl, isNative } from '../lib/native';
import type { MediaItem } from '../lib/types';

export function MediaImage({ item, ...props }: { item: MediaItem | null | undefined } & Omit<ImgHTMLAttributes<HTMLImageElement>, 'src'>) {
  const [src, setSrc] = useState('./landscape.svg');
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setFailed(false);
    if (!item) { setSrc('./landscape.svg'); return; }
    if (!isNative()) { setSrc(imageUrl(item.url)); return; }
    setSrc('./landscape.svg');
    callNative<string>('get_preview', { id: item.id })
      .then(url => { if (!cancelled) setSrc(imageUrl(url)); })
      .catch(() => { if (!cancelled) setFailed(true); });
    return () => { cancelled = true; };
  }, [item?.id, item?.url]);
  return <img {...props} src={failed ? './landscape.svg' : src} onError={() => setFailed(true)} />;
}
