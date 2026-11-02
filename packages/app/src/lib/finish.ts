import { fileOrder } from './focus';
import type { HeldChanges } from './live';
import { fileViewState, type FileSummary, type FindingSummary, type ReviewOverview } from './review';

/** Severities from most to least severe, the order the panel counts them in. */
export const SEVERITY_ORDER = ['critical', 'high', 'medium', 'low', 'note'];

/** The downloads the panel offers. Each one reads the review file as it is on disk. */
export const EXPORTS: ExportChoice[] = [
	{
		label: 'Notes as Markdown',
		route: '/api/v1/exports/notes.md',
		filename: 'mire-notes.md',
		purpose: 'for a PR comment'
	},
	{ label: 'Notes as JSON', route: '/api/v1/exports/notes.json', filename: 'mire-notes.json', purpose: 'for tools' },
	{
		label: 'Agent context',
		route: '/api/v1/exports/context.json',
		filename: 'mire-context.json',
		purpose: 'for the next agent'
	},
	{
		label: 'Patch',
		route: '/api/v1/exports/changeset.patch',
		filename: 'mire-changeset.patch',
		purpose: 'Git-compatible'
	}
];

/** One download in the panel: the server route, the file name it saves as, and what it is for. */
export type ExportChoice = { label: string; route: string; filename: string; purpose: string };

/**
 * One thing left before handoff, with a title, an optional detail line, and
 * where to act on it: a revision to apply, a file to open, or a finding to
 * open.
 */
export type HandoffItem =
	| { kind: 'held'; title: string; detail: string; revision: number }
	| { kind: 'unviewed'; title: string; detail: string | null; target: FileSummary }
	| { kind: 'findings'; title: string; detail: string; target: FindingSummary }
	| { kind: 'anchors'; title: string; detail: string; target: FindingSummary };

/**
 * Lists what is left before handoff, in the order the panel shows it: a
 * held revision, files not viewed as they are now, open findings, and
 * findings whose anchor is stale or ambiguous. An empty list means nothing is
 * left. Generated files are left out of the viewed count when the review has
 * other files, and the detail line says how many; `uncountedGenerated`
 * counts them when no unviewed row shows.
 */
export function handoffItems(
	review: ReviewOverview,
	held: HeldChanges | null,
	shown: Pick<FileSummary, 'path'> | null
): HandoffItem[] {
	const items: HandoffItem[] = [];
	if (held) items.push(heldItem(review, held));
	const unviewed = unviewedItem(review.files, shown);
	if (unviewed) items.push(unviewed);
	const findings = openFindingsItem(review.findings);
	if (findings) items.push(findings);
	const anchors = anchorsItem(review.findings);
	if (anchors) items.push(anchors);
	return items;
}

/**
 * Counts the generated files not viewed as they are now that no row names:
 * the review has other files and every one of them is viewed. The panel lists
 * these apart, since they do not count toward what is left. Returns 0 when an
 * unviewed row would show, as its detail line already counts them.
 */
export function uncountedGenerated(files: FileSummary[]): number {
	const source = files.filter((file) => !file.generated);
	if (!source.length || source.some((file) => fileViewState(file) !== 'viewed')) return 0;
	return files.filter((file) => file.generated && fileViewState(file) !== 'viewed').length;
}

/** The panel's opening sentence, such as "Three things are left." */
export function handoffHeadline(count: number): string {
	if (count === 0) return 'Nothing is left.';
	const words = ['One thing is', 'Two things are', 'Three things are', 'Four things are'];
	return `${words[count - 1] ?? `${count} things are`} left.`;
}

/**
 * Returns the file to open for "Next unviewed": the first one after the shown
 * file, in the file tree's order, that is not viewed as it is now. It wraps
 * to the start and returns `null` when every file in `files` is viewed.
 */
