<script lang="ts">
	import { defaultKeymap } from '@codemirror/commands';
	import {
		bracketMatching,
		foldGutter,
		foldKeymap,
		HighlightStyle,
		LanguageDescription,
		syntaxHighlighting
	} from '@codemirror/language';
	import { languages } from '@codemirror/language-data';
	import { MergeView, unifiedMergeView } from '@codemirror/merge';
	import { EditorState, type Extension } from '@codemirror/state';
	import {
		Decoration,
		drawSelection,
		EditorView,
		GutterMarker,
		gutter,
		highlightActiveLine,
		highlightActiveLineGutter,
		highlightSpecialChars,
		keymap,
		lineNumbers
	} from '@codemirror/view';
	import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search';
	import { tags } from '@lezer/highlight';
	import { onMount } from 'svelte';
	import { hunkDocuments } from '$lib/diff-editor';
	import type { DiffHunk } from '$lib/types';

	type Props = { path: string; hunk: DiffHunk; mode: 'unified' | 'split'; selected?: boolean };

	let { path, hunk, mode, selected = false }: Props = $props();

	let host = $state<HTMLDivElement>();
	let ready = $state(false);

	class AnchorMarker extends GutterMarker {
		active: boolean;

		constructor(active: boolean) {
			super();
			this.active = active;
		}

		eq(other: GutterMarker) {
			return other instanceof AnchorMarker && other.active === this.active;
		}

		toDOM() {
			const marker = document.createElement('span');
			marker.className = this.active ? 'cm-mire-anchor cm-mire-anchor--selected' : 'cm-mire-anchor';
			marker.textContent = '◆';
			marker.setAttribute('aria-hidden', 'true');
			return marker;
		}
	}

	const syntaxTheme = HighlightStyle.define([
		{ tag: tags.keyword, color: '#c4adff' },
		{ tag: [tags.name, tags.deleted, tags.character, tags.propertyName], color: '#f0d5ff' },
		{ tag: [tags.function(tags.variableName), tags.labelName], color: '#8dc9ff' },
		{ tag: [tags.color, tags.constant(tags.name), tags.standard(tags.name)], color: '#f3c780' },
		{ tag: [tags.definition(tags.name), tags.separator], color: '#f2f3f8' },
		{ tag: [tags.typeName, tags.className, tags.number, tags.changed, tags.annotation], color: '#78dce8' },
		{ tag: [tags.operator, tags.operatorKeyword, tags.url, tags.escape, tags.regexp, tags.link], color: '#f491b8' },
		{ tag: [tags.meta, tags.comment], color: '#747893', fontStyle: 'italic' },
		{ tag: tags.string, color: '#a9dc9a' },
		{ tag: tags.invalid, color: '#ff7b9c', textDecoration: 'underline' }
	]);

	const editorTheme = EditorView.theme(
		{
			'&': { backgroundColor: '#0b0d17', color: '#d9dbe7', fontFamily: 'var(--mono)', fontSize: '13px' },
			'&.cm-focused': { outline: '1px solid rgb(164 140 242 / 45%)', outlineOffset: '-1px' },
			'.cm-scroller': { maxHeight: '640px', overflow: 'auto', lineHeight: '1.65' },
			'.cm-content': { minHeight: '28px', padding: '8px 0', caretColor: '#f1a6ff' },
			'.cm-line': { padding: '0 16px 0 10px' },
			'.cm-gutters': { backgroundColor: '#0b0d17', color: '#50536b', border: '0' },
			'.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'rgb(164 140 242 / 7%)' },
			'.cm-changedText, .cm-deletedText': { backgroundImage: 'none !important' },
			'.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
				backgroundColor: 'rgb(108 182 255 / 28%) !important'
			},
			'.cm-searchMatch': { backgroundColor: 'rgb(243 199 128 / 28%)', outline: '1px solid #b88f4e' },
			'.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'rgb(241 166 255 / 35%)' },
			'.cm-panels': { color: '#d9dbe7', backgroundColor: '#121522' },
			'.cm-textfield': { color: '#f2f3f8', backgroundColor: '#090b13', border: '1px solid #343850' },
			'.cm-button': { color: '#f2f3f8', backgroundImage: 'none', backgroundColor: '#25283a', border: '0' },
			'.cm-mire-anchor-gutter': { minWidth: '18px' },
			'.cm-mire-anchor': { color: '#656981', fontSize: '8px' },
			'.cm-mire-anchor--selected': { color: '#f1a6ff', textShadow: '0 0 8px rgb(241 166 255 / 70%)' },
			'.cm-mire-anchor-line': { backgroundColor: 'rgb(164 140 242 / 4%)' },
			'.cm-mire-anchor-line--selected': { backgroundColor: 'rgb(164 140 242 / 9%)' }
		},
		{ dark: true }
	);

	function editorExtensions(startLine: number, language: Extension | undefined, label: string): Extension[] {
		const marker = new AnchorMarker(selected);
		const anchorLine = Decoration.line({
			class: selected ? 'cm-mire-anchor-line cm-mire-anchor-line--selected' : 'cm-mire-anchor-line'
		});

		return [
			EditorState.readOnly.of(true),
			EditorView.contentAttributes.of({ 'aria-label': label, 'aria-readonly': 'true' }),
			lineNumbers({ formatNumber: (line) => String(Math.max(1, startLine) + line - 1) }),
			gutter({
				class: 'cm-mire-anchor-gutter',
				lineMarker: (_view, line) => (line.from === 0 ? marker : null),
				initialSpacer: () => marker
			}),
			EditorView.decorations.of(Decoration.set([anchorLine.range(0)])),
			highlightSpecialChars(),
			drawSelection(),
			highlightActiveLine(),
			highlightActiveLineGutter(),
			foldGutter(),
			bracketMatching(),
			search({ top: true }),
			highlightSelectionMatches(),
			keymap.of([...searchKeymap, ...foldKeymap, ...defaultKeymap]),
			syntaxHighlighting(syntaxTheme),
			editorTheme,
			...(language ? [language] : [])
		];
	}

	onMount(() => {
		let disposed = false;
		let destroy: (() => void) | undefined;

		async function mountEditor() {
			const description = LanguageDescription.matchFilename(languages, path);
			const language = await description?.load().catch(() => undefined);
			if (disposed || !host) return;

			const { before, after } = hunkDocuments(hunk);
			if (mode === 'split' && before.length > 0 && after.length > 0) {
				const merge = new MergeView({
					parent: host,
					a: { doc: before, extensions: editorExtensions(hunk.old_start, language, `Before ${path}`) },
					b: { doc: after, extensions: editorExtensions(hunk.new_start, language, `After ${path}`) },
					highlightChanges: true,
					gutter: true,
					collapseUnchanged: { margin: 3, minSize: 8 }
				});
				destroy = () => merge.destroy();
			} else {
				const editorLabel =
					mode === 'split'
						? `${before.length === 0 ? 'Added' : 'Deleted'} code for ${path}`
						: `Unified code for ${path}`;
				const view = new EditorView({
					parent: host,
					state: EditorState.create({
						doc: after,
						extensions: [
							...editorExtensions(hunk.new_start, language, editorLabel),
							unifiedMergeView({
								original: before,
								highlightChanges: true,
								gutter: true,
								allowInlineDiffs: false,
								mergeControls: false,
								collapseUnchanged: { margin: 3, minSize: 8 }
							})
						]
					})
				});
				destroy = () => view.destroy();
			}
			ready = true;
		}

		void mountEditor();
		return () => {
			disposed = true;
			destroy?.();
		};
	});
