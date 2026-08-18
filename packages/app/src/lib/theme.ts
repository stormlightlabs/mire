export type Theme = 'light' | 'dark';

const THEME_STORAGE_KEY = 'mire:theme';

/** Applies the selected color theme to the application document. */
export function applyTheme(theme: Theme) {
	document.documentElement.dataset.theme = theme;
	document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme === 'dark' ? '#1b1d21' : '#f7f7f5');
}

/** Loads a saved color theme or uses the operating-system preference. */
export function loadTheme(): Theme {
	try {
		const stored = localStorage.getItem(THEME_STORAGE_KEY);
		if (stored === 'light' || stored === 'dark') return stored;
	} catch {
		// Browser privacy settings can disable local storage.
	}
	return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/** Saves a color theme without retaining review data. */
export function saveTheme(theme: Theme) {
	try {
		localStorage.setItem(THEME_STORAGE_KEY, theme);
	} catch {
		// Browser privacy settings can disable local storage.
	}
}
