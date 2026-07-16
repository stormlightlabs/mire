<script lang="ts">
	import ConnectionState from '$lib/components/ConnectionState.svelte';
	import ReviewOverview from '$lib/components/ReviewOverview.svelte';
	import SessionRail from '$lib/components/SessionRail.svelte';
	import WorkbenchTopbar from '$lib/components/WorkbenchTopbar.svelte';
	import { onMount } from 'svelte';
	import type {
		Activity,
		Bootstrap,
		Divergence,
		FindingLane,
		FindingResource,
		LoadState,
		OverviewResource,
		ResourceState,
		ReviewWorkspace,
		Round,
		Session,
		SlicesResource
	} from '$lib/types';
	import { getJSON } from '$lib/api/client';

	let loadState = $state<LoadState>('loading');
	let resourceState = $state<ResourceState>('idle');
	let bootstrap = $state<Bootstrap | null>(null);
	let selectedSession = $state<Session | null>(null);
	let selectedRound = $state<Round | null>(null);
	let rounds = $state<Round[]>([]);
	let workspace = $state<ReviewWorkspace | null>(null);
	let recentActivity = $state<Activity[]>([]);
	let errorMessage = $state('');
	let sessionRailOpen = $state(true);
	let eventSource: EventSource | undefined;
	let resourceController: AbortController | undefined;



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

	async function loadRoundResources(round: Round | null) {
		resourceController?.abort();
		workspace = null;
		if (!round?.snapshot_id) {
			resourceState = 'idle';
			return;
		}
		resourceController = new AbortController();
		const signal = resourceController.signal;
		resourceState = 'loading';
		try {
			const base = `/api/v1/rounds/${encodeURIComponent(round.id)}`;
			const lane = (name: FindingLane) =>
				getJSON<FindingResource>(`${base}/findings?lane=${encodeURIComponent(name)}`, signal);
			const [overview, diff, slices, verified, candidate, refuted, coverage, divergence] = await Promise.all([
				getJSON<OverviewResource>(`${base}/overview`, signal),
				getJSON<ReviewWorkspace['diff']>(`${base}/diff`, signal),
				getJSON<SlicesResource>(`${base}/slices`, signal),
				lane('verified'),
				lane('candidate'),
				lane('refuted'),
				getJSON<ReviewWorkspace['coverage']>(`${base}/coverage`, signal),
				getJSON<{ divergence: Divergence }>(`${base}/divergence`, signal)
			]);
			workspace = {
				overview,
				diff,
				slices,
				findings: { verified, candidate, refuted },
				coverage,
				divergence: divergence.divergence
			};
			resourceState = 'ready';
		} catch (error) {
			if (error instanceof DOMException && error.name === 'AbortError') return;
			errorMessage = error instanceof Error ? error.message : 'Unable to load review data.';
			resourceState = 'error';
		}
	}

	async function loadSession(session: Session) {
		selectedSession = session;
		connectActivity(session.id);
		try {
			const payload = await getJSON<{ session: Session; rounds: Round[] }>(
				`/api/v1/sessions/${encodeURIComponent(session.id)}`
			);
			rounds = payload.rounds;
			selectedRound =
				payload.rounds.find((round) => round.id === payload.session.current_round_id) ?? payload.rounds.at(-1) ?? null;
			await loadRoundResources(selectedRound);
		} catch (error) {
			errorMessage = error instanceof Error ? error.message : 'Unable to load the selected session.';
			resourceState = 'error';
		}
	}

	async function selectRound(round: Round) {
		selectedRound = round;
		await loadRoundResources(round);
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
			loadState = 'ready';
			if (selectedSession) await loadSession(selectedSession);
		} catch (error) {
			if (error instanceof DOMException && error.name === 'AbortError') return;
			errorMessage = error instanceof Error ? error.message : 'Unable to reach the local server.';
			loadState = error instanceof TypeError ? 'offline' : 'error';
		}
	}

	onMount(() => {
		const controller = new AbortController();
		void loadBootstrap(controller.signal);
		return () => {
			controller.abort();
			resourceController?.abort();
			eventSource?.close();
		};
	});
</script>

<svelte:head><title>Mire / review workbench</title></svelte:head>

<div class="workbench-shell">
	<WorkbenchTopbar {loadState} />
	<div class="workspace-grid" class:workspace-grid--rail-closed={!sessionRailOpen}>
		{#if sessionRailOpen}
			<SessionRail
				sessions={bootstrap?.sessions ?? []}
				selectedSessionId={selectedSession?.id}
				{loadState}
				onSelect={(session) => void loadSession(session)}
				onCollapse={() => (sessionRailOpen = false)} />
		{:else}
			<button
				class="session-rail-restore"
				type="button"
				aria-label="Show review sessions"
				onclick={() => (sessionRailOpen = true)}>→</button>
		{/if}
		<main class="review-main">
			{#if loadState === 'ready' || loadState === 'loading'}
				<ReviewOverview
					{selectedSession}
					{selectedRound}
					{rounds}
					{workspace}
					{resourceState}
					{recentActivity}
					{errorMessage}
					onSelectRound={(round) => void selectRound(round)}
					onRetry={() => void loadRoundResources(selectedRound)} />
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
		grid-template-columns: minmax(260px, 300px) minmax(0, 1fr);
		height: calc(100dvh - 58px);
		min-height: 0;
		overflow: hidden;
	}
	.workspace-grid--rail-closed {
		grid-template-columns: 48px minmax(0, 1fr);
	}
	.session-rail-restore {
		display: grid;
		width: 100%;
		height: 48px;
		place-items: center;
		border: 0;
		border-radius: 0 0 8px 0;
		color: var(--muted);
		background: var(--panel);
		font: 15px var(--mono);
		cursor: pointer;
		transition:
			color 150ms,
			background-color 150ms;
	}
	.session-rail-restore:hover {
		color: var(--ink);
		background: var(--panel-hover);
	}
	.review-main {
		width: 100%;
		min-width: 0;
		min-height: 0;
		overflow: auto;
		overscroll-behavior: contain;
		scrollbar-gutter: stable;
	}
	@media (max-width: 800px) {
		.workspace-grid {
			grid-template-columns: 1fr;
			grid-template-rows: auto minmax(0, 1fr);
		}
		.workspace-grid--rail-closed {
			grid-template-rows: 44px minmax(0, 1fr);
		}
		.session-rail-restore {
			width: 48px;
			height: 44px;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		* {
			scroll-behavior: auto !important;
		}
	}
</style>
