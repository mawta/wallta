import { t } from './i18n';
import { defaultSettings, type MediaItem, type Settings, type Source } from './types';

export function selectNext(items: MediaItem[], current: string | null, order: Settings['order']): MediaItem | null {
  if (!items.length) return null;
  if (order === 'sequential') return items[(items.findIndex(item => item.id === current) + 1) % items.length];
  const candidates = items.length > 1 ? items.filter(item => item.id !== current) : items;
  return candidates[Math.floor(Math.random() * candidates.length)] ?? null;
}

export class ShuffleCycle {
  private catalog = '';
  private remaining: string[] = [];

  next(items: MediaItem[], current: string | null): MediaItem | null {
    if (!items.length) { this.catalog = ''; this.remaining = []; return null; }
    const ids = [...new Set(items.map(item => item.id))];
    const catalog = [...ids].sort().join('\n');
    const changed = catalog !== this.catalog;
    if (changed || !this.remaining.length) {
      this.catalog = catalog;
      this.remaining = changed && current && ids.length > 1 ? ids.filter(id => id !== current) : [...ids];
      for (let i = this.remaining.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [this.remaining[i], this.remaining[j]] = [this.remaining[j], this.remaining[i]];
      }
      if (this.remaining.length > 1 && this.remaining[this.remaining.length - 1] === current) {
        [this.remaining[0], this.remaining[this.remaining.length - 1]] = [this.remaining[this.remaining.length - 1], this.remaining[0]];
      }
    }
    const id = this.remaining.pop();
    return items.find(item => item.id === id) || null;
  }
}

export function isWithinSchedule(settings: Settings, date = new Date()): boolean {
  if (!settings.scheduleEnabled) return true;
  const toMinutes = (value: string) => Number(value.slice(0, 2)) * 60 + Number(value.slice(3));
  const start = toMinutes(settings.startTime), end = toMinutes(settings.endTime);
  const now = date.getHours() * 60 + date.getMinutes();
  if (start === end) return settings.days.includes(date.getDay());
  if (start < end) return settings.days.includes(date.getDay()) && now >= start && now < end;
  if (now >= start) return settings.days.includes(date.getDay());
  return now < end && settings.days.includes((date.getDay() + 6) % 7);
}

export function clampSettings(settings: Settings, minimum = 1): Settings {
  const finite = (n: number, fallback: number, min: number, max: number) => Number.isFinite(n) ? Math.max(min, Math.min(max, Math.round(n))) : fallback;
  const time = (s: string, fallback: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(s) ? s : fallback;
  return {
    ...defaultSettings, ...settings,
    intervalSeconds: finite(settings.intervalSeconds, Math.max(900, minimum), minimum, 604800),
    slideshowSeconds: finite(settings.slideshowSeconds, 30, 5, 3600),
    cacheLimitMb: finite(settings.cacheLimitMb, 256, 32, 4096),
    days: [...new Set((Array.isArray(settings.days) ? settings.days : defaultSettings.days).filter(day => Number.isInteger(day) && day >= 0 && day <= 6))],
    startTime: time(settings.startTime, '07:00'), endTime: time(settings.endTime, '23:00'),
    order: settings.order === 'sequential' ? 'sequential' : 'shuffle',
    fit: ['fill', 'fit', 'stretch'].includes(settings.fit) ? settings.fit : 'fill',
  };
}

export function parseManifest(data: unknown, base: string, sourceId: string): MediaItem[] {
  const entries: unknown = Array.isArray(data) ? data : data && typeof data === 'object' && 'images' in data ? data.images : undefined;
  if (!Array.isArray(entries)) throw new Error(t("Danh sách ảnh cần là mảng URL hoặc đối tượng có thuộc tính images."));
  const seen = new Set<string>();
  const items: MediaItem[] = [];
  for (const entry of entries.slice(0, 10000)) {
    const value = typeof entry === 'string' ? { url: entry } : entry;
    if (!value || typeof value !== 'object' || !('url' in value) || typeof value.url !== 'string') continue;
    try {
      const url = new URL(value.url, base);
      if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password || seen.has(url.href)) continue;
      seen.add(url.href);
      const name = 'name' in value && typeof value.name === 'string' ? value.name : decodeURIComponent(url.pathname.split('/').pop() || t("Ảnh"));
      items.push({ id: `${sourceId}:${url.href}`, name, url: url.href, sourceId });
    } catch { /* An invalid entry must not prevent valid images from being listed. */ }
  }
  return items;
}

export function validateSource(source: Pick<Source, 'kind' | 'location' | 'name'> & Partial<Pick<Source, 'enabled' | 'google'>>): string | null {
  if (!source.name.trim()) return t("Đặt tên cho nguồn ảnh này.");
  if (source.kind === 'drive' && !source.location.trim() && source.google?.connected && source.enabled === false) return null;
  if (!source.location.trim()) return source.kind === 'local' ? t("Chọn một thư mục ảnh.") : t("Nhập địa chỉ nguồn ảnh.");
  if (source.kind === 'local' || source.kind === 'drive' || source.kind === 'photos') return null;
  try {
    const url = new URL(source.location);
    if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password) throw new Error();
    return null;
  } catch { return t("Dùng địa chỉ HTTP/HTTPS hợp lệ, không đặt mật khẩu trong URL."); }
}
