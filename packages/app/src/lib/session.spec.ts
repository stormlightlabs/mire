import { describe, expect, it } from 'vitest';

import { parseSessionSecret } from './session';

describe('parseSessionSecret', () => {
	it('accepts a bare session secret', () => {
		expect(parseSessionSecret('  abc123  ')).toBe('abc123');
	});

	it('extracts the secret from a Mire review URL', () => {
		expect(parseSessionSecret('http://127.0.0.1:3737/#abc123')).toBe('abc123');
	});

	it('rejects a URL without a session secret', () => {
		expect(parseSessionSecret('http://127.0.0.1:3737/')).toBe('');
	});
});
