<script lang="ts">
	import { resolve } from '$app/paths';
	import Seo from '$lib/components/Seo.svelte';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import TermShot from '$lib/components/TermShot.svelte';
	import { getDocs } from '$lib/content';
	import { site } from '$lib/site';

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

<Seo title={site.title} description={site.description} pathname="/" />

<SiteHeader {docs} />

<main id="main-content" class="landing">
	<section class="landing-hero" aria-labelledby="landing-title">
		<p class="eyebrow">LOCAL CODE REVIEW</p>
		<h1 id="landing-title">Review code together.</h1>

		<p class="landing-lede">Mire is a local code review environment for humans and agents.</p>
		<div class="hero-shot">
			<TermShot
				expand
				src="/screencap.png"
				command="mire diff HEAD..bf30007 --theme eldritch"
				alt="Mire showing a unified Rust diff with a changed-files sidebar."
				loading="eager" />
		</div>
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
				<div class="review-step-copy">
					<h3>Review the changeset</h3>
					<p>See every changed file and hunk in one continuous review.</p>
				</div>
				<TermShot
					src="/screencap_split.png"
					command="mire diff HEAD..bf30007 --theme catppuccin"
					alt="Mire comparing a Rust file's source and changes side by side." />
			</li>
			<li>
				<div class="review-step-copy">
					<h3>Share the review</h3>
					<p>Humans, agents, and tools attach feedback to the same anchored notes.</p>
					<p>As code changes, Mire preserves review state instead of throwing the conversation away.</p>
				</div>
				<TermShot
					src="/screencap_range.png"
					command="mire review range.json"
					alt="Mire creating a review note on a selected range of Rust code." />
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

	.hero-shot {
		margin-top: clamp(2.25rem, 4vw, 3rem);
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
		gap: clamp(2.75rem, 6vw, 5rem);
		margin-top: 2.5rem;
		margin-bottom: 0;
		padding: 0;
		counter-reset: review-step;
		list-style: none;
	}

	.review-loop li {
		display: flex;
		gap: clamp(1.5rem, 4vw, 3.5rem);
		position: relative;
		justify-content: center;
		align-items: center;
		padding: 2.25rem 0 0 0;
		border-top: 2px solid var(--line);
		counter-increment: review-step;
	}

	.review-loop li:nth-of-type(2) {
		flex-direction: row-reverse;
	}

	.review-loop li:last-child {
		min-height: 10rem;
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
			gap: 2.75rem;
		}

		.review-loop li,
		.review-loop li:last-child {
			grid-template-columns: 1fr;
		}

		.landing-hero h1 {
			font-size: clamp(2.8rem, 14vw, 4rem);
		}

		.landing-index a {
			grid-template-columns: 2rem 1fr auto;
			gap: 0.65rem;
		}
	}
</style>
