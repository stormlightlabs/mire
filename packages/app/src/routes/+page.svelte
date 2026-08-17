<script lang="ts">
	import { onMount } from 'svelte';
	import DiffPane from '$lib/DiffPane.svelte';
	import FileNavigator from '$lib/FileNavigator.svelte';
	import FindingQueue from '$lib/FindingQueue.svelte';
	import type { FileDetail, FindingDetail, FindingSummary, ReviewOverview } from '$lib/review';

	let review = $state<ReviewOverview | null>(null);
	let activeFile = $state<string | null>(null);
	let file = $state<FileDetail | null>(null);
	let activeFinding = $state<FindingDetail | null>(null);
	let error = $state<string | null>(null);
	let fileError = $state<string | null>(null);
	let findingError = $state<string | null>(null);
	let secret = '';

	onMount(() => {
		secret = window.location.hash.slice(1);
		window.history.replaceState(null, '', `${window.location.pathname}${window.location.search}`);
		if (!secret) {
			error = 'This review URL is missing its session secret. Run mire serve again.';
			return;
		}
		void loadReview();
	});

	async function request<T>(path: string): Promise<T> {
		const response = await fetch(path, { headers: { Authorization: `Bearer ${secret}` } });
		if (!response.ok) throw new Error(`The requested review data is unavailable (${response.status}).`);
		return (await response.json()) as T;
	}

	async function loadReview() {
		try {
			review = await request<ReviewOverview>('/api/v1/review');
			if (review.files[0]) await selectFile(review.files[0].id);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The review could not be loaded.';
		}
	}

	async function selectFile(id: string, finding?: FindingSummary) {
		activeFile = id;
		file = null;
		fileError = null;
		try {
			file = await request<FileDetail>(`/api/v1/files/${encodeURIComponent(id)}`);
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
			<FileNavigator files={review.files} {activeFile} onSelect={selectFile} />
			<DiffPane {file} {fileError} {activeFinding} {findingError} onFindingClick={selectFinding} />
			<FindingQueue
				findings={review.findings}
				activeFindingId={activeFinding?.id ?? null}
				openCount={review.totals.open}
				onSelect={selectFinding} />
		{/if}
	</main>
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
