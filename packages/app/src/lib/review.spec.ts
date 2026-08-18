import { describe, expect, it } from 'vitest';

import { completionSummary, type ReviewOverview } from './review';

function review(): ReviewOverview {
	return {
		reviewIdentity: 'review',
		revision: 1,
		source: 'Git comparison',
		files: [
			{
				id: 'first',
				path: { display: 'first.rs', lossy: false },
				status: 'modified',
				contentKind: 'text',
				openFindings: 0
			},
			{
				id: 'second',
				path: { display: 'second.rs', lossy: false },
				status: 'modified',
				contentKind: 'text',
				openFindings: 1
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
	it('combines local viewed files with durable review readiness', () => {
		const summary = completionSummary(review(), ['first']);

		expect(summary).toEqual({ ready: false, unviewedFiles: 1, openFindings: 1, unsafeAnchors: 0 });
	});
});
