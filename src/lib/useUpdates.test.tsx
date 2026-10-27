// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useUpdates } from './useUpdates';
import { callNative } from './native';
import type { UpdatesController, UpdateStatus } from './updates';

const environment = vi.hoisted(() => ({ native: true }));
vi.mock('./native', () => ({ callNative: vi.fn(), isNative: () => environment.native, errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error) }));
const available: UpdateStatus = { currentVersion: '0.1.2', phase: 'available', version: '0.1.3', notes: null, downloadedBytes: 0, totalBytes: 1000, error: null, lastChecked: 1000 };
let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
let updates: UpdatesController;
function Probe() { updates = useUpdates(true); return <output>{updates.status.phase}:{updates.status.error}</output>; }
beforeEach(() => {
  vi.useFakeTimers();
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  environment.native = true; vi.mocked(callNative).mockReset();
  vi.mocked(callNative).mockResolvedValue(available as never);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers(); vi.restoreAllMocks(); });
const mount = () => act(async () => root.render(<Probe />));

it('checks automatically once at startup, with no automatic download or install', async () => {
  await mount();
  expect(callNative).toHaveBeenCalledWith('update_check', { automatic: true });
  expect(vi.mocked(callNative).mock.calls.filter(([command]) => command === 'update_check')).toHaveLength(1);
  expect(callNative).not.toHaveBeenCalledWith('update_download');
  expect(callNative).not.toHaveBeenCalledWith('update_install');
  expect(updates.status.phase).toBe('available');
});

it('keeps a bridge failure as an error and lets manual checking recover', async () => {
  await mount();
  vi.mocked(callNative).mockRejectedValueOnce(new Error('Mất kết nối.'));
  await act(async () => updates.check());
  expect(updates.status.phase).toBe('error');
  expect(host.textContent).toContain('Mất kết nối.');
  await act(async () => updates.check());
  expect(callNative).toHaveBeenLastCalledWith('update_check', { automatic: false });
  expect(updates.status.phase).toBe('available');
});

it('ignores an old status poll that arrives after download cancellation', async () => {
  await mount();
  vi.mocked(callNative).mockResolvedValueOnce({ ...available, phase: 'downloading' } as never);
  await act(async () => updates.download());
  let finishPoll!: (status: UpdateStatus) => void;
  vi.mocked(callNative).mockImplementationOnce(() => new Promise<UpdateStatus>(resolve => { finishPoll = resolve; }) as Promise<never>);
  await act(async () => vi.advanceTimersByTimeAsync(1000));
  expect(finishPoll).toBeTypeOf('function');
  await act(async () => updates.cancel());
  expect(updates.status.phase).toBe('available');
  await act(async () => finishPoll({ ...available, phase: 'ready' }));
  expect(updates.status.phase).toBe('available');
});

it('does not contact native APIs or releases in browser preview', async () => {
  environment.native = false;
  await mount();
  expect(updates.status.phase).toBe('unsupported');
  expect(callNative).not.toHaveBeenCalled();
});

it('defers hidden startup checks until the application becomes visible', async () => {
  const visibility = vi.spyOn(document, 'hidden', 'get').mockReturnValue(true);
  await mount();
  expect(callNative).not.toHaveBeenCalled();
  visibility.mockReturnValue(false);
  await act(async () => document.dispatchEvent(new Event('visibilitychange')));
  expect(callNative).toHaveBeenCalledWith('update_check', { automatic: true });
  expect(updates.status.phase).toBe('available');
});
