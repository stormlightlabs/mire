<script lang="ts">
	import { onMount } from 'svelte';

	type Path = { display: string; lossy: boolean };
	type FileSummary = {
		id: string;
		path: Path;
		status: string;
		openFindings: number;
	};
	type FindingSummary = {
		id: string;
		path: Path;
		severity: string;
		status: string;
		body: string;
	};
	type ReviewOverview = {
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

	let review = $state<ReviewOverview | null>(null);
	let error = $state<string | null>(null);
	let activeFile = $state<string | null>(null);

	onMount(() => {
		const secret = window.location.hash.slice(1);
		window.history.replaceState(null, '', `${window.location.pathname}${window.location.search}`);

		if (!secret) {
			error = 'This review URL is missing its session secret. Run mire serve again.';
			return;
		}

		void loadReview(secret);
	});

	async function loadReview(secret: string) {
		try {
			const response = await fetch('/api/v1/review', {
				headers: { Authorization: `Bearer ${secret}` }
			});
			if (!response.ok) throw new Error(`The review could not be loaded (${response.status}).`);
			review = (await response.json()) as ReviewOverview;
			activeFile = review.files[0]?.id ?? null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The review could not be loaded.';
		}
	}
</script>

<svelte:head>
	<meta name="description" content="A local browser surface for reviewing a Mire review file." />
</svelte:head>

<a class="skip-link" href="#review-overview">Skip to review overview</a>

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

	<main id="review-overview" class="workspace" tabindex="-1">
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
			<aside class="files" aria-label="Changed files">
				<div class="pane-heading">
					<h1>Changed files</h1>
					<span>{review.totals.files}</span>
				</div>
				<div class="file-list">
					{#if review.files.length === 0}
						<p class="empty">This review has no changed files.</p>
					{:else}
						{#each review.files as file (file.id)}
							<button
								class:active={activeFile === file.id}
								class="file"
								onclick={() => (activeFile = file.id)}
								aria-pressed={activeFile === file.id}>
								<span class="path">{file.path.display}{file.path.lossy ? ' �' : ''}</span>
								<span class="file-meta">{file.status} · {file.openFindings} open</span>
							</button>
						{/each}
					{/if}
				</div>
			</aside>

			<section class="overview" aria-labelledby="overview-heading">
				<p class="eyebrow">Review overview</p>
				<h1 id="overview-heading">Read the change before deciding.</h1>
				<p class="lede">
					This local review has {review.totals.files} changed files and {review.totals.findings} findings. File diffs and
					inline findings will appear here next.
				</p>
				<div class="stats" aria-label="Review totals">
					<div><strong>{review.totals.open}</strong><span>open</span></div>
					<div><strong>{review.totals.resolved}</strong><span>resolved</span></div>
					<div>
						<strong>{review.totals.dismissed + review.totals.acceptedRisk}</strong><span>closed</span>
					</div>
				</div>
			</section>

			<aside class="findings" aria-labelledby="findings-heading">
				<div class="pane-heading">
					<h2 id="findings-heading">Review queue</h2>
					<span>{review.totals.open} open</span>
				</div>
				<div class="finding-list">
					{#if review.findings.length === 0}
						<p class="empty">No findings have been recorded.</p>
					{:else}
						{#each review.findings as finding (finding.id)}
							<article class="finding">
								<div>
									<span class="badge">{finding.severity}</span><span class="state">{finding.status}</span>
								</div>
								<p>{finding.body}</p>
								<code>{finding.path.display}{finding.path.lossy ? ' �' : ''}</code>
							</article>
						{/each}
					{/if}
				</div>
			</aside>
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
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		min-width: 12rem;
		font-family: 'Google Sans Variable', 'Google Sans', sans-serif;
	}

	.brand strong {
		font-size: 1.25rem;
		letter-spacing: -0.04em;
	}
	.brand span,
	.review-meta span,
	.status,
	.file-meta,
	.empty,
	.state {
		color: var(--muted);
		font-size: 0.75rem;
	}

	.review-meta {
		display: grid;
		min-width: 0;
		flex: 1;
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
		grid-template-columns: 15.5rem minmax(22rem, 1fr) 19rem;
	}
	.files,
	.findings {
		min-height: 0;
		background: var(--surface);
	}
	.files {
		border-right: 1px solid var(--line);
	}
	.findings {
		border-left: 1px solid var(--line);
	}
	.pane-heading {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		padding: 0.8rem;
		border-bottom: 1px solid var(--line);
	}
	.pane-heading h1,
	.pane-heading h2 {
		margin: 0;
		font:
			600 0.875rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.pane-heading > span {
		color: var(--muted);
		font-size: 0.75rem;
	}
	.file-list,
	.finding-list {
		overflow: auto;
	}
	.file {
		width: 100%;
		display: grid;
		gap: 0.2rem;
		border: 0;
		border-bottom: 1px solid var(--line);
		background: transparent;
		padding: 0.7rem 0.8rem;
		text-align: left;
		cursor: pointer;
	}
	.file:hover {
		background: var(--paper);
	}
	.file.active {
		background: var(--ink);
		color: var(--paper);
	}
	.file.active .file-meta {
		color: color-mix(in srgb, var(--paper) 72%, transparent);
	}
	.path,
	code {
		overflow: hidden;
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.75rem;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.overview {
		display: grid;
		align-content: center;
		max-width: 48rem;
		padding: clamp(2rem, 8vw, 7rem);
	}
	.eyebrow {
		margin: 0 0 0.8rem;
		color: var(--muted);
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.72rem;
		font-weight: 650;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}
	.overview h1,
	.message h1 {
		margin: 0;
		font-family: 'Google Sans Variable', 'Google Sans', sans-serif;
		font-size: clamp(2rem, 5vw, 3.5rem);
		letter-spacing: -0.04em;
		line-height: 1;
	}
	.lede {
		max-width: 40rem;
		color: var(--muted);
		font-size: 1.05rem;
		line-height: 1.55;
	}
	.stats {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		margin-top: 1.5rem;
		border: 1px solid var(--line);
	}
	.stats div {
		display: grid;
		gap: 0.2rem;
		padding: 0.9rem;
		border-right: 1px solid var(--line);
	}
	.stats div:last-child {
		border-right: 0;
	}
	.stats strong {
		font:
			600 1.6rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		letter-spacing: -0.04em;
	}
	.stats span {
		color: var(--muted);
		font-size: 0.75rem;
	}
	.finding {
		margin: 0.7rem;
		padding: 0.7rem;
		border: 1px solid var(--line);
	}
	.finding p {
		margin: 0.55rem 0;
		font-size: 0.82rem;
		line-height: 1.45;
	}
	.badge {
		display: inline-block;
		border: 1px solid currentColor;
		padding: 0.1rem 0.3rem;
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
	}
	.state {
		margin-left: 0.45rem;
	}
	.empty {
		padding: 0.8rem;
	}
	.message {
		align-self: center;
		max-width: 42rem;
		padding: clamp(2rem, 8vw, 7rem);
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
		.files,
		.findings {
			border: 0;
			border-top: 1px solid var(--line);
		}
		.files {
			order: 2;
		}
		.findings {
			order: 3;
		}
		.file-list {
			display: flex;
			overflow-x: auto;
		}
		.file {
			min-width: 12rem;
			border-right: 1px solid var(--line);
			border-bottom: 0;
		}
		.finding-list {
			display: grid;
			grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
		}
		.brand {
			min-width: auto;
		}
		.brand span {
			display: none;
		}
	}

	@media (prefers-reduced-motion: no-preference) {
		.file {
			transition: background-color 120ms ease-out;
		}
	}
</style>
