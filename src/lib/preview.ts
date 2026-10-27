import { emptyState, defaultSettings, type AppState, type Source } from './types';
import { clampSettings } from './rules';

export function persistablePreview(state: AppState): AppState {
  return {
    ...state,
    sources: state.sources.map(({ password: _password, accessToken: _accessToken, apiKey: _apiKey, ...source }) => source),
    current: null, running: false, error: null,
  };
}
export function restorePreview(saved: string | null): AppState {
  try {
    const parsed = saved ? JSON.parse(saved) : {};
    if (!parsed || parsed.version !== 1) return structuredClone(emptyState);
    return {
      ...structuredClone(emptyState),
      sources: Array.isArray(parsed.sources) ? parsed.sources.filter((source: Source) => source && typeof source.id === 'string' && typeof source.location === 'string' && typeof source.name === 'string' && ['local','url','manifest','webdav','drive','photos'].includes(source.kind)) : [],
      settings: clampSettings({ ...defaultSettings, ...parsed.settings }, 5),
    };
  } catch { return structuredClone(emptyState); }
}
