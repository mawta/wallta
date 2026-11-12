// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import App from './App';
import { localeStore } from './lib/i18n';
import { callNative } from './lib/native';
import { emptyState } from './lib/types';

vi.mock('./lib/native', () => ({ isNative: () => true, isAndroid: () => false, isDesktop: () => true, imageUrl: (url: string) => url, callNative: vi.fn(), registerPreviewFiles: vi.fn(), errorMessage: (error: unknown) => String(error) }));
vi.mock('./lib/useUpdates', () => ({ useUpdates: () => ({ status: { phase: 'idle', currentVersion: '0.1.8', downloadedBytes: 0 }, pending: false }) }));
let host: HTMLDivElement, root: ReturnType<typeof createRoot>;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  localeStore.setPreference('en');
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute('open'); };
  vi.mocked(callNative).mockReset().mockImplementation(async command => {
    if (command === 'get_state') return { ...structuredClone(emptyState), sources: [{ id: 'user', kind: 'local', name: 'Ảnh nhà tôi', location: 'D:/Ảnh nhà tôi', enabled: true, recursive: false }] } as never;
    if (command === 'get_capabilities') return { platform: 'windows', wallpaper: true, screensaver: false, minIntervalSeconds: 60, notes: [] } as never;
    if (command === 'list_media') return [] as never;
    if (command === 'upscale_storage') return { cacheBytes: 0, collections: [] } as never;
    return undefined as never;
  });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
const click = async (label: string) => {
  const button = [...host.querySelectorAll('button')].find(element => element.getAttribute('aria-label') === label || element.textContent?.trim() === label);
  expect(button, label).toBeDefined(); await act(async () => button!.click());
};

it('switches the whole app instantly, preserves user names and remembers language on remount', async () => {
  await act(async () => root.render(<App />));
  expect(host.querySelector('nav')?.textContent).toContain('Overview');
  expect(host.textContent).toContain('Ảnh nhà tôi');
  expect(document.documentElement.lang).toBe('en');
  await click('Settings');
  const selector = host.querySelector<HTMLSelectElement>('select[aria-label="Language"]')!;
  expect(selector).not.toBeNull();
  await act(async () => { selector.value = 'vi'; selector.dispatchEvent(new Event('change', { bubbles: true })); });
  expect(host.querySelector('nav')?.textContent).toContain('Tổng quan');
  expect(host.querySelector('select[aria-label="Ngôn ngữ"]')).not.toBeNull();
  expect(document.documentElement.lang).toBe('vi');
  expect(localStorage.getItem('wallta.language.v1')).toBe('vi');
  expect(callNative).not.toHaveBeenCalledWith('save_state', expect.anything());
  await act(async () => root.unmount()); root = createRoot(host);
  await act(async () => root.render(<App />));
  expect(host.querySelector('nav')?.textContent).toContain('Tổng quan');
  expect(host.textContent).toContain('Ảnh nhà tôi');
});

it('follows device changes in system mode while leaving an open form intact', async () => {
  const language = vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['en-US']);
  await act(async () => { localeStore.setPreference('system'); root.render(<App />); });
  await click('Add source');
  const form = host.querySelector('dialog')!;
  const name = form.querySelector<HTMLInputElement>('input')!;
  const originalValue = name.value;
  await act(async () => { language.mockReturnValue(['vi-VN']); window.dispatchEvent(new Event('languagechange')); });
  expect(document.documentElement.lang).toBe('vi');
  expect(host.querySelector('dialog')).toBe(form);
  expect(name.value).toBe(originalValue);
  expect(form.textContent).toContain('Lưu');
  language.mockRestore();
});

it('opens a library photo in a full-sized native preview without starting the slideshow', async () => {
  const previous=vi.mocked(callNative).getMockImplementation()!;
  vi.mocked(callNative).mockImplementation(async (command,args)=>{
    if(command==='list_media')return [{id:'photo',sourceId:'user',name:'Landscape',url:'/native/photo.jpg'}] as never;
    if(command==='get_preview')return '/cache/thumb.jpg' as never;
    if(command==='get_photo_preview')return {url:'/cache/full.jpg',width:3840,height:2160} as never;
    return previous(command,args);
  });
  await act(async()=>root.render(<App/>));await click('Sources');
  await click('Preview Landscape');
  expect(callNative).toHaveBeenCalledWith('get_photo_preview',{id:'photo',sourceId:'user'});
  expect(host.querySelector('dialog[aria-label="Photo preview"]')).not.toBeNull();
  expect(host.querySelector('img[src="/cache/full.jpg"]')).not.toBeNull();
  expect(host.querySelector('.slideshow')).toBeNull();
});

it('remembers an Android lock screen source and uses the dedicated apply action', async () => {
  const previous = vi.mocked(callNative).getMockImplementation()!;
  let stored = await previous('get_state') as unknown as typeof emptyState;
  vi.mocked(callNative).mockImplementation(async (command, args) => {
    if (command === 'get_capabilities') return { platform: 'android', wallpaper: true, lockWallpaper: true, screensaver: true, minIntervalSeconds: 900, notes: [] } as never;
    if (command === 'get_state' || command === 'next_lock_wallpaper') return structuredClone(stored) as never;
    if (command === 'save_state') {
      stored = structuredClone(args!.state as typeof emptyState);
      return structuredClone(stored) as never;
    }
    return previous(command, args);
  });
  const selector = () => host.querySelector<HTMLSelectElement>('select[aria-label="Lock screen source"]')!;
  await act(async () => root.render(<App />));
  await click('Lock screen');
  expect(selector()?.options[1]?.textContent).toBe('Ảnh nhà tôi');
  expect(selector().value).toBe('');
  await act(async () => {
    selector().value = 'user';
    selector().dispatchEvent(new Event('change', { bubbles: true }));
  });
  await act(async () => root.unmount());
  root = createRoot(host);
  await act(async () => root.render(<App />));
  await click('Lock screen');
  expect(selector().value).toBe('user');
  await click('Change lock screen now');
  expect(callNative).toHaveBeenCalledWith('next_lock_wallpaper');
  expect(callNative).not.toHaveBeenCalledWith('next_wallpaper');
  expect(host.querySelector('.toast[role="status"]')?.textContent).toContain('Lock screen wallpaper updated.');
});

