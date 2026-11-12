import { LockKeyhole, SkipForward } from 'lucide-react';
import { t } from '../lib/i18n';
import { intervalLabel, SectionTitle, SettingRow, Toggle } from '../components/ui';
import { MediaImage } from '../components/MediaImage';
import type { AppState, Capabilities, Settings } from '../lib/types';

export function LockScreen({ state, capabilities, busy, onSave, onNext }: {
  state: AppState;
  capabilities: Capabilities | null;
  busy: boolean;
  onSave: (state: AppState) => Promise<unknown>;
  onNext: () => Promise<unknown>;
}) {
  const supported = capabilities?.lockWallpaper ?? capabilities?.platform === 'android';
  const sources = state.sources.filter(source => source.enabled);
  const sourceId = sources.find(source => source.id === state.settings.lockSourceId)?.id ?? '';
  const currentSource = state.sources.find(source => source.id === state.lockCurrent?.sourceId);
  const minimum = capabilities?.minIntervalSeconds ?? 900;
  const intervals = [...new Set([minimum, 60, 300, 900, 1800, 3600, 10800, 21600, 86400].filter(value => value >= minimum))];
  const update = (settings: Partial<Settings>) => void onSave({ ...state, settings: { ...state.settings, ...settings } });

  return <>
    <SectionTitle title={t('Màn hình khóa')}>
      {supported && <button className="button primary" disabled={busy || !sourceId} onClick={() => void onNext()}>
        <SkipForward size={17} />{t('Đổi ảnh khóa ngay')}
      </button>}
    </SectionTitle>
    {!supported ? <section className="panel lock-unavailable">
      <LockKeyhole size={28} /><p>{t('Thiết bị này không có chức năng hình nền màn hình khóa.')}</p>
    </section> : <div className="overview-grid lock-layout">
      <section className="wallpaper-card">
        <div className="wallpaper-preview">
          <MediaImage item={state.lockCurrent} alt={state.lockCurrent?.name || t('Màn hình khóa')} />
          <div className="wallpaper-shade" />
          <span className="photo-badge"><LockKeyhole size={14} />{t('Màn hình khóa')}</span>
          <div className="wallpaper-caption"><span>{currentSource?.name}</span><h3>{state.lockCurrent?.name || t('Chưa có ảnh')}</h3></div>
        </div>
      </section>
      <section className="panel settings-panel">
        <SettingRow title={t('Nguồn ảnh')}>
          <select aria-label={t('Nguồn màn hình khóa')} value={sourceId} disabled={busy} onChange={event => update({ lockSourceId: event.target.value, ...(!event.target.value ? { lockRunning: false } : {}) })}>
            <option value="">{t('Chọn nguồn ảnh')}</option>
            {sources.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </SettingRow>
        <SettingRow title={t('Tự đổi:')}>
          <Toggle label={t('Tự đổi màn hình khóa')} checked={state.settings.lockRunning} disabled={busy || !sourceId} onChange={() => update({ lockRunning: !state.settings.lockRunning })} />
        </SettingRow>
        <SettingRow title={t('Chu kỳ đổi ảnh')}>
          <select aria-label={t('Chu kỳ màn hình khóa')} value={state.settings.lockIntervalSeconds} disabled={busy} onChange={event => update({ lockIntervalSeconds: Number(event.target.value) })}>
            {!intervals.includes(state.settings.lockIntervalSeconds) && <option value={state.settings.lockIntervalSeconds}>{intervalLabel(state.settings.lockIntervalSeconds)}</option>}
            {intervals.map(seconds => <option key={seconds} value={seconds}>{intervalLabel(seconds)}</option>)}
          </select>
        </SettingRow>
        <p className="muted small">{t(capabilities?.platform === 'windows' ? 'Video/GIF: dùng ảnh tĩnh.' : 'Video/GIF: dùng ảnh tĩnh. Hình nền động tùy hệ thống.')}</p>
        {state.lockError && <p className="form-error" role="alert">{t(state.lockError)}</p>}
      </section>
    </div>}
  </>;
}
