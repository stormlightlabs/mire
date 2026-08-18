<script lang="ts">
	import { formatPath, type FindingFilters, type FindingSummary } from './review';

	let {
		findings,
		activeFindingId,
		openCount,
		filters = $bindable<FindingFilters>(),
		onSelect
	}: {
		findings: FindingSummary[];
		activeFindingId: string | null;
		openCount: number;
		filters: FindingFilters;
		onSelect: (finding: FindingSummary) => void;
	} = $props();
</script>

<aside class="findings" aria-labelledby="findings-heading">
	<div class="pane-heading">
		<h2 id="findings-heading">Review queue</h2>
		<span>{openCount} open</span>
	</div>
	<div class="filters" aria-label="Filter findings">
		<label
			>Status <select bind:value={filters.status}
				><option value="all">All</option><option value="open">Open</option><option value="resolved">Resolved</option
				><option value="dismissed">Dismissed</option><option value="accepted-risk">Accepted risk</option></select
			></label>
		<label
			>Severity <select bind:value={filters.severity}
				><option value="all">All</option><option value="note">Note</option><option value="low">Low</option><option
					value="medium">Medium</option
				><option value="high">High</option><option value="critical">Critical</option></select
			></label>
		<label
			>Intent <select bind:value={filters.annotationKind}
				><option value="all">All</option><option value="comment">Comment</option><option value="defect">Defect</option
				><option value="suggestion">Suggestion</option><option value="question">Question</option></select
			></label>
	</div>
	<p class="queue-help">Press J or K to move through this queue.</p>
	<div class="finding-list">
		{#each findings as finding (finding.id)}
			<button class:current={activeFindingId === finding.id} class="finding" onclick={() => onSelect(finding)}>
				<div><span class="badge">{finding.severity}</span><span class="state">{finding.status}</span></div>
				<p>{finding.body}</p>
				<code
					>{formatPath(finding.path)}:{finding.startLine}{finding.endLine !== finding.startLine
						? `-${finding.endLine}`
						: ''}</code>
				{#if finding.anchorState !== 'captured'}
					<span class="anchor-state">{finding.anchorState}{finding.navigable ? '' : ' — cannot navigate'}</span>
				{/if}
			</button>
		{:else}
			<p class="empty">No findings have been recorded.</p>
		{/each}
	</div>
</aside>

<style>
	.findings {
		min-height: 0;
		overflow: auto;
		border-left: 1px solid var(--line);
		background: var(--surface);
	}
	.pane-heading {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		padding: 0.8rem;
		border-bottom: 1px solid var(--line);
	}
	.pane-heading h2 {
		margin: 0;
		font:
			600 0.875rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.pane-heading > span,
	.empty,
	.state,
	.anchor-state {
		color: var(--muted);
		font-size: 0.75rem;
	}
	.filters {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 0.35rem;
		padding: 0.55rem 0.7rem;
		border-bottom: 1px solid var(--line);
	}
	.filters label {
		display: grid;
		gap: 0.15rem;
		color: var(--muted);
		font-size: 0.65rem;
		font-weight: 600;
	}
	.filters select {
		min-width: 0;
		border: 1px solid var(--line-strong);
		border-radius: 0.2rem;
		padding: 0.2rem;
		background: var(--paper);
		color: var(--ink);
		font: inherit;
	}
	.queue-help {
		margin: 0;
		padding: 0.4rem 0.7rem 0;
		color: var(--muted);
		font-size: 0.68rem;
	}
	.finding {
		width: calc(100% - 1.4rem);
		margin: 0.7rem;
		padding: 0.7rem;
		border: 1px solid var(--line);
		background: var(--surface);
		color: var(--ink);
		text-align: left;
		cursor: pointer;
	}
	.finding:hover,
	.finding.current {
		border-color: var(--ink);
		background: var(--paper);
	}
	.finding p {
		margin: 0.55rem 0;
		font-size: 0.82rem;
		line-height: 1.45;
	}
	.badge {
		display: inline-block;
		padding: 0.1rem 0.3rem;
		border: 1px solid currentColor;
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
	}
	.state {
		margin-left: 0.45rem;
	}
	.anchor-state {
		display: block;
		margin-top: 0.55rem;
	}
	code {
		overflow: hidden;
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.75rem;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.empty {
		padding: 0.8rem;
	}
	@media (max-width: 54rem) {
		.findings {
			order: 3;
			max-height: 15rem;
			border: 0;
			border-top: 1px solid var(--line);
		}
	}
	@media (prefers-reduced-motion: no-preference) {
		.finding {
			transition:
				background-color 120ms ease-out,
				border-color 120ms ease-out;
		}
	}
</style>
