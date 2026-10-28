import { expect, test } from './mire';

test.use({ mireAttributes: 'two.txt linguist-generated\n' });

test('collapses generated files until the reader opens them', async ({ page, mire }) => {
	await page.goto(mire.url);

	const header = page.getByRole('banner');
	const files = page.getByRole('tree', { name: 'Changed file tree' });
	const diff = page.getByRole('region', { name: 'File diff' });
	const group = files.getByRole('treeitem', { name: /Generated/ });
	const generated = files.getByRole('treeitem', { name: /two\.txt/ });

	// The generated file sits in a collapsed group and the first other file opens.
	await expect(header).toContainText('revision 3');
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('one.txt');
	await expect(group).toHaveAttribute('aria-expanded', 'false');
	await expect(group).toContainText('1 file, collapsed');
	await expect(generated).toHaveCount(0);

	// Expanding the group lists the file; opening it shows a collapsed diff.
	await group.click();
	await expect(group).toHaveAttribute('aria-expanded', 'true');
	await expect(generated).toContainText('generated');
	await generated.click();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');
	await expect(diff.getByRole('heading', { level: 2, name: 'Generated file' })).toBeVisible();
	await expect(diff.getByRole('button', { name: 'Unified' })).toHaveCount(0);

	// The diff was captured, so showing it needs no new request to the source.
	await diff.getByRole('button', { name: 'Show diff' }).click();
	await expect(diff.getByRole('heading', { level: 2, name: 'Generated file' })).toHaveCount(0);
	await expect(diff.getByRole('button', { name: 'Unified' })).toBeVisible();

	// The group collapses again on request.
	await group.click();
	await expect(group).toHaveAttribute('aria-expanded', 'false');
	await expect(generated).toHaveCount(0);

	// Opening the file from the review queue expands the group, with its diff collapsed again.
	await files.getByRole('treeitem', { name: /one\.txt/ }).click();
	await page
		.getByRole('complementary', { name: 'Review queue' })
		.getByRole('button', { name: /capital Y/ })
		.click();
	await expect(diff.getByRole('heading', { level: 1 })).toHaveText('two.txt');
	await expect(group).toHaveAttribute('aria-expanded', 'true');
	await expect(generated).toHaveAttribute('aria-selected', 'true');
	await expect(diff.getByRole('button', { name: 'Show diff' })).toBeVisible();

	// A reload after marking the open file viewed keeps the group as the reader left it.
	await group.click();
	await expect(group).toHaveAttribute('aria-expanded', 'false');
	const viewed = diff.getByRole('button', { name: 'Viewed' });
	await viewed.click();
	await expect(viewed).toHaveAttribute('aria-pressed', 'true');
	await expect(group).toHaveAttribute('aria-expanded', 'false');
	await expect(generated).toHaveCount(0);

	// A filter shows matching generated files, and the group cannot collapse until it clears.
	const filter = page.getByRole('searchbox', { name: 'Filter files' });
	await filter.fill('two');
	await expect(group).toHaveAttribute('aria-expanded', 'true');
	await expect(group).toBeDisabled();
	await expect(group).toContainText('1 file, shown by filter');
	await expect(generated).toContainText('viewed');
	await filter.fill('');
	await expect(group).toBeEnabled();
	await expect(group).toHaveAttribute('aria-expanded', 'false');
	await expect(generated).toHaveCount(0);
});
