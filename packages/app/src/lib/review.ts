export type DisplayText = { display: string; lossy: boolean };

/** A repository path, with its raw bytes as lowercase hex for routes that take a `path` parameter. */
export type PathText = DisplayText & { raw: string };

/** Formats a potentially lossy filesystem path for display. */
export function formatPath(path: DisplayText): string {
	return `${path.display}${path.lossy ? ' �' : ''}`;
}

/** The fingerprint a reader marked viewed, and the review revision they read it at. */
export type ViewedState = { fingerprint: string; revision: number };

/** Whether the reader viewed a file as it is now, viewed it before it changed, or has not viewed it. */
export type FileViewState = 'viewed' | 'changed' | 'unviewed';

export type FileSummary = {
	id: string;
	path: PathText;
	status: string;
	contentKind: 'text' | 'binary';
	/** Whether the repository's attributes mark the file as generated. Generated files start collapsed. */
	generated: boolean;
	openFindings: number;
	/** The reader's viewed mark for this path, or `null`. */
	viewed: ViewedState | null;
};

export type ViewedResponse = {
	revision: number;
	path: PathText;
	viewed: ViewedState | null;
};

/** Reports whether the reader's mark holds the file's current fingerprint, its `id`. */
export function fileViewState(file: Pick<FileSummary, 'id' | 'viewed'>): FileViewState {
	if (!file.viewed) return 'unviewed';
	return file.viewed.fingerprint === file.id ? 'viewed' : 'changed';
}

/** Returns the file to open first: the first one not generated, or the first file when all are. */
export function firstFile(files: FileSummary[]): FileSummary | null {
	return files.find((file) => !file.generated) ?? files[0] ?? null;
}

/** The file to open after the review reloads, and a notice for the reader when it is not the one they had open. */
export type ReopenedFile = { file: FileSummary | null; notice: string | null };

/**
 * Finds the file to keep open after the review reloads. A file's `id` is its
 * fingerprint, so a source change gives it a new one; the file is then found by
 * its raw path. When the open file left the review, or was renamed, this falls
 * back to the first file and returns a notice naming both paths, so the switch
 * is not silent.
 */
export function reopenedFile(files: FileSummary[], previous: Pick<FileSummary, 'id' | 'path'> | null): ReopenedFile {
	const first = firstFile(files);
	if (!previous) return { file: first, notice: null };
	const same =
		files.find((file) => file.id === previous.id) ?? files.find((file) => file.path.raw === previous.path.raw);
	if (same) return { file: same, notice: null };
	const gone = `${formatPath(previous.path)} is no longer in the review`;
	return { file: first, notice: first ? `${gone}; showing ${formatPath(first.path)}.` : `${gone}.` };
}

/**
 * Reports whether loading `next` reloads the file already on screen. The page
 * keeps a shown file and its controls mounted while it reloads, and replaces
 * the content when the new data arrives, so a click during the reload still
 * lands.
 */
export function isShownFile(shown: Pick<FileDetail, 'path'> | null, next: Pick<FileSummary, 'path'>): boolean {
	return shown !== null && shown.path.raw === next.path.raw;
}

/**
 * Reports the reader's viewed state for the file content on screen. It compares
 * the mark on the file's path with the shown content's fingerprint, not with the
 * newest one, so the state matches what the reader sees while a reload is in
 * flight.
 */
export function shownViewState(files: FileSummary[], shown: Pick<FileDetail, 'id' | 'path'>): FileViewState {
	const summary = files.find((file) => file.path.raw === shown.path.raw);
	return summary ? fileViewState({ id: shown.id, viewed: summary.viewed }) : 'unviewed';
}

/**
 * Reports whether a file was added or deleted whole. Such a diff reads as plain
 * code, with the gutter bar marking every line, rather than as tinted rows.
 */
export function isWholeFileChange(status: string): boolean {
	return status === 'added' || status === 'deleted';
}

/** Returns the viewed route for a path. */
export function viewedRoute(path: PathText): string {
	return `/api/v1/viewed?path=${path.raw}`;
}

export type SemanticLine = {
	kind: 'context' | 'addition' | 'deletion';
	oldLine: number | null;
	newLine: number | null;
	content: DisplayText;
	missingNewline: 'none' | 'old' | 'new' | 'both';
};

