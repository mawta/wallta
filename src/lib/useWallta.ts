import { cloneJson } from './compat';
import { useCallback, useEffect, useRef, useState } from 'react';
import { callNative, errorMessage, isNative } from './native';
import { isWithinSchedule } from './rules';
import { emptyState, defaultSettings, type AppState, type Capabilities, type MediaItem } from './types';

const normalize = (state: AppState): AppState => ({ ...emptyState, ...state, settings: { ...defaultSettings, ...state.settings } });
export function useWallta() {
  const [state, setState] = useState<AppState>(cloneJson(emptyState));
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [media, setMedia] = useState<MediaItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(null);
  const lock = useRef(false);
  const notify = useCallback((text: string, error = false) => setNotice({ text, error }), []);
  const refresh = useCallback(async () => {
    const items = await callNative<MediaItem[]>('list_media');
    setMedia(items);
    setState(normalize(await callNative<AppState>('get_state')));
  }, []);
  const perform = useCallback(async (operation: () => Promise<void>) => {
    if (lock.current) return false;
    lock.current = true; setBusy(true);
    try { await operation(); return true; }
    catch (error) { notify(errorMessage(error), true); return false; }
    finally { lock.current = false; setBusy(false); }
  }, [notify]);
  useEffect(() => {
    let disposed = false;
    Promise.all([callNative<AppState>('get_state'), callNative<Capabilities>('get_capabilities')])
      .then(([saved, caps]) => { if (!disposed) { setState(normalize(saved)); setCapabilities(caps); setLoaded(true); } })
      .catch(error => { if (!disposed) { notify(errorMessage(error), true); setLoaded(true); } });
    return () => { disposed = true; };
  }, [notify]);
  useEffect(() => {
    if (!loaded || !capabilities) return;
    void perform(refresh);
  }, [loaded, capabilities, perform, refresh]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), notice.error ? 12000 : 4000);
    return () => clearTimeout(timer);
  }, [notice]);
  const save = async (next: AppState, message = 'Đã lưu thay đổi.') => perform(async () => {
    setState(normalize(await callNative<AppState>('save_state', { state: next })));
    await refresh(); notify(message);
  });
  const next = useCallback(() => perform(async () => {
    const changed = normalize(await callNative<AppState>('next_wallpaper'));
    setState(changed);
    if (changed.error) { notify(changed.error, true); return; }
    notify(isNative() ? 'Đã cập nhật hình nền.' : 'Đã đổi ảnh trong bản xem trước.');
  }), [perform, notify]);
  const toggle = () => perform(async () => {
    if (!state.running && !media.length) throw new Error('Thêm ít nhất một nguồn có ảnh trước khi bật tự động.');
    setState(normalize(await callNative<AppState>('set_running', { running: !state.running })));
  });
  useEffect(() => {
    if (!state.running || isNative()) return;
    const timer = setInterval(() => {
      if (isWithinSchedule(state.settings)) void next();
    }, state.settings.intervalSeconds * 1000);
    return () => clearInterval(timer);
  }, [state.running, state.settings, next]);
  useEffect(() => {
    if (!loaded || !isNative()) return;
    const timer = setInterval(() => {
      if (lock.current) return;
      callNative<AppState>('get_state').then(saved => { if (!lock.current) setState(normalize(saved)); }).catch(() => {});
    }, 5000);
    return () => clearInterval(timer);
  }, [loaded]);
  return { state, capabilities, media, busy, loaded, notice, notify, save, next, toggle, refresh: () => perform(refresh), perform };
}
