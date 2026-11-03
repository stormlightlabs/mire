<script lang="ts">
	import { FileDiff, type DiffLineAnnotation, type FileDiffMetadata, type Hunk } from '@pierre/diffs';
	import { onMount } from 'svelte';
	import { isWholeFileChange, type FileDetail, type FindingSummary, type SemanticHunk } from './review';
	import { SYNTAX_THEMES } from './syntax';
	import type { Theme } from './theme';

	let {
		file,
		diffStyle,
		expandContext,
		theme,
		showHeader = true,
		wrap = false,
		onFindingClick
	}: {
		file: FileDetail;
		diffStyle: 'unified' | 'split';
		expandContext: boolean;
		theme: Theme;
		/** Whether to draw the file name and line counts above the diff. Focus mode names the file in its own bar. */
		showHeader?: boolean;
		/** Whether long lines wrap inside the pane instead of scrolling sideways, as on a narrow screen. */
		wrap?: boolean;
		onFindingClick: (finding: FindingSummary) => void;
	} = $props();

	/**
	 * Rows have fractional heights, so each row's gutter bar can leave a
	 * one-pixel seam below it. Running a bar one pixel into the next row draws
	 * a run of changed rows as one continuous bar; the next row's own bar paints
	 * over the overlap. The last row of a run stops at its own edge, so the bar
	 * does not reach into the unchanged row below.
	 */
	const GUTTER_CSS = `[data-indicators='bars'] [data-column-number]:has(+ [data-column-number][data-line-type^='change-'])::before { height: calc(100% + 1px); }`;

	/**
	 * Changed rows take `--diff-added` and `--diff-removed` as they are, rather
	 * than a blend of them with the plain row, so the syntax colors keep the
	 * contrast `themes.css` was checked for. The library picks a token's weight
	 * with `light-dark()`, which takes colors only, so the bold that
	 * `syntax.ts` gives keywords, types and functions is read directly; both
	 * themes give the same weight.
	 */
	const SYNTAX_CSS = `[data-line], [data-no-newline] { --mix-light: 0%; --mix-dark: 0%; }
[data-line] span { font-weight: var(--diffs-token-light-font-weight, inherit); }`;

	let host = $state<HTMLElement>();
	let viewer: FileDiff<FindingSummary> | undefined;
	/** Where each hunk's rows begin and how many there are, by unified line index, as last rendered. */
	let hunkSpans: [number, number][] = [];

	/**
	 * Returns the element that starts each hunk on screen, in order: the
	 * separator above the hunk where there is one, or else its first row.
	 *
	 * Every row's `data-line-index` reads `unified,split` in either layout, so
	 * the unified index places a row in its hunk whichever layout is shown. A
	 * hunk's lowest unified index is on its first row in both layouts.
	 */
	export function hunkStarts(): HTMLElement[] {
		const root = host?.shadowRoot;
		if (!root) return [];
		const firstRows: { index: number; row: HTMLElement }[] = [];
		for (const row of root.querySelectorAll<HTMLElement>('[data-line][data-line-index]')) {
			const index = Number(row.dataset.lineIndex?.split(',')[0]);
			const hunk = hunkContaining(index);
			if (hunk < 0) continue;
			const first = firstRows[hunk];
			if (!first || index < first.index) firstRows[hunk] = { index, row };
		}
		return firstRows.flatMap((first) => {
			if (!first) return [];
			const above = first.row.previousElementSibling;
			return [above instanceof HTMLElement && above.hasAttribute('data-separator') ? above : first.row];
		});
	}

	onMount(() => {
		viewer = new FileDiff<FindingSummary>(viewerOptions());
		render(file);
		return () => viewer?.cleanUp();
	});

	$effect(() => {
		if (!viewer) return;
		viewer.setOptions(viewerOptions());
		render(file);
	});

	function viewerOptions() {
		return {
			theme: SYNTAX_THEMES,
			themeType: theme,
			diffStyle,
			disableFileHeader: !showHeader,
			unsafeCSS: `${GUTTER_CSS}\n${SYNTAX_CSS}`,
			// A whole-file addition or deletion reads as plain code, with only the gutter bar marking it.
			disableBackground: isWholeFileChange(file.status),
			hunkSeparators: 'line-info' as const,
			overflow: wrap ? ('wrap' as const) : ('scroll' as const),
			expandUnchanged: expandContext,
			collapsedContextThreshold: 3,
			renderAnnotation,
			onPostRender(node: HTMLElement) {
				makeExpansionControlsKeyboardAccessible(node);
			}
		};
	}

	/**
	 * Draws a finding as an annotation between code rows: a severity rule and
	 * mark, its kind and status, then the body in prose type.
	 */
	function renderAnnotation(annotation: DiffLineAnnotation<FindingSummary>) {
		const finding = annotation.metadata;
		if (!finding) return undefined;
		const button = document.createElement('button');
		button.type = 'button';
		button.className = 'finding-line';
		button.dataset.mireFinding = finding.id;
		button.dataset.severity = finding.severity;
		button.setAttribute(
			'aria-label',
			`Open ${finding.severity} ${finding.annotationKind}, ${finding.status}: ${finding.body}`
		);
		const head = document.createElement('span');
		head.className = 'finding-head';
		const mark = document.createElement('span');
		mark.className = 'severity-mark';
		mark.dataset.severity = finding.severity;
		mark.textContent = finding.severity;
		const meta = document.createElement('span');
		meta.className = 'finding-meta';
		meta.textContent = `${finding.annotationKind} · ${finding.status}`;
		head.append(mark, meta);
		const body = document.createElement('span');
		body.className = 'finding-body';
		body.textContent = finding.body;
		button.append(head, body);
		button.onclick = () => onFindingClick(finding);
		return button;
	}

	function makeExpansionControlsKeyboardAccessible(node: HTMLElement) {
		const root = node.shadowRoot ?? node;
		for (const control of root.querySelectorAll<HTMLElement>('[data-expand-button]')) {
			control.tabIndex = 0;
			control.setAttribute(
				'aria-label',
				control.hasAttribute('data-expand-all-button')
					? 'Expand all unchanged lines'
					: control.hasAttribute('data-expand-both')
						? 'Expand unchanged lines'
						: control.hasAttribute('data-expand-up')
							? 'Expand unchanged lines above'
							: 'Expand unchanged lines below'
			);
			control.onkeydown = (event) => {
				if (event.key !== 'Enter' && event.key !== ' ') return;
				event.preventDefault();
				control.click();
			};
		}
	}

	/** Returns the position in `hunkSpans` of the hunk holding unified line index `index`, or -1. */
	function hunkContaining(index: number): number {
		let low = 0;
		let high = hunkSpans.length - 1;
		while (low <= high) {
			const middle = (low + high) >> 1;
			const [start, count] = hunkSpans[middle];
			if (index < start) high = middle - 1;
			else if (index >= start + count) low = middle + 1;
			else return middle;
		}
		return -1;
	}

	function annotations(findings: FindingSummary[]): DiffLineAnnotation<FindingSummary>[] {
		return findings
			.filter((finding) => finding.navigable)
			.map((finding) => ({
				side: finding.side === 'old' ? 'deletions' : 'additions',
				lineNumber: finding.startLine,
				metadata: finding
			}));
	}

	function render(currentFile: FileDetail) {
		if (!viewer || !host || currentFile.content.kind !== 'text') return;
		const fileDiff = toPierreDiff(currentFile);
		hunkSpans = fileDiff.hunks.map((hunk) => [hunk.unifiedLineStart, hunk.unifiedLineCount]);
		viewer.render({
			fileContainer: host,
			fileDiff,
			lineAnnotations: annotations(currentFile.findings)
		});
	}

	function toPierreDiff(file: FileDetail): FileDiffMetadata {
		if (file.content.kind !== 'text') throw new Error('A binary file cannot be rendered as a text diff.');
		let additionIndex = 0;
		let deletionIndex = 0;
		let splitIndex = 0;
		let unifiedIndex = 0;
		let previousNewEnd = 0;
		const additionLines: string[] = [];
		const deletionLines: string[] = [];
		const hunks = file.content.hunks.map((hunk) => {
			const collapsedBefore = Math.max(hunk.newStart - (hunk.newLineCount === 0 ? 0 : 1) - previousNewEnd, 0);
			const result = toPierreHunk(
				hunk,
				additionIndex,
				deletionIndex,
				splitIndex + collapsedBefore,
				unifiedIndex + collapsedBefore,
				collapsedBefore
			);
			additionIndex += result.additions.length;
			deletionIndex += result.deletions.length;
			splitIndex += collapsedBefore + result.hunk.splitLineCount;
			unifiedIndex += collapsedBefore + result.hunk.unifiedLineCount;
			previousNewEnd = hunk.newStart - (hunk.newLineCount === 0 ? 0 : 1) + hunk.newLineCount;
			additionLines.push(...result.additions);
			deletionLines.push(...result.deletions);
			return result.hunk;
		});
		return {
			name: file.path.display,
			// The header draws an arrow from the old path whenever one is given, so only a rename passes it.
			prevName: file.oldPath && file.oldPath.display !== file.path.display ? file.oldPath.display : undefined,
			type: pierreChangeType(file.status),
			hunks,
			splitLineCount: splitIndex,
			unifiedLineCount: unifiedIndex,
			isPartial: true,
			additionLines,
			deletionLines,
			cacheKey: file.id
		};
	}

	function toPierreHunk(
		hunk: SemanticHunk,
		additionLineIndex: number,
		deletionLineIndex: number,
		splitLineStart: number,
		unifiedLineStart: number,
		collapsedBefore: number
	): { hunk: Hunk; additions: string[]; deletions: string[] } {
		const additions: string[] = [];
		const deletions: string[] = [];
		const hunkContent: Hunk['hunkContent'] = [];
		let index = 0;
		let addedLineCount = 0;
		let deletedLineCount = 0;
		let splitLineCount = 0;
		let unifiedLineCount = 0;
		while (index < hunk.lines.length) {
			const line = hunk.lines[index];
			if (line.kind === 'context') {
				let count = 0;
				while (hunk.lines[index]?.kind === 'context') {
					const context = hunk.lines[index++];
					additions.push(withNewline(context.content.display, context.missingNewline, 'additions'));
					deletions.push(withNewline(context.content.display, context.missingNewline, 'deletions'));
					count += 1;
				}
				hunkContent.push({
					type: 'context',
					lines: count,
					additionLineIndex: additionLineIndex + additions.length - count,
					deletionLineIndex: deletionLineIndex + deletions.length - count
				});
				splitLineCount += count;
				unifiedLineCount += count;
				continue;
			}

			const additionStart = additions.length;
			const deletionStart = deletions.length;
			let additionsInChange = 0;
			let deletionsInChange = 0;
			while (index < hunk.lines.length && hunk.lines[index].kind !== 'context') {
				const changed = hunk.lines[index++];
				if (changed.kind === 'addition') {
					additions.push(withNewline(changed.content.display, changed.missingNewline, 'additions'));
					additionsInChange += 1;
					addedLineCount += 1;
				} else {
					deletions.push(withNewline(changed.content.display, changed.missingNewline, 'deletions'));
					deletionsInChange += 1;
					deletedLineCount += 1;
				}
			}
			hunkContent.push({
				type: 'change',
				additions: additionsInChange,
				deletions: deletionsInChange,
				additionLineIndex: additionLineIndex + additionStart,
				deletionLineIndex: deletionLineIndex + deletionStart
			});
			splitLineCount += Math.max(additionsInChange, deletionsInChange);
			unifiedLineCount += additionsInChange + deletionsInChange;
		}

		return {
			hunk: {
				collapsedBefore,
				additionStart: hunk.newStart,
				additionCount: hunk.newLineCount,
				additionLines: addedLineCount,
				additionLineIndex,
				deletionStart: hunk.oldStart,
				deletionCount: hunk.oldLineCount,
				deletionLines: deletedLineCount,
				deletionLineIndex,
				hunkContent,
				hunkContext: hunk.section.display || undefined,
				hunkSpecs: `@@ -${hunk.oldStart},${hunk.oldLineCount} +${hunk.newStart},${hunk.newLineCount} @@\n`,
				splitLineStart,
				splitLineCount,
				unifiedLineStart,
				unifiedLineCount,
				noEOFCRDeletions: hunk.lines.some((line) => line.missingNewline === 'old' || line.missingNewline === 'both'),
				noEOFCRAdditions: hunk.lines.some((line) => line.missingNewline === 'new' || line.missingNewline === 'both')
			},
			additions,
			deletions
		};
	}

	function withNewline(content: string, missingNewline: string, side: 'additions' | 'deletions') {
		const missing =
			missingNewline === 'both' ||
			(missingNewline === 'new' && side === 'additions') ||
			(missingNewline === 'old' && side === 'deletions');
		return missing ? content : `${content}\n`;
	}

	function pierreChangeType(status: FileDetail['status']): FileDiffMetadata['type'] {
		switch (status) {
			case 'added':
				return 'new';
			case 'deleted':
				return 'deleted';
			case 'renamed':
				return 'rename-changed';
			default:
				return 'change';
		}
	}
