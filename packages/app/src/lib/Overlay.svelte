<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		open,
		labelledBy,
		variant,
		onClose,
		children
	}: {
		open: boolean;
		labelledBy: string;
		variant: 'dialog' | 'drawer';
		onClose: () => void;
		children: Snippet;
	} = $props();

	let element = $state<HTMLDialogElement>();

	$effect(() => {
		if (!element) return;
		if (open && !element.open) element.showModal();
		if (!open && element.open) element.close();
	});

	function closeWhenBackdropClicked(event: MouseEvent) {
		if (event.target === element) element.close();
	}
</script>

<dialog
	bind:this={element}
	class:drawer={variant === 'drawer'}
	class="overlay"
	aria-labelledby={labelledBy}
	onclick={closeWhenBackdropClicked}
	onclose={onClose}>
	{@render children()}
</dialog>

<style>
	.overlay {
		width: min(calc(100% - 2rem), 30rem);
		max-height: min(100% - 2rem, 42rem);
		margin: auto;
		padding: 1.5rem;
		border: 1px solid var(--line-strong);
		background: var(--surface);
		color: var(--ink);
		box-shadow: 0 1rem 3rem var(--overlay-shadow);
	}
	.overlay::backdrop {
		background: var(--backdrop);
	}
	.overlay.drawer {
		width: min(100% - 2rem, 24rem);
		max-width: none;
		height: 100%;
		max-height: none;
		margin: 0 0 0 auto;
		padding: 0;
	}
</style>
