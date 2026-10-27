// @vitest-environment jsdom
import { act, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { SourceDialog } from './SourceDialog';

it('keeps an OAuth source mounted while saving, including Escape and close buttons', async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute('open'); };
  const closed = vi.fn();
  let finish!: (value: boolean) => void;
  const saving = new Promise<boolean>(resolve => { finish = resolve; });
  function Harness() {
    const [busy, setBusy] = useState(false);
    return <SourceDialog busy={busy} initial={{ id: 'source', name: 'Test', kind: 'url', location: 'https://example.com/image.jpg', recursive: false, enabled: true }} onClose={closed} onSave={async () => { setBusy(true); const result = await saving; setBusy(false); return result; }} />;
  }
  const host = document.createElement('div'); document.body.append(host);
  const root = createRoot(host);
  try {
    await act(async () => root.render(<Harness />));
    await act(async () => host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    await act(async () => {
      host.querySelector<HTMLButtonElement>('[aria-label="Đóng"]')!.click();
      Array.from(host.querySelectorAll('button')).find(button => button.textContent === 'Để sau')!.click();
      host.querySelector('dialog')!.dispatchEvent(new Event('cancel', { bubbles: true, cancelable: true }));
    });
    expect(closed).not.toHaveBeenCalled();
    await act(async () => finish(false));
    await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="Đóng"]')!.click());
    expect(closed).toHaveBeenCalledOnce();
  } finally { await act(async () => root.unmount()); host.remove(); }
});
