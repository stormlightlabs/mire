import { expect, test } from './mire';

test('header icons draw under the content security policy', async ({ page, mire }) => {
	const violations: string[] = [];
	page.on('console', (message) => {
		if (message.type() === 'error' && message.text().includes('Content Security Policy')) {
			violations.push(message.text());
		}
	});
	await page.goto(mire.url);
	const header = page.getByRole('banner');
	await expect(header).toContainText('revision 3');

	const icons = header.locator('.topbar-actions .icon');
	await expect(icons).toHaveCount(3);
	for (const icon of await icons.all()) {
		const box = await icon.boundingBox();
		expect(box?.width).toBeGreaterThan(0);
		expect(box?.height).toBeGreaterThan(0);
		// A blocked mask leaves the icon's box empty, so it looks the same
		// whether the icon is opaque or transparent.
		const shown = await icon.screenshot();
		await icon.evaluate((element) => ((element as HTMLElement).style.opacity = '0'));
		const hidden = await icon.screenshot();
		await icon.evaluate((element) => ((element as HTMLElement).style.opacity = ''));
		expect(shown.equals(hidden), `${await icon.getAttribute('class')} drew nothing`).toBe(false);
	}
	expect(violations).toEqual([]);
});
