import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import type { DiffFile } from '$lib/types';
import DiffViewer from './DiffViewer.svelte';

const files: DiffFile[] = [
	{
		status: 'modified',
		target_path: 'internal/server/server.go',
		hunks: [
			{
				id: 'server.go#abc',
				kind: 'changed',
				old_start: 10,
				old_lines: 2,
				new_start: 10,
				new_lines: 2,
				lines: [' package server\n', '-func open(path string) {}\n', '+func open(id string) {}\n'],
				available: true,
				digest: 'digest'
			}
		]
	}
];

test('renders a searchable read-only unified CodeMirror diff', async () => {
	const screen = await render(DiffViewer, { files, mode: 'unified', onSelectAnchor: vi.fn() });
	await expect.element(screen.getByLabelText('Unified diff for internal/server/server.go')).toBeVisible();
	await expect
		.element(screen.getByLabelText('Unified code for internal/server/server.go'))
		.toHaveTextContent('id string');
});

test('renders aligned read-only before and after editors', async () => {
	const screen = await render(DiffViewer, { files, mode: 'split', onSelectAnchor: vi.fn() });
	await expect.element(screen.getByLabelText('Side-by-side diff for internal/server/server.go')).toBeVisible();
	await expect
		.element(screen.getByLabelText('Before internal/server/server.go'))
		.toHaveTextContent('func open(path string) {}');
	await expect
		.element(screen.getByLabelText('After internal/server/server.go'))
		.toHaveTextContent('func open(id string) {}');
});

test('mounts editors only when their file is expanded', async () => {
	const secondFile: DiffFile = {
		...files[0],
		target_path: 'internal/server/other.go',
		hunks: [{ ...files[0].hunks![0], id: 'other.go#def' }]
	};
	const screen = await render(DiffViewer, {
		files: [...files, secondFile],
		mode: 'unified',
		onSelectAnchor: vi.fn()
	});

	const secondFileButton = screen.getByRole('button', { name: /internal\/server\/other.go/ });
	await expect.element(secondFileButton).toHaveAttribute('aria-expanded', 'false');
	await secondFileButton.click();
	await expect.element(screen.getByLabelText('Unified diff for internal/server/other.go')).toBeVisible();
});
