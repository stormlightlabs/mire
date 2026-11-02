<script lang="ts">
	import { onMount, tick } from 'svelte';
	import DiffPane from '$lib/DiffPane.svelte';
	import FileNavigator from '$lib/FileNavigator.svelte';
	import FindingQueue from '$lib/FindingQueue.svelte';
	import FocusBar from '$lib/FocusBar.svelte';
	import KeyBar from '$lib/KeyBar.svelte';
	import Overlay from '$lib/Overlay.svelte';
	import { filePosition, takesText } from '$lib/focus';
	import {
		FIRST_RETRY_DELAY,
		heldChanges,
		heldFileChange,
		heldHeading,
		heldSummary,
		nextHeld,
		nextRetryDelay,
		refusedConnection,
		retries,
		staleFindingWrite,
		watchLabel,
		watchNotice,
		withViewedMarks,
		type Connection,
		type FindingWriteBase
	} from '$lib/live';
	import { OverviewLoads } from '$lib/reload';
	import { loadDevelopmentSessionSecret, parseSessionSecret, saveDevelopmentSessionSecret } from '$lib/session';
	import { applyTheme, loadTheme, saveTheme, type Theme } from '$lib/theme';
	import {
		completionSummary,
		defaultFindingFilters,
		formatPath,
		filterFindings,
		firstFile,
		hasReviewMetadata,
		isShownFile,
		reopenedFile,
		RequestError,
		reviewDescription,
		reviewTitle,
		shownViewState,
		viewedRoute
	} from '$lib/review';
	import type {
		FileDetail,
		FindingDetail,
		FindingDraft,
		FindingMutation,
		FindingSummary,
		Problem,
		RefreshResponse,
		ReviewOverview,
		ViewedResponse,
		WatchStatus
	} from '$lib/review';

	type RefreshState = 'idle' | 'pending' | 'refreshed' | 'unchanged' | 'failed';
	/**
	 * A finding edit the server refused because the review changed. It fills
	 * the editor until the reader changes the text, decides, or opens another
	 * finding.
	 */
	type KeptDraft = FindingDraft & { findingId: string };

	let review = $state<ReviewOverview | null>(null);
	let activeFile = $state<string | null>(null);
	let file = $state<FileDetail | null>(null);
	let fileLoading = $state(false);
	let notice = $state<string | null>(null);
	let activeFinding = $state<FindingDetail | null>(null);
	let error = $state<string | null>(null);
	let fileError = $state<string | null>(null);
	let findingError = $state<string | null>(null);
	let filters = $state({ ...defaultFindingFilters });
	let viewedError = $state<string | null>(null);
	let conflictRevision = $state<number | null>(null);
	/**
	 * Why a write was not saved after the page applied a held revision for it,
	 * or `null` when the server refused the write and the newer revision still
	 * waits to be applied.
	 */
	let conflictReason = $state<string | null>(null);
	let refreshState = $state<RefreshState>('idle');
	let finishReviewOpen = $state(false);
	let descriptionOpen = $state(false);
	let sessionSecretInput = $state('');
	let theme = $state<Theme>('light');
	let themeReady = $state(false);
	let isNarrow = $state(false);
	let mobilePane = $state<'files' | 'findings' | null>(null);
	/** The raw path of the generated file whose diff is open, if any. */
	let openedGenerated = $state<string | null>(null);
	/** Whether focus mode shows the open file alone. It lasts for this tab and is never saved. */
	let focused = $state(false);
	/** The topbar's Focus button, which takes keyboard focus back when focus mode ends. */
	let focusButton = $state<HTMLButtonElement>();
	/** The focus bar, which takes keyboard focus when focus mode starts. */
	let focusBar = $state<FocusBar>();
	/**
	 * A revision other than the one on screen, held until the reader applies
	 * it. It is usually newer; a lower one means the review file was replaced.
	 * Source changes and other writers' edits wait here. A viewed mark applies
	 * at once; a finding edit or decision applies the held revision first.
	 */
	let held = $state<ReviewOverview | null>(null);
	/** Whether the held revision's file list is open. */
	let heldListOpen = $state(false);
	/** Whether live updates reach the page. */
	let connection = $state<Connection>('live');
	/** The reader's finding edit from a save that hit a revision conflict, until the editor takes it over. */
	let keptDraft = $state<KeptDraft | null>(null);
	let draftCopied = $state(false);
	/**
	 * The open finding's text when the reader started editing it. A save checks
	 * the finding against it, so an edit typed from older text never replaces
	 * another writer's. It clears when the edit saves or another finding opens.
	 */
	let editBase: { findingId: string; body: string } | null = null;

	let streamAbort: AbortController | null = null;
	let secret = '';
	/** The review revision the shown file was requested at; a viewed mark records it. */
	let fileRevision = 0;
	/** Counts file loads, so only the newest one replaces the shown file. */
	let fileLoads = 0;
	/**
	 * Whether a viewed mark is being saved. Clicks wait for the reload that
	 * follows it, so a second click acts on the saved state, not a stale one.
	 */
	let viewedPending = false;
	/** Orders overview loads, so a slow response cannot overwrite a newer one. */
	const overviewLoads = new OverviewLoads();
	/** Counts the reader's own writes in flight. Live updates that arrive meanwhile wait for them. */
	let writesInFlight = 0;
	/** Whether a live update arrived during one of the reader's writes and still needs a check. */
	let checkAfterWrites = false;
	let retryTimer: ReturnType<typeof setTimeout> | undefined;
	/** How long to wait before the next attempt to reconnect the live channel. */
	let retryDelay = FIRST_RETRY_DELAY;

	const completion = $derived(review ? completionSummary(review) : null);
	const activeSummary = $derived(review?.files.find((summary) => summary.id === activeFile) ?? null);
	const shownView = $derived(review && file ? shownViewState(review.files, file) : 'unviewed');
	const visibleFindings = $derived(review ? filterFindings(review.findings, filters) : []);
	const title = $derived(reviewTitle(review));
	const description = $derived(reviewDescription(review));
	const hasMetadata = $derived(hasReviewMetadata(review));
	const position = $derived(review ? filePosition(review.files, file ?? activeSummary) : null);
	/** Focus mode needs a loaded review; until then the normal header explains what is happening. */
	const inFocus = $derived(focused && review !== null && error === null);
	const themeLabel = $derived(theme === 'dark' ? 'Use light mode' : 'Use dark mode');
	const changes = $derived(review && held ? heldChanges(review, held) : null);
	const shownChange = $derived(heldFileChange(changes, file ?? activeSummary));
	const stopped = $derived(review ? watchNotice(connection, review.watch, review.revision) : null);
	const shownDraft = $derived(keptDraft && keptDraft.findingId === activeFinding?.id ? keptDraft : null);

	$effect(() => {
		if (!themeReady) return;
		applyTheme(theme);
		saveTheme(theme);
	});

	onMount(() => {
		const narrowScreen = window.matchMedia('(max-width: 54rem)');
		const updateNarrowScreen = () => {
			isNarrow = narrowScreen.matches;
			if (!isNarrow) mobilePane = null;
		};
		theme = loadTheme();
		themeReady = true;
		updateNarrowScreen();
		narrowScreen.addEventListener('change', updateNarrowScreen);
		secret = parseSessionSecret(window.location.hash);
		if (secret) {
			window.history.replaceState(null, '', `${window.location.pathname}${window.location.search}`);
			if (import.meta.env.DEV) saveDevelopmentSessionSecret(secret);
		} else if (import.meta.env.DEV) {
			secret = loadDevelopmentSessionSecret();
		}
		if (!secret) {
			error = import.meta.env.DEV
				? 'Paste the session URL printed by mire serve to connect.'
				: 'This review URL is missing its session secret. Run mire serve again.';
			return;
		}
		void loadReview().then((loaded) => {
			if (loaded) return connectEvents();
		});
		return () => {
			streamAbort?.abort();
			clearTimeout(retryTimer);
			narrowScreen.removeEventListener('change', updateNarrowScreen);
		};
	});

	function handleErr(err: unknown, fallback: string) {
		return err instanceof Error ? err.message : fallback;
	}

	async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
		const headers = new Headers(init.headers);
		headers.set('Authorization', `Bearer ${secret}`);
		const response = await fetch(path, { ...init, headers });
		if (!response.ok) {
			const problem = (await response.json().catch(() => null)) as Problem | null;
			if (response.status === 409 && problem?.code === 'revision_conflict') {
				conflictRevision = problem.actualRevision ?? null;
			}
			throw new RequestError(
				problem?.detail ?? `The requested review data is unavailable (${response.status}).`,
				problem?.code ?? null
			);
		}
		return (await response.json()) as T;
	}

	async function loadReview(): Promise<boolean> {
		const ticket = overviewLoads.start(false);
		try {
			const overview = await request<ReviewOverview>('/api/v1/review');
			if (!overviewLoads.settle(ticket).apply) return true;
			review = overview;
			const first = firstFile(review.files);
			if (first) await selectFile(first.id);
			else closeFile();
			error = null;
			return true;
		} catch (cause) {
			error = handleErr(cause, 'The review could not be loaded.');
			return false;
		}
	}

	async function connectDevelopmentSession(event: SubmitEvent) {
		event.preventDefault();
		const nextSecret = parseSessionSecret(sessionSecretInput);
		if (!nextSecret) {
			error = 'Enter the session secret or the complete URL printed by mire serve.';
			return;
		}
		streamAbort?.abort();
		secret = nextSecret;
		review = null;
		error = null;
		if (await loadReview()) {
			saveDevelopmentSessionSecret(secret);
			sessionSecretInput = '';
			void connectEvents();
		}
	}

	/**
	 * Reloads the overview. With `apply`, as after the reader's own finding
	 * change or when they apply a held revision, the response is shown and the
	 * open file loads again. Without it, as after a live update or a viewed
	 * mark, a newer revision is held for the reader to apply and only viewed
	 * marks change on screen. When a newer reload's response is shown first,
	 * that one applies instead, so the diff never lags the header.
	 */
	async function reloadReview(apply = true) {
		if (!review) return;
		const ticket = overviewLoads.start(apply);
		try {
			const overview = await request<ReviewOverview>('/api/v1/review');
			const settled = overviewLoads.settle(ticket);
			if (!settled.apply || !review) return;
			if (!settled.reopen) {
				holdOverview(review, overview);
				return;
			}
			const previous = activeSummary ?? file;
			review = overview;
			// The applied overview is the newest the page has, so nothing older waits.
			held = null;
			heldListOpen = false;
			conflictRevision = null;
			conflictReason = null;
			await syncActiveFinding(overview);
			const reopened = reopenedFile(review.files, previous);
			if (reopened.notice) notice = reopened.notice;
			if (reopened.file) await selectFile(reopened.file.id);
			else closeFile();
		} catch (cause) {
			// A background check that fails leaves the page as it is; the next update checks again.
			if (!apply) return;
			const message = handleErr(cause, 'The review could not be reloaded.');
			// The revision on screen stays readable when the held one cannot load.
			if (held) notice = `Revision ${held.revision} could not be loaded: ${message}`;
			else error = message;
		}
	}

	/**
	 * Brings the open finding up to date with a newly shown overview: it closes
	 * when the overview no longer lists it, and loads again when its summary
	 * changed. A failed load keeps the copy on screen.
	 */
	async function syncActiveFinding(overview: ReviewOverview) {
		const shown = activeFinding;
		if (!shown) return;
		const summary = overview.findings.find((finding) => finding.id === shown.id);
		if (!summary) {
			activeFinding = null;
			editBase = null;
			return;
		}
		const same =
			summary.body === shown.body &&
			summary.status === shown.status &&
			summary.severity === shown.severity &&
			summary.annotationKind === shown.annotationKind &&
			summary.anchorState === shown.anchorState &&
			summary.startLine === shown.startLine &&
			summary.endLine === shown.endLine;
		if (same) return;
		try {
			const detail = await request<FindingDetail>(`/api/v1/findings/${encodeURIComponent(shown.id)}`);
			if (activeFinding?.id === shown.id) activeFinding = detail;
		} catch {
			// The copy on screen stays; the editor's save reports any real problem.
		}
	}

	/**
	 * Holds an overview at any revision other than the one on screen, and shows
	 * its viewed marks and watch status now. It replaces a held revision the
	 * server has moved past or gone back from. At the revision on screen it
	 * shows the overview and drops anything held.
	 */
	function holdOverview(shown: ReviewOverview, overview: ReviewOverview) {
		held = nextHeld(shown, overview);
		if (held) {
			review = withViewedMarks(shown, overview);
		} else {
			review = overview;
			heldListOpen = false;
		}
	}

	/** Shows the held revision and loads the open file at it. */
	function applyHeld() {
		if (held) void reloadReview(true);
	}

	/**
	 * Runs one of the reader's own writes. A live update that arrives during it
	 * is checked once the write and its reload finish, so the reader's change is
	 * not held as if another writer had made it.
	 */
	async function ownWrite<T>(write: () => Promise<T>): Promise<T> {
		writesInFlight++;
		try {
			return await write();
		} finally {
			writesInFlight--;
			if (writesInFlight === 0 && checkAfterWrites) {
				checkAfterWrites = false;
				void reloadReview(false);
			}
		}
	}

	async function refreshReview() {
		if (!review || refreshState === 'pending') return;
		refreshState = 'pending';
		try {
			await ownWrite(async () => {
				const result = await request<RefreshResponse>('/api/v1/refresh', { method: 'POST' });
				refreshState = result.status;
				await reloadReview();
			});
		} catch (cause) {
			refreshState = 'failed';
			error = handleErr(cause, 'The source could not be refreshed.');
		}
	}

	async function download(path: string, filename: string) {
		try {
			const response = await fetch(path, { headers: { Authorization: `Bearer ${secret}` } });
			if (!response.ok) throw new Error(`The download is unavailable (${response.status}).`);
			const url = URL.createObjectURL(await response.blob());
			const link = document.createElement('a');
			link.href = url;
			link.download = filename;
			link.click();
			URL.revokeObjectURL(url);
		} catch (cause) {
			error = handleErr(cause, 'The download could not be created.');
		}
	}

	/**
	 * Opens the live channel and reads its events until it closes. A closed or
	 * failed channel marks the watch stopped, keeps the review on screen, and
	 * tries again after a delay that doubles up to 30 seconds. Once it
	 * reconnects, the page checks for a revision it missed. After a shutdown
	 * event, or when the server refuses the session secret, it stops trying.
	 */
	async function connectEvents() {
		clearTimeout(retryTimer);
		streamAbort?.abort();
		const abort = new AbortController();
		streamAbort = abort;
		try {
			const response = await fetch('/api/v1/events', {
				headers: { Authorization: `Bearer ${secret}` },
				signal: abort.signal
			});
			if (!response.ok || !response.body) {
				connection = refusedConnection(response.status);
				throw new Error('Live review updates are unavailable.');
			}
			const resumed = connection !== 'live';
			connection = 'live';
			retryDelay = FIRST_RETRY_DELAY;
			if (resumed) void reloadReview(false);
			const reader = response.body.getReader();
			const decoder = new TextDecoder();
			let buffer = '';
			while (true) {
				const { done, value } = await reader.read();
				if (done) break;
				buffer += decoder.decode(value, { stream: true });
				const messages = buffer.split('\n\n');
				buffer = messages.pop() ?? '';
				for (const message of messages) {
					const data = message
						.split('\n')
						.find((line) => line.startsWith('data:'))
						?.slice('data:'.length)
						.trim();
					if (data) await handleEvent(data);
				}
			}
		} catch {
			// A failed or cut-off channel is reported below, like one that closed.
		}
		if (abort.signal.aborted || !retries(connection)) return;
		connection = 'lost';
		retryTimer = setTimeout(connectEvents, retryDelay);
		retryDelay = nextRetryDelay(retryDelay);
	}

	/** Tries the live channel again now, as the stopped-watch notice offers. */
	function reconnect() {
		retryDelay = FIRST_RETRY_DELAY;
		void connectEvents();
	}

	/**
	 * Handles one live event. A watch status change shows at once; any other
	 * update checks for a newer revision, which is held until the reader
	 * applies it.
	 */
	async function handleEvent(data: string) {
		const event = JSON.parse(data) as { kind?: string; watch?: WatchStatus };
		if (event.kind === 'shutdown') {
			connection = 'stopped';
			return;
		}
		if (event.kind === 'watch_status') {
			if (review && event.watch) review.watch = event.watch;
			return;
		}
		if (writesInFlight > 0) {
			checkAfterWrites = true;
			return;
		}
		await reloadReview(false);
	}

	/**
	 * Marks or clears the file on screen. A mark records the content the reader
	 * sees, by its fingerprint and the revision it was loaded at, even when the
	 * overview has moved on while a reload of the file is in flight.
	 */
	async function toggleViewed() {
		if (!review || !file || viewedPending) return;
		viewedPending = true;
		viewedError = null;
		const shown = file;
		const route = viewedRoute(shown.path);
		const revision = fileRevision;
		try {
			if (shownViewState(review.files, shown) === 'viewed') {
				await request<ViewedResponse>(route, { method: 'DELETE' });
			} else {
				await request<ViewedResponse>(route, {
					method: 'PUT',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ fingerprint: shown.id, revision })
				});
			}
		} catch (cause) {
			viewedError = handleErr(cause, 'The viewed mark could not be saved.');
		}
		try {
			await reloadReview(false);
		} finally {
			viewedPending = false;
		}
	}

	/**
	 * Loads a file into the diff pane. Reloading the file on screen keeps it and
	 * its controls mounted until the new content arrives; another file clears the
	 * pane first. Only the newest load replaces what is shown.
	 */
	async function selectFile(id: string, finding?: FindingSummary) {
		const ticket = ++fileLoads;
		const next = review?.files.find((summary) => summary.id === id);
		const revision = review?.revision ?? 0;
		activeFile = id;
		if (!next || !isShownFile(file, next)) file = null;
		fileError = null;
		// The server only has the newest content, so a file the held revision changed waits for it.
		if (held && next && !held.files.some((summary) => summary.id === id)) {
			file = null;
			fileLoading = false;
			fileError = `${formatPath(next.path)} changed in revision ${held.revision}. Apply it to read this file.`;
			return;
		}
		fileLoading = true;
		try {
			const detail = await request<FileDetail>(`/api/v1/files/${encodeURIComponent(id)}`);
			if (ticket !== fileLoads) return;
			file = detail;
			fileRevision = revision;
			viewedError = null;
			if (finding) {
				// A finding's line is inside the diff, so a collapsed generated diff opens first.
				if (detail.generated) openedGenerated = detail.path.raw;
				window.setTimeout(() => scrollToFinding(finding.id));
			}
		} catch (cause) {
			if (ticket !== fileLoads) return;
			file = null;
			fileError = handleErr(cause, 'The file diff could not be loaded.');
		} finally {
			if (ticket === fileLoads) fileLoading = false;
		}
	}

	/** Empties the diff pane when no file is left to show, and drops any file load in flight. */
	function closeFile() {
		fileLoads++;
		activeFile = null;
		file = null;
		fileError = null;
		fileLoading = false;
		viewedError = null;
	}

	/** Opens a file the reader picked, which replaces any reload notice. */
	function chooseFile(id: string) {
		notice = null;
		void selectFile(id);
	}

	async function selectFinding(finding: FindingSummary) {
		findingError = null;
		notice = null;
		if (keptDraft?.findingId !== finding.id) keptDraft = null;
		// The editor fills from another finding's text, so its edit starts over.
		if (activeFinding?.id !== finding.id) editBase = null;
		try {
			activeFinding = await request<FindingDetail>(`/api/v1/findings/${encodeURIComponent(finding.id)}`);
			if (!finding.navigable || !review) return;
			const target = review.files.find((file) => file.path.display === finding.path.display);
			if (target) await selectFile(target.id, finding);
		} catch (cause) {
			findingError = handleErr(cause, 'The finding could not be loaded.');
		}
	}

	/** Records the open finding's text when the reader first changes the editor. */
	function startEdit() {
		if (!activeFinding || editBase?.findingId === activeFinding.id) return;
		editBase = { findingId: activeFinding.id, body: activeFinding.body };
	}

	async function editFinding(draft: FindingDraft): Promise<string | null> {
		if (!review || !activeFinding) return 'Select a finding before editing it.';
		const findingId = activeFinding.id;
		const base = editBase?.findingId === findingId ? editBase : { findingId, body: activeFinding.body };
		try {
			const stale = await ownWrite(async () => {
				const stale = await checkFindingWrite(base);
				if (stale || !review) return stale;
				const result = await request<FindingMutation>(`/api/v1/findings/${encodeURIComponent(findingId)}`, {
					method: 'PATCH',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ expectedRevision: review.revision, ...draft })
				});
				await applyFindingMutation(result);
				return null;
			});
			if (stale) {
				keepDraft(findingId, draft);
				// The dialog showed the reader the newer text, so their next save is made knowing it.
				const current = review?.findings.find((finding) => finding.id === findingId);
				editBase = current ? { findingId, body: current.body } : null;
				return `${stale} Your edit was not saved, and your text is still here.`;
			}
			keptDraft = null;
			editBase = null;
			return null;
		} catch (cause) {
			if (cause instanceof RequestError && cause.code === 'revision_conflict') keepDraft(findingId, draft);
			return handleErr(cause, 'The finding could not be saved.');
		}
	}

	async function decideFinding(decision: 'resolve' | 'reopen' | 'dismiss' | 'accept-risk'): Promise<string | null> {
		if (!review || !activeFinding) return 'Select a finding before recording a decision.';
		const findingId = activeFinding.id;
		const status = activeFinding.status;
		keptDraft = null;
		try {
			const stale = await ownWrite(async () => {
				const stale = await checkFindingWrite({ findingId, status });
				if (stale || !review) return stale;
				const result = await request<FindingMutation>(`/api/v1/findings/${encodeURIComponent(findingId)}/decision`, {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ expectedRevision: review.revision, decision })
				});
				await applyFindingMutation(result);
				return null;
			});
			return stale ? `${stale} Your decision was not saved.` : null;
		} catch (cause) {
			return handleErr(cause, 'The decision could not be saved.');
		}
	}

	/**
	 * Checks one of the reader's finding writes before it is sent. A held
	 * revision applies first, so the write goes to the revision it changes.
	 * Then the finding on screen is compared with what the reader started
	 * from. Returns `null` when the finding is there and unchanged. Otherwise
	 * it opens the conflict dialog and returns the reason.
	 */
	async function checkFindingWrite(base: FindingWriteBase): Promise<string | null> {
		if (held) {
			const revision = held.revision;
			await reloadReview(true);
			if (!review || held) return `Revision ${revision} could not be applied.`;
		}
		if (!review) return 'The review is not loaded.';
		const stale = staleFindingWrite(review, base);
		if (stale) {
			conflictRevision = review.revision;
			conflictReason = stale;
		}
		return stale;
	}

	/** Keeps a finding edit that did not save, for the editor and the conflict dialog. */
	function keepDraft(findingId: string, draft: FindingDraft) {
		keptDraft = { findingId, ...draft };
		draftCopied = false;
	}

	/** Closes the conflict dialog. */
	function closeConflict() {
		conflictRevision = null;
		conflictReason = null;
	}

	async function applyFindingMutation(result: FindingMutation) {
		activeFinding = result.finding;
		await reloadReview();
	}

	function navigateQueue(direction: 1 | -1) {
		if (!visibleFindings.length) return;
		const current = visibleFindings.findIndex((finding) => finding.id === activeFinding?.id);
		// FIXME: this is awful
		const nextIndex =
			current < 0
				? direction > 0
					? 0
					: visibleFindings.length - 1
				: (current + direction + visibleFindings.length) % visibleFindings.length;
		const next = visibleFindings[nextIndex];
		void selectFinding(next);
	}

	/** Opens the previous or next file in the file tree's order. The ends do not wrap. */
	function moveFile(direction: 1 | -1) {
		const target = direction > 0 ? position?.next : position?.previous;
		if (target) chooseFile(target.id);
	}

	/**
	 * Enters or leaves focus mode. Either way the control that had keyboard
	 * focus is removed from the page, so focus moves to the Leave focus button
	 * on entering and back to the Focus button on leaving, not to the body.
	 */
	async function setFocused(next: boolean) {
		focused = next;
		await tick();
		if (next) focusBar?.focusLeave();
		else focusButton?.focus();
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.defaultPrevented || event.metaKey) return;
		const target = event.target;
		if (takesText(target)) return;
		// [ and ] need AltGr or Option on many layouts, so only those two keys accept Alt.
		// Windows reports AltGr as Ctrl+Alt, so Ctrl counts only without Alt.
		if (event.key === '[' || event.key === ']') {
			if (event.ctrlKey && !event.altKey && !event.getModifierState('AltGraph')) return;
			if (!review || inDialog(target)) return;
			event.preventDefault();
			moveFile(event.key === ']' ? 1 : -1);
			return;
		}
		if (event.ctrlKey || event.altKey) return;
		if (event.key === 'j') {
			event.preventDefault();
			navigateQueue(1);
			return;
		} else if (event.key === 'k') {
			event.preventDefault();
			navigateQueue(-1);
			return;
		}
		// An open dialog takes Esc for itself, and the page behind it stays as it is.
		if (!review || inDialog(target)) return;
		if (event.key === 'u' && held) {
			event.preventDefault();
			if (!event.repeat) applyHeld();
		} else if (event.key === 'b') {
			event.preventDefault();
			// A held key would flip focus mode on every repeat.
			if (!event.repeat) void setFocused(!focused);
		} else if (event.key === 'Escape' && focused) {
			event.preventDefault();
			if (!event.repeat) void setFocused(false);
		}
	}

	/** Whether a key event started inside an open dialog, which owns its keys. */
	function inDialog(target: EventTarget | null) {
		return target instanceof Element && target.closest('dialog') !== null;
	}

	function scrollToFinding(id: string) {
		document.querySelector<HTMLElement>(`[data-mire-finding="${CSS.escape(id)}"]`)?.scrollIntoView({
			block: 'center',
			behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth'
		});
	}

	/** Copies the kept draft's text, so the reader has it even if the editor closes. */
	async function copyDraft() {
		if (!keptDraft) return;
		try {
			await navigator.clipboard.writeText(keptDraft.body);
			draftCopied = true;
		} catch {
			draftCopied = false;
		}
	}

	function selectFileFromDrawer(id: string) {
		mobilePane = null;
		chooseFile(id);
	}

	function selectFindingFromDrawer(finding: FindingSummary) {
		mobilePane = null;
		void selectFinding(finding);
	}
