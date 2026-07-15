<script lang="ts">
	import '$lib/styles/app.css';
	import favicon from '$lib/assets/favicon.svg';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<meta name="theme-color" content="#090a12" />
	<meta name="description" content="Mire — a private, evidence-led review workbench." />
</svelte:head>

{@render children()}
