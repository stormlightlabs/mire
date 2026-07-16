export type Session = {
	id: string;
	repository_name: string;
	title: string;
	created_at: string;
	current_round_id?: string;
};

export type Round = {
	id: string;
	number: number;
	status: string;
	snapshot_id?: string;
	created_at: string;
	updated_at?: string;
};

export type Activity = { activity_id: number; event_kind: string; message: string; created_at: string };
export type Capabilities = { review_data: boolean; actions: boolean; sse: boolean };

export type Bootstrap = {
	schema_version: string;
	authenticated: boolean;
	sessions: Session[];
	selected_session: Session | null;
	current_round: Round | null;
	capabilities: Capabilities;
};

export type Anchor = {
	snapshot_id: string;
	path: string;
	hunk_id: string;
	side?: string;
	start_line?: number;
	end_line?: number;
};

export type DiffHunk = {
	id: string;
	kind: string;
	old_start: number;
	old_lines: number;
	new_start: number;
	new_lines: number;
	lines?: string[];
	binary?: boolean;
	available: boolean;
	digest: string;
};

export type DiffFile = {
	status: string;
	base_path?: string;
	target_path?: string;
	hunks?: DiffHunk[];
	surfaces?: string[];
	patch?: string;
};

export type LogicalSlice = {
	id: string;
	title: string;
	file_paths: string[];
	hunk_ids: string[];
	grouping: string;
	risk_cues?: string[];
};

export type Evidence = {
	id: string;
	relation: 'supports' | 'contradicts' | 'contextualizes';
	summary: string;
	kind?: string;
	anchors?: Anchor[];
	artifact_digest?: string;
	producing_run_id?: string;
	independent?: boolean;
	concrete?: boolean;
	material?: boolean;
};

export type FindingRevision = {
	finding_id: string;
	revision: number;
	claim: string;
	invariant?: string;
	impact: string;
	category: string;
	severity: string;
	confidence: number;
	verification: string;
	verification_run_id?: string;
	anchors: Anchor[];
	evidence?: Evidence[];
	relationships?: { kind: string; finding_id: string; revision: number; reason?: string }[];
	origin: { candidate_id?: string; review_run_id?: string; pass_name?: string; source?: string };
};

export type Verification = {
	id: string;
	state: string;
	suspected_invariant?: string;
	refutation_attempt?: string;
	concrete_path?: { summary: string; anchors?: Anchor[] }[];
	guard_evidence?: Evidence[];
	test_evidence?: Evidence[];
	evidence?: Evidence[];
	diagnostics?: { code: string; message: string }[];
};

export type FindingProjection = {
	finding: FindingRevision;
	lane: FindingLane;
	candidate_id?: string;
	verification?: Verification;
};

export type CandidateProjection = {
	candidate: {
		id: string;
		pass_name: string;
		candidate: {
			claim: string;
			impact: string;
			category: string;
			severity: string;
			confidence: number;
			anchors: Anchor[];
			rationale?: string;
		};
	};
	lane: FindingLane;
	reason?: string;
	verification?: Verification;
};

export type FindingLane = 'verified' | 'candidate' | 'refuted';
export type FindingResource = { lane: FindingLane; findings: FindingProjection[]; candidates: CandidateProjection[] };

export type FindingDetailResource = {
	finding: FindingProjection;
	artifacts: {
		id: string;
		kind: string;
		path?: string;
		digest: string;
		excluded?: boolean;
		exclusion_reason?: string;
		truncated?: boolean;
	}[];
	coverage: ReviewCoverage;
	provenance: OverviewResource['provenance'];
};

export type ReviewCoverage = {
	examined_files: string[];
	examined_hunks: string[];
	retrieved_artifacts?: { id: string; path?: string; kind: string; digest: string; excluded?: boolean }[];
	passes: { name: string; status: string; applicable: boolean; candidate_count: number; reason: string }[];
	analyzers?: { name: string; available: boolean; version?: string; reason?: string }[];
	exclusions?: { pass_name: string; path?: string; kind: string; reason: string }[];
	failures?: { pass_name: string; code: string; message: string }[];
	gaps?: string[];
};

export type Omission = { kind: string; reason: string };
export type Run = {
	id: string;
	role: string;
	pass_name?: string;
	status: string;
	provenance: { adapter: string; protocol: string; model?: string; termination_cause?: string };
};

export type OverviewResource = {
	session: Session;
	round: Round;
	snapshot: { id: string; kind: string; requested_comparison: string; manifest_digest: string; created_at: string };
	intent: { prompt?: string; commit_messages?: { message: string; oid: string }[] };
	change: { files: Omit<DiffFile, 'hunks' | 'patch'>[]; surfaces: { kind: string }[] };
	coverage: ReviewCoverage;
	omissions: Omission[];
	provenance: { planner_runs: Run[]; review_runs: Run[]; verification_runs: Run[] };
};

export type SlicesResource = {
	slices: LogicalSlice[];
	risk_areas: { id: string; title: string; reason: string; file_paths?: string[] }[];
	ordering_rationale: string;
	files: Omit<DiffFile, 'hunks' | 'patch'>[];
};

export type Divergence = { status: string; message: string; affected_paths?: string[]; affected_refs?: string[] };

export type ReviewWorkspace = {
	overview: OverviewResource;
	diff: { snapshot_id: string; files: DiffFile[]; omissions: Omission[] };
	slices: SlicesResource;
	findings: Record<FindingLane, FindingResource>;
	coverage: { coverage: ReviewCoverage; omissions: Omission[] };
	divergence: Divergence;
};

export type LoadState = 'loading' | 'ready' | 'unauthenticated' | 'offline' | 'error';
export type ResourceState = 'idle' | 'loading' | 'ready' | 'error';
