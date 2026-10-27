export type UpdatePhase = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'installing' | 'upToDate' | 'error' | 'permissionRequired' | 'unsupported';

export interface UpdateStatus {
  currentVersion: string;
  phase: UpdatePhase;
  version: string | null;
  notes: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: string | null;
  lastChecked: number | null;
}

export interface UpdatesController {
  status: UpdateStatus;
  pending: boolean;
  check: () => Promise<void>;
  download: () => Promise<void>;
  cancel: () => Promise<void>;
  install: () => Promise<void>;
}
