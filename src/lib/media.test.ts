import { expect, it } from 'vitest';
import { mediaKind } from './media';
it('recognizes video URLs with custom labels and keeps native format metadata', () => {
  expect(mediaKind({ id: 'x', sourceId: 's', name: 'Sunset', url: 'https://example.com/clip.mp4?token=public' })).toBe('video');
  expect(mediaKind({ id: 'x', sourceId: 's', name: 'Sunset', url: '/opaque', kind: 'gif' })).toBe('gif');
  expect(mediaKind({ id: 'x', sourceId: 's', name: 'photo.jpg', url: '/photo' })).toBe('image');
});
