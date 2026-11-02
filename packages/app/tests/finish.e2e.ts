import { readFile } from 'node:fs/promises';
import type { Page } from '@playwright/test';
import { expect, test, type MireServer } from './mire';

/** Opens the review, waits for it to load at `revision`, and opens the finish panel from the header. */
async function openPanel(page: Page, revision = 3) {
	const header = page.getByRole('banner');
	await expect(header).toContainText(`revision ${revision}`);
	await expect(page.getByRole('region', { name: 'File diff' })).toHaveAttribute('aria-busy', 'false');
	await header.getByRole('button', { name: 'Finish review' }).click();
	const panel = page.getByRole('dialog', { name: 'Finish review' });
	await expect(panel).toBeVisible();
	return panel;
}

/** The list of what is left before handoff. */
function left(page: Page) {
	return page.getByRole('dialog', { name: 'Finish review' }).getByRole('list', { name: 'Left before handoff' });
}

/** Resolves both fixture findings with the CLI, then marks both files viewed in the page, leaving nothing at revision 5. */
async function settleEverything(page: Page, mire: MireServer) {
	mire.cli('note', 'resolve', mire.reviewPath, 'note-1', '--revision', '3', '--author', 'reviewer');
	mire.cli('note', 'resolve', mire.reviewPath, 'note-2', '--revision', '4', '--author', 'reviewer');
	await page.goto(mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	const files = page.getByRole('tree', { name: 'Changed file tree' });
	await expect(page.getByRole('banner')).toContainText('revision 5');
	for (const name of ['one.txt', 'two.txt']) {
		await files.getByRole('treeitem', { name: new RegExp(name.replace('.', '\\.')) }).click();
		await expect(diff.getByRole('heading', { level: 1 })).toHaveText(name);
		await expect(diff).toHaveAttribute('aria-busy', 'false');
		const viewed = diff.getByRole('button', { name: 'Viewed' });
		await viewed.click();
		await expect(viewed).toHaveAttribute('aria-pressed', 'true');
	}
}

test('lists unviewed files and open findings, each with a way to act', async ({ page, mire }) => {
	await page.goto(mire.url);
	const panel = await openPanel(page);
	const diff = page.getByRole('region', { name: 'File diff' });

	await expect(panel).toContainText(
		'Two things are left. You can export now; nothing here approves or merges anything.'
	);
	await expect(left(page).getByRole('listitem')).toHaveText([
		/2 of 2 files not viewed\s*Next unviewed/,
		/2 open findings\s*1 medium · 1 low\s*Next open finding/
	]);
	await expect(panel.getByRole('button', { name: /approve/i })).toHaveCount(0);
	await expect(panel).not.toContainText(/approved|ready to (merge|hand off)/i);

	// The most severe open finding opens, and focus moves to the review content.
	await panel.getByRole('button', { name: 'Next open finding' }).click();
	await expect(panel).toBeHidden();
	await expect(diff.getByRole('article')).toContainText('second line should stay lowercase');
	await expect(page.locator('#review-content')).toBeFocused();

	// Next unviewed starts after the open file.
	await openPanel(page);
	await panel.getByRole('button', { name: 'Next unviewed' }).click();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');

	// A viewed mark takes the file off the count.
	await diff.getByRole('button', { name: 'Viewed' }).click();
	await expect(diff.getByRole('button', { name: 'Viewed' })).toHaveAttribute('aria-pressed', 'true');
	await openPanel(page);
	await expect(left(page)).toContainText('1 of 2 files not viewed');
	await panel.getByRole('button', { name: 'Next unviewed' }).click();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
});

test('says nothing is left and still offers every export', async ({ page, mire }) => {
	await settleEverything(page, mire);
	const panel = await openPanel(page, 5);

	await expect(panel).toContainText('Nothing is left. You can export now; nothing here approves or merges anything.');
	await expect(left(page).getByRole('listitem')).toHaveText([/Every file viewed, every finding settled/]);
	await expect(left(page).getByRole('button')).toHaveCount(0);

	const exports = panel.getByRole('list', { name: 'Export' });
	await expect(exports.getByRole('button')).toHaveText([
		/Notes as Markdown\s*mire-notes\.md/,
		/Notes as JSON\s*mire-notes\.json/,
		/Agent context\s*mire-context\.json/,
		/Patch\s*mire-changeset\.patch/
	]);

	const downloading = page.waitForEvent('download');
	await exports.getByRole('button', { name: /Patch/ }).click();
	const download = await downloading;
	expect(download.suggestedFilename()).toBe('mire-changeset.patch');
	const patch = await readFile(await download.path(), 'utf8');
	expect(patch).toContain('-beta\n+BETA\n');
	expect(patch).not.toContain('note-1');

	// The panel wrote nothing.
	expect((await mire.readReview()).revision).toBe(5);
	await panel.getByRole('button', { name: 'Keep reviewing' }).click();
	await expect(panel).toBeHidden();
	await expect(page.getByRole('banner').getByRole('button', { name: 'Finish review' })).toBeFocused();
});

test('names findings that lost their line and opens one', async ({ page, mire }) => {
	await page.goto(mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(page.getByRole('banner')).toContainText('revision 3');

	// Removing the line a finding points at leaves its anchor stale.
	await mire.writeSource('one.txt', 'alpha\ngamma\n');
	await page.getByRole('button', { name: 'Apply revision 4' }).click();
	const panel = await openPanel(page, 4);

	await expect(left(page)).toContainText('1 finding lost its line');
	await expect(left(page)).toContainText('1 stale');
	await panel.getByRole('button', { name: 'Review anchors' }).click();
	await expect(panel).toBeHidden();
	await expect(diff.getByRole('article')).toContainText('second line should stay lowercase');
	await expect(diff.getByRole('article')).toContainText('stale');
});

test('puts a held revision first and applies it from the panel', async ({ page, mire }) => {
	await page.goto(mire.url);
	await expect(page.getByRole('banner')).toContainText('revision 3');

	await mire.writeSource('two.txt', 'x\nY\nz\n');
	const held = page.getByRole('region', { name: 'Revision 4 is ready.' });
	await expect(held).toBeVisible();
	const panel = await openPanel(page);

	// Re-anchoring on the new revision changes the findings too, so the counts are not pinned here.
	await expect(left(page).getByRole('listitem').first()).toHaveText(
		/Revision 4 is not applied\s*1 file.* changed\. The counts below are for revision 3\.\s*Apply revision 4/
	);
	await panel.getByRole('button', { name: 'Apply revision 4' }).click();
	await expect(panel).toBeHidden();
	await expect(held).toBeHidden();
	await expect(page.getByRole('banner')).toContainText('revision 4');

	await openPanel(page, 4);
	await expect(left(page)).not.toContainText('Revision');
});

test('opens from focus mode and returns there', async ({ page, mire }) => {
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	await expect(header).toContainText('revision 3');
	await page.keyboard.press('b');
	const finish = header.getByRole('button', { name: 'Finish review' });
	await expect(header.getByRole('button', { name: 'Leave focus' })).toBeVisible();

	await finish.focus();
	await page.keyboard.press('Enter');
	const panel = page.getByRole('dialog', { name: 'Finish review' });
	await expect(panel).toBeVisible();
	// The first action takes focus, and Tab walks the panel.
	await expect(panel.getByRole('button', { name: 'Next unviewed' })).toBeFocused();
	await page.keyboard.press('Tab');
	await expect(panel.getByRole('button', { name: 'Next open finding' })).toBeFocused();

	await page.keyboard.press('Escape');
	await expect(panel).toBeHidden();
	await expect(finish).toBeFocused();
	await expect(header.getByRole('button', { name: 'Leave focus' })).toBeVisible();

	// Acting from focus mode keeps focus mode.
	await finish.click();
	await panel.getByRole('button', { name: 'Next open finding' }).click();
	await expect(page.getByRole('region', { name: 'File diff' }).getByRole('article')).toContainText(
		'second line should stay lowercase'
	);
	await expect(header.getByRole('button', { name: 'Leave focus' })).toBeVisible();
});

test('shows why a download failed, and clears it when the panel opens again', async ({ page, mire }) => {
	await page.goto(mire.url);
	await expect(page.getByRole('banner')).toContainText('revision 3');

	// A binary file leaves the changeset with no patch.
	await mire.writeSource('two.txt', 'x\u0000Y\n');
	await page.getByRole('button', { name: 'Apply revision 4' }).click();
	const panel = await openPanel(page, 4);
	const status = panel.getByRole('status');

	await panel.getByRole('button', { name: /Patch/ }).click();
	await expect(status).toHaveText('Patch: The changeset has binary files, which a patch cannot carry.');
	await expect(panel).toBeVisible();
	await expect(page.getByRole('banner')).toContainText('revision 4');
	await expect(page.getByRole('region', { name: 'File diff' })).toBeVisible();

	await panel.getByRole('button', { name: 'Keep reviewing' }).click();
	await expect(panel).toBeHidden();
	await openPanel(page, 4);
	await expect(status).toHaveText('');
});

test.describe('with a generated file', () => {
	test.use({ mireAttributes: 'two.txt linguist-generated\n' });

	test('lists generated files not viewed apart, and still says nothing is left', async ({ page, mire }) => {
		mire.cli('note', 'resolve', mire.reviewPath, 'note-1', '--revision', '3', '--author', 'reviewer');
		mire.cli('note', 'resolve', mire.reviewPath, 'note-2', '--revision', '4', '--author', 'reviewer');
		await page.goto(mire.url);
		const diff = page.getByRole('region', { name: 'File diff' });
		await expect(page.getByRole('banner')).toContainText('revision 5');
		await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
		await expect(diff).toHaveAttribute('aria-busy', 'false');
		const viewed = diff.getByRole('button', { name: 'Viewed' });
		await viewed.click();
		await expect(viewed).toHaveAttribute('aria-pressed', 'true');

		const panel = await openPanel(page, 5);
		await expect(panel).toContainText('Nothing is left.');
		await expect(left(page).getByRole('listitem')).toHaveText([
			/Every counted file viewed, every finding settled/,
			/1 generated file not viewed\s*Generated files are not counted\./
		]);
		await expect(left(page).getByRole('button')).toHaveCount(0);
	});
});
