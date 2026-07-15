<script lang="ts">
	import ConnectionState from '$lib/components/ConnectionState.svelte';
	import ReviewOverview from '$lib/components/ReviewOverview.svelte';
	import SessionRail from '$lib/components/SessionRail.svelte';
	import WorkbenchTopbar from '$lib/components/WorkbenchTopbar.svelte';
	import { onMount } from 'svelte';
	import type { Activity, Bootstrap, LoadState, Round, Session } from '$lib/workbench/types';

	let loadState = $state<LoadState>('loading');
	let bootstrap = $state<Bootstrap | null>(null);
	let selectedSession = $state<Session | null>(null);
	let selectedRound = $state<Round | null>(null);
	let recentActivity = $state<Activity[]>([]);
	let errorMessage = $state('');
	let eventSource: EventSource | undefined;

	function connectActivity(sessionID?: string) {
		eventSource?.close();
		recentActivity = [];
		if (!sessionID) return;
		eventSource = new EventSource(`/api/v1/events?sessionId=${encodeURIComponent(sessionID)}`);
		eventSource.addEventListener('activity', (event) => {
			const activity = JSON.parse((event as MessageEvent).data) as Activity;
			recentActivity = [activity, ...recentActivity].slice(0, 4);
		});
	}

	async function loadBootstrap(signal?: AbortSignal) {
		try {
			const response = await fetch('/api/v1/bootstrap', { signal, headers: { Accept: 'application/json' } });
			if (response.status === 401) {
				loadState = 'unauthenticated';
				return;
			}
			if (!response.ok) throw new Error(`Bootstrap returned ${response.status}.`);
			bootstrap = (await response.json()) as Bootstrap;
			selectedSession = bootstrap.selected_session;
			selectedRound = bootstrap.current_round;
			connectActivity(selectedSession?.id);
			loadState = 'ready';
		} catch (error) {
			if (error instanceof DOMException && error.name === 'AbortError') return;
			errorMessage = error instanceof Error ? error.message : 'Unable to reach the local server.';
			loadState = error instanceof TypeError ? 'offline' : 'error';
		}
	}

	function selectSession(session: Session) {
		selectedSession = session;
		selectedRound = null;
		connectActivity(session.id);
	}

	onMount(() => {
		const controller = new AbortController();
		void loadBootstrap(controller.signal);
		return () => {
			controller.abort();
			eventSource?.close();
		};
	});
</script>

<svelte:head>
	<title>Mire / review workbench</title>
</svelte:head>

<div class="workbench-shell">
	<WorkbenchTopbar {loadState} />

	<div class="workspace-grid">
		<SessionRail
			sessions={bootstrap?.sessions ?? []}
			selectedSessionId={selectedSession?.id}
			{loadState}
			onSelect={selectSession} />

		<main class="review-main">
			{#if loadState === 'ready' || loadState === 'loading'}
				<ReviewOverview {selectedSession} {selectedRound} />
			{:else}
				<ConnectionState {loadState} {errorMessage} onRetry={() => void loadBootstrap()} />
			{/if}
		</main>
	</div>
</div>

<style>
	.workbench-shell {
		height: 100dvh;
		overflow: hidden;
	}

	.workspace-grid {
		display: grid;
		grid-template-columns: minmax(240px, 280px) minmax(0, 1fr);
		height: calc(100dvh - 58px);
		min-height: 0;
		overflow: hidden;
	}

	.review-main {
		width: 100%;
		max-width: 1320px;
		min-width: 0;
		min-height: 0;
		overflow-x: hidden;
		overflow-y: auto;
		overscroll-behavior: contain;
		scrollbar-gutter: stable;
		padding: clamp(34px, 6vw, 84px) clamp(28px, 6vw, 96px);
	}

	@media (max-width: 800px) {
		.workspace-grid {
			grid-template-columns: 1fr;
			grid-template-rows: auto minmax(0, 1fr);
		}

		.review-main {
			padding: 36px 18px 60px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		* {
			scroll-behavior: auto !important;
		}
	}
</style>
