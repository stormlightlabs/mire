<script lang="ts">
	import { onMount } from 'svelte';
	import DiffPane from '$lib/DiffPane.svelte';
	import FileNavigator from '$lib/FileNavigator.svelte';
	import FindingQueue from '$lib/FindingQueue.svelte';
	import Overlay from '$lib/Overlay.svelte';
	import { OverviewLoads } from '$lib/reload';
	import { loadDevelopmentSessionSecret, parseSessionSecret, saveDevelopmentSessionSecret } from '$lib/session';
	import { applyTheme, loadTheme, saveTheme, type Theme } from '$lib/theme';
	import {
		completionSummary,
		defaultFindingFilters,
		filterFindings,
		hasReviewMetadata,
		isShownFile,
		reopenedFile,
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
	let refreshState = $state<RefreshState>('idle');
	let finishReviewOpen = $state(false);
	let descriptionOpen = $state(false);
	let sessionSecretInput = $state('');
	let theme = $state<Theme>('light');
	let themeReady = $state(false);
	let isNarrow = $state(false);
	let mobilePane = $state<'files' | 'findings' | null>(null);

	let streamAbort: AbortController | null = null;
	let secret = '';
	/** The review revision the shown file was requested at; a viewed mark records it. */
	let fileRevision = 0;
	/** Counts file loads, so only the newest one replaces the shown file. */
	let fileLoads = 0;
	/** Orders overview loads, so a slow response cannot overwrite a newer one. */
	const overviewLoads = new OverviewLoads();

	const completion = $derived(review ? completionSummary(review) : null);
	const activeSummary = $derived(review?.files.find((summary) => summary.id === activeFile) ?? null);
	const shownView = $derived(review && file ? shownViewState(review.files, file) : 'unviewed');
	const visibleFindings = $derived(review ? filterFindings(review.findings, filters) : []);
	const title = $derived(reviewTitle(review));
	const description = $derived(reviewDescription(review));
	const hasMetadata = $derived(hasReviewMetadata(review));
	const themeLabel = $derived(theme === 'dark' ? 'Use light mode' : 'Use dark mode');

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
			throw new Error(problem?.detail ?? `The requested review data is unavailable (${response.status}).`);
		}
		return (await response.json()) as T;
	}

	async function loadReview(): Promise<boolean> {
		const ticket = overviewLoads.start(false);
		try {
			const overview = await request<ReviewOverview>('/api/v1/review');
			if (!overviewLoads.settle(ticket).apply) return true;
			review = overview;
			if (review.files[0]) await selectFile(review.files[0].id);
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
	 * Reloads the overview. With `reopen`, as after a source change or a finding
	 * change, the open file loads again too; a viewed mark changes no file's diff
	 * and passes `false`. When a newer reload's response is shown first, that one
	 * reopens the file instead, so the diff never lags the header.
	 */
	async function reloadReview(reopen = true) {
		if (!review) return;
		const ticket = overviewLoads.start(reopen);
		try {
			const overview = await request<ReviewOverview>('/api/v1/review');
			const settled = overviewLoads.settle(ticket);
			if (!settled.apply) return;
			const previous = activeSummary ?? file;
			review = overview;
			if (!settled.reopen) return;
			conflictRevision = null;
			const reopened = reopenedFile(review.files, previous);
			if (reopened.notice) notice = reopened.notice;
			if (reopened.file) await selectFile(reopened.file.id);
			else closeFile();
		} catch (cause) {
			error = handleErr(cause, 'The review could not be reloaded.');
		}
	}

	async function refreshReview() {
		if (!review || refreshState === 'pending') return;
		refreshState = 'pending';
		try {
			const result = await request<RefreshResponse>('/api/v1/refresh', { method: 'POST' });
			refreshState = result.status;
			await reloadReview();
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

	async function connectEvents() {
		streamAbort?.abort();
		streamAbort = new AbortController();
		try {
			const response = await fetch('/api/v1/events', {
				headers: { Authorization: `Bearer ${secret}` },
				signal: streamAbort.signal
			});
			if (!response.ok || !response.body) throw new Error('Live review updates are unavailable.');
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
		} catch (cause) {
			if (!streamAbort?.signal.aborted) {
				error = handleErr(cause, 'Live review updates disconnected.');
				window.setTimeout(connectEvents, 1_000);
			}
		}
	}

	async function handleEvent(data: string) {
		const event = JSON.parse(data) as { kind?: string; watch?: WatchStatus };
		if (event.kind === 'shutdown') return;
		await reloadReview(event.kind !== 'viewed');
	}

	/**
	 * Marks or clears the file on screen. A mark records the content the reader
	 * sees, by its fingerprint and the revision it was loaded at, even when the
	 * overview has moved on while a reload of the file is in flight.
	 */
	async function toggleViewed() {
		if (!review || !file) return;
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
		await reloadReview(false);
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
		fileLoading = true;
		try {
			const detail = await request<FileDetail>(`/api/v1/files/${encodeURIComponent(id)}`);
			if (ticket !== fileLoads) return;
			file = detail;
			fileRevision = revision;
			viewedError = null;
			if (finding) window.setTimeout(() => scrollToFinding(finding.id));
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
		try {
			activeFinding = await request<FindingDetail>(`/api/v1/findings/${encodeURIComponent(finding.id)}`);
			if (!finding.navigable || !review) return;
			const target = review.files.find((file) => file.path.display === finding.path.display);
			if (target) await selectFile(target.id, finding);
		} catch (cause) {
			findingError = handleErr(cause, 'The finding could not be loaded.');
		}
	}

	async function editFinding(draft: FindingDraft): Promise<string | null> {
		if (!review || !activeFinding) return 'Select a finding before editing it.';
		try {
			const result = await request<FindingMutation>(`/api/v1/findings/${encodeURIComponent(activeFinding.id)}`, {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ expectedRevision: review.revision, ...draft })
			});
			await applyFindingMutation(result);
			return null;
		} catch (cause) {
			return handleErr(cause, 'The finding could not be saved.');
		}
	}

	async function decideFinding(decision: 'resolve' | 'reopen' | 'dismiss' | 'accept-risk'): Promise<string | null> {
		if (!review || !activeFinding) return 'Select a finding before recording a decision.';
		try {
			const result = await request<FindingMutation>(
				`/api/v1/findings/${encodeURIComponent(activeFinding.id)}/decision`,
				{
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ expectedRevision: review.revision, decision })
				}
			);
			await applyFindingMutation(result);
			return null;
		} catch (cause) {
			return handleErr(cause, 'The decision could not be saved.');
		}
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

	function onKeydown(event: KeyboardEvent) {
		if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
		const target = event.target;
		if (
			target instanceof HTMLInputElement ||
			target instanceof HTMLTextAreaElement ||
			target instanceof HTMLSelectElement
		)
			return;
		if (event.key === 'j') {
			event.preventDefault();
			navigateQueue(1);
		} else if (event.key === 'k') {
			event.preventDefault();
			navigateQueue(-1);
		}
	}

	function scrollToFinding(id: string) {
		document.querySelector<HTMLElement>(`[data-mire-finding="${CSS.escape(id)}"]`)?.scrollIntoView({
			block: 'center',
			behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth'
		});
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

<div class="app-shell">
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
			{#if review && hasMetadata}
				<button class="topbar-button" aria-haspopup="dialog" onclick={() => (descriptionOpen = true)}>
					<span class="i-lucide-file-text icon" aria-hidden="true"></span>Description
				</button>
			{/if}
			{#if review}
				<button class="topbar-button" disabled={refreshState === 'pending'} onclick={() => void refreshReview()}>
					<span class="i-lucide-refresh-cw icon" class:spinning={refreshState === 'pending'} aria-hidden="true"></span>
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
				class:ready={review?.watch === 'watching'}
				class:degraded={review?.watch === 'degraded'}
				class="status"
				aria-live="polite">{review ? review.watch : 'loading'}</span>
		</div>
	</header>

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
		{:else if isNarrow}
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
				onDismissNotice={() => (notice = null)}
				onToggleViewed={toggleViewed}
				onFindingClick={selectFinding}
				onEditFinding={editFinding}
				onDecideFinding={decideFinding} />
		{:else}
			<FileNavigator files={review.files} {activeFile} onSelect={chooseFile} />
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
				onDismissNotice={() => (notice = null)}
				onToggleViewed={toggleViewed}
				onFindingClick={selectFinding}
				onEditFinding={editFinding}
				onDecideFinding={decideFinding} />
			<FindingQueue
				findings={visibleFindings}
				activeFindingId={activeFinding?.id ?? null}
				openCount={review.totals.open}
				bind:filters
				onSelect={selectFinding} />
		{/if}
	</main>
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
		<Overlay open={true} labelledBy="conflict-heading" variant="dialog" onClose={() => (conflictRevision = null)}>
			<section class="dialog-content">
				<h1 id="conflict-heading">Review changed</h1>
				<p>Revision {conflictRevision} is now current. Your unsaved edit is still here; reload, then save it again.</p>
				<button class="primary-button" onclick={() => void reloadReview()}>Reload review</button>
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
		grid-template-rows: auto 1fr;
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
