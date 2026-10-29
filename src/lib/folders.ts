import type { Source, SourceFolder } from './types';

export function supportsFolders(source: Pick<Source, 'kind' | 'google' | 'location'>): boolean {
  return ['local', 'drive', 'webdav'].includes(source.kind) && source.google?.provider !== 'photos' && !source.location.startsWith('wallta-import://');
}
export function sourceFolders(source: Pick<Source, 'location' | 'folders' | 'google'>): SourceFolder[] {
  return source.folders?.length ? source.folders : source.location.trim() ? [{ location: source.location, name: source.google?.selectionName }] : [];
}
function normalizedLocation(source: Source, value: string): string {
  const location = value.trim();
  if (!location || location.length > 8192) throw new Error('Địa chỉ thư mục không hợp lệ.');
  if (source.kind === 'drive') {
    const id = /^https?:/.test(location) ? /\/folders\/([A-Za-z0-9_-]+)/.exec(location)?.[1] : location;
    if (!id || !/^[A-Za-z0-9_-]+$/.test(id)) throw new Error('Nhập link hoặc ID thư mục Google Drive.');
    return id;
  }
  if (source.kind === 'webdav') {
    const url = new URL(location);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.hash) throw new Error('Dùng URL WebDAV hợp lệ, không chứa mật khẩu.');
    const first = sourceFolders(source)[0];
    if (first && new URL(first.location).origin !== url.origin) throw new Error('Các thư mục WebDAV trong một nguồn phải cùng máy chủ.');
    return url.href;
  }
  return location;
}
export function setSourceFolders(source: Source, folders: SourceFolder[]): Source {
  return { ...source, folders, location: folders[0]?.location || '', enabled: folders.length ? source.enabled : false,
    google: source.google ? { ...source.google, selectionName: folders[0]?.name } : undefined };
}
export function addSourceFolder(source: Source, folder: SourceFolder): Source {
  const location = normalizedLocation(source, folder.location);
  const folders = sourceFolders(source);
  const duplicate = folders.some(item => {
    const existing = normalizedLocation(source, item.location);
    return source.kind === 'local' && /^[A-Za-z]:[\\/]/.test(location) ? existing.replaceAll('\\', '/').toLowerCase() === location.replaceAll('\\', '/').toLowerCase() : existing === location;
  });
  if (duplicate) return setSourceFolders(source, folders);
  if (folders.length >= 50) throw new Error('Tối đa 50 thư mục trong một nguồn.');
  return setSourceFolders({ ...source, enabled: true }, [...folders, { ...folder, location }]);
}
