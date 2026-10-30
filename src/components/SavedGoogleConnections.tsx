import { t } from '../lib/i18n';
import { Check, UserRound } from 'lucide-react';
import { sourceLabels, type Source } from '../lib/types';

export function SavedGoogleConnections({ sources, provider, busy, action = t("Quản lý"), onOpen }: {
  sources: Source[];
  provider?: 'drive' | 'photos';
  busy: boolean;
  action?: string;
  onOpen: (source: Source) => void;
}) {
  const connections = sources.filter(source => source.google?.connected && (!provider || source.google.provider === provider));
  if (!connections.length) return null;
  return <section className="saved-google-connections" aria-label={t("Tài khoản đã kết nối")}>
    <h3><UserRound size={18} />{t("Tài khoản đã kết nối")}</h3>
    {connections.map(source => <div className="saved-google-account" key={source.id}>
      <div><strong>{source.google?.account || source.name}</strong><small>{t(sourceLabels[source.google!.provider])} · <Check size={12} /> {t("Đã kết nối")}</small><small>{source.google?.selectionName || (source.location ? source.name : t("Chưa chọn thư mục"))}</small></div>
      <button type="button" className="button secondary" disabled={busy} onClick={() => onOpen(source)}>{action}</button>
    </div>)}
  </section>;
}
