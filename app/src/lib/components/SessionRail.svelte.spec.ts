import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import SessionRail from './SessionRail.svelte';

test('shows the loading state without inventing sessions', async () => {
	const screen = await render(SessionRail, {
		sessions: [],
		loadState: 'loading',
		onSelect: vi.fn(),
		onCollapse: vi.fn()
	});

	await expect.element(screen.getByRole('heading', { name: 'Sessions' })).toBeVisible();
	await expect.element(screen.getByText('—')).toBeVisible();
	await expect.element(screen.getByText('Reading private state…')).toBeVisible();
});

test('selects a session from the session list', async () => {
	const onSelect = vi.fn();
	const onCollapse = vi.fn();
	const session = {
		id: 'session-1234',
		repository_name: 'mire',
		title: 'Captured review',
		created_at: '2026-07-15T12:00:00Z'
	};
	const screen = await render(SessionRail, {
		sessions: [session],
		selectedSessionId: session.id,
		loadState: 'ready',
		onSelect,
		onCollapse
	});

	const sessionButton = screen.getByRole('button', { name: /Captured review/ });
	await expect.element(sessionButton).toBeVisible();
	await sessionButton.click();

	expect(onSelect).toHaveBeenCalledWith(session);

	await screen.getByRole('button', { name: 'Collapse review sessions' }).click();
	expect(onCollapse).toHaveBeenCalledOnce();
});
