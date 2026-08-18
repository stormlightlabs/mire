export type DisplayText = { display: string; lossy: boolean };

/** Formats a potentially lossy filesystem path for display. */
export function formatPath(path: DisplayText): string {
	return `${path.display}${path.lossy ? ' �' : ''}`;
}

export type FileSummary = {
	id: string;
	path: DisplayText;
	status: string;
	contentKind: 'text' | 'binary';
	openFindings: number;
};

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
	path: DisplayText;
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
	path: DisplayText;
	oldPath: DisplayText | null;
	status: string;
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

/** Combines durable review readiness with browser-local file progress. */
export function completionSummary(review: ReviewOverview, viewedFileIds: string[]): CompletionSummary {
	const viewed = new Set(viewedFileIds);
	const unviewedFiles = review.files.filter((file) => !viewed.has(file.id)).length;
	return {
		ready: review.readiness.ready && unviewedFiles === 0,
		unviewedFiles,
		openFindings: review.readiness.openFindings,
		unsafeAnchors: review.readiness.unsafeAnchors
	};
}
