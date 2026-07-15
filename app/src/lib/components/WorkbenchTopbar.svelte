<script lang="ts">
	import { resolve } from '$app/paths';
	import favicon from '$lib/assets/favicon.svg';
	import type { LoadState } from '$lib/workbench/types';

	let { loadState }: { loadState: LoadState } = $props();
</script>

<header class="topbar">
	<a class="wordmark" href={resolve('/')} aria-label="Mire home">
		<img class="wordmark__mark" src={favicon} alt="" aria-hidden="true" /><span>MIRE</span>
	</a>
	<div class="topbar__context">
		<span class="status-dot" class:status-dot--live={loadState === 'ready'} aria-hidden="true"></span>
		<span>{loadState === 'ready' ? 'LOCAL / CONNECTED' : 'LOCAL / ' + loadState.toUpperCase()}</span>
	</div>
	<div class="topbar__meta">EVIDENCE LEDGER <span class="topbar__version">V1</span></div>
</header>

<style>
	.topbar {
		display: flex;
		align-items: center;
		gap: 1.5rem;
		height: 58px;
		padding: 0 28px;
		border-bottom: 1px solid var(--line);
		background: rgb(9 10 18 / 82%);
		backdrop-filter: blur(18px);
		font: 10px var(--mono);
		letter-spacing: 0.12em;
	}

	.wordmark {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		color: var(--ink);
		text-decoration: none;
		font-family: 'Overpass Variable', 'Overpass', sans-serif;
		font-weight: 800;
		letter-spacing: 0.22em;
	}

	.wordmark__mark {
		display: block;
		width: 22px;
		height: 22px;
	}

	.topbar__context {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		color: var(--muted);
	}

	.topbar__meta {
		margin-left: auto;
		color: var(--faint);
	}

	.topbar__version {
		color: var(--lavender);
	}

	.status-dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--faint);
		box-shadow: 0 0 0 3px rgb(93 96 126 / 12%);
	}

	.status-dot--live {
		background: var(--green);
		box-shadow: 0 0 0 3px rgb(123 216 143 / 12%);
	}

	@media (max-width: 800px) {
		.topbar {
			padding: 0 18px;
		}

		.topbar__meta {
			display: none;
		}
	}
</style>
