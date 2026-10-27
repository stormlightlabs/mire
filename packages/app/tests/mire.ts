import { test as base } from '@playwright/test';
import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';

/** The banner `mire serve` prints once it is listening. */
const BANNER = /^Mire review server: (http:\/\/\S+\/#\S+)$/;

/** A temporary Git repository with a review served by a running `mire serve`. */
export type MireServer = {
	/** The session URL `mire serve` printed, secret included. */
	url: string;
	/** The repository root whose working tree the review compares. */
	root: string;
	/** Reads the review file the server writes. */
	readReview: () => Promise<{ revision: number; notes: { id: string; status: string; body: string }[] }>;
	/** Replaces a file in the repository's working tree. */
	writeSource: (file: string, content: string) => Promise<void>;
};

/** The binary under test: `$MIRE_BIN`, or the workspace's debug build. */
export function mireBinary(): string {
	const binary = process.env.MIRE_BIN ?? path.resolve(import.meta.dirname, '../../../target/debug/mire');
	if (!existsSync(binary)) {
		throw new Error(`No mire binary at ${binary}. Run \`cargo build -p mire\` or set MIRE_BIN.`);
	}
	return binary;
}

/**
 * Playwright's `test` with a `mire` fixture. Each test gets its own repository
 * with two changed files, a review holding one finding on each, and a server.
 * The fixture stops the server and removes the directory even when the test or
 * the setup fails.
 */
export const test = base.extend<{ mire: MireServer }>({
	// Playwright reads the fixture's parameters from this destructuring pattern.
	// eslint-disable-next-line no-empty-pattern
	mire: async ({}, use) => {
		const directory = await mkdtemp(path.join(tmpdir(), 'mire-e2e-'));
		let child: ChildProcess | null = null;
		try {
			const binary = mireBinary();
			const root = path.join(directory, 'repository');
			const review = path.join(directory, 'review.json');
			await createRepository(root);
			const mire = (...args: string[]) => execFileSync(binary, args, { cwd: root, stdio: 'pipe' });
			mire('review', 'init', review);
			addFinding(mire, review, 1, 'one.txt', 'medium', 'defect', 'The second line should stay lowercase.');
			addFinding(mire, review, 2, 'two.txt', 'low', 'question', 'Is the capital Y intended?');

			child = spawn(binary, ['serve', review], { cwd: root, stdio: ['ignore', 'pipe', 'inherit'] });
			const url = await readBanner(child);
			await use({
				url,
				root,
				readReview: async () => JSON.parse(await readFile(review, 'utf8')),
				writeSource: (file, content) => writeFile(path.join(root, file), content)
			});
		} finally {
			if (child) await stop(child);
			await rm(directory, { recursive: true, force: true });
		}
	}
});

export { expect } from '@playwright/test';

async function createRepository(root: string) {
	await mkdir(root, { recursive: true });
	const git = (...args: string[]) => execFileSync('git', args, { cwd: root, stdio: 'pipe' });
	git('init', '--quiet');
	git('config', 'user.name', 'Mire Tests');
	git('config', 'user.email', 'mire@example.invalid');
	await writeFile(path.join(root, 'one.txt'), 'alpha\nbeta\ngamma\n');
	await writeFile(path.join(root, 'two.txt'), 'x\ny\n');
	git('add', '.');
	git('-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'base');
	await writeFile(path.join(root, 'one.txt'), 'alpha\nBETA\ngamma\n');
	await writeFile(path.join(root, 'two.txt'), 'x\nY\n');
}

function addFinding(
	mire: (...args: string[]) => Buffer,
	review: string,
	revision: number,
	file: string,
	severity: string,
	kind: string,
	body: string
) {
	mire(
		'note',
		'add',
		review,
		'--revision',
		String(revision),
		'--file',
		file,
		'--new-line',
		'2',
		'--author',
		'reviewer',
		'--provenance',
		'agent',
		'--producer',
		'e2e',
		'--severity',
		severity,
		'--kind',
		kind,
		'--body',
		body
	);
}

/** Resolves with the session URL from the server's first line of output. */
function readBanner(child: ChildProcess): Promise<string> {
	return new Promise((resolve, reject) => {
		const lines = createInterface({ input: child.stdout! });
		const fail = (reason: string) => {
			lines.close();
			reject(new Error(reason));
		};
		lines.once('line', (line) => {
			lines.close();
			const match = BANNER.exec(line.trim());
			if (match) resolve(match[1]);
			else fail(`Unexpected mire serve banner: ${JSON.stringify(line)}`);
		});
		child.once('error', (error) => fail(`mire serve did not start: ${error.message}`));
		child.once('exit', (code) => fail(`mire serve exited with ${code} before printing its URL`));
	});
}

/** Stops the server and waits for it to exit. */
async function stop(child: ChildProcess) {
	if (child.exitCode !== null || child.signalCode !== null) return;
	const exited = new Promise((resolve) => child.once('exit', resolve));
	child.kill('SIGTERM');
	const timeout = setTimeout(() => child.kill('SIGKILL'), 5_000);
	await exited;
	clearTimeout(timeout);
}
