// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { SourceDialog } from './SourceDialog';
import { callNative } from '../lib/native';
import type { Source } from '../lib/types';

vi.mock('../lib/native', () => ({ isNative: () => true, isAndroid: () => false, isDesktop: () => true, callNative: vi.fn(), registerPreviewFiles: vi.fn(), errorMessage: (e: unknown) => e instanceof Error ? e.message : String(e) }));
let host: HTMLDivElement; let root: ReturnType<typeof createRoot>;
const save = vi.fn().mockResolvedValue(true);
const click = async (name: string) => {
  const button = Array.from(host.querySelectorAll('button')).find(b => b.getAttribute('aria-label') === name || b.textContent?.includes(name) || (name === 'Chọn thư mục' && b.classList.contains('folder-picker')));
  expect(button).toBeDefined(); await act(async () => button!.click());
};
const submit = async () => act(async () => host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); }; HTMLDialogElement.prototype.close = function () {};
  vi.mocked(callNative).mockReset(); save.mockClear(); host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it('retains both local folder selections and can remove one after reopening', async () => {
  vi.mocked(callNative).mockImplementation(async command => command === 'pick_folder' ? (vi.mocked(callNative).mock.calls.filter(([name]) => name === 'pick_folder').length === 1 ? 'D:\\Family' : 'D:\\Trips') as never : undefined as never);
  await act(async () => root.render(<SourceDialog busy={false} onSave={save} onClose={vi.fn()} />));
  await click('Chọn thư mục'); await click('Chọn thư mục'); await submit();
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ location: 'D:\\Family', folders: [{ location: 'D:\\Family', name: 'Family' }, { location: 'D:\\Trips', name: 'Trips' }] }), expect.anything());
  const saved: Source = save.mock.calls[0][0];
  await act(async () => root.render(null));
  await act(async () => root.render(<SourceDialog initial={saved} busy={false} onSave={save} onClose={vi.fn()} />));
  expect(host.querySelector('[aria-label="Thư mục đã chọn"]')!.textContent).toContain('Family');
  await click('Bỏ thư mục Family'); await submit();
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ location: 'D:\\Trips', folders: [{ location: 'D:\\Trips', name: 'Trips' }] }), expect.anything());
});

it('does not add the same local selection twice', async () => {
  vi.mocked(callNative).mockImplementation(async command => command === 'pick_folder' ? 'D:\\Family' as never : undefined as never);
  await act(async () => root.render(<SourceDialog busy={false} onSave={save} onClose={vi.fn()} />));
  await click('Chọn thư mục'); await click('Chọn thư mục'); await submit();
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ folders: [{ location: 'D:\\Family', name: 'Family' }] }), expect.anything());
});

it('adds a second Drive folder using the saved account without replacing the first', async () => {
  vi.mocked(callNative).mockImplementation(async command => (command === 'google_drive_folders' ? { folders: [{ id: 'trips', name: 'Trips' }] } : command === 'google_drive_select' ? { id: 'trips', name: 'Trips', account: 'saved@example.com' } : undefined) as never);
  await act(async () => root.render(<SourceDialog initial={{ id: 'saved-drive', name: 'Drive', kind: 'drive', location: 'family', recursive: true, enabled: true, google: { provider: 'drive', connected: true, account: 'saved@example.com', selectionName: 'Family' } }} busy={false} onSave={save} onClose={vi.fn()} />));
  await click('Chọn / đổi thư mục'); await click('Trips'); await click('Dùng thư mục này'); await submit();
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ location: 'family', folders: [{ location: 'family', name: 'Family' }, { location: 'trips', name: 'Trips' }] }), expect.anything());
  expect(callNative).not.toHaveBeenCalledWith('google_begin', expect.anything());
});
it('reports an oversized picked local folder without throwing during render', async () => {
  vi.mocked(callNative).mockImplementation(async command => command === 'pick_folder' ? 'D:\\Extra' as never : undefined as never);
  await act(async () => root.render(<SourceDialog initial={{ id: 's', name: 'Many', kind: 'local', location: 'D:\\0', folders: Array.from({ length: 50 }, (_, index) => ({ location: `D:\\${index}` })), recursive: true, enabled: true }} busy={false} onSave={save} onClose={vi.fn()} />));
  await click('Chọn thư mục');
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('50');
  expect(host.querySelector('form')).not.toBeNull();
});
