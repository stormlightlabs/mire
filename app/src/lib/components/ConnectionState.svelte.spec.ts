import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import ConnectionState from './ConnectionState.svelte';

test('explains when the launch link is required', async () => {
	const screen = await render(ConnectionState, { loadState: 'unauthenticated', errorMessage: '', onRetry: vi.fn() });

	await expect.element(screen.getByText('401')).toBeVisible();
	await expect.element(screen.getByRole('heading', { name: 'Open the one-time launch link.' })).toBeVisible();
	await expect.element(screen.getByText('mire web')).toBeVisible();
});

test('retries an unavailable connection', async () => {
	const onRetry = vi.fn();
	const screen = await render(ConnectionState, {
		loadState: 'offline',
		errorMessage: 'The server is not running.',
		onRetry
	});

	await expect.element(screen.getByText('The server is not running.')).toBeVisible();
	await screen.getByRole('button', { name: /Retry connection/ }).click();

	expect(onRetry).toHaveBeenCalledTimes(1);
});
