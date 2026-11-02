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
