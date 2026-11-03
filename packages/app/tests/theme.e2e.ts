import { expect, test, type MireServer } from './mire';

/** Opens the review and waits until it has loaded. */
async function open(page: import('@playwright/test').Page, url: string) {
	await page.goto(url);
	await expect(page.getByRole('banner')).toContainText('revision 3');
}

function background(page: import('@playwright/test').Page) {
	return page.evaluate(() => getComputedStyle(document.body).backgroundColor);
}

/**
 * Adds a Rust file with a comment, opens it, and checks that the comment takes
 * `--syntax-comment` from `themes.css`, which resolves to `expected`.
 */
async function expectCommentColor(page: import('@playwright/test').Page, mire: MireServer, expected: string) {
	await mire.writeSource('lib.rs', '// Counts the findings.\nfn count() -> usize {\n    0\n}\n');
	await page.getByRole('button', { name: 'Apply revision 4' }).click();
	await page.getByRole('treeitem', { name: /lib\.rs/ }).click();
	const palette = await page.evaluate(() => {
		const probe = document.body.appendChild(document.createElement('span'));
		probe.style.color = 'var(--syntax-comment)';
		const color = getComputedStyle(probe).color;
		probe.remove();
		return color;
	});
	expect(palette).toBe(expected);
	const comment = page
		.getByRole('region', { name: 'File diff' })
		.locator('[data-line] span', { hasText: '// Counts the findings.' });
	// A light palette's comment role is its text color, so check the token names the variable too.
	await expect(comment).toHaveAttribute('style', /var\(--syntax-comment\)/);
	await expect(comment).toHaveCSS('color', expected);
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
		const line = page.getByRole('button', { name: /Open medium defect, open: / });
		await expect(line).toContainText('medium');
		await expect(line).toContainText('defect · open');
		const style = await line.evaluate((element) => {
			const computed = getComputedStyle(element);
			return { background: computed.backgroundColor, rule: computed.borderLeftColor };
		});
		// --annotation and --severity-medium in Nord Light.
		expect(style).toEqual({ background: 'rgb(248, 249, 251)', rule: 'rgb(114, 78, 9)' });
	});

	test('colors a code comment with the palette, not a Shiki theme', async ({ page, mire }) => {
		await open(page, mire.url);
		// --syntax-comment in Nord Light: the terminal's comment role, its text color.
		await expectCommentColor(page, mire, 'rgb(46, 52, 64)');
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

	test('colors a code comment with the palette, not a Shiki theme', async ({ page, mire }) => {
		await open(page, mire.url);
		// --syntax-comment in Catppuccin Mocha: overlay1, lightened a step to reach 4.5:1 on the diff rows.
		await expectCommentColor(page, mire, 'rgb(138, 143, 167)');
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
