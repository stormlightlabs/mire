<script lang="ts">
	import { keyBarEntries } from './keys';

	let {
		heldRevision = null,
		narrow = false
	}: {
		/** The revision waiting for the reader, which `u` applies, or `null` when none waits. */
		heldRevision?: number | null;
		/** Whether the screen is narrow, where the layout and wrap keys do nothing and are not listed. */
		narrow?: boolean;
	} = $props();

	const keys = $derived(
		heldRevision === null
			? keyBarEntries(narrow)
			: [{ keys: ['u'], action: `apply revision ${heldRevision}` }, ...keyBarEntries(narrow)]
	);
</script>

<footer class="key-bar" aria-label="Keys">
	<span class="label">Keys:</span>
	<ul>
		{#each keys as entry (entry.action)}
			<li>
				{#each entry.keys as key (key)}<kbd>{key}</kbd>{/each}
				{entry.action}
			</li>
		{/each}
	</ul>
</footer>

<style>
	.key-bar {
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 1.25rem;
		padding: 0.45rem 1rem calc(0.45rem + env(safe-area-inset-bottom));
		border-top: 1px solid var(--line);
		background: var(--surface);
		color: var(--muted);
		font-size: 0.72rem;
		white-space: nowrap;
		overflow: hidden;
	}
	ul {
		display: flex;
		gap: 1.25rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		display: inline-flex;
		align-items: baseline;
		gap: 0.3rem;
	}
	kbd {
		color: var(--ink);
		font:
			600 0.72rem 'Google Sans Code Variable',
			'Google Sans Code',
			monospace;
	}
	/* At phone width the list takes two rows rather than running past the edge. */
	@media (max-width: 32rem) {
		.key-bar {
			align-items: baseline;
			gap: 0.75rem;
		}
		ul {
			flex-wrap: wrap;
			gap: 0.2rem 0.9rem;
		}
	}
</style>