</script>

<diffs-container class="diff-host" bind:this={host}></diffs-container>

<style>
	.diff-host {
		/* The code rows scroll on their own layer, which can round out over the right border unless clipped. */
		overflow: clip;
		min-width: 0;
		border: 1px solid var(--line);
		background: var(--surface);
		--diffs-addition-color-override: var(--success);
		--diffs-deletion-color-override: var(--danger);
		--diffs-modified-color-override: var(--focus);
		--diffs-bg-addition-override: var(--diff-added);
		--diffs-bg-deletion-override: var(--diff-removed);
		--diffs-bg-addition-emphasis-override: var(--diff-added-word);
		--diffs-bg-deletion-emphasis-override: var(--diff-removed-word);
		--diffs-font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		--diffs-header-font-family: 'IBM Plex Sans Variable', 'IBM Plex Sans', sans-serif;
		--diffs-font-size: 0.75rem;
		--diffs-line-height: 1.4rem;
	}
	/* Findings sit between code rows, so they take prose type, their own surface and a severity rule. */
	.diff-host :global(.finding-line) {
		display: grid;
		gap: 0.3rem;
		width: calc(100% - 1.5rem);
		margin: 0.4rem 0.75rem;
		border: 1px solid var(--line);
		border-left: 3px solid var(--severity-color, var(--line-strong));
		border-radius: 0.25rem;
		padding: 0.5rem 0.75rem 0.6rem;
		background: var(--annotation);
		color: var(--ink);
		text-align: left;
		font:
			400 0.8125rem / 1.5 'IBM Plex Sans Variable',
			'IBM Plex Sans',
			sans-serif;
		cursor: pointer;
	}
	.diff-host :global(.finding-head) {
		display: flex;
		align-items: baseline;
		gap: 0.6rem;
	}
	.diff-host :global(.finding-meta) {
		color: var(--muted);
		font-size: 0.72rem;
	}
	.diff-host :global(.finding-body) {
		overflow-wrap: anywhere;
	}
	@media (hover: hover) {
		.diff-host :global(.finding-line:hover) {
			border-color: var(--line-strong);
			border-left-color: var(--severity-color, var(--line-strong));
		}
	}
	@media (pointer: coarse) {
		.diff-host :global(.finding-line) {
			min-height: 2.75rem;
		}
	}
</style>
