import { useEffect, useState } from 'react';
import { Check, Shuffle, ListOrdered, Maximize2, Minimize2, StretchHorizontal, Clock3 } from 'lucide-react';
import { defaultSettings, type Capabilities, type Settings } from '../lib/types';
import { clampSettings } from '../lib/rules';
import { SectionTitle, SettingRow, Toggle, intervalLabel } from '../components/ui';

export function Schedule({ settings, capabilities, onSave, busy }: { settings: Settings; capabilities: Capabilities | null; onSave: (settings: Settings) => Promise<unknown>; busy: boolean }) {
  const [draft, setDraft] = useState(settings);
  const [error, setError] = useState('');
  const settingsKey = JSON.stringify(settings);
  useEffect(() => { setDraft(settings); }, [settingsKey]);
  const set = (change: Partial<Settings>) => setDraft(previous => ({ ...previous, ...change }));
  const minimum = capabilities?.minIntervalSeconds || 5;
  const intervals = [...new Set([minimum, 60, 300, 900, 1800, 3600, 10800, 21600, 86400].filter(interval => interval >= minimum))];
  const save = () => {
    if (draft.scheduleEnabled && !draft.days.length) { setError('Chọn ít nhất một ngày hoạt động.'); return; }
    setError(''); void onSave(clampSettings(draft, minimum));
  };
  return <>
    <SectionTitle title="Lịch & hiển thị"><button className="button primary" onClick={save} disabled={busy}><Check size={17} />Lưu cài đặt</button></SectionTitle>
    <section className="panel settings-panel"><h3><Clock3 size={18} /> Nhịp đổi ảnh</h3>
      <SettingRow title="Tự đổi sau mỗi" description={capabilities?.platform.startsWith('android') ? 'Android: tối thiểu 15 phút; có thể trễ do tiết kiệm pin.' : undefined}><select aria-label="Khoảng thời gian đổi ảnh" value={draft.intervalSeconds} onChange={event => set({ intervalSeconds: Number(event.target.value) })}>{!intervals.includes(draft.intervalSeconds) && <option value={draft.intervalSeconds}>{intervalLabel(draft.intervalSeconds)}</option>}{intervals.map(seconds => <option value={seconds} key={seconds}>{intervalLabel(seconds)}</option>)}</select></SettingRow>
      <SettingRow title="Trình chiếu & screensaver" description="Chu kỳ riêng khi trình chiếu."><select aria-label="Khoảng thời gian trình chiếu" value={draft.slideshowSeconds} onChange={event => set({ slideshowSeconds: Number(event.target.value) })}>{[5,10,15,30,60,120,300].map(seconds => <option key={seconds} value={seconds}>{intervalLabel(seconds)}</option>)}</select></SettingRow>
      <SettingRow title="Thứ tự ảnh" description="Ngẫu nhiên: hết một vòng mới lặp."><div className="segmented"><button aria-pressed={draft.order === 'shuffle'} className={draft.order === 'shuffle' ? 'active' : ''} onClick={() => set({ order: 'shuffle' })}><Shuffle size={16} />Ngẫu nhiên</button><button aria-pressed={draft.order === 'sequential'} className={draft.order === 'sequential' ? 'active' : ''} onClick={() => set({ order: 'sequential' })}><ListOrdered size={16} />Tuần tự</button></div></SettingRow>
    </section>
    <section className="panel settings-panel"><h3><Clock3 size={18} /> Khung giờ hoạt động</h3>
      <SettingRow title="Chỉ đổi ảnh theo lịch" description="Ngoài lịch: giữ nguyên hình nền."><Toggle checked={draft.scheduleEnabled} label="Bật lịch hoạt động" onChange={() => set({ scheduleEnabled: !draft.scheduleEnabled })} /></SettingRow>
      {draft.scheduleEnabled && <div className="schedule-details"><div className="weekdays">{['CN','T2','T3','T4','T5','T6','T7'].map((label, day) => <button aria-pressed={draft.days.includes(day)} className={draft.days.includes(day) ? 'selected' : ''} key={day} onClick={() => set({ days: draft.days.includes(day) ? draft.days.filter(value => value !== day) : [...draft.days, day] })}>{label}</button>)}</div><div className="field-pair"><label className="field">Bắt đầu<input type="time" value={draft.startTime} onChange={event => set({ startTime: event.target.value })} /></label><label className="field">Kết thúc<input type="time" value={draft.endTime} onChange={event => set({ endTime: event.target.value })} /></label></div><p className="muted small">Hỗ trợ qua đêm. Giờ bắt đầu = kết thúc: cả ngày.</p></div>}
    </section>
    <section className="panel settings-panel"><h3><Maximize2 size={18} /> Hiển thị ảnh</h3><div className="fit-options">{([{ value: 'fill', label: 'Phủ kín', description: 'Có cắt ảnh', Icon: Maximize2 }, { value: 'fit', label: 'Vừa ảnh', description: 'Giữ tỉ lệ, không cắt', Icon: Minimize2 }, { value: 'stretch', label: 'Kéo giãn', description: 'Có thể méo ảnh', Icon: StretchHorizontal }] as const).map(({ value, label, description, Icon }) => <button key={value} aria-pressed={draft.fit === value} className={draft.fit === value ? 'fit-option selected' : 'fit-option'} onClick={() => set({ fit: value })}><div className={`fit-demo ${value}`}><img src="./landscape.svg" alt="" /><span><Icon size={18} /></span></div><strong>{label}</strong><small>{description}</small></button>)}</div>
      <SettingRow title="Nền mờ phía sau ảnh" description="Áp dụng khi chọn Vừa ảnh."><Toggle checked={draft.blurBackground} label="Nền mờ phía sau ảnh" onChange={() => set({ blurBackground: !draft.blurBackground })} /></SettingRow>
      <SettingRow title="Đồng hồ khi trình chiếu"><Toggle checked={draft.showClock} label="Hiển thị đồng hồ" onChange={() => set({ showClock: !draft.showClock })} /></SettingRow>
    </section>
    {error && <p className="form-error" role="alert">{error}</p>}
    <div className="page-footer"><button className="text-button" onClick={() => set({ ...defaultSettings })}>Khôi phục mặc định</button><button className="button primary" disabled={busy} onClick={save}><Check size={17} />Lưu cài đặt</button></div>
  </>;
}
