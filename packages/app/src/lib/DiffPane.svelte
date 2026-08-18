<script lang="ts">
	import DiffViewer from './DiffViewer.svelte';
	import FindingEditor from './FindingEditor.svelte';
	import { formatPath, type FindingDetail, type FindingDraft, type FileDetail, type FindingSummary } from './review';

	let {
		file,
		fileError,
		activeFinding,
		findingError,
		onFindingClick,
		onEditFinding,
		onDecideFinding
	}: {
		file: FileDetail | null;
		fileError: string | null;
		activeFinding: FindingDetail | null;
		findingError: string | null;
		onFindingClick: (finding: FindingSummary) => void;
		onEditFinding: (draft: FindingDraft) => Promise<string | null>;
		onDecideFinding: (decision: 'resolve' | 'reopen' | 'dismiss' | 'accept-risk') => Promise<string | null>;
	} = $props();
</script>

<section class="diff-pane" aria-label="File diff">
	{#if fileError}
		<section class="message error">
			<h1>File unavailable</h1>
			<p>{fileError}</p>
		</section>
	{:else if !file}
		<section class="message" aria-live="polite">
			<h1>Loading file</h1>
			<p>Reading the semantic diff.</p>
		</section>
	{:else}
		<header class="diff-heading">
			<div>
				<span class="eyebrow">{file.status}</span>
				<h1>{formatPath(file.path)}</h1>
			</div>
			{#if file.oldPath && file.oldPath.display !== file.path.display}
				<span class="renamed">from {formatPath(file.oldPath)}</span>
			{/if}
		</header>

		{#if activeFinding}
			<article class="finding-detail" aria-live="polite">
				<div class="finding-title">
					<span class="badge">{activeFinding.severity}</span>
					<span>{activeFinding.annotationKind} · {activeFinding.status}</span>
				</div>
				<p>{activeFinding.body}</p>
				<footer>
					{activeFinding.author.displayName ?? activeFinding.author.id} · {activeFinding.provenance} ·
					{activeFinding.anchorState}
				</footer>
			</article>
		{/if}
		{#if activeFinding}
			<FindingEditor finding={activeFinding} onEdit={onEditFinding} onDecision={onDecideFinding} />
		{/if}
		{#if findingError}<p class="finding-error" role="alert">{findingError}</p>{/if}

		{#if file.content.kind === 'binary'}
			<section class="diff-state">
				<h2>Binary file</h2>
				<p>Mire retained the file change but does not expose binary content.</p>
			</section>
		{:else if file.content.hunks.length === 0}
			<section class="diff-state">
				<h2>No text hunks</h2>
				<p>This file changed without a textual diff.</p>
			</section>
		{:else}
			<DiffViewer {file} {onFindingClick} />
		{/if}
	{/if}
</section>

<style>
	.diff-pane {
		min-width: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: clamp(0.85rem, 2vw, 1.25rem);
	}
	.diff-heading {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.75rem;
		margin-bottom: 0.85rem;
	}
	.diff-heading h1 {
		margin: 0.1rem 0 0;
		overflow-wrap: anywhere;
		font:
			600 clamp(1.1rem, 2.5vw, 1.5rem) 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		letter-spacing: -0.035em;
	}
	.eyebrow {
		color: var(--muted);
		font:
			650 0.65rem 'Google Sans Code Variable',
			'Google Sans Code',
			monospace;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}
	.renamed,
	.finding-title,
	.finding-detail footer {
		color: var(--muted);
		font-size: 0.72rem;
	}
	.finding-detail {
		margin-bottom: 0.85rem;
		padding: 0.7rem;
		border: 1px solid var(--line-strong);
		background: var(--paper);
	}
	.finding-title {
		display: flex;
		align-items: center;
		gap: 0.45rem;
	}
	.finding-detail p {
		margin: 0.55rem 0;
		line-height: 1.5;
	}
	.finding-error {
		color: #a4332f;
		font-size: 0.78rem;
	}
	.diff-state {
		display: grid;
		min-height: 16rem;
		place-content: center;
		padding: 2rem;
		border: 1px dashed var(--line-strong);
		text-align: center;
	}
	.diff-state h2 {
		margin: 0;
		font:
			600 1.1rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.diff-state p,
	.message p {
		color: var(--muted);
		margin-top: 0.35rem;
	}
	.badge {
		display: inline-block;
		padding: 0.08rem 0.3rem;
		border: 1px solid currentColor;
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
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
	.message.error h1 {
		color: #a4332f;
	}
</style>
