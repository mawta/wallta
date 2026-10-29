import { expect, it } from 'vitest';
import { addSourceFolder, setSourceFolders, sourceFolders } from './folders';
import { googleConnectionChanges } from './google';
import type { Source } from './types';
const source: Source = { id: 's', name: 'Drive', kind: 'drive', location: 'family', enabled: true, recursive: true, google: { provider: 'drive', connected: true, selectionName: 'Family' } };
it('upgrades legacy folders while deduplicating Drive links and keeping the first selection', () => {
  const next = addSourceFolder(source, { location: 'https://drive.google.com/drive/folders/trips?usp=sharing', name: 'Trips' });
  expect(next.location).toBe('family'); expect(sourceFolders(next)).toHaveLength(2);
  expect(sourceFolders(addSourceFolder(next, { location: 'trips' }))).toHaveLength(2);
  expect(setSourceFolders(next, [])).toMatchObject({ location: '', enabled: false });
});
it('keeps WebDAV credentials on the same server and limits folder count', () => {
  const webdav = { ...source, kind: 'webdav' as const, location: 'https://nas.test/photos/', google: undefined };
  expect(() => addSourceFolder(webdav, { location: 'https://other.test/photos/' })).toThrow('cùng máy chủ');
  expect(() => addSourceFolder({ ...source, folders: Array.from({ length: 50 }, (_, i) => ({ location: `f${i}` })) }, { location: 'extra' })).toThrow('50');
});
it('clears previously selected Drive folders when authorizing a new account', () => {
  const changes = googleConnectionChanges({ ...source, folders: [{ location: 'private-old', name: 'Old' }] }, { phase: 'ready', sourceKind: 'drive', account: 'new@example.com' });
  expect({ ...source, folders: [{ location: 'private-old' }], ...changes }.folders).toBeUndefined();
});
