import { describe, expect, it } from 'vitest';

import {
	FIRST_RETRY_DELAY,
	MAX_RETRY_DELAY,
	heldChanges,
	heldFileChange,
	heldSummary,
	nextRetryDelay,
	refusedConnection,
	retries,
	staleFindingWrite,
	watchLabel,
	watchNotice,
	withViewedMarks
} from './live';
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
		const expired = watchNotice('expired', 'watching', 3);
		expect(expired?.heading).toBe('Session expired');
		expect(expired?.action).toBeNull();
		expect(expired?.body).toContain('Open the URL mire serve printed');
	});
});

describe('reconnecting', () => {
	it('doubles the wait up to the cap', () => {
		const delays = [FIRST_RETRY_DELAY];
		while (delays.length < 7) delays.push(nextRetryDelay(delays[delays.length - 1]));
		expect(delays).toEqual([1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000]);
		expect(MAX_RETRY_DELAY).toBe(30_000);
	});

	it('stops after a shutdown or a refused session', () => {
		expect(retries('lost')).toBe(true);
		expect(retries('stopped')).toBe(false);
		expect(retries('expired')).toBe(false);
		expect(refusedConnection(401)).toBe('expired');
		expect(refusedConnection(403)).toBe('expired');
		expect(refusedConnection(502)).toBe('lost');
	});
});

describe('staleFindingWrite', () => {
	const next = overview(5, [], [finding('n1', 'resolved', 'New text'), finding('n2')]);

	it('lets a write through when what the reader started from is unchanged', () => {
		expect(staleFindingWrite(next, { findingId: 'n2', status: 'open' })).toBeNull();
		expect(staleFindingWrite(next, { findingId: 'n1', body: 'New text' })).toBeNull();
	});

	it('names what the newer revision changed', () => {
		expect(staleFindingWrite(next, { findingId: 'gone', status: 'open' })).toBe('Revision 5 removed this finding.');
		expect(staleFindingWrite(next, { findingId: 'n1', body: 'Body' })).toBe("Revision 5 changed this finding's text.");
		expect(staleFindingWrite(next, { findingId: 'n1', status: 'open' })).toBe(
			"Revision 5 changed this finding's status to resolved."
		);
	});
});

describe('watchLabel', () => {
	it('adds the revision on screen once updates stop', () => {
		expect(watchLabel('live', 'watching', 3)).toBe('watching');
		expect(watchLabel('live', 'unavailable', 3)).toBe('unavailable');
		expect(watchLabel('live', 'degraded', 3)).toBe('degraded · showing revision 3');
		expect(watchLabel('lost', 'watching', 3)).toBe('disconnected · showing revision 3');
		expect(watchLabel('stopped', 'watching', 3)).toBe('stopped · showing revision 3');
		expect(watchLabel('expired', 'watching', 3)).toBe('expired · showing revision 3');
	});
});
