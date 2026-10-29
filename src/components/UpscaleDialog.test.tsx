// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { UpscaleDialog } from './UpscaleDialog';
import { callNative } from '../lib/native';
vi.mock('../lib/native', () => ({ isNative: () => true, callNative: vi.fn(), imageUrl: (s: string) => s, errorMessage: (e: unknown) => String(e) }));
let host: HTMLDivElement; let root: ReturnType<typeof createRoot>;
const source = { id: '4k', name: 'Ảnh 4K', kind: 'local' as const, location: 'managed', enabled: true, recursive: false };
const ready = { jobId: 'job', phase: 'ready', mode: 'ai', completed: 1, total: 1, results: [{ id: 'img', name: 'Beach', width: 3840, height: 2160, originalPreview: '/before.jpg', resultPreview: '/after.jpg' }] };
const onSaved = vi.fn().mockResolvedValue(true); const onClose = vi.fn();
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  HTMLDialogElement.prototype.showModal = function () {}; HTMLDialogElement.prototype.close = function () {};
  vi.mocked(callNative).mockReset(); onSaved.mockClear(); onClose.mockClear();
  vi.mocked(callNative).mockImplementation(async command => (command === 'upscale_status' ? { phase: 'idle', jobId: null, results: [] } : command === 'upscale_start' ? ready : command === 'upscale_save' ? source : { ...ready, phase: 'cancelled' }) as never);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
const click = async (text: string) => { const button = Array.from(host.querySelectorAll('button')).find(b => b.textContent?.includes(text) || b.getAttribute('aria-label') === text); expect(button).toBeDefined(); await act(async () => button!.click()); };
it('processes selected IDs in AI mode and saves the native committed source with optional apply', async () => {
  await act(async () => root.render(<UpscaleDialog platform="windows" media={[{ id: 'img', name: 'Beach', sourceId: 's', url: '/original' }]} initialIds={['img']} onSaved={onSaved} onClose={onClose} />));
  const select = host.querySelector('select')!;
  await act(async () => { select.value = 'ai'; select.dispatchEvent(new Event('change', { bubbles: true })); });
  await click('Tạo ảnh 4K');
  expect(callNative).toHaveBeenCalledWith('upscale_start', { ids: ['img'], mode: 'ai' });
  expect(host.querySelector('img[alt="Trước"]')?.getAttribute('src')).toBe('/before.jpg');
  expect(host.querySelector('img[alt="Sau"]')?.getAttribute('src')).toBe('/after.jpg');
  await click('Lưu ảnh 4K');
  expect(callNative).toHaveBeenCalledWith('upscale_save', { jobId: 'job' });
  expect(onSaved).toHaveBeenCalledWith(source, false);
  expect(onClose).toHaveBeenCalledOnce();
});
it('keeps AI unavailable on Android TV', async () => {
  await act(async () => root.render(<UpscaleDialog platform="android-tv" media={[]} onSaved={onSaved} onClose={onClose} />));
  expect(host.querySelector('option[value="ai"]')?.hasAttribute('disabled')).toBe(true);
  expect(host.textContent).toContain('AI chỉ có trên Windows');
});
it('cancels the active native job when closing without saving', async () => {
  vi.mocked(callNative).mockImplementation(async command => (command === 'upscale_status' ? { ...ready, phase: 'processing', results: [] } : { ...ready, phase: 'cancelled', results: [] }) as never);
  await act(async () => root.render(<UpscaleDialog platform="windows" media={[]} onSaved={onSaved} onClose={onClose} />));
  await click('Đóng');
  expect(callNative).toHaveBeenCalledWith('upscale_cancel', { jobId: 'job' });
  expect(onClose).toHaveBeenCalledOnce();
});
it('cleans up a native status response that arrives after unmount', async () => {
  let resolveStatus!: (value: unknown) => void;
  vi.mocked(callNative).mockImplementation(command => command === 'upscale_status' ? new Promise(resolve => { resolveStatus = resolve; }) as never : Promise.resolve({ ...ready, phase: 'cancelled' }) as never);
  await act(async () => root.render(<UpscaleDialog platform="windows" media={[]} onSaved={onSaved} onClose={onClose} />));
  await act(async () => root.render(null));
  await act(async () => resolveStatus(ready));
  expect(callNative).toHaveBeenCalledWith('upscale_cancel', { jobId: 'job' });
});
it('polls the native job then passes the apply choice when saving', async () => {
  vi.useFakeTimers();
  try {
    vi.mocked(callNative).mockImplementation(async command => (command === 'upscale_status' ? (vi.mocked(callNative).mock.calls.filter(([name]) => name === 'upscale_status').length === 1 ? { ...ready, phase: 'processing', completed: 0, results: [] } : ready) : source) as never);
    await act(async () => root.render(<UpscaleDialog platform="windows" media={[]} onSaved={onSaved} onClose={onClose} />));
    expect(host.querySelector('progress')).not.toBeNull();
    await act(async () => vi.advanceTimersByTimeAsync(750));
    expect(host.querySelector('img[alt="Sau"]')).not.toBeNull();
    await act(async () => (host.querySelector('input[type="checkbox"]') as HTMLInputElement).click());
    await click('Lưu ảnh 4K');
    expect(onSaved).toHaveBeenLastCalledWith(source, true);
  } finally { vi.useRealTimers(); }
});
it('can start another image when the native service restores a previously saved ready job', async () => {
  vi.mocked(callNative).mockImplementation(async command => (command === 'upscale_start' ? { ...ready, jobId: 'second' } : ready) as never);
  await act(async () => root.render(<UpscaleDialog platform="windows" media={[{ id: 'other', name: 'Other', sourceId: 's', url: '/other' }]} initialIds={['other']} onSaved={onSaved} onClose={onClose} />));
  await click('Tạo bộ ảnh khác');
  await click('Tạo ảnh 4K');
  expect(callNative).toHaveBeenCalledWith('upscale_start', { ids: ['other'], mode: 'fast' });
});
it('recovers polling after consecutive identical transient failures', async () => {
  vi.useFakeTimers();
  try {
    let calls = 0;
    vi.mocked(callNative).mockImplementation(async command => {
      if (command !== 'upscale_status') return ready as never;
      calls++;
      if (calls === 2 || calls === 3) throw new Error('Temporary disconnect');
      return (calls === 1 ? { ...ready, phase: 'processing', completed: 0, results: [] } : ready) as never;
    });
    await act(async () => root.render(<UpscaleDialog platform="windows" media={[]} onSaved={onSaved} onClose={onClose} />));
    for (let index = 0; index < 3; index++) await act(async () => vi.advanceTimersByTimeAsync(750));
    expect(calls).toBe(4);
    expect(host.querySelector('img[alt="Sau"]')).not.toBeNull();
  } finally { vi.useRealTimers(); }
});
