<script lang="ts">
	import type { LoadState } from '$lib/types';

	let { loadState, errorMessage, onRetry }: { loadState: LoadState; errorMessage: string; onRetry: () => void } =
		$props();
</script>

{#if loadState === 'unauthenticated'}
	<section class="state-panel">
		<span class="state-panel__number">401</span>
		<h2>Open the one-time launch link.</h2>
		<p>
			This app only runs through an authenticated loopback session. Relaunch <code>mire web</code> and use the printed URL.
		</p>
	</section>
{:else if loadState === 'offline' || loadState === 'error'}
	<section class="state-panel">
		<span class="state-panel__number">!</span>
		<h2>The workbench is waiting for MIRE.</h2>
		<p>{errorMessage || 'Start the foreground web server, then refresh this page.'}</p>
		<button class="button button--primary" onclick={onRetry}>Retry connection <span aria-hidden="true">↗</span></button>
	</section>
{/if}

<style>
	.state-panel {
		max-width: 560px;
		padding-top: 16vh;
	}

	.state-panel__number {
		display: block;
		margin-bottom: 20px;
		color: var(--pink);
		font: 12px var(--mono);
		letter-spacing: 0.12em;
	}

	.state-panel h2 {
		max-width: 480px;
		font-size: clamp(32px, 5vw, 58px);
		font-weight: 430;
		letter-spacing: -0.06em;
		line-height: 0.96;
	}

	.state-panel p {
		margin-top: 22px;
		color: var(--muted);
		font-size: 14px;
		line-height: 1.6;
	}

	.state-panel code {
		color: var(--lavender);
		font-size: 12px;
	}

	.button {
		min-height: 42px;
		margin-top: 24px;
		padding: 0 14px;
		border: 1px solid var(--line-bright);
		border-radius: 5px;
		color: var(--ink);
		background: var(--panel-raised);
		cursor: pointer;
		transition:
			transform 160ms,
			background 160ms,
			border-color 160ms;
	}

	.button:hover {
		border-color: var(--lavender);
		background: var(--panel-hover);
	}

	.button:active {
		transform: scale(0.96);
	}

	.button--primary {
		border-color: rgb(164 140 242 / 54%);
		background: rgb(164 140 242 / 13%);
	}
</style>
