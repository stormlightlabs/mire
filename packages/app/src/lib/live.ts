import type { FileSummary, FindingSummary, PathText, ReviewOverview, WatchStatus } from './review';

/** How a file differs between the revision on screen and a newer one. */
export type FileChange = 'changed' | 'added' | 'removed';

/** One file that differs in a held revision. */
export type RevisionFileChange = { path: PathText; change: FileChange };

/** How long the page waits before its first attempt to reconnect the live channel, in milliseconds. */
export const FIRST_RETRY_DELAY = 1_000;

/** The longest wait between attempts to reconnect the live channel, in milliseconds. */
export const MAX_RETRY_DELAY = 30_000;

/** What a held revision changes, compared with the revision on screen. */
export type HeldChanges = {
	/** The held revision's number. */
	revision: number;
	/** Files whose diff differs, in the held revision's order, then the removed ones. */
	files: RevisionFileChange[];
	/** Findings added, removed, or with a new status, body or anchor state. */
	findings: number;
};

/**
 * The state of the live channel to `mire serve`: connected, cut off and
 * retrying, closed because the server shut down, or refused because the
 * server no longer accepts the page's session secret.
 */
export type Connection = 'live' | 'lost' | 'stopped' | 'expired';

/**
 * What the reader saw of a finding when they started a write: its text for an
 * edit, or its status for a decision.
 */
export type FindingWriteBase = { findingId: string; body: string } | { findingId: string; status: string };

/** What the reader can do about a stopped watch. */
export type WatchAction = 'reconnect' | 'refresh' | null;

/** A notice that live updates no longer reach the page, and how to resume them. */
export type WatchNotice = { heading: string; body: string; action: WatchAction };

/**
 * Returns the connection state for a live channel response the server
 * refused: `expired` for 401 and 403, which no retry can fix, or `lost`.
 */
export function refusedConnection(status: number): Connection {
	return status === 401 || status === 403 ? 'expired' : 'lost';
}

/** Whether the page should try the live channel again on its own. A shut-down server or an expired session will not come back. */
export function retries(connection: Connection): boolean {
	return connection === 'live' || connection === 'lost';
}

/** Returns the wait before the next reconnect attempt: double the last one, up to {@link MAX_RETRY_DELAY}. */
export function nextRetryDelay(delay: number): number {
	return Math.min(delay * 2, MAX_RETRY_DELAY);
}

/**
 * Checks whether one of the reader's finding writes still makes sense after a
 * newer revision applies. It returns `null` when the finding is still there
 * and the text or status the reader started from is unchanged, or else a
 * sentence naming what the newer revision changed.
 */
export function staleFindingWrite(next: ReviewOverview, base: FindingWriteBase): string | null {
	const finding = next.findings.find((candidate) => candidate.id === base.findingId);
	if (!finding) return `Revision ${next.revision} removed this finding.`;
	if ('body' in base && finding.body !== base.body) return `Revision ${next.revision} changed this finding's text.`;
	if ('status' in base && finding.status !== base.status) {
		return `Revision ${next.revision} changed this finding's status to ${finding.status}.`;
	}
	return null;
}

/**
 * Lists the files and counts the findings that differ between the revision on
 * screen and a newer one. A file is matched by raw path; a new `id`, its
 * fingerprint, means its diff changed.
 */
export function heldChanges(shown: ReviewOverview, next: ReviewOverview): HeldChanges {
	const before = new Map(shown.files.map((file) => [file.path.raw, file]));
	const after = new Set(next.files.map((file) => file.path.raw));
	const files: RevisionFileChange[] = [];
	for (const file of next.files) {
		const previous = before.get(file.path.raw);
		if (!previous) files.push({ path: file.path, change: 'added' });
		else if (previous.id !== file.id) files.push({ path: file.path, change: 'changed' });
	}
	for (const file of shown.files) {
		if (!after.has(file.path.raw)) files.push({ path: file.path, change: 'removed' });
	}
	return { revision: next.revision, files, findings: findingChanges(shown.findings, next.findings) };
}

/** Returns the held change to the file on screen, or `null` when the held revision leaves it as it is. */
export function heldFileChange(
	changes: HeldChanges | null,
	shown: Pick<FileSummary, 'path'> | null
): FileChange | null {
	if (!changes || !shown) return null;
	return changes.files.find((file) => file.path.raw === shown.path.raw)?.change ?? null;
}

/** Describes a held revision in one sentence, such as "2 files and 1 finding changed while you read." */
export function heldSummary(changes: HeldChanges): string {
	const parts = [];
	if (changes.files.length) parts.push(count(changes.files.length, 'file'));
	if (changes.findings) parts.push(count(changes.findings, 'finding'));
	if (!parts.length) return 'No diff or finding changed while you read.';
	return `${parts.join(' and ')} changed while you read.`;
}

/**
 * Copies the reader's viewed marks from a newer overview onto the one on
 * screen. A mark is the reader's own write and applies at once, while the
 * rest of the newer revision waits until the reader applies it. A file the
 * newer overview no longer lists keeps the mark it had.
 */
export function withViewedMarks(shown: ReviewOverview, next: ReviewOverview): ReviewOverview {
	const marks = new Map(next.files.map((file) => [file.path.raw, file.viewed]));
	return {
		...shown,
		watch: next.watch,
		files: shown.files.map((file) =>
			marks.has(file.path.raw) ? { ...file, viewed: marks.get(file.path.raw) ?? null } : file
		)
	};
}

/**
 * Explains why live updates stopped, or returns `null` while they arrive. A
 * lost connection retries on its own and can retry now; a shut-down server
 * needs a new `mire serve`; an expired session needs the URL `mire serve`
 * printed; a degraded watch can read the source now.
 */
export function watchNotice(connection: Connection, watch: WatchStatus, revision: number): WatchNotice | null {
	const keep = `You can keep reading revision ${revision}`;
	if (connection === 'expired') {
		return {
			heading: 'Session expired',
			body: `mire serve no longer accepts this page's session. ${keep}. Open the URL mire serve printed to resume.`,
			action: null
		};
	}
	if (connection === 'stopped') {
		return {
			heading: 'Watching stopped',
			body: `mire serve has shut down. ${keep}. Run mire serve again and open the URL it prints to resume.`,
			action: null
		};
	}
	if (connection === 'lost') {
		return {
			heading: 'Watching stopped',
			body: `The connection to mire serve was lost. ${keep} while Mire retries.`,
			action: 'reconnect'
		};
	}
	if (watch === 'degraded') {
		return {
			heading: 'Watching stopped',
			body: `The watch is degraded: the last reload of the review file or the source failed. ${keep} while Mire retries.`,
			action: 'refresh'
		};
	}
	return null;
}

/** The header's watch status: the watch state, and the revision on screen once updates stop. */
export function watchLabel(connection: Connection, watch: WatchStatus, revision: number): string {
	const state = connection === 'lost' ? 'disconnected' : connection === 'live' ? watch : connection;
	return state === 'watching' || state === 'unavailable' ? state : `${state} · showing revision ${revision}`;
}

function findingChanges(shown: FindingSummary[], next: FindingSummary[]): number {
	const before = new Map(shown.map((finding) => [finding.id, finding]));
	let changed = 0;
	for (const finding of next) {
		const previous = before.get(finding.id);
		before.delete(finding.id);
		if (
			!previous ||
			previous.status !== finding.status ||
			previous.body !== finding.body ||
			previous.anchorState !== finding.anchorState
		) {
			changed++;
		}
	}
	return changed + before.size;
}

function count(value: number, noun: string): string {
	return `${value} ${noun}${value === 1 ? '' : 's'}`;
}
