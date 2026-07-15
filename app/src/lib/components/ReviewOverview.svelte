<script lang="ts">
	import DiffPreview from '$lib/components/DiffPreview.svelte';
	import type { Round, Session } from '$lib/workbench/types';

	let { selectedSession, selectedRound }: { selectedSession: Session | null; selectedRound: Round | null } = $props();

	function shortId(value: string) {
		return value.slice(0, 8);
	}
</script>

<div class="review-header">
	<div>
		<p class="eyebrow">REVIEW WORKBENCH <span>/</span> SESSION {selectedSession ? shortId(selectedSession.id) : '—'}</p>
		<h2>{selectedSession?.title ?? 'No review selected'}</h2>
		<p class="review-header__lede">TODO: review intent, coverage, and round summary</p>
	</div>
	<div class="header-chip"><span class="header-chip__pulse"></span> LOOPBACK ONLY</div>
</div>

<div class="signal-row" aria-label="Review status">
	<div class="signal-card signal-card--accent">
		<span class="signal-card__label">CURRENT ROUND</span>
		<strong>{selectedRound ? `ROUND ${selectedRound.number}` : '—'}</strong>
		<span class="signal-card__detail">{selectedRound?.status ?? 'No captured round'}</span>
	</div>
	<div class="signal-card">
		<span class="signal-card__label">DURABLE STATE</span>
		<strong>TODO: snapshot persistence and repository divergence</strong>
		<span class="signal-card__detail">TODO: durable snapshot and divergence details</span>
	</div>
	<div class="signal-card">
		<span class="signal-card__label">STREAM</span>
		<strong>TODO: operation progress and activity state</strong>
		<span class="signal-card__detail">TODO: latest durable activity event</span>
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
				<strong>TODO: snapshot-bound diff, slices, and finding lanes</strong>
				<p>TODO: selected round diff with anchored findings and evidence</p>
			</div>
		</div>
	</section>
	<section class="surface">
		<div class="surface__heading">
			<div>
				<p class="eyebrow">02 / TODO: capability matrix</p>
				<h3>TODO: review capability state</h3>
			</div>
		</div>
		<p class="surface__todo">
			TODO: availability of diff, findings, evidence, operations, and human actions
		</p>
	</section>
</div>

<style>
	.eyebrow {
		color: var(--lavender);
		font: 650 9px/1.4 var(--mono);
		letter-spacing: 0.16em;
	}

	.eyebrow span {
		color: var(--faint);
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

	.header-chip__pulse {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--green);
		box-shadow: 0 0 0 3px rgb(123 216 143 / 12%);
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
		background: rgb(164 140 242 / 12%);
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

	.surface__todo {
		padding-top: 22px;
		color: var(--muted);
		font: 11px/1.6 var(--mono);
	}

	@media (max-width: 800px) {
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
</style>
