import { describe, expect, it } from 'vitest';
import { clampSettings, isWithinSchedule, parseManifest, selectNext, ShuffleCycle, validateSource } from './rules';
import { defaultSettings, type MediaItem } from './types';

describe('rotation rules', () => {
  const images: MediaItem[] = ['a', 'b', 'c'].map(id => ({ id, name: id, url: `https://images.example/${id}.jpg`, sourceId: 's' }));
  it('never immediately repeats a shuffled image when alternatives exist', () => {
    for (const current of ['a', 'b', 'c']) for (let i = 0; i < 30; i++) {
      const next = selectNext(images, current, 'shuffle');
      expect(next).not.toBeNull();
      expect(next?.id).not.toBe(current);
    }
  });
  it('wraps sequential rotation and supports an empty library', () => {
    expect(selectNext(images, 'c', 'sequential')?.id).toBe('a');
    expect(selectNext(images, 'b', 'sequential')?.id).toBe('c');
    expect(selectNext([], null, 'shuffle')).toBeNull();
  });
  it('uses every image once per shuffle cycle and reconciles removed images', () => {
    const cycle = new ShuffleCycle();
    let current: string | null = null;
    for (let round = 0; round < 4; round++) {
      const ids: string[] = [];
      for (let i = 0; i < images.length; i++) {
        const next: MediaItem = cycle.next(images, current)!;
        expect(next.id).not.toBe(current);
        ids.push(next.id); current = next.id;
      }
      expect(new Set(ids).size).toBe(images.length);
    }
    expect(cycle.next([images[0]], current)?.id).toBe('a');
    expect(cycle.next([], 'a')).toBeNull();
  });
  it('uses the previous selected day for an overnight window after midnight', () => {
    const settings = { ...defaultSettings, scheduleEnabled: true, days: [1], startTime: '22:00', endTime: '06:00' };
    expect(isWithinSchedule(settings, new Date(2026, 9, 26, 23))).toBe(true);
    expect(isWithinSchedule(settings, new Date(2026, 9, 27, 5))).toBe(true);
    expect(isWithinSchedule(settings, new Date(2026, 9, 27, 23))).toBe(false);
    expect(isWithinSchedule(settings, new Date(2026, 9, 27, 6))).toBe(false);
  });
  it('treats equal endpoints as a full selected day', () => {
    expect(isWithinSchedule({ ...defaultSettings, scheduleEnabled: true, startTime: '00:00', endTime: '00:00', days: [1] }, new Date(2026, 9, 26, 12))).toBe(true);
  });
  it('enforces platform intervals and finite cache limits', () => {
    const settings = clampSettings({ ...defaultSettings, intervalSeconds: 10, cacheLimitMb: NaN }, 900);
    expect(settings.intervalSeconds).toBe(900);
    expect(settings.cacheLimitMb).toBe(256);
  });
  it('keeps slideshow speed independent from wallpaper background limits', () => {
    expect(clampSettings({ ...defaultSettings, slideshowSeconds: 1 }, 900).slideshowSeconds).toBe(5);
    expect(clampSettings({ ...defaultSettings, slideshowSeconds: 30 }, 900).slideshowSeconds).toBe(30);
  });
});

describe('online source validation', () => {
  it('resolves relative URLs without accepting executable URLs', () => {
    const items = parseManifest({ images: ['a.jpg', { url: '../b.webp', name: 'B' }, 'javascript:alert(1)'] }, 'https://example.com/photos/list.json', 'source');
    expect(items.map(item => item.url)).toEqual(['https://example.com/photos/a.jpg', 'https://example.com/b.webp']);
    expect(items[1].name).toBe('B');
  });
  it('rejects malformed manifests rather than treating them as empty success', () => {
    expect(() => parseManifest({ message: 'permission denied' }, 'https://example.com', 's')).toThrow();
  });
  it('rejects insecure online sources and accepts SAF folder URIs', () => {
    expect(validateSource({ kind: 'manifest', location: 'javascript:alert(1)', name: 'test' })).toBeTruthy();
    expect(validateSource({ kind: 'local', location: 'content://com.android/tree/photos', name: 'ảnh' })).toBeNull();
  });
});
