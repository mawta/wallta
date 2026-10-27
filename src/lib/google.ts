import type { Source, SourceKind } from './types';
import { uuid } from './compat';

export interface GoogleStatus {
  account?: string;
  selectionName?: string;
  phase: 'authorizing' | 'selecting' | 'ready' | 'error';
  verificationUrl?: string;
  userCode?: string;
  pollIntervalSeconds?: number;
  message?: string;
  location?: string;
  sourceKind?: SourceKind;
}
export function googleConnectionChanges(source: Source, next: GoogleStatus): Partial<Source> {
  const provider = source.kind === 'drive' ? 'drive' : 'photos';
  return { location: next.location || (provider === 'drive' ? '' : source.location), kind: next.sourceKind || source.kind,
    name: source.name || (provider === 'drive' ? 'Google Drive' : 'Google Photos'),
    google: { provider, connected: true, connectionId: uuid(), account: next.account,
      selectionName: next.selectionName || (provider === 'photos' && next.sourceKind === 'local' ? 'Bộ ảnh đã nhập' : undefined) },
  };
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
