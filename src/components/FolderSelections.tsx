import { FolderOpen, X } from 'lucide-react';
import type { SourceFolder } from '../lib/types';

export function FolderSelections({ folders, busy, onRemove }: { folders: SourceFolder[]; busy: boolean; onRemove: (index: number) => void }) {
  if (!folders.length) return null;
  return <section className="folder-selections" aria-label="Thư mục đã chọn"><strong>{folders.length} thư mục</strong>{folders.map((folder, index) => {
    const name = folder.name || folder.location;
    return <div key={folder.location}><FolderOpen size={16} /><div><span>{name}</span>{folder.name && <small>{folder.location}</small>}</div><button type="button" className="icon-button" aria-label={`Bỏ thư mục ${name}`} disabled={busy} onClick={() => onRemove(index)}><X size={15} /></button></div>;
  })}</section>;
}
