import type { SourceKind } from './types';

export interface GoogleStatus {
  phase: 'authorizing' | 'selecting' | 'ready' | 'error';
  verificationUrl?: string;
  userCode?: string;
  pollIntervalSeconds?: number;
  message?: string;
  location?: string;
  sourceKind?: SourceKind;
}
export function googlePollDelay(seconds?: number): number {
  return Number.isFinite(seconds) && seconds! >= 1 ? seconds! * 1000 : 5000;
}
export function googleVerificationUrl(value?: string, relayUrl?: string): string | null {
  try {
    const url = new URL(value || '');
    const relay = relayUrl ? new URL(relayUrl) : null;
    const trustedRelay = relay?.protocol === 'https:' && !relay.username && !relay.password && relay.origin === url.origin;
    return url.protocol === 'https:' && !url.username && !url.password && (trustedRelay || url.hostname === 'google.com' || url.hostname.endsWith('.google.com')) ? url.href : null;
  } catch { return null; }
}
