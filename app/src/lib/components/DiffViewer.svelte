<script lang="ts">
	import { pathOf, type DiffFile } from '$lib/types';
	import HunkEditor from './HunkEditor.svelte';

	type Props = {
		files: DiffFile[];
		mode: 'unified' | 'split';
		selectedPath?: string;
		selectedHunks?: string[];
		selectedAnchor?: string;
		onSelectAnchor: (hunkID: string) => void;
	};

	let { files, mode, selectedPath, selectedHunks = [], selectedAnchor, onSelectAnchor }: Props = $props();

	const visibleFiles = $derived(
		files
			.filter((file) => !selectedPath || pathOf(file) === selectedPath)
			.map((file) => ({
				...file,
				hunks: (file.hunks ?? []).filter((hunk) => !selectedHunks.length || selectedHunks.includes(hunk.id))
			}))
			.filter((file) => (file.hunks?.length ?? 0) > 0)
	);
	let expandedPaths = $state<string[]>([]);
	let initializedScope = $state('');

	$effect(() => {
		const scope = selectedPath || 'all';
		const firstPath = visibleFiles.at(0) ? pathOf(visibleFiles[0]) : '';
		if (firstPath && initializedScope !== scope) {
			initializedScope = scope;
			if (!expandedPaths.includes(firstPath)) expandedPaths = [...expandedPaths, firstPath];
		}
	});

	function toggleFile(path: string) {
		expandedPaths = expandedPaths.includes(path)
			? expandedPaths.filter((expandedPath) => expandedPath !== path)
			: [...expandedPaths, path];
	}
</script>

<div class="diff" aria-label={`${mode} snapshot diff`}>
	{#if visibleFiles.length === 0}
		<div class="diff__empty">No diff hunks match this navigation view.</div>
	{/if}
	{#each visibleFiles as file (pathOf(file))}
		{@const path = pathOf(file)}
		{@const expanded = expandedPaths.includes(path)}
		<section class="file" aria-labelledby={`file-${pathOf(file).replaceAll(/[^a-zA-Z0-9]/g, '-')}`}>
			<button
				class="file__header"
				aria-expanded={expanded}
				aria-controls={`file-body-${path.replaceAll(/[^a-zA-Z0-9]/g, '-')}`}
				onclick={() => toggleFile(path)}>
				<span class="file__status">{file.status.slice(0, 1).toUpperCase()}</span>
				<h4 id={`file-${path.replaceAll(/[^a-zA-Z0-9]/g, '-')}`}>{path}</h4>
				<span class="file__meta">{file.hunks?.length ?? 0} {(file.hunks?.length ?? 0) === 1 ? 'hunk' : 'hunks'}</span>
				<span class="file__chevron" aria-hidden="true"></span>
			</button>
			{#if expanded}
				<div id={`file-body-${path.replaceAll(/[^a-zA-Z0-9]/g, '-')}`}>
					{#each file.hunks ?? [] as hunk (hunk.id)}
						<article class="hunk" class:hunk--selected={selectedAnchor === hunk.id} id={`anchor-${hunk.id}`}>
							<button
								class="hunk__anchor"
								aria-pressed={selectedAnchor === hunk.id}
								onclick={() => onSelectAnchor(hunk.id)}>
								<span>ANCHOR</span>
								<code>−{hunk.old_start},{hunk.old_lines} / +{hunk.new_start},{hunk.new_lines}</code>
								<small>{hunk.id}</small>
							</button>
							{#if hunk.binary}
								<p class="binary">Binary change — line content is unavailable.</p>
							{:else}
								{#key `${hunk.id}:${mode}:${selectedAnchor === hunk.id}`}
									<HunkEditor {path} {hunk} {mode} selected={selectedAnchor === hunk.id} />
								{/key}
							{/if}
						</article>
					{/each}
				</div>
			{/if}
		</section>
	{/each}
</div>

<style>
	.diff {
		display: grid;
		gap: 16px;
		min-width: 0;
	}

	.diff__empty {
		display: grid;
		min-height: 190px;
		place-items: center;
		color: var(--muted);
		font-size: 13px;
	}

	.file {
		min-width: 0;
		overflow: hidden;
		border-radius: 10px;
		background: #0b0d17;
		box-shadow:
			0 0 0 1px rgb(255 255 255 / 8%),
			0 18px 52px rgb(0 0 0 / 18%);
	}

	.file__header {
		display: grid;
		grid-template-columns: 28px minmax(0, 1fr) auto 24px;
		align-items: center;
		width: 100%;
		min-height: 58px;
		gap: 12px;
		padding: 0 16px;
		border: 0;
		color: var(--ink);
		background: #11131f;
		text-align: left;
		cursor: pointer;
		transition-property: background-color;
		transition-duration: 150ms;
		transition-timing-function: ease-out;
	}

	.file__header:hover {
		background: #161927;
	}

	.file__header:active {
		scale: 0.99;
	}

	.file__header h4 {
		overflow: hidden;
		margin: 0;
		font: 12px/1.4 var(--mono);
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.file__meta {
		color: var(--faint);
		font: 11px/1.4 var(--mono);
		font-variant-numeric: tabular-nums;
	}

	.file__status {
		display: grid;
		width: 21px;
		height: 21px;
		place-items: center;
		border-radius: 4px;
		color: var(--blue);
		background: rgb(108 182 255 / 12%);
		font: 11px var(--mono);
	}

	.file__chevron {
		display: block;
		width: 8px;
		height: 8px;
		justify-self: center;
		border-right: 1.5px solid currentColor;
		border-bottom: 1.5px solid currentColor;
		color: var(--muted);
		rotate: -45deg;
		transition-property: rotate, color;
		transition-duration: 150ms;
		transition-timing-function: ease-out;
	}

	.file__header[aria-expanded='true'] .file__chevron {
		rotate: 45deg;
	}

	.hunk {
		scroll-margin: 72px;
		transition: box-shadow 160ms ease-out;
	}

	.hunk + .hunk {
		border-top: 1px solid var(--line);
	}

	.hunk--selected {
		box-shadow: inset 3px 0 var(--pink);
	}

	.hunk__anchor {
		display: grid;
		grid-template-columns: auto auto minmax(0, 1fr);
		align-items: center;
		width: 100%;
		min-height: 42px;
		gap: 12px;
		padding: 0 14px;
		border: 0;
		color: var(--muted);
		background: rgb(164 140 242 / 6%);
		text-align: left;
		cursor: pointer;
		transition: background-color 150ms ease-out;
	}

	.hunk__anchor:hover {
		background: rgb(164 140 242 / 11%);
	}

	.hunk__anchor:active {
		scale: 0.99;
	}

	.hunk__anchor span {
		color: var(--lavender);
		font: 650 10px var(--mono);
		letter-spacing: 0.13em;
	}

	.hunk__anchor code {
		color: var(--ink);
		font-size: 11px;
	}

	.hunk__anchor small {
		overflow: hidden;
		color: var(--faint);
		font: 10px var(--mono);
		text-align: right;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.binary {
		padding: 24px;
		color: var(--muted);
		font-size: 12px;
	}

	@media (max-width: 680px) {
		.hunk__anchor {
			grid-template-columns: auto 1fr;
		}
		.hunk__anchor small {
			display: none;
		}
	}
</style>
