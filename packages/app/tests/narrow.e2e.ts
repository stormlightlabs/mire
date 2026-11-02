import type { Page } from '@playwright/test';
import { expect, test } from './mire';

/** A line too long for a phone screen, which must wrap rather than scroll the page sideways. */
const LONG_LINE =
	'pub const CURRENT_REVIEW_SCHEMA_VERSION: SchemaVersion = SchemaVersion { major: 1, minor: 4 }; // wraps at phone width';

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

/** Opens the review and waits until the first file's diff has loaded. */
async function open(page: Page, url: string) {
	await page.goto(url);
	await expect(page.getByRole('banner')).toContainText('revision 3');
	await expect(page.getByRole('region', { name: 'File diff' })).toHaveAttribute('aria-busy', 'false');
}

/**
 * Checks that the page does not scroll sideways and that no visible element
 * reaches past either edge of the screen, where the app shell would clip it.
 */
async function expectNoSidewaysOverflow(page: Page) {
	const result = await page.evaluate(() => {
		const width = window.innerWidth;
		const outside = [...document.querySelectorAll<HTMLElement>('body *')]
			.filter((element) => element.checkVisibility())
			.filter((element) => {
				const box = element.getBoundingClientRect();
				return box.width > 1 && (box.right > width + 0.5 || box.left < -0.5);
			})
			.map((element) => `${element.tagName.toLowerCase()}.${element.className}`);
		return { scrollWidth: document.scrollingElement!.scrollWidth, width, outside };
	});
	expect(result.scrollWidth).toBeLessThanOrEqual(result.width);
	expect(result.outside).toEqual([]);
}

/** The bar at the bottom of a narrow screen. */
function tabs(page: Page) {
	return page.getByRole('navigation', { name: 'Review sections' });
}

test('wraps long lines and keeps the held revision bar on screen', async ({ page, mire }) => {
	await open(page, mire.url);
	const header = page.getByRole('banner');
	const diff = page.getByRole('region', { name: 'File diff' });
	// The header keeps the review's title, and the file list and queue move to the tab bar.
	await expect(header.getByText('Local review')).toBeVisible();
	await expect(tabs(page).getByRole('button')).toHaveText([/Files/, /Findings/, /Focus/, /Finish/]);
	await expect(diff.getByRole('button', { name: 'Split' })).toHaveCount(0);
	await expectNoSidewaysOverflow(page);

	await mire.writeSource('one.txt', `alpha\nBETA\ngamma\n${LONG_LINE}\n`);
	const held = page.getByRole('region', { name: 'Revision 4 is ready.' });
	await held.getByRole('button', { name: 'See what changed' }).click();
	await expect(held.getByRole('list', { name: 'Files changed in revision 4' })).toBeVisible();
	await expectNoSidewaysOverflow(page);

	await held.getByRole('button', { name: /Apply revision 4/ }).click();
	await expect(header).toContainText('revision 4');
	const line = diff.getByText(LONG_LINE);
	await expect(line).toBeVisible();
	const short = await diff.getByText('gamma', { exact: true }).boundingBox();
	const long = await line.boundingBox();
	// The long line takes several rows and ends inside the screen.
	expect(long!.height).toBeGreaterThan(short!.height * 2);
	expect(long!.x + long!.width).toBeLessThanOrEqual(390);
	await expectNoSidewaysOverflow(page);
});

