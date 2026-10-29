import { describe, expect, it } from 'vitest';

import { changeSummary, fileOrder, filePosition, splitPath, takesText } from './focus';
import type { FileDetail, FileSummary, SemanticLine } from './review';

function summary(display: string, generated = false): FileSummary {
	return {
		id: `id-${display}`,
		path: { display, lossy: false, raw: Buffer.from(display).toString('hex') },
		status: 'modified',
		contentKind: 'text',
		generated,
		openFindings: 0,
		viewed: null
	};
}

function line(kind: SemanticLine['kind']): SemanticLine {
	return { kind, oldLine: null, newLine: null, content: { display: '', lossy: false }, missingNewline: 'none' };
}

function detail(status: string, kinds: SemanticLine['kind'][]): Pick<FileDetail, 'status' | 'content' | 'generated'> {
	const hunk = {
		oldStart: 1,
		oldLineCount: 0,
		newStart: 1,
		newLineCount: 0,
		section: { display: '', lossy: false },
		lines: kinds.map(line)
	};
	return { status, generated: false, content: { kind: 'text', hunks: [hunk] } };
}

describe('fileOrder', () => {
	it('lists files by path, then generated files in review order', () => {
		const files = [summary('z.lock', true), summary('src/b.rs'), summary('a.lock', true), summary('src/a.rs')];
		expect(fileOrder(files).map((file) => file.path.display)).toEqual(['src/a.rs', 'src/b.rs', 'z.lock', 'a.lock']);
	});
});

describe('filePosition', () => {
	const files = [summary('b.rs'), summary('a.rs'), summary('c.rs')];

	it('finds the shown file and its neighbours by raw path', () => {
		const position = filePosition(files, { path: summary('b.rs').path });
		expect(position?.index).toBe(1);
		expect(position?.total).toBe(3);
		expect(position?.previous?.path.display).toBe('a.rs');
		expect(position?.next?.path.display).toBe('c.rs');
	});

	it('stops at the ends', () => {
		expect(filePosition(files, { path: summary('a.rs').path })?.previous).toBeNull();
		expect(filePosition(files, { path: summary('c.rs').path })?.next).toBeNull();
	});

	it('returns null for no file or a file outside the review', () => {
		expect(filePosition(files, null)).toBeNull();
		expect(filePosition(files, { path: summary('gone.rs').path })).toBeNull();
	});
});

describe('splitPath', () => {
	it('keeps the trailing slash on the directory', () => {
		expect(splitPath('crates/ui/src/description.rs')).toEqual({
			directory: 'crates/ui/src/',
			name: 'description.rs'
		});
	});

	it('returns no directory for a top-level file', () => {
		expect(splitPath('README.md')).toEqual({ directory: '', name: 'README.md' });
	});
});

describe('changeSummary', () => {
	it('counts the lines of an added file', () => {
		expect(changeSummary(detail('added', ['addition', 'addition', 'addition']))).toBe('+ New file · 3 lines');
		expect(changeSummary(detail('added', ['addition']))).toBe('+ New file · 1 line');
	});

	it('counts the lines of a deleted file', () => {
		expect(changeSummary(detail('deleted', ['deletion', 'deletion']))).toBe('− Deleted file · 2 lines');
	});

	it('counts added and removed lines of a changed file, leaving out context', () => {
		expect(changeSummary(detail('modified', ['context', 'deletion', 'addition', 'addition']))).toBe('Modified · +2 −1');
	});

	it('names binary and generated files', () => {
		expect(changeSummary({ status: 'modified', generated: true, content: { kind: 'binary' } })).toBe(
			'Modified · binary · generated'
		);
	});
});

describe('takesText', () => {
	const element = (tagName: string, extra: Record<string, unknown> = {}) =>
		({ tagName, ...extra }) as unknown as EventTarget;

	it('blocks shortcuts in text fields', () => {
		expect(takesText(element('INPUT', { type: 'search' }))).toBe(true);
		expect(takesText(element('TEXTAREA'))).toBe(true);
		expect(takesText(element('SELECT'))).toBe(true);
		expect(takesText(element('DIV', { isContentEditable: true }))).toBe(true);
	});

	it('allows shortcuts on checkboxes, buttons and the page', () => {
		expect(takesText(element('INPUT', { type: 'checkbox' }))).toBe(false);
		expect(takesText(element('BUTTON'))).toBe(false);
		expect(takesText(null)).toBe(false);
	});
});
