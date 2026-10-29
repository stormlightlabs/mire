import { describe, expect, it } from 'vitest';

import {
	completionSummary,
	fileViewState,
	firstFile,
	hasReviewMetadata,
	isShownFile,
	isWholeFileChange,
	reopenedFile,
	reviewDescription,
	reviewTitle,
	shownViewState,
	viewedRoute,
	type ReviewOverview
} from './review';

function review(): ReviewOverview {
	return {
		reviewIdentity: 'review',
		revision: 1,
		source: 'Git comparison',
		title: null,
		description: null,
		files: [
			{
				id: 'first',
				path: { display: 'first.rs', lossy: false, raw: '66697273742e7273' },
				status: 'modified',
				contentKind: 'text',
				generated: false,
				openFindings: 0,
				viewed: { fingerprint: 'first', revision: 1 }
			},
			{
				id: 'second',
				path: { display: 'second.rs', lossy: false, raw: '7365636f6e642e7273' },
				status: 'modified',
				contentKind: 'text',
				generated: false,
				openFindings: 1,
				viewed: null
			}
		],
		findings: [],
		totals: { files: 2, findings: 1, open: 1, resolved: 0, dismissed: 0, acceptedRisk: 0 },
		changes: { additions: 2, deletions: 1 },
		reanchor: { captured: 1, exact: 0, moved: 0, stale: 0, ambiguous: 0 },
		readiness: { ready: false, openFindings: 1, unsafeAnchors: 0 },
		watch: 'watching'
	};
}

describe('completionSummary', () => {
	it('combines viewed marks with review readiness', () => {
		const summary = completionSummary(review());

		expect(summary).toEqual({ ready: false, unviewedFiles: 1, openFindings: 1, unsafeAnchors: 0 });
	});

	it('counts a file that changed since it was viewed as unviewed', () => {
		const changed = review();
		changed.files[0].viewed = { fingerprint: 'older', revision: 1 };
		changed.files[1].viewed = { fingerprint: 'second', revision: 1 };
		changed.readiness = { ready: true, openFindings: 0, unsafeAnchors: 0 };

		expect(completionSummary(changed)).toEqual({ ready: false, unviewedFiles: 1, openFindings: 0, unsafeAnchors: 0 });
		changed.files[0].viewed = { fingerprint: 'first', revision: 2 };
		expect(completionSummary(changed).ready).toBe(true);
	});
});

describe('viewed state', () => {
	it('compares the mark with the current fingerprint', () => {
		expect(fileViewState({ id: 'abc', viewed: null })).toBe('unviewed');
		expect(fileViewState({ id: 'abc', viewed: { fingerprint: 'abc', revision: 3 } })).toBe('viewed');
		expect(fileViewState({ id: 'abc', viewed: { fingerprint: 'old', revision: 2 } })).toBe('changed');
	});

	it('addresses a path by its raw hex bytes', () => {
		expect(viewedRoute({ display: 'a�', lossy: true, raw: '61ff' })).toBe('/api/v1/viewed?path=61ff');
	});
});

describe('firstFile', () => {
	it('skips generated files unless every file is generated', () => {
		const files = review().files;
		files[0].generated = true;
		expect(firstFile(files)).toBe(files[1]);
		files[1].generated = true;
		expect(firstFile(files)).toBe(files[0]);
		expect(firstFile([])).toBeNull();
	});
});

