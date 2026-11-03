import { describe, expect, it } from 'vitest';

import { hunkTarget, keyAction, keyBarEntries } from './keys';

function press(
	key: string,
	modifiers: { ctrlKey?: boolean; altKey?: boolean; metaKey?: boolean; altGraph?: boolean } = {}
) {
	return keyAction({ key, ctrlKey: false, altKey: false, metaKey: false, ...modifiers });
}

describe('keyAction', () => {
	it('maps the terminal display keys', () => {
		expect(press('}')).toBe('next-hunk');
		expect(press('{')).toBe('previous-hunk');
		expect(press('1')).toBe('unified');
		expect(press('2')).toBe('split');
		expect(press('w')).toBe('wrap');
	});

	it('treats p and P as j and k', () => {
		expect(press('p')).toBe(press('j'));
		expect(press('P')).toBe(press('k'));
		expect(press('j')).toBe('next-finding');
		expect(press('k')).toBe('previous-finding');
	});

	it('keeps the existing keys', () => {
		expect(press(']')).toBe('next-file');
		expect(press('[')).toBe('previous-file');
		expect(press('b')).toBe('toggle-focus');
		expect(press('Escape')).toBe('leave-focus');
		expect(press('u')).toBe('apply-revision');
	});

	it('ignores keys the review does not use', () => {
		expect(press('c')).toBeNull();
		expect(press('3')).toBeNull();
		expect(press('W')).toBeNull();
	});

	it('lets brackets and braces take Alt or AltGr, but not Ctrl alone', () => {
		expect(press('{', { altKey: true })).toBe('previous-hunk');
		expect(press('}', { ctrlKey: true, altKey: true })).toBe('next-hunk');
		expect(press(']', { ctrlKey: true, altGraph: true })).toBe('next-file');
		expect(press('}', { ctrlKey: true })).toBeNull();
	});

	it('rules out the other keys under Ctrl, Alt or Meta', () => {
		expect(press('w', { ctrlKey: true })).toBeNull();
		expect(press('1', { altKey: true })).toBeNull();
		expect(press('}', { metaKey: true })).toBeNull();
	});
});

describe('keyBarEntries', () => {
	const listed = (narrow: boolean) => keyBarEntries(narrow).flatMap((entry) => entry.keys);

	it('lists every display key on a wide screen', () => {
		expect(listed(false)).toEqual(['[', ']', '{', '}', 'j', 'k', 'p', 'P', '1', '2', 'w', 'b', 'Esc']);
	});

	it('leaves out the layout and wrap keys on a narrow screen', () => {
		expect(listed(true)).toEqual(['[', ']', '{', '}', 'j', 'k', 'p', 'P', 'b', 'Esc']);
	});

	it('marks the layout and wrap keys as the wide-only ones', () => {
		const wideOnly = keyBarEntries(false).filter((entry) => entry.wideOnly);
		expect(wideOnly.flatMap((entry) => entry.keys)).toEqual(['1', '2', 'w']);
		expect(keyBarEntries(true).some((entry) => entry.wideOnly)).toBe(false);
	});
});

describe('hunkTarget', () => {
	const starts = [100, 400, 900];

	it('moves to the next hunk below the top of the pane', () => {
		expect(hunkTarget(starts, 0, 1)).toBe(100);
		expect(hunkTarget(starts, 100, 1)).toBe(400);
		expect(hunkTarget(starts, 250, 1)).toBe(400);
	});

	it('moves to the previous hunk above the top of the pane', () => {
		expect(hunkTarget(starts, 900, -1)).toBe(400);
		expect(hunkTarget(starts, 250, -1)).toBe(100);
	});

	it('stops at the first and last hunk', () => {
		expect(hunkTarget(starts, 900, 1)).toBeNull();
		expect(hunkTarget(starts, 100, -1)).toBeNull();
		expect(hunkTarget([], 0, 1)).toBeNull();
	});

	it('counts a hunk within a pixel of the top as the one on screen', () => {
		expect(hunkTarget(starts, 399.5, 1)).toBe(900);
		expect(hunkTarget(starts, 400.5, -1)).toBe(100);
	});
});
