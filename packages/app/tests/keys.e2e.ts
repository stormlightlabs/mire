import { execFileSync } from 'node:child_process';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { expect, test, type MireServer } from './mire';

/** A file long enough that its hunks need scrolling to reach on a short screen. */
const LONG_FILE = Array.from({ length: 200 }, (_, index) => `line ${index + 1}`);

/** The new line number of each hunk's first row in `long.txt`, three lines above its change. */
const HUNK_STARTS = [2, 57, 107, 157, 182];

/** Opens the review and waits until the first file's diff has loaded. */
async function open(page: Page, url: string) {
	await page.goto(url);
	await expect(page.getByRole('banner')).toContainText('revision 3');
	await expect(page.getByRole('region', { name: 'File diff' })).toHaveAttribute('aria-busy', 'false');
}

/**
 * Commits `long.txt`, then changes one line in each of five places, so the
 * working tree diff has the hunks `HUNK_STARTS` lists.
 */
async function addLongFile(mire: MireServer) {
	const git = (...args: string[]) => execFileSync('git', args, { cwd: mire.root, stdio: 'pipe' });
	await writeFile(path.join(mire.root, 'long.txt'), `${LONG_FILE.join('\n')}\n`);
	git('add', 'long.txt');
	git('-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'long file');
	const changed = LONG_FILE.map((line, index) => (HUNK_STARTS.includes(index - 2) ? line.toUpperCase() : line));
	await mire.writeSource('long.txt', `${changed.join('\n')}\n`);
}

/** How far the code row for new line `line` sits below the top of the diff pane, in pixels. */
async function rowOffset(diff: Locator, line: number): Promise<number> {
	const pane = await diff.boundingBox();
	const row = await diff.locator(`[data-line="${line}"]`).first().boundingBox();
	if (!pane || !row) throw new Error(`No box for line ${line}`);
	return row.y - pane.y;
}

test.describe('on a short screen', () => {
	test.use({ viewport: { width: 1280, height: 400 } });

	test('{ and } jump between the hunks of the shown file', async ({ page, mire }) => {
		// The watcher may record the change before the page loads, so refresh rather than wait for a revision.
		await addLongFile(mire);
		await page.goto(mire.url);
		const diff = page.getByRole('region', { name: 'File diff' });
		await page.getByRole('banner').getByRole('button', { name: 'Refresh source' }).click();
		await page
			.getByRole('tree', { name: 'Changed file tree' })
			.getByRole('treeitem', { name: /long\.txt/ })
			.click();
		await expect(diff.getByRole('heading', { level: 1 })).toHaveText('long.txt');
		await expect(diff.locator('[data-line="182"]').first()).toBeAttached();
		const scrollTop = () => diff.evaluate((pane) => pane.scrollTop);
		const furthest = () => diff.evaluate((pane) => pane.scrollHeight - pane.clientHeight);
		// The separator above a hunk takes the top, with the hunk's first row just below it.
		const atTop = async (line: number) => {
			const offset = await rowOffset(diff, line);
			return offset >= 0 && offset < 64;
		};
		expect(await scrollTop()).toBe(0);

		// } brings each hunk to the top of the pane, until the pane can scroll no further.
		for (const line of [2, 57, 107]) {
			await page.keyboard.press('}');
			await expect.poll(() => atTop(line)).toBe(true);
		}
		await page.keyboard.press('}');
		await page.keyboard.press('}');
		await expect.poll(scrollTop).toBe(await furthest());
		await page.keyboard.press('}');
		await page.waitForTimeout(100);
		expect(await scrollTop()).toBe(await furthest());

		// { goes back a hunk at a time, and stops at the first.
		for (const line of [157, 107, 57, 2]) {
			await page.keyboard.press('{');
			await expect.poll(() => atTop(line)).toBe(true);
		}
		const first = await scrollTop();
		await page.keyboard.press('{');
		await page.waitForTimeout(100);
		expect(await scrollTop()).toBe(first);

		// The jumps follow the split layout too.
		await page.keyboard.press('2');
		await expect(diff.locator('pre[data-diff-type="split"]')).toHaveCount(1);
		await page.keyboard.press('}');
		await expect.poll(() => atTop(57)).toBe(true);
	});
});

test('1, 2 and w switch the layout and wrapping', async ({ page, mire }) => {
	await open(page, mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	const unified = diff.getByRole('button', { name: 'Unified' });
	const split = diff.getByRole('button', { name: 'Split' });
	const wrap = diff.getByRole('button', { name: 'Wrap' });
	await expect(unified).toHaveAttribute('aria-pressed', 'true');
	await expect(wrap).toHaveAttribute('aria-pressed', 'false');
	await expect(diff.locator('pre[data-overflow="scroll"]')).toHaveCount(1);

	await page.keyboard.press('2');
	await expect(split).toHaveAttribute('aria-pressed', 'true');
	await expect(diff.locator('pre[data-diff-type="split"]')).toHaveCount(1);
	await page.keyboard.press('1');
	await expect(unified).toHaveAttribute('aria-pressed', 'true');
	await expect(diff.locator('pre[data-diff-type="single"]')).toHaveCount(1);

	await page.keyboard.press('w');
	await expect(wrap).toHaveAttribute('aria-pressed', 'true');
	await expect(diff.locator('pre[data-overflow="wrap"]')).toHaveCount(1);
	await page.keyboard.press('w');
	await expect(wrap).toHaveAttribute('aria-pressed', 'false');
	await expect(diff.locator('pre[data-overflow="scroll"]')).toHaveCount(1);

	// The Wrap button changes the same state the key does.
	await wrap.click();
	await expect(diff.locator('pre[data-overflow="wrap"]')).toHaveCount(1);
	await page.keyboard.press('w');
	await expect(wrap).toHaveAttribute('aria-pressed', 'false');

	// The keys do nothing while typing in a text field or with a dialog open.
	// Real key presses, since `fill` sends no keydown for the page to act on.
	const filter = page.getByRole('searchbox', { name: 'Filter files' });
	await filter.focus();
	await page.keyboard.press('2');
	await page.keyboard.press('w');
	await expect(filter).toHaveValue('2w');
	await expect(unified).toHaveAttribute('aria-pressed', 'true');
	await expect(wrap).toHaveAttribute('aria-pressed', 'false');
	await expect(diff.locator('pre[data-diff-type="single"][data-overflow="scroll"]')).toHaveCount(1);
	await filter.fill('');
	await page.getByRole('banner').getByRole('button', { name: 'Finish review' }).click();
	const finish = page.getByRole('dialog');
	await expect(finish).toBeVisible();
	await page.keyboard.press('2');
	await page.keyboard.press('w');
	await page.keyboard.press('Escape');
	await expect(finish).toBeHidden();
	await expect(unified).toHaveAttribute('aria-pressed', 'true');
	await expect(wrap).toHaveAttribute('aria-pressed', 'false');
});

test('p and P move between findings as j and k do', async ({ page, mire }) => {
	await open(page, mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	const finding = diff.getByRole('article');

	await page.keyboard.press('p');
	await expect(finding).toContainText('The second line should stay lowercase.');
	await page.keyboard.press('p');
	await expect(finding).toContainText('Is the capital Y intended?');
	await page.keyboard.press('P');
	await expect(finding).toContainText('The second line should stay lowercase.');
});

test('the focus key bar lists the display keys', async ({ page, mire }) => {
	await open(page, mire.url);
	await page.keyboard.press('b');
	const keys = page.getByRole('contentinfo', { name: 'Keys' });
	await expect(keys).toContainText('{} hunk');
	await expect(keys).toContainText('jkpP finding');
	await expect(keys).toContainText('12 unified or split');
	await expect(keys).toContainText('w wrap');
});