describe('reopenedFile', () => {
	it('follows a file whose fingerprint changed by its raw path', () => {
		const reloaded = review();
		reloaded.files[1].id = 'second-changed';

		const previous = { id: 'second', path: { display: 'second.rs', lossy: false, raw: '7365636f6e642e7273' } };
		expect(reopenedFile(reloaded.files, previous)).toEqual({ file: reloaded.files[1], notice: null });
	});

	it('falls back to the first file with a notice when the open one left the review', () => {
		const previous = { id: 'gone', path: { display: 'gone.rs', lossy: false, raw: '676f6e652e7273' } };

		expect(reopenedFile(review().files, previous)).toEqual({
			file: review().files[0],
			notice: 'gone.rs is no longer in the review; showing first.rs.'
		});
		expect(reopenedFile([], previous)).toEqual({ file: null, notice: 'gone.rs is no longer in the review.' });
	});

	it('names a renamed file by its old path', () => {
		const renamed = review();
		renamed.files[1] = {
			...renamed.files[1],
			id: 'moved',
			path: { display: 'moved.rs', lossy: false, raw: '6d6f7665642e7273' }
		};
		const previous = { id: 'second', path: { display: 'second.rs', lossy: false, raw: '7365636f6e642e7273' } };

		expect(reopenedFile(renamed.files, previous).notice).toBe(
			'second.rs is no longer in the review; showing first.rs.'
		);
	});

	it('opens the first file without a notice when nothing was open', () => {
		expect(reopenedFile(review().files, null)).toEqual({ file: review().files[0], notice: null });
		expect(reopenedFile([], null)).toEqual({ file: null, notice: null });
	});
});

describe('isShownFile', () => {
	const shown = { path: { display: 'second.rs', lossy: false, raw: '7365636f6e642e7273' } };

	it('keeps the shown file mounted when the same path reloads with a new fingerprint', () => {
		const reloaded = review();
		reloaded.files[1].id = 'second-changed';

		expect(isShownFile(shown, reloaded.files[1])).toBe(true);
	});

	it('clears the pane for another file or when nothing is shown', () => {
		expect(isShownFile(shown, review().files[0])).toBe(false);
		expect(isShownFile(null, review().files[1])).toBe(false);
	});
});

describe('shownViewState', () => {
	it('reads the mark against the fingerprint on screen', () => {
		const files = review().files;
		const path = files[0].path;

		expect(shownViewState(files, { id: 'first', path })).toBe('viewed');
		expect(shownViewState(files, { id: 'first-older', path })).toBe('changed');
		expect(shownViewState(files, { id: 'second', path: files[1].path })).toBe('unviewed');
	});

	it('reads older content on screen as changed once the newer one is marked', () => {
		const files = review().files;
		files[0].viewed = { fingerprint: 'first-newer', revision: 2 };
		files[0].id = 'first-newer';

		expect(shownViewState(files, { id: 'first', path: files[0].path })).toBe('changed');
		expect(shownViewState(files, { id: 'first-newer', path: files[0].path })).toBe('viewed');
	});

	it('reports a path that left the review as unviewed', () => {
		const path = { display: 'gone.rs', lossy: false, raw: '676f6e652e7273' };

		expect(shownViewState(review().files, { id: 'gone', path })).toBe('unviewed');
	});
});

describe('review metadata', () => {
	it('falls back to the default heading without a title', () => {
		expect(reviewTitle(null)).toBe('Local review');
		expect(reviewTitle(review())).toBe('Local review');
		expect(reviewTitle({ title: '   ' })).toBe('Local review');
		expect(reviewTitle({ title: ' Add caching ' })).toBe('Add caching');
	});

	it('keeps the description text as written', () => {
		expect(reviewDescription(review())).toBeNull();
		expect(reviewDescription({ description: '\n  \n' })).toBeNull();
		expect(reviewDescription({ description: 'First\n\n<b>raw</b>' })).toBe('First\n\n<b>raw</b>');
	});

	it('has metadata with a title, a description or both', () => {
		expect(hasReviewMetadata(null)).toBe(false);
		expect(hasReviewMetadata({ title: '  ', description: '\n' })).toBe(false);
		expect(hasReviewMetadata({ title: 'Add caching', description: null })).toBe(true);
		expect(hasReviewMetadata({ title: null, description: 'Body' })).toBe(true);
	});
});

describe('isWholeFileChange', () => {
	it('holds for added and deleted files only', () => {
		expect(isWholeFileChange('added')).toBe(true);
		expect(isWholeFileChange('deleted')).toBe(true);
		expect(isWholeFileChange('modified')).toBe(false);
		expect(isWholeFileChange('renamed')).toBe(false);
	});
});
