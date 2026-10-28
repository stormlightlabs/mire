<script lang="ts">
	import DiffViewer from './DiffViewer.svelte';
	import FindingEditor from './FindingEditor.svelte';
	import {
		formatPath,
		type FileViewState,
		type FindingDetail,
		type FindingDraft,
		type FileDetail,
		type FindingSummary
	} from './review';
	import type { Theme } from './theme';

	let {
		file,
		fileError,
		fileLoading = false,
		notice = null,
		activeFinding,
		findingError,
		theme,
		viewState,
		viewedError,
		onDismissNotice = () => {},
		onToggleViewed,
		onFindingClick,
		onEditFinding,
		onDecideFinding
	}: {
		file: FileDetail | null;
		fileError: string | null;
		/** Whether a file load is in flight. A reload of the shown file keeps it on screen meanwhile. */
		fileLoading?: boolean;
		/** A short message for the reader, such as the open file leaving the review. */
		notice?: string | null;
		activeFinding: FindingDetail | null;
		findingError: string | null;
		theme: Theme;
		viewState: FileViewState;
		viewedError: string | null;
		onDismissNotice?: () => void;
		onToggleViewed: () => void;
		onFindingClick: (finding: FindingSummary) => void;
		onEditFinding: (draft: FindingDraft) => Promise<string | null>;
		onDecideFinding: (decision: 'resolve' | 'reopen' | 'dismiss' | 'accept-risk') => Promise<string | null>;
	} = $props();

	let diffStyle = $state<'unified' | 'split'>('unified');
	let expandContext = $state(false);
	/** The raw path of the generated file whose diff the reader opened, if any. */
	let openedGenerated = $state<string | null>(null);
	const generatedCollapsed = $derived(file !== null && file.generated && openedGenerated !== file.path.raw);

	// Another file closes the opened generated diff, so returning to it starts collapsed.
	$effect(() => {
		if (file && file.path.raw !== openedGenerated) openedGenerated = null;
	});
</script>

<section class="diff-pane" aria-label="File diff" aria-busy={fileLoading}>
	{#if notice}
		<div class="notice" role="status">
			<p>{notice}</p>
			<button onclick={onDismissNotice}>Dismiss</button>
		</div>
	{/if}
	{#if fileError}
		<section class="message error">
			<h1>File unavailable</h1>
			<p>{fileError}</p>
		</section>
	{:else if !file && fileLoading}
		<section class="message" aria-live="polite">
			<h1>Loading file</h1>
			<p>Reading the semantic diff.</p>
		</section>
	{:else if !file}
		<section class="message" aria-live="polite">
			<h1>No changed files</h1>
			<p>The review has no file diffs to show.</p>
		</section>
	{:else}
		<header class="diff-heading">
			<div>
				<span class="eyebrow">{file.status}{file.generated ? ' · generated' : ''}</span>
				<h1>{formatPath(file.path)}</h1>
			</div>
			{#if file.oldPath && file.oldPath.display !== file.path.display}
				<span class="renamed">from {formatPath(file.oldPath)}</span>
			{/if}
			<button
				class="viewed-toggle"
				class:viewed={viewState === 'viewed'}
				aria-pressed={viewState === 'viewed'}
				onclick={onToggleViewed}>
				<span class={viewState === 'viewed' ? 'i-lucide-square-check' : 'i-lucide-square'}></span>
				{viewState === 'changed' ? 'Changed since viewed' : 'Viewed'}
			</button>
		</header>
		{#if viewedError}<p class="finding-error" role="alert">{viewedError}</p>{/if}

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
			<FindingEditor finding={activeFinding} onEdit={onEditFinding} onDecision={onDecideFinding} />
		{/if}
		{#if findingError}<p class="finding-error" role="alert">{findingError}</p>{/if}

		{#if generatedCollapsed}
			<section class="diff-state">
				<h2>Generated file</h2>
				<p>The repository's attributes mark this file as generated, so its diff starts collapsed.</p>
				<button class="show-diff" onclick={() => (openedGenerated = file.path.raw)}>Show diff</button>
			</section>
		{:else if file.content.kind === 'binary'}
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
			<div class="diff-controls" role="group" aria-label="Diff display">
				<div class="control-group" role="group" aria-label="Diff layout">
					<button aria-pressed={diffStyle === 'unified'} onclick={() => (diffStyle = 'unified')}>Unified</button>
					<button aria-pressed={diffStyle === 'split'} onclick={() => (diffStyle = 'split')}>Split</button>
				</div>
				<button onclick={() => (expandContext = !expandContext)}>
					{expandContext ? 'Collapse context' : 'Expand context'}
				</button>
			</div>
			<p class="diff-help">Show or collapse all available context around the captured changes.</p>
			<DiffViewer {file} {diffStyle} {expandContext} {theme} {onFindingClick} />
		{/if}
	{/if}
</section>

<style>
	.notice {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		margin-bottom: 0.85rem;
		border: 1px solid var(--line-strong);
		padding: 0.5rem 0.75rem;
		background: var(--paper);
		font-size: 0.78rem;
	}
	.notice p {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.notice button {
		flex: none;
		border: 1px solid var(--line-strong);
		padding: 0.2rem 0.55rem;
		background: var(--surface);
		color: var(--ink);
		font: inherit;
		cursor: pointer;
	}
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
	.viewed-toggle {
		display: inline-flex;
		flex-shrink: 0;
		align-items: center;
		gap: 0.35rem;
		min-height: 2rem;
		border: 1px solid var(--line-strong);
		padding: 0.3rem 0.55rem;
		background: var(--surface);
		color: var(--ink);
		font:
			600 0.72rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.viewed-toggle.viewed {
		border-color: var(--ink);
		background: var(--ink);
		color: var(--surface);
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
	.finding-detail footer,
	.diff-help {
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
		color: var(--danger);
		font-size: 0.78rem;
	}
	.diff-controls,
	.control-group {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}
	.diff-controls {
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.35rem;
	}
	.diff-controls button {
		min-height: 2rem;
		border: 1px solid var(--line-strong);
		padding: 0.3rem 0.55rem;
		background: var(--surface);
		color: var(--ink);
		font:
			600 0.72rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.diff-controls button[aria-pressed='true'] {
		border-color: var(--ink);
		background: var(--ink);
		color: var(--surface);
	}
	.diff-help {
		margin: 0 0 0.65rem;
		line-height: 1.4;
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
	.show-diff {
		justify-self: center;
		min-height: 2rem;
		margin-top: 0.75rem;
		border: 1px solid var(--line-strong);
		padding: 0.3rem 0.75rem;
		background: var(--surface);
		color: var(--ink);
		font:
			600 0.72rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		cursor: pointer;
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
			600 clamp(2rem, 5vw, 3.5rem) / 1 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		letter-spacing: -0.04em;
	}
	.message.error h1 {
		color: var(--danger);
	}
	@media (hover: hover) {
		.diff-controls button:hover:not([aria-pressed='true']) {
			background: var(--button-hover);
		}
	}
	@media (pointer: coarse) {
		.diff-controls button {
			min-height: 2.75rem;
		}
	}
	@media (prefers-reduced-motion: no-preference) {
		.diff-controls button {
			transition:
				background-color 100ms ease-out,
				border-color 100ms ease-out;
		}
	}
</style>
