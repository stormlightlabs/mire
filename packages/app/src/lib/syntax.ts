import { registerCustomTheme, type ThemeRegistration } from '@pierre/diffs';

/**
 * The Shiki themes the diff viewer highlights with, one per color mode.
 *
 * Both name CSS variables instead of colors, so `themes.css` sets every
 * color: `--syntax-*` for code and `--ink` for the rest. The two differ only
 * in their color scheme, which the viewer uses for its scrollbars and form
 * controls. Shiki keeps variables only in token colors and the editor's
 * foreground and background, so `DiffViewer.svelte` sets the change colors.
 */
export const SYNTAX_THEMES = { light: 'mire-light', dark: 'mire-dark' } as const;

/**
 * TextMate scopes for each syntax role, matching the terminal's highlight
 * names in `crates/ui/src/theme.rs`: comments, strings and markup, constants
 * and numbers, keywords, types, and functions. Keywords, types and functions
 * are bold, as in the terminal. Word operators, such as Python's `and` and
 * `in`, are keywords too. Other operators, punctuation and names take `--ink`.
 */
const TOKEN_COLORS: ThemeRegistration['tokenColors'] = [
	{
		scope: ['comment', 'punctuation.definition.comment', 'string.quoted.docstring'],
		settings: { foreground: 'var(--syntax-comment)' }
	},
	{
		scope: ['string', 'punctuation.definition.string', 'markup'],
		settings: { foreground: 'var(--syntax-string)' }
	},
	{
		scope: ['constant', 'variable.other.constant', 'support.constant'],
		settings: { foreground: 'var(--syntax-constant)' }
	},
	{
		scope: [
			'keyword',
			'storage',
			'keyword.operator.new',
			'keyword.operator.expression',
			'keyword.operator.logical',
			'keyword.operator.word'
		],
		settings: { foreground: 'var(--syntax-keyword)', fontStyle: 'bold' }
	},
	{
		scope: ['keyword.operator', 'support.type.property-name'],
		settings: { foreground: 'var(--ink)', fontStyle: '' }
	},
	{
		scope: ['entity.name.type', 'entity.name.class', 'entity.other.inherited-class', 'support.type', 'support.class'],
		settings: { foreground: 'var(--syntax-type)', fontStyle: 'bold' }
	},
	{
		scope: ['entity.name.function', 'support.function'],
		settings: { foreground: 'var(--syntax-function)', fontStyle: 'bold' }
	}
];

for (const type of ['light', 'dark'] as const) {
	const name = SYNTAX_THEMES[type];
	const theme: ThemeRegistration = {
		name,
		type,
		colors: {
			'editor.foreground': 'var(--ink)',
			'editor.background': 'var(--surface)'
		},
		tokenColors: TOKEN_COLORS
	};
	registerCustomTheme(name, () => Promise.resolve(theme));
}
