import { useSyncExternalStore } from 'react';
import { english } from './locales/en';
import { nativeEnglish, nativeVietnamese } from './locales/native';

export type Locale = 'en' | 'vi';
export type LanguagePreference = Locale | 'system';
type Snapshot = Readonly<{ preference: LanguagePreference; locale: Locale }>;
type PreferenceStorage = Pick<Storage, 'getItem' | 'setItem'>;
const storageKey = 'wallta.language.v1';
const validPreference = (value: unknown): value is LanguagePreference => value === 'en' || value === 'vi' || value === 'system';

export function createLocaleStore(options: { storage?: PreferenceStorage; languages?: () => readonly string[] } = {}) {
  let preference: LanguagePreference = 'system';
  try { const saved = options.storage?.getItem(storageKey); if (validPreference(saved)) preference = saved; } catch { /* A blocked store still permits a session preference. */ }
  const deviceLocale = (): Locale => /^vi(?:[-_]|$)/i.test(options.languages?.()[0] || '') ? 'vi' : 'en';
  let snapshot: Snapshot = { preference, locale: preference === 'system' ? deviceLocale() : preference };
  const listeners = new Set<() => void>();
  const refresh = () => {
    const locale = preference === 'system' ? deviceLocale() : preference;
    if (snapshot.preference === preference && snapshot.locale === locale) return;
    snapshot = { preference, locale };
    for (const listener of listeners) listener();
  };
  return {
    getSnapshot: () => snapshot,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    setPreference: (next: LanguagePreference) => {
      if (!validPreference(next)) return;
      preference = next;
      try { options.storage?.setItem(storageKey, next); } catch { /* Keep the selection usable without persistent storage. */ }
      refresh();
    },
    syncPreference: (value: string | null) => { preference = validPreference(value) ? value : 'system'; refresh(); },
    refresh,
  };
}

function browserStorage(): PreferenceStorage | undefined {
  try { return globalThis.localStorage; } catch { return undefined; }
}
export const localeStore = createLocaleStore({ storage: browserStorage(), languages: () => typeof navigator === 'undefined' ? [] : navigator.languages?.length ? navigator.languages : [navigator.language] });
if (typeof window !== 'undefined') {
  window.addEventListener('languagechange', () => localeStore.refresh());
  window.addEventListener('storage', event => { if (event.key === storageKey || event.key === null) localeStore.syncPreference(event.newValue); });
}
export function useLocale() {
  const snapshot = useSyncExternalStore(localeStore.subscribe, localeStore.getSnapshot, localeStore.getSnapshot);
  return { ...snapshot, setPreference: localeStore.setPreference };
}
export const localeTag = () => localeStore.getSnapshot().locale === 'vi' ? 'vi-VN' : 'en-US';

const extraEnglish: Record<string, string> = {
  'Ngôn ngữ': 'Language', 'Theo thiết bị': 'System default',
  'CN': 'Sun', 'T2': 'Mon', 'T3': 'Tue', 'T4': 'Wed', 'T5': 'Thu', 'T6': 'Fri', 'T7': 'Sat',
  'Wallta — Hình nền tự động': 'Wallta — Automatic wallpapers',
  'Cài bản cập nhật': 'Install update',
  'Nhanh': 'Fast', 'Sau': 'After', 'Sau ·': 'After ·', 'Model upscale': 'Upscale model',
};
const reverseCatalog = (catalog: Record<string, string>): Record<string, string> => Object.fromEntries(Object.entries(catalog).map(([key, value]) => [value, key]));
const toEnglish: Record<string, string> = { ...reverseCatalog(nativeVietnamese), ...nativeEnglish, ...english, ...extraEnglish };
const toVietnamese: Record<string, string> = { ...Object.fromEntries(Object.entries(toEnglish).map(([vi, en]) => [en, vi])), ...nativeVietnamese, 'Cài bản cập nhật': 'Cài đặt' };
const escapeRegex = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
// UI templates receive explicit values. Only native diagnostics and these
// stored error envelopes may infer parameters from already-rendered text.
const storedErrors = ['Đã lưu nguồn. {0}', '{0}: chọn lại thư mục sau khi tải lại bản xem trước.', '{0}: dùng bản cài Wallta để truy cập nguồn {1}.'];
const automaticMessages = new Set([
  ...Object.keys(nativeEnglish), ...Object.values(nativeEnglish),
  ...Object.keys(nativeVietnamese), ...Object.values(nativeVietnamese),
  ...storedErrors, ...storedErrors.map(key => english[key]),
]);
function patterns(catalog: Record<string, string>) {
  return Object.entries(catalog).filter(([key]) => automaticMessages.has(key) && /\{\d+\}/.test(key))
    .sort(([left], [right]) => right.replace(/\{\d+\}/g, '').length - left.replace(/\{\d+\}/g, '').length).map(([key, value]) => {
    const slots = [...key.matchAll(/\{(\d+)\}/g)].map(match => match[1]);
    const parts = key.split(/\{\d+\}/).map(escapeRegex);
    const diagnosticSlots = key === 'Đã lưu nguồn. {0}' || key === 'Source saved. {0}' ? ['0'] : [];
    return { regex: new RegExp(`^${parts.join('([\\s\\S]+?)')}$`), slots, diagnosticSlots, value };
  });
}
const enPatterns = patterns(toEnglish), viPatterns = patterns(toVietnamese);
export function translate(text: string, values: Record<string | number, unknown> = {}, locale: Locale = localeStore.getSnapshot().locale): string {
  const catalog = locale === 'en' ? toEnglish : toVietnamese;
  const known = (input: string): string | undefined => {
    if (Object.prototype.hasOwnProperty.call(catalog, input)) return catalog[input];
    for (const pattern of locale === 'en' ? enPatterns : viPatterns) {
      const match = pattern.regex.exec(input);
      if (!match) continue;
      return pattern.value.replace(/\{(\d+)\}/g, (placeholder, slot: string) => {
        const index = pattern.slots.indexOf(slot);
        if (index < 0) return placeholder;
        return pattern.diagnosticSlots.includes(slot) ? translate(match[index + 1], {}, locale) : match[index + 1];
      });
    }
    return undefined;
  };
  let result = Object.prototype.hasOwnProperty.call(catalog, text) ? catalog[text] : text;
  if (!Object.keys(values).length) {
    result = known(text) ?? text.split('\n').map(line => {
      const direct = known(line);
      if (direct !== undefined) return direct;
      // Native provider errors are prefixed with a user-owned source name.
      // Translate only a recognized diagnostic, never the name or unknown detail.
      for (let index = line.indexOf(': '); index >= 0; index = line.indexOf(': ', index + 2)) {
        const suffix = known(line.slice(index + 2));
        if (suffix !== undefined) return line.slice(0, index + 2) + suffix;
      }
      return line;
    }).join('\n');
  }
  return result.replace(/\{(\d+)\}/g, (placeholder, key: string) => Object.prototype.hasOwnProperty.call(values, key) ? String(values[key]) : placeholder);
}
export const t = translate;
