<script lang="ts">
	import { resolve } from '$app/paths';
	import DiffPreview from '$lib/components/DiffPreview.svelte';
	import { onMount } from 'svelte';

	type Session = { id: string; repository_name: string; title: string; created_at: string; current_round_id?: string };
	type Round = { id: string; number: number; status: string; snapshot_id?: string; created_at: string };
	type Activity = { activity_id: number; event_kind: string; message: string; created_at: string };
	type Bootstrap = {
		schema_version: string;
		authenticated: boolean;
		sessions: Session[];
		selected_session: Session | null;
		current_round: Round | null;
		capabilities: { review_data: boolean; actions: boolean; sse: boolean };
	};
	type LoadState = 'loading' | 'ready' | 'unauthenticated' | 'offline' | 'error';

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

	function formatDate(value: string) {
		return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', year: 'numeric' }).format(
			new Date(value)
		);
	}

	function shortId(value: string) {
		return value.slice(0, 8);
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
	<header class="topbar">
		<a class="wordmark" href={resolve('/')} aria-label="Mire home"
			><span class="wordmark__mark" aria-hidden="true">✳</span><span>MIRE</span></a>
		<div class="topbar__context">
			<span class="status-dot" class:status-dot--live={loadState === 'ready'} aria-hidden="true"></span><span
				>{loadState === 'ready' ? 'LOCAL / CONNECTED' : 'LOCAL / ' + loadState.toUpperCase()}</span>
		</div>
		<div class="topbar__meta">EVIDENCE LEDGER <span class="topbar__version">V1</span></div>
	</header>

	<div class="workspace-grid">
		<aside class="session-rail" aria-label="Review sessions">
			<div class="rail-heading">
				<div>
					<p class="eyebrow">WORKSPACE</p>
					<h1>Sessions</h1>
				</div>
				<span class="count-badge">{bootstrap?.sessions.length ?? '—'}</span>
			</div>
			{#if loadState === 'ready' && bootstrap?.sessions.length}
				<nav class="session-list" aria-label="Available sessions">
					{#each bootstrap.sessions as session, index (session.id)}
						<button
							class="session-card"
							class:session-card--active={selectedSession?.id === session.id}
							onclick={() => selectSession(session)}>
							<span class="session-card__index">{String(index + 1).padStart(2, '0')}</span>
							<span class="session-card__body"
								><strong>{session.title}</strong><small
									>{session.repository_name} <span>·</span> {formatDate(session.created_at)}</small
								></span>
							<span class="session-card__arrow" aria-hidden="true">↗</span>
						</button>
					{/each}
				</nav>
			{:else}
				<div class="rail-empty">
					<span class="rail-empty__glyph" aria-hidden="true">⌁</span>
					<p>{loadState === 'loading' ? 'Reading private state…' : 'Start a review from the CLI to see it here.'}</p>
				</div>
			{/if}
			<div class="rail-footer"><span class="rail-footer__line"></span><span>REPOSITORY STATE IS READ-ONLY</span></div>
		</aside>

		<main class="review-main">
			{#if loadState === 'unauthenticated'}
				<section class="state-panel">
					<span class="state-panel__number">401</span>
					<h2>Open the one-time launch link.</h2>
					<p>
						This app only runs through an authenticated loopback session. Relaunch <code>mire web</code> and use the printed
						URL.
					</p>
				</section>
			{:else if loadState === 'offline' || loadState === 'error'}
				<section class="state-panel">
					<span class="state-panel__number">!</span>
					<h2>The workbench is waiting for MIRE.</h2>
					<p>{errorMessage || 'Start the foreground web server, then refresh this page.'}</p>
					<button class="button button--primary" onclick={() => void loadBootstrap()}
						>Retry connection <span aria-hidden="true">↗</span></button>
				</section>
			{:else}
				<div class="review-header">
					<div>
						<p class="eyebrow">
							REVIEW WORKBENCH <span>/</span> SESSION {selectedSession ? shortId(selectedSession.id) : '—'}
						</p>
						<h2>{selectedSession?.title ?? 'No review selected'}</h2>
						<p class="review-header__lede">
							A private, snapshot-bound view of change. Every state transition stays inspectable.
						</p>
					</div>
					<div class="header-chip"><span class="header-chip__pulse"></span> LOOPBACK ONLY</div>
				</div>

				<div class="signal-row" aria-label="Review status">
					<div class="signal-card signal-card--accent">
						<span class="signal-card__label">CURRENT ROUND</span><strong
							>{selectedRound ? `ROUND ${selectedRound.number}` : '—'}</strong
						><span class="signal-card__detail">{selectedRound?.status ?? 'No captured round'}</span>
					</div>
					<div class="signal-card">
						<span class="signal-card__label">DURABLE STATE</span><strong
							>{selectedRound?.snapshot_id ? 'SNAPSHOT READY' : 'CAPTURE PENDING'}</strong
						><span class="signal-card__detail">Source state is never mutated</span>
					</div>
					<div class="signal-card">
						<span class="signal-card__label">STREAM</span><strong
							>{bootstrap?.capabilities.sse ? 'SSE ENABLED' : 'OFFLINE'}</strong
						><span class="signal-card__detail">{recentActivity[0]?.message ?? 'Reconnectable activity timeline'}</span>
					</div>
				</div>

				<div class="dashboard-grid">
					<section class="surface surface--wide">
						<div class="surface__heading">
							<div>
								<p class="eyebrow">01 / ORIENTATION</p>
								<h3>Review surface</h3>
							</div>
							<span class="surface__status">READ-ONLY</span>
						</div>
						<div class="surface__empty">
							<DiffPreview
								value={selectedRound
									? `// Snapshot ${shortId(selectedRound.snapshot_id ?? selectedRound.id)}\n// Unified diff endpoint is the next review surface.`
									: '// Capture a round to initialize the diff surface.'} />
							<div>
								<strong>Diff inspection is next on the surface.</strong>
								<p>
									The authenticated API and durable activity feed are ready. Snapshot-bound diff, slices, and finding
									lanes plug into this frame in V1-22.
								</p>
							</div>
						</div>
					</section>
					<section class="surface">
						<div class="surface__heading">
							<div>
								<p class="eyebrow">02 / CAPABILITIES</p>
								<h3>Available now</h3>
							</div>
						</div>
						<ul class="capability-list">
							<li><span class="capability-list__marker">+</span><span>Authenticated bootstrap</span><b>READY</b></li>
							<li><span class="capability-list__marker">+</span><span>Durable operations</span><b>READY</b></li>
							<li><span class="capability-list__marker">+</span><span>Activity replay via SSE</span><b>READY</b></li>
							<li class="capability-list__muted">
								<span class="capability-list__marker">·</span><span>Findings &amp; evidence</span><b>SOON</b>
							</li>
						</ul>
					</section>
				</div>
			{/if}
		</main>
	</div>
</div>

<style>
	.workbench-shell {
		min-height: 100vh;
	}

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
		display: inline-grid;
		width: 22px;
		height: 22px;
		place-items: center;
		color: var(--lavender);
		font-size: 18px;
		transform: rotate(15deg);
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

	.status-dot,
	.header-chip__pulse {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--faint);
		box-shadow: 0 0 0 3px rgb(93 96 126 / 12%);
	}

	.status-dot--live,
	.header-chip__pulse {
		background: var(--green);
		box-shadow: 0 0 0 3px rgb(123 216 143 / 12%);
	}

	.workspace-grid {
		display: grid;
		grid-template-columns: minmax(240px, 280px) minmax(0, 1fr);
		min-height: calc(100vh - 58px);
	}

	.session-rail {
		display: flex;
		flex-direction: column;
		padding: 42px 22px 24px;
		border-right: 1px solid var(--line);
		background: linear-gradient(180deg, rgb(16 18 30 / 76%), rgb(9 10 18 / 48%));
	}

	.rail-heading {
		display: flex;
		align-items: end;
		justify-content: space-between;
		margin-bottom: 28px;
		padding: 0 8px;
	}

	.rail-heading h1 {
		margin-top: 8px;
		font-size: 19px;
		font-weight: 580;
		letter-spacing: -0.03em;
	}

	.eyebrow {
		color: var(--lavender);
		font: 650 9px/1.4 var(--mono);
		letter-spacing: 0.16em;
	}

	.eyebrow span {
		color: var(--faint);
	}

	.count-badge {
		display: grid;
		min-width: 28px;
		height: 22px;
		padding: 0 6px;
		place-items: center;
		border: 1px solid var(--line-bright);
		border-radius: 5px;
		color: var(--muted);
		font: 10px var(--mono);
	}

	.session-list {
		display: grid;
		gap: 8px;
	}

	.session-card {
		display: grid;
		grid-template-columns: 25px minmax(0, 1fr) 18px;
		align-items: center;
		min-height: 68px;
		gap: 7px;
		padding: 11px 8px;
		border: 1px solid transparent;
		border-radius: 7px;
		color: var(--ink);
		background: transparent;
		text-align: left;
		cursor: pointer;
		transition-property: background, border-color, transform;
		transition-duration: 160ms;
		transition-timing-function: ease-out;
	}

	.session-card:hover {
		background: var(--panel-hover);
		border-color: var(--line);
		transform: translateX(2px);
	}

	.session-card:active {
		transform: scale(0.98);
	}

	.session-card--active {
		border-color: rgb(164 140 242 / 58%);
		background: linear-gradient(100deg, rgb(164 140 242 / 14%), rgb(164 140 242 / 4%));
		box-shadow:
			inset 3px 0 var(--lavender),
			0 8px 24px rgb(0 0 0 / 12%);
	}

	.session-card__index {
		align-self: start;
		padding-top: 2px;
		color: var(--faint);
		font: 10px var(--mono);
	}

	.session-card__body {
		display: grid;
		min-width: 0;
		gap: 6px;
	}

	.session-card__body strong {
		overflow: hidden;
		font-size: 12px;
		font-weight: 620;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.session-card__body small {
		overflow: hidden;
		color: var(--muted);
		font-size: 10px;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.session-card__body small span {
		color: var(--faint);
	}

	.session-card__arrow {
		color: var(--faint);
		font-size: 15px;
		opacity: 0;
		transition-property: opacity, color;
		transition-duration: 160ms;
	}

	.session-card:hover .session-card__arrow,
	.session-card--active .session-card__arrow {
		color: var(--lavender);
		opacity: 1;
	}

	.rail-empty {
		display: grid;
		place-items: center;
		gap: 12px;
		min-height: 180px;
		padding: 20px;
		border: 1px dashed var(--line);
		border-radius: 8px;
		color: var(--muted);
		text-align: center;
	}

	.rail-empty p {
		max-width: 170px;
		font-size: 12px;
		line-height: 1.5;
	}

	.rail-empty__glyph {
		color: var(--lavender);
		font-size: 28px;
	}

	.rail-footer {
		display: flex;
		align-items: center;
		gap: 9px;
		margin-top: auto;
		padding: 16px 8px 0;
		color: var(--faint);
		font: 8px var(--mono);
		letter-spacing: 0.08em;
	}

	.rail-footer__line {
		width: 22px;
		height: 1px;
		background: var(--pink);
	}

	.review-main {
		width: 100%;
		max-width: 1320px;
		padding: clamp(34px, 6vw, 84px) clamp(28px, 6vw, 96px);
	}

	.review-header {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 28px;
		margin-bottom: 48px;
	}

	.review-header h2 {
		max-width: 700px;
		margin-top: 12px;
		color: var(--ink);
		font-size: clamp(29px, 4vw, 52px);
		font-weight: 430;
		letter-spacing: -0.055em;
		line-height: 0.98;
	}

	.review-header__lede {
		max-width: 490px;
		margin-top: 18px;
		color: var(--muted);
		font-size: 14px;
		line-height: 1.55;
	}

	.header-chip {
		display: inline-flex;
		align-items: center;
		gap: 9px;
		margin-top: 5px;
		padding: 9px 11px;
		border: 1px solid var(--line);
		border-radius: 5px;
		color: var(--muted);
		font: 9px var(--mono);
		letter-spacing: 0.08em;
		white-space: nowrap;
	}

	.signal-row {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 1px;
		margin-bottom: 34px;
		background: var(--line);
		box-shadow: var(--shadow);
	}

	.signal-card {
		display: grid;
		min-height: 113px;
		gap: 9px;
		padding: 18px 20px;
		background: var(--panel);
	}

	.signal-card--accent {
		background: linear-gradient(125deg, rgb(164 140 242 / 18%), var(--panel) 68%);
	}

	.signal-card__label {
		color: var(--faint);
		font: 9px var(--mono);
		letter-spacing: 0.1em;
	}

	.signal-card strong {
		color: var(--ink);
		font-size: 17px;
		font-weight: 570;
		letter-spacing: -0.02em;
	}

	.signal-card--accent strong {
		color: var(--lavender);
	}

	.signal-card__detail {
		color: var(--muted);
		font-size: 11px;
	}

	.dashboard-grid {
		display: grid;
		grid-template-columns: minmax(0, 1.28fr) minmax(260px, 0.72fr);
		gap: 14px;
	}

	.surface {
		min-height: 246px;
		padding: 22px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: rgb(16 18 30 / 82%);
		box-shadow: 0 12px 40px rgb(0 0 0 / 12%);
	}

	.surface--wide {
		min-height: 288px;
	}

	.surface__heading {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 20px;
		padding-bottom: 18px;
		border-bottom: 1px solid var(--line);
	}

	.surface h3 {
		margin-top: 8px;
		font-size: 16px;
		font-weight: 560;
		letter-spacing: -0.02em;
	}

	.surface__status {
		color: var(--green);
		font: 9px var(--mono);
		letter-spacing: 0.08em;
	}

	.surface__empty {
		display: flex;
		align-items: center;
		gap: 24px;
		min-height: 190px;
		padding: 15px 8px;
	}

	.surface__empty strong {
		display: block;
		margin-bottom: 8px;
		font-size: 13px;
		font-weight: 580;
	}

	.surface__empty p {
		max-width: 390px;
		color: var(--muted);
		font-size: 12px;
		line-height: 1.55;
	}

	.capability-list {
		display: grid;
		gap: 0;
		padding: 13px 0 0;
		margin: 0;
		list-style: none;
	}

	.capability-list li {
		display: grid;
		grid-template-columns: 22px minmax(0, 1fr) auto;
		align-items: center;
		gap: 7px;
		padding: 10px 0;
		border-bottom: 1px solid rgb(41 43 67 / 72%);
		color: var(--ink);
		font-size: 11px;
	}

	.capability-list li:last-child {
		border-bottom: 0;
	}

	.capability-list__marker {
		color: var(--green);
		font: 13px var(--mono);
	}

	.capability-list b {
		color: var(--green);
		font: 500 9px var(--mono);
		letter-spacing: 0.08em;
	}

	.capability-list__muted,
	.capability-list__muted b,
	.capability-list__muted .capability-list__marker {
		color: var(--faint);
	}

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
		transition-property: transform, background, border-color;
		transition-duration: 160ms;
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

	@media (max-width: 800px) {
		.topbar {
			padding: 0 18px;
		}

		.topbar__meta {
			display: none;
		}

		.workspace-grid {
			grid-template-columns: 1fr;
		}

		.session-rail {
			padding: 25px 18px 16px;
			border-right: 0;
			border-bottom: 1px solid var(--line);
		}

		.session-list {
			grid-template-columns: repeat(auto-fit, minmax(205px, 1fr));
		}

		.rail-footer {
			display: none;
		}

		.review-main {
			padding: 36px 18px 60px;
		}

		.review-header {
			display: block;
			margin-bottom: 34px;
		}

		.header-chip {
			margin-top: 24px;
		}

		.signal-row,
		.dashboard-grid {
			grid-template-columns: 1fr;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		*,
		*::before,
		*::after {
			scroll-behavior: auto !important;
			transition-duration: 0.01ms !important;
			animation-duration: 0.01ms !important;
		}
	}
</style>
