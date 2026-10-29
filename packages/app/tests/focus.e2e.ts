import { expect, test } from './mire';

test('focus mode shows one file and leaves by key or button', async ({ page, mire }) => {
	await page.goto(mire.url);

	const header = page.getByRole('banner');
	const files = page.getByRole('tree', { name: 'Changed file tree' });
	const queue = page.getByRole('complementary', { name: 'Review queue' });
	const diff = page.getByRole('region', { name: 'File diff' });
	const keys = page.getByRole('contentinfo', { name: 'Keys' });
	await expect(header).toContainText('revision 3');

	// The button enters focus: the panels go, and the bar names the file and its place.
	await header.getByRole('button', { name: 'Focus' }).click();
	await expect(files).toHaveCount(0);
	await expect(queue).toHaveCount(0);
	await expect(header.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(header).toContainText('Modified · +1 −1');
	await expect(header).toContainText('1 of 2');
	await expect(header.getByRole('button', { name: 'Previous file' })).toBeDisabled();
	await expect(keys).toContainText('file');
	await expect(diff.getByRole('button', { name: 'Unified' })).toBeVisible();

	// ] and [ move between files, and the arrow buttons do the same.
	await page.keyboard.press(']');
	await expect(header.getByRole('heading', { level: 1 })).toHaveText('two.txt');
	await expect(header).toContainText('2 of 2');
	await expect(header.getByRole('button', { name: 'Next file' })).toBeDisabled();
	await page.keyboard.press('[');
	await expect(header.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await header.getByRole('button', { name: 'Next file' }).click();
	await expect(header.getByRole('heading', { level: 1 })).toHaveText('two.txt');
	await header.getByRole('button', { name: 'Previous file' }).click();
	await expect(header.getByRole('heading', { level: 1 })).toHaveText('one.txt');

	// The Viewed box marks the file and clears the mark, and keys still work after clicking it.
	const viewed = header.getByRole('checkbox', { name: 'Viewed' });
	await expect(diff).toHaveAttribute('aria-busy', 'false');
	await expect(viewed).not.toBeChecked();
	await viewed.click();
	await expect(viewed).toBeChecked();
	await viewed.click();
	await expect(viewed).not.toBeChecked();
	await viewed.click();
	await expect(viewed).toBeChecked();

	// Esc leaves focus, and the tree shows the mark made in focus.
	await page.keyboard.press('Escape');
	await expect(files).toBeVisible();
	await expect(queue).toBeVisible();
	await expect(keys).toHaveCount(0);
	await expect(files.getByRole('treeitem', { name: /one\.txt/ })).toContainText('viewed');

	// B enters focus and the Leave focus button leaves it.
	await page.keyboard.press('b');
	await expect(files).toHaveCount(0);
	await header.getByRole('button', { name: 'Leave focus' }).click();
	await expect(files).toBeVisible();

	// B toggles focus off too, and [ and ] move between files outside focus.
	await page.keyboard.press('b');
	await expect(files).toHaveCount(0);
	await page.keyboard.press('b');
	await expect(files).toBeVisible();
	await page.keyboard.press(']');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');

	// Shortcuts do not fire while typing in a text field.
	await page.getByRole('searchbox', { name: 'Filter files' }).fill('b');
	await expect(files).toBeVisible();
});

test.describe('with a generated file', () => {
	test.use({ mireAttributes: 'two.txt linguist-generated\n' });

	test('focus mode keeps the Show diff gate', async ({ page, mire }) => {
		await page.goto(mire.url);
		const header = page.getByRole('banner');
		const diff = page.getByRole('region', { name: 'File diff' });
		await expect(header).toContainText('revision 3');

		await page.keyboard.press('b');
		await page.keyboard.press(']');
		await expect(header.getByRole('heading', { level: 1 })).toHaveText('two.txt');
		await expect(header).toContainText('generated');
		await expect(diff.getByRole('heading', { level: 2, name: 'Generated file' })).toBeVisible();
		await diff.getByRole('button', { name: 'Show diff' }).click();
		await expect(diff.getByRole('button', { name: 'Unified' })).toBeVisible();

		// Leaving and returning to the file starts its diff collapsed again.
		await page.keyboard.press('[');
		await expect(header.getByRole('heading', { level: 1 })).toHaveText('one.txt');
		await page.keyboard.press(']');
		await expect(diff.getByRole('button', { name: 'Show diff' })).toBeVisible();
	});
});
