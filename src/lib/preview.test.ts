import { describe, expect, it } from 'vitest';
import { persistablePreview, restorePreview } from './preview';
import { emptyState } from './types';
describe('browser preview persistence', () => {
  it('never writes credentials or temporary image content into local storage', () => {
    const state = { ...emptyState, sources: [{ id: 's', name: 'Drive', kind: 'drive' as const, location: 'folder', enabled: true, recursive: false, accessToken: 'private-token', password: 'private-password', apiKey: 'key' }], current: { id: 'one', name: 'private.jpg', sourceId: 's', url: 'blob:session' } };
    const saved = JSON.stringify(persistablePreview(state));
    expect(saved).not.toContain('private-token');
    expect(saved).not.toContain('private-password');
    expect(saved).not.toContain('blob:session');
  });
  it('recovers malformed saved state without leaving the app unusable', () => {
    expect(restorePreview('{broken').sources).toEqual([]);
    expect(restorePreview('{"version":1,"sources":null}').sources).toEqual([]);
    expect(restorePreview('{"version":1,"settings":{"intervalSeconds":-5}}').settings.intervalSeconds).toBeGreaterThan(0);
  });
});
