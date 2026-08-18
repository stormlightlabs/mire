const SESSION_SECRET_STORAGE_KEY = 'mire:development-session-secret';

/** Extracts a session secret from either a bare secret or a Mire review URL. */
export function parseSessionSecret(value: string): string {
	const candidate = value.trim();
	const hashIndex = candidate.lastIndexOf('#');
	if (hashIndex >= 0) return candidate.slice(hashIndex + 1).trim();
	return candidate.includes('://') ? '' : candidate;
}

/** Loads the development session secret retained for the current browser tab. */
export function loadDevelopmentSessionSecret(): string {
	try {
		return sessionStorage.getItem(SESSION_SECRET_STORAGE_KEY) ?? '';
	} catch {
		return '';
	}
}

/** Retains the development session secret for reloads in the current browser tab. */
export function saveDevelopmentSessionSecret(secret: string) {
	try {
		sessionStorage.setItem(SESSION_SECRET_STORAGE_KEY, secret);
	} catch {
		// Browser privacy settings can disable session storage; the review remains usable.
	}
}
