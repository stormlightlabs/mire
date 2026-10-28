<script lang="ts">
	import 'virtual:uno.css';
	import '../themes.css';
	import '../app.css';

	let { children } = $props();
</script>

<svelte:head>
	<title>Mire review</title>
	<meta name="theme-color" content="#e5e9f0" />
</svelte:head>

{@render children()}
