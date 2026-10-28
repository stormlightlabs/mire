import { expect, test } from './mire';

/** Opens the review and waits until it has loaded. */
async function open(page: import('@playwright/test').Page, url: string) {
	await page.goto(url);
	await expect(page.getByRole('banner')).toContainText('revision 3');
}

function background(page: import('@playwright/test').Page) {
	return page.evaluate(() => getComputedStyle(document.body).backgroundColor);
}

test.describe('with a light browser and --theme iceberg', () => {
	test.use({ mireTheme: 'iceberg', colorScheme: 'light' });

	test('uses Nord Light', async ({ page, mire }) => {
		await open(page, mire.url);
		const root = page.locator('html');
		await expect(root).toHaveAttribute('data-theme-family', 'iceberg');
		await expect(root).toHaveAttribute('data-theme', 'light');
		expect(await background(page)).toBe('rgb(236, 239, 244)');
	});
});

test.describe('with a dark browser and --theme catppuccin', () => {
	test.use({ mireTheme: 'catppuccin', colorScheme: 'dark' });

	test('uses Catppuccin Mocha, and Latte after switching to light', async ({ page, mire }) => {
		await open(page, mire.url);
		const root = page.locator('html');
		await expect(root).toHaveAttribute('data-theme-family', 'catppuccin');
		await expect(root).toHaveAttribute('data-theme', 'dark');
		expect(await background(page)).toBe('rgb(30, 30, 46)');

		await page.getByRole('button', { name: 'Use light mode' }).click();
		await expect(root).toHaveAttribute('data-theme', 'light');
		expect(await background(page)).toBe('rgb(239, 241, 245)');
	});
});

test.describe('with a dark browser and no --theme', () => {
	test.use({ colorScheme: 'dark' });

	test('uses Eldritch Minimal', async ({ page, mire }) => {
		await open(page, mire.url);
		await expect(page.locator('html')).toHaveAttribute('data-theme-family', 'auto');
		expect(await background(page)).toBe('rgb(23, 25, 40)');
	});
});

test.describe('with a light browser and no --theme', () => {
	test.use({ colorScheme: 'light' });

	test('uses Nord Light, as the terminal does', async ({ page, mire }) => {
		await open(page, mire.url);
		await expect(page.locator('html')).toHaveAttribute('data-theme-family', 'auto');
		expect(await background(page)).toBe('rgb(236, 239, 244)');
	});
});
