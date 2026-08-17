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
};
