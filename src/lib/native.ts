import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { parseManifest, selectNext } from './rules';
import { persistablePreview, restorePreview } from './preview';
import type { AppState, Capabilities, MediaItem, Source } from './types';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    WalltaAndroid?: { postMessage: (message: string) => void };
    __walltaReply?: (id: string, success: boolean, value: unknown) => void;
  }
}
export const isDesktop = () => Boolean(window.__TAURI_INTERNALS__);
export const isAndroid = () => Boolean(window.WalltaAndroid);
export const isNative = () => isDesktop() || isAndroid();
const pending = new Map<string, { resolve: (value: unknown) => void; reject: (reason: Error) => void; timer: ReturnType<typeof setTimeout> }>();
window.__walltaReply = (id, success, value) => {
  const request = pending.get(id);
  if (!request) return;
  clearTimeout(request.timer);
  pending.delete(id);
  if (success) request.resolve(value);
  else request.reject(new Error(typeof value === 'string' ? value : String((value as { message?: string })?.message || 'Không thực hiện được thao tác.')));
};
let previewState: AppState | undefined;
const localMedia = new Map<string, MediaItem[]>();
function getPreview(): AppState {
  if (!previewState) {
    let saved: string | null = null;
    try { saved = localStorage.getItem('wallta.preview.v1'); } catch { /* Storage may be unavailable. */ }
    previewState = restorePreview(saved);
  }
  return previewState;
}
export function registerPreviewFiles(id: string, files: FileList | File[]): number {
  for (const item of localMedia.get(id) || []) URL.revokeObjectURL(item.url);
  const images = Array.from(files).filter(file => /\.(jpe?g|png|webp)$/i.test(file.name));
  localMedia.set(id, images.map((file, index) => ({ id: `${id}:${index}`, name: file.name, url: URL.createObjectURL(file), sourceId: id })));
  return images.length;
}
async function previewMedia(source: Source): Promise<MediaItem[]> {
  if (source.kind === 'local') {
    const images = localMedia.get(source.id);
    if (!images) throw new Error(`${source.name}: chọn lại thư mục sau khi tải lại bản xem trước.`);
    return images;
  }
  if (source.kind === 'url') return [{ id: source.id + ':image', name: source.name, url: source.location, sourceId: source.id }];
  if (source.kind !== 'manifest') throw new Error(`${source.name}: dùng bản cài Wallta để truy cập nguồn ${source.kind}.`);
  const response = await fetch(source.location, { signal: AbortSignal.timeout(20000) });
  if (!response.ok) throw new Error(`${source.name}: HTTP ${response.status}`);
  const reader = response.body?.getReader();
  if (!reader) throw new Error('Nguồn không trả về nội dung.');
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    bytes += value.byteLength;
    if (bytes > 8 * 1024 * 1024) { await reader.cancel(); throw new Error('Danh sách vượt giới hạn 8 MB.'); }
    chunks.push(value);
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) { data.set(chunk, offset); offset += chunk.byteLength; }
  return parseManifest(JSON.parse(new TextDecoder().decode(data)), source.location, source.id);
}
async function browserCall(command: string, args: Record<string, unknown>): Promise<unknown> {
  const state = getPreview();
  const save = () => {
    try { localStorage.setItem('wallta.preview.v1', JSON.stringify(persistablePreview(state))); } catch { /* Preview remains usable without storage. */ }
    return structuredClone(state);
  };
  switch (command) {
    case 'get_state': return structuredClone(state);
    case 'get_capabilities': return { platform: 'browser', wallpaper: false, screensaver: false, minIntervalSeconds: 5, notes: ['Bản xem trước trong trình duyệt. Cài Wallta để đổi hình nền hệ thống và chạy nền.', 'Thư mục local chỉ khả dụng trong phiên đang mở. Nguồn online cần cho phép CORS.'] } satisfies Capabilities;
    case 'save_state': {
      const next = args.state as AppState;
      Object.assign(state, next);
      return save();
    }
    case 'set_running': state.running = Boolean(args.running); return save();
    case 'list_media': {
      const items: MediaItem[] = [], errors: string[] = [];
      for (const source of state.sources.filter(source => source.enabled)) {
        try { items.push(...await previewMedia(source)); } catch (error) { errors.push(errorMessage(error)); }
      }
      state.error = errors.join('\n') || null;
      return items;
    }
    case 'next_wallpaper': {
      const media = await browserCall('list_media', {}) as MediaItem[];
      const next = selectNext(media, state.current?.id || null, state.settings.order);
      if (!next) throw new Error(state.error || 'Chưa có ảnh khả dụng. Thêm hoặc bật nguồn ảnh trước.');
      state.current = next;
      state.lastChanged = new Date().toISOString();
      return save();
    }
    case 'clear_cache': return undefined;
    case 'pick_folder': throw new Error('Dùng nút chọn thư mục trong bản xem trước.');
    default: throw new Error('Tính năng này cần bản cài Wallta.');
  }
}
export async function callNative<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isDesktop()) return invoke<T>(command, args);
  if (isAndroid()) return new Promise<T>((resolve, reject) => {
    const id = crypto.randomUUID();
    const timer = setTimeout(() => { pending.delete(id); reject(new Error('Thiết bị phản hồi quá lâu. Hãy thử lại.')); }, command === 'pick_folder' ? 180000 : 90000);
    pending.set(id, { resolve: value => resolve(value as T), reject, timer });
    try { window.WalltaAndroid!.postMessage(JSON.stringify({ id, command, args })); }
    catch (error) { clearTimeout(timer); pending.delete(id); reject(error); }
  });
  return browserCall(command, args) as Promise<T>;
}
export function imageUrl(url?: string): string {
  if (!url) return './landscape.svg';
  if (/^(https?:|data:|blob:|asset:)/.test(url)) return url;
  return isDesktop() ? convertFileSrc(url.replace(/^file:\/\//, '')) : url;
}
export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : typeof error === 'string' ? error : 'Có lỗi xảy ra. Hãy thử lại.';
}
