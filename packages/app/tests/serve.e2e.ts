import { readFile } from 'node:fs/promises';
import { expect, test } from './mire';

test('reviews a change from load to export', async ({ page, mire }) => {
	await page.goto(mire.url);

	const header = page.getByRole('banner');
	const files = page.getByRole('tree', { name: 'Changed file tree' });
	const diff = page.getByRole('region', { name: 'File diff' });
	const queue = page.getByRole('complementary', { name: 'Review queue' });
	const first = queue.getByRole('button', { name: /second line should stay lowercase/ });
	const second = queue.getByRole('button', { name: /capital Y intended/ });

	// Load: the first file opens and both findings are queued.
	await expect(header).toContainText('revision 3');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(queue).toContainText('2 open');
	await expect(first).toBeVisible();
	await expect(second).toBeVisible();

	// Navigate by click: the file tree and the queue.
	await files.getByRole('treeitem', { name: /two\.txt/ }).click();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');
	await expect(files.getByRole('treeitem', { name: /two\.txt/ })).toHaveAttribute('aria-selected', 'true');
	await first.click();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(diff.getByRole('article')).toContainText('second line should stay lowercase');

	// Navigate by keyboard: J moves to the next finding, K back.
	await page.keyboard.press('j');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');
	await expect(diff.getByRole('article')).toContainText('capital Y intended');
	await page.keyboard.press('k');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(diff.getByRole('article')).toContainText('second line should stay lowercase');

	// Decide: resolving writes the decision to the review file.
	await diff.getByRole('button', { name: 'Resolve' }).click();
	await expect(diff.getByRole('article')).toContainText('defect · resolved');
	await expect(queue).toContainText('1 open');
	await expect(header).toContainText('revision 4');
	const decided = await mire.readReview();
	expect(decided.notes.map((note) => note.status)).toEqual(['resolved', 'open']);

	// Mark the open file viewed once the reload after the decision has settled.
	const viewed = diff.getByRole('button', { name: 'Viewed' });
	await expect(diff).toHaveAttribute('aria-busy', 'false');
	await expect(viewed).toHaveAttribute('aria-pressed', 'false');
	await viewed.click();
	await expect(viewed).toHaveAttribute('aria-pressed', 'true');
	await expect(files.getByRole('treeitem', { name: /one\.txt/ })).toContainText('viewed');

	// Export: the Markdown download carries the decision.
	await header.getByRole('button', { name: 'Finish review' }).click();
	const dialog = page.getByRole('dialog', { name: 'Finish review' });
	await expect(dialog).toContainText('1 open finding');
	const downloading = page.waitForEvent('download');
	await dialog.getByRole('button', { name: 'Markdown' }).click();
	const download = await downloading;
	expect(download.suggestedFilename()).toBe('mire-notes.md');
	const markdown = await readFile(await download.path(), 'utf8');
	expect(markdown).toContain('Review revision: 4');
	expect(markdown).toMatch(/## Medium: note-1\n[\s\S]*- Status: `resolved`/);
	expect(markdown).toMatch(/## Low: note-2\n[\s\S]*- Status: `open`/);
	await dialog.getByRole('button', { name: 'Close' }).click();

	// Refresh: a source edit reaches the open file, which no longer counts as viewed.
	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	await expect(header).toContainText('revision 5');
	await expect(diff).toContainText('delta');
	await expect(diff.getByRole('button', { name: 'Changed since viewed' })).toBeVisible();
	await header.getByRole('button', { name: 'Refresh source' }).click();
	await expect(header).toContainText('revision 5 · current');
	expect((await mire.readReview()).revision).toBe(5);
});
