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

	test('sets a finding apart from the code with its own surface and a severity rule and word', async ({
		page,
		mire
	}) => {
		await open(page, mire.url);
		const line = page.getByRole('button', { name: /Open medium finding/ });
		await expect(line).toContainText('medium');
		await expect(line).toContainText('defect · open');
		const style = await line.evaluate((element) => {
			const computed = getComputedStyle(element);
			return { background: computed.backgroundColor, rule: computed.borderLeftColor };
		});
		// --annotation and --severity-medium in Nord Light.
		expect(style).toEqual({ background: 'rgb(248, 249, 251)', rule: 'rgb(120, 82, 14)' });
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
