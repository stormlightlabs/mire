<script lang="ts">
	import { EXPORTS, handoffHeadline, type ExportChoice, type HandoffItem } from './finish';

	let {
		items,
		onAct,
		onDownload,
		onClose
	}: {
		/** What is left before handoff, from `handoffItems`. Empty when nothing is left. */
		items: HandoffItem[];
		/** Closes the panel and takes the reader to where they can act on the item. */
		onAct: (item: HandoffItem) => void;
		/** Downloads an export, and resolves with why it failed, or `null`. */
		onDownload: (choice: ExportChoice) => Promise<string | null>;
		onClose: () => void;
	} = $props();

	let downloadError = $state<string | null>(null);

	async function download(choice: ExportChoice) {
		downloadError = null;
		downloadError = await onDownload(choice);
	}

	function actionLabel(item: HandoffItem): string {
		switch (item.kind) {
			case 'held':
				return `Apply revision ${item.revision}`;
			case 'unviewed':
				return 'Next unviewed';
			case 'findings':
				return 'Next open finding';
			case 'anchors':
				return 'Review anchors';
		}
	}
</script>

<section class="finish" aria-describedby="finish-review-summary">
	<h1 id="finish-review-heading">Finish review</h1>
	<p id="finish-review-summary" class="summary">
		{handoffHeadline(items.length)} You can export now; nothing here approves or merges anything.
	</p>

	<ul class="left" aria-label="Left before handoff">
		{#each items as item (item.kind)}
			<li class="item" data-kind={item.kind}>
				{#if item.kind === 'held'}
					<span class="mark i-lucide-refresh-cw" aria-hidden="true"></span>
				{:else if item.kind === 'unviewed'}
					<span class="mark i-lucide-circle" aria-hidden="true"></span>
				{:else if item.kind === 'findings'}
					<span class="mark i-lucide-circle-alert" aria-hidden="true"></span>
				{:else}
					<span class="mark i-lucide-unlink" aria-hidden="true"></span>
				{/if}
				<div class="text">
					<strong>{item.title}</strong>
					{#if item.detail}<span class="detail">{item.detail}</span>{/if}
				</div>
				<button class="act" onclick={() => onAct(item)}>{actionLabel(item)}</button>
			</li>
		{:else}
			<li class="item" data-kind="none">
				<span class="mark i-lucide-check" aria-hidden="true"></span>
				<div class="text">
					<strong>Every file viewed, every finding settled</strong>
					<span class="detail">No open findings, and every finding still matches its line.</span>
				</div>
			</li>
		{/each}
	</ul>

	<h2 class="export-heading" id="finish-export-heading">Export</h2>
	<ul class="exports" aria-labelledby="finish-export-heading">
		{#each EXPORTS as choice (choice.route)}
			<li>
				<button class="export" onclick={() => void download(choice)}>
					<strong>{choice.label}</strong>
					<span class="detail">{choice.filename} · {choice.purpose}</span>
				</button>
			</li>
		{/each}
	</ul>
	<p class="download-error" role="status">{downloadError ?? ''}</p>

	<div class="footer">
		<button class="keep" onclick={onClose}>Keep reviewing</button>
	</div>
</section>

<style>
	.finish h1 {
		margin: 0;
		font:
			600 1.4rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.summary {
		margin: 0.5rem 0 1.25rem;
		color: var(--ink);
		font-size: 0.9rem;
		line-height: 1.55;
	}
	.left,
	.exports {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.left {
		border: 1px solid var(--line);
		border-radius: 0.3rem;
	}
	.item {
		display: grid;
		grid-template-columns: 1rem minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.85rem;
		padding: 0.8rem 1rem;
	}
	.item + .item {
		border-top: 1px solid var(--line);
	}
	.mark {
		width: 0.9rem;
		height: 0.9rem;
		color: var(--muted);
	}
	[data-kind='findings'] .mark {
		color: var(--danger);
	}
	[data-kind='anchors'] .mark {
		color: var(--severity-medium);
	}
	[data-kind='none'] .mark {
		color: var(--success);
	}
	.text {
		min-width: 0;
		display: grid;
		gap: 0.15rem;
	}
	.text strong,
	.export strong {
		font-size: 0.9rem;
		font-weight: 600;
	}
	.detail {
		color: var(--muted);
		font-size: 0.76rem;
		line-height: 1.4;
	}
	.act {
		border: 0;
		padding: 0.2rem 0;
		background: none;
		color: var(--focus);
		font-size: 0.8rem;
		text-decoration: underline;
		text-underline-offset: 0.2em;
		white-space: nowrap;
	}
	.export-heading {
		margin: 1.5rem 0 0.6rem;
		color: var(--muted);
		font:
			600 0.72rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}
	.exports {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
		gap: 0.55rem;
	}
	.export {
		width: 100%;
		height: 100%;
		display: grid;
		align-content: start;
		gap: 0.15rem;
		border: 1px solid var(--line);
		border-radius: 0.3rem;
		padding: 0.65rem 0.8rem;
		background: var(--surface);
		color: var(--ink);
		text-align: left;
	}
	.download-error {
		min-height: 1.2rem;
		margin: 0.5rem 0 0;
		color: var(--danger);
		font-size: 0.78rem;
	}
	.footer {
		display: flex;
		justify-content: flex-end;
		margin-top: 0.5rem;
	}
	.keep {
		min-height: 2.25rem;
		border: 1px solid var(--line-strong);
		border-radius: 0.3rem;
		padding: 0.35rem 0.9rem;
		background: var(--surface);
		color: var(--ink);
		font-size: 0.8rem;
	}
	@media (hover: hover) {
		.export:hover,
		.keep:hover {
			border-color: var(--ink);
			background: var(--button-hover);
		}
		.act:hover {
			text-decoration-thickness: 2px;
		}
	}
	@media (pointer: coarse) {
		.act,
		.keep {
			min-height: 2.75rem;
		}
	}
</style>
