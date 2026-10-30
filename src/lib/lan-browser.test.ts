import { readFileSync } from 'node:fs';
import { JSDOM } from 'jsdom';
import { expect, it, vi } from 'vitest';

const html = readFileSync(new URL('../../shared/lan/index.html', import.meta.url), 'utf8');
const pageUrl = 'http://127.0.0.1:1234/' + 'a'.repeat(64) + '/';

function openPage(options: { language?: string; saved?: string; blockedStorage?: boolean; active?: boolean; outgoing?: { id: string; name: string }[] } = {}) {
  const fetch = vi.fn(async () => ({ ok: true, json: async () => ({ active: options.active ?? true, receivedCount: 2, outgoing: options.outgoing ?? [] }) }));
  const dom = new JSDOM(html, { url: pageUrl, runScripts: 'dangerously', beforeParse(window) {
    Object.defineProperty(window.navigator, 'language', { value: options.language ?? 'en-US', configurable: true });
    if (options.saved) window.localStorage.setItem('wallta-lan-language', options.saved);
    if (options.blockedStorage) Object.defineProperty(window, 'localStorage', { get() { throw new Error('Storage blocked'); } });
    window.fetch = fetch as unknown as typeof window.fetch;
  } });
  return { dom, fetch, doc: dom.window.document };
}

function chooseLanguage(dom: JSDOM, language: string) {
  const selector = dom.window.document.querySelector<HTMLSelectElement>('#language')!;
  expect(selector).not.toBeNull();
  selector.value = language;
  selector.dispatchEvent(new dom.window.Event('change', { bubbles: true }));
}

it('defaults to English and translates current status when switched without changing file names or token URLs', async () => {
  const name = 'Ảnh cưới <2026> & hello.png';
  const { dom, doc, fetch } = openPage({ outgoing: [{ id: 'photo id/+one', name }] });
  try {
    await vi.waitFor(() => expect(doc.querySelector('#send')!.hasAttribute('disabled')).toBe(false));
    expect(doc.documentElement.lang).toBe('en');
    expect(doc.title).toBe('Wallta — Send / receive photos');
    expect(doc.querySelector('h1')!.textContent).toBe('Wallta · Send / receive photos');
    expect(doc.querySelector('#message')!.textContent).toBe('Ready · received 2 photos.');
    expect(doc.querySelector('.file span')!.textContent).toBe(name);
    const originalUrl = doc.querySelector<HTMLAnchorElement>('.download')!.href;
    expect(originalUrl).toBe(pageUrl + 'files/photo%20id%2F%2Bone');
    const requests = fetch.mock.calls.length;
    chooseLanguage(dom, 'vi');
    expect(doc.documentElement.lang).toBe('vi');
    expect(doc.title).toBe('Wallta — Gửi / nhận ảnh');
    expect(doc.querySelector('#message')!.textContent).toBe('Sẵn sàng · đã nhận 2 ảnh.');
    expect(doc.querySelector('.download')!.textContent).toBe('Tải ảnh');
    expect(doc.querySelector('.file span')!.textContent).toBe(name);
    expect(doc.querySelector<HTMLAnchorElement>('.download')!.href).toBe(originalUrl);
    expect(fetch.mock.calls.length).toBe(requests);
    expect(dom.window.localStorage.getItem('wallta-lan-language')).toBe('vi');
  } finally { dom.window.close(); }
});

it.each(['vi', 'vi-VN'])('uses Vietnamese for primary browser language %s', async language => {
  const { dom, doc } = openPage({ language });
  try {
    await vi.waitFor(() => expect(doc.querySelector('#send')!.hasAttribute('disabled')).toBe(false));
    expect(doc.documentElement.lang).toBe('vi');
    expect(doc.querySelector<HTMLSelectElement>('#language')!.value).toBe('vi');
    expect(doc.querySelector('#message')!.textContent).toBe('Sẵn sàng · đã nhận 2 ảnh.');
  } finally { dom.window.close(); }
});

it('restores the saved language over the browser default', async () => {
  const { dom, doc } = openPage({ language: 'vi-VN', saved: 'en' });
  try {
    await vi.waitFor(() => expect(doc.querySelector('#send')!.hasAttribute('disabled')).toBe(false));
    expect(doc.documentElement.lang).toBe('en');
    expect(doc.querySelector('#message')!.textContent).toBe('Ready · received 2 photos.');
  } finally { dom.window.close(); }
});

it('falls back to English for other browser languages and remains switchable with blocked storage', async () => {
  const { dom, doc } = openPage({ language: 'ja-JP', blockedStorage: true, active: false });
  try {
    await vi.waitFor(() => expect(doc.querySelector('#message')!.textContent).toBe('The session has stopped. Start a new session in Wallta.'));
    expect(doc.documentElement.lang).toBe('en');
    chooseLanguage(dom, 'vi');
    expect(doc.documentElement.lang).toBe('vi');
    expect(doc.querySelector('#message')!.textContent).toBe('Phiên đã dừng. Mở phiên mới trong Wallta.');
    expect(doc.querySelector('#message')!.className).toBe('error');
    expect(doc.querySelector('#send')!.hasAttribute('disabled')).toBe(true);
  } finally { dom.window.close(); }
});

it('retries only the unsent tail after a partial browser upload', async () => {
  const uploaded: string[] = [];
  let failSecond = true;
  const dom = new JSDOM(html, { url: pageUrl, runScripts: 'dangerously', beforeParse(window) {
    Object.defineProperty(window.navigator, 'language', { value: 'vi-VN', configurable: true });
    window.fetch = (async (url: string) => {
      if (url.includes('/upload?')) {
        const name = new URL(url, window.location.href).searchParams.get('name')!;
        if (name === 'second.png' && failSecond) { failSecond = false; return { ok: false, status: 503 }; }
        uploaded.push(name);
        return { ok: true, json: async () => ({ ok: true }) };
      }
      return { ok: true, json: async () => ({ active: true, receivedCount: uploaded.length, outgoing: [] }) };
    }) as unknown as typeof window.fetch;
  } });
  try {
    const doc = dom.window.document;
    await vi.waitFor(() => expect(doc.querySelector('#send')!.hasAttribute('disabled')).toBe(false));
    Object.defineProperty(doc.querySelector('#photos'), 'files', { value: [new dom.window.File(['x'], 'first.png'), new dom.window.File(['y'], 'second.png')], configurable: true });
    const submit = () => doc.querySelector('form')!.dispatchEvent(new dom.window.Event('submit', { bubbles: true, cancelable: true }));
    submit();
    await vi.waitFor(() => expect(doc.querySelector('#message')!.textContent).toContain('Đã gửi 1/2'));
    expect(uploaded).toEqual(['first.png']);
    chooseLanguage(dom, 'en');
    expect(doc.querySelector('#message')!.textContent).toContain('Sent 1/2 photos.');
    expect(doc.querySelector('#message')!.textContent).toContain('HTTP 503');
    expect(doc.querySelector('#send')!.textContent).toBe('Send remaining photos');
    submit();
    await vi.waitFor(() => expect(uploaded).toContain('second.png'));
    expect(uploaded).toEqual(['first.png', 'second.png']);
  } finally { dom.window.close(); }
});