test('reaches the file list and the queue, and decides a finding', async ({ page, mire }) => {
	await open(page, mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(page.getByRole('tree', { name: 'Changed file tree' })).toHaveCount(0);

	await tabs(page).getByRole('button', { name: /Files/ }).click();
	const files = page.getByRole('dialog', { name: 'Changed files' });
	await expectNoSidewaysOverflow(page);
	await files.getByRole('treeitem', { name: /two\.txt/ }).click();
	await expect(files).toBeHidden();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');

	await tabs(page)
		.getByRole('button', { name: /2 open Findings/ })
		.click();
	const queue = page.getByRole('dialog', { name: 'Review queue' });
	await expectNoSidewaysOverflow(page);
	await queue.getByRole('button', { name: /second line should stay lowercase/ }).click();
	await expect(queue).toBeHidden();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(diff.getByRole('article')).toContainText('The second line should stay lowercase.');
	await expectNoSidewaysOverflow(page);

	await diff.getByRole('button', { name: 'Resolve' }).click();
	await expect(diff.getByRole('article')).toContainText('resolved');
	await expect.poll(async () => (await mire.readReview()).notes[0].status).toBe('resolved');
	await expect(tabs(page).getByRole('button', { name: /1 open Findings/ })).toBeVisible();
});

test('fits focus mode and the finish panel', async ({ page, mire }) => {
	await open(page, mire.url);
	const header = page.getByRole('banner');

	const focus = tabs(page).getByRole('button', { name: 'Focus' });
	await focus.click();
	await expect(header.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(tabs(page)).toHaveCount(0);
	await expect(header.getByRole('button', { name: 'Leave focus' })).toBeInViewport({ ratio: 1 });
	await expectNoSidewaysOverflow(page);

	await header.getByRole('button', { name: 'Finish review' }).click();
	const panel = page.getByRole('dialog', { name: 'Finish review' });
	await expect(panel.getByRole('button', { name: 'Next open finding' })).toBeInViewport({ ratio: 1 });
	await expectNoSidewaysOverflow(page);
	await panel.getByRole('button', { name: 'Keep reviewing' }).click();
	await expect(panel).toBeHidden();

	await header.getByRole('button', { name: 'Leave focus' }).click();
	await expect(focus).toBeFocused();

	await tabs(page).getByRole('button', { name: 'Finish' }).click();
	await expect(panel).toBeVisible();
	await expectNoSidewaysOverflow(page);
});

test('fits the stopped watch bar and a conflict dialog', async ({ page, mire }) => {
	// Without live updates the page cannot hold the new revision, so the server refuses the save.
	await page.route('**/api/v1/events', (route) => route.abort());
	await open(page, mire.url);
	const diff = page.getByRole('region', { name: 'File diff' });
	await expect(page.getByRole('region', { name: 'Watching stopped.' })).toBeVisible();
	await expectNoSidewaysOverflow(page);

	await page.getByRole('button', { name: /Open medium defect/ }).click();
	await diff.getByRole('textbox', { name: 'Finding' }).fill('Keep BETA lowercase, as the test expects.');
	await mire.writeSource('one.txt', 'alpha\nBETA\ngamma\ndelta\n');
	await expect.poll(() => serverRevision(page, mire.url)).toBe(4);

	await diff.getByRole('button', { name: 'Save edit' }).click();
	const dialog = page.getByRole('dialog', { name: 'Your edit was not saved' });
	await expect(dialog.getByRole('button', { name: 'Copy text' })).toBeInViewport({ ratio: 1 });
	await expectNoSidewaysOverflow(page);
});

test.describe('with a generated file', () => {
	test.use({ mireAttributes: 'two.txt linguist-generated\n' });

	test('fits the generated group and its collapsed diff', async ({ page, mire }) => {
		await open(page, mire.url);
		const diff = page.getByRole('region', { name: 'File diff' });

		await tabs(page).getByRole('button', { name: /Files/ }).click();
		const files = page.getByRole('dialog', { name: 'Changed files' });
		const group = files.getByRole('treeitem', { name: /Generated/ });
		await group.click();
		await expect(group).toHaveAttribute('aria-expanded', 'true');
		await expectNoSidewaysOverflow(page);
		await files.getByRole('treeitem', { name: /two\.txt/ }).click();
		await expect(diff.getByRole('heading', { level: 2, name: 'Generated file' })).toBeVisible();
		await expectNoSidewaysOverflow(page);
	});
});

/** Reads the server's current review revision, outside the page and its routes. */
async function serverRevision(page: Page, url: string): Promise<number> {
	const secret = url.slice(url.lastIndexOf('#') + 1);
	const response = await page.request.get(new URL('/api/v1/review', url).href, {
		headers: { Authorization: `Bearer ${secret}` }
	});
	return ((await response.json()) as { revision: number }).revision;
}
