<script lang="ts">
	import type { DiffFile, DiffHunk } from '$lib/workbench/types';

	let {
		files,
		mode,
		selectedPath,
		selectedHunks = [],
		selectedAnchor,
		onSelectAnchor
	}: {
		files: DiffFile[];
		mode: 'unified' | 'split';
		selectedPath?: string;
		selectedHunks?: string[];
		selectedAnchor?: string;
		onSelectAnchor: (hunkID: string) => void;
	} = $props();

	const pathOf = (file: DiffFile) => file.target_path || file.base_path || 'unknown';
	const visibleFiles = $derived(
		files
			.filter((file) => !selectedPath || pathOf(file) === selectedPath)
			.map((file) => ({
				...file,
				hunks: (file.hunks ?? []).filter((hunk) => !selectedHunks.length || selectedHunks.includes(hunk.id))
			}))
			.filter((file) => (file.hunks?.length ?? 0) > 0)
	);

	function lineKind(line: string) {
		if (line.startsWith('+')) return 'added';
		if (line.startsWith('-')) return 'removed';
		return 'context';
	}

	function splitRows(hunk: DiffHunk) {
		return (hunk.lines ?? []).map((line) => ({
			left: line.startsWith('+') ? '' : line,
			right: line.startsWith('-') ? '' : line,
			kind: lineKind(line)
		}));
	}
</script>

<div class="diff" aria-label={`${mode} snapshot diff`}>
	{#if visibleFiles.length === 0}
		<div class="diff__empty">No diff hunks match this navigation view.</div>
	{/if}
	{#each visibleFiles as file (pathOf(file))}
		<section class="file" aria-labelledby={`file-${pathOf(file).replaceAll(/[^a-zA-Z0-9]/g, '-')}`}>
			<header class="file__header">
				<span class="file__status">{file.status.slice(0, 1).toUpperCase()}</span>
				<h4 id={`file-${pathOf(file).replaceAll(/[^a-zA-Z0-9]/g, '-')}`}>{pathOf(file)}</h4>
				<span>{file.hunks?.length ?? 0} {(file.hunks?.length ?? 0) === 1 ? 'hunk' : 'hunks'}</span>
			</header>
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
					{:else if mode === 'unified'}
						<div class="code code--unified" aria-label={`Unified diff for ${pathOf(file)}`}>
							{#each hunk.lines ?? [] as line, index (`${hunk.id}-${index}`)}
								<div class="code-line code-line--{lineKind(line)}">
									<span class="code-line__number" aria-hidden="true">{String(index + 1).padStart(3, ' ')}</span>
									<code>{line}</code>
								</div>
							{/each}
						</div>
					{:else}
						<div class="code code--split" aria-label={`Side-by-side diff for ${pathOf(file)}`}>
							<div class="split-labels" aria-hidden="true"><span>BEFORE</span><span>AFTER</span></div>
							{#each splitRows(hunk) as row, index (`${hunk.id}-split-${index}`)}
								<div class="split-row split-row--{row.kind}">
									<code>{row.left}</code><code>{row.right}</code>
								</div>
							{/each}
						</div>
					{/if}
				</article>
			{/each}
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
		grid-template-columns: 24px minmax(0, 1fr) auto;
		align-items: center;
		min-height: 48px;
		gap: 10px;
		padding: 0 15px;
		border-bottom: 1px solid var(--line);
		background: #11131f;
	}
	.file__header h4 {
		overflow: hidden;
		margin: 0;
		font: 11px var(--mono);
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.file__header > span:last-child {
		color: var(--faint);
		font: 9px var(--mono);
	}
	.file__status {
		display: grid;
		width: 21px;
		height: 21px;
		place-items: center;
		border-radius: 4px;
		color: var(--blue);
		background: rgb(108 182 255 / 12%);
		font: 10px var(--mono);
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
		font: 650 8px var(--mono);
		letter-spacing: 0.13em;
	}
	.hunk__anchor code {
		color: var(--ink);
		font-size: 9px;
	}
	.hunk__anchor small {
		overflow: hidden;
		color: var(--faint);
		font: 8px var(--mono);
		text-align: right;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.code {
		overflow-x: auto;
		font: 10px/1.65 var(--mono);
	}
	.code-line {
		display: grid;
		grid-template-columns: 46px minmax(max-content, 1fr);
		min-height: 21px;
	}
	.code-line code {
		padding: 1px 16px 1px 12px;
		white-space: pre;
	}
	.code-line__number {
		padding: 1px 10px;
		color: #50536b;
		background: rgb(0 0 0 / 10%);
		text-align: right;
		user-select: none;
	}
	.code-line--added {
		color: #b7e9bf;
		background: rgb(80 180 105 / 11%);
	}
	.code-line--removed {
		color: #f6b0c8;
		background: rgb(241 108 158 / 11%);
	}
	.split-labels,
	.split-row {
		display: grid;
		grid-template-columns: repeat(2, minmax(max-content, 1fr));
		min-width: 680px;
	}
	.split-labels {
		color: var(--faint);
		background: rgb(0 0 0 / 18%);
		font: 8px var(--mono);
		letter-spacing: 0.12em;
	}
	.split-labels span,
	.split-row code {
		padding: 3px 13px;
	}
	.split-labels > :last-child,
	.split-row > :last-child {
		border-left: 1px solid var(--line);
	}
	.split-row code {
		min-height: 22px;
		white-space: pre;
	}
	.split-row--added > :last-child {
		color: #b7e9bf;
		background: rgb(80 180 105 / 11%);
	}
	.split-row--removed > :first-child {
		color: #f6b0c8;
		background: rgb(241 108 158 / 11%);
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
