export type Theme = 'light' | 'dark';

const THEME_STORAGE_KEY = 'mire:theme';

/**
 * Applies the selected color mode to the application document. The server sets
 * the palette family on the root element, and `themes.css` holds each
 * family's light and dark palettes.
 */
export function applyTheme(theme: Theme) {
	const root = document.documentElement;
	root.dataset.theme = theme;
	const paper = getComputedStyle(root).getPropertyValue('--paper').trim();
	if (paper) document.querySelector('meta[name="theme-color"]')?.setAttribute('content', paper);
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
