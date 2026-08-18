<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import { formatPath, type FileSummary } from './review';

	type TreeRow =
		| { kind: 'directory'; path: string; name: string; depth: number; ancestors: string[] }
		| { kind: 'file'; file: FileSummary; name: string; depth: number; ancestors: string[] };

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
	const treeRows = $derived(buildTreeRows(visibleFiles));
	let collapsed = new SvelteSet<string>();
	const visibleRows = $derived(
		treeRows.filter((row) => query.trim() || row.ancestors.every((ancestor) => !collapsed.has(ancestor)))
	);

	function toggleDirectory(path: string) {
		if (collapsed.has(path)) collapsed.delete(path);
		else collapsed.add(path);
	}

	function buildTreeRows(changedFiles: FileSummary[]): TreeRow[] {
		const rows: TreeRow[] = [];
		const directories = new SvelteSet<string>();

		for (const file of changedFiles) {
			const parts = file.path.display.split('/');
			for (let index = 1; index < parts.length; index += 1) directories.add(parts.slice(0, index).join('/'));
		}

		for (const path of [...directories].sort((left, right) => left.localeCompare(right))) {
			const parts = path.split('/');
			rows.push({
				kind: 'directory',
				path,
				name: parts.at(-1) ?? path,
				depth: parts.length - 1,
				ancestors: parts.slice(0, -1).map((_, index) => parts.slice(0, index + 1).join('/'))
			});
		}

		for (const file of changedFiles) {
			const parts = file.path.display.split('/');
			rows.push({
				kind: 'file',
				file,
				name: parts.at(-1) ?? file.path.display,
				depth: parts.length - 1,
				ancestors: parts.slice(0, -1).map((_, index) => parts.slice(0, index + 1).join('/'))
			});
		}

		return rows.sort((left, right) => {
			const leftPath = left.kind === 'directory' ? `${left.path}/` : left.file.path.display;
			const rightPath = right.kind === 'directory' ? `${right.path}/` : right.file.path.display;
			return leftPath.localeCompare(rightPath);
		});
	}
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
	<div class="file-list" role="tree" aria-label="Changed file tree">
		{#each visibleRows as row (row.kind === 'directory' ? `directory:${row.path}` : row.file.id)}
			{#if row.kind === 'directory'}
				<button
					class="directory"
					style:--tree-depth={row.depth}
					role="treeitem"
					aria-level={row.depth + 1}
					aria-selected="false"
					aria-expanded={!collapsed.has(row.path)}
					onclick={() => toggleDirectory(row.path)}>
					<span class="i-lucide-chevron-down disclosure" class:collapsed={collapsed.has(row.path)}></span>
					<span class="i-lucide-folder tree-icon"></span>
					<span class="node-name">{row.name}</span>
				</button>
			{:else}
				<button
					class:active={activeFile === row.file.id}
					class="file"
					style:--tree-depth={row.depth}
					role="treeitem"
					aria-level={row.depth + 1}
					aria-selected={activeFile === row.file.id}
					onclick={() => onSelect(row.file.id)}
					aria-current={activeFile === row.file.id ? 'page' : undefined}>
					<span class="i-lucide-file-code-2 tree-icon"></span>
					<span class="path" title={formatPath(row.file.path)}>{row.name}</span>
					<span class="file-meta"
						>{row.file.status} · {row.file.openFindings} open{viewedFileIds.includes(row.file.id)
							? ' · viewed'
							: ''}</span>
				</button>
			{/if}
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
		grid-template-columns: auto minmax(0, 1fr);
		gap: 0.15rem;
		padding: 0.45rem 0.75rem 0.45rem calc(0.75rem + var(--tree-depth) * 0.85rem);
		border: 0;
		border-bottom: 1px solid var(--line);
		background: transparent;
		text-align: left;
		cursor: pointer;
	}
	.directory {
		width: 100%;
		min-height: 1.85rem;
		display: flex;
		align-items: center;
		gap: 0.35rem;
		border: 0;
		padding: 0.25rem 0.75rem 0.25rem calc(0.55rem + var(--tree-depth) * 0.85rem);
		background: transparent;
		color: var(--ink);
		text-align: left;
	}
	.directory:hover {
		background: var(--paper);
	}
	.disclosure {
		width: 0.7rem;
		height: 0.7rem;
		flex: none;
		transition: transform 100ms ease-out;
	}
	.disclosure.collapsed {
		transform: rotate(-90deg);
	}
	.tree-icon {
		width: 0.85rem;
		height: 0.85rem;
		flex: none;
		color: var(--muted);
	}
	.node-name {
		overflow: hidden;
		font-size: 0.74rem;
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.file .tree-icon {
		align-self: center;
		grid-row: 1 / 3;
	}
	.file-meta {
		grid-column: 2;
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
	.file.active .tree-icon {
		color: currentColor;
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
