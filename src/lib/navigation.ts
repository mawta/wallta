type Point = { x: number; y: number };
export function nearestInDirection(origin: Point, points: Point[], direction: string): number {
  let best = -1, score = Infinity;
  points.forEach((point, index) => {
    const dx = point.x - origin.x, dy = point.y - origin.y;
    const vertical = direction === 'ArrowDown' || direction === 'ArrowUp';
    const forward = direction === 'ArrowDown' ? dy : direction === 'ArrowUp' ? -dy : direction === 'ArrowRight' ? dx : -dx;
    if (forward <= 2) return;
    const perpendicular = Math.abs(vertical ? dx : dy);
    const candidate = forward + perpendicular * 4;
    if (candidate < score) { best = index; score = candidate; }
  });
  return best;
}

export function installDirectionalNavigation(): () => void {
  const handler = (event: KeyboardEvent) => {
    if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key) || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
    const active = document.activeElement as HTMLElement | null;
    if (active?.matches('input,select,textarea,[contenteditable=true]') || active?.closest('.slideshow')) return;
    const scope = document.querySelector('dialog[open], [role=dialog]') || document;
    const controls = Array.from(scope.querySelectorAll<HTMLElement>('button:not(:disabled),a[href],input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]')).filter(element => element.getClientRects().length > 0 && !element.closest('[inert]'));
    const center = (element: HTMLElement) => { const rect = element.getBoundingClientRect(); return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }; };
    const target = active && controls.includes(active) ? nearestInDirection(center(active), controls.map(center), event.key) : 0;
    if (target >= 0 && controls[target]) { event.preventDefault(); controls[target].focus(); controls[target].scrollIntoView({ block: 'nearest', inline: 'nearest' }); }
  };
  window.addEventListener('keydown', handler);
  return () => window.removeEventListener('keydown', handler);
}
