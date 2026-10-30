import { t } from './i18n';
import type { AppState, MediaItem, Source } from './types';
export interface SaveSourceOptions { apply: boolean; autostart: boolean }
type Command = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

export async function saveSourceSelection(state: AppState, source: Source, options: SaveSourceOptions, call: Command, onSaved: (state: AppState) => void, onMedia?: (media: MediaItem[]) => void) {
  const saved = await call('save_state', { state: { ...state,
    sources: state.sources.some(s => s.id === source.id) ? state.sources.map(s => s.id === source.id ? source : s) : [...state.sources, source],
    settings: { ...state.settings, autostart: options.apply ? options.autostart : state.settings.autostart },
  } }) as AppState;
  onSaved(saved); // An import/connection is committed even if scanning or applying later fails.
  const media = await call('list_media') as MediaItem[];
  onMedia?.(media);
  const refreshed = await call('get_state') as AppState;
  onSaved(refreshed);
  if (!options.apply) return { state: refreshed, media };
  if (!media.some(item => item.sourceId === source.id)) throw new Error(t("Đã lưu nguồn. {0}", { 0: t(refreshed.error || "Thư mục chưa có ảnh khả dụng.") }));
  const applied = await call('next_wallpaper', { sourceId: source.id, startRunning: true }) as AppState;
  onSaved(applied);
  if (applied.current?.sourceId !== source.id || !applied.running) throw new Error(t("Đã lưu nguồn. {0}", { 0: t(applied.error || "Chưa áp dụng được ảnh. Thử Đổi ảnh ngay.") }));
  return { state: applied, media };
}
