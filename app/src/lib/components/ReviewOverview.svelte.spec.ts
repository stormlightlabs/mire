import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import type { ReviewWorkspace } from '$lib/workbench/types';
import ReviewOverview from './ReviewOverview.svelte';

const anchor = { snapshot_id: 'snapshot-1234', path: 'internal/server/server.go', hunk_id: 'server.go#abc' };
const finding = {
	finding: {
		finding_id: 'finding-1234',
		revision: 1,
		claim: 'The read endpoint accepts an unbounded path.',
		impact: 'A request could escape the frozen snapshot boundary.',
		category: 'security',
		severity: 'high',
		confidence: 0.9,
		verification: 'supported',
		anchors: [anchor],
		origin: { candidate_id: 'candidate-1', pass_name: 'security' }
	},
	lane: 'verified' as const,
	verification: {
		id: 'verification-1',
		state: 'supported',
		refutation_attempt: 'Checked the route validator and found no wildcard path route.',
		evidence: [
			{
				id: 'evidence-1',
				relation: 'supports' as const,
				summary: 'The handler accepts a caller-provided path.',
				independent: true,
				concrete: true
			}
		]
	}
};

const workspace: ReviewWorkspace = {
	overview: {
		session: {
			id: 'session-1234',
			repository_name: 'mire',
			title: 'Captured review',
			created_at: '2026-07-15T12:00:00Z'
		},
		round: {
			id: 'round-1234',
			number: 4,
			status: 'complete',
			snapshot_id: 'snapshot-1234',
			created_at: '2026-07-15T12:01:00Z'
		},
		snapshot: {
			id: 'snapshot-1234',
			kind: 'two_dot',
			requested_comparison: 'main..feature',
			manifest_digest: 'manifest-123456789',
			created_at: '2026-07-15T12:01:00Z'
		},
		intent: { prompt: 'Review the authenticated read-only API.' },
		change: { files: [{ status: 'modified', target_path: 'internal/server/server.go' }], surfaces: [] },
		coverage: {
			examined_files: ['internal/server/server.go'],
			examined_hunks: ['server.go#abc'],
			passes: [],
			analyzers: [{ name: 'setaryb', available: false, reason: 'not configured' }],
			gaps: []
		},
		omissions: [{ kind: 'analyzer', reason: 'Setaryb was not configured.' }],
		provenance: {
			planner_runs: [],
			review_runs: [
				{
					id: 'run-review',
					role: 'reviewer',
					pass_name: 'security',
					status: 'complete',
					provenance: { adapter: 'fixture', protocol: 'fixture', model: 'deterministic' }
				}
			],
			verification_runs: []
		}
	},
	diff: {
		snapshot_id: 'snapshot-1234',
		files: [
			{
				status: 'modified',
				target_path: 'internal/server/server.go',
				surfaces: ['public_api'],
				hunks: [
					{
						id: 'server.go#abc',
						kind: 'changed',
						old_start: 10,
						old_lines: 2,
						new_start: 10,
						new_lines: 2,
						lines: ['-old handler\n', '+bounded handler\n'],
						available: true,
						digest: 'digest'
					}
				]
			}
		],
		omissions: []
	},
	slices: {
		slices: [
			{
				id: 'slice-api',
				title: 'Authenticated review API',
				file_paths: ['internal/server/server.go'],
				hunk_ids: ['server.go#abc'],
				grouping: 'public surface',
				risk_cues: ['security']
			}
		],
		risk_areas: [],
		ordering_rationale: 'Security-sensitive API work appears first.',
		files: [{ status: 'modified', target_path: 'internal/server/server.go' }]
	},
	findings: {
		verified: { lane: 'verified', findings: [finding], candidates: [] },
		candidate: {
			lane: 'candidate',
			findings: [],
			candidates: [
				{
					candidate: {
						id: 'candidate-2',
						pass_name: 'correctness',
						candidate: {
							claim: 'A status may be stale.',
							impact: 'The overview may briefly lag.',
							category: 'correctness',
							severity: 'low',
							confidence: 0.4,
							anchors: [anchor]
						}
					},
					lane: 'candidate'
				}
			]
		},
		refuted: { lane: 'refuted', findings: [], candidates: [] }
	},
	coverage: { coverage: { examined_files: [], examined_hunks: [], passes: [] }, omissions: [] },
	divergence: { status: 'unchanged', message: 'Repository still matches the captured snapshot.' }
};

test('explores ordering, diff modes, lanes, and evidence with non-color cues', async () => {
	vi.stubGlobal(
		'fetch',
		vi.fn(
			async () =>
				new Response(
					JSON.stringify({
						finding,
						artifacts: [],
						coverage: workspace.overview.coverage,
						provenance: workspace.overview.provenance
					}),
					{ status: 200 }
				)
		)
	);
	const screen = await render(ReviewOverview, {
		selectedSession: workspace.overview.session,
		selectedRound: workspace.overview.round,
		rounds: [workspace.overview.round],
		workspace,
		resourceState: 'ready'
	});

	await expect.element(screen.getByRole('heading', { name: 'Captured review' })).toBeVisible();
	await expect.element(screen.getByText('Review the authenticated read-only API.')).toBeVisible();
	await expect.element(screen.getByText('Navigation order is not proof of coverage.', { exact: false })).toBeVisible();
	await expect.element(screen.getByRole('button', { name: /ANCHOR/ })).toBeVisible();
	await screen.getByRole('button', { name: 'Side by side' }).click();
	await expect.element(screen.getByLabelText('Side-by-side diff for internal/server/server.go')).toBeVisible();

	await screen.getByRole('tab', { name: /Candidates/ }).click();
	await expect.element(screen.getByText('UNVERIFIED')).toBeVisible();
	await expect.element(screen.getByRole('button', { name: /A status may be stale/ })).toBeVisible();
	await screen.getByRole('tab', { name: /Verified/ }).click();
	await screen.getByRole('button', { name: /The read endpoint accepts/ }).click();
	await expect.element(screen.getByRole('heading', { name: 'Evidence' })).toBeVisible();
	await expect.element(screen.getByText('supports')).toBeVisible();
	await expect.element(screen.getByText('Independent · Concrete')).toBeVisible();
	await expect.element(screen.getByRole('heading', { name: 'Retrieved context' })).toBeVisible();
	await expect.element(screen.getByRole('heading', { name: 'Analyzer limitations' })).toBeVisible();
});
