// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { GoogleConnect } from './GoogleConnect';
import { callNative } from '../lib/native';

vi.mock('../lib/native', () => ({ isNative: () => true, isAndroid: () => true, errorMessage: (error: unknown) => String(error), callNative: vi.fn() }));

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
