<script lang="ts">
	import { tick } from 'svelte';
	import { changeSummary, splitPath, type FilePosition } from './focus';
	import { formatPath, type FileDetail, type FileViewState } from './review';

	let {
		file,
		position,
		viewState,
		onPrevious,
		onNext,
		onToggleViewed,
		onLeave
	}: {
		/** The file on screen, or `null` while none is loaded. */
		file: FileDetail | null;
		/** Where the file sits in the tree order, and its neighbours. */
		position: FilePosition | null;
		viewState: FileViewState;
		onPrevious: () => void;
		onNext: () => void;
		onToggleViewed: () => void;
		onLeave: () => void;
	} = $props();

	const path = $derived(file ? splitPath(formatPath(file.path)) : null);
	/** The path a renamed file had before, or `null` when the path did not change. */
	const oldPath = $derived(
		file?.oldPath && file.oldPath.display !== file.path.display ? formatPath(file.oldPath) : null
	);

	let previousButton = $state<HTMLButtonElement>();
	let nextButton = $state<HTMLButtonElement>();
	let leaveButton = $state<HTMLButtonElement>();

	/** Moves keyboard focus to the Leave focus button, which is always in the bar. */
	export function focusLeave() {
		leaveButton?.focus();
	}

	// Reaching the first or last file disables the arrow that got there. A
	// disabled button drops keyboard focus to the body, so hand it to the other
	// arrow, or to Leave focus when there is only one file. This runs before
	// the DOM update, while the arrow still holds focus.
	$effect.pre(() => {
		const atStart = !position?.previous;
		const atEnd = !position?.next;
		const active = document.activeElement;
		let target: HTMLButtonElement | undefined;
		if (atStart && active === previousButton) target = atEnd ? leaveButton : nextButton;
		else if (atEnd && active === nextButton) target = atStart ? leaveButton : previousButton;
		if (target) void tick().then(() => target.focus());
	});

	/** Asks for the other mark, and leaves the box as it is until the saved mark comes back. */
	function toggleViewed(event: MouseEvent) {
		event.preventDefault();
		onToggleViewed();
	}
</script>

<header class="focus-bar">
	<strong class="brand">mire</strong>
	<button
		bind:this={previousButton}
		class="step"
		aria-label="Previous file"
		title="Previous file ["
		aria-keyshortcuts="["
		disabled={!position?.previous}
		onclick={onPrevious}>
		<span class="i-lucide-chevron-left icon" aria-hidden="true"></span>
	</button>
	{#if file && path}
		<h1 class="path" title={oldPath ? `${formatPath(file.path)}, renamed from ${oldPath}` : formatPath(file.path)}>
			<span class="directory">{path.directory}</span><span class="name">{path.name}</span>
		</h1>
		{#if oldPath}
			<span class="meta renamed" title={oldPath}>from {oldPath}</span>
		{/if}
		<span class="meta">{changeSummary(file)}</span>
	{/if}
	<button
		bind:this={nextButton}
		class="step"
		aria-label="Next file"
		title="Next file ]"
		aria-keyshortcuts="]"
		disabled={!position?.next}
		onclick={onNext}>
		<span class="i-lucide-chevron-right icon" aria-hidden="true"></span>
	</button>
	{#if position}
		<span class="meta position">{position.index + 1} of {position.total}</span>
	{/if}
	<span class="spacer"></span>
	{#if file}
		<label class="viewed">
			<input type="checkbox" checked={viewState === 'viewed'} onclick={toggleViewed} />
			{viewState === 'changed' ? 'Changed since viewed' : 'Viewed'}
		</label>
	{/if}
	<button bind:this={leaveButton} class="leave" aria-keyshortcuts="Escape" onclick={onLeave}>
		Leave focus <kbd aria-hidden="true">Esc</kbd>
	</button>
</header>

<style>
	.focus-bar {
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.5rem 1rem;
		border-bottom: 1px solid var(--line);
		background: var(--surface);
		white-space: nowrap;
	}
	.brand {
		margin-right: 0.25rem;
		color: var(--focus);
		font:
			600 0.95rem 'Google Sans Code Variable',
			'Google Sans Code',
			monospace;
	}
	.step,
	.leave {
		flex: none;
		min-height: 1.85rem;
		border: 1px solid var(--line-strong);
		background: var(--surface);
		color: var(--ink);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.35rem;
	}
	.step {
		width: 1.85rem;
		padding: 0;
	}
	.step .icon {
		width: 1rem;
		height: 1rem;
	}
	.step:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}
	.leave {
		padding: 0.25rem 0.65rem;
		font:
			600 0.72rem 'IBM Plex Sans Variable',
			'IBM Plex Sans',
			sans-serif;
	}
	.leave kbd {
		color: var(--muted);
		font: inherit;
		font-weight: 500;
	}
	.path {
		min-width: 0;
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font:
			400 0.82rem 'Google Sans Code Variable',
			'Google Sans Code',
			monospace;
	}
	.directory {
		color: var(--muted);
	}
	.name {
		font-weight: 650;
	}
	.meta,
	.viewed {
		color: var(--muted);
		font-size: 0.72rem;
	}
	.meta {
		flex: none;
	}
	.meta.renamed {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.spacer {
		flex: 1;
	}
	.viewed {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		cursor: pointer;
	}
	.viewed input {
		width: 0.9rem;
		height: 0.9rem;
		margin: 0;
		accent-color: var(--success);
	}
	@media (hover: hover) {
		.step:not(:disabled):hover,
		.leave:hover {
			background: var(--button-hover);
			border-color: var(--ink);
		}
	}
	@media (max-width: 54rem) {
		.focus-bar {
			gap: 0.5rem;
		}
		.meta,
		.directory,
		.leave kbd {
			display: none;
		}
	}
	@media (max-width: 32rem) {
		.brand {
			display: none;
		}
	}
	@media (pointer: coarse) {
		.step,
		.leave {
			min-height: 2.75rem;
		}
		.step {
			width: 2.75rem;
		}
	}
</style>
