// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { LanTransfer } from './LanTransfer';
import { callNative } from '../lib/native';
import type { MediaItem, Source } from '../lib/types';

vi.mock('../lib/native', () => ({ callNative: vi.fn(), isNative: () => true, errorMessage: (error: unknown) => String(error) }));
vi.mock('qrcode', () => ({ default: { toDataURL: vi.fn().mockResolvedValue('data:image/png;base64,QR') } }));
const local: Source = { id: 'local', name: 'Local', kind: 'local', location: '/photos', enabled: true, recursive: false };
const online: Source = { ...local, id: 'cloud', kind: 'photos' };
const media: MediaItem[] = [{ id: 'one', sourceId: 'local', name: 'a.jpg', url: '/a.jpg' }, { id: 'private', sourceId: 'cloud', name: 'cloud.jpg', url: 'https://private.example/a' }];
const active = { active: true, urls: ['http://192.168.1.10:1234/' + 'a'.repeat(64) + '/'], expiresAt: Date.now() + 600000, receivedCount: 2, receivedBytes: 2000, outgoing: [] };
let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute('open'); };
  vi.mocked(callNative).mockReset();
  vi.mocked(callNative).mockResolvedValue(active as never);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
const button = (name: string) => Array.from(host.querySelectorAll('button')).find(item => item.textContent === name)!;

it('exports only explicitly selected local media and starts nothing on mount', async () => {
  await act(async () => root.render(<LanTransfer sources={[local, online]} media={media} onClose={vi.fn()} onReceived={vi.fn()} />));
  expect(callNative).not.toHaveBeenCalled();
  expect(host.textContent).not.toContain('cloud.jpg');
  await act(async () => host.querySelector<HTMLInputElement>('input[type=checkbox]')!.click());
  await act(async () => button('Bật gửi / nhận').click());
  expect(callNative).toHaveBeenCalledWith('lan_start', { mediaIds: ['one'] });
  expect(host.textContent).toContain('2 ảnh');
});

it('closes the native session before closing the dialog', async () => {
  const closed = vi.fn();
  await act(async () => root.render(<LanTransfer sources={[]} media={[]} onClose={closed} onReceived={vi.fn()} />));
  await act(async () => button('Bật gửi / nhận').click());
  let stop!: () => void;
  vi.mocked(callNative).mockImplementation(command => command === 'lan_stop' ? new Promise<void>(resolve => { stop = resolve; }) as Promise<never> : Promise.resolve(active as never));
  await act(async () => button('Dừng & đóng').click());
  expect(closed).not.toHaveBeenCalled();
  await act(async () => stop());
  expect(closed).toHaveBeenCalledOnce();
});

it('refreshes the library only after the native host accepts received files', async () => {
  const received = vi.fn().mockResolvedValue(undefined); const closed = vi.fn();
  await act(async () => root.render(<LanTransfer sources={[]} media={[]} onClose={closed} onReceived={received} />));
  await act(async () => button('Bật gửi / nhận').click());
  let accept!: () => void;
  vi.mocked(callNative).mockImplementation(command => command === 'lan_accept' ? new Promise<void>(resolve => { accept = resolve; }) as Promise<never> : Promise.resolve(active as never));
  await act(async () => button('Thêm ảnh đã nhận').click());
  expect(received).not.toHaveBeenCalled();
  await act(async () => accept());
  expect(received).toHaveBeenCalledOnce(); expect(closed).toHaveBeenCalledOnce();
});

it('stops a possibly late native session when starting fails', async () => {
  vi.mocked(callNative).mockImplementation(command => command === 'lan_start'
    ? Promise.reject(new Error('Native command timed out'))
    : Promise.resolve(undefined as never));
  await act(async () => root.render(<LanTransfer sources={[]} media={[]} onClose={vi.fn()} onReceived={vi.fn()} />));
  await act(async () => button('Bật gửi / nhận').click());
  expect(callNative).toHaveBeenCalledWith('lan_stop');
  expect(host.textContent).toContain('Native command timed out');
});
