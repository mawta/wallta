// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
const playerScript = readFileSync('public/motion-player.js', 'utf8');

type Player = Window & {
  walltaMotionConfigure(config: { url: string; poster: string; gif: boolean; fit: string }): void;
  walltaMotionPause(paused: boolean): void;
};
const player = window as unknown as Player;
const source = 'http://asset.localhost/C%3A%5Ccache%5Cmotion.mp4';
const poster = 'http://asset.localhost/C%3A%5Ccache%5Cmotion.poster.png';

beforeEach(() => {
  document.body.replaceChildren();
  Object.defineProperty(document, 'hidden', { configurable: true, value: false });
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {});
  window.eval(playerScript);
});

it('loops muted local video and applies its fit setting', () => {
  player.walltaMotionConfigure({ url: source, poster, gif: false, fit: 'fit' });
  const video = document.querySelector('video')!;
  expect(video.src).toBe(source);
  expect(video.muted).toBe(true);
  expect(video.loop).toBe(true);
  expect(video.controls).toBe(false);
  expect(video.style.objectFit).toBe('contain');
  player.walltaMotionPause(true);
  expect(video.pause).toHaveBeenCalled();
});

it('stops GIF decoding while paused and displays the still poster', () => {
  player.walltaMotionConfigure({ url: source.replace('.mp4', '.gif'), poster, gif: true, fit: 'fill' });
  const [gif, still] = document.querySelectorAll('img');
  const original = gif.src;
  player.walltaMotionPause(true);
  expect(gif.hasAttribute('src')).toBe(false);
  expect(gif.hidden).toBe(true);
  expect(still.hidden).toBe(false);
  player.walltaMotionPause(false);
  expect(gif.src).toBe(original);
  expect(gif.hidden).toBe(false);
  expect(still.hidden).toBe(true);
});

it('releases video sources when the wallpaper page closes', () => {
  player.walltaMotionConfigure({ url: source, poster, gif: false, fit: 'fill' });
  const video = document.querySelector('video')!;
  window.dispatchEvent(new Event('pagehide'));
  expect(video.hasAttribute('src')).toBe(false);
  expect(video.load).toHaveBeenCalled();
});
