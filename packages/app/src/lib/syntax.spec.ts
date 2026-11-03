import { getSharedHighlighter } from '@pierre/diffs';
import { describe, expect, it } from 'vitest';
import { SYNTAX_THEMES } from './syntax';

/** Highlights one line of Python with the light theme and returns its tokens. */
async function pythonTokens(code: string) {
	const highlighter = await getSharedHighlighter({ themes: [SYNTAX_THEMES.light], langs: ['python'] });
	return highlighter.codeToTokens(code, { lang: 'python', theme: SYNTAX_THEMES.light }).tokens[0];
}

describe('syntax themes', () => {
	it("colors Python's word operators as bold keywords, as the terminal does", async () => {
		const line = await pythonTokens('x = a and not b or c in d is e');
		const style = (word: string) => {
			const token = line.find((candidate) => candidate.content.trim() === word);
			return token && { color: token.color, bold: token.fontStyle !== 0 };
		};
		for (const word of ['and', 'not', 'or', 'in', 'is']) {
			expect(style(word), word).toEqual({ color: 'var(--syntax-keyword)', bold: true });
		}
		const assignment = line.find((token) => token.content.includes('='));
		expect(assignment?.color).toBe('var(--ink)');
	});
});
