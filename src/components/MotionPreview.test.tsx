// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { MotionPreview } from './MotionPreview';
import { callNative } from '../lib/native';
import { localeStore } from '../lib/i18n';
vi.mock('../lib/native', () => ({ isNative: () => true, imageUrl: (value: string) => value, callNative: vi.fn(), errorMessage: (error: unknown) => String(error) }));
let host: HTMLDivElement, root: ReturnType<typeof createRoot>;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  localeStore.setPreference('en');
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {});
  vi.mocked(callNative).mockReset();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); });
it('plays the native validated cache URL muted and releases it on close', async () => {
  vi.mocked(callNative).mockResolvedValue('https://appassets.androidplatform.net/playback/abc.mp4');
  await act(async () => root.render(<MotionPreview controls item={{ id: 'a', sourceId: 'drive', name: 'clip.mp4', url: 'https://private-provider.example/original' }} />));
  expect(callNative).toHaveBeenCalledWith('get_playback_preview', { id: 'a' });
  const video = host.querySelector('video')!;
  expect(video.src).toContain('/playback/abc.mp4');
  expect(video.muted && video.loop && video.controls).toBe(true);
  expect(video.src).not.toContain('private-provider');
  await act(async () => root.render(<></>));
  expect(video.hasAttribute('src')).toBe(false);
});
it('ignores an older pending request and reports a codec error without stale media', async () => {
  let finish!: (value: string) => void;
  vi.mocked(callNative).mockImplementationOnce(() => new Promise(resolve => { finish = resolve as typeof finish; })).mockResolvedValue('https://appassets.androidplatform.net/playback/new.gif');
  const item = { id: 'old', sourceId: 's', name: 'old.mp4', url: '/old' };
  await act(async () => root.render(<MotionPreview item={item} />));
  await act(async () => root.render(<MotionPreview item={{ ...item, id: 'new', name: 'new.gif', url: '/new' }} />));
  await act(async () => finish('/stale.mp4'));
  expect(host.querySelector('video')).toBeNull();
  const image = host.querySelector('img')!;
  expect(image.src).toContain('new.gif');
  await act(async () => image.dispatchEvent(new Event('error')));
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Cannot play this file');
});
