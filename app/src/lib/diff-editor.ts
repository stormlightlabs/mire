import type { DiffHunk } from './types';

export type HunkDocuments = { before: string; after: string };

/**
 * Reconstructs the two hunk documents without inventing lines outside the captured patch.
 * */
export function hunkDocuments(hunk: Pick<DiffHunk, 'lines'>): HunkDocuments {
	const before: string[] = [];
	const after: string[] = [];

	for (const line of hunk.lines ?? []) {
		const marker = line.at(0);
		const content = marker === '+' || marker === '-' || marker === ' ' ? line.slice(1) : line;

		if (marker !== '+') before.push(content);
		if (marker !== '-') after.push(content);
	}

	return { before: before.join(''), after: after.join('') };
}
