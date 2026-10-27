import { expect, it, vi } from 'vitest';
import { saveSourceSelection } from './saveSource';
import { emptyState, type Source } from './types';
const source: Source = { id: 'new', name: 'Gia đình', kind: 'drive', location: 'folder', enabled: true, recursive: true };
it('persists the folder and autostart before applying that exact source and starting rotation', async () => {
  const state = { ...emptyState, sources: [source] };
  const applied = { ...state, running: true, current: { id: 'image', name: 'Photo', url: '', sourceId: source.id } };
  const call = vi.fn(async (command: string) => command === 'list_media' ? [applied.current] : command === 'next_wallpaper' ? applied : state);
  const result = await saveSourceSelection(emptyState, source, { apply: true, autostart: true }, call, () => {});
  expect(call.mock.calls.map(c => c[0])).toEqual(['save_state', 'list_media', 'get_state', 'next_wallpaper']);
  expect(call).toHaveBeenCalledWith('save_state', { state: expect.objectContaining({ settings: expect.objectContaining({ autostart: true }) }) });
  expect(call).toHaveBeenCalledWith('next_wallpaper', { sourceId: source.id, startRunning: true });
  expect(result.state.current?.sourceId).toBe(source.id);
});
it('retains a saved source and reports an empty folder without applying another source', async () => {
  const saved = { ...emptyState, sources: [source] }; const changed = vi.fn();
  const call = vi.fn(async (command: string) => command === 'list_media' ? [] : saved);
  await expect(saveSourceSelection(emptyState, source, { apply: true, autostart: true }, call, changed)).rejects.toThrow('Đã lưu nguồn');
  expect(changed).toHaveBeenCalledWith(saved);
  expect(call.mock.calls.map(c => c[0])).not.toContain('next_wallpaper');
});
it('can save an account and choose its folder later without enabling autostart or rotation', async () => {
  const pending = { ...source, location: '', enabled: false }; const saved = { ...emptyState, sources: [pending] };
  const call = vi.fn(async (command: string) => command === 'list_media' ? [] : saved);
  await saveSourceSelection(emptyState, pending, { apply: false, autostart: true }, call, () => {});
  expect(call).toHaveBeenCalledWith('save_state', { state: expect.objectContaining({ settings: expect.objectContaining({ autostart: false }) }) });
  expect(call.mock.calls.map(c => c[0])).not.toContain('next_wallpaper');
});
