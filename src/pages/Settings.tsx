import { ArrowUpRight, HardDrive, Monitor, ShieldCheck, Trash2, Info, Play } from 'lucide-react';
import { callNative, isNative } from '../lib/native';
import { SectionTitle, SettingRow, Toggle } from '../components/ui';
import type { AppState, Capabilities, Source } from '../lib/types';
import type { UpdatesController } from '../lib/updates';
import { AppUpdates } from '../components/AppUpdates';
import { SavedGoogleConnections } from '../components/SavedGoogleConnections';

export function SettingsPage({ state, capabilities, onSave, busy, perform, notify, updates, onManageSource }: { state: AppState; capabilities: Capabilities | null; onSave: (state: AppState) => Promise<unknown>; busy: boolean; perform: (fn: () => Promise<void>) => Promise<unknown>; notify: (text: string, error?: boolean) => void; updates: UpdatesController; onManageSource: (source: Source) => void }) {
  const update = (settings: Partial<AppState['settings']>) => void onSave({ ...state, settings: { ...state.settings, ...settings } });
  return <>
    <SectionTitle title="Cài đặt" />
    <AppUpdates updates={updates} busy={busy} perform={perform} />
    <SavedGoogleConnections sources={state.sources} busy={busy} onOpen={onManageSource} />
    <section className="panel settings-panel"><h3><Monitor size={18} /> Thiết bị</h3>
      <SettingRow title="Khởi động cùng thiết bị" description={capabilities?.platform === 'windows' ? 'Chạy trong khay khi đăng nhập.' : 'Khôi phục lịch sau khi khởi động lại.'}><Toggle checked={state.settings.autostart} label="Khởi động cùng thiết bị" onChange={() => update({ autostart: !state.settings.autostart })} disabled={busy || !isNative()} /></SettingRow>
      <SettingRow title="Chỉ tải ảnh qua Wi-Fi"><Toggle checked={state.settings.wifiOnly} label="Chỉ tải ảnh qua Wi-Fi" onChange={() => update({ wifiOnly: !state.settings.wifiOnly })} disabled={busy || !capabilities?.platform.startsWith('android')} /></SettingRow>
      {capabilities?.screensaver && <SettingRow title="Screensaver của TV" description="Chọn Wallta trong menu hệ thống."><button className="button secondary" onClick={() => void perform(async () => { await callNative('open_screensaver_settings'); })}><Play size={15} />Thiết lập</button></SettingRow>}
    </section>
    <section className="panel settings-panel"><h3><HardDrive size={18} /> Lưu trữ</h3>
      <SettingRow title="Giới hạn cache"><select aria-label="Giới hạn bộ nhớ cache" value={state.settings.cacheLimitMb} onChange={event => update({ cacheLimitMb: Number(event.target.value) })} disabled={busy}>{[64,128,256,512,1024,2048].map(value => <option key={value} value={value}>{value >= 1024 ? `${value / 1024} GB` : `${value} MB`}</option>)}</select></SettingRow>
      <SettingRow title="Dọn ảnh đã tải" description="Giữ ảnh gốc; tải lại cache khi cần."><button className="button secondary" disabled={busy || !isNative()} onClick={() => void perform(async () => { await callNative('clear_cache'); notify('Đã dọn ảnh trong bộ nhớ đệm.'); })}><Trash2 size={15} />Dọn bộ nhớ</button></SettingRow>
    </section>
    <section className="panel settings-panel"><h3><ShieldCheck size={18} /> Khả năng thiết bị</h3><div className="capability-grid"><div><span className="muted">Nền tảng</span><strong>{capabilities?.platform === 'browser' ? 'Trình duyệt · Xem trước' : capabilities?.platform || 'Đang kiểm tra…'}</strong></div><div><span className="muted">Hình nền hệ thống</span><strong>{capabilities?.wallpaper ? 'API khả dụng' : 'Không khả dụng'}</strong></div><div><span className="muted">Screensaver</span><strong>{capabilities?.screensaver ? 'Khả dụng' : 'Không khả dụng'}</strong></div></div>{capabilities?.notes.map(note => <p className="capability-note" key={note}><Info size={15} /><span>{note}</span></p>)}{capabilities?.platform === 'android-tv' && <p className="capability-note"><Info size={15} /><span>Nền Home tùy launcher. Áp dụng thử rồi về Home để kiểm tra.</span></p>}</section>
    <div className="about-panel"><img src="./wallta.svg" alt="" /><div><strong>Wallta <span>{updates.status.currentVersion}</span></strong></div><button className="icon-button" aria-label="Mở kho mã Wallta" onClick={() => void perform(async () => {
      if (isNative()) await callNative('open_external', { url: 'https://github.com/mawta/wallta' });
      else window.open('https://github.com/mawta/wallta', '_blank', 'noopener,noreferrer');
    })}><ArrowUpRight size={20} /></button></div>
  </>;
}
