// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import App from './App';
import { callNative } from './lib/native';
import { emptyState, type AppState, type Source } from './lib/types';

vi.mock('./lib/native', () => ({ isNative: () => true, isAndroid: () => false, isDesktop: () => true, callNative: vi.fn(), registerPreviewFiles: vi.fn(), errorMessage: (e: unknown) => String(e) }));
vi.mock('./lib/useUpdates', () => ({ useUpdates: () => ({ status: { phase: 'idle', currentVersion: '0.1.3' }, pending: false }) }));
let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
let stored: AppState;
const drive: Source = { id: 'saved-drive', name: 'Drive gia đình', kind: 'drive', location: '', recursive: true, enabled: false, google: { provider: 'drive', connected: true, account: 'saved@example.com', connectionId: 'grant-1' } };
const photos: Source = { id: 'saved-photos', name: 'Ảnh Google', kind: 'local', location: 'photos-import', recursive: true, enabled: true, google: { provider: 'photos', connected: true, account: 'photos@example.com', selectionName: 'Bộ ảnh đã nhập' } };
const click = async (text: string, scope: ParentNode = host) => {
  const button = Array.from(scope.querySelectorAll('button')).find(b => b.getAttribute('aria-label') === text || b.textContent?.trim() === text);
  expect(button, `button ${text}`).toBeDefined();
  await act(async () => button!.click());
};
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute('open'); };
  stored = structuredClone({ ...emptyState, sources: [drive, photos] });
  vi.mocked(callNative).mockReset().mockImplementation(async (command, args) => {
    if (command === 'get_state') return structuredClone(stored) as never;
    if (command === 'save_state') { stored = structuredClone(args!.state as AppState); return structuredClone(stored) as never; }
    if (command === 'get_capabilities') return { platform: 'windows', wallpaper: true, screensaver: false, minIntervalSeconds: 60, notes: [] } as never;
    if (command === 'list_media') return [] as never;
    if (command === 'google_drive_folders') return { folders: [{ id: 'family', name: 'Gia đình' }] } as never;
    if (command === 'google_drive_select') return { id: 'family', name: 'Gia đình', account: 'saved@example.com' } as never;
    return undefined as never;
  });
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it('reuses a saved Drive connection from the new-source form and saves folder changes under the existing ID', async () => {
  await act(async () => root.render(<App />));
  await click('Nguồn ảnh');
  await click('Google DriveThư mục Drive');
  let dialog = host.querySelector('dialog')!;
  expect(dialog.textContent).toContain('Tài khoản đã kết nối');
  expect(dialog.textContent).toContain('saved@example.com');
  expect(dialog.textContent).not.toContain('photos@example.com');
  await click('Dùng kết nối', dialog);
  dialog = host.querySelector('dialog')!;
  expect(dialog.textContent).toContain('saved@example.com');
  expect(dialog.querySelector('input[placeholder="…apps.googleusercontent.com"]')).toBeNull();
  expect(callNative).not.toHaveBeenCalledWith('google_begin', expect.anything());
  expect(callNative).not.toHaveBeenCalledWith('google_drive_folders', expect.anything());
  await click('Chọn / đổi thư mục', dialog);
  expect(callNative).toHaveBeenCalledWith('google_drive_folders', expect.objectContaining({ sourceId: 'saved-drive' }));
  await click('Gia đình', dialog);
  await click('Dùng thư mục này', dialog);
  // Save the selected folder without applying a wallpaper in this test.
  await act(async () => Array.from(dialog.querySelectorAll('label')).find(l => l.textContent === 'Áp dụng sau khi lưu')!.querySelector<HTMLInputElement>('input')!.click());
  await click('Lưu nguồn', dialog);
  expect(host.querySelector('dialog')).toBeNull();
  expect(stored.sources).toHaveLength(2);
  expect(stored.sources[0]).toMatchObject({ id: 'saved-drive', location: 'family', google: { connectionId: 'grant-1', account: 'saved@example.com' } });
  await click('Google DriveThư mục Drive');
  await click('Dùng kết nối', host.querySelector('dialog')!);
  expect(host.querySelector('dialog')!.textContent).toContain('Gia đình');
});

it('shows both providers in Settings and opens their saved sources after closing and reopening configuration', async () => {
  await act(async () => root.render(<App />));
  await click('Cài đặt');
  const section = Array.from(host.querySelectorAll('section')).find(s => s.textContent?.includes('Tài khoản đã kết nối'));
  expect(section).toBeDefined();
  expect(section!.textContent).toContain('saved@example.com');
  expect(section!.textContent).toContain('photos@example.com');
  await click('Quản lý', section!);
  expect(host.querySelector('dialog')!.textContent).toContain('saved@example.com');
  await act(async () => host.querySelector<HTMLButtonElement>('dialog [aria-label="Đóng"]')!.click());
  await click('Tổng quan'); await click('Cài đặt');
  const buttons = Array.from(host.querySelectorAll('button')).filter(b => b.textContent === 'Quản lý');
  await act(async () => buttons[1].click());
  expect(host.querySelector('dialog')!.textContent).toContain('photos@example.com');
  expect(host.querySelector('dialog')!.textContent).toContain('Chọn lại ảnh Google Photos');
});

it('opens imported Photos from the Photos new-source screen without a second login', async () => {
  await act(async () => root.render(<App />));
  await click('Nguồn ảnh'); await click('Google PhotosNhập ảnh / album TV');
  const dialog = host.querySelector('dialog')!;
  expect(dialog.textContent).toContain('photos@example.com');
  expect(dialog.textContent).not.toContain('saved@example.com');
  await click('Dùng kết nối', dialog);
  expect(host.querySelector('dialog')!.textContent).toContain('Chọn lại ảnh Google Photos');
  expect(callNative).not.toHaveBeenCalledWith('google_begin', expect.anything());
});

it('restores a newly saved account after the entire UI remounts without selecting a folder', async () => {
  stored.sources = [];
  const backend = vi.mocked(callNative).getMockImplementation()!;
  vi.mocked(callNative).mockImplementation(async (command, args) => command === 'google_begin' ? { phase: 'ready', sourceKind: 'drive', account: 'new@example.com' } as never : backend(command, args));
  await act(async () => root.render(<App />));
  await click('Nguồn ảnh'); await click('Google DriveThư mục Drive');
  const client = host.querySelector<HTMLInputElement>('input[placeholder="…apps.googleusercontent.com"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(client, 'test-client');
    client.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await click('Kết nối tài khoản Google');
  await click('Lưu kết nối');
  expect(host.querySelector('dialog')).toBeNull();
  expect(stored.sources).toHaveLength(1);
  const savedId = stored.sources[0].id;
  expect(stored.sources[0]).toMatchObject({ location: '', enabled: false, google: { connected: true, account: 'new@example.com' } });
  await act(async () => root.unmount()); root = createRoot(host);
  await act(async () => root.render(<App />));
  await click('Cài đặt'); await click('Quản lý');
  expect(host.querySelector('dialog')!.textContent).toContain('new@example.com');
  await click('Chọn / đổi thư mục', host.querySelector('dialog')!);
  expect(callNative).toHaveBeenCalledWith('google_drive_folders', expect.objectContaining({ sourceId: savedId }));
  expect(vi.mocked(callNative).mock.calls.filter(([command]) => command === 'google_begin')).toHaveLength(1);
});