export function nextUnviewed(files: FileSummary[], shown: Pick<FileSummary, 'path'> | null): FileSummary | null {
	const ordered = fileOrder(files);
	const start = shown ? ordered.findIndex((file) => file.path.raw === shown.path.raw) + 1 : 0;
	for (let step = 0; step < ordered.length; step++) {
		const file = ordered[(start + step) % ordered.length];
		if (fileViewState(file) !== 'viewed') return file;
	}
	return null;
}

function heldItem(review: ReviewOverview, held: HeldChanges): HandoffItem {
	const title = held.replaced
		? `Revision ${held.revision} replaced the review`
		: `Revision ${held.revision} is not applied`;
	return {
		kind: 'held',
		title,
		detail: `${heldSummaryLine(held)} The counts below are for revision ${review.revision}.`,
		revision: held.revision
	};
}

function heldSummaryLine(held: HeldChanges): string {
	const parts = [];
	if (held.files.length) parts.push(count(held.files.length, 'file'));
	if (held.findings) parts.push(count(held.findings, 'finding'));
	return parts.length ? `${parts.join(' and ')} changed.` : 'No diff or finding changed.';
}

function unviewedItem(files: FileSummary[], shown: Pick<FileSummary, 'path'> | null): HandoffItem | null {
	const source = files.filter((file) => !file.generated);
	const counted = source.length ? source : files;
	const target = nextUnviewed(counted, shown);
	if (!target) return null;
	const states = counted.map(fileViewState);
	const notViewed = states.filter((state) => state !== 'viewed').length;
	const changed = states.filter((state) => state === 'changed').length;
	const excluded = files.length - counted.length;
	const detail = [];
	if (changed) detail.push(`${changed} changed since you viewed ${changed === 1 ? 'it' : 'them'}.`);
	if (excluded) detail.push(`${count(excluded, 'generated file')} ${excluded === 1 ? 'is' : 'are'} not counted.`);
	return {
		kind: 'unviewed',
		title: `${notViewed} of ${count(counted.length, 'file')} not viewed`,
		detail: detail.length ? detail.join(' ') : null,
		target
	};
}

function openFindingsItem(findings: FindingSummary[]): HandoffItem | null {
	const open = findings.filter((finding) => finding.status === 'open');
	if (!open.length) return null;
	const ranked = [...open].sort((left, right) => rank(left.severity) - rank(right.severity));
	const bySeverity = SEVERITY_ORDER.map(
		(severity) => [severity, open.filter((finding) => finding.severity === severity).length] as const
	)
		.filter(([, total]) => total > 0)
		.map(([severity, total]) => `${total} ${severity}`);
	const other = open.filter((finding) => !SEVERITY_ORDER.includes(finding.severity)).length;
	if (other) bySeverity.push(`${other} other`);
	return {
		kind: 'findings',
		title: count(open.length, 'open finding'),
		detail: bySeverity.join(' · '),
		target: ranked[0]
	};
}

function anchorsItem(findings: FindingSummary[]): HandoffItem | null {
	const unsafe = findings.filter((finding) => finding.anchorState === 'stale' || finding.anchorState === 'ambiguous');
	if (!unsafe.length) return null;
	const stale = unsafe.filter((finding) => finding.anchorState === 'stale').length;
	const ambiguous = unsafe.length - stale;
	const detail = [];
	if (stale) detail.push(`${stale} stale`);
	if (ambiguous) detail.push(`${ambiguous} ambiguous`);
	return {
		kind: 'anchors',
		title: `${count(unsafe.length, 'finding')} lost ${unsafe.length === 1 ? 'its' : 'their'} line`,
		detail: detail.join(' · '),
		target: unsafe[0]
	};
}

/** Orders a severity for sorting; unknown severities sort last. */
function rank(severity: string): number {
	const index = SEVERITY_ORDER.indexOf(severity);
	return index < 0 ? SEVERITY_ORDER.length : index;
}

function count(total: number, noun: string): string {
	return `${total} ${noun}${total === 1 ? '' : 's'}`;
}
