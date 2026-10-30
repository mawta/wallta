import { t } from '../lib/i18n';
import { Download, RefreshCw } from 'lucide-react';
import type { UpdatesController } from '../lib/updates';

export function AppUpdates({ updates, busy, perform }: { updates: UpdatesController; busy: boolean; perform: (fn: () => Promise<void>) => Promise<unknown> }) {
  const { status, pending } = updates;
  const { phase } = status;
  const active = phase === 'checking' || phase === 'downloading' || phase === 'installing';
  const locked = pending || busy;
  const label = {
    idle: '', checking: t("Đang kiểm tra…"), available: t("Có bản mới"), downloading: t("Đang tải…"), ready: t("Sẵn sàng cài"),
    installing: t("Đang mở bộ cài…"), upToDate: t("Đã mới nhất"), error: '', permissionRequired: t("Cho phép Wallta cài ứng dụng, rồi tiếp tục."), unsupported: t("Dùng bản cài để cập nhật."),
  }[phase];
  const total = status.totalBytes && status.totalBytes > 0 ? status.totalBytes : undefined;
  const progress = Math.max(0, Math.min(status.downloadedBytes, total || Number.MAX_SAFE_INTEGER));
  return <section className="panel settings-panel app-updates" aria-label={t("Cập nhật Wallta")}>
    <h3><Download size={18} />{t("Cập nhật")} <span className="update-current">{status.currentVersion}</span></h3>
    <div className="update-row">
      <div className="update-status" role="status" aria-live="polite">
        {status.version && status.version !== status.currentVersion && <strong>Wallta {status.version}</strong>}
        {label && <span className="muted">{label}</span>}
      </div>
      {phase !== 'unsupported' && <div className="update-actions">
        {phase !== 'downloading' && phase !== 'installing' && <button className="button secondary" disabled={locked || active} onClick={() => void updates.check()}><RefreshCw size={15} className={phase === 'checking' ? 'spin' : ''} />{t("Kiểm tra")}</button>}
        {phase === 'available' && <button className="button primary" disabled={locked} onClick={() => void updates.download()}>{t("Tải bản mới")}</button>}
        {(phase === 'ready' || phase === 'permissionRequired') && <button className="button primary" disabled={locked} onClick={() => void perform(updates.install)}>{phase === 'permissionRequired' ? t("Tiếp tục cài") : t("Cài bản cập nhật")}</button>}
        {(phase === 'downloading' || phase === 'ready' || phase === 'permissionRequired') && <button className="button secondary" disabled={pending} onClick={() => void updates.cancel()}>{t("Hủy tải")}</button>}
      </div>}
    </div>
    {phase === 'downloading' && <div className="update-progress"><progress aria-label={t("Tiến độ tải bản cập nhật")} max={total || 1} value={total ? progress : undefined} /><span>{(progress / 1048576).toFixed(1)}{total ? ` / ${(total / 1048576).toFixed(1)}` : ''} MB</span></div>}
    {status.error && <p className="update-error" role="alert">{t(status.error)}</p>}
    {status.notes && status.version && <details className="update-notes"><summary>{t("Thay đổi")}</summary><p>{status.notes.slice(0, 2000)}</p></details>}
  </section>;
}
