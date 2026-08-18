<script lang="ts">
	import { onMount } from 'svelte';
	import DiffPane from '$lib/DiffPane.svelte';
	import FileNavigator from '$lib/FileNavigator.svelte';
	import FindingQueue from '$lib/FindingQueue.svelte';
	import {
		defaultFindingFilters,
		filterFindings,
		type FileDetail,
		type FindingDetail,
		type FindingDraft,
		type FindingMutation,
		type FindingSummary,
		type Problem,
		type ReviewOverview
	} from '$lib/review';

	let review = $state<ReviewOverview | null>(null);
	let activeFile = $state<string | null>(null);
	let file = $state<FileDetail | null>(null);
	let activeFinding = $state<FindingDetail | null>(null);
	let error = $state<string | null>(null);
	let fileError = $state<string | null>(null);
	let findingError = $state<string | null>(null);
	let filters = $state({ ...defaultFindingFilters });
	let viewedFileIds = $state<string[]>([]);
	let conflictRevision = $state<number | null>(null);
	let secret = '';
	const visibleFindings = $derived(review ? filterFindings(review.findings, filters) : []);

	onMount(() => {
		secret = window.location.hash.slice(1);
		window.history.replaceState(null, '', `${window.location.pathname}${window.location.search}`);
		if (!secret) {
			error = 'This review URL is missing its session secret. Run mire serve again.';
			return;
		}
		void loadReview();
	});

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

	async function loadReview() {
		try {
			review = await request<ReviewOverview>('/api/v1/review');
			loadViewedFiles();
			if (review.files[0]) await selectFile(review.files[0].id);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The review could not be loaded.';
		}
	}

	async function reloadReview() {
		if (!review) return;
		try {
			review = await request<ReviewOverview>('/api/v1/review');
			loadViewedFiles();
			conflictRevision = null;
			if (activeFile) await selectFile(activeFile);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The review could not be reloaded.';
		}
	}

	async function selectFile(id: string, finding?: FindingSummary) {
		activeFile = id;
		file = null;
		fileError = null;
		try {
			file = await request<FileDetail>(`/api/v1/files/${encodeURIComponent(id)}`);
			markViewed(id);
			if (finding) window.setTimeout(() => scrollToFinding(finding.id));
		} catch (cause) {
			fileError = cause instanceof Error ? cause.message : 'The file diff could not be loaded.';
		}
	}

	async function selectFinding(finding: FindingSummary) {
		findingError = null;
		try {
			activeFinding = await request<FindingDetail>(`/api/v1/findings/${encodeURIComponent(finding.id)}`);
			if (!finding.navigable || !review) return;
			const target = review.files.find((file) => file.path.display === finding.path.display);
			if (target) await selectFile(target.id, finding);
		} catch (cause) {
			findingError = cause instanceof Error ? cause.message : 'The finding could not be loaded.';
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
			return cause instanceof Error ? cause.message : 'The finding could not be saved.';
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
			return cause instanceof Error ? cause.message : 'The decision could not be saved.';
		}
	}

	async function applyFindingMutation(result: FindingMutation) {
		activeFinding = result.finding;
		await reloadReview();
	}

	function loadViewedFiles() {
		if (!review) return;
		try {
			const stored = JSON.parse(localStorage.getItem(viewedStorageKey()) ?? '[]');
			viewedFileIds = Array.isArray(stored)
				? stored.filter((id): id is string => typeof id === 'string' && !!review?.files.some((file) => file.id === id))
				: [];
		} catch {
			viewedFileIds = [];
		}
	}

	function markViewed(fileId: string) {
		if (!review || viewedFileIds.includes(fileId)) return;
		viewedFileIds = [...viewedFileIds, fileId];
		try {
			localStorage.setItem(viewedStorageKey(), JSON.stringify(viewedFileIds));
		} catch {
			// Browser privacy settings can disable local storage; the review remains usable.
		}
	}

	function viewedStorageKey() {
		return `mire:viewed-files:${review?.reviewIdentity ?? ''}`;
	}

	function navigateQueue(direction: 1 | -1) {
		if (!visibleFindings.length) return;
		const current = visibleFindings.findIndex((finding) => finding.id === activeFinding?.id);
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
		if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return;
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
			behavior: 'smooth'
		});
	}