it('configures Windows lock rotation independently from Home and shows firmware limitations on TV', async () => {
  const previous = vi.mocked(callNative).getMockImplementation()!;
  let stored = structuredClone(emptyState);
  stored.sources = [
    { id: 'home', name: 'Home source', kind: 'local', location: 'home', enabled: true, recursive: false },
    { id: 'lock', name: 'Lock source', kind: 'local', location: 'lock', enabled: true, recursive: false },
  ];
  stored.settings.homeSourceId = 'home';
  let platform = 'windows';
  vi.mocked(callNative).mockImplementation(async (command, args) => {
    if (command === 'get_capabilities') return { platform, wallpaper: true, lockWallpaper: platform === 'windows', screensaver: false, minIntervalSeconds: 10, notes: [] } as never;
    if (command === 'get_state') return structuredClone(stored) as never;
    if (command === 'save_state') { stored = structuredClone(args!.state as typeof stored); return structuredClone(stored) as never; }
    if (command === 'next_lock_wallpaper') return { ...stored, lockError: 'Windows rejected the lock screen image.' } as never;
    return previous(command, args);
  });
  await act(async () => root.render(<App />));
  await click('Lock screen');
  const selector = host.querySelector<HTMLSelectElement>('select[aria-label="Lock screen source"]')!;
  expect(selector).not.toBeNull();
  await act(async () => { selector.value = 'lock'; selector.dispatchEvent(new Event('change', { bubbles: true })); });
  await click('Automatically change lock screen');
  expect(stored.settings.lockRunning).toBe(true);
  expect(stored.running).toBe(false);
  expect(stored.settings.homeSourceId).toBe('home');
  expect(stored.settings.lockSourceId).toBe('lock');
  await click('Change lock screen now');
  expect(host.querySelector('.toast[role="alert"]')?.textContent).toContain('Windows rejected');
  platform = 'android-tv';
  await act(async () => root.unmount()); root = createRoot(host);
  await act(async () => root.render(<App />));
  await click('Lock screen');
  expect(host.textContent).toContain('This device does not provide a lock screen wallpaper.');
  expect(host.querySelector('select[aria-label="Lock screen source"]')).toBeNull();
});


it('keeps the Home source separate and opens Android live wallpaper through the system', async () => {
  const previous = vi.mocked(callNative).getMockImplementation()!;
  let stored = structuredClone(emptyState);
  stored.sources = [{ id: 'home', name: 'Home collection', kind: 'local', location: 'home', enabled: true, recursive: false }];
  stored.settings.lockSourceId = 'separate-lock';
  stored.settings.lockRunning = true;
  vi.mocked(callNative).mockImplementation(async (command, args) => {
    if (command === 'get_capabilities') return { platform: 'android', wallpaper: true, lockWallpaper: true, liveWallpaper: true, screensaver: false, minIntervalSeconds: 900, notes: [] } as never;
    if (command === 'get_state') return structuredClone(stored) as never;
    if (command === 'save_state') { stored = structuredClone(args!.state as typeof stored); return structuredClone(stored) as never; }
    return previous(command, args);
  });
  await act(async () => root.render(<App />));
  const selector = host.querySelector<HTMLSelectElement>('select[aria-label="Wallpaper source"]')!;
  await act(async () => { selector.value = 'home'; selector.dispatchEvent(new Event('change', { bubbles: true })); });
  expect(stored.settings.homeSourceId).toBe('home');
  expect(stored.settings.lockSourceId).toBe('separate-lock');
  expect(stored.settings.lockRunning).toBe(true);
  await click('Settings');
  await click('Enable animated wallpaper');
  expect(stored.settings.animatedWallpaper).toBe(true);
  const row = [...host.querySelectorAll('.setting-row')].find(element => element.textContent?.includes('Android live wallpaper'))!;
  expect(row).toBeDefined();
  await act(async () => row.querySelector<HTMLButtonElement>('button')!.click());
  expect(callNative).toHaveBeenCalledWith('open_live_wallpaper_settings');
});


it('allows TV screensaver animation without advertising a live Home API', async () => {
  const previous = vi.mocked(callNative).getMockImplementation()!;
  let stored = structuredClone(emptyState);
  vi.mocked(callNative).mockImplementation(async (command, args) => {
    if (command === 'get_capabilities') return { platform: 'android-tv', wallpaper: false, liveWallpaper: false, lockWallpaper: false, screensaver: true, minIntervalSeconds: 900, notes: [] } as never;
    if (command === 'get_state') return structuredClone(stored) as never;
    if (command === 'save_state') { stored = structuredClone(args!.state as typeof stored); return structuredClone(stored) as never; }
    return previous(command, args);
  });
  await act(async () => root.render(<App />)); await click('Settings');
  await click('Enable animated wallpaper');
  expect(stored.settings.animatedWallpaper).toBe(true);
  expect(host.textContent).not.toContain('Android live wallpaper');
});
