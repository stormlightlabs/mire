<script lang="ts">
	import { EditorState } from '@codemirror/state';
	import { EditorView, lineNumbers } from '@codemirror/view';
	import { onMount } from 'svelte';

	let { value = '// Not yet implemented.' }: { value?: string } = $props();
	let host = $state<HTMLDivElement>();

	onMount(() => {
		if (!host) return;
		const view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: value,
				extensions: [
					lineNumbers(),
					EditorState.readOnly.of(true),
					EditorView.editable.of(false),
					EditorView.theme({
						'&': { backgroundColor: '#0b0d17', color: '#8c8fac', fontSize: '11px' },
						'.cm-gutters': { backgroundColor: '#0b0d17', color: '#4f5270', border: '0' },
						'.cm-content': { padding: '14px 0' },
						'.cm-line': { padding: '0 14px' }
					})
				]
			})
		});
		return () => view.destroy();
	});
</script>

<div class="diff-preview" bind:this={host} aria-label="Snapshot diff preview"></div>

<style>
	.diff-preview {
		flex: 0 0 min(48%, 380px);
		min-height: 132px;
		overflow: hidden;
		border: 1px solid var(--line);
		border-radius: 5px;
		box-shadow: inset 0 0 0 1px rgb(0 0 0 / 16%);
	}
</style>