</script>

{#snippet refreshLabel(state: RefreshState)}
	{#if state === 'refreshed'}
		· refreshed
	{:else if state === 'unchanged'}
		· current
	{/if}
{/snippet}

<svelte:head>
	<meta name="description" content="A local browser surface for reviewing a Mire review file." />
</svelte:head>

<svelte:window onkeydown={onKeydown} />

<a class="skip-link" href="#review-content">Skip to diff</a>

<div class="app-shell" class:focused={inFocus}>
	{#if inFocus}
		<FocusBar
			bind:this={focusBar}
			{file}
			{position}
			viewState={shownView}
			onPrevious={() => moveFile(-1)}
			onNext={() => moveFile(1)}
			onToggleViewed={toggleViewed}
			onLeave={() => void setFocused(false)} />
	{:else}
		<header class="topbar">
			<div class="brand" aria-label="Mire">
				<strong>mire</strong>
			</div>
			<div class="review-meta">
				<strong class="review-title" {title}>{title}</strong><span aria-hidden="true">·</span>
				<span class="review-detail">
					{#if review}
						{review.source} · revision {review.revision} {@render refreshLabel(refreshState)}
					{:else}
						Connecting to local review
					{/if}
				</span>
			</div>
			<div class="topbar-actions">
				{#if review && isNarrow}
					<button class="topbar-button mobile-pane-button" onclick={() => (mobilePane = 'files')}>
						<span class="i-lucide-files icon" aria-hidden="true"></span>Files
					</button>
					<button class="topbar-button mobile-pane-button" onclick={() => (mobilePane = 'findings')}>
						<span class="i-lucide-list-checks icon" aria-hidden="true"></span>Findings
					</button>
				{/if}
				{#if review}
					<button
						bind:this={focusButton}
						class="topbar-button"
						aria-keyshortcuts="b"
						onclick={() => void setFocused(true)}>
						<span class="i-lucide-maximize-2 icon" aria-hidden="true"></span>Focus
						<kbd class="key-hint" aria-hidden="true">b</kbd>
					</button>
				{/if}
				{#if review && hasMetadata}
					<button class="topbar-button" aria-haspopup="dialog" onclick={() => (descriptionOpen = true)}>
						<span class="i-lucide-file-text icon" aria-hidden="true"></span>Description
					</button>
				{/if}
				{#if review}
					<button class="topbar-button" disabled={refreshState === 'pending'} onclick={() => void refreshReview()}>
						<span class="i-lucide-refresh-cw icon" class:spinning={refreshState === 'pending'} aria-hidden="true"
						></span>
						{#if refreshState === 'pending'}
							Refreshing…
						{:else}
							Refresh source
						{/if}
					</button>
					<button class="topbar-button" onclick={() => (finishReviewOpen = true)}>
						<span class="i-lucide-circle-check-big icon" aria-hidden="true"></span>Finish review
					</button>
					<button
						class="topbar-button icon-button"
						aria-label={themeLabel}
						title={themeLabel}
						aria-pressed={theme === 'dark'}
						onclick={() => (theme = theme === 'dark' ? 'light' : 'dark')}>
						<span class={theme === 'dark' ? 'i-lucide-sun icon' : 'i-lucide-moon icon'} aria-hidden="true"></span>
					</button>
				{/if}
				<span
					class:ready={review?.watch === 'watching' && connection === 'live'}
					class:degraded={review?.watch === 'degraded' || (review !== null && connection !== 'live')}
					class="status"
					aria-live="polite">{review ? watchLabel(connection, review.watch, review.revision) : 'loading'}</span>
			</div>
		</header>
	{/if}

	<div class="live-strip">
		<!-- Only the status sentences are announced, not the buttons and file list around them. -->
		<p class="sr-only" aria-live="polite">
			{#if review && changes}{heldHeading(changes)} {heldSummary(changes)}{/if}
			{#if stopped}{stopped.heading}. {stopped.body}{/if}
		</p>
		{#if review && changes}
			<section class="held" aria-labelledby="held-heading">
				<div class="held-row">
					<span class="i-lucide-refresh-cw held-icon" aria-hidden="true"></span>
					<p>
						<strong id="held-heading">{heldHeading(changes)}</strong>
						{heldSummary(changes)} Your place is held here until you apply it.
					</p>
					<div class="held-actions">
						<button class="held-apply" aria-keyshortcuts="u" onclick={applyHeld}>
							Apply revision {changes.revision}
							<kbd class="key-hint" aria-hidden="true">u</kbd>
						</button>
						{#if changes.files.length}
							<button
								class="held-toggle"
								aria-expanded={heldListOpen}
								aria-controls="held-files"
								onclick={() => (heldListOpen = !heldListOpen)}>
								{heldListOpen ? 'Hide changes' : 'See what changed'}
							</button>
						{/if}
					</div>
				</div>
				{#if heldListOpen && changes.files.length}
					<ul id="held-files" class="held-files" aria-label="Files changed in revision {changes.revision}">
						{#each changes.files as change (change.path.raw)}
							<li><span class="held-change">{change.change}</span> {formatPath(change.path)}</li>
						{/each}
					</ul>
				{/if}
			</section>
		{/if}
		{#if stopped}
			<section class="watch-stopped" aria-labelledby="watch-stopped-heading">
				<span class="i-lucide-triangle-alert watch-icon" aria-hidden="true"></span>
				<p><strong id="watch-stopped-heading">{stopped.heading}.</strong> {stopped.body}</p>
				{#if stopped.action === 'reconnect'}
					<button onclick={reconnect}>Try again</button>
				{:else if stopped.action === 'refresh'}
					<button disabled={refreshState === 'pending'} onclick={() => void refreshReview()}>Refresh source</button>
				{/if}
			</section>
		{/if}
	</div>

	<main id="review-content" class="workspace" tabindex="-1">
		{#if error}
			<section class="message error" aria-labelledby="error-heading">
				<h1 id="error-heading">Review unavailable</h1>
				<p>{error}</p>
				{#if import.meta.env.DEV}
					<form class="session-form" onsubmit={connectDevelopmentSession}>
						<label for="session-secret">Session secret or Mire URL</label>
						<div>
							<input
								id="session-secret"
								type="password"
								bind:value={sessionSecretInput}
								autocomplete="off"
								placeholder="http://127.0.0.1:3737/#…" />
							<button type="submit">Connect</button>
						</div>
					</form>
				{/if}
			</section>
		{:else if !review}
			<section class="message" aria-live="polite">
				<h1>Loading review</h1>
				<p>Reading the local review file.</p>
			</section>
		{:else}
			<!-- One DiffPane stays mounted across layouts, so its Split, Expand, scroll and draft state survive focus mode. -->
			{#if !isNarrow && !inFocus}
				<FileNavigator files={review.files} {activeFile} onSelect={chooseFile} />
			{/if}
			<DiffPane
				{file}
				{fileError}
				{fileLoading}
				{notice}
				{activeFinding}
				{findingError}
				{theme}
				viewState={shownView}
				{viewedError}
				heldChange={shownChange && changes ? { change: shownChange, revision: changes.revision } : null}
				keptDraft={shownDraft}
				onDraftTaken={() => (keptDraft = null)}
				onEditStart={startEdit}
				bind:openedGenerated
				showHeading={!inFocus}
				onDismissNotice={() => (notice = null)}
				onToggleViewed={toggleViewed}
				onFindingClick={selectFinding}
				onEditFinding={editFinding}
				onDecideFinding={decideFinding} />
			{#if !isNarrow && !inFocus}
				<FindingQueue
					findings={visibleFindings}
					activeFindingId={activeFinding?.id ?? null}
					openCount={review.totals.open}
					bind:filters
					onSelect={selectFinding} />
			{/if}
		{/if}
	</main>
	{#if inFocus}
		<KeyBar heldRevision={changes?.revision ?? null} />
	{/if}
	{#if review && isNarrow && mobilePane === 'files'}
		<Overlay open={true} labelledBy="files-drawer-heading" variant="drawer" onClose={() => (mobilePane = null)}>
			<section class="drawer-content">
				<header class="drawer-heading">
					<h2 id="files-drawer-heading">Changed files</h2>
					<button onclick={() => (mobilePane = null)}>Close files</button>
				</header>
				<FileNavigator files={review.files} {activeFile} drawer onSelect={selectFileFromDrawer} />
			</section>
		</Overlay>
	{:else if review && isNarrow && mobilePane === 'findings'}
		<Overlay open={true} labelledBy="findings-drawer-heading" variant="drawer" onClose={() => (mobilePane = null)}>
			<section class="drawer-content">
				<header class="drawer-heading">
					<h2 id="findings-drawer-heading">Review queue</h2>
					<button onclick={() => (mobilePane = null)}>Close findings</button>
				</header>
				<FindingQueue
					findings={visibleFindings}
					activeFindingId={activeFinding?.id ?? null}
					openCount={review.totals.open}
					drawer
					bind:filters
					onSelect={selectFindingFromDrawer} />
			</section>
		</Overlay>
	{/if}
	{#if review && completion}
		<Overlay
			open={finishReviewOpen}
			labelledBy="finish-review-heading"
			variant="dialog"
			onClose={() => (finishReviewOpen = false)}>
			<section class="dialog-content">
				<h1 id="finish-review-heading">Finish review</h1>
				<p>
					{completion.ready ? 'This review is ready to hand off.' : 'Resolve the remaining review work before handoff.'}
				</p>
				<ul>
					<li>{completion.unviewedFiles} unviewed file{completion.unviewedFiles === 1 ? '' : 's'}</li>
					<li>{completion.openFindings} open finding{completion.openFindings === 1 ? '' : 's'}</li>
					<li>{completion.unsafeAnchors} stale or ambiguous anchor{completion.unsafeAnchors === 1 ? '' : 's'}</li>
				</ul>
				<div class="downloads" aria-label="Review downloads">
					<button onclick={() => void download('/api/v1/exports/notes.md', 'mire-notes.md')}>Markdown</button>
					<button onclick={() => void download('/api/v1/exports/notes.json', 'mire-notes.json')}>JSON</button>
					<button onclick={() => void download('/api/v1/exports/context.json', 'mire-context.json')}
						>Agent context</button>
				</div>
				<button class="close" onclick={() => (finishReviewOpen = false)}>Close</button>
			</section>
		</Overlay>
	{/if}
	{#if review && hasMetadata}
		<Overlay
			open={descriptionOpen}
			labelledBy="description-heading"
			variant="dialog"
			onClose={() => (descriptionOpen = false)}>
			<section class="dialog-content">
				<h1 id="description-heading" class="description-heading">{title}</h1>
				<p class="description-label">Description</p>
				{#if description}
					<div class="description-body">{description}</div>
				{:else}
					<p class="description-empty">No description.</p>
				{/if}
				<button class="close" onclick={() => (descriptionOpen = false)}>Close</button>
			</section>
		</Overlay>
	{/if}
	{#if conflictRevision !== null}
		<Overlay open={true} labelledBy="conflict-heading" variant="dialog" onClose={closeConflict}>
			<section class="dialog-content">
				{#if conflictReason && keptDraft}
					<h1 id="conflict-heading">Your edit was not saved</h1>
					<p>
						{conflictReason} Revision {conflictRevision} is on screen now. Your text is kept below and in the editor. Read
						the finding, then save again.
					</p>
				{:else if conflictReason}
					<h1 id="conflict-heading">Your decision was not saved</h1>
					<p>
						{conflictReason} Revision {conflictRevision} is on screen now. Read the finding, then decide again.
					</p>
				{:else if keptDraft}
					<h1 id="conflict-heading">Your edit was not saved</h1>
					<p>
						Revision {conflictRevision} was written while you edited. Your text is kept below and in the editor. Apply revision
						{conflictRevision}, then save again.
					</p>
				{:else}
					<h1 id="conflict-heading">Review changed</h1>
					<p>
						Revision {conflictRevision} was written while you worked, so your change was not saved. Apply it, then try again.
					</p>
				{/if}
				{#if keptDraft}
					<label class="kept-draft">
						<span>Your text</span>
						<textarea readonly rows="4" value={keptDraft.body}></textarea>
					</label>
				{/if}
				<div class="conflict-actions">
					<!-- The dialog opens on its main action, not on the read-only copy of the text. -->
					{#if conflictReason}
						<!-- svelte-ignore a11y_autofocus -->
						<button class="primary-button" autofocus onclick={closeConflict}>
							{keptDraft ? 'Keep editing' : 'Close'}
						</button>
					{:else}
						<!-- svelte-ignore a11y_autofocus -->
						<button class="primary-button" autofocus onclick={() => void reloadReview()}>
							Apply revision {conflictRevision}{keptDraft ? ' and keep draft' : ''}
						</button>
					{/if}
					{#if keptDraft}
						<button class="secondary-button" onclick={() => void copyDraft()}>
							{draftCopied ? 'Copied' : 'Copy text'}
						</button>
					{/if}
				</div>
			</section>
		</Overlay>
	{/if}
</div>

<style>
	.app-shell {
		height: 100vh;
		height: 100dvh;
		min-height: 0;
		overflow: hidden;
		display: grid;
		grid-template-rows: auto auto 1fr;
	}
	.app-shell.focused {
		grid-template-rows: auto auto 1fr auto;
	}
	/* The bar is inverted in light palettes. In dark ones an inverted bar would
	   be the brightest thing on screen, so it sits one step above the page. */
	.held {
		--held-bg: var(--selected);
		--held-ink: var(--selected-ink);
		border-bottom: 1px solid var(--line);
		background: var(--held-bg);
		color: var(--held-ink);
		font-size: 0.78rem;
	}
	:global(:root[data-theme='dark']) .held {
		--held-bg: var(--paper-deep);
		--held-ink: var(--ink);
	}
	.held-row,
	.watch-stopped {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 0.75rem;
		padding: 0.55rem 1rem;
	}
	.held-row p,
	.watch-stopped p {
		flex: 1 1 20rem;
		min-width: 0;
		margin: 0;
		line-height: 1.45;
	}
	.held-icon,
	.watch-icon {
		width: 0.95rem;
		height: 0.95rem;
		flex: none;
	}
	.held-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.held-actions button,
	.watch-stopped button {
		min-height: 1.85rem;
		padding: 0.25rem 0.75rem;
		font:
			600 0.72rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		cursor: pointer;
		white-space: nowrap;
	}
	.held-apply {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		border: 1px solid var(--held-ink);
		background: var(--held-ink);
		color: var(--held-bg);
	}
	.held-apply .key-hint {
		color: inherit;
		opacity: 0.75;
	}
	.held-toggle {
		border: 1px solid var(--held-ink);
		background: transparent;
		color: var(--held-ink);
	}
	.held button:focus-visible {
		outline: 2px solid var(--held-ink);
		outline-offset: 2px;
	}
	.held-files {
		margin: 0;
		padding: 0 1rem 0.6rem 2.7rem;
		list-style: none;
		font:
			0.75rem/1.6 'Google Sans Code Variable',
			'Google Sans Code',
			monospace;
		overflow-wrap: anywhere;
	}
	.held-change {
		display: inline-block;
		min-width: 4.5rem;
		opacity: 0.8;
	}
	.watch-stopped {
		border-bottom: 1px solid var(--danger);
		background: var(--surface);
		font-size: 0.78rem;
	}
	.watch-stopped strong,
	.watch-icon {
		color: var(--danger);
	}
	.watch-stopped button {
		border: 1px solid var(--line-strong);
		background: var(--paper);
		color: var(--ink);
	}
	.watch-stopped button:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}
	.kept-draft {
		display: grid;
		gap: 0.25rem;
		margin: 0.9rem 0;
		color: var(--muted);
		font-size: 0.72rem;
		font-weight: 600;
	}
	.kept-draft textarea {
		width: 100%;
		border: 1px solid var(--line-strong);
		padding: 0.45rem 0.55rem;
		background: var(--paper);
		color: var(--ink);
		font: inherit;
		font-size: 0.82rem;
		font-weight: 400;
		line-height: 1.5;
		resize: vertical;
	}
	.conflict-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.secondary-button {
		min-height: 2.25rem;
		border: 1px solid var(--line-strong);
		padding: 0.35rem 0.7rem;
		background: var(--surface);
		color: var(--ink);
		font:
			600 0.75rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.topbar {
		display: grid;
		grid-template-columns: 14rem minmax(8rem, 1fr) auto;
		align-items: center;
		border-bottom: 1px solid var(--line);
		background: var(--surface);
	}
	.brand {
		display: flex;
		align-items: baseline;
		gap: 0.4rem;
		padding: 0.5rem 1rem;
		font-family: 'Google Sans Variable', 'Google Sans', sans-serif;
	}
	.brand strong {
		font-size: 1.15rem;
		letter-spacing: -0.04em;
	}

	.review-meta {
		min-width: 0;
		display: flex;
		align-items: baseline;
		gap: 0.35rem;
		padding: 0.5rem 0.75rem;
		white-space: nowrap;
	}
	.topbar-actions {
		min-width: 0;
		display: flex;
		flex-wrap: nowrap;
		align-items: center;
		justify-content: flex-end;
		gap: 0.5rem;
		padding: 0.5rem 1rem;
	}
	.review-meta strong {
		font-size: 0.82rem;
	}
	.review-meta span,
	.status {
		color: var(--muted);
		font-size: 0.72rem;
	}
	.review-meta span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.review-detail {
		min-width: 0;
	}
	.review-meta .review-title {
		min-width: 0;
		max-width: 60%;
		flex: 0 1 auto;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.topbar-button {
		min-height: 1.85rem;
		border: 1px solid var(--line-strong);
		padding: 0.25rem 0.6rem;
		background: var(--surface);
		color: var(--ink);
		font:
			600 0.72rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.35rem;
		white-space: nowrap;
		transition:
			background-color 100ms ease-out,
			border-color 100ms ease-out;
	}
	.key-hint {
		color: var(--muted);
		font: inherit;
		font-weight: 500;
	}
	.topbar-button .icon {
		width: 0.85rem;
		height: 0.85rem;
		flex: none;
	}
	.icon-button {
		width: 1.85rem;
		padding-inline: 0;
	}
	.spinning {
		animation: spin 800ms linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(1turn);
		}
	}
	.topbar-button:hover {
		background: var(--paper);
		border-color: var(--ink);
	}
	.topbar-button:active {
		background: var(--paper-deep);
	}
	.topbar-button:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}
	.topbar-button:disabled:hover {
		background: var(--surface);
		border-color: var(--line-strong);
	}
	.status {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		white-space: nowrap;
	}
	.status::before {
		width: 0.4rem;
		height: 0.4rem;
		border-radius: 50%;
		background: var(--faint);
		content: '';
	}
	.status.ready::before {
		background: var(--success);
	}
	.status.degraded::before {
		background: var(--danger);
	}
	.workspace {
		min-height: 0;
		overflow: hidden;
		display: grid;
		grid-template-columns: 14rem minmax(0, 1fr) 18rem;
	}
	.focused .workspace {
		grid-template-columns: minmax(0, 1fr);
	}
	.focused .workspace > :global(.diff-pane) {
		padding-inline: max(1.25rem, calc((100% - 72rem) / 2));
	}
	.message {
		align-self: center;
		grid-column: 1 / -1;
		width: min(100%, 42rem);
		max-width: 42rem;
		padding: clamp(2rem, 8vw, 7rem);
	}
	.message h1 {
		margin: 0;
		font:
			600 clamp(2rem, 5vw, 3.5rem)/1 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		letter-spacing: -0.04em;
	}
	.message p {
		color: var(--muted);
		margin-top: 0.5rem;
	}
	.message.error h1 {
		color: var(--danger);
	}
	.session-form {
		display: grid;
		gap: 0.4rem;
		max-width: 34rem;
		margin-top: 1.5rem;
	}
	.session-form label {
		font:
			600 0.75rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.session-form div {
		display: flex;
		gap: 0.5rem;
	}
	.session-form input {
		min-width: 0;
		min-height: 2.5rem;
		flex: 1;
		border: 1px solid var(--line-strong);
		padding: 0.5rem 0.65rem;
		background: var(--surface);
		color: var(--ink);
		font:
			0.82rem 'Google Sans Code Variable',
			'Google Sans Code',
			monospace;
	}
	.session-form input:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
	.session-form button {
		min-height: 2.5rem;
		border: 1px solid var(--ink);
		padding: 0.5rem 0.9rem;
		background: var(--ink);
		color: var(--surface);
		font:
			600 0.78rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		cursor: pointer;
	}
	.dialog-content h1,
	.drawer-heading h2 {
		margin: 0;
		font:
			600 1.25rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.dialog-content p {
		color: var(--muted);
		line-height: 1.5;
		margin-top: 0.4rem;
	}
	.description-heading {
		overflow-wrap: anywhere;
	}
	.dialog-content .description-label {
		margin: 0.25rem 0 0;
		font-size: 0.75rem;
	}
	.description-body {
		max-height: min(60vh, 28rem);
		overflow: auto;
		margin: 1rem 0;
		border: 1px solid var(--line);
		padding: 0.75rem;
		background: var(--paper);
		font-size: 0.82rem;
		line-height: 1.55;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.dialog-content ul {
		margin: 1rem 0;
		padding-left: 1.2rem;
		line-height: 1.7;
	}
	.downloads {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 1rem;
	}
	.downloads button,
	.dialog-content .close,
	.primary-button,
	.drawer-heading button,
	.session-form button {
		min-height: 2.25rem;
		border: 1px solid var(--ink);
		padding: 0.35rem 0.7rem;
		background: var(--surface);
		color: var(--ink);
		font:
			600 0.75rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.primary-button,
	.session-form button {
		background: var(--ink);
		color: var(--surface);
	}
	.dialog-content .close {
		width: 100%;
	}
	.drawer-content {
		height: 100%;
		display: grid;
		grid-template-rows: auto minmax(0, 1fr);
	}
	.drawer-heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		padding: 0.75rem;
		border-bottom: 1px solid var(--line);
	}
	@media (hover: hover) {
		.topbar-button:hover,
		.downloads button:hover,
		.dialog-content .close:hover,
		.drawer-heading button:hover,
		.session-form button:hover {
			background: var(--button-hover);
			border-color: var(--ink);
		}
		.primary-button:hover,
		.session-form button:hover {
			background: var(--selected-ink);
			color: var(--selected);
		}
	}
	@media (max-width: 54rem) {
		.topbar {
			display: grid;
			grid-template-columns: auto minmax(0, 1fr) auto;
			align-items: center;
			gap: 0.75rem;
			padding: 0.5rem 1rem;
		}
		.brand,
		.review-meta,
		.topbar-actions {
			padding: 0;
		}
		.review-meta {
			display: none;
		}
		.topbar-actions {
			grid-column: 3;
			margin-left: 0;
		}
		.workspace {
			grid-template-columns: minmax(0, 1fr);
		}

		.topbar-actions .topbar-button:not(.mobile-pane-button):not(.icon-button) {
			width: 2.75rem;
			padding-inline: 0;
			font-size: 0;
		}
		.topbar-actions .topbar-button .icon {
			width: 1rem;
			height: 1rem;
		}
	}
	@media (max-width: 32rem) {
		.topbar-actions {
			flex-wrap: wrap;
			gap: 0.35rem;
		}
		.topbar-actions .mobile-pane-button {
			width: 2.75rem;
			padding-inline: 0;
			font-size: 0;
		}
		.status {
			gap: 0;
			font-size: 0;
		}
		.message {
			padding: 2rem 1rem;
		}
		.session-form div {
			align-items: stretch;
			flex-direction: column;
		}
		.session-form button {
			width: 100%;
		}
	}
	@media (pointer: coarse) {
		.topbar-button,
		.downloads button,
		.dialog-content .close,
		.primary-button,
		.drawer-heading button {
			min-height: 2.75rem;
		}
	}
	@media (prefers-reduced-motion: no-preference) {
		.topbar-button,
		.downloads button,
		.dialog-content .close,
		.primary-button,
		.drawer-heading button,
		.session-form button {
			transition:
				background-color 100ms ease-out,
				border-color 100ms ease-out;
		}
	}
</style>
