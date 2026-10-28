import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const stylesheet = readFileSync(new URL('../themes.css', import.meta.url), 'utf8');

const TEXT = ['ink', 'muted', 'focus', 'danger'];
/** Graphics, such as the ready status dot, need 3:1 rather than 4.5:1. */
const GRAPHICS = ['success'];
const BACKGROUNDS = ['paper', 'surface'];

/** Reads each palette block of the generated stylesheet into its custom properties. */
function palettes(css: string) {
	const blocks = [...css.matchAll(/\/\* ([a-z-]+) \*\/\n[^{]+\{([^}]+)\}/g)];
	return blocks.map(([, name, body]) => ({
		name,
		colors: Object.fromEntries([...body.matchAll(/--([a-z-]+): (#[0-9a-f]{6});/g)].map(([, key, hex]) => [key, hex]))
	}));
}

function luminance(hex: string) {
	const channels = [1, 3, 5].map((start) => {
		const value = parseInt(hex.slice(start, start + 2), 16) / 255;
		return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(foreground: string, background: string) {
	const [lighter, darker] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
	return (lighter + 0.05) / (darker + 0.05);
}

describe('generated themes', () => {
	it('holds the six terminal palettes', () => {
		expect(palettes(stylesheet).map((palette) => palette.name)).toEqual([
			'iceberg-dark',
			'nord-light',
			'eldritch-minimal',
			'eldritch-dusk',
			'catppuccin-mocha',
			'catppuccin-latte'
		]);
	});

	it('gives text, muted text, accent and danger 4.5:1 on every background', () => {
		for (const { name, colors } of palettes(stylesheet)) {
			for (const background of BACKGROUNDS) {
				for (const foreground of TEXT) {
					const ratio = contrast(colors[foreground], colors[background]);
					expect(ratio, `${name}: --${foreground} on --${background}`).toBeGreaterThanOrEqual(4.5);
				}
			}
		}
	});

	it('gives the success color 3:1 on every background', () => {
		for (const { name, colors } of palettes(stylesheet)) {
			for (const background of BACKGROUNDS) {
				for (const foreground of GRAPHICS) {
					const ratio = contrast(colors[foreground], colors[background]);
					expect(ratio, `${name}: --${foreground} on --${background}`).toBeGreaterThanOrEqual(3);
				}
			}
		}
	});
});
