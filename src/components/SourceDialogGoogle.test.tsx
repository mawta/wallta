// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { SourceDialog } from './SourceDialog';
import { callNative } from '../lib/native';
vi.mock('../lib/native', () => ({ isNative: () => true, isAndroid: () => false, isDesktop: () => true, callNative: vi.fn(), registerPreviewFiles: vi.fn(), errorMessage: (e: unknown) => String(e) }));

it('can save a connected account without a folder and choose one after reopening the saved source', async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); };
  HTMLDialogElement.prototype.close = function () {};
  vi.mocked(callNative).mockImplementation(async command => (command === 'google_drive_folders' ? { folders: [{ id: 'family', name: 'Gia đình' }] } : command === 'google_drive_select' ? { id: 'family', name: 'Gia đình', account: 'test@example.com' } : undefined) as never);
  const saved = vi.fn().mockResolvedValue(true); const host = document.createElement('div'); document.body.append(host); const root = createRoot(host);
  const source = { id: 'saved', name: 'Drive', kind: 'drive' as const, location: '', enabled: false, recursive: true, google: { provider: 'drive' as const, connected: true, account: 'test@example.com' } };
  try {
    await act(async () => root.render(<SourceDialog initial={source} onSave={saved} onClose={vi.fn()} busy={false} />));
    await act(async () => host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    expect(saved).toHaveBeenCalledWith(expect.objectContaining({ location: '', enabled: false }), expect.objectContaining({ apply: false }));
    await act(async () => root.render(null));
    await act(async () => root.render(<SourceDialog initial={saved.mock.calls[0][0]} onSave={saved} onClose={vi.fn()} busy={false} />));
    await act(async () => Array.from(host.querySelectorAll('button')).find(b => b.textContent?.includes('Chọn / đổi thư mục'))!.click());
    await act(async () => Array.from(host.querySelectorAll('button')).find(b => b.textContent === 'Gia đình')!.click());
    await act(async () => Array.from(host.querySelectorAll('button')).find(b => b.textContent?.includes('Dùng thư mục này'))!.click());
    await act(async () => host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    expect(saved).toHaveBeenLastCalledWith(expect.objectContaining({ location: 'family', enabled: true, google: expect.objectContaining({ selectionName: 'Gia đình' }) }), expect.objectContaining({ apply: true }));
  } finally { await act(async () => root.unmount()); host.remove(); }
});

it('keeps folder selection optional after completing a new Drive login', async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); };
  HTMLDialogElement.prototype.close = function () {};
  vi.mocked(callNative).mockReset().mockImplementation(async command => (command === 'google_begin' ? { phase: 'ready', sourceKind: 'drive', account: 'new@example.com' } : { folders: [] }) as never);
  const host = document.createElement('div'); document.body.append(host); const root = createRoot(host); const saved = vi.fn().mockResolvedValue(true);
  try {
    await act(async () => root.render(<SourceDialog initialKind="drive" busy={false} onSave={saved} onClose={vi.fn()} />));
    const client = host.querySelector<HTMLInputElement>('input[placeholder="…apps.googleusercontent.com"]')!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(client, 'test-client');
      client.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await act(async () => Array.from(host.querySelectorAll('button')).find(b => b.textContent === 'Kết nối tài khoản Google')!.click());
    expect(host.textContent).toContain('new@example.com');
    expect(callNative).not.toHaveBeenCalledWith('google_drive_folders', expect.anything());
    expect(host.textContent).toContain('Lưu kết nối');
    await act(async () => host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    expect(saved).toHaveBeenCalledWith(expect.objectContaining({ location: '', enabled: false, google: expect.objectContaining({ connected: true }) }), expect.objectContaining({ apply: false }));
  } finally { await act(async () => root.unmount()); host.remove(); }
});
