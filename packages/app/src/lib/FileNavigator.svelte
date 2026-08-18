<script lang="ts">
	import { formatPath, type FileSummary } from './review';

	let {
		files,
		activeFile,
		viewedFileIds,
		onSelect
	}: {
		files: FileSummary[];
		activeFile: string | null;
		viewedFileIds: string[];
		onSelect: (id: string) => void;
	} = $props();

	let query = $state('');
	const visibleFiles = $derived(
		files.filter((file) => file.path.display.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
	);
</script>

<aside class="files" aria-label="Changed files">
	<div class="pane-heading">
		<h1>Changed files</h1>
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
		overflow: auto;
		border-right: 1px solid var(--line);
		background: var(--surface);
	}
	.pane-heading {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		padding: 0.8rem;
		border-bottom: 1px solid var(--line);
	}
	.pane-heading h1 {
		margin: 0;
		font:
			600 0.875rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.pane-heading > span,
	.file-meta,
	.empty {
		color: var(--muted);
		font-size: 0.75rem;
	}
	.search {
		display: grid;
		gap: 0.3rem;
		padding: 0.7rem 0.8rem;
		border-bottom: 1px solid var(--line);
		color: var(--muted);
		font-size: 0.72rem;
		font-weight: 600;
	}
	.search input {
		min-width: 0;
		border: 1px solid var(--line-strong);
		border-radius: 0.2rem;
		padding: 0.4rem 0.5rem;
		background: var(--paper);
		color: var(--ink);
	}
	.file {
		width: 100%;
		display: grid;
		gap: 0.2rem;
		padding: 0.7rem 0.8rem;
		border: 0;
		border-bottom: 1px solid var(--line);
		background: transparent;
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
	.path {
		overflow: hidden;
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.75rem;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.empty {
		padding: 0.8rem;
	}
	@media (max-width: 54rem) {
		.files {
			order: 2;
			max-height: 15rem;
			border: 0;
			border-top: 1px solid var(--line);
		}
		.file-list {
			display: flex;
			overflow-x: auto;
		}
		.file {
			min-width: 14rem;
			border-right: 1px solid var(--line);
			border-bottom: 0;
		}
	}
	@media (prefers-reduced-motion: no-preference) {
		.file {
			transition:
				background-color 120ms ease-out,
				border-color 120ms ease-out;
		}
	}
</style>
