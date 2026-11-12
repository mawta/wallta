(() => {
  'use strict';
  let media;
  let poster;
  let explicitlyPaused = false;
  const fail = () => { window.location.href = '/motion-player-error'; };
  const localAsset = value => {
    try { const url = new URL(value); return url.origin === 'http://asset.localhost' && !url.search && !url.hash; }
    catch { return false; }
  };
  window.walltaMotionConfigure = config => {
    if (!localAsset(config.url) || !localAsset(config.poster)) { fail(); return; }
    if (media) { media.pause?.(); media.removeAttribute('src'); media.remove(); }
    poster?.remove();
    const fit = config.fit === 'fit' ? 'contain' : config.fit === 'stretch' ? 'fill' : 'cover';
    poster = document.createElement('img');
    poster.src = config.poster;
    poster.style.objectFit = fit;
    poster.hidden = true;
    media = document.createElement(config.gif ? 'img' : 'video');
    media.style.objectFit = fit;
    if (!config.gif) {
      media.muted = true;
      media.defaultMuted = true;
      media.loop = true;
      media.autoplay = true;
      media.playsInline = true;
      media.controls = false;
      media.disablePictureInPicture = true;
      media.poster = config.poster;
    }
    media.onerror = fail;
    media.src = config.url;
    document.body.replaceChildren(media, poster);
    window.walltaMotionPause(explicitlyPaused);
  };
  window.walltaMotionPause = paused => {
    explicitlyPaused = Boolean(paused);
    if (!media) return;
    const hidden = explicitlyPaused || document.hidden;
    if (media.tagName === 'VIDEO') {
      if (hidden) media.pause();
      else media.play().catch(fail);
    } else {
      // Removing the GIF source stops its decode/animation work while paused.
      if (hidden && media.hasAttribute('src')) {
        media.dataset.source = media.src;
        media.removeAttribute('src');
      } else if (!hidden && !media.hasAttribute('src') && media.dataset.source) {
        media.src = media.dataset.source;
      }
      media.hidden = hidden;
      poster.hidden = !hidden;
    }
  };
  document.addEventListener('visibilitychange', () => window.walltaMotionPause(explicitlyPaused));
  window.addEventListener('pagehide', () => {
    media?.pause?.();
    media?.removeAttribute('src');
    media?.load?.();
    poster?.removeAttribute('src');
  });
  window.addEventListener('contextmenu', event => event.preventDefault());
  window.addEventListener('dragstart', event => event.preventDefault());
})();
