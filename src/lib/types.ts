export type SourceKind = 'local' | 'url' | 'manifest' | 'webdav' | 'drive' | 'photos';
export interface SourceFolder { location: string; name?: string }
export interface Source {
  id: string;
  name: string;
  kind: SourceKind;
  location: string;
  folders?: SourceFolder[];
  recursive: boolean;
  enabled: boolean;
  username?: string;
  password?: string;
  apiKey?: string;
  accessToken?: string;
  google?: { provider: 'drive' | 'photos'; account?: string; selectionName?: string; connected: boolean; connectionId?: string };
}
export interface MediaItem { id: string; name: string; url: string; sourceId: string }
export interface Settings {
  intervalSeconds: number;
  slideshowSeconds: number;
  order: 'shuffle' | 'sequential';
  fit: 'fill' | 'fit' | 'stretch';
  blurBackground: boolean;
  autostart: boolean;
  wifiOnly: boolean;
  cacheLimitMb: number;
  scheduleEnabled: boolean;
  days: number[];
  startTime: string;
  endTime: string;
  showClock: boolean;
}
export interface AppState {
  version: 1;
  sources: Source[];
  settings: Settings;
  running: boolean;
  current: MediaItem | null;
  lastChanged: string | null;
  error: string | null;
}
export interface Capabilities {
  platform: 'windows' | 'android' | 'android-tv' | 'browser';
  wallpaper: boolean;
  screensaver: boolean;
  minIntervalSeconds: number;
  notes: string[];
}
export const defaultSettings: Settings = {
  intervalSeconds: 900, slideshowSeconds: 30, order: 'shuffle', fit: 'fill', blurBackground: false,
  autostart: false, wifiOnly: false, cacheLimitMb: 256,
  scheduleEnabled: false, days: [0, 1, 2, 3, 4, 5, 6], startTime: '07:00', endTime: '23:00', showClock: false,
};
export const emptyState: AppState = { version: 1, sources: [], settings: defaultSettings, running: false, current: null, lastChanged: null, error: null };
export const sourceLabels: Record<SourceKind, string> = { local: 'Thư mục thiết bị', url: 'Link ảnh', manifest: 'Thư mục online', webdav: 'WebDAV', drive: 'Google Drive', photos: 'Google Photos' };
