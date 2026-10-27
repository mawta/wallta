// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { AppUpdates } from './AppUpdates';
import type { UpdateStatus, UpdatesController } from '../lib/updates';

const initial: UpdateStatus = { currentVersion: '0.1.2', phase: 'idle', version: null, notes: null, downloadedBytes: 0, totalBytes: null, error: null, lastChecked: null };
let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
let controller: UpdatesController;
const perform = vi.fn(async (operation: () => Promise<void>) => { await operation(); return true; });
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
  controller = { status: { ...initial }, pending: false, check: vi.fn().mockResolvedValue(undefined), download: vi.fn().mockResolvedValue(undefined), cancel: vi.fn().mockResolvedValue(undefined), install: vi.fn().mockResolvedValue(undefined) };
  perform.mockClear();
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
const button = (name: string) => Array.from(host.querySelectorAll('button')).find(item => item.textContent === name)!;
const render = async (status: Partial<UpdateStatus>, busy = false) => {
  controller.status = { ...initial, ...status };
  await act(async () => root.render(<AppUpdates updates={controller} busy={busy} perform={perform} />));
};

it('offers download separately from installation and starts nothing during render', async () => {
  await render({ phase: 'available', version: '0.1.3', notes: 'Sửa lỗi ảnh.' });
  expect(host.textContent).toContain('0.1.2');
  expect(host.textContent).toContain('0.1.3');
  expect(controller.download).not.toHaveBeenCalled();
  expect(button('Cài đặt')).toBeUndefined();
  await act(async () => button('Tải bản mới').click());
  expect(controller.download).toHaveBeenCalledOnce();
  expect(controller.install).not.toHaveBeenCalled();
});

it('locks installation behind pending settings saves and the application operation lock', async () => {
  await render({ phase: 'ready', version: '0.1.3' }, true);
  expect(button('Cài đặt').disabled).toBe(true);
  await act(async () => button('Cài đặt').click());
  expect(controller.install).not.toHaveBeenCalled();
  await render({ phase: 'ready', version: '0.1.3' });
  await act(async () => button('Cài đặt').click());
  expect(perform).toHaveBeenCalledOnce();
  expect(controller.install).toHaveBeenCalledOnce();
});

it('shows actual progress, cancellation and a separate Android permission continuation', async () => {
  await render({ phase: 'downloading', version: '0.1.3', downloadedBytes: 512, totalBytes: 1024 });
  expect(host.querySelector('progress')?.value).toBe(512);
  expect(host.querySelector('progress')?.max).toBe(1024);
  await act(async () => button('Hủy tải').click());
  expect(controller.cancel).toHaveBeenCalledOnce();
  await render({ phase: 'permissionRequired', version: '0.1.3' });
  expect(host.textContent).toContain('Cho phép');
  await act(async () => button('Tiếp tục cài').click());
  expect(controller.install).toHaveBeenCalledOnce();
});

it('keeps failure visible, renders release notes as text and permits a fresh check', async () => {
  await render({ phase: 'error', version: '0.1.3', error: 'Mất kết nối.', notes: '<img src=x onerror=alert(1)>' });
  expect(host.textContent).toContain('Mất kết nối.');
  expect(host.textContent).not.toContain('Đã mới nhất');
  expect(host.querySelector('img')).toBeNull();
  await act(async () => button('Kiểm tra').click());
  expect(controller.check).toHaveBeenCalledOnce();
});

it('makes browser preview read-only without offering an installer', async () => {
  await render({ phase: 'unsupported' });
  expect(host.textContent).toContain('Dùng bản cài');
  expect(host.querySelectorAll('button').length).toBe(0);
  expect(controller.check).not.toHaveBeenCalled();
  expect(controller.install).not.toHaveBeenCalled();
});
