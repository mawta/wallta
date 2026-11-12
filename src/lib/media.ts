import type { MediaItem } from './types';

export function mediaKind(item?: MediaItem | null): 'image' | 'video' | 'gif' {
  if (item?.kind) return item.kind;
  const candidates = [item?.name || '', item?.url || ''];
  if (candidates.some(name => /\.(mp4|webm)(?:[?#]|$)/i.test(name))) return 'video';
  if (candidates.some(name => /\.gif(?:[?#]|$)/i.test(name))) return 'gif';
  return 'image';
}
