import { readFileSync } from 'node:fs';
import { JSDOM } from 'jsdom';
import { expect, it, vi } from 'vitest';

it('retries only the unsent tail after a partial browser upload', async () => {
  const uploaded: string[] = [];
  let failSecond = true;
  const html = readFileSync(new URL('../../shared/lan/index.html', import.meta.url), 'utf8');
  const dom = new JSDOM(html, { url: 'http://127.0.0.1:1234/' + 'a'.repeat(64) + '/', runScripts: 'dangerously', beforeParse(window) {
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
    submit();
    await vi.waitFor(() => expect(uploaded).toContain('second.png'));
    expect(uploaded).toEqual(['first.png', 'second.png']);
  } finally { dom.window.close(); }
});
