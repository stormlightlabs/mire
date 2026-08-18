<script lang="ts">
	import { FileDiff, type DiffLineAnnotation, type FileDiffMetadata, type Hunk } from '@pierre/diffs';
	import { onMount } from 'svelte';
	import type { FileDetail, FindingSummary, SemanticHunk } from './review';

	let { file, onFindingClick }: { file: FileDetail; onFindingClick: (finding: FindingSummary) => void } = $props();

	let host = $state<HTMLElement>();
	let viewer: FileDiff<FindingSummary> | undefined;

	onMount(() => {
		viewer = new FileDiff<FindingSummary>({
			theme: { light: 'github-light', dark: 'github-dark' },
			themeType: 'light',
			diffStyle: 'unified',
			hunkSeparators: 'line-info-basic',
			overflow: 'scroll',
			renderAnnotation(annotation) {
				const finding = annotation.metadata;
				if (!finding) return undefined;
				const button = document.createElement('button');
				button.type = 'button';
				button.dataset.mireFinding = finding.id;
				button.textContent = `${finding.severity} · ${finding.body}`;
				button.setAttribute('aria-label', `Open ${finding.severity} finding: ${finding.body}`);
				button.style.cssText =
					'display:block;width:100%;border:1px solid #b7b7b0;background:#fff;color:#111;padding:.5rem .65rem;text-align:left;font:600 .72rem/1.4 system-ui,sans-serif;cursor:pointer;transition:background-color 100ms ease-out;border-radius:0;';
				button.onclick = () => onFindingClick(finding);
				return button;
			}
		});
		render(file);
		return () => viewer?.cleanUp();
	});

	$effect(() => {
		render(file);
	});

	function render(currentFile: FileDetail) {
		if (!viewer || !host || currentFile.content.kind !== 'text') return;
		viewer.render({
			fileContainer: host,
			fileDiff: toPierreDiff(currentFile),
			lineAnnotations: annotations(currentFile.findings)
		});
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
			prevName: file.oldPath?.display,
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
		min-width: 0;
		border: 1px solid var(--line);
		background: var(--surface);
		--diffs-bg: var(--surface);
		--diffs-light-bg: var(--surface);
		--diffs-light: var(--ink);
		--diffs-font-family: 'Google Sans Code Variable', 'Google Sans Code', monospace;
		--diffs-header-font-family: 'IBM Plex Sans Variable', 'IBM Plex Sans', sans-serif;
		--diffs-font-size: 0.75rem;
		--diffs-line-height: 1.4rem;
	}
</style>