export type SemanticHunk = {
	oldStart: number;
	oldLineCount: number;
	newStart: number;
	newLineCount: number;
	section: DisplayText;
	lines: SemanticLine[];
};

export type SemanticFileContent = { kind: 'text'; hunks: SemanticHunk[] } | { kind: 'binary' };

export type FindingSummary = {
	id: string;
	path: PathText;
	side: 'old' | 'new';
	startLine: number;
	endLine: number;
	anchorState: 'captured' | 'exact' | 'moved' | 'stale' | 'ambiguous';
	navigable: boolean;
	severity: string;
	annotationKind: string;
	status: string;
	body: string;
};

export type FileDetail = {
	id: string;
	path: PathText;
	oldPath: PathText | null;
	status: string;
	/** Whether the repository's attributes mark the file as generated. */
	generated: boolean;
	content: SemanticFileContent;
	findings: FindingSummary[];
};

export type FindingDetail = FindingSummary & {
	body: string;
	author: { id: string; displayName: string | null };
	provenance: string;
};

export type FindingDraft = Pick<FindingDetail, 'body' | 'severity' | 'annotationKind'>;

export type FindingFilters = {
	status: string;
	severity: string;
	annotationKind: string;
};

export const defaultFindingFilters: FindingFilters = {
	status: 'all',
	severity: 'all',
	annotationKind: 'all'
};

/** Returns findings that match the active queue filters. */
export function filterFindings(findings: FindingSummary[], filters: FindingFilters): FindingSummary[] {
	return findings.filter(
		(finding) =>
			(filters.status === 'all' || finding.status === filters.status) &&
			(filters.severity === 'all' || finding.severity === filters.severity) &&
			(filters.annotationKind === 'all' || finding.annotationKind === filters.annotationKind)
	);
}

export type FindingMutation = {
	revision: number;
	finding: FindingDetail;
};

export type Problem = {
	code: string;
	detail: string;
	actualRevision?: number;
};

export type WatchStatus = 'watching' | 'unavailable' | 'degraded';

export type ReanchorTotals = {
	captured: number;
	exact: number;
	moved: number;
	stale: number;
	ambiguous: number;
};

export type ReviewOverview = {
	reviewIdentity: string;
	revision: number;
	source: string;
	/** Caller-supplied one-line title, such as a pull request title. */
	title: string | null;
	/** Caller-supplied plain-text description. Render it as text, never as HTML. */
	description: string | null;
	files: FileSummary[];
	findings: FindingSummary[];
	totals: {
		files: number;
		findings: number;
		open: number;
		resolved: number;
		dismissed: number;
		acceptedRisk: number;
	};
	changes: { additions: number; deletions: number };
	reanchor: ReanchorTotals;
	readiness: { ready: boolean; openFindings: number; unsafeAnchors: number };
	watch: WatchStatus;
};

export type RefreshResponse = {
	revision: number;
	status: 'refreshed' | 'unchanged';
	reanchor: ReanchorTotals;
};

export type CompletionSummary = {
	ready: boolean;
	unviewedFiles: number;
	openFindings: number;
	unsafeAnchors: number;
};

/** Fallback heading for a review without a caller-supplied title. */
export const DEFAULT_REVIEW_TITLE = 'Local review';

/** Returns the heading for a review: its title, or the default when the title is missing or blank. */
export function reviewTitle(review: Pick<ReviewOverview, 'title'> | null): string {
	const title = review?.title?.trim();
	return title ? title : DEFAULT_REVIEW_TITLE;
}

/** Returns the review description, or `null` when it is missing or only whitespace. */
export function reviewDescription(review: Pick<ReviewOverview, 'description'> | null): string | null {
	const description = review?.description;
	return description && description.trim() ? description : null;
}

/** Reports whether a review has a caller-supplied title or a description to show. */
export function hasReviewMetadata(review: Pick<ReviewOverview, 'title' | 'description'> | null): boolean {
	return Boolean(review?.title?.trim()) || reviewDescription(review) !== null;
}

/** Combines review readiness with the files the reader has not viewed as they are now. */
export function completionSummary(review: ReviewOverview): CompletionSummary {
	const unviewedFiles = review.files.filter((file) => fileViewState(file) !== 'viewed').length;
	return {
		ready: review.readiness.ready && unviewedFiles === 0,
		unviewedFiles,
		openFindings: review.readiness.openFindings,
		unsafeAnchors: review.readiness.unsafeAnchors
	};
}
