/** What a single key press asks the browser review to do. */
export type KeyAction =
	| 'previous-file'
	| 'next-file'
	| 'previous-hunk'
	| 'next-hunk'
	| 'previous-finding'
	| 'next-finding'
	| 'unified'
	| 'split'
	| 'wrap'
	| 'apply-revision'
	| 'toggle-focus'
	| 'leave-focus';

/** One row of the focus-mode key bar: the keys and what they do. */
export type KeyBarEntry = { keys: string[]; action: string };

/** The review's keys, matching the terminal's for the same actions. */
const ACTIONS: Record<string, KeyAction> = {
	'[': 'previous-file',
	']': 'next-file',
	'{': 'previous-hunk',
	'}': 'next-hunk',
	j: 'next-finding',
	k: 'previous-finding',
	p: 'next-finding',
	P: 'previous-finding',
	'1': 'unified',
	'2': 'split',
	w: 'wrap',
	u: 'apply-revision',
	b: 'toggle-focus',
	Escape: 'leave-focus'
};

/**
 * Keys that need AltGr or Option on many keyboard layouts, so they accept Alt
 * where the other keys do not.
 */
const ALT_KEYS = ['[', ']', '{', '}'];

/** What the focus-mode key bar lists on a wide screen. */
const WIDE_KEY_BAR: KeyBarEntry[] = [
	{ keys: ['[', ']'], action: 'file' },
	{ keys: ['{', '}'], action: 'hunk' },
	{ keys: ['j', 'k', 'p', 'P'], action: 'finding' },
	{ keys: ['1', '2'], action: 'unified or split' },
	{ keys: ['w'], action: 'wrap' },
	{ keys: ['b', 'Esc'], action: 'leave focus' }
];

/** The layout and wrap keys do nothing on a narrow screen, so its key bar leaves them out. */
const NARROW_KEY_BAR = WIDE_KEY_BAR.filter((entry) => entry.action !== 'unified or split' && entry.action !== 'wrap');

/**
 * Maps a key press to the review action it asks for, or `null` for a key the
 * review does not use or a modifier that rules it out. Ctrl or Alt rule out
 * every key except the brackets and braces, which accept Alt and AltGr; Windows
 * reports AltGr as Ctrl+Alt, so Ctrl counts against them only without Alt.
 */
export function keyAction(
	event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'altKey' | 'metaKey'> & {
		altGraph?: boolean;
	}
): KeyAction | null {
	if (event.metaKey) return null;
	const action = ACTIONS[event.key] ?? null;
	if (!action) return null;
	if (ALT_KEYS.includes(event.key)) {
		return event.ctrlKey && !event.altKey && !event.altGraph ? null : action;
	}
	return event.ctrlKey || event.altKey ? null : action;
}

/** Lists the keys the focus-mode key bar shows, leaving out those a narrow screen ignores. */
export function keyBarEntries(narrow: boolean): KeyBarEntry[] {
	return narrow ? NARROW_KEY_BAR : WIDE_KEY_BAR;
}

/**
 * Picks the scroll offset of the next or previous hunk from the pane's current
 * offset. `starts` holds each hunk's offset in order, already capped at the
 * pane's furthest scroll, so a hunk the pane cannot bring to the top counts as
 * being where the pane stops. A hunk within a pixel of `current` is the one on
 * screen. Returns `null` past the first or last hunk; the jumps do not wrap.
 */
export function hunkTarget(starts: number[], current: number, direction: 1 | -1): number | null {
	if (direction > 0) return starts.find((start) => start > current + 1) ?? null;
	const earlier = starts.filter((start) => start < current - 1);
	return earlier.length ? earlier[earlier.length - 1] : null;
}
