<script lang="ts">
	import { asset, resolve } from '$app/paths';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import { getDocs } from '$lib/content';

	const docs = getDocs();
	const entryPoints = [
		{
			label: 'Get started',
			title: 'Install Mire',
			href: '/docs/getting-started/installation/'
		},
		{
			label: 'Review code',
			title: 'Review a local changeset',
			href: '/docs/guides/review-changes/'
		},
		{
			label: 'Use an agent',
			title: 'Add agent feedback to a review',
			href: '/docs/guides/review-files/'
		}
	] as const;
</script>

<svelte:head>
	<title>Mire</title>
	<meta
		name="description"
		content="Documentation for Mire, the terminal difftool for humans and agents."
	/>
</svelte:head>

<SiteHeader {docs} />

<main id="main-content" class="landing">
	<section class="landing-hero" aria-labelledby="landing-title">
		<p class="eyebrow">LOCAL CODE REVIEW</p>
		<h1 id="landing-title">Review code together.</h1>

		<p class="landing-lede">Mire is a local code review environment for humans and agents.</p>
		<figure class="terminal" aria-labelledby="terminal-caption">
			<figcaption id="terminal-caption">
				<span class="terminal-dots" aria-hidden="true"><i></i><i></i><i></i></span>
				<span>mire · your terminal emulator</span>
			</figcaption>
			<img
				src={asset('/screencap.png')}
				width="1600"
				height="900"
				alt="Mire showing a split review with a file sidebar, syntax-highlighted Rust changes, and a review note editor."
			/>
		</figure>
		<p class="landing-lede">
			Inspect changes, leave anchored feedback, and keep the review intact as the code evolves.
		</p>
		<div class="landing-actions">
			<a class="button-link" href={resolve('/docs/getting-started/installation/')}>Get started</a>
			<a class="text-link" href={resolve('/docs/concepts/review-model/')}>
				Learn more <span class="i-ri-arrow-right-line" aria-hidden="true"></span>
			</a>
		</div>
	</section>

	<section class="landing-thesis" aria-labelledby="loop-title">
		<h2 id="loop-title" class="eyebrow">Built for the review loop</h2>
		<ol class="review-loop">
			<li>
				<h3>Review the changeset</h3>
				<p>See every changed file and hunk in one continuous review.</p>
			</li>
			<li>
				<h3>Share the review</h3>
				<p>Humans, agents, and tools attach feedback to the same anchored notes.</p>
			</li>
			<li>
				<h3>Keep it current</h3>
				<p>
					As code changes, Mire preserves review state instead of throwing the conversation away.
				</p>
			</li>
		</ol>
	</section>

	<section class="landing-docs" aria-labelledby="docs-title">
		<div class="section-heading">
			<p class="eyebrow">Documentation</p>
			<h2 id="docs-title">Workflows</h2>
		</div>

		<nav class="landing-index" aria-label="Documentation entry points">
			{#each entryPoints as entry, index (entry.href)}
				<a href={resolve(entry.href)}>
					<span class="doc-number">{String(index + 1).padStart(2, '0')}</span>
					<span class="doc-label"><small>{entry.label}</small><strong>{entry.title}</strong></span>
					<span class="doc-arrow i-ri-arrow-right-line" aria-hidden="true"></span>
				</a>
			{/each}
		</nav>
	</section>
</main>

<style>
	.landing {
		max-width: 52rem;
		margin: 0 auto;
		padding: clamp(2.5rem, 5vw, 4.5rem) 2rem 6rem;
	}

	.landing-hero {
		padding-bottom: clamp(3.5rem, 6vw, 5rem);
	}

	.landing-hero h1 {
		margin: 0;
		font-size: clamp(3.25rem, 8vw, 5.7rem);
		line-height: 0.96;
		letter-spacing: -0.025em;
	}

	.landing-lede {
		max-width: 48rem;
		margin: 2rem 0 1rem;
		color: var(--muted);
		font-size: clamp(1.1rem, 2vw, 1.28rem);
	}

	.landing-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
	}

	.text-link {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		min-height: 2.75rem;
		padding: 0.55rem 0.5rem;
		color: var(--navy-dark);
		font-weight: 650;
		text-decoration: none;
	}

	.text-link:hover {
		color: var(--teal);
	}

	.terminal {
		margin: clamp(2.25rem, 4vw, 3rem) 0 0;
		overflow: hidden;
		border: 1px solid var(--code-line);
		border-radius: 0.65rem;
		background: var(--code-surface);
		box-shadow: 0 24px 60px rgb(16 43 62 / 18%);
		color: var(--code-ink);
	}

	.terminal figcaption {
		display: grid;
		grid-template-columns: 4rem 1fr 4rem;
		align-items: center;
		min-height: 2.75rem;
		margin: 0;
		padding: 0 0.9rem;
		border-bottom: 1px solid var(--code-line);
		color: color-mix(in srgb, var(--code-ink) 62%, transparent);
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.72rem;
		text-align: center;
	}

	.terminal-dots {
		display: flex;
		gap: 0.35rem;
	}

	.terminal-dots i {
		width: 0.55rem;
		height: 0.55rem;
		border-radius: 50%;
		background: var(--teal);
	}

	.terminal-dots i:nth-child(2) {
		background: var(--mark-ink);
	}

	.terminal-dots i:nth-child(3) {
		background: var(--navy);
	}

	.terminal img {
		display: block;
		width: 100%;
		height: auto;
	}

	.landing-thesis {
		--review-loop-surface: color-mix(in srgb, var(--paper) 70%, var(--surface-raised));
		margin-inline: calc(50% - 50vw);
		padding: clamp(3.5rem, 6vw, 5rem) 0;
		border-block: 1px solid var(--line);
		background: var(--review-loop-surface);
	}

	.landing-thesis > .eyebrow,
	.review-loop {
		width: min(72rem, calc(100% - 4rem));
		margin-right: auto;
		margin-left: auto;
	}

	.review-loop {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		column-gap: clamp(1.5rem, 3vw, 2.5rem);
		margin-top: 2.5rem;
		margin-bottom: 0;
		padding: 0;
		counter-reset: review-step;
		list-style: none;
	}

	.review-loop li {
		position: relative;
		padding: 2.25rem 0 0;
		border-top: 2px solid var(--line);
		counter-increment: review-step;
	}

	.review-loop li::before {
		position: absolute;
		top: -0.75rem;
		left: 0;
		padding-right: 0.75rem;
		background: var(--review-loop-surface);
		color: var(--teal);
		content: '0' counter(review-step);
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.72rem;
		font-weight: 650;
	}

	.review-loop h3,
	.review-loop p {
		margin: 0;
	}

	.review-loop h3 {
		font-size: clamp(1.25rem, 2vw, 1.55rem);
		line-height: 1.2;
	}

	.review-loop p {
		max-width: 30ch;
		margin-top: 0.75rem;
		color: var(--muted);
	}

	.landing-docs {
		padding-top: 3rem;
	}

	.section-heading h2 {
		max-width: 32ch;
		margin: 0 0 2.5rem;
		font-size: clamp(2rem, 5vw, 3rem);
		line-height: 1.08;
	}

	.landing-index {
		border-top: 1px solid var(--line);
	}

	.landing-index a {
		display: grid;
		grid-template-columns: 2.5rem 1fr auto;
		gap: 1rem;
		align-items: center;
		padding: 1.15rem 0;
		border-bottom: 1px solid var(--line);
		color: var(--ink);
		text-decoration: none;
	}

	.landing-index a:hover {
		color: var(--navy-dark);
	}

	.doc-number,
	.doc-arrow {
		color: var(--teal);
		font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		font-size: 0.72rem;
	}

	.doc-label {
		display: grid;
	}

	.doc-label small {
		color: var(--muted);
		font-size: 0.72rem;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}

	.doc-label strong {
		font-family: 'Google Sans Variable', 'Google Sans', sans-serif;
		font-size: 1.15rem;
	}

	@media (max-width: 600px) {
		.landing {
			padding: 2.5rem 1.25rem 4rem;
		}

		.landing-thesis > .eyebrow,
		.review-loop {
			width: calc(100% - 2.5rem);
		}

		.review-loop {
			grid-template-columns: 1fr;
			gap: 2.75rem;
		}

		.landing-hero h1 {
			font-size: clamp(2.8rem, 14vw, 4rem);
		}

		.terminal figcaption {
			grid-template-columns: 3.5rem 1fr 3.5rem;
		}

		.landing-index a {
			grid-template-columns: 2rem 1fr auto;
			gap: 0.65rem;
		}
	}
</style>
