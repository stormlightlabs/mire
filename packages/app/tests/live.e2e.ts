import { readFile, rename, writeFile } from 'node:fs/promises';
import type { Page } from '@playwright/test';
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

test('applies a held revision before saving an edit', async ({ page, mire }) => {
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(header).toContainText('revision 3');

	await page.keyboard.press('j');
	const editor = diff.getByRole('textbox', { name: 'Finding' });
	await editor.fill('Keep BETA lowercase, as the test expects.');

	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	const held = page.getByRole('region', { name: 'Revision 4 is ready.' });
	await expect(held).toBeVisible();

	await diff.getByRole('button', { name: 'Save edit' }).click();
	await expect(diff.getByRole('article')).toContainText('Keep BETA lowercase');
	await expect(held).toBeHidden();
	await expect(page.getByRole('dialog')).toHaveCount(0);
	await expect(diff).toContainText('delta');
	expect((await mire.readReview()).notes[0].body).toBe('Keep BETA lowercase, as the test expects.');
});

test('applies a held revision before saving a decision', async ({ page, mire }) => {
	await page.goto(mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(page.getByRole('banner')).toContainText('revision 3');
	await page.keyboard.press('j');

	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	const held = page.getByRole('region', { name: 'Revision 4 is ready.' });
	await expect(held).toBeVisible();

	await diff.getByRole('button', { name: 'Resolve' }).click();
	await expect(diff.getByRole('article')).toContainText('resolved');
	await expect(held).toBeHidden();
	await expect(page.getByRole('dialog')).toHaveCount(0);
	await expect(diff).toContainText('delta');
	expect((await mire.readReview()).notes[0].status).toBe('resolved');
});

test('saves no decision when the held revision changed the status', async ({ page, mire }) => {
	await page.goto(mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(page.getByRole('banner')).toContainText('revision 3');
	await page.keyboard.press('j');
	await expect(diff.getByRole('article')).toContainText('open');

	const review = await mire.readReview();
	mire.cli(
		'note',
		'resolve',
		mire.reviewPath,
		review.notes[0].id,
		'--revision',
		String(review.revision),
		'--author',
		'other'
	);
	await expect(page.getByRole('region', { name: 'Revision 4 is ready.' })).toBeVisible();

	await diff.getByRole('button', { name: 'Dismiss' }).click();
	const dialog = page.getByRole('dialog', { name: 'Your decision was not saved' });
	await expect(dialog).toContainText("Revision 4 changed this finding's status to resolved.");
	await expect(dialog.getByRole('button', { name: 'Close' })).toBeFocused();
	await expect(page.getByRole('banner')).toContainText('revision 4');
	expect((await mire.readReview()).notes[0].status).toBe('resolved');

	await dialog.getByRole('button', { name: 'Close' }).click();
	await expect(dialog).toBeHidden();
	await expect(diff.getByRole('article')).toContainText('resolved');
});

test('keeps the text when the held revision changed the finding', async ({ page, mire }) => {
	await page.goto(mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(page.getByRole('banner')).toContainText('revision 3');
	await page.keyboard.press('j');
	const editor = diff.getByRole('textbox', { name: 'Finding' });
	await editor.fill('My rewrite.');

	// Another writer edits the same finding through the API.
	const review = await mire.readReview();
	const response = await page.request.patch(new URL(`/api/v1/findings/${review.notes[0].id}`, mire.url).href, {
		headers: { Authorization: `Bearer ${secretOf(mire.url)}`, Origin: new URL(mire.url).origin },
		data: { expectedRevision: review.revision, body: 'Their rewrite.', severity: 'medium', annotationKind: 'defect' }
	});
	expect(response.ok()).toBe(true);
	await expect(page.getByRole('region', { name: 'Revision 4 is ready.' })).toBeVisible();

	await diff.getByRole('button', { name: 'Save edit' }).click();
	const dialog = page.getByRole('dialog', { name: 'Your edit was not saved' });
	await expect(dialog).toContainText("Revision 4 changed this finding's text.");
	await expect(dialog.getByRole('textbox', { name: 'Your text' })).toHaveValue('My rewrite.');
	await expect(dialog.getByRole('button', { name: 'Keep editing' })).toBeFocused();
	expect((await mire.readReview()).notes[0].body).toBe('Their rewrite.');

	await dialog.getByRole('button', { name: 'Keep editing' }).click();
	await expect(diff.getByRole('article')).toContainText('Their rewrite.');
	await expect(editor).toHaveValue('My rewrite.');
	await diff.getByRole('button', { name: 'Save edit' }).click();
	await expect(diff.getByRole('article')).toContainText('My rewrite.');
	expect((await mire.readReview()).notes[0].body).toBe('My rewrite.');
});

test('keeps a draft when the server refuses a save', async ({ page, mire }) => {
	// Without live updates the page cannot hold the new revision, so the server refuses the save.
	await page.route('**/api/v1/events', (route) => route.abort());
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(header).toContainText('revision 3');

	await page.keyboard.press('j');
	const editor = diff.getByRole('textbox', { name: 'Finding' });
	await editor.fill('Keep BETA lowercase, as the test expects.');
	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	await expect.poll(() => serverRevision(page, mire.url)).toBe(4);

	await diff.getByRole('button', { name: 'Save edit' }).click();
	const dialog = page.getByRole('dialog', { name: 'Your edit was not saved' });
	await expect(dialog).toContainText('Revision 4 was written while you edited.');
	await expect(dialog.getByRole('textbox', { name: 'Your text' })).toHaveValue(
		'Keep BETA lowercase, as the test expects.'
	);
	await expect(dialog.getByRole('button', { name: 'Copy text' })).toBeVisible();
	const apply = dialog.getByRole('button', { name: 'Apply revision 4 and keep draft' });
	await expect(apply).toBeFocused();
	expect((await mire.readReview()).notes[0].body).toBe('The second line should stay lowercase.');

	await apply.click();
	await expect(dialog).toBeHidden();
	await expect(header).toContainText('revision 4');
	await expect(diff).toContainText('delta');
	await expect(editor).toHaveValue('Keep BETA lowercase, as the test expects.');

	// Typing takes the draft over, so a decision keeps what the reader typed, not the kept draft.
	await editor.fill('Typed after the conflict.');
	await diff.getByRole('button', { name: 'Resolve' }).click();
	await expect(diff.getByRole('article')).toContainText('resolved');
	await expect(editor).toHaveValue('Typed after the conflict.');
	await expect(diff.getByRole('article')).toContainText('The second line should stay lowercase.');

	// Another finding and back shows the saved text; the draft is gone.
	await page.keyboard.press('j');
	await expect(editor).toHaveValue('Is the capital Y intended?');
	await page.keyboard.press('k');
	await expect(editor).toHaveValue('The second line should stay lowercase.');
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
	let connects = 0;
	page.on('request', (request) => {
		if (request.url().endsWith('/api/v1/events')) connects++;
	});
	// The server sends its shutdown event only to tabs already listening.
	const listening = page.waitForResponse((response) => response.url().endsWith('/api/v1/events'));
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(header).toContainText('revision 3');
	await listening;

	await mire.stop();
	const notice = page.getByRole('region', { name: 'Watching stopped.' });
	await expect(notice).toContainText('mire serve has shut down');
	await expect(notice).toContainText('Run mire serve again');
	await expect(header).toContainText('stopped · showing revision 3');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');

	// A shut-down server does not come back, so the page stops trying. The first retry would come after 1 second.
	await page.waitForTimeout(2_500);
	expect(connects).toBe(1);
});

test('stops retrying when the server refuses the session', async ({ page, mire }) => {
	let connects = 0;
	await page.route('**/api/v1/events', (route) => {
		connects++;
		return route.fulfill({
			status: 401,
			contentType: 'application/problem+json',
			body: JSON.stringify({ code: 'unauthorized', detail: 'This API requires the session secret.' })
		});
	});
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	const notice = page.getByRole('region', { name: 'Session expired.' });
	await expect(notice).toContainText("mire serve no longer accepts this page's session");
	await expect(notice).toContainText('Open the URL mire serve printed');
	await expect(notice.getByRole('button')).toHaveCount(0);
	await expect(header).toContainText('expired · showing revision 3');

	await page.waitForTimeout(2_500);
	expect(connects).toBe(1);
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

test('holds a revision that landed while disconnected', async ({ page, mire }) => {
	await page.route('**/api/v1/events', (route) => route.abort());
	await page.goto(mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	const notice = page.getByRole('region', { name: 'Watching stopped.' });
	await expect(notice).toContainText('The connection to mire serve was lost');
	await expect(diff).toHaveAttribute('aria-busy', 'false');

	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	await expect.poll(() => serverRevision(page, mire.url)).toBe(4);

	await page.unroute('**/api/v1/events');
	await notice.getByRole('button', { name: 'Try again' }).click();
	await expect(notice).toBeHidden();
	await expect(page.getByRole('region', { name: 'Revision 4 is ready.' })).toContainText('1 file');
	await expect(page.getByRole('banner')).toContainText('revision 3');
	await expect(diff).not.toContainText('delta');
});

/** The session secret in a URL `mire serve` printed. */
function secretOf(url: string): string {
	return url.slice(url.lastIndexOf('#') + 1);
}

/** Reads the server's current review revision, outside the page and its routes. */
async function serverRevision(page: Page, url: string): Promise<number> {
	const response = await page.request.get(new URL('/api/v1/review', url).href, {
		headers: { Authorization: `Bearer ${secretOf(url)}` }
	});
	return ((await response.json()) as { revision: number }).revision;
}
