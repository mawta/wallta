import { describe, expect, it, vi } from 'vitest';
import { createLocaleStore, translate } from './i18n';

const storage = (value: string | null = null) => {
  const data = new Map<string, string>(value ? [['wallta.language.v1', value]] : []);
  return { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, next: string) => { data.set(key, next); } };
};

describe('language preferences', () => {
  it('follows the primary device language and falls back to English', () => {
    expect(createLocaleStore({ languages: () => ['vi-VN'] }).getSnapshot().locale).toBe('vi');
    expect(createLocaleStore({ languages: () => ['en-US'] }).getSnapshot().locale).toBe('en');
    expect(createLocaleStore({ languages: () => ['fr-FR', 'vi'] }).getSnapshot().locale).toBe('en');
    expect(createLocaleStore({ languages: () => [] }).getSnapshot().locale).toBe('en');
  });
  it('remembers an override across store recreation and returns to device language', () => {
    const saved = storage();
    const first = createLocaleStore({ storage: saved, languages: () => ['en'] });
    first.setPreference('vi');
    const reopened = createLocaleStore({ storage: saved, languages: () => ['en'] });
    expect(reopened.getSnapshot()).toEqual({ preference: 'vi', locale: 'vi' });
    reopened.setPreference('system');
    expect(createLocaleStore({ storage: saved, languages: () => ['en'] }).getSnapshot()).toEqual({ preference: 'system', locale: 'en' });
  });
  it('ignores invalid persisted values and keeps switching usable when storage is blocked', () => {
    expect(createLocaleStore({ storage: storage('invalid'), languages: () => ['vi'] }).getSnapshot().locale).toBe('vi');
    const blocked = createLocaleStore({ storage: { getItem: () => { throw Error('blocked'); }, setItem: () => { throw Error('blocked'); } }, languages: () => ['en'] });
    expect(() => blocked.setPreference('vi')).not.toThrow();
    expect(blocked.getSnapshot().locale).toBe('vi');
  });
  it('notifies mounted UI once per change, with stable snapshots and device updates', () => {
    let device = ['en'];
    const store = createLocaleStore({ languages: () => device });
    const listener = vi.fn(); const off = store.subscribe(listener);
    const snapshot = store.getSnapshot();
    expect(store.getSnapshot()).toBe(snapshot);
    store.setPreference('system'); expect(listener).not.toHaveBeenCalled();
    device = ['vi']; store.refresh(); expect(listener).toHaveBeenCalledOnce();
    expect(store.getSnapshot().locale).toBe('vi');
    off(); store.setPreference('en'); expect(listener).toHaveBeenCalledOnce();
  });
});

describe('offline translations', () => {
  it('translates labels and interpolated text without changing user-provided values', () => {
    expect(translate('Nguồn ảnh', {}, 'en')).toBe('Sources');
    expect(translate('Tùy chọn {0}', { 0: 'Ảnh nhà tôi' }, 'en')).toBe('Options for Ảnh nhà tôi');
    expect(translate('Tùy chọn {0}', { 0: 'Ảnh nhà tôi' }, 'vi')).toBe('Tùy chọn Ảnh nhà tôi');
    expect(translate('Unknown provider: custom details', {}, 'en')).toBe('Unknown provider: custom details');
  });
  it('keeps repeated render-time translation stable', () => {
    expect(translate(translate('Nguồn ảnh', {}, 'en'), {}, 'vi')).toBe('Nguồn ảnh');
  });
  it('translates source-prefixed native errors while preserving names and unknown diagnostics', () => {
    expect(translate('Ảnh nhà tôi: Local image exceeds 32 MB.\nNAS: HTTP 503 custom', {}, 'vi')).toBe('Ảnh nhà tôi: Ảnh cục bộ vượt quá 32 MB.\nNAS: HTTP 503 custom');
    expect(translate('Không tải được cập nhật (HTTP 503). Hãy thử lại.', {}, 'en')).toBe('Could not download the update (HTTP 503). Try again.');
    expect(translate('constructor', {}, 'en')).toBe('constructor');
  });
  it('translates a diagnostic inside a saved-source message when the language changes', () => {
    expect(translate('Source saved. The selected folder has no available images yet.', {}, 'vi')).toBe('Đã lưu nguồn. Thư mục đã chọn chưa có ảnh khả dụng.');
    expect(translate('Đã lưu nguồn. Thư mục đã chọn chưa có ảnh khả dụng.', {}, 'en')).toBe('Source saved. The selected folder has no available images yet.');
    expect(translate('Đã lưu nguồn. Ảnh cục bộ vượt quá 32 MB.', {}, 'en')).toBe('Source saved. Local image exceeds 32 MB.');
  });
  it('does not mistake unknown diagnostics for generic UI unit templates', () => {
    expect(translate('Request failed after 30 seconds', {}, 'vi')).toBe('Request failed after 30 seconds');
    expect(translate('NAS: timeout after 30 seconds', {}, 'vi')).toBe('NAS: timeout after 30 seconds');
    expect(translate('Source saved. Request failed after 30 seconds', {}, 'vi')).toBe('Đã lưu nguồn. Request failed after 30 seconds');
  });
});