</script>

<div
	class="hunk-editor hunk-editor--{mode}"
	class:hunk-editor--ready={ready}
	bind:this={host}
	role="region"
	aria-label={mode === 'split' ? `Side-by-side diff for ${path}` : `Unified diff for ${path}`}
	aria-busy={!ready}>
	{#if !ready}<span class="hunk-editor__loading">Loading language-aware diff…</span>{/if}
</div>

<style>
	.hunk-editor {
		position: relative;
		min-height: 38px;
		background: #0b0d17;
	}
	.hunk-editor--split {
		overflow: hidden;
	}
	.hunk-editor--split :global(.cm-mergeView) {
		width: 100%;
		overflow: hidden;
	}
	.hunk-editor :global(.cm-mergeViewEditors) {
		align-items: stretch;
		width: 100%;
	}
	.hunk-editor :global(.cm-mergeViewEditor) {
		flex: 1 1 50%;
		width: 50%;
		min-width: 0;
	}
	.hunk-editor :global(.cm-mergeViewEditor + .cm-mergeViewEditor) {
		box-shadow: -8px 0 24px rgb(0 0 0 / 14%);
	}
	.hunk-editor :global(.cm-changedLine) {
		background: rgb(80 180 105 / 10%);
	}
	.hunk-editor :global(.cm-deletedChunk) {
		background: rgb(241 108 158 / 10%);
	}
	.hunk-editor :global(.cm-changeGutter) {
		background: #0b0d17;
	}
	.hunk-editor__loading {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--faint);
		font: 11px var(--mono);
	}
	.hunk-editor--ready .hunk-editor__loading {
		display: none;
	}
</style>
