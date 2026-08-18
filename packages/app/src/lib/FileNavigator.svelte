<script lang="ts">
	import { formatPath, type FileSummary } from './review';

	let {
		files,
		activeFile,
		viewedFileIds,
		drawer = false,
		onSelect
	}: {
		files: FileSummary[];
		activeFile: string | null;
		viewedFileIds: string[];
		drawer?: boolean;
		onSelect: (id: string) => void;
	} = $props();

	let query = $state('');
	const visibleFiles = $derived(
		files.filter((file) => file.path.display.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
	);
</script>

<aside class:drawer class="files" aria-label="Changed files">
	<div class="pane-heading">
		<h2>Changed files</h2>
		<span>{files.length}</span>
	</div>
	<label class="search">
		<span>Filter files</span>
		<input bind:value={query} type="search" placeholder="Filter files" />
	</label>
	<div class="file-list">
		{#each visibleFiles as summary (summary.id)}
			<button
				class:active={activeFile === summary.id}
				class="file"
				onclick={() => onSelect(summary.id)}
				aria-current={activeFile === summary.id ? 'page' : undefined}>
				<span class="path">{formatPath(summary.path)}</span>
				<span class="file-meta"
					>{summary.status} · {summary.contentKind} · {summary.openFindings} open{viewedFileIds.includes(summary.id)
						? ' · viewed'
						: ''}</span>
			</button>
		{:else}
			<p class="empty">No changed files match this filter.</p>
		{/each}
	</div>
</aside>

<style>
	.files {
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		border-right: 1px solid var(--line);
		background: var(--surface);
	}
	.pane-heading {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		padding: 0.65rem 0.75rem;
		border-bottom: 1px solid var(--line);
	}
	.pane-heading h2 {
		margin: 0;
		font:
			600 0.82rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.pane-heading > span,
	.file-meta,
	.empty {
		color: var(--muted);
		font-size: 0.72rem;
	}
	.search {
		display: grid;
		gap: 0.3rem;
		padding: 0.6rem 0.75rem;
		border-bottom: 1px solid var(--line);
		color: var(--muted);
		font-size: 0.68rem;
		font-weight: 600;
	}
	.search input {
		min-width: 0;
		border: 1px solid var(--line-strong);
		border-radius: 0.2rem;
		padding: 0.35rem 0.45rem;
		background: var(--paper);
		color: var(--ink);
		font: inherit;
		transition: border-color 100ms ease-out;
	}
	.search input:focus-visible {
		border-color: var(--focus);
	}
	.file {
		width: 100%;
		display: grid;
		gap: 0.15rem;
		padding: 0.55rem 0.75rem;
		border: 0;
		border-bottom: 1px solid var(--line);
		background: transparent;
		text-align: left;
		cursor: pointer;
	}
	@media (hover: hover) {
		.file:hover {
			background: var(--paper);
		}
	}
	.file.active {
		background: var(--ink);
		color: var(--paper);
	}
	.file.active .file-meta {
		color: color-mix(in srgb, var(--paper) 72%, transparent);
	}
	.path {
		overflow: hidden;
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.72rem;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.empty {
		padding: 0.75rem;
	}
	.files.drawer {
		height: 100%;
		border: 0;
	}
	@media (max-width: 54rem) {
		.files:not(.drawer) {
			order: 2;
			max-height: 12rem;
			border: 0;
			border-top: 1px solid var(--line);
		}
		.files:not(.drawer) .file-list {
			display: flex;
			overflow-x: auto;
		}
		.files:not(.drawer) .file {
			min-width: 13rem;
			border-right: 1px solid var(--line);
			border-bottom: 0;
		}
	}
	@media (pointer: coarse) {
		.file {
			min-height: 2.75rem;
		}
	}
	@media (prefers-reduced-motion: no-preference) {
		.file,
		.search input {
			transition:
				background-color 100ms ease-out,
				border-color 100ms ease-out;
		}
	}
</style>