</script>

<svelte:head>
	<meta name="description" content="A local browser surface for reviewing a Mire review file." />
</svelte:head>

<svelte:window onkeydown={onKeydown} />

<a class="skip-link" href="#review-content">Skip to diff</a>

<div class="app-shell">
	<header class="topbar">
		<div class="brand" aria-label="Mire">
			<strong>mire</strong>
			<span>review in context</span>
		</div>
		<div class="review-meta">
			<strong>Local review</strong>
			<span>{review ? `${review.source} · revision ${review.revision}` : 'Connecting to local review'}</span>
		</div>
		<span class:ready={review} class="status" aria-live="polite">{review ? 'ready' : 'loading'}</span>
	</header>

	<main id="review-content" class="workspace" tabindex="-1">
		{#if error}
			<section class="message error" aria-labelledby="error-heading">
				<h1 id="error-heading">Review unavailable</h1>
				<p>{error}</p>
			</section>
		{:else if !review}
			<section class="message" aria-live="polite">
				<h1>Loading review</h1>
				<p>Reading the local review file.</p>
			</section>
		{:else}
			<FileNavigator files={review.files} {activeFile} {viewedFileIds} onSelect={selectFile} />
			<DiffPane
				{file}
				{fileError}
				{activeFinding}
				{findingError}
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
	{#if conflictRevision !== null}
		<dialog class="conflict" open aria-labelledby="conflict-heading">
			<div>
				<h1 id="conflict-heading">Review changed</h1>
				<p>Revision {conflictRevision} is now current. Your unsaved edit is still here; reload, then save it again.</p>
				<button onclick={() => void reloadReview()}>Reload review</button>
			</div>
		</dialog>
	{/if}
</div>

<style>
	.app-shell {
		min-height: 100vh;
		display: grid;
		grid-template-rows: auto 1fr;
	}
	.topbar {
		min-height: 3.75rem;
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.625rem 1rem;
		border-bottom: 1px solid var(--line);
		background: var(--surface);
	}
	.brand {
		min-width: 12rem;
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		font-family: 'Google Sans Variable', 'Google Sans', sans-serif;
	}
	.brand strong {
		font-size: 1.25rem;
		letter-spacing: -0.04em;
	}
	.brand span,
	.review-meta span,
	.status {
		color: var(--muted);
		font-size: 0.75rem;
	}
	.review-meta {
		min-width: 0;
		flex: 1;
		display: grid;
	}
	.review-meta strong {
		font-size: 0.875rem;
	}
	.review-meta span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.status {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		white-space: nowrap;
	}
	.status::before {
		width: 0.45rem;
		height: 0.45rem;
		border-radius: 50%;
		background: var(--muted);
		content: '';
	}
	.status.ready::before {
		background: var(--ink);
	}
	.workspace {
		min-height: 0;
		display: grid;
		grid-template-columns: 15.5rem minmax(26rem, 1fr) 19rem;
	}
	.message {
		align-self: center;
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
	}
	.message.error h1 {
		color: #a4332f;
	}
	.conflict {
		position: fixed;
		inset: 0;
		z-index: 2;
		display: grid;
		place-items: center;
		padding: 1rem;
		background: rgb(17 17 17 / 32%);
	}
	.conflict > div {
		max-width: 30rem;
		padding: 1.25rem;
		border: 1px solid var(--ink);
		background: var(--surface);
		box-shadow: 0 0.75rem 2rem rgb(17 17 17 / 18%);
	}
	.conflict h1 {
		margin: 0;
		font: 600 1.25rem 'Google Sans Variable', 'Google Sans', sans-serif;
	}
	.conflict p {
		line-height: 1.5;
	}
	.conflict button {
		min-height: 2.5rem;
		border: 1px solid var(--ink);
		padding: 0.4rem 0.7rem;
		background: var(--ink);
		color: var(--surface);
		cursor: pointer;
	}
	@media (max-width: 54rem) {
		.workspace {
			grid-template-columns: minmax(0, 1fr);
		}
		.brand {
			min-width: auto;
		}
		.brand span {
			display: none;
		}
	}
</style>
