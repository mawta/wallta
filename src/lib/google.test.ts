import { expect, it } from 'vitest';
import { googlePollDelay, googleVerificationUrl } from './google';

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
