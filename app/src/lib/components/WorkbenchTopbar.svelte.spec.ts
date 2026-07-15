import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import WorkbenchTopbar from './WorkbenchTopbar.svelte';

test('shows the connected workbench state and favicon wordmark', async () => {
	const screen = await render(WorkbenchTopbar, { loadState: 'ready' });

	await expect.element(screen.getByRole('link', { name: 'Mire home' })).toBeVisible();
	await expect.element(screen.getByText('LOCAL / CONNECTED')).toBeVisible();
});
