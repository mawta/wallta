import { expect, it } from 'vitest';
import { validateSource } from './rules';

it('allows a connected Drive account to be saved before choosing its folder', () => {
  const pending = { kind: 'drive' as const, location: '', name: 'Drive', enabled: false, google: { provider: 'drive' as const, connected: true } };
  expect(validateSource(pending)).toBeNull();
  expect(validateSource({ ...pending, google: undefined })).not.toBeNull();
  expect(validateSource({ ...pending, enabled: true })).not.toBeNull();
});
