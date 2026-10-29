import { expect, it } from 'vitest';
import { googleConnectionChanges, googlePollDelay, googleVerificationUrl } from './google';

it('respects server polling intervals with bounded fallback', () => {
  expect(googlePollDelay(12)).toBe(12000);
  expect(googlePollDelay(undefined)).toBe(5000);
  expect(googlePollDelay(-1)).toBe(5000);
});
it('only links to HTTPS Google authorization and photo selection pages', () => {
  expect(googleVerificationUrl('https://accounts.google.com/device')).toContain('accounts.google.com');
  expect(googleVerificationUrl('https://photos.google.com/ambient/abc')).toBeTruthy();
  expect(googleVerificationUrl('javascript:alert(1)')).toBeNull();
  expect(googleVerificationUrl('https://google.com.evil.example')).toBeNull();
});
it('allows only the configured HTTPS relay origin for TV pairing', () => {
  expect(googleVerificationUrl('https://relay.example/connect/abc', 'https://relay.example/')).toBe('https://relay.example/connect/abc');
  expect(googleVerificationUrl('https://other.example/connect/abc', 'https://relay.example')).toBeNull();
  expect(googleVerificationUrl('http://relay.example/connect/abc', 'http://relay.example')).toBeNull();
});
it('clears the previous folder and identity when a new Drive grant needs a fresh selection', () => {
  const changed = googleConnectionChanges({ id: 'source', kind: 'drive', name: 'Drive', location: 'old-folder', enabled: true, recursive: true, google: { provider: 'drive', account: 'old@example.com', selectionName: 'Old', connected: true, connectionId: 'old' } }, { phase: 'ready' });
  expect(changed.location).toBe('');
  expect(changed.google?.account).toBeUndefined();
  expect(changed.google?.selectionName).toBeUndefined();
  expect(changed.google?.connectionId).not.toBe('old');
});
