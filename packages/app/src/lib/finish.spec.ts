import { describe, expect, it } from 'vitest';

import { EXPORTS, handoffHeadline, handoffItems, nextUnviewed } from './finish';
import type { HeldChanges } from './live';
import type { FileSummary, FindingSummary, ReviewOverview } from './review';

function file(raw: string, options: { viewed?: 'viewed' | 'changed'; generated?: boolean } = {}): FileSummary {
	const id = `${raw}-now`;
	return {
		id,
		path: { display: raw, lossy: false, raw },
		status: 'modified',
		contentKind: 'text',
		generated: options.generated ?? false,
		openFindings: 0,
		viewed: options.viewed ? { fingerprint: options.viewed === 'viewed' ? id : `${raw}-before`, revision: 1 } : null
	};
}

function finding(
	id: string,
	options: { status?: string; severity?: string; anchorState?: FindingSummary['anchorState'] } = {}
): FindingSummary {
	return {
		id,
		path: { display: 'a.rs', lossy: false, raw: 'a.rs' },
		side: 'new',
		startLine: 1,
		endLine: 1,
		anchorState: options.anchorState ?? 'exact',
		navigable: true,
		severity: options.severity ?? 'low',
		annotationKind: 'comment',
		status: options.status ?? 'open',
		body: 'Body'
	};
}

function overview(files: FileSummary[], findings: FindingSummary[] = []): ReviewOverview {
	return {
		reviewIdentity: 'review',
		revision: 4,
		source: 'working tree',
		title: null,
		description: null,
		files,
		findings,
		totals: { files: files.length, findings: findings.length, open: 0, resolved: 0, dismissed: 0, acceptedRisk: 0 },
		changes: { additions: 0, deletions: 0 },
		reanchor: { captured: 0, exact: 0, moved: 0, stale: 0, ambiguous: 0 },
		readiness: { ready: false, openFindings: 0, unsafeAnchors: 0 },
		watch: 'watching'
	};
}

describe('handoffItems', () => {
	it('is empty when every file is viewed and every finding is settled and anchored', () => {
		const review = overview(
			[file('a.rs', { viewed: 'viewed' }), file('b.rs', { viewed: 'viewed' })],
			[finding('1', { status: 'resolved' }), finding('2', { status: 'dismissed', anchorState: 'moved' })]
		);

		expect(handoffItems(review, null, null)).toEqual([]);
		expect(handoffHeadline(0)).toBe('Nothing is left.');
	});

	it('counts files not viewed as they are now, and leaves generated files out', () => {
		const review = overview([
			file('a.rs', { viewed: 'viewed' }),
			file('b.rs', { viewed: 'changed' }),
			file('c.rs'),
			file('Cargo.lock', { generated: true })
		]);

		const [item] = handoffItems(review, null, review.files[0]);

		expect(item).toMatchObject({
			kind: 'unviewed',
			title: '2 of 3 files not viewed',
			detail: '1 changed since you viewed it. 1 generated file is not counted.'
		});
		expect(item.kind === 'unviewed' && item.target.path.raw).toBe('b.rs');
	});

	it('counts generated files when the review has nothing else', () => {
		const review = overview([file('Cargo.lock', { generated: true }), file('dist/app.js', { generated: true })]);

		expect(handoffItems(review, null, null)).toMatchObject([{ title: '2 of 2 files not viewed', detail: null }]);
	});

	it('counts open findings by severity and opens the most severe first', () => {
		const review = overview(
			[file('a.rs', { viewed: 'viewed' })],
			[
				finding('1', { severity: 'low' }),
				finding('2', { severity: 'high' }),
				finding('3', { severity: 'low' }),
				finding('4', { severity: 'critical', status: 'resolved' })
			]
		);

		const [item] = handoffItems(review, null, null);

		expect(item).toMatchObject({ kind: 'findings', title: '3 open findings', detail: '1 high · 2 low' });
		expect(item.kind === 'findings' && item.target.id).toBe('2');
	});

	it('counts stale and ambiguous anchors on every finding, open or not', () => {
		const review = overview(
			[file('a.rs', { viewed: 'viewed' })],
			[
				finding('1', { status: 'resolved', anchorState: 'stale' }),
				finding('2', { status: 'resolved', anchorState: 'ambiguous' }),
				finding('3', { status: 'resolved', anchorState: 'moved' })
			]
		);

		const [item] = handoffItems(review, null, null);

		expect(item).toMatchObject({
			kind: 'anchors',
			title: '2 findings lost their line',
			detail: '1 stale · 1 ambiguous'
		});
		expect(item.kind === 'anchors' && item.target.id).toBe('1');
	});

	it('puts a held revision first and says which revision the counts are for', () => {
		const review = overview([file('a.rs')], [finding('1')]);
		const held: HeldChanges = {
			revision: 5,
			files: [{ path: review.files[0].path, change: 'changed' }],
			findings: 0,
			replaced: false
		};

		const items = handoffItems(review, held, null);

		expect(items.map((item) => item.kind)).toEqual(['held', 'unviewed', 'findings']);
		expect(items[0]).toMatchObject({
			title: 'Revision 5 is not applied',
			detail: '1 file changed. The counts below are for revision 4.',
			revision: 5
		});
		expect(handoffHeadline(items.length)).toBe('Three things are left.');
	});
});

describe('nextUnviewed', () => {
	it('starts after the shown file in tree order and wraps', () => {
		const files = [file('c.rs'), file('a.rs'), file('b.rs', { viewed: 'viewed' })];

		expect(nextUnviewed(files, files[0])?.path.raw).toBe('a.rs');
		expect(nextUnviewed(files, files[1])?.path.raw).toBe('c.rs');
		expect(nextUnviewed(files, null)?.path.raw).toBe('a.rs');
	});

	it('returns null when every file is viewed', () => {
		expect(nextUnviewed([file('a.rs', { viewed: 'viewed' })], null)).toBeNull();
	});
});

describe('EXPORTS', () => {
	it('offers only the downloads the server serves', () => {
		expect(EXPORTS.map((choice) => choice.route)).toEqual([
			'/api/v1/exports/notes.md',
			'/api/v1/exports/notes.json',
			'/api/v1/exports/context.json',
			'/api/v1/exports/changeset.patch'
		]);
	});
});
