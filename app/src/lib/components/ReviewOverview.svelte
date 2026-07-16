<script lang="ts">
	import DiffViewer from '$lib/components/DiffViewer.svelte';
	import type {
		CandidateProjection,
		FindingDetailResource,
		FindingLane,
		FindingProjection,
		LogicalSlice,
		ResourceState,
		ReviewWorkspace,
		Round,
		Run,
		Session
	} from '$lib/types';

	let {
		selectedSession,
		selectedRound,
		rounds = [],
		workspace,
		resourceState = 'idle',
		recentActivity = [],
		errorMessage = '',
		onSelectRound = () => {},
		onRetry = () => {}
	}: {
		selectedSession: Session | null;
		selectedRound: Round | null;
		rounds?: Round[];
		workspace?: ReviewWorkspace | null;
		resourceState?: ResourceState;
		recentActivity?: { message: string }[];
		errorMessage?: string;
		onSelectRound?: (round: Round) => void;
		onRetry?: () => void;
	} = $props();

	type NavMode = 'slices' | 'files';
	type Order = 'risk' | 'relevance' | 'diff' | 'tests' | 'dependency';
	type ReviewItem = { kind: 'finding'; value: FindingProjection } | { kind: 'candidate'; value: CandidateProjection };

	let navMode = $state<NavMode>('slices');
	let order = $state<Order>('risk');
	let diffMode = $state<'unified' | 'split'>('unified');
	let lane = $state<FindingLane>('verified');
	let selectedNavID = $state('all');
	let selectedAnchor = $state('');
	let selectedItemID = $state('');
	let detail = $state<FindingDetailResource | null>(null);
	let detailLoading = $state(false);
	let navigatorOpen = $state(true);
	let ledgerOpen = $state(true);

	const shortID = (value = '') => (value ? value.slice(0, 10) : '—');
	const pathOf = (file: { target_path?: string; base_path?: string }) =>
		file.target_path || file.base_path || 'unknown';
	const allRuns = $derived([
		...(workspace?.overview.provenance.planner_runs ?? []),
		...(workspace?.overview.provenance.review_runs ?? []),
		...(workspace?.overview.provenance.verification_runs ?? [])
	]);
	const modelNames = $derived([
		...new Set(allRuns.map((run) => run.provenance.model || run.provenance.adapter).filter(Boolean))
	]);
	const laneResource = $derived(workspace?.findings[lane]);
	const reviewItems = $derived<ReviewItem[]>([
		...(laneResource?.findings ?? []).map((value) => ({ kind: 'finding' as const, value })),
		...(laneResource?.candidates ?? []).map((value) => ({ kind: 'candidate' as const, value }))
	]);
	const selectedItem = $derived(
		reviewItems.find((item) => itemID(item) === selectedItemID) ?? reviewItems.at(0) ?? null
	);
	const selectedSlice = $derived(
		workspace?.slices.slices.find((slice) => `slice:${slice.id}` === selectedNavID) ?? null
	);
	const selectedPath = $derived(selectedNavID.startsWith('file:') ? selectedNavID.slice(5) : undefined);
	const titledComparison = $derived(
		selectedSession?.title.startsWith('Review ') ? selectedSession.title.slice('Review '.length) : ''
	);

	const navigationItems = $derived.by(() => {
		if (!workspace) return [] as { id: string; title: string; meta: string; score: number; kind: NavMode }[];
		if (navMode === 'files') {
			return workspace.diff.files
				.map((file, index) => ({
					id: `file:${pathOf(file)}`,
					title: pathOf(file),
					meta: `${file.status} · ${file.hunks?.length ?? 0} hunks`,
					score: orderScore(
						order,
						pathOf(file),
						file.hunks?.flatMap((hunk) => hunk.lines ?? []).length ?? 0,
						index,
						file.surfaces ?? []
					),
					kind: 'files' as const
				}))
				.sort((left, right) => right.score - left.score || left.title.localeCompare(right.title));
		}
		return workspace.slices.slices
			.map((slice, index) => ({
				id: `slice:${slice.id}`,
				title: slice.title,
				meta: `${slice.file_paths.length} files · ${slice.hunk_ids.length} hunks`,
				score: sliceScore(order, slice, index),
				kind: 'slices' as const
			}))
			.sort((left, right) => right.score - left.score || left.title.localeCompare(right.title));
	});

	function orderScore(value: Order, path: string, size: number, index: number, surfaces: string[]) {
		switch (value) {
			case 'diff':
				return size;
			case 'tests':
				return /(^|\/)(test|tests|spec)|[_.](test|spec)\./i.test(path) ? 10_000 - index : -index;
			case 'dependency':
				return surfaces.includes('dependencies') || /go\.mod|package\.json|lock/i.test(path) ? 10_000 - index : -index;
			case 'relevance':
				return 10_000 - index;
			default:
				return 10_000 - index;
		}
	}

	function sliceScore(value: Order, slice: LogicalSlice, index: number) {
		const paths = slice.file_paths.join(' ');
		switch (value) {
			case 'diff':
				return slice.hunk_ids.length;
			case 'tests':
				return /test|spec/i.test(paths) ? 10_000 - index : -index;
			case 'dependency':
				return /go\.mod|package\.json|lock|depend/i.test(paths + slice.risk_cues?.join(' ')) ? 10_000 - index : -index;
			case 'relevance':
				return 10_000 - index;
			default:
				return (slice.risk_cues?.length ?? 0) * 100 + (10_000 - index);
		}
	}

	function itemID(item: ReviewItem) {
		return item.kind === 'finding' ? item.value.finding.finding_id : item.value.candidate.id;
	}

	function itemContent(item: ReviewItem) {
		return item.kind === 'finding' ? item.value.finding : item.value.candidate.candidate;
	}

	function selectNavigation(id: string) {
		selectedNavID = id;
		const target = id.startsWith('slice:')
			? workspace?.slices.slices.find((slice) => `slice:${slice.id}` === id)?.hunk_ids.at(0)
			: workspace?.diff.files.find((file) => `file:${pathOf(file)}` === id)?.hunks?.at(0)?.id;
		if (target)
			requestAnimationFrame(() => document.getElementById(`anchor-${target}`)?.focus({ preventScroll: true }));
	}

	async function selectItem(item: ReviewItem) {
		selectedItemID = itemID(item);
		detail = null;
		if (item.kind !== 'finding') return;
		detailLoading = true;
		try {
			const response = await fetch(`/api/v1/findings/${encodeURIComponent(item.value.finding.finding_id)}`, {
				headers: { Accept: 'application/json' }
			});
			if (response.ok) detail = (await response.json()) as FindingDetailResource;
		} finally {
			detailLoading = false;
		}
	}

	function handleFindingKeys(event: KeyboardEvent) {
		if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
		const buttons = [...(event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('.finding-card')];
		const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
		if (index < 0) return;
		event.preventDefault();
		buttons[(index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length]?.focus();
	}

	function selectAnchor(hunkID: string) {
		selectedAnchor = hunkID;
	}

	function runLabel(run: Run) {
		return `${run.role}${run.pass_name ? ` / ${run.pass_name}` : ''}`;
	}
</script>

{#if resourceState === 'loading' || (!selectedSession && resourceState === 'idle')}
	<section class="loading-state" aria-live="polite">
		<span>ASSEMBLING CANONICAL VIEW</span>
		<div class="loading-line"></div>
		<h2>{selectedSession ? 'Reading the frozen review ledger.' : 'Select a review session.'}</h2>
	</section>
{:else if resourceState === 'error'}
	<section class="loading-state">
		<span>REVIEW DATA UNAVAILABLE</span>
		<h2>The canonical review could not be loaded.</h2>
		<p>{errorMessage}</p>
		<button class="button" onclick={onRetry}>Retry review data</button>
	</section>
{:else if workspace && selectedSession && selectedRound}
	<div class="review-shell">
		<header class="review-header">
			<div class="review-header__copy">
				<p class="eyebrow">REVIEW / {selectedSession.repository_name} / {shortID(selectedSession.id)}</p>
				<h2>
					{#if titledComparison}Review <code>{titledComparison}</code>{:else}{selectedSession.title}{/if}
				</h2>
				<p>
					{workspace.overview.intent.prompt ||
						workspace.overview.intent.commit_messages?.at(0)?.message ||
						'No explicit review intent was recorded.'}
				</p>
			</div>
			<div class="round-picker">
				<label for="round">ROUND</label>
				<select
					id="round"
					value={selectedRound.id}
					onchange={(event) => {
						const round = rounds.find((value) => value.id === event.currentTarget.value);
						if (round) onSelectRound(round);
					}}>
					{#each rounds as round (round.id)}<option value={round.id}>#{round.number} · {round.status}</option>{/each}
				</select>
			</div>
		</header>

		<section class="orientation" aria-label="Review orientation">
			<div class="orientation__primary">
				<span class="status-mark" data-status={selectedRound.status}>R{selectedRound.number}</span>
				<div>
					<small>ROUND</small>
					<strong>{selectedRound.status}</strong>
					<p>{workspace.overview.snapshot.requested_comparison || workspace.overview.snapshot.kind}</p>
				</div>
			</div>
			<div>
				<small>SNAPSHOT</small><strong title={workspace.overview.snapshot.manifest_digest}
					>{shortID(workspace.overview.snapshot.manifest_digest)}</strong>
				<p>{workspace.overview.snapshot.kind.replaceAll('_', ' ')}</p>
			</div>
			<div>
				<small>DIVERGENCE</small><strong>{workspace.divergence.status}</strong>
				<p>{workspace.divergence.message}</p>
			</div>
			<div>
				<small>MODELS</small><strong>{modelNames.length || 'None'}</strong>
				<p>{modelNames.join(', ') || 'No model run recorded'}</p>
			</div>
			<div>
				<small>COVERAGE</small><strong>
					{workspace.overview.coverage.examined_hunks.length} / {workspace.diff.files.flatMap(
						(file) => file.hunks ?? []
					).length}
				</strong>
				<p>Examined / changed hunks</p>
			</div>
		</section>

		<div
			class="work-area"
			class:work-area--navigator-collapsed={!navigatorOpen}
			class:work-area--ledger-collapsed={!ledgerOpen}>
			<aside id="change-navigation" class="navigator" aria-label="Change navigation">
				<div class="panel-heading">
					<strong>Changes</strong>
					<button aria-label="Collapse change navigation" onclick={() => (navigatorOpen = false)}>←</button>
				</div>
				<div class="segmented" aria-label="Navigation type">
					<button
						class:active={navMode === 'slices'}
						onclick={() => {
							navMode = 'slices';
							selectedNavID = 'all';
						}}>Slices</button>
					<button
						class:active={navMode === 'files'}
						onclick={() => {
							navMode = 'files';
							selectedNavID = 'all';
						}}>Files</button>
				</div>
				<label class="order"
					><span>ORDER BY</span><select bind:value={order}
						><option value="risk">Recorded risk</option><option value="relevance">Intent relevance</option><option
							value="diff">Diff size</option
						><option value="tests">Tests first</option><option value="dependency">Dependency impact</option></select
					></label>
				<p class="ordering-note">{workspace.slices.ordering_rationale} Navigation order is not proof of coverage.</p>
				<nav class="nav-list">
					<button class="nav-item" class:active={selectedNavID === 'all'} onclick={() => selectNavigation('all')}
						><span>ALL CHANGES</span><small>{workspace.diff.files.length} files</small></button>
					{#each navigationItems as item, index (item.id)}
						<button class="nav-item" class:active={selectedNavID === item.id} onclick={() => selectNavigation(item.id)}>
							<i>{String(index + 1).padStart(2, '0')}</i><span>{item.title}</span><small>{item.meta}</small>
						</button>
					{/each}
				</nav>
			</aside>

			<section class="review-surface" aria-label="Snapshot diff and findings">
				<div class="surface-toolbar">
					<div class="surface-toolbar__title">
						{#if !navigatorOpen}<button
								class="panel-restore"
								aria-controls="change-navigation"
								onclick={() => (navigatorOpen = true)}>Show changes</button
							>{/if}
						<div>
							<p class="eyebrow">FROZEN CHANGE</p>
							<strong>{selectedSlice?.title || selectedPath || 'Complete snapshot diff'}</strong>
						</div>
					</div>
					<div class="surface-toolbar__actions">
						<div class="segmented" aria-label="Diff mode">
							<button class:active={diffMode === 'unified'} onclick={() => (diffMode = 'unified')}>Unified</button
							><button class:active={diffMode === 'split'} onclick={() => (diffMode = 'split')}>Side by side</button>
						</div>
						{#if !ledgerOpen}<button
								class="panel-restore"
								aria-controls="finding-ledger"
								onclick={() => (ledgerOpen = true)}>Show findings</button
							>{/if}
					</div>
				</div>
				<DiffViewer
					files={workspace.diff.files}
					mode={diffMode}
					{selectedPath}
					selectedHunks={selectedSlice?.hunk_ids ?? []}
					{selectedAnchor}
					onSelectAnchor={selectAnchor} />
			</section>

			<aside id="finding-ledger" class="ledger" aria-label="Finding ledger">
				<div class="panel-heading">
					<strong>Findings</strong>
					<button aria-label="Collapse finding ledger" onclick={() => (ledgerOpen = false)}>→</button>
				</div>
				<div class="lane-tabs" role="tablist" aria-label="Finding lanes">
					{#each [['verified', '✓', 'Verified'], ['candidate', '?', 'Candidates'], ['refuted', '×', 'Refuted']] as tab (tab[0])}
						<button
							role="tab"
							aria-selected={lane === tab[0]}
							onclick={() => {
								lane = tab[0] as FindingLane;
								selectedItemID = '';
								detail = null;
							}}>
							<span aria-hidden="true">{tab[1]}</span>{tab[2]}<small>
								{workspace.findings[tab[0] as FindingLane].findings.length +
									workspace.findings[tab[0] as FindingLane].candidates.length}</small>
						</button>
					{/each}
				</div>
				<div class="finding-list" role="tabpanel" tabindex="0" onkeydown={handleFindingKeys}>
					{#if reviewItems.length === 0}<p class="empty-lane">No {lane} findings are recorded for this round.</p>{/if}
					{#each reviewItems as item (itemID(item))}
						<button
							class="finding-card finding-card--{lane}"
							class:active={selectedItem && itemID(selectedItem) === itemID(item)}
							onclick={() => void selectItem(item)}>
							<span class="finding-card__cue"
								>{lane === 'verified' ? 'VERIFIED' : lane === 'candidate' ? 'UNVERIFIED' : 'REFUTED'}</span>
							<strong>{itemContent(item).claim}</strong>
							<small>{itemContent(item).severity} · {itemContent(item).category}</small>
						</button>
					{/each}
				</div>
				{#if selectedItem}
					{@const content = itemContent(selectedItem)}
					{@const verification = selectedItem.value.verification}
					<section class="finding-detail" aria-live="polite">
						<div class="detail-heading">
							<span>{lane.toUpperCase()} / {content.severity.toUpperCase()}</span><strong>{content.claim}</strong>
						</div>
						<dl>
							<div>
								<dt>Impact</dt>
								<dd>{content.impact}</dd>
							</div>
							{#if 'invariant' in content && content.invariant}<div>
									<dt>Invariant</dt>
									<dd>{content.invariant}</dd>
								</div>{/if}
						</dl>
						<div class="anchor-list">
							<h4>Anchors</h4>
							{#each content.anchors as anchor (`${anchor.path}:${anchor.hunk_id}`)}<button
									onclick={() => {
										selectedAnchor = anchor.hunk_id;
										document
											.getElementById(`anchor-${anchor.hunk_id}`)
											?.scrollIntoView({ behavior: 'smooth', block: 'center' });
									}}>{anchor.path}<small>{anchor.hunk_id}</small></button
								>{/each}
						</div>
						{#if verification}
							<div class="evidence">
								<h4>Evidence</h4>
								{#each verification.evidence ?? [] as evidence (evidence.id)}<article data-relation={evidence.relation}>
										<span>{evidence.relation}</span>
										<p>{evidence.summary}</p>
										<small
											>{evidence.independent ? 'Independent' : 'Origin-linked'} · {evidence.concrete
												? 'Concrete'
												: 'Contextual'}</small>
									</article>{/each}{#if !verification.evidence?.length}<p class="muted">
										No normalized evidence records.
									</p>{/if}
							</div>
							<div class="refutation">
								<h4>Refutation attempt</h4>
								<p>{verification.refutation_attempt || 'No refutation narrative retained.'}</p>
							</div>
						{/if}
						{#if selectedItem.kind === 'finding' && selectedItem.value.finding.relationships?.length}<div
								class="relationships">
								<h4>Relationships</h4>
								{#each selectedItem.value.finding.relationships as relation (`${relation.kind}:${relation.finding_id}:${relation.revision}`)}<p>
										{relation.kind.replaceAll('_', ' ')} → {shortID(relation.finding_id)} r{relation.revision}
									</p>{/each}
							</div>{/if}
						{#if detail}<div class="retrieved-context">
								<h4>Retrieved context</h4>
								{#if detail.artifacts.length}{#each detail.artifacts as artifact (artifact.id)}<p>
											<span>{artifact.path || artifact.kind}</span><code>{artifact.id}</code><small
												>{artifact.excluded
													? `Excluded — ${artifact.exclusion_reason || 'policy'}`
													: artifact.truncated
														? 'Truncated within the recorded budget'
														: 'Snapshot-bound artifact'}</small>
										</p>{/each}{:else}<p class="muted">No retrieved context artifacts were recorded.</p>{/if}
							</div>
							<div class="analyzer-limitations">
								<h4>Analyzer limitations</h4>
								{#if detail.coverage.analyzers?.length}{#each detail.coverage.analyzers as analyzer (analyzer.name)}<p>
											<strong>{analyzer.name}</strong><span
												>{analyzer.available
													? `Available ${analyzer.version || ''}`
													: analyzer.reason || 'Unavailable'}</span>
										</p>{/each}{:else}<p class="muted">No analyzer evidence was recorded for this round.</p>{/if}
							</div>{/if}
						<div class="provenance">
							<h4>Run provenance</h4>
							{#each allRuns.filter((run) => selectedItem?.kind !== 'finding' || run.id === selectedItem.value.finding.verification_run_id || run.id === selectedItem.value.finding.origin.review_run_id) as run (run.id)}<p>
									<span>{runLabel(run)}</span><code
										>{run.provenance.adapter} / {run.provenance.model || run.provenance.protocol}</code>
								</p>{/each}{#if detailLoading}<small>Loading full provenance…</small>{:else if detail}<small
									>Canonical finding detail refreshed from the API.</small
								>{/if}
						</div>
					</section>
				{/if}
			</aside>
		</div>

		<section class="audit-grid" aria-label="Coverage, omissions, and analyzers">
			<div>
				<p class="eyebrow">COVERAGE</p>
				<h3>What was actually examined</h3>
				<p>
					{workspace.overview.coverage.examined_files.length} files and {workspace.overview.coverage.examined_hunks
						.length} hunks. {workspace.overview.coverage.gaps?.length ?? 0} declared gaps.
				</p>
			</div>
			<div>
				<p class="eyebrow">ANALYZERS</p>
				{#if workspace.overview.coverage.analyzers?.length}{#each workspace.overview.coverage.analyzers as analyzer (analyzer.name)}<p
							class="audit-row">
							<strong>{analyzer.name}</strong><span
								>{analyzer.available
									? `Available ${analyzer.version ?? ''}`
									: `Unavailable — ${analyzer.reason ?? 'not configured'}`}</span>
						</p>{/each}{:else}<p>No analyzer provenance was recorded.</p>{/if}
			</div>
			<div>
				<p class="eyebrow">OMISSIONS</p>
				{#if workspace.overview.omissions.length}{#each workspace.overview.omissions as omission (`${omission.kind}:${omission.reason}`)}<p
							class="audit-row">
							<strong>{omission.kind}</strong><span>{omission.reason}</span>
						</p>{/each}{:else}<p>No projection omissions were recorded.</p>{/if}
			</div>
			<div>
				<p class="eyebrow">LATEST ACTIVITY</p>
				<p>{recentActivity.at(0)?.message || 'Canonical state loaded. Live activity is quiet.'}</p>
			</div>
		</section>
	</div>
{/if}

<style>
	:global(.review-main) {
		background: radial-gradient(circle at 72% -10%, rgb(164 140 242 / 8%), transparent 30%), var(--void);
	}
	.review-shell {
		min-width: 0;
		padding: 36px clamp(20px, 3vw, 52px) 64px;
	}
	.eyebrow {
		color: var(--lavender);
		font: 650 11px/1.4 var(--mono);
		letter-spacing: 0.125em;
	}
	.review-header {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 28px;
		max-width: 1600px;
		margin: 0 auto 34px;
	}
	.review-header__copy {
		max-width: 760px;
	}
	.review-header h2 {
		margin-top: 10px;
		font-size: clamp(30px, 4vw, 50px);
		font-weight: 430;
		letter-spacing: -0.055em;
		line-height: 0.98;
	}
	.review-header h2 code {
		font-family: var(--mono);
		font-size: 0.78em;
		font-weight: 520;
		letter-spacing: -0.045em;
	}
	.review-header__copy > p:last-child {
		max-width: 690px;
		margin-top: 15px;
		color: var(--muted);
		font-size: 15px;
		line-height: 1.55;
	}
	.round-picker {
		display: grid;
		min-width: 145px;
		gap: 7px;
	}
	.round-picker label,
	.order span {
		color: var(--faint);
		font: 10px var(--mono);
		letter-spacing: 0.12em;
	}
	select {
		min-height: 40px;
		padding: 0 42px 0 11px;
		border: 1px solid var(--line-bright);
		border-radius: 5px;
		color: var(--ink);
		background-color: var(--panel);
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='8' viewBox='0 0 12 8'%3E%3Cpath d='m1 1.5 5 5 5-5' fill='none' stroke='%238c8fac' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 14px center;
		appearance: none;
		font: 12px var(--mono);
		cursor: pointer;
	}
	.orientation {
		display: grid;
		grid-template-columns: 1.15fr repeat(4, minmax(130px, 1fr));
		max-width: 1600px;
		margin: 0 auto 18px;
		gap: 10px;
	}
	.orientation > div {
		display: grid;
		align-content: center;
		min-width: 0;
		min-height: 104px;
		gap: 6px;
		padding: 18px 20px;
		border-radius: 10px;
		background: var(--panel);
		box-shadow: var(--shadow-border);
	}
	.orientation__primary {
		grid-template-columns: 48px minmax(0, 1fr);
		align-items: center;
		background: rgb(164 140 242 / 11%) !important;
	}
	.status-mark {
		display: grid;
		width: 46px;
		height: 46px;
		place-items: center;
		border-radius: 50%;
		box-shadow: 0 0 0 1px rgb(164 140 242 / 50%);
		color: var(--lavender);
		font: 12px var(--mono);
		font-variant-numeric: tabular-nums;
	}
	.orientation small {
		color: var(--faint);
		font: 10px var(--mono);
		letter-spacing: 0.1em;
	}
	.orientation strong {
		overflow: hidden;
		font-size: 15px;
		font-weight: 620;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.orientation p {
		overflow: hidden;
		color: var(--muted);
		font-size: 12px;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.orientation__primary p {
		font-family: var(--mono);
	}
	.work-area {
		display: grid;
		grid-template-areas: 'navigator surface ledger';
		grid-template-columns: 270px minmax(520px, 1fr) minmax(340px, 390px);
		align-items: start;
		max-width: 1600px;
		margin: 0 auto;
		gap: 12px;
	}
	.work-area--navigator-collapsed {
		grid-template-areas: 'surface ledger';
		grid-template-columns: minmax(520px, 1fr) minmax(340px, 390px);
	}
	.work-area--ledger-collapsed {
		grid-template-areas: 'navigator surface';
		grid-template-columns: 270px minmax(520px, 1fr);
	}
	.work-area--navigator-collapsed.work-area--ledger-collapsed {
		grid-template-areas: 'surface';
		grid-template-columns: minmax(0, 1fr);
	}
	.work-area--navigator-collapsed .navigator,
	.work-area--ledger-collapsed .ledger {
		display: none;
	}
	.navigator,
	.ledger {
		min-width: 0;
		padding: 18px;
		border-radius: 10px;
		background: var(--panel);
		box-shadow: var(--shadow-border);
	}
	.navigator {
		grid-area: navigator;
	}
	.ledger {
		grid-area: ledger;
	}
	.panel-heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		min-height: 40px;
		margin-bottom: 12px;
	}
	.panel-heading > strong {
		font-size: 14px;
		font-weight: 650;
	}
	.panel-heading button,
	.panel-restore {
		min-width: 40px;
		min-height: 40px;
		border: 0;
		border-radius: 7px;
		color: var(--muted);
		background: var(--panel-hover);
		cursor: pointer;
		transition-property: color, background-color, scale;
		transition-duration: 150ms;
		transition-timing-function: ease-out;
	}
	.panel-heading button:hover,
	.panel-restore:hover {
		color: var(--ink);
		background: rgb(164 140 242 / 14%);
	}

	.panel-heading button:active,
	.panel-restore:active {
		scale: 0.96;
	}

	.panel-restore {
		width: auto;
		padding: 0 12px;
		font-size: 12px;
	}

	.segmented {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 1fr;
		gap: 3px;
		padding: 3px;
		border-radius: 7px;
		background: #090b14;
		box-shadow: inset 0 0 0 1px rgb(255 255 255 / 6%);
	}

	.segmented button {
		min-height: 42px;
		padding: 0 9px;
		border: 0;
		border-radius: 4px;
		color: var(--muted);
		background: transparent;
		font-size: 12px;
		cursor: pointer;
		transition:
			color 150ms,
			background-color 150ms,
			box-shadow 150ms;
	}

	.segmented button.active {
		color: var(--ink);
		background: var(--panel-hover);
		box-shadow: 0 2px 8px rgb(0 0 0 / 24%);
	}

	.segmented button:active,
	.button:active {
		scale: 0.96;
	}

	.order {
		display: grid;
		gap: 7px;
		margin-top: 17px;
	}

	.order select {
		width: 100%;
	}

	.ordering-note {
		margin: 12px 2px 15px;
		color: var(--faint);
		font-size: 11px;
		line-height: 1.6;
	}

	.nav-list {
		display: grid;
		gap: 4px;
		max-height: 640px;
		overflow: auto;
	}

	.nav-item {
		display: grid;
		grid-template-columns: 21px minmax(0, 1fr);
		min-height: 64px;
		gap: 3px 7px;
		padding: 9px;
		border: 0;
		border-radius: 6px;
		color: var(--muted);
		background: transparent;
		text-align: left;
		cursor: pointer;
		transition:
			color 150ms,
			background-color 150ms,
			transform 150ms;
	}
	.nav-item:first-child {
		grid-template-columns: 1fr;
	}
	.nav-item:hover {
		color: var(--ink);
		background: var(--panel-hover);
		transform: translateX(2px);
	}
	.nav-item.active {
		color: var(--ink);
		background: rgb(164 140 242 / 13%);
	}
	.nav-item i {
		grid-row: 1 / 3;
		color: var(--faint);
		font: normal 10px var(--mono);
		font-variant-numeric: tabular-nums;
	}
	.nav-item span {
		overflow: hidden;
		font-size: 12px;
		font-weight: 610;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.nav-item small {
		color: var(--faint);
		font: 10px var(--mono);
	}
	.review-surface {
		grid-area: surface;
		min-width: 0;
		padding: 20px;
		border-radius: 10px;
		background: #0e101a;
		box-shadow: var(--shadow-border);
	}
	.surface-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		min-height: 52px;
		margin-bottom: 13px;
	}
	.surface-toolbar strong {
		display: block;
		max-width: 480px;
		margin-top: 5px;
		overflow: hidden;
		font-size: 14px;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.surface-toolbar .segmented {
		width: 220px;
	}
	.surface-toolbar__title,
	.surface-toolbar__actions {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.lane-tabs {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 2px;
		padding: 3px;
		border-radius: 8px;
		background: #090b14;
		box-shadow: inset 0 0 0 1px rgb(255 255 255 / 5%);
	}
	.lane-tabs button {
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 0;
		min-height: 46px;
		gap: 6px;
		padding: 0 7px;
		border: 0;
		border-radius: 5px;
		color: var(--faint);
		background: transparent;
		font-size: 11px;
		white-space: nowrap;
		cursor: pointer;
	}
	.lane-tabs button span {
		font: 11px var(--mono);
	}
	.lane-tabs button small {
		display: grid;
		min-width: 17px;
		height: 17px;
		place-items: center;
		border-radius: 4px;
		background: rgb(255 255 255 / 5%);
		font: 10px var(--mono);
		font-variant-numeric: tabular-nums;
	}
	.lane-tabs button[aria-selected='true'] {
		color: var(--ink);
		background: var(--panel-hover);
		box-shadow: 0 2px 10px rgb(0 0 0 / 24%);
	}
	.finding-list {
		display: grid;
		gap: 6px;
		max-height: 315px;
		overflow: auto;
		padding: 12px 1px 2px;
	}
	.finding-card {
		display: grid;
		min-height: 88px;
		gap: 8px;
		padding: 12px;
		border: 0;
		border-radius: 7px;
		color: var(--ink);
		background: #121521;
		box-shadow: 0 0 0 1px rgb(255 255 255 / 6%);
		text-align: left;
		cursor: pointer;
		transition:
			box-shadow 150ms,
			transform 150ms,
			background-color 150ms;
	}
	.finding-card:hover {
		transform: translateY(-1px);
		box-shadow:
			0 0 0 1px rgb(255 255 255 / 12%),
			0 7px 18px rgb(0 0 0 / 16%);
	}
	.finding-card.active {
		background: rgb(164 140 242 / 10%);
		box-shadow:
			inset 3px 0 var(--lavender),
			0 0 0 1px rgb(164 140 242 / 26%);
	}
	.finding-card--candidate.active {
		box-shadow:
			inset 3px 0 var(--blue),
			0 0 0 1px rgb(108 182 255 / 24%);
	}
	.finding-card--refuted.active {
		box-shadow:
			inset 3px 0 var(--pink),
			0 0 0 1px rgb(241 108 158 / 24%);
	}
	.finding-card__cue {
		color: var(--green);
		font: 10px var(--mono);
		letter-spacing: 0.1em;
	}
	.finding-card--candidate .finding-card__cue {
		color: var(--blue);
	}
	.finding-card--refuted .finding-card__cue {
		color: var(--pink);
		text-decoration: line-through;
	}
	.finding-card strong {
		font-size: 13px;
		line-height: 1.45;
	}
	.finding-card small {
		color: var(--faint);
		font: 10px var(--mono);
		text-transform: uppercase;
	}
	.empty-lane,
	.muted {
		padding: 18px 10px;
		color: var(--muted);
		font-size: 13px;
		line-height: 1.5;
	}
	.finding-detail {
		display: grid;
		gap: 17px;
		padding-top: 16px;
		box-shadow: 0 -1px rgb(255 255 255 / 6%);
	}
	.detail-heading {
		display: grid;
		gap: 7px;
	}
	.detail-heading span,
	.finding-detail h4 {
		margin: 0;
		color: var(--lavender);
		font: 10px var(--mono);
		letter-spacing: 0.11em;
		text-transform: uppercase;
	}
	.detail-heading strong {
		font-size: 16px;
		line-height: 1.35;
	}
	dl,
	dd {
		margin: 0;
	}
	dl {
		display: grid;
		gap: 12px;
	}
	dt {
		margin-bottom: 4px;
		color: var(--faint);
		font: 10px var(--mono);
		text-transform: uppercase;
	}
	dd,
	.refutation p,
	.relationships p {
		color: var(--muted);
		font-size: 12px;
		line-height: 1.6;
	}
	.anchor-list,
	.evidence,
	.refutation,
	.relationships,
	.retrieved-context,
	.analyzer-limitations,
	.provenance {
		display: grid;
		gap: 8px;
	}
	.anchor-list button {
		display: grid;
		min-height: 42px;
		gap: 3px;
		padding: 8px 9px;
		border: 0;
		border-radius: 5px;
		color: var(--ink);
		background: #0b0d17;
		box-shadow: 0 0 0 1px var(--line);
		font: 11px var(--mono);
		text-align: left;
		cursor: pointer;
	}
	.anchor-list small {
		overflow: hidden;
		color: var(--faint);
		text-overflow: ellipsis;
	}
	.evidence article {
		display: grid;
		gap: 5px;
		padding: 9px;
		border-left: 2px solid var(--blue);
		background: rgb(108 182 255 / 6%);
	}
	.evidence article[data-relation='supports'] {
		border-left-color: var(--green);
	}
	.evidence article[data-relation='contradicts'] {
		border-left-color: var(--pink);
	}
	.evidence article span,
	.evidence article small {
		color: var(--faint);
		font: 9px var(--mono);
		text-transform: uppercase;
	}
	.evidence article p {
		font-size: 12px;
		line-height: 1.45;
	}
	.provenance p {
		display: grid;
		gap: 3px;
		font-size: 11px;
	}
	.retrieved-context > p,
	.analyzer-limitations > p {
		display: grid;
		gap: 3px;
		padding: 8px 9px;
		border-radius: 5px;
		background: #0b0d17;
		box-shadow: 0 0 0 1px var(--line);
		font-size: 11px;
	}
	.retrieved-context code,
	.retrieved-context small,
	.analyzer-limitations span {
		color: var(--faint);
		font-size: 10px;
	}
	.provenance code,
	.provenance small {
		color: var(--faint);
		font-size: 10px;
	}
	.audit-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		max-width: 1600px;
		margin: 18px auto 0;
		gap: 12px;
	}
	.audit-grid > div {
		min-height: 145px;
		padding: 18px;
		border-radius: 10px;
		background: var(--panel);
		box-shadow: var(--shadow-border);
	}
	.audit-grid h3 {
		margin: 9px 0 8px;
		font-size: 16px;
	}
	.audit-grid p:not(.eyebrow) {
		margin-top: 9px;
		color: var(--muted);
		font-size: 12px;
		line-height: 1.6;
	}
	.audit-row {
		display: grid;
		gap: 3px;
	}
	.audit-row strong {
		color: var(--ink);
		text-transform: capitalize;
	}
	.loading-state {
		display: grid;
		max-width: 650px;
		min-height: 70vh;
		align-content: center;
		gap: 18px;
		padding: 40px clamp(24px, 6vw, 80px);
	}
	.loading-state > span {
		color: var(--lavender);
		font: 9px var(--mono);
		letter-spacing: 0.13em;
	}
	.loading-state h2 {
		font-size: clamp(28px, 5vw, 52px);
		font-weight: 430;
		letter-spacing: -0.05em;
	}
	.loading-state p {
		color: var(--muted);
		font-size: 12px;
	}
	.loading-line {
		width: 120px;
		height: 1px;
		background: linear-gradient(90deg, var(--lavender), transparent);
	}
	.button {
		min-height: 42px;
		width: fit-content;
		padding: 0 14px;
		border: 1px solid var(--line-bright);
		border-radius: 5px;
		color: var(--ink);
		background: var(--panel);
		cursor: pointer;
	}
	@media (max-width: 1600px) {
		.orientation {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.orientation__primary {
			grid-column: 1 / -1;
		}
		.work-area {
			grid-template-areas:
				'navigator surface'
				'ledger ledger';
			grid-template-columns: 250px minmax(0, 1fr);
		}
		.work-area--navigator-collapsed {
			grid-template-areas:
				'surface'
				'ledger';
			grid-template-columns: minmax(0, 1fr);
		}
		.work-area--ledger-collapsed {
			grid-template-areas: 'navigator surface';
			grid-template-columns: 250px minmax(0, 1fr);
		}
		.work-area--navigator-collapsed.work-area--ledger-collapsed {
			grid-template-areas: 'surface';
			grid-template-columns: minmax(0, 1fr);
		}
		.audit-grid {
			grid-template-columns: repeat(2, 1fr);
		}
	}
	@media (max-width: 800px) {
		.review-shell {
			padding: 28px 16px 55px;
		}
		.review-header {
			display: grid;
		}
		.round-picker {
			width: 100%;
		}
		.orientation {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.work-area {
			grid-template-areas:
				'navigator'
				'surface'
				'ledger';
			grid-template-columns: 1fr;
		}
		.work-area--navigator-collapsed {
			grid-template-areas:
				'surface'
				'ledger';
		}
		.work-area--ledger-collapsed {
			grid-template-areas:
				'navigator'
				'surface';
		}
		.work-area--navigator-collapsed.work-area--ledger-collapsed {
			grid-template-areas: 'surface';
		}
		.nav-list {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			max-height: 210px;
			overflow: auto;
		}
		.review-surface {
			padding: 16px;
		}
		.surface-toolbar {
			align-items: stretch;
			flex-direction: column;
		}
		.surface-toolbar__actions {
			justify-content: space-between;
		}
		.finding-detail {
			padding: 16px 0 0;
		}
		.audit-grid {
			grid-template-columns: 1fr;
		}
	}
	@media (max-width: 520px) {
		.orientation {
			grid-template-columns: 1fr;
		}
		.orientation__primary {
			grid-column: auto;
		}
		.orientation > div {
			min-height: 82px;
		}
		.nav-list {
			grid-template-columns: 1fr;
		}
	}
</style>
