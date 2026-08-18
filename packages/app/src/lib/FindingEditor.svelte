<script lang="ts">
	import type { FindingDetail, FindingDraft } from './review';

	type Decision = 'resolve' | 'reopen' | 'dismiss' | 'accept-risk';

	type Props = {
		finding: FindingDetail;
		onEdit: (draft: FindingDraft) => Promise<string | null>;
		onDecision: (decision: Decision) => Promise<string | null>;
	};

	let { finding, onEdit, onDecision }: Props = $props();

	let body = $derived(finding.body);
	let severity = $derived(finding.severity);
	let annotationKind = $derived(finding.annotationKind);
	let pending = $state(false);
	let message = $derived.by<string | null>(() => {
		void finding.id;
		return null;
	});

	async function save() {
		pending = true;
		message = await onEdit({ body, severity, annotationKind });
		pending = false;
	}

	async function decide(decision: Decision) {
		pending = true;
		message = await onDecision(decision);
		pending = false;
	}
</script>

<form
	class="finding-editor"
	onsubmit={(event) => {
		event.preventDefault();
		void save();
	}}>
	<h2>Edit finding</h2>
	<label>
		<span>Finding</span>
		<textarea bind:value={body} required maxlength="16384" disabled={pending}></textarea>
	</label>
	<div class="editor-fields">
		<label>
			<span>Severity</span>
			<select bind:value={severity} disabled={pending}>
				<option value="note">Note</option>
				<option value="low">Low</option>
				<option value="medium">Medium</option>
				<option value="high">High</option>
				<option value="critical">Critical</option>
			</select>
		</label>
		<label>
			<span>Intent</span>
			<select bind:value={annotationKind} disabled={pending}>
				<option value="comment">Comment</option>
				<option value="defect">Defect</option>
				<option value="suggestion">Suggestion</option>
				<option value="question">Question</option>
			</select>
		</label>
	</div>
	<div class="editor-actions">
		<button type="submit" disabled={pending}>{pending ? 'Saving…' : 'Save edit'}</button>
		{#if finding.status === 'open'}
			<button type="button" disabled={pending} onclick={() => void decide('resolve')}>Resolve</button>
			<button type="button" disabled={pending} onclick={() => void decide('dismiss')}>Dismiss</button>
			<button type="button" disabled={pending} onclick={() => void decide('accept-risk')}>Accept risk</button>
		{:else}
			<button type="button" disabled={pending} onclick={() => void decide('reopen')}>Reopen</button>
		{/if}
	</div>
	{#if message}<p class="editor-message" role="alert">{message}</p>{/if}
</form>

<style>
	.finding-editor {
		display: grid;
		gap: 0.65rem;
		margin-bottom: 1rem;
		padding: 0.8rem;
		border: 1px solid var(--line-strong);
		background: var(--surface);
	}
	.finding-editor h2 {
		margin: 0;
		font:
			600 0.875rem 'Google Sans Variable',
			'Google Sans',
			sans-serif;
	}
	.finding-editor label {
		display: grid;
		gap: 0.3rem;
		color: var(--muted);
		font-size: 0.72rem;
		font-weight: 600;
	}
	.finding-editor textarea,
	.finding-editor select {
		width: 100%;
		border: 1px solid var(--line-strong);
		border-radius: 0.2rem;
		padding: 0.45rem 0.55rem;
		background: var(--paper);
		color: var(--ink);
		font: inherit;
	}
	.finding-editor textarea {
		min-height: 5rem;
		resize: vertical;
	}
	.editor-fields,
	.editor-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.55rem;
	}
	.editor-fields > label {
		flex: 1 1 9rem;
	}
	.editor-actions button {
		min-height: 2.25rem;
		border: 1px solid var(--ink);
		border-radius: 0.2rem;
		padding: 0.35rem 0.55rem;
		background: var(--surface);
		cursor: pointer;
	}
	.editor-actions button[type='submit'] {
		background: var(--ink);
		color: var(--surface);
	}
	.editor-actions button:disabled {
		cursor: wait;
		opacity: 0.6;
	}
	.editor-message {
		margin: 0;
		color: #a4332f;
		font-size: 0.8rem;
	}
</style>
