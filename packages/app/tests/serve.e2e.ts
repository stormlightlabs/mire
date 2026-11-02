import { execFileSync } from 'node:child_process';
import { readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
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
	// The diff header draws an old path and an arrow only for a rename.
	await expect(diff.locator('[data-diffs-header]')).toContainText('one.txt');
	await expect(diff.locator('[data-prev-name]')).toHaveCount(0);
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
	await expect(dialog).toContainText('1 low');
	const downloading = page.waitForEvent('download');
	await dialog.getByRole('button', { name: /Notes as Markdown/ }).click();
	const download = await downloading;
	expect(download.suggestedFilename()).toBe('mire-notes.md');
	const markdown = await readFile(await download.path(), 'utf8');
	expect(markdown).toContain('Review revision: 4');
	expect(markdown).toMatch(/## Medium: note-1\n[\s\S]*- Status: `resolved`/);
	expect(markdown).toMatch(/## Low: note-2\n[\s\S]*- Status: `open`/);
	await dialog.getByRole('button', { name: 'Keep reviewing' }).click();

	// Refresh: a source edit waits until the reader applies it. Then it reaches
	// the open file, which no longer counts as viewed.
	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	const held = page.getByRole('region', { name: 'Revision 5 is ready.' });
	await expect(held).toContainText('1 file');
	await expect(header).toContainText('revision 4');
	await expect(diff).not.toContainText('delta');
	await held.getByRole('button', { name: 'Apply revision 5' }).click();
	await expect(held).toBeHidden();
	await expect(header).toContainText('revision 5');
	await expect(diff).toContainText('delta');
	await expect(diff.getByRole('button', { name: 'Changed since viewed' })).toBeVisible();
	await header.getByRole('button', { name: 'Refresh source' }).click();
	await expect(header).toContainText('revision 5 · current');
	expect((await mire.readReview()).revision).toBe(5);
});

test('holds a source change when a viewed reload overtakes its slow reload', async ({ page, mire }) => {
	await page.goto(mire.url);

	const header = page.getByRole('banner');
	const files = page.getByRole('tree', { name: 'Changed file tree' });
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(header).toContainText('revision 3');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(diff).toHaveAttribute('aria-busy', 'false');

	// Hold the response to the next overview request, the reload the source edit starts.
	let hold = false;
	let held: () => void = () => {};
	const release = new Promise<void>((resolve) => (held = resolve));
	let holding: () => void = () => {};
	const holdingStarted = new Promise<void>((resolve) => (holding = resolve));
	await page.route('**/api/v1/review', async (route) => {
		if (!hold) return route.continue();
		hold = false;
		const response = await route.fetch();
		holding();
		await release;
		await route.fulfill({ response });
	});
	hold = true;
	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	await holdingStarted;

	// Marking the old content viewed reloads the overview, which overtakes the
	// slow reload. The mark shows at once; the new revision waits.
	const viewed = diff.getByRole('button', { name: 'Viewed' });
	await viewed.click();
	await expect(viewed).toHaveAttribute('aria-pressed', 'true');
	const ready = page.getByRole('region', { name: 'Revision 4 is ready.' });
	await expect(ready).toBeVisible();
	await expect(header).toContainText('revision 3');
	await expect(diff).not.toContainText('delta');

	// The slow response arrives late and changes nothing.
	held();
	await expect(ready).toBeVisible();
	await expect(header).toContainText('revision 3');
	await expect(diff).not.toContainText('delta');

	// Applying shows the new content, which no longer counts as viewed.
	await ready.getByRole('button', { name: 'Apply revision 4' }).click();
	await expect(header).toContainText('revision 4');
	await expect(diff).toContainText('delta');
	await expect(files.getByRole('treeitem', { name: /one\.txt/ })).toHaveAttribute('aria-selected', 'true');
	await expect(diff.getByRole('button', { name: 'Changed since viewed' })).toBeVisible();
});

test('empties the diff pane when the last file leaves the review', async ({ page, mire }) => {
	await page.goto(mire.url);

	const files = page.getByRole('tree', { name: 'Changed file tree' });
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');

	await mire.writeSource('one.txt', 'alpha\nbeta\ngamma\n');
	await mire.writeSource('two.txt', 'x\ny\n');

	// The two edits may land as one revision or two, so apply until none is left.
	await expect(async () => {
		const apply = page.getByRole('button', { name: /^Apply revision/ });
		if (await apply.isVisible()) await apply.click();
		await expect(diff.getByRole('heading', { level: 1 })).toHaveText('No changed files', { timeout: 1_000 });
	}).toPass();
	await expect(page.getByRole('region', { name: /is ready/ })).toHaveCount(0);
	await expect(diff.getByRole('status')).toHaveText(/is no longer in the review\./);
	await expect(diff.getByRole('button', { name: /Viewed/ })).toHaveCount(0);
	await expect(files.getByRole('treeitem')).toHaveCount(0);
});

test('the diff header shows the old path of a renamed and changed file', async ({ page, mire }) => {
	// Move two.txt and add a line, and mark the new path intent-to-add so the
	// working tree diff pairs the two as a rename with a content change.
	await writeFile(path.join(mire.root, 'two-moved.txt'), 'x\ny\nz\n');
	await rm(path.join(mire.root, 'two.txt'));
	execFileSync('git', ['add', '--intent-to-add', 'two-moved.txt'], { cwd: mire.root, stdio: 'pipe' });

	await page.goto(mire.url);
	const header = page.getByRole('banner');
	await header.getByRole('button', { name: 'Refresh source' }).click();
	const files = page.getByRole('tree', { name: 'Changed file tree' });
	const diff = page.getByRole('region', { name: 'File diff' });
	await files.getByRole('treeitem', { name: /two-moved\.txt/ }).click();

	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two-moved.txt');
	await expect(diff.locator('[data-prev-name]')).toContainText('two.txt');
	await expect(diff.locator('[data-diffs-header]')).toContainText('two-moved.txt');
});
