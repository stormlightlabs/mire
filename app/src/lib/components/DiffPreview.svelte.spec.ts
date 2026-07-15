import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import DiffPreview from './DiffPreview.svelte';

test('renders a read-only snapshot preview', async () => {
	const screen = await render(DiffPreview, { value: '// Snapshot abc12345' });

	await expect.element(screen.getByLabelText('Snapshot diff preview')).toBeVisible();
	await expect.element(screen.getByText('// Snapshot abc12345')).toBeVisible();
});
