import type { FileSummary } from './review';

/** The review's files as the file tree groups them: files listed by path, then the generated group. */
export type TreeGroups = { source: FileSummary[]; generated: FileSummary[] };

/** Orders two repository paths, or directory paths, as the file tree lists them. */
export function comparePaths(left: string, right: string): number {
	return left.localeCompare(right);
}

/**
 * Splits files into the two groups the file tree shows. Files not marked
 * generated are sorted by path; generated files keep the review's order.
 */
export function treeGroups(files: FileSummary[]): TreeGroups {
	return {
		source: files
			.filter((file) => !file.generated)
			.sort((left, right) => comparePaths(left.path.display, right.path.display)),
		generated: files.filter((file) => file.generated)
	};
}
