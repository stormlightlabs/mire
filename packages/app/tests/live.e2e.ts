import { readFile, rename, writeFile } from 'node:fs/promises';
import { expect, test } from './mire';

test('holds a new revision in focus mode until u applies it', async ({ page, mire }) => {
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	const files = page.getByRole('tree', { name: 'Changed file tree' });
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(header).toContainText('revision 3');
	await expect(diff).toHaveAttribute('aria-busy', 'false');

	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	const held = page.getByRole('region', { name: 'Revision 4 is ready.' });
	await expect(held).toContainText('1 file');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(diff).toContainText('Changed on disk at revision 4');
	await expect(diff).not.toContainText('delta');

	// The list names what changed.
	await held.getByRole('button', { name: 'See what changed' }).click();
	await expect(held.getByRole('list', { name: 'Files changed in revision 4' })).toHaveText(/changed\s+one\.txt/);

	// A file the held revision leaves alone still opens; a changed one waits.
	await files.getByRole('treeitem', { name: /two\.txt/ }).click();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');
	await files.getByRole('treeitem', { name: /one\.txt/ }).click();
	await expect(diff).toContainText('one.txt changed in revision 4. Apply it to read this file.');
	await files.getByRole('treeitem', { name: /two\.txt/ }).click();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');

	// Focus mode keeps the notice, and its key bar names the key.
	await page.keyboard.press('b');
	await expect(header.getByRole('button', { name: 'Leave focus' })).toBeVisible();
	await expect(held).toBeVisible();
	await expect(page.getByRole('contentinfo', { name: 'Keys' })).toContainText(/u\s*apply revision 4/);

	await page.keyboard.press('u');
	await expect(held).toBeHidden();
	await expect(header.getByRole('button', { name: 'Leave focus' })).toBeVisible();
	await page.keyboard.press('[');
	await expect(diff).toContainText('delta');
	await expect(page.getByRole('contentinfo', { name: 'Keys' })).not.toContainText('apply revision');
});

test('keeps a draft through a revision conflict', async ({ page, mire }) => {
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(header).toContainText('revision 3');

	await page.keyboard.press('j');
	const editor = diff.getByRole('textbox', { name: 'Finding' });
	await editor.fill('Keep BETA lowercase, as the test expects.');

	// Another writer lands a revision while the reader types.
	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	await expect(page.getByRole('region', { name: 'Revision 4 is ready.' })).toBeVisible();

	await diff.getByRole('button', { name: 'Save edit' }).click();
	const dialog = page.getByRole('dialog', { name: 'Your edit was not saved' });
	await expect(dialog).toContainText('Revision 4 was written while you edited.');
	await expect(dialog.getByRole('textbox', { name: 'Your text' })).toHaveValue(
		'Keep BETA lowercase, as the test expects.'
	);
	await expect(dialog.getByRole('button', { name: 'Copy text' })).toBeVisible();
	expect((await mire.readReview()).notes[0].body).toBe('The second line should stay lowercase.');

	await dialog.getByRole('button', { name: 'Apply revision 4 and keep draft' }).click();
	await expect(dialog).toBeHidden();
	await expect(header).toContainText('revision 4');
	await expect(diff).toContainText('delta');
	await expect(editor).toHaveValue('Keep BETA lowercase, as the test expects.');

	await diff.getByRole('button', { name: 'Save edit' }).click();
	await expect(diff.getByRole('article')).toContainText('Keep BETA lowercase');
	await expect(page.getByRole('region', { name: /is ready/ })).toHaveCount(0);
	expect((await mire.readReview()).notes[0].body).toBe('Keep BETA lowercase, as the test expects.');
});

test('shows a degraded watch and clears it when the watch recovers', async ({ page, mire }) => {
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	await expect(header).toContainText('revision 3');
	await expect(header).toContainText('watching');

	const review = await readFile(mire.reviewPath, 'utf8');
	const replaceReview = async (content: string) => {
		const staged = `${mire.reviewPath}.staged`;
		await writeFile(staged, content);
		await rename(staged, mire.reviewPath);
	};
	await replaceReview('not a review');
	const notice = page.getByRole('region', { name: 'Watching stopped.' });
	await expect(notice).toContainText('The watch is degraded');
	await expect(notice.getByRole('button', { name: 'Refresh source' })).toBeVisible();
	await expect(header).toContainText('degraded · showing revision 3');

	await replaceReview(review);
	await expect(notice).toBeHidden();
	await expect(header).toContainText('watching');
});

test('keeps the review on screen when the server shuts down', async ({ page, mire }) => {
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(header).toContainText('revision 3');

	await mire.stop();
	const notice = page.getByRole('region', { name: 'Watching stopped.' });
	await expect(notice).toContainText('mire serve has shut down');
	await expect(notice).toContainText('Run mire serve again');
	await expect(header).toContainText('stopped · showing revision 3');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
});

test('keeps the revision on screen when the held one cannot load', async ({ page, mire }) => {
	await page.goto(mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(page.getByRole('banner')).toContainText('revision 3');

	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	const held = page.getByRole('region', { name: 'Revision 4 is ready.' });
	await expect(held).toBeVisible();
	await mire.stop();
	await expect(page.getByRole('region', { name: 'Watching stopped.' })).toBeVisible();

	await page.keyboard.press('u');
	await expect(diff.getByRole('status')).toContainText('Revision 4 could not be loaded');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(held).toBeVisible();
});

test('reconnects a lost connection on Try again', async ({ page, mire }) => {
	await page.route('**/api/v1/events', (route) => route.abort());
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	const notice = page.getByRole('region', { name: 'Watching stopped.' });
	await expect(notice).toContainText('The connection to mire serve was lost');
	await expect(header).toContainText('disconnected · showing revision 3');
	await expect(page.getByRole('region', { name: 'File diff' }).getByRole('heading', { level: 1 })).toHaveText(
		'one.txt'
	);

	await page.unroute('**/api/v1/events');
	await notice.getByRole('button', { name: 'Try again' }).click();
	await expect(notice).toBeHidden();
	await expect(header).toContainText('watching');

	// Updates flow again once it reconnects.
	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	await expect(page.getByRole('region', { name: 'Revision 4 is ready.' })).toBeVisible();
});
