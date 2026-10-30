import { t, useLocale, type LanguagePreference } from '../lib/i18n';
import { ArrowUpRight, HardDrive, Monitor, ShieldCheck, Info, Play } from 'lucide-react';
import { callNative, isNative } from '../lib/native';
import { SectionTitle, SettingRow, Toggle } from '../components/ui';
import type { AppState, Capabilities, Source } from '../lib/types';
import type { UpdatesController } from '../lib/updates';
import { AppUpdates } from '../components/AppUpdates';
import { SavedGoogleConnections } from '../components/SavedGoogleConnections';
import { UpscaleStorage } from '../components/UpscaleStorage';

export function SettingsPage({ state, capabilities, onSave, busy, perform, updates, onManageSource, upscaleActive, onStorageChanged }: { state: AppState; capabilities: Capabilities | null; onSave: (state: AppState) => Promise<unknown>; busy: boolean; perform: (fn: () => Promise<void>) => Promise<unknown>; notify: (text: string, error?: boolean) => void; updates: UpdatesController; onManageSource: (source: Source) => void; upscaleActive: boolean; onStorageChanged: () => unknown }) {
  const { preference, setPreference } = useLocale();
  const update = (settings: Partial<AppState['settings']>) => void onSave({ ...state, settings: { ...state.settings, ...settings } });
  return <>
    <SectionTitle title={t("Cài đặt")} />
    <section className="panel settings-panel"><SettingRow title={t('Ngôn ngữ')}><select aria-label={t('Ngôn ngữ')} value={preference} onChange={event => setPreference(event.target.value as LanguagePreference)}><option value="system">{t('Theo thiết bị')}</option><option value="en">English</option><option value="vi">Tiếng Việt</option></select></SettingRow></section>
    <AppUpdates updates={updates} busy={busy} perform={perform} />
    <SavedGoogleConnections sources={state.sources} busy={busy} onOpen={onManageSource} />
    <section className="panel settings-panel"><h3><Monitor size={18} /> {t("Thiết bị")}</h3>
      <SettingRow title={t("Khởi động cùng thiết bị")} description={capabilities?.platform === 'windows' ? t("Chạy trong khay khi đăng nhập.") : t("Khôi phục lịch sau khi khởi động lại.")}><Toggle checked={state.settings.autostart} label={t("Khởi động cùng thiết bị")} onChange={() => update({ autostart: !state.settings.autostart })} disabled={busy || !isNative()} /></SettingRow>
      <SettingRow title={t("Chỉ tải ảnh qua Wi-Fi")}><Toggle checked={state.settings.wifiOnly} label={t("Chỉ tải ảnh qua Wi-Fi")} onChange={() => update({ wifiOnly: !state.settings.wifiOnly })} disabled={busy || !capabilities?.platform.startsWith('android')} /></SettingRow>
      {capabilities?.screensaver && <SettingRow title={t("Screensaver của TV")} description={t("Chọn Wallta trong menu hệ thống.")}><button className="button secondary" onClick={() => void perform(async () => { await callNative('open_screensaver_settings'); })}><Play size={15} />{t("Thiết lập")}</button></SettingRow>}
    </section>
    <section className="panel settings-panel"><h3><HardDrive size={18} /> {t("Lưu trữ")}</h3>
      <SettingRow title={t("Giới hạn cache")}><select aria-label={t("Giới hạn bộ nhớ cache")} value={state.settings.cacheLimitMb} onChange={event => update({ cacheLimitMb: Number(event.target.value) })} disabled={busy}>{[64,128,256,512,1024,2048].map(value => <option key={value} value={value}>{value >= 1024 ? `${value / 1024} GB` : `${value} MB`}</option>)}</select></SettingRow>
      <UpscaleStorage active={busy || upscaleActive} onChanged={onStorageChanged} />
    </section>
    <section className="panel settings-panel"><h3><ShieldCheck size={18} /> {t("Khả năng thiết bị")}</h3><div className="capability-grid"><div><span className="muted">{t("Nền tảng")}</span><strong>{capabilities?.platform === 'browser' ? t("Trình duyệt · Xem trước") : capabilities?.platform || t("Đang kiểm tra…")}</strong></div><div><span className="muted">{t("Hình nền hệ thống")}</span><strong>{capabilities?.wallpaper ? t("API khả dụng") : t("Không khả dụng")}</strong></div><div><span className="muted">Screensaver</span><strong>{capabilities?.screensaver ? t("Khả dụng") : t("Không khả dụng")}</strong></div></div>{capabilities?.notes.map(note => <p className="capability-note" key={t(note)}><Info size={15} /><span>{t(note)}</span></p>)}{capabilities?.platform === 'android-tv' && <p className="capability-note"><Info size={15} /><span>{t("Nền Home tùy launcher. Áp dụng thử rồi về Home để kiểm tra.")}</span></p>}</section>
    <div className="about-panel"><img src="./wallta.svg" alt="" /><div><strong>Wallta <span>{updates.status.currentVersion}</span></strong></div><button className="icon-button" aria-label={t("Mở kho mã Wallta")} onClick={() => void perform(async () => {
      if (isNative()) await callNative('open_external', { url: 'https://github.com/mawta/wallta' });
      else window.open('https://github.com/mawta/wallta', '_blank', 'noopener,noreferrer');
    })}><ArrowUpRight size={20} /></button></div>
  </>;
}
