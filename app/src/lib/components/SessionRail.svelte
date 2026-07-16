<script lang="ts">
	import type { LoadState, Session } from '$lib/types';

	let {
		sessions,
		selectedSessionId,
		loadState,
		onSelect,
		onCollapse
	}: {
		sessions: Session[];
		selectedSessionId?: string;
		loadState: LoadState;
		onSelect: (session: Session) => void;
		onCollapse: () => void;
	} = $props();

	function formatDate(value: string) {
		return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', year: 'numeric' }).format(
			new Date(value)
		);
	}
</script>

<aside class="session-rail" aria-label="Review sessions">
	<div class="rail-heading">
		<div>
			<p class="eyebrow">WORKSPACE</p>
			<h1>Sessions</h1>
		</div>
		<div class="rail-heading__actions">
			<span class="count-badge">{loadState === 'loading' ? '—' : sessions.length}</span>
			<button class="collapse-button" type="button" aria-label="Collapse review sessions" onclick={onCollapse}
				>←</button>
		</div>
	</div>

	{#if loadState === 'ready' && sessions.length}
		<nav class="session-list" aria-label="Available sessions">
			{#each sessions as session, index (session.id)}
				<button
					class="session-card"
					class:session-card--active={selectedSessionId === session.id}
					onclick={() => onSelect(session)}>
					<span class="session-card__index">{String(index + 1).padStart(2, '0')}</span>
					<span class="session-card__body">
						<strong>{session.title}</strong>
						<small>{session.repository_name} <span>·</span> {formatDate(session.created_at)}</small>
					</span>
					<span class="session-card__arrow" aria-hidden="true">↗</span>
				</button>
			{/each}
		</nav>
	{:else}
		<div class="rail-empty">
			<p>{loadState === 'loading' ? 'Reading private state…' : 'Start a review from the CLI to see it here.'}</p>
		</div>
	{/if}
</aside>

<style>
	.session-rail {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow-x: hidden;
		overflow-y: auto;
		overscroll-behavior: contain;
		scrollbar-gutter: stable;
		padding: 36px 22px 24px;
		background: var(--panel);
		box-shadow: 12px 0 40px rgb(0 0 0 / 10%);
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
		font-size: 24px;
		font-weight: 580;
		letter-spacing: -0.03em;
	}

	.rail-heading__actions {
		display: flex;
		align-items: center;
		gap: 7px;
	}

	.collapse-button {
		display: grid;
		width: 28px;
		height: 28px;
		place-items: center;
		border: 0;
		border-radius: 6px;
		color: var(--muted);
		background: transparent;
		font: 15px var(--mono);
		cursor: pointer;
		transition:
			color 150ms,
			background-color 150ms;
	}

	.collapse-button:hover {
		color: var(--ink);
		background: var(--panel-hover);
	}

	.eyebrow {
		color: var(--lavender);
		font: 650 11px/1.4 var(--mono);
		letter-spacing: 0.16em;
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
		font: 12px var(--mono);
	}

	.session-list {
		display: grid;
		gap: 8px;
	}

	.session-card {
		display: grid;
		grid-template-columns: 28px minmax(0, 1fr) 20px;
		align-items: start;
		min-height: 82px;
		gap: 10px;
		padding: 16px 14px;
		border: 1px solid transparent;
		border-radius: 10px;
		color: var(--ink);
		background: transparent;
		text-align: left;
		cursor: pointer;
		transition:
			background 160ms ease-out,
			border-color 160ms ease-out,
			translate 160ms ease-out;
	}

	.session-card:hover {
		background: var(--panel-hover);
		border-color: var(--line);
		translate: 2px 0;
	}

	.session-card:active {
		scale: 0.96;
	}

	.session-card--active {
		border-color: rgb(164 140 242 / 58%);
		background: rgb(164 140 242 / 10%);
		box-shadow:
			inset 3px 0 var(--lavender),
			0 8px 24px rgb(0 0 0 / 12%);
	}

	.session-card__index {
		padding-top: 1px;
		color: var(--faint);
		font: 12px/1.45 var(--mono);
		font-variant-numeric: tabular-nums;
	}

	.session-card__body {
		display: grid;
		min-width: 0;
		gap: 6px;
	}

	.session-card__body strong {
		overflow: hidden;
		font-size: 14px;
		font-weight: 620;
		line-height: 1.45;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.session-card__body small {
		overflow: hidden;
		color: var(--muted);
		font-size: 12px;
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
		transition:
			opacity 160ms,
			color 160ms;
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

	@media (max-width: 800px) {
		.session-rail {
			max-height: min(280px, 42dvh);
			padding: 25px 18px 16px;
			border-right: 0;
			box-shadow: 0 12px 40px rgb(0 0 0 / 12%);
		}

		.session-list {
			grid-template-columns: repeat(auto-fit, minmax(205px, 1fr));
		}
	}
</style>
