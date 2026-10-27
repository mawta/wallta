// @vitest-environment jsdom
import { act, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { GoogleConnect } from './GoogleConnect';
import { callNative } from '../lib/native';

vi.mock('../lib/native', () => ({ isNative: () => true, isAndroid: () => true, errorMessage: (error: unknown) => String(error), callNative: vi.fn() }));

it('continues polling Google while parent state refreshes and callbacks change', async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true }); vi.useFakeTimers();
  vi.mocked(callNative).mockImplementation(async command => (command === 'google_begin' ? { phase: 'authorizing', pollIntervalSeconds: 5 } : { phase: 'ready', sourceKind: 'local', location: 'import' }) as never);
  function Harness() {
    const [tick, setTick] = useState(0);
    useEffect(() => { const t = setInterval(() => setTick(n => n + 1), 1000); return () => clearInterval(t); }, []);
    return <div data-tick={tick}><GoogleConnect source={{ id: 'polling', kind: 'photos', name: 'Photos', location: '', enabled: true, recursive: true }} onConnected={() => {}} /></div>;
  }
  const host = document.createElement('div'); const root = createRoot(host);
  try {
    await act(async () => root.render(<Harness />));
    await act(async () => host.querySelector('button')!.click());
    for (let i = 0; i < 6; i++) await act(async () => vi.advanceTimersByTimeAsync(1000));
    expect(callNative).toHaveBeenCalledWith('google_poll', { sourceId: 'polling' });
  } finally { await act(async () => root.unmount()); vi.useRealTimers(); }
});

it('does not label a newly authorized grant with the previous account when identity is unavailable', async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.mocked(callNative).mockResolvedValue({ phase: 'ready', sourceKind: 'local', location: 'new-import' });
  const connected = vi.fn(); const host = document.createElement('div'); const root = createRoot(host);
  try {
    await act(async () => root.render(<GoogleConnect source={{ id: 'account', name: 'Photos', kind: 'photos', location: '', recursive: false, enabled: true, google: { provider: 'photos', connected: true, account: 'old@example.com' } }} onConnected={connected} />));
    await act(async () => host.querySelector('button')!.click());
    expect(connected).toHaveBeenCalledWith(expect.objectContaining({ google: expect.objectContaining({ account: undefined }) }));
  } finally { await act(async () => root.unmount()); }
});

it('cancels authorization that starts after the source form has already closed', async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  let finish!: (value: unknown) => void;
  const started = new Promise(resolve => { finish = resolve; });
  vi.mocked(callNative).mockImplementation(command => command === 'google_begin' ? started as Promise<never> : Promise.resolve(undefined as never));
  const host = document.createElement('div'); document.body.append(host);
  const root = createRoot(host);
  await act(async () => root.render(<GoogleConnect source={{ id: 'late', name: 'Photos', kind: 'photos', location: '', recursive: false, enabled: true }} onConnected={vi.fn()} />));
  await act(async () => host.querySelector('button')!.click());
  expect(callNative).toHaveBeenCalledWith('google_begin', expect.anything());
  await act(async () => root.unmount());
  await act(async () => finish({ phase: 'authorizing' }));
  expect(callNative).toHaveBeenCalledWith('google_cancel', { sourceId: 'late', discardImports: true });
  host.remove();
});
