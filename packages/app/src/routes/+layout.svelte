<script lang="ts">
	import 'virtual:uno.css';
	import '../app.css';

	let { children } = $props();
</script>

<svelte:head>
	<title>Mire review</title>
	<meta name="theme-color" content="#f7f7f5" />
</svelte:head>

{@render children()}
