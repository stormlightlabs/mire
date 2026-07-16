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
	} from '$lib/workbench/types';

	let loadState = $state<LoadState>('loading');
	let resourceState = $state<ResourceState>('idle');
	let bootstrap = $state<Bootstrap | null>(null);
	let selectedSession = $state<Session | null>(null);
	let selectedRound = $state<Round | null>(null);
	let rounds = $state<Round[]>([]);
	let workspace = $state<ReviewWorkspace | null>(null);
	let recentActivity = $state<Activity[]>([]);
	let errorMessage = $state('');
	let eventSource: EventSource | undefined;
	let resourceController: AbortController | undefined;

	async function getJSON<T>(path: string, signal?: AbortSignal): Promise<T> {
		const response = await fetch(path, { signal, headers: { Accept: 'application/json' } });
		if (!response.ok) throw new Error(`${path} returned ${response.status}.`);
		return (await response.json()) as T;
	}

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
	<div class="workspace-grid">
		<SessionRail
			sessions={bootstrap?.sessions ?? []}
			selectedSessionId={selectedSession?.id}
			{loadState}
			onSelect={(session) => void loadSession(session)} />
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
		grid-template-columns: minmax(230px, 270px) minmax(0, 1fr);
		height: calc(100dvh - 58px);
		min-height: 0;
		overflow: hidden;
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
	}
	@media (prefers-reduced-motion: reduce) {
		* {
			scroll-behavior: auto !important;
		}
	}
</style>
