import { useCallback, useEffect, useRef, useState } from 'react';
import { callNative, errorMessage, isNative } from './native';
import type { UpdatesController } from './updates';
import type { UpdateStatus } from './updates';
import { APP_VERSION } from './version';

export function useUpdates(enabled: boolean): UpdatesController {
  const [status, setStatus] = useState<UpdateStatus>(() => ({ currentVersion: APP_VERSION, phase: isNative() ? 'idle' : 'unsupported', version: null, notes: null, downloadedBytes: 0, totalBytes: null, error: null, lastChecked: null }));
  const [pending, setPending] = useState(false);
  const alive = useRef(false);
  const acting = useRef(false);
  const revision = useRef(0);
  const polling = useRef(false);

  const execute = useCallback(async (command: string, args?: Record<string, unknown>) => {
    if (!enabled || !isNative() || !alive.current || acting.current) return;
    if (args?.automatic === true && document.hidden) return;
    acting.current = true;
    const ticket = ++revision.current;
    setPending(true);
    try {
      const next = args ? await callNative<UpdateStatus>(command, args) : await callNative<UpdateStatus>(command);
      if (alive.current && ticket === revision.current) setStatus(next);
    } catch (error) {
      if (alive.current && ticket === revision.current) setStatus(previous => ({ ...previous, phase: 'error', error: errorMessage(error) }));
    } finally {
      if (ticket === revision.current) {
        acting.current = false;
        if (alive.current) setPending(false);
      }
    }
  }, [enabled]);

  const refresh = useCallback(async () => {
    if (!enabled || !isNative() || !alive.current || acting.current || polling.current) return;
    polling.current = true;
    const ticket = revision.current;
    try {
      const next = await callNative<UpdateStatus>('update_status');
      if (alive.current && ticket === revision.current) setStatus(next);
    } catch (error) {
      if (alive.current && ticket === revision.current) setStatus(previous => ({ ...previous, phase: 'error', error: errorMessage(error) }));
    } finally { polling.current = false; }
  }, [enabled]);

  useEffect(() => {
    alive.current = true;
    acting.current = false;
    ++revision.current;
    if (enabled && isNative()) void execute('update_check', { automatic: true });
    const foreground = () => {
      if (document.hidden) return;
      void refresh().then(() => execute('update_check', { automatic: true }));
    };
    if (enabled && isNative()) {
      window.addEventListener('focus', foreground);
      document.addEventListener('visibilitychange', foreground);
    }
    return () => {
      alive.current = false;
      ++revision.current;
      window.removeEventListener('focus', foreground);
      document.removeEventListener('visibilitychange', foreground);
    };
  }, [enabled, execute, refresh]);

  useEffect(() => {
    if (!enabled || !isNative() || !['checking', 'downloading', 'installing', 'permissionRequired'].includes(status.phase)) return;
    const timer = setInterval(() => { if (!document.hidden) void refresh(); }, 1000);
    return () => clearInterval(timer);
  }, [enabled, status.phase, refresh]);

  return {
    status, pending,
    check: useCallback(() => execute('update_check', { automatic: false }), [execute]),
    download: useCallback(() => execute('update_download'), [execute]),
    cancel: useCallback(() => execute('update_cancel'), [execute]),
    install: useCallback(() => execute('update_install'), [execute]),
  };
}
