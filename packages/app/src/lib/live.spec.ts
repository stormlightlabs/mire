import { describe, expect, it } from 'vitest';

import { heldChanges, heldFileChange, heldSummary, watchLabel, watchNotice, withViewedMarks } from './live';
import type { FileSummary, FindingSummary, ReviewOverview } from './review';

function file(raw: string, id: string, viewed: FileSummary['viewed'] = null): FileSummary {
	return {
		id,
		path: { display: raw, lossy: false, raw },
		status: 'modified',
		contentKind: 'text',
		generated: false,
		openFindings: 0,
		viewed
	};
}

function finding(id: string, status = 'open', body = 'Body'): FindingSummary {
	return {
		id,
		path: { display: 'a.rs', lossy: false, raw: 'a.rs' },
		side: 'new',
		startLine: 1,
		endLine: 1,
		anchorState: 'exact',
		navigable: true,
		severity: 'low',
		annotationKind: 'comment',
		status,
		body
	};
}

function overview(revision: number, files: FileSummary[], findings: FindingSummary[] = []): ReviewOverview {
	return {
		reviewIdentity: 'review',
		revision,
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

describe('heldChanges', () => {
	it('lists changed, added and removed files by path', () => {
		const shown = overview(14, [file('a.rs', '1'), file('b.rs', '2'), file('c.rs', '3')]);
		const next = overview(15, [file('a.rs', '1'), file('b.rs', '9'), file('d.rs', '4')]);

		const changes = heldChanges(shown, next);

		expect(changes.revision).toBe(15);
		expect(changes.files.map((change) => [change.path.raw, change.change])).toEqual([
			['b.rs', 'changed'],
			['d.rs', 'added'],
			['c.rs', 'removed']
		]);
		expect(heldFileChange(changes, file('b.rs', '2'))).toBe('changed');
		expect(heldFileChange(changes, file('a.rs', '1'))).toBeNull();
		expect(heldFileChange(null, file('b.rs', '2'))).toBeNull();
	});

	it('counts findings added, removed or changed', () => {
		const shown = overview(1, [], [finding('n1'), finding('n2'), finding('n3')]);
		const next = overview(2, [], [finding('n1'), finding('n2', 'resolved'), finding('n4')]);

		expect(heldChanges(shown, next).findings).toBe(3);
	});
});

describe('heldSummary', () => {
	it('names files and findings', () => {
		const files = [{ path: { display: 'a', lossy: false, raw: 'a' }, change: 'changed' as const }];
		expect(heldSummary({ revision: 2, files, findings: 1 })).toBe('1 file and 1 finding changed while you read.');
		expect(heldSummary({ revision: 2, files: [...files, ...files], findings: 0 })).toBe(
			'2 files changed while you read.'
		);
		expect(heldSummary({ revision: 2, files: [], findings: 0 })).toBe('No diff or finding changed while you read.');
	});
});

describe('withViewedMarks', () => {
	it('copies marks by path and keeps the rest of the shown revision', () => {
		const mark = { fingerprint: '1', revision: 14 };
		const shown = overview(14, [file('a.rs', '1'), file('gone.rs', '5', mark)]);
		const next = { ...overview(15, [file('a.rs', '7', mark)]), watch: 'degraded' as const };

		const merged = withViewedMarks(shown, next);

		expect(merged.revision).toBe(14);
		expect(merged.watch).toBe('degraded');
		expect(merged.files.map((summary) => [summary.id, summary.viewed])).toEqual([
			['1', mark],
			['5', mark]
		]);
	});
});

describe('watchNotice', () => {
	it('stays quiet while updates arrive', () => {
		expect(watchNotice('live', 'watching', 3)).toBeNull();
		expect(watchNotice('live', 'unavailable', 3)).toBeNull();
	});

	it('offers a way to resume each stop', () => {
		expect(watchNotice('lost', 'watching', 3)?.action).toBe('reconnect');
		expect(watchNotice('live', 'degraded', 3)?.action).toBe('refresh');
		const stopped = watchNotice('stopped', 'watching', 3);
		expect(stopped?.action).toBeNull();
		expect(stopped?.body).toContain('Run mire serve again');
		expect(stopped?.body).toContain('revision 3');
	});
});

describe('watchLabel', () => {
	it('adds the revision on screen once updates stop', () => {
		expect(watchLabel('live', 'watching', 3)).toBe('watching');
		expect(watchLabel('live', 'unavailable', 3)).toBe('unavailable');
		expect(watchLabel('live', 'degraded', 3)).toBe('degraded · showing revision 3');
		expect(watchLabel('lost', 'watching', 3)).toBe('disconnected · showing revision 3');
		expect(watchLabel('stopped', 'watching', 3)).toBe('stopped · showing revision 3');
	});
});
