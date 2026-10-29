<script lang="ts">
	/** The keys focus mode answers to, each with the action it takes. */
	const KEYS: { keys: string[]; action: string }[] = [
		{ keys: ['[', ']'], action: 'file' },
		{ keys: ['j', 'k'], action: 'finding' },
		{ keys: ['b', 'Esc'], action: 'leave focus' }
	];
</script>

<footer class="key-bar" aria-label="Keys">
	<span class="label">Keys:</span>
	<ul>
		{#each KEYS as entry (entry.action)}
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
		padding: 0.45rem 1rem;
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
</style>
