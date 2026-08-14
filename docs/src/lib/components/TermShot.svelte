<script lang="ts">
	import { asset } from '$app/paths';

	let {
		src,
		command,
		alt,
		loading = 'lazy',
		expand = false
	}: {
		src: string;
		command: string;
		alt: string;
		loading?: 'eager' | 'lazy';
		expand?: boolean;
	} = $props();
</script>

<figure class={['terminal', !expand && 'terminal__compact']}>
	<figcaption>
		<span class="terminal-dots" aria-hidden="true"><i></i><i></i><i></i></span>
		<span class="terminal-command" title={command}>mire · {command}</span>
	</figcaption>
	<img src={asset(src)} width="1600" height="900" {alt} {loading} />
</figure>

<style>
	.terminal {
		overflow: hidden;
		border: 1px solid var(--code-line);
		border-radius: 0.65rem;
		background: var(--code-surface);
		box-shadow: 0 24px 60px rgb(16 43 62 / 18%);
		color: var(--code-ink);
	}

	.terminal__compact {
		max-width: 50%;
	}

	figcaption {
		display: grid;
		grid-template-columns: 4rem minmax(0, 1fr) 4rem;
		align-items: center;
		min-height: 2.75rem;
		margin: 0;
		padding: 0 0.9rem;
		border-bottom: 1px solid var(--code-line);
		color: color-mix(in srgb, var(--code-ink) 62%, transparent);
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.72rem;
		text-align: center;
	}

	.terminal-command {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.terminal-dots {
		display: flex;
		gap: 0.35rem;
	}

	.terminal-dots i {
		width: 0.55rem;
		height: 0.55rem;
		border-radius: 50%;
		background: var(--teal);
	}

	.terminal-dots i:nth-child(2) {
		background: var(--mark-ink);
	}

	.terminal-dots i:nth-child(3) {
		background: var(--navy);
	}

	img {
		display: block;
		width: 100%;
		height: auto;
	}

	@media (max-width: 600px) {
		figcaption {
			grid-template-columns: 3.5rem minmax(0, 1fr) 3.5rem;
			font-size: 0.65rem;
		}
	}
</style>
