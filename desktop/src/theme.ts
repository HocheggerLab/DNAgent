// Light/dark appearance. Follows the OS by default; the user's explicit choice is
// remembered locally. Presentation state only.
export type ThemePreference = 'system' | 'light' | 'dark';
const KEY = 'dnagent.theme';
const media = window.matchMedia('(prefers-color-scheme: dark)');

function stored(): ThemePreference {
  try {
    const value = localStorage.getItem(KEY);
    return value === 'light' || value === 'dark' ? value : 'system';
  } catch { return 'system'; }
}

let preference: ThemePreference = stored();

export function themeState(): { preference: ThemePreference; resolved: 'light' | 'dark' } {
  return { preference, resolved: preference === 'system' ? (media.matches ? 'dark' : 'light') : preference };
}

function apply() {
  document.documentElement.dataset.theme = themeState().resolved;
}

export function initTheme(select: HTMLSelectElement) {
  select.value = preference;
  select.onchange = () => {
    preference = select.value as ThemePreference;
    try { if (preference === 'system') localStorage.removeItem(KEY); else localStorage.setItem(KEY, preference); } catch { /* not persisted */ }
    apply();
  };
  media.addEventListener('change', apply);
  apply();
}
