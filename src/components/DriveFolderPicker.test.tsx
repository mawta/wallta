// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { DriveFolderPicker } from './DriveFolderPicker';
import { callNative } from '../lib/native';

vi.mock('../lib/native', () => ({ callNative: vi.fn(), errorMessage: (e: unknown) => String(e) }));
afterEach(() => vi.clearAllMocks());

it('opens the remembered folder of a saved source and confirms it without signing in again', async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.mocked(callNative).mockImplementation(async command => (command === 'google_drive_select'
    ? { id: 'saved-folder', name: 'Gia đình', account: 'test@example.com' }
    : { folders: [{ id: 'child', name: 'Du lịch' }], account: 'test@example.com' }) as never);
  const selected = vi.fn();
  const host = document.createElement('div'); document.body.append(host); const root = createRoot(host);
  try {
    await act(async () => root.render(<DriveFolderPicker sourceId="saved" location="saved-folder" selectionName="Gia đình" onSelect={selected} />));
    expect(callNative).toHaveBeenCalledWith('google_drive_folders', expect.objectContaining({ sourceId: 'saved', parentId: 'saved-folder' }));
    expect(host.textContent).toContain('Du lịch');
    await act(async () => Array.from(host.querySelectorAll('button')).find(b => b.textContent?.includes('Dùng thư mục này'))!.click());
    expect(selected).toHaveBeenCalledWith({ id: 'saved-folder', name: 'Gia đình', account: 'test@example.com' });
    expect(callNative).not.toHaveBeenCalledWith('google_begin', expect.anything());
  } finally { await act(async () => root.unmount()); host.remove(); }
});

it('keeps the folder choice unchanged when loading another folder fails', async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.mocked(callNative).mockRejectedValue(new Error('Không thể tải Drive'));
  const selected = vi.fn(); const host = document.createElement('div'); const root = createRoot(host);
  try {
    await act(async () => root.render(<DriveFolderPicker sourceId="saved" location="folder" selectionName="Ảnh" onSelect={selected} />));
    expect(host.textContent).toContain('Không thể tải Drive'); expect(selected).not.toHaveBeenCalled();
  } finally { await act(async () => root.unmount()); }
});
