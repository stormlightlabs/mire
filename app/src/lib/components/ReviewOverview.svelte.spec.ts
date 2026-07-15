import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import ReviewOverview from './ReviewOverview.svelte';

test('shows the selected round and available review capabilities', async () => {
	const screen = await render(ReviewOverview, {
		selectedSession: {
			id: 'session-1234',
			repository_name: 'mire',
			title: 'Captured review',
			created_at: '2026-07-15T12:00:00Z'
		},
		selectedRound: {
			id: 'round-1234',
			number: 4,
			status: 'captured',
			snapshot_id: 'snapshot-1234',
			created_at: '2026-07-15T12:01:00Z'
		}
	});

	await expect.element(screen.getByRole('heading', { name: 'Captured review' })).toBeVisible();
	await expect.element(screen.getByText('ROUND 4')).toBeVisible();
	await expect.element(screen.getByText('TODO: snapshot persistence and repository divergence')).toBeVisible();
	await expect.element(screen.getByText('TODO: operation progress and activity state')).toBeVisible();
	await expect.element(screen.getByText('TODO: snapshot-bound diff, slices, and finding lanes')).toBeVisible();
	await expect
		.element(screen.getByText('TODO: availability of diff, findings, evidence, operations, and human actions'))
		.toBeVisible();
	await expect.element(screen.getByText('TODO: review intent, coverage, and round summary')).toBeVisible();
});
