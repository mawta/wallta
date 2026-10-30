import { beforeEach } from 'vitest';
import { localeStore } from '../lib/i18n';

// Existing interaction regressions exercise the Vietnamese UI explicitly.
// Bilingual tests override this preference and test system defaults separately.
beforeEach(() => localeStore.setPreference('vi'));
