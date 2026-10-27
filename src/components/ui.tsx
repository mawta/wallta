import type { ReactNode } from 'react';
import { Check, FolderOpen, Globe2, HardDrive, Images, Cloud, type LucideIcon } from 'lucide-react';
import type { SourceKind } from '../lib/types';

export function Toggle({ checked, onChange, label, disabled = false }: { checked: boolean; onChange: () => void; label: string; disabled?: boolean }) {
  return <button type="button" className={`toggle ${checked ? 'on' : ''}`} role="switch" aria-checked={checked} aria-label={label} onClick={onChange} disabled={disabled}><span>{checked && <Check size={11} />}</span></button>;
}
export function SourceIcon({ kind, size = 22 }: { kind: SourceKind; size?: number }) {
  if (kind === 'drive') return <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"><path fill="#54ae75" d="m8 2 4 7-7 12-4-7z"/><path fill="#edc45f" d="M8 2h8l7 12h-8z"/><path fill="#72a9e7" d="m5 21 4-7h14l-4 7z"/></svg>;
  if (kind === 'photos') return <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"><path fill="#e7ae54" d="M12 12H2a5 5 0 0 1 10 0"/><path fill="#e56f66" d="M12 12V2a5 5 0 0 1 0 10"/><path fill="#7da8e0" d="M12 12h10a5 5 0 0 1-10 0"/><path fill="#69b789" d="M12 12v10a5 5 0 0 1 0-10"/></svg>;
  const Icon = ({ local: FolderOpen, url: Globe2, manifest: Images, webdav: Cloud } as Record<string, LucideIcon>)[kind] || HardDrive;
  return <Icon size={size} strokeWidth={1.6} />;
}
export function SettingRow({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return <div className="setting-row"><div><strong>{title}</strong>{description && <p>{description}</p>}</div><div className="setting-control">{children}</div></div>;
}
export function SectionTitle({ eyebrow, title, description, children }: { eyebrow?: string; title: string; description?: string; children?: ReactNode }) {
  return <div className="section-heading"><div>{eyebrow && <span className="eyebrow">{eyebrow}</span>}<h2>{title}</h2>{description && <p>{description}</p>}</div>{children}</div>;
}
export const intervalLabel = (seconds: number) => seconds < 60 ? `${seconds} giây` : seconds < 3600 ? `${Math.round(seconds / 60)} phút` : `${Number((seconds / 3600).toFixed(1))} giờ`;
